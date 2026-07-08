use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use walkdir::WalkDir;

const DEFAULT_DOC_ROOT: &str = "docs";

const SKIP_DIRS: &[&str] = &[".git", "target", "node_modules"];

#[derive(Parser)]
#[command(
    name = "kusara",
    version,
    about = "Cross-reference graph tooling for Markdown specs and docs"
)]
struct Cli {
    #[arg(long, value_name = "DIR", global = true)]
    root: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Validate the doc graph (dangling refs, dup IDs, unknown kinds, missing modules).
    Validate,
    /// Print everything that depends on the given IDs (forward impact, transitive).
    Impact {
        ids: Vec<String>,
        #[arg(long, default_value_t = u32::MAX)]
        depth: u32,
        /// Include `related:` edges (soft links). Default: hard edges only.
        #[arg(long)]
        include_related: bool,
    },
    /// Print everything the given IDs depend on (reverse, transitive).
    Deps {
        ids: Vec<String>,
        #[arg(long, default_value_t = u32::MAX)]
        depth: u32,
        #[arg(long)]
        include_related: bool,
    },
    /// Print one doc: front matter + immediate forward and reverse refs.
    Show { id: String },
    /// Given source files, print docs whose `modules:` cover them, plus impact closure.
    Touched {
        files: Vec<PathBuf>,
        #[arg(long)]
        no_closure: bool,
    },
    /// Regenerate index files. Targets:
    ///   `map` — global map.md + ai/graph.json + ai/modules.md
    ///   `all` (default) — every kind whose manifest entry has `index.output`
    Index {
        #[arg(value_enum, default_value_t = IndexTarget::All)]
        target: IndexTarget,
    },
    /// List docs whose `modules:` code changed after the doc's last commit
    /// (git history based; uncommitted docs are treated as fresh).
    Stale,
    /// List every known ID (debug aid).
    List,
    /// Rewrite legacy `refs:` front matter into the OKF-native shape in place.
    Migrate {
        /// Print which files would change without writing them.
        #[arg(long)]
        dry_run: bool,
    },
    /// Claude Code hook adapters. Each reads the hook JSON payload on stdin.
    Hook {
        #[command(subcommand)]
        cmd: HookCmd,
    },
}

#[derive(Subcommand)]
enum HookCmd {
    /// PostToolUse: append the edited file to the session journal. Emits nothing.
    Postedit {
        /// Journal directory (default: <system tmp>/kusara-hook).
        #[arg(long, value_name = "DIR")]
        journal_dir: Option<PathBuf>,
    },
    /// Stop: batch-check the session's journaled edits (validate + touched)
    /// and emit a single context block, then clear the journal.
    Stop {
        /// Journal directory (default: <system tmp>/kusara-hook).
        #[arg(long, value_name = "DIR")]
        journal_dir: Option<PathBuf>,
        /// Extra line appended to any emitted context (e.g. a pointer to repo rules).
        #[arg(long, value_name = "TEXT")]
        note: Option<String>,
    },
}

#[derive(clap::ValueEnum, Debug, PartialEq, Eq, Clone, Copy)]
enum IndexTarget {
    /// Global doc map: `${KUSARA_DOC_ROOT}/map.md` + `ai/graph.json` + `ai/modules.md`.
    Map,
    /// Every kind whose manifest entry has `index.output`.
    All,
}

// ---------------------------------------------------------------------------
// Newtypes
// ---------------------------------------------------------------------------

/// Identifier of a doc node in the cross-reference graph.
#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd, Deserialize, serde::Serialize)]
#[serde(transparent)]
struct DocId(String);

impl DocId {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for DocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for DocId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for DocId {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::borrow::Borrow<str> for DocId {
    fn borrow(&self) -> &str {
        &self.0
    }
}

/// Identifier of a doc kind (the `kind:` field in front matter).
/// The literal `"index"` denotes a generated INDEX doc.
#[derive(Debug, Clone, Eq, Hash, PartialEq, Ord, PartialOrd, Deserialize, serde::Serialize)]
#[serde(transparent)]
struct Kind(String);

impl Kind {
    const INDEX_LITERAL: &'static str = "index";

    #[allow(dead_code)]
    fn as_str(&self) -> &str {
        &self.0
    }

    fn is_index(&self) -> bool {
        self.0 == Self::INDEX_LITERAL
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<String> for Kind {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for Kind {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

impl std::borrow::Borrow<str> for Kind {
    fn borrow(&self) -> &str {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// Kind manifest (loaded from ${KUSARA_DOC_ROOT}/kinds.md)
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ManifestFile {
    kinds: Vec<KindDef>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DeclaredVia {
    /// IDs are declared inside another doc's `provides:` list.
    Provides,
    /// File written by `kusara index`; never hand-edited.
    Generated,
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)] // `id_pattern` and `singleton` are informational manifest fields.
struct KindDef {
    name: Kind,
    #[serde(default)]
    path_globs: Vec<String>,
    /// `provides` for kinds declared inside another doc's `provides:`,
    /// `generated` for kinds written by `kusara index`. Absent for
    /// hand-authored, file-backed kinds.
    #[serde(default)]
    declared_via: Option<DeclaredVia>,
    #[serde(default)]
    id_pattern: Option<String>,
    #[serde(default)]
    singleton: bool,
    #[serde(default)]
    index: Option<KindIndex>,
}

#[derive(Debug, Deserialize, Clone)]
struct KindIndex {
    output: String,
    #[serde(default)]
    group_by: Option<String>,
}

struct Manifest {
    kinds: BTreeMap<Kind, KindDef>,
}

impl Manifest {
    fn load(root: &Path, doc_root: &Path) -> Result<Self> {
        let path = root.join(doc_root).join("kinds.md");
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("read kind manifest at {}", path.display()))?;
        let yaml = extract_fenced_yaml(&raw)
            .ok_or_else(|| anyhow!("{}: no ```yaml ...``` fenced block found", path.display()))?;
        let parsed: ManifestFile = serde_yaml_ng::from_str(yaml)
            .with_context(|| format!("parse kind manifest at {}", path.display()))?;
        let mut kinds = BTreeMap::new();
        for k in parsed.kinds {
            if kinds.contains_key(&k.name) {
                bail!("{}: duplicate kind `{}`", path.display(), k.name);
            }
            if k.path_globs.is_empty() && k.declared_via.is_none() {
                bail!(
                    "{}: kind `{}` has neither `path_globs` nor `declared_via`",
                    path.display(),
                    k.name
                );
            }
            kinds.insert(k.name.clone(), k);
        }
        Ok(Self { kinds })
    }

    fn knows(&self, kind: &Kind) -> bool {
        self.kinds.contains_key(kind)
    }
}

/// Returns the body of the first ```yaml or ```yml fenced block in `content`.
fn extract_fenced_yaml(content: &str) -> Option<&str> {
    let mut byte_start: Option<usize> = None;
    let mut byte_end: Option<usize> = None;
    let mut cursor: usize = 0;
    for line in content.split_inclusive('\n') {
        let trimmed = line.trim_end_matches(['\n', '\r']);
        if byte_start.is_none() {
            if trimmed == "```yaml" || trimmed == "```yml" {
                byte_start = Some(cursor + line.len());
            }
        } else if trimmed == "```" {
            byte_end = Some(cursor);
            break;
        }
        cursor += line.len();
    }
    Some(&content[byte_start?..byte_end?])
}

// ---------------------------------------------------------------------------
// Front matter + doc model
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct FrontMatter {
    // Legacy shape: everything nested under `refs:`.
    #[serde(default)]
    refs: Option<RefsBlock>,
    // OKF shape: `type` is the bridge field (== kind), `title` shared, plus
    // OKF reserved keys, plus the kusara graph layer under `kusara:`.
    #[serde(default, rename = "type")]
    typ: Option<Kind>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    resource: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    #[serde(default)]
    timestamp: Option<String>,
    #[serde(default)]
    kusara: Option<KusaraBlock>,
}

/// The kusara graph layer in the OKF shape. Mirrors `RefsBlock` minus the
/// hoisted `kind`/`title`. Unknown keys are rejected: this is our namespace.
///
/// NOTE: the graph field list is duplicated across three places that must
/// stay in sync when a field is added/removed: this struct (read), `RefsBlock`
/// (read, legacy shape), and `KusaraOut` in `emit_okf_frontmatter` (write).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct KusaraBlock {
    id: DocId,
    #[serde(default)]
    spec: Option<String>,
    #[serde(default)]
    provides: Vec<DocId>,
    #[serde(default)]
    implements: Vec<DocId>,
    #[serde(default)]
    depends_on: Vec<DocId>,
    #[serde(default)]
    related: Vec<DocId>,
    #[serde(default)]
    modules: Vec<String>,
    #[serde(default)]
    generated: bool,
    #[serde(default)]
    indexes_kind: Option<Kind>,
}

/// OKF reserved keys that have no place in the legacy `refs:` shape. Carried
/// onto `Doc` and surfaced in `show`/JSON; never validated for content.
#[derive(Debug, Clone, Default)]
struct OkfMeta {
    description: Option<String>,
    resource: Option<String>,
    tags: Vec<String>,
    timestamp: Option<String>,
}

/// NOTE: mirrors `KusaraBlock`'s graph fields (plus the legacy-only `kind`/
/// `title`). Keep in sync with `KusaraBlock` (read) and `KusaraOut` in
/// `emit_okf_frontmatter` (write) when a graph field is added.
#[derive(Debug, Deserialize)]
struct RefsBlock {
    id: DocId,
    kind: Kind,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    spec: Option<String>,
    #[serde(default)]
    provides: Vec<DocId>,
    #[serde(default)]
    implements: Vec<DocId>,
    #[serde(default)]
    depends_on: Vec<DocId>,
    #[serde(default)]
    related: Vec<DocId>,
    #[serde(default)]
    modules: Vec<String>,
    #[serde(default)]
    generated: bool,
    /// For `kind: index` — name of the kind being indexed.
    #[serde(default)]
    indexes_kind: Option<Kind>,
}

impl FrontMatter {
    /// Normalize either shape into `(RefsBlock, OkfMeta, is_legacy)`.
    /// `Ok(None)` = the file carries no kusara metadata (skip it).
    /// `Err` = ambiguous shape or a missing required field.
    ///
    /// The presence of the `kusara:` block -- not `type:` -- is the trigger
    /// for "this is an OKF kusara doc". A bare top-level `type:` with no
    /// `kusara:` block is a non-kusara doc and is silently skipped (per
    /// product decision); `refs:` plus a stray top-level `type:` is legacy,
    /// not ambiguous (only `refs:` + `kusara:` together are ambiguous).
    fn normalize(self) -> Result<Option<(RefsBlock, OkfMeta, bool)>, String> {
        let FrontMatter {
            refs,
            typ,
            title,
            description,
            resource,
            tags,
            timestamp,
            kusara,
        } = self;
        match (refs, kusara) {
            (Some(_), Some(_)) => {
                Err("ambiguous front matter: both `refs:` and `kusara:` present".into())
            }
            (Some(refs), None) => Ok(Some((refs, OkfMeta::default(), true))),
            (None, Some(k)) => {
                let kind =
                    typ.ok_or("OKF front matter has `kusara:` but is missing required `type:`")?;
                let refs = RefsBlock {
                    id: k.id,
                    kind,
                    title,
                    spec: k.spec,
                    provides: k.provides,
                    implements: k.implements,
                    depends_on: k.depends_on,
                    related: k.related,
                    modules: k.modules,
                    generated: k.generated,
                    indexes_kind: k.indexes_kind,
                };
                let okf = OkfMeta {
                    description,
                    resource,
                    tags,
                    timestamp,
                };
                Ok(Some((refs, okf, false)))
            }
            (None, None) => Ok(None),
        }
    }
}

#[derive(Debug, Clone)]
struct Doc {
    id: DocId,
    class: DocClass,
    title: Option<String>,
    spec: Option<String>,
    rel_path: PathBuf,
    provides: Vec<DocId>,
    implements: Vec<DocId>,
    depends_on: Vec<DocId>,
    related: Vec<DocId>,
    modules: Vec<String>,
    /// OKF reserved metadata, carried through from either shape.
    description: Option<String>,
    resource: Option<String>,
    tags: Vec<String>,
    timestamp: Option<String>,
}

/// Classifies a doc as either user-authored or `kusara`-generated.
#[derive(Debug, Clone)]
enum DocClass {
    /// Hand-authored doc with the given kind. The kind is never `"index"`.
    Regular(Kind),
    /// Generated INDEX doc with `kind: index` and `generated: true`.
    Index(IndexFlavor),
}

#[derive(Debug, Clone)]
enum IndexFlavor {
    /// Global INDEX (e.g. `map.md`, `ai/modules.md`); no `indexes_kind`.
    Global,
    /// Per-kind INDEX pointing at the named kind.
    PerKind(Kind),
}

impl Doc {
    fn kind(&self) -> Kind {
        match &self.class {
            DocClass::Regular(k) => k.clone(),
            DocClass::Index(_) => Kind::from(Kind::INDEX_LITERAL),
        }
    }

    fn generated(&self) -> bool {
        matches!(self.class, DocClass::Index(_))
    }

    fn indexes_kind(&self) -> Option<&Kind> {
        match &self.class {
            DocClass::Index(IndexFlavor::PerKind(k)) => Some(k),
            _ => None,
        }
    }
}

type EdgeMap = HashMap<DocId, BTreeSet<DocId>>;

struct Graph {
    docs: BTreeMap<DocId, Doc>,
    /// Maps every known id (including `provides:` aliases) to the id of the
    /// owning doc in `docs`. A doc's primary id maps to itself.
    id_to_doc: HashMap<DocId, DocId>,
    forward: EdgeMap,
    reverse: EdgeMap,
    related_forward: EdgeMap,
    related_reverse: EdgeMap,
}

// ---------------------------------------------------------------------------
// Entry
// ---------------------------------------------------------------------------

fn main() -> ExitCode {
    match real_main() {
        Ok(rc) => rc,
        Err(e) => {
            eprintln!("error: {e:#}");
            ExitCode::from(2)
        }
    }
}

fn real_main() -> Result<ExitCode> {
    let cli = Cli::parse();
    let root = match cli.root.clone() {
        Some(p) => p,
        None => std::env::current_dir().context("resolve current working directory")?,
    };
    let doc_root = match std::env::var("KUSARA_DOC_ROOT") {
        Ok(s) => PathBuf::from(s),
        Err(std::env::VarError::NotPresent) => PathBuf::from(DEFAULT_DOC_ROOT),
        Err(std::env::VarError::NotUnicode(_)) => {
            bail!("KUSARA_DOC_ROOT is not valid UTF-8");
        }
    };
    run(&cli, &root, &doc_root)
}

fn run(cli: &Cli, root: &Path, doc_root: &Path) -> Result<ExitCode> {
    // Hook adapters dispatch before the manifest/graph load: `postedit` must
    // stay on a fast no-graph path, and `stop` reports load failures as hook
    // context instead of a hard error.
    if let Cmd::Hook { cmd } = &cli.cmd {
        return run_hook(cmd, root, doc_root);
    }
    let manifest = Manifest::load(root, doc_root)?;
    let warn_deprecated = !matches!(cli.cmd, Cmd::Migrate { .. });
    let (graph, parse_errors) = build_graph(root, doc_root, &manifest, warn_deprecated)?;
    if !matches!(cli.cmd, Cmd::Validate) && !parse_errors.is_empty() {
        eprintln!(
            "warning: {} parse error(s); run `kusara validate` for details",
            parse_errors.len()
        );
    }
    match &cli.cmd {
        Cmd::Validate => cmd_validate(root, &manifest, &graph, &parse_errors),
        Cmd::Impact {
            ids,
            depth,
            include_related,
        } => cmd_traverse(&graph, ids, *depth, Direction::Forward, *include_related),
        Cmd::Deps {
            ids,
            depth,
            include_related,
        } => cmd_traverse(&graph, ids, *depth, Direction::Reverse, *include_related),
        Cmd::Show { id } => cmd_show(&graph, id),
        Cmd::Touched { files, no_closure } => cmd_touched(root, &graph, files, *no_closure),
        Cmd::Index { target } => cmd_index(root, doc_root, &manifest, &graph, *target),
        Cmd::Stale => cmd_stale(root, &graph),
        Cmd::List => cmd_list(&graph),
        Cmd::Migrate { dry_run } => cmd_migrate(root, doc_root, &manifest, *dry_run),
        Cmd::Hook { .. } => unreachable!("hook commands dispatch before the graph load"),
    }
}

// ---------------------------------------------------------------------------
// Scan & graph build
// ---------------------------------------------------------------------------

/// Source syntax kusara knows how to read metadata from.
#[derive(Clone, Copy)]
enum DocFormat {
    Markdown,
    Html,
}

fn build_graph(
    root: &Path,
    doc_root: &Path,
    manifest: &Manifest,
    warn_deprecated: bool,
) -> Result<(Graph, Vec<String>)> {
    let mut docs: BTreeMap<DocId, Doc> = BTreeMap::new();
    let mut id_to_doc: HashMap<DocId, DocId> = HashMap::new();
    let mut errors: Vec<String> = Vec::new();

    let scan_roots = derive_scan_roots(manifest, doc_root);
    let mut visited_files: BTreeSet<PathBuf> = BTreeSet::new();
    for rel in scan_roots {
        let scan = root.join(&rel);
        match fs::metadata(&scan) {
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => {
                errors.push(format!("scan root {}: {e}", scan.display()));
                continue;
            }
        }
        let walker = WalkDir::new(&scan).into_iter().filter_entry(|e| {
            !e.file_name()
                .to_str()
                .map(|n| SKIP_DIRS.contains(&n))
                .unwrap_or(false)
        });
        for entry in walker {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    errors.push(format!("walk {}: {e}", scan.display()));
                    continue;
                }
            };
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let format = match path.extension().and_then(|s| s.to_str()) {
                Some("md") => DocFormat::Markdown,
                Some("html") | Some("htm") => DocFormat::Html,
                _ => continue,
            };
            let Ok(rel_path) = path.strip_prefix(root) else {
                errors.push(format!(
                    "path {} escaped scan root {}",
                    path.display(),
                    root.display()
                ));
                continue;
            };
            let rel = rel_path.to_path_buf();
            if !visited_files.insert(rel.clone()) {
                continue;
            }

            let raw = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(e) => {
                    errors.push(format!("{}: read failed: {e}", rel.display()));
                    continue;
                }
            };
            let yaml = match format {
                DocFormat::Markdown => match extract_frontmatter(&raw) {
                    Some(y) => y,
                    None => {
                        if raw.starts_with("---\n") || raw.starts_with("---\r\n") {
                            errors.push(format!(
                                "{}: malformed front matter (missing closing `---`)",
                                rel.display()
                            ));
                        }
                        continue;
                    }
                },
                DocFormat::Html => match extract_html_metadata(&raw) {
                    HtmlMeta::Found(y) => y,
                    HtmlMeta::None => continue,
                    HtmlMeta::Unterminated => {
                        errors.push(format!(
                            "{}: malformed metadata block (missing closing `</script>`)",
                            rel.display()
                        ));
                        continue;
                    }
                },
            };
            let fm: FrontMatter = match serde_yaml_ng::from_str(yaml) {
                Ok(v) => v,
                Err(e) => {
                    errors.push(format!("{}: front matter parse error: {e}", rel.display()));
                    continue;
                }
            };
            let (refs_block, okf_meta, is_legacy) = match fm.normalize() {
                Ok(Some(v)) => v,
                Ok(None) => continue,
                Err(msg) => {
                    errors.push(format!("{}: {msg}", rel.display()));
                    continue;
                }
            };
            if is_legacy && warn_deprecated {
                eprintln!(
                    "warning: {}: legacy `refs:` front matter is deprecated; run `kusara migrate`",
                    rel.display()
                );
            }
            if !manifest.knows(&refs_block.kind) {
                errors.push(format!(
                    "{} ({}): unknown kind `{}` (not declared in kinds.md)",
                    rel.display(),
                    refs_block.id,
                    refs_block.kind
                ));
                continue;
            }
            let class = match (
                refs_block.kind.is_index(),
                refs_block.generated,
                refs_block.indexes_kind.clone(),
            ) {
                (false, false, None) => DocClass::Regular(refs_block.kind.clone()),
                (true, true, None) => DocClass::Index(IndexFlavor::Global),
                (true, true, Some(k)) => DocClass::Index(IndexFlavor::PerKind(k)),
                (true, false, _) | (false, true, _) => {
                    errors.push(format!(
                        "{} ({}): kind=`{}` and generated={} must agree (kind `index` requires generated: true and vice versa)",
                        rel.display(),
                        refs_block.id,
                        refs_block.kind,
                        refs_block.generated,
                    ));
                    continue;
                }
                (false, false, Some(_)) => {
                    errors.push(format!(
                        "{} ({}): indexes_kind set on non-index doc (kind=`{}`)",
                        rel.display(),
                        refs_block.id,
                        refs_block.kind,
                    ));
                    continue;
                }
            };
            let doc = Doc {
                id: refs_block.id.clone(),
                class,
                title: refs_block.title,
                spec: refs_block.spec,
                rel_path: rel.clone(),
                provides: refs_block.provides,
                implements: refs_block.implements,
                depends_on: refs_block.depends_on,
                related: refs_block.related,
                modules: refs_block.modules,
                description: okf_meta.description,
                resource: okf_meta.resource,
                tags: okf_meta.tags,
                timestamp: okf_meta.timestamp,
            };
            if let Some(prev) = docs.get(&doc.id) {
                errors.push(format!(
                    "duplicate id `{}` in {} and {}",
                    doc.id,
                    prev.rel_path.display(),
                    rel.display()
                ));
                continue;
            }
            let mut conflict = false;
            for pid in &doc.provides {
                if let Some(owner) = id_to_doc.get(pid) {
                    errors.push(format!(
                        "duplicate id `{}` provided by both {} and {}",
                        pid,
                        docs.get(owner)
                            .map(|d| d.rel_path.display().to_string())
                            .unwrap_or_default(),
                        rel.display()
                    ));
                    conflict = true;
                }
            }
            if conflict {
                continue;
            }
            for pid in &doc.provides {
                id_to_doc.insert(pid.clone(), doc.id.clone());
            }
            id_to_doc.insert(doc.id.clone(), doc.id.clone());
            docs.insert(doc.id.clone(), doc);
        }
    }

    let (forward, reverse, related_forward, related_reverse) =
        build_edges(&docs, &id_to_doc, &mut errors);
    Ok((
        Graph {
            docs,
            id_to_doc,
            forward,
            reverse,
            related_forward,
            related_reverse,
        },
        errors,
    ))
}

/// Returns the directories to walk: each `path_glob` reduced to its longest
/// non-glob prefix, plus `doc_root`.
fn derive_scan_roots(manifest: &Manifest, doc_root: &Path) -> BTreeSet<PathBuf> {
    let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
    roots.insert(doc_root.to_path_buf());
    for kind in manifest.kinds.values() {
        for glob in &kind.path_globs {
            roots.insert(glob_root(glob));
        }
    }
    roots
}

fn glob_root(glob: &str) -> PathBuf {
    let meta_idx = glob.find(['*', '?', '[']);
    match meta_idx {
        None => Path::new(glob)
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(".")),
        Some(idx) => {
            let prefix = &glob[..idx];
            let trimmed = prefix.rsplit_once('/').map(|(a, _)| a).unwrap_or("");
            if trimmed.is_empty() {
                PathBuf::from(".")
            } else {
                PathBuf::from(trimmed)
            }
        }
    }
}

fn extract_frontmatter(raw: &str) -> Option<&str> {
    let body = raw
        .strip_prefix("---\n")
        .or_else(|| raw.strip_prefix("---\r\n"))?;
    let end = body.find("\n---\n").or_else(|| body.find("\n---\r\n"))?;
    Some(&body[..end])
}

const HTML_META_TYPE: &str = "application/kusara+yaml";

/// Result of scanning an HTML doc for its embedded `refs:` metadata block.
enum HtmlMeta<'a> {
    /// Inner text of the first matching `<script>` block (the `refs:` YAML).
    Found(&'a str),
    /// No matching metadata block present.
    None,
    /// A matching opening tag was found but never closed.
    Unterminated,
}

/// Extracts the body of the first
/// `<script type="application/kusara+yaml">…</script>` block.
///
/// Tag name and attributes are matched case-insensitively (HTML is
/// case-insensitive). The whole file is scanned — the block need not be in
/// `<head>` — and the first matching block wins, mirroring "first frontmatter
/// wins" for Markdown. Script raw-text content needs no entity decoding.
fn extract_html_metadata(raw: &str) -> HtmlMeta<'_> {
    // Lowercase copy for case-insensitive search. `to_ascii_lowercase` only
    // rewrites ASCII bytes, so byte offsets stay aligned with `raw`.
    let lower = raw.to_ascii_lowercase();
    let mut from = 0usize;
    loop {
        let Some(rel) = lower[from..].find("<script") else {
            return HtmlMeta::None;
        };
        let tag_start = from + rel;
        let after_kw = tag_start + "<script".len();
        // Require a tag boundary so `<scriptx` does not match `<script`.
        match lower[after_kw..].chars().next() {
            Some(c) if c.is_ascii_whitespace() || c == '>' => {}
            _ => {
                from = after_kw;
                continue;
            }
        }
        let Some(rel_gt) = lower[after_kw..].find('>') else {
            return HtmlMeta::Unterminated;
        };
        let tag_end = after_kw + rel_gt; // byte index of '>'
        let open_tag = &lower[tag_start..tag_end]; // excludes '>'
        if script_type_matches(open_tag) {
            let content_start = tag_end + 1;
            let Some(rel_close) = lower[content_start..].find("</script") else {
                return HtmlMeta::Unterminated;
            };
            let content_end = content_start + rel_close;
            return HtmlMeta::Found(&raw[content_start..content_end]);
        }
        from = tag_end + 1;
    }
}

/// True when a lowercased `<script …` opening tag (without the closing `>`)
/// carries `type="application/kusara+yaml"`. Assumes canonical generator
/// output: no whitespace around `=` in the `type` attribute.
fn script_type_matches(open_tag_lower: &str) -> bool {
    let attrs = open_tag_lower
        .strip_prefix("<script")
        .unwrap_or(open_tag_lower);
    for token in attrs.split_ascii_whitespace() {
        if let Some(val) = token.strip_prefix("type=") {
            let v = val.trim_matches('"').trim_matches('\'');
            return v == HTML_META_TYPE;
        }
    }
    false
}

fn build_edges(
    docs: &BTreeMap<DocId, Doc>,
    id_to_doc: &HashMap<DocId, DocId>,
    errors: &mut Vec<String>,
) -> (EdgeMap, EdgeMap, EdgeMap, EdgeMap) {
    let mut forward: EdgeMap = HashMap::new();
    let mut reverse: EdgeMap = HashMap::new();
    let mut related_forward: EdgeMap = HashMap::new();
    let mut related_reverse: EdgeMap = HashMap::new();
    for doc in docs.values() {
        for target in &doc.related {
            if !id_to_doc.contains_key(target) {
                errors.push(format!(
                    "{} ({}): dangling reference `{}` (related)",
                    doc.rel_path.display(),
                    doc.id,
                    target
                ));
                continue;
            }
            related_forward
                .entry(doc.id.clone())
                .or_default()
                .insert(target.clone());
            related_reverse
                .entry(target.clone())
                .or_default()
                .insert(doc.id.clone());
        }
        let edges = doc.implements.iter().chain(doc.depends_on.iter());
        for target in edges {
            if !id_to_doc.contains_key(target) {
                errors.push(format!(
                    "{} ({}): dangling reference `{}`",
                    doc.rel_path.display(),
                    doc.id,
                    target
                ));
                continue;
            }
            forward
                .entry(doc.id.clone())
                .or_default()
                .insert(target.clone());
            reverse
                .entry(target.clone())
                .or_default()
                .insert(doc.id.clone());
        }
    }
    (forward, reverse, related_forward, related_reverse)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

fn cmd_validate(
    root: &Path,
    manifest: &Manifest,
    graph: &Graph,
    parse_errors: &[String],
) -> Result<ExitCode> {
    let errors = collect_validate_errors(root, manifest, graph, parse_errors);
    if errors.is_empty() {
        println!("OK ({} docs)", graph.docs.len());
        Ok(ExitCode::SUCCESS)
    } else {
        for e in &errors {
            eprintln!("- {e}");
        }
        eprintln!("\n{} error(s)", errors.len());
        Ok(ExitCode::from(1))
    }
}

fn collect_validate_errors(
    root: &Path,
    manifest: &Manifest,
    graph: &Graph,
    parse_errors: &[String],
) -> Vec<String> {
    let mut errors: Vec<String> = parse_errors.to_vec();
    for doc in graph.docs.values() {
        for m in &doc.modules {
            let abs = root.join(m.trim_end_matches('/'));
            if !abs.exists() {
                errors.push(format!(
                    "{} ({}): module path `{}` does not exist",
                    doc.rel_path.display(),
                    doc.id,
                    m
                ));
            }
        }
    }

    // Every file matched by a kind's `path_globs` must have a `refs:` block.
    let in_graph: BTreeSet<PathBuf> = graph.docs.values().map(|d| d.rel_path.clone()).collect();
    for kind in manifest.kinds.values() {
        for pat in &kind.path_globs {
            let absolute = format!("{}/{}", root.display(), pat);
            let matches = match glob::glob(&absolute) {
                Ok(m) => m,
                Err(e) => {
                    errors.push(format!("kind `{}`: bad glob `{}`: {e}", kind.name, pat));
                    continue;
                }
            };
            for entry in matches {
                let path = match entry {
                    Ok(p) => p,
                    Err(e) => {
                        errors.push(format!("kind `{}` glob `{}`: {e}", kind.name, pat));
                        continue;
                    }
                };
                let rel = path.strip_prefix(root).unwrap_or(&path).to_path_buf();
                if !in_graph.contains(&rel) {
                    errors.push(format!(
                        "{}: matches kind `{}` glob `{}` but has no kusara front matter (`type:` + `kusara:`, or legacy `refs:`)",
                        rel.display(),
                        kind.name,
                        pat
                    ));
                }
            }
        }
    }

    errors
}

#[derive(Copy, Clone)]
enum Direction {
    Forward,
    Reverse,
}

fn cmd_traverse(
    graph: &Graph,
    ids: &[String],
    depth: u32,
    dir: Direction,
    include_related: bool,
) -> Result<ExitCode> {
    if ids.is_empty() {
        bail!("at least one id is required");
    }
    let mut missing: Vec<&str> = Vec::new();
    let mut seeds: Vec<DocId> = Vec::with_capacity(ids.len());
    for id in ids {
        if graph.id_to_doc.contains_key(id.as_str()) {
            seeds.push(DocId::from(id.as_str()));
        } else {
            missing.push(id.as_str());
        }
    }
    if !missing.is_empty() {
        bail!("unknown id(s): {}", missing.join(", "));
    }
    let (hard, soft) = match dir {
        Direction::Forward => (&graph.reverse, &graph.related_reverse),
        Direction::Reverse => (&graph.forward, &graph.related_forward),
    };
    let header = match dir {
        Direction::Forward => "Affected by changes to:",
        Direction::Reverse => "Dependencies of:",
    };
    println!("{header}");
    for id in ids {
        println!("  {id}");
    }
    if include_related {
        println!("(including soft `related:` edges)");
    }
    println!();

    let mut seen: BTreeSet<DocId> = seeds.iter().cloned().collect();
    let mut frontier: VecDeque<(DocId, u32)> = seeds.iter().map(|id| (id.clone(), 0)).collect();
    let mut layered: BTreeMap<u32, BTreeSet<DocId>> = BTreeMap::new();

    while let Some((cur, d)) = frontier.pop_front() {
        if d == depth {
            continue;
        }
        let mut neighbors: BTreeSet<DocId> = BTreeSet::new();
        if let Some(s) = hard.get(&cur) {
            neighbors.extend(s.iter().cloned());
        }
        if include_related {
            if let Some(s) = soft.get(&cur) {
                neighbors.extend(s.iter().cloned());
            }
        }
        for n in neighbors {
            if seen.insert(n.clone()) {
                layered.entry(d + 1).or_default().insert(n.clone());
                frontier.push_back((n.clone(), d + 1));
            }
        }
    }

    if layered.is_empty() {
        println!("(none)");
        return Ok(ExitCode::SUCCESS);
    }
    for (d, set) in &layered {
        println!("depth {d}:");
        for id in set {
            print_id_line(graph, id.as_str(), "  ");
        }
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_show(graph: &Graph, id: &str) -> Result<ExitCode> {
    let doc_id = graph
        .id_to_doc
        .get(id)
        .ok_or_else(|| anyhow!("unknown id `{id}`"))?;
    let doc = graph
        .docs
        .get(doc_id)
        .ok_or_else(|| anyhow!("internal: doc `{doc_id}` missing"))?;

    // All labels below pad to the same column (14 chars: enough to fit the
    // widest label, `indexes_kind:`, plus one space) so values line up.
    println!("id:           {}", doc.id);
    if doc.id.as_str() != id {
        println!("queried:      {id}  (provided by {})", doc.id);
    }
    println!("kind:         {}", doc.kind());
    if doc.generated() {
        println!("generated:    true");
        if let Some(k) = doc.indexes_kind() {
            println!("indexes_kind: {k}");
        }
    }
    if let Some(s) = &doc.spec {
        println!("spec:         {s}");
    }
    if let Some(t) = &doc.title {
        println!("title:        {t}");
    }
    if let Some(d) = &doc.description {
        println!("description:  {d}");
    }
    if let Some(r) = &doc.resource {
        println!("resource:     {r}");
    }
    if !doc.tags.is_empty() {
        println!("tags:         {}", doc.tags.join(", "));
    }
    if let Some(ts) = &doc.timestamp {
        println!("timestamp:    {ts}");
    }
    println!("path:         {}", doc.rel_path.display());

    print_list("provides:", &doc.provides);
    print_list("implements:", &doc.implements);
    print_list("depends_on:", &doc.depends_on);
    print_list("related:", &doc.related);
    print_list("modules:", &doc.modules);

    println!();
    println!("Direct impact (hard, who depends on this doc):");
    if let Some(rev) = graph.reverse.get(&doc.id) {
        for r in rev {
            print_id_line(graph, r.as_str(), "  ");
        }
    } else {
        println!("  (none)");
    }
    println!();
    println!("Soft mentions (related: from other docs):");
    if let Some(soft) = graph.related_reverse.get(&doc.id) {
        for r in soft {
            print_id_line(graph, r.as_str(), "  ");
        }
    } else {
        println!("  (none)");
    }
    Ok(ExitCode::SUCCESS)
}

fn cmd_touched(
    root: &Path,
    graph: &Graph,
    files: &[PathBuf],
    no_closure: bool,
) -> Result<ExitCode> {
    if files.is_empty() {
        bail!("at least one file is required");
    }
    let mut rel_files: Vec<String> = Vec::new();
    for f in files {
        let abs = if f.is_absolute() {
            f.clone()
        } else {
            root.join(f)
        };
        let rel = abs
            .strip_prefix(root)
            .unwrap_or(f)
            .to_string_lossy()
            .to_string();
        rel_files.push(normalize_separators(&rel));
    }

    let mut hits: BTreeSet<DocId> = BTreeSet::new();
    for doc in graph.docs.values() {
        for m in &doc.modules {
            let pat = normalize_separators(m);
            for f in &rel_files {
                if module_covers(&pat, f) {
                    hits.insert(doc.id.clone());
                }
            }
        }
    }

    println!("Files:");
    for f in &rel_files {
        println!("  {f}");
    }
    println!();
    if hits.is_empty() {
        println!("No docs claim these files in `modules:`.");
        return Ok(ExitCode::SUCCESS);
    }
    println!("Docs of record (modules: covers these files):");
    for id in &hits {
        print_id_line(graph, id.as_str(), "  ");
    }

    if no_closure {
        return Ok(ExitCode::SUCCESS);
    }
    let mut seen = hits.clone();
    let mut frontier: VecDeque<DocId> = hits.iter().cloned().collect();
    let mut indirect: BTreeSet<DocId> = BTreeSet::new();
    while let Some(cur) = frontier.pop_front() {
        if let Some(rev) = graph.reverse.get(&cur) {
            for n in rev {
                if seen.insert(n.clone()) {
                    indirect.insert(n.clone());
                    frontier.push_back(n.clone());
                }
            }
        }
    }
    if indirect.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }
    println!();
    println!("Transitively affected:");
    for id in &indirect {
        print_id_line(graph, id.as_str(), "  ");
    }
    Ok(ExitCode::SUCCESS)
}

fn module_covers(pattern: &str, file: &str) -> bool {
    if let Some(prefix) = pattern.strip_suffix('/') {
        // "a/b/" matches any file *under* a/b, not a/b itself.
        file.starts_with(prefix) && file[prefix.len()..].starts_with('/')
    } else {
        file == pattern
    }
}

/// Replaces every `\` with `/`.
fn normalize_separators(s: &str) -> String {
    if s.contains('\\') {
        s.replace('\\', "/")
    } else {
        s.to_owned()
    }
}

// ---------------------------------------------------------------------------
// Stale detection (git history)
// ---------------------------------------------------------------------------

/// Last commit that touched a path: unix committer time, short hash, `%cs` date.
struct GitStamp {
    time: i64,
    hash: String,
    date: String,
}

/// Returns the last commit touching `rel_path` (a file, or a directory whose
/// history covers everything under it). `Ok(None)` = no commit touches it
/// (untracked or brand-new). Errors when `root` is not inside a git work tree.
fn git_last_commit(root: &Path, rel_path: &str) -> Result<Option<GitStamp>> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["log", "-1", "--format=%ct %h %cs", "--"])
        .arg(rel_path)
        .output()
        .context("run git (is git installed?)")?;
    if !out.status.success() {
        bail!(
            "git log failed for `{}`: {}",
            rel_path,
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let line = stdout.trim();
    if line.is_empty() {
        return Ok(None);
    }
    let mut parts = line.split_ascii_whitespace();
    let (Some(time), Some(hash), Some(date)) = (parts.next(), parts.next(), parts.next()) else {
        bail!("unexpected git log output for `{rel_path}`: {line}");
    };
    Ok(Some(GitStamp {
        time: time
            .parse()
            .with_context(|| format!("parse commit time `{time}`"))?,
        hash: hash.to_owned(),
        date: date.to_owned(),
    }))
}

fn cmd_stale(root: &Path, graph: &Graph) -> Result<ExitCode> {
    // path (trailing slash stripped) -> last commit; shared across docs.
    let mut cache: HashMap<String, Option<GitStamp>> = HashMap::new();
    struct Finding<'a> {
        doc: &'a Doc,
        doc_stamp: GitStamp,
        module: String,
        module_stamp: GitStamp,
    }
    let mut findings: Vec<Finding> = Vec::new();
    let mut checked = 0usize;
    for doc in graph.docs.values() {
        if doc.generated() || doc.modules.is_empty() {
            continue;
        }
        let doc_rel = normalize_separators(&doc.rel_path.to_string_lossy());
        // An uncommitted doc is being written right now; it cannot be stale.
        let Some(doc_stamp) = git_last_commit(root, &doc_rel)? else {
            continue;
        };
        checked += 1;
        let mut newest: Option<(String, GitStamp)> = None;
        for m in &doc.modules {
            let key = normalize_separators(m.trim_end_matches('/'));
            let stamp = match cache.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(e) => e.into_mut(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(git_last_commit(root, &key)?)
                }
            };
            let Some(stamp) = stamp else { continue };
            if stamp.time <= doc_stamp.time {
                continue;
            }
            if newest.as_ref().is_none_or(|(_, s)| stamp.time > s.time) {
                newest = Some((
                    m.clone(),
                    GitStamp {
                        time: stamp.time,
                        hash: stamp.hash.clone(),
                        date: stamp.date.clone(),
                    },
                ));
            }
        }
        if let Some((module, module_stamp)) = newest {
            findings.push(Finding {
                doc,
                doc_stamp,
                module,
                module_stamp,
            });
        }
    }
    if findings.is_empty() {
        println!("OK ({checked} docs with modules checked)");
        return Ok(ExitCode::SUCCESS);
    }
    println!("Stale docs (code in `modules:` changed after the doc's last commit):");
    for f in &findings {
        println!("  {} ({})", f.doc.rel_path.display(), f.doc.id);
        println!(
            "    doc last commit:    {} {}",
            f.doc_stamp.date, f.doc_stamp.hash
        );
        println!(
            "    newer module path:  {} — {} {}",
            f.module, f.module_stamp.date, f.module_stamp.hash
        );
    }
    println!();
    println!("{} stale doc(s)", findings.len());
    Ok(ExitCode::from(1))
}

fn cmd_index(
    root: &Path,
    doc_root: &Path,
    manifest: &Manifest,
    graph: &Graph,
    target: IndexTarget,
) -> Result<ExitCode> {
    match target {
        IndexTarget::Map => write_map(root, doc_root, graph),
        IndexTarget::All => write_per_kind_indexes(root, manifest, graph),
    }
}

fn write_map(root: &Path, doc_root: &Path, graph: &Graph) -> Result<ExitCode> {
    let map_path = root.join(doc_root).join("map.md");
    let json_path = root.join(doc_root).join("ai/graph.json");
    let modules_path = root.join(doc_root).join("ai/modules.md");

    if let Some(parent) = json_path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
    }

    // 1. Human-readable map.md
    let mut by_kind: BTreeMap<Kind, Vec<&Doc>> = BTreeMap::new();
    for d in graph.docs.values() {
        if d.generated() {
            continue;
        }
        by_kind.entry(d.kind()).or_default().push(d);
    }
    let mut map = String::new();
    map.push_str(
        "---\ntype: index\ntitle: \"Doc Map (all kinds)\"\nkusara:\n  id: index:map\n  generated: true\n---\n\n",
    );
    map.push_str("# Doc Map\n\n");
    map.push_str("Generated by `kusara index map`. Do not edit by hand.\n");
    map.push_str("All docs across kinds. For per-kind indexes see the per-kind INDEX files; for AI consumption see [ai/graph.json](ai/graph.json).\n\n");
    for (kind, docs) in &by_kind {
        map.push_str(&format!("## {} ({})\n\n", kind, docs.len()));
        map.push_str("| ID | Title | Spec | Path |\n|---|---|---|---|\n");
        for d in docs {
            map.push_str(&format!(
                "| `{}` | {} | {} | [{}]({}) |\n",
                d.id,
                d.title.clone().unwrap_or_default(),
                d.spec.clone().unwrap_or_default(),
                d.rel_path.display(),
                doc_link(doc_root, &d.rel_path),
            ));
        }
        map.push('\n');
    }
    fs::write(&map_path, map).with_context(|| format!("write {}", map_path.display()))?;
    println!("wrote {}", map_path.display());

    // 2. AI graph.json
    let json = build_graph_json(graph)?;
    fs::write(&json_path, json).with_context(|| format!("write {}", json_path.display()))?;
    println!("wrote {}", json_path.display());

    // 3. ai/modules.md
    let mut by_module: BTreeMap<&String, BTreeSet<&DocId>> = BTreeMap::new();
    for d in graph.docs.values() {
        for m in &d.modules {
            by_module.entry(m).or_default().insert(&d.id);
        }
    }
    let mut mm = String::new();
    mm.push_str(
        "---\ntype: index\ntitle: \"Source -> Doc Map\"\nkusara:\n  id: index:modules\n  generated: true\n---\n\n",
    );
    mm.push_str("# Source -> Doc Map\n\n");
    mm.push_str("Generated by `kusara index map`. Do not edit by hand.\n\n");
    mm.push_str("| Source path | Docs of record |\n|---|---|\n");
    for (m, ids) in &by_module {
        let cell: Vec<String> = ids.iter().map(|i| format!("`{i}`")).collect();
        mm.push_str(&format!("| `{}` | {} |\n", m, cell.join(", ")));
    }
    fs::write(&modules_path, mm).with_context(|| format!("write {}", modules_path.display()))?;
    println!("wrote {}", modules_path.display());

    Ok(ExitCode::SUCCESS)
}

/// Returns `rel_path` rewritten relative to `doc_root` (where `map.md` lives).
/// Paths under `doc_root` are stripped of that prefix; paths outside it are
/// prefixed with one `../` per `doc_root` component.
fn doc_link(doc_root: &Path, rel_path: &Path) -> String {
    if let Ok(stripped) = rel_path.strip_prefix(doc_root) {
        return stripped.display().to_string();
    }
    let depth = doc_root.components().count();
    let mut up = String::new();
    for _ in 0..depth {
        up.push_str("../");
    }
    format!("{}{}", up, rel_path.display())
}

#[derive(serde::Serialize)]
struct JsonGraph<'a> {
    schema_version: u32,
    docs: Vec<JsonDoc<'a>>,
    modules: BTreeMap<&'a String, BTreeSet<&'a DocId>>,
}

#[derive(serde::Serialize)]
struct JsonDoc<'a> {
    id: &'a DocId,
    kind: Kind,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    spec: Option<&'a String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    generated: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    indexes_kind: Option<&'a Kind>,
    provides: &'a [DocId],
    implements: &'a [DocId],
    depends_on: &'a [DocId],
    related: &'a [DocId],
    modules: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<&'a String>,
    #[serde(skip_serializing_if = "slice_is_empty")]
    tags: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<&'a String>,
}

fn slice_is_empty<T>(s: &&[T]) -> bool {
    s.is_empty()
}

fn build_graph_json(graph: &Graph) -> Result<String> {
    let docs: Vec<JsonDoc> = graph
        .docs
        .values()
        .map(|d| JsonDoc {
            id: &d.id,
            kind: d.kind(),
            path: d.rel_path.display().to_string(),
            title: d.title.as_ref(),
            spec: d.spec.as_ref(),
            generated: d.generated(),
            indexes_kind: d.indexes_kind(),
            provides: &d.provides,
            implements: &d.implements,
            depends_on: &d.depends_on,
            related: &d.related,
            modules: &d.modules,
            description: d.description.as_ref(),
            resource: d.resource.as_ref(),
            tags: &d.tags,
            timestamp: d.timestamp.as_ref(),
        })
        .collect();
    let mut modules: BTreeMap<&String, BTreeSet<&DocId>> = BTreeMap::new();
    for d in graph.docs.values() {
        for m in &d.modules {
            modules.entry(m).or_default().insert(&d.id);
        }
    }
    let payload = JsonGraph {
        schema_version: 1,
        docs,
        modules,
    };
    let mut s = serde_json::to_string_pretty(&payload).context("serialize graph.json")?;
    s.push('\n');
    Ok(s)
}

fn write_per_kind_indexes(root: &Path, manifest: &Manifest, graph: &Graph) -> Result<ExitCode> {
    let mut wrote = 0u32;
    for kind in manifest.kinds.values() {
        let Some(idx) = &kind.index else {
            continue;
        };
        let mut docs: Vec<&Doc> = graph
            .docs
            .values()
            .filter(|d| matches!(&d.class, DocClass::Regular(k) if *k == kind.name))
            .collect();
        docs.sort_by(|a, b| a.id.cmp(&b.id));

        let title = format!("{} Index", kind.name);
        let mut out = String::new();
        out.push_str("---\n");
        out.push_str("type: index\n");
        out.push_str(&format!("title: \"{title}\"\n"));
        out.push_str("kusara:\n");
        out.push_str(&format!("  id: index:{}\n", kind.name));
        out.push_str(&format!("  indexes_kind: {}\n", kind.name));
        out.push_str("  generated: true\n");
        out.push_str("---\n\n");
        out.push_str(&format!("# {title}\n\n"));
        out.push_str(&format!(
            "Generated by `kusara index`. Do not edit by hand. Lists every `{}` doc in this repository.\n\n",
            kind.name
        ));

        let group_by = idx.group_by.as_deref();
        if group_by == Some("spec") {
            let mut by_spec: BTreeMap<String, Vec<&Doc>> = BTreeMap::new();
            for d in &docs {
                by_spec
                    .entry(d.spec.clone().unwrap_or_else(|| "(unspec'd)".into()))
                    .or_default()
                    .push(d);
            }
            for (spec, ds) in &by_spec {
                out.push_str(&format!("## {spec}\n\n"));
                emit_kind_table(&mut out, ds);
            }
        } else {
            emit_kind_table(&mut out, &docs);
        }

        let target = root.join(&idx.output);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).with_context(|| format!("create {}", parent.display()))?;
        }
        fs::write(&target, out).with_context(|| format!("write {}", target.display()))?;
        println!("wrote {}", target.display());
        wrote += 1;
    }
    if wrote == 0 {
        println!("(no kinds with `index.output` configured)");
    }
    Ok(ExitCode::SUCCESS)
}

fn emit_kind_table(out: &mut String, docs: &[&Doc]) {
    out.push_str("| ID | Title | Implements | Depends on |\n|---|---|---|---|\n");
    for d in docs {
        out.push_str(&format!(
            "| `{}` | {} | {} | {} |\n",
            d.id,
            d.title.clone().unwrap_or_default(),
            d.implements
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", "),
            d.depends_on
                .iter()
                .map(|s| format!("`{s}`"))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    out.push('\n');
}

fn cmd_list(graph: &Graph) -> Result<ExitCode> {
    let mut ids: Vec<&DocId> = graph.id_to_doc.keys().collect();
    ids.sort();
    for id in ids {
        let Some(doc_id) = graph.id_to_doc.get(id) else {
            continue;
        };
        let Some(doc) = graph.docs.get(doc_id) else {
            eprintln!("warning: id `{id}` resolves to missing doc `{doc_id}`");
            continue;
        };
        println!("{:<40} {:<14} {}", id, doc.kind(), doc.rel_path.display());
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// Migrate
// ---------------------------------------------------------------------------

/// Serialize the OKF-shaped front matter body (between the `---` fences) for a
/// normalized doc. Deterministic field order. Ends with a trailing newline.
fn emit_okf_frontmatter(rb: &RefsBlock, okf: &OkfMeta) -> String {
    // NOTE: mirrors the graph fields of `KusaraBlock`/`RefsBlock` (read side).
    // Keep the three field lists in sync when a graph field is added.
    #[derive(serde::Serialize)]
    struct KusaraOut {
        id: DocId,
        #[serde(skip_serializing_if = "Option::is_none")]
        spec: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        implements: Vec<DocId>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        depends_on: Vec<DocId>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        related: Vec<DocId>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        provides: Vec<DocId>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        modules: Vec<String>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        generated: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        indexes_kind: Option<Kind>,
    }
    #[derive(serde::Serialize)]
    struct OkfOut {
        #[serde(rename = "type")]
        typ: Kind,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        description: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        resource: Option<String>,
        #[serde(skip_serializing_if = "Vec::is_empty")]
        tags: Vec<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<String>,
        kusara: KusaraOut,
    }
    let out = OkfOut {
        typ: rb.kind.clone(),
        title: rb.title.clone(),
        description: okf.description.clone(),
        resource: okf.resource.clone(),
        tags: okf.tags.clone(),
        timestamp: okf.timestamp.clone(),
        kusara: KusaraOut {
            id: rb.id.clone(),
            spec: rb.spec.clone(),
            implements: rb.implements.clone(),
            depends_on: rb.depends_on.clone(),
            related: rb.related.clone(),
            provides: rb.provides.clone(),
            modules: rb.modules.clone(),
            generated: rb.generated,
            indexes_kind: rb.indexes_kind.clone(),
        },
    };
    serde_yaml_ng::to_string(&out).expect("serialize OKF front matter")
}

/// Replace `inner` (a subslice of `raw`) with `new_inner`, returning the whole
/// string. Relies on `inner` being a borrow into `raw`.
fn splice_subslice(raw: &str, inner: &str, new_inner: &str) -> String {
    let off = inner.as_ptr() as usize - raw.as_ptr() as usize;
    let mut out = String::with_capacity(raw.len() - inner.len() + new_inner.len());
    out.push_str(&raw[..off]);
    out.push_str(new_inner);
    out.push_str(&raw[off + inner.len()..]);
    out
}

fn cmd_migrate(
    root: &Path,
    doc_root: &Path,
    manifest: &Manifest,
    dry_run: bool,
) -> Result<ExitCode> {
    let mut changed = 0u32;
    let scan_roots = derive_scan_roots(manifest, doc_root);
    let mut visited: BTreeSet<PathBuf> = BTreeSet::new();
    for rel in scan_roots {
        let scan = root.join(&rel);
        if fs::metadata(&scan).is_err() {
            continue;
        }
        let walker = WalkDir::new(&scan).into_iter().filter_entry(|e| {
            !e.file_name()
                .to_str()
                .map(|n| SKIP_DIRS.contains(&n))
                .unwrap_or(false)
        });
        for entry in walker.flatten() {
            if !entry.file_type().is_file() {
                continue;
            }
            let path = entry.path();
            let format = match path.extension().and_then(|s| s.to_str()) {
                Some("md") => DocFormat::Markdown,
                Some("html") | Some("htm") => DocFormat::Html,
                _ => continue,
            };
            let Ok(rel_path) = path.strip_prefix(root) else {
                continue;
            };
            if !visited.insert(rel_path.to_path_buf()) {
                continue;
            }
            let raw = match fs::read_to_string(path) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let yaml = match format {
                DocFormat::Markdown => match extract_frontmatter(&raw) {
                    Some(y) => y,
                    None => continue,
                },
                DocFormat::Html => match extract_html_metadata(&raw) {
                    HtmlMeta::Found(y) => y,
                    _ => continue,
                },
            };
            let fm: FrontMatter = match serde_yaml_ng::from_str(yaml) {
                Ok(v) => v,
                Err(_) => continue,
            };
            if fm.refs.is_some() && fm.kusara.is_some() {
                eprintln!(
                    "warning: {}: ambiguous front matter (both refs: and kusara:), not migrated",
                    rel_path.display()
                );
                continue;
            }
            // Only legacy docs need migration.
            let Some(refs_block) = fm.refs else {
                continue;
            };
            let okf = OkfMeta::default();
            let body = emit_okf_frontmatter(&refs_block, &okf);
            let new_yaml = match format {
                // Markdown yaml has no surrounding newlines inside the fences;
                // `body` already ends with `\n`, matching `extract_frontmatter`
                // (which excludes the trailing `\n` before `---`). Drop
                // exactly the single trailing newline serde_yaml emits.
                DocFormat::Markdown => body.strip_suffix('\n').unwrap_or(&body).to_string(),
                // HTML script content in canonical output is newline-wrapped.
                DocFormat::Html => format!("\n{}", body),
            };
            let new_raw = splice_subslice(&raw, yaml, &new_yaml);
            if new_raw == raw {
                continue;
            }
            if dry_run {
                println!("would migrate {}", rel_path.display());
            } else {
                fs::write(path, new_raw).with_context(|| format!("write {}", path.display()))?;
                println!("migrated {}", rel_path.display());
            }
            changed += 1;
        }
    }
    if changed == 0 {
        println!("(nothing to migrate)");
    }
    Ok(ExitCode::SUCCESS)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Claude Code hook adapters
// ---------------------------------------------------------------------------

/// Subset of the Claude Code hook payload the adapters care about.
#[derive(Deserialize)]
struct HookPayload {
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    stop_hook_active: bool,
    #[serde(default)]
    tool_input: Option<HookToolInput>,
}

#[derive(Deserialize)]
struct HookToolInput {
    #[serde(default)]
    file_path: Option<String>,
}

fn run_hook(cmd: &HookCmd, root: &Path, doc_root: &Path) -> Result<ExitCode> {
    match cmd {
        HookCmd::Postedit { journal_dir } => hook_postedit(root, journal_dir.as_deref()),
        HookCmd::Stop { journal_dir, note } => {
            hook_stop(root, doc_root, journal_dir.as_deref(), note.as_deref())
        }
    }
}

/// Hooks are informational: a payload we cannot read or use means
/// "do nothing", never a user-visible failure.
fn read_hook_payload() -> Option<HookPayload> {
    let mut buf = String::new();
    std::io::Read::read_to_string(&mut std::io::stdin(), &mut buf).ok()?;
    serde_json::from_str(&buf).ok()
}

fn hook_journal_path(journal_dir: Option<&Path>, root: &Path, session_id: &str) -> PathBuf {
    use std::hash::{Hash, Hasher};
    let dir = journal_dir
        .map(Path::to_path_buf)
        .unwrap_or_else(default_journal_dir);
    // The root-path hash keys journals per repo so one Claude session that
    // touches several kusara-managed repos keeps separate journals.
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    root.hash(&mut hasher);
    let session: String = session_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    dir.join(format!("{:016x}-{session}.journal", hasher.finish()))
}

/// Journals reveal which files a session touched, so keep them out of the
/// world-writable shared temp dir: prefer a user-private cache directory.
/// (The Windows temp dir is already per-user.)
fn default_journal_dir() -> PathBuf {
    #[cfg(unix)]
    {
        if let Some(cache) = std::env::var_os("XDG_CACHE_HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(cache).join("kusara").join("hook");
        }
        if let Some(home) = std::env::var_os("HOME").filter(|v| !v.is_empty()) {
            return PathBuf::from(home)
                .join(".cache")
                .join("kusara")
                .join("hook");
        }
    }
    std::env::temp_dir().join("kusara-hook")
}

/// Defense in depth for journal privacy: 0700 directories on Unix.
fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
        // `create` leaves a pre-existing directory's mode untouched.
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))
    }
    #[cfg(not(unix))]
    fs::create_dir_all(dir)
}

/// Defense in depth for journal privacy: 0600 journal files on Unix.
fn open_private_append(path: &Path) -> std::io::Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.create(true).append(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)
}

fn hook_postedit(root: &Path, journal_dir: Option<&Path>) -> Result<ExitCode> {
    let Some(payload) = read_hook_payload() else {
        return Ok(ExitCode::SUCCESS);
    };
    let (Some(session_id), Some(file_path)) = (
        payload.session_id,
        payload.tool_input.and_then(|t| t.file_path),
    ) else {
        return Ok(ExitCode::SUCCESS);
    };
    let abs = PathBuf::from(&file_path);
    let abs = if abs.is_absolute() {
        abs
    } else {
        root.join(abs)
    };
    // Edits outside this repository are none of our business. The prefix
    // check below is lexical, so also reject `..` components: they can
    // survive strip_prefix yet resolve outside the repo. (Canonicalizing
    // instead would misreport when the edited file no longer exists.)
    if abs
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Ok(ExitCode::SUCCESS);
    }
    let Ok(rel) = abs.strip_prefix(root) else {
        return Ok(ExitCode::SUCCESS);
    };
    let journal = hook_journal_path(journal_dir, root, &session_id);
    if let Some(parent) = journal.parent() {
        let _ = create_private_dir(parent);
    }
    let line = format!("{}\n", normalize_separators(&rel.to_string_lossy()));
    // Best effort: a journal write failure must not surface as a hook error.
    let _ = open_private_append(&journal)
        .and_then(|mut f| std::io::Write::write_all(&mut f, line.as_bytes()));
    Ok(ExitCode::SUCCESS)
}

fn hook_stop(
    root: &Path,
    doc_root: &Path,
    journal_dir: Option<&Path>,
    note: Option<&str>,
) -> Result<ExitCode> {
    let Some(payload) = read_hook_payload() else {
        return Ok(ExitCode::SUCCESS);
    };
    let Some(session_id) = payload.session_id else {
        return Ok(ExitCode::SUCCESS);
    };
    let journal = hook_journal_path(journal_dir, root, &session_id);
    let Ok(raw) = fs::read_to_string(&journal) else {
        return Ok(ExitCode::SUCCESS); // nothing recorded this turn
    };
    let _ = fs::remove_file(&journal); // consume: each edit is reported once
    let files: BTreeSet<String> = raw
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(str::to_owned)
        .collect();
    if files.is_empty() {
        return Ok(ExitCode::SUCCESS);
    }

    let loaded = Manifest::load(root, doc_root)
        .and_then(|m| build_graph(root, doc_root, &m, false).map(|g| (m, g)));
    let (manifest, (graph, parse_errors)) = match loaded {
        Ok(x) => x,
        Err(e) => {
            emit_stop_context(
                &format!("kusara: cannot check this turn's edits: {e:#}"),
                note,
            );
            return Ok(ExitCode::SUCCESS);
        }
    };

    let errors = collect_validate_errors(root, &manifest, &graph, &parse_errors);
    if !errors.is_empty() {
        let mut msg = String::from("kusara validate fails after this turn's edits:\n");
        for e in &errors {
            msg.push_str(&format!("- {e}\n"));
        }
        msg.push_str("\nFix the refs metadata (or adjust the kinds manifest) before finishing.");
        // `stop_hook_active` means we already interrupted this stop once;
        // downgrade to non-blocking context to avoid a block loop.
        if payload.stop_hook_active {
            emit_stop_context(&msg, note);
        } else {
            emit_stop_block(&msg, note);
        }
        return Ok(ExitCode::SUCCESS);
    }

    if let Some(msg) = hook_stop_advisory(&graph, &files) {
        emit_stop_context(&msg, note);
    }
    Ok(ExitCode::SUCCESS)
}

/// Advisory body for a clean-validate stop: docs of record whose `modules:`
/// cover the edited sources, plus link fan-out of edited managed docs.
fn hook_stop_advisory(graph: &Graph, files: &BTreeSet<String>) -> Option<String> {
    let mut hits: BTreeMap<&DocId, BTreeSet<&str>> = BTreeMap::new();
    for doc in graph.docs.values() {
        for m in &doc.modules {
            let pat = normalize_separators(m);
            for f in files {
                if module_covers(&pat, f) {
                    hits.entry(&doc.id).or_default().insert(f.as_str());
                }
            }
        }
    }
    let edited_docs: Vec<&Doc> = graph
        .docs
        .values()
        .filter(|d| files.contains(&normalize_separators(&d.rel_path.to_string_lossy())))
        .collect();
    if hits.is_empty() && edited_docs.is_empty() {
        return None;
    }

    let mut msg = format!(
        "kusara end-of-turn check ({} edited file(s)).\n",
        files.len()
    );
    if !hits.is_empty() {
        msg.push_str("\nDocs of record whose `modules:` cover this turn's source edits:\n");
        for (id, via) in &hits {
            let path = graph
                .docs
                .get(*id)
                .map(|d| d.rel_path.display().to_string())
                .unwrap_or_default();
            msg.push_str(&format!("  {id}  {path}\n"));
            msg.push_str(&format!(
                "    via: {}\n",
                via.iter().copied().collect::<Vec<_>>().join(", ")
            ));
        }
    }
    if !edited_docs.is_empty() {
        msg.push_str("\nEdited managed docs -> linked docs to check for content drift:\n");
        for d in &edited_docs {
            let mut links: BTreeSet<&DocId> = BTreeSet::new();
            links.extend(d.depends_on.iter());
            links.extend(d.related.iter());
            if let Some(r) = graph.reverse.get(&d.id) {
                links.extend(r.iter());
            }
            if let Some(r) = graph.related_reverse.get(&d.id) {
                links.extend(r.iter());
            }
            links.remove(&d.id);
            let list = if links.is_empty() {
                "(none)".to_owned()
            } else {
                links
                    .iter()
                    .map(|l| l.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            };
            msg.push_str(&format!("  {}  ->  {list}\n", d.id));
        }
    }
    msg.push_str(
        "\nIf this turn changed observable behaviour, update the docs of record above to match before finishing. Internal-only changes (refactor/bugfix/perf) need no doc update.",
    );
    Some(msg)
}

fn emit_stop_context(text: &str, note: Option<&str>) {
    let v = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "Stop",
            "additionalContext": append_note(text, note),
        }
    });
    println!("{v}");
}

fn emit_stop_block(reason: &str, note: Option<&str>) {
    let v = serde_json::json!({
        "decision": "block",
        "reason": append_note(reason, note),
    });
    println!("{v}");
}

fn append_note(text: &str, note: Option<&str>) -> String {
    match note {
        Some(n) => format!("{text}\n\n{n}"),
        None => text.to_owned(),
    }
}

fn print_list<T: std::fmt::Display>(label: &str, xs: &[T]) {
    if xs.is_empty() {
        return;
    }
    println!("{label}");
    for x in xs {
        println!("  - {x}");
    }
}

fn print_id_line(graph: &Graph, id: &str, indent: &str) {
    if let Some(doc_id) = graph.id_to_doc.get(id) {
        if let Some(doc) = graph.docs.get(doc_id) {
            let title = doc.title.as_deref().unwrap_or("");
            println!("{indent}{id:<40} {title}");
            return;
        }
    }
    println!("{indent}{id}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn module_covers_exact() {
        assert!(module_covers("a/b.rs", "a/b.rs"));
        assert!(!module_covers("a/b.rs", "a/c.rs"));
    }

    #[test]
    fn module_covers_dir_prefix() {
        assert!(module_covers("a/b/", "a/b/c.rs"));
        assert!(module_covers("a/b/", "a/b/d/e.rs"));
        assert!(!module_covers("a/b/", "a/bc/x.rs"));
        assert!(!module_covers("a/b/", "a/b"));
    }

    #[test]
    fn normalize_separators_passthrough_for_unix() {
        assert_eq!(normalize_separators("a/b/c.rs"), "a/b/c.rs");
        assert_eq!(normalize_separators(""), "");
    }

    #[test]
    fn normalize_separators_rewrites_windows() {
        assert_eq!(normalize_separators("a\\b\\c.rs"), "a/b/c.rs");
        assert_eq!(normalize_separators("a/b\\c.rs"), "a/b/c.rs");
    }

    #[test]
    fn module_covers_after_normalization() {
        let pat = normalize_separators("src\\auth\\");
        let file = normalize_separators("src\\auth\\session.rs");
        assert!(module_covers(&pat, &file));
    }

    #[test]
    fn fenced_yaml_extracted() {
        let content = "before\n```yaml\nfoo: 1\nbar: [x]\n```\nafter\n";
        let yaml = extract_fenced_yaml(content).unwrap();
        assert_eq!(yaml, "foo: 1\nbar: [x]\n");
    }

    #[test]
    fn frontmatter_extracted() {
        let content = "---\na: 1\n---\nbody\n";
        assert_eq!(extract_frontmatter(content).unwrap(), "a: 1");
    }

    #[test]
    fn frontmatter_absent() {
        assert!(extract_frontmatter("# title only\n").is_none());
    }

    #[test]
    fn glob_root_literal_path() {
        assert_eq!(
            glob_root(".kiro/steering/roadmap.md"),
            PathBuf::from(".kiro/steering")
        );
    }

    #[test]
    fn glob_root_with_meta_in_middle() {
        assert_eq!(
            glob_root(".kiro/specs/*/brief.md"),
            PathBuf::from(".kiro/specs")
        );
        assert_eq!(glob_root("docs/fr/[0-9]*.md"), PathBuf::from("docs/fr"));
        assert_eq!(glob_root("crates/*/README.md"), PathBuf::from("crates"));
    }

    #[test]
    fn glob_root_no_dir() {
        assert_eq!(glob_root("*.md"), PathBuf::from("."));
        assert_eq!(glob_root("README.md"), PathBuf::from(""));
    }

    #[test]
    fn glob_root_double_star() {
        assert_eq!(glob_root("docs/**/*.md"), PathBuf::from("docs"));
    }

    #[test]
    fn html_meta_happy_path() {
        let raw = "<head>\n<script type=\"application/kusara+yaml\">\nrefs:\n  id: spec:x\n</script>\n</head>\n";
        let HtmlMeta::Found(y) = extract_html_metadata(raw) else {
            panic!("expected Found");
        };
        assert_eq!(y, "\nrefs:\n  id: spec:x\n");
    }

    #[test]
    fn html_meta_type_among_other_attrs() {
        let raw =
            "<script id=\"m\" type=\"application/kusara+yaml\" defer>\nrefs:\n  id: a\n</script>";
        assert!(matches!(extract_html_metadata(raw), HtmlMeta::Found(_)));
    }

    #[test]
    fn html_meta_single_quotes() {
        let raw = "<script type='application/kusara+yaml'>\nrefs:\n  id: a\n</script>";
        assert!(matches!(extract_html_metadata(raw), HtmlMeta::Found(_)));
    }

    #[test]
    fn html_meta_case_insensitive() {
        let raw = "<SCRIPT TYPE=\"application/kusara+yaml\">\nrefs:\n  id: a\n</SCRIPT>";
        let HtmlMeta::Found(y) = extract_html_metadata(raw) else {
            panic!("expected Found");
        };
        assert_eq!(y, "\nrefs:\n  id: a\n");
    }

    #[test]
    fn html_meta_absent_returns_none() {
        let raw = "<html><body>no metadata here</body></html>";
        assert!(matches!(extract_html_metadata(raw), HtmlMeta::None));
    }

    #[test]
    fn html_meta_unterminated_returns_unterminated() {
        let raw = "<head>\n<script type=\"application/kusara+yaml\">\nrefs:\n  id: a\n</head>\n";
        assert!(matches!(extract_html_metadata(raw), HtmlMeta::Unterminated));
    }

    #[test]
    fn html_meta_first_block_wins() {
        let raw = concat!(
            "<script type=\"application/kusara+yaml\">\nfirst\n</script>",
            "<script type=\"application/kusara+yaml\">\nsecond\n</script>",
        );
        let HtmlMeta::Found(y) = extract_html_metadata(raw) else {
            panic!("expected Found");
        };
        assert_eq!(y, "\nfirst\n");
    }

    #[test]
    fn html_meta_skips_non_matching_script_type() {
        let raw = concat!(
            "<script type=\"text/javascript\">var x = 1;</script>",
            "<script type=\"application/kusara+yaml\">\nrefs:\n  id: a\n</script>",
        );
        let HtmlMeta::Found(y) = extract_html_metadata(raw) else {
            panic!("expected Found");
        };
        assert_eq!(y, "\nrefs:\n  id: a\n");
    }

    #[test]
    fn html_meta_tag_boundary_not_fooled_by_prefix() {
        // `<scriptx` must not be treated as `<script`.
        let raw = "<scriptx type=\"application/kusara+yaml\">nope</scriptx>";
        assert!(matches!(extract_html_metadata(raw), HtmlMeta::None));
    }
}
