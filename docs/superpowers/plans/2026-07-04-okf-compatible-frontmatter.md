# OKF-Compatible Frontmatter Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make kusara's Markdown/HTML frontmatter a valid OKF v0.1 concept document by hoisting OKF reserved keys to the top level and nesting kusara's typed graph under a `kusara:` key, while dual-reading the legacy `refs:` shape and shipping a `kusara migrate` command.

**Architecture:** All changes live at the frontmatter deserialization boundary in `src/main.rs`. A shape-detecting `FrontMatter` normalizes either shape into the existing internal `RefsBlock` + a new `OkfMeta`, so the downstream `Doc`/`Graph`/command engine is untouched in behavior. New OKF fields are carried onto `Doc` and surfaced in `show`/JSON. Emitters (`kusara index`) switch to the new shape; a new `migrate` subcommand rewrites legacy docs.

**Tech Stack:** Rust, `clap` (derive), `serde` + `serde_yaml_ng`, `walkdir`. Tests: `assert_cmd` + `predicates` + `tempfile` integration tests in `tests/integration.rs`.

## Global Constraints

- Single source file: all Rust changes go in `src/main.rs` (the project is one binary crate; follow its existing hand-written-YAML-emitter and integration-test conventions).
- No new dependencies. Current deps only: `anyhow`, `clap`, `glob`, `serde`, `serde_json`, `serde_yaml_ng`, `walkdir`.
- The bridge field is `type` (== kusara kind slug), validated against `docs/kinds.md`. `title` is the only other shared top-level field.
- kusara graph fields live under `kusara:`: `id` (required), `implements`, `depends_on`, `related`, `provides`, `modules`, `spec`, `generated`, `indexes_kind`.
- OKF reserved keys (flat, all optional, never validated for content): `description`, `resource`, `tags`, `timestamp`.
- Unknown keys **inside `kusara:`** are rejected; unknown keys **at top level** are tolerated (OKF forward-compat).
- A doc containing BOTH `refs:` and `type:`/`kusara:` is an error (ambiguous shape).
- `generated` / `indexes_kind` are machine-written only.
- Commit style: conventional commits (`feat:`, `test:`, `docs:`, `refactor:`). Do not commit on `main` — this work happens on branch `docs/okf-compatible-frontmatter-spec` or a feature branch off it.
- Run `cargo test` and `cargo clippy` before each commit; both must pass.

## File Structure

- `src/main.rs` — Modify. New deserialization structs (`KusaraBlock`, extended `FrontMatter`), `OkfMeta`, `FrontMatter::normalize`, extended `Doc`, extended `JsonDoc`/`cmd_show`, rewritten index emitters, new `Cmd::Migrate` + `cmd_migrate` + `emit_okf_frontmatter` + splice helpers, deprecation `eprintln!`.
- `tests/integration.rs` — Modify. Add tests per task using the existing `fixture()`/`write()`/`ks()` helpers.
- `docs/refs.md` — Modify (Task 6). Canonical schema rewrite + legacy appendix.
- `docs/kinds.md` — Modify (Task 6). Note `kind`→`type` rename.
- `README.md` — Modify (Task 6). Example frontmatter.
- kusara skills `refs-schema`, `kinds-manifest` (in the plugin cache) — Modify (Task 6) if the repo vendors them; otherwise note for follow-up.

---

### Task 1: Dual-read parsing + OKF fields on `Doc`

Introduce the OKF shape, detect it vs. legacy `refs:`, normalize both into the existing `RefsBlock`, and carry the new OKF fields onto `Doc`. This is the core of the feature.

**Files:**
- Modify: `src/main.rs:261-290` (FrontMatter/RefsBlock), `src/main.rs:292-304` (Doc), `src/main.rs:510-568` (parse + construct in `build_graph`)
- Test: `tests/integration.rs`

**Interfaces:**
- Produces:
  - `struct KusaraBlock` (deny_unknown_fields) — the graph layer minus `kind`/`title`.
  - `struct OkfMeta { description: Option<String>, resource: Option<String>, tags: Vec<String>, timestamp: Option<String> }`
  - `fn FrontMatter::normalize(self) -> Result<Option<(RefsBlock, OkfMeta)>, String>` — `Ok(None)` = no kusara metadata; `Err(String)` = ambiguous/missing-required.
  - `Doc` gains fields: `description: Option<String>`, `resource: Option<String>`, `tags: Vec<String>`, `timestamp: Option<String>`.

- [ ] **Step 1: Write the failing tests**

Add to `tests/integration.rs`:

```rust
// ---------------------------------------------------------------------------
// OKF-shape frontmatter (dual-read)
// ---------------------------------------------------------------------------

const OKF_DOC: &str = "---\n\
type: spec\n\
title: \"Auth\"\n\
description: \"Authentication design\"\n\
tags: [auth, security]\n\
kusara:\n\
  id: spec:auth\n\
---\n\n# Auth\n";

#[test]
fn okf_shape_validates() {
    let dir = fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/specs/auth.md", OKF_DOC);
    ks(dir.path()).arg("validate").assert().success();
}

#[test]
fn okf_shape_resolves_refs_like_legacy() {
    let dir = fixture(MIN_KINDS_MD);
    // Legacy upstream, OKF downstream that depends on it: no dangling => success.
    write(
        dir.path(),
        "docs/specs/up.md",
        "---\nrefs:\n  id: spec:up\n  kind: spec\n---\n# up\n",
    );
    write(
        dir.path(),
        "docs/specs/down.md",
        "---\ntype: spec\nkusara:\n  id: spec:down\n  depends_on: [spec:up]\n---\n# down\n",
    );
    ks(dir.path()).arg("validate").assert().success();
    ks(dir.path())
        .args(["show", "spec:down"])
        .assert()
        .success()
        .stdout(predicate::str::contains("kind:     spec"))
        .stdout(predicate::str::contains("depends_on:"));
}

#[test]
fn okf_and_legacy_both_present_is_error() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/x.md",
        "---\ntype: spec\nkusara:\n  id: spec:x\nrefs:\n  id: spec:x\n  kind: spec\n---\n# x\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("ambiguous"));
}

#[test]
fn okf_unknown_key_in_kusara_rejected() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/x.md",
        "---\ntype: spec\nkusara:\n  id: spec:x\n  bogus: 1\n---\n# x\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("front matter parse error"));
}

#[test]
fn okf_top_level_unknown_key_tolerated() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/x.md",
        "---\ntype: spec\ncustom_field: hello\nkusara:\n  id: spec:x\n---\n# x\n",
    );
    ks(dir.path()).arg("validate").assert().success();
}

#[test]
fn okf_missing_id_rejected() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/x.md",
        "---\ntype: spec\nkusara:\n  implements: [spec:up]\n---\n# x\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure();
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration okf_ 2>&1 | tail -20`
Expected: FAIL — the OKF-shape docs currently parse to `fm.refs == None` and are skipped, so `spec:down`/`spec:auth` are unknown and the "both present" doc does not error.

- [ ] **Step 3: Add the OKF deserialization structs**

Replace `src/main.rs:261-290` (the `FrontMatter` and `RefsBlock` definitions) with:

```rust
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
    /// Normalize either shape into `(RefsBlock, OkfMeta)`.
    /// `Ok(None)` = the file carries no kusara metadata (skip it).
    /// `Err` = ambiguous shape or a missing required field.
    fn normalize(self) -> Result<Option<(RefsBlock, OkfMeta)>, String> {
        let has_okf = self.typ.is_some() || self.kusara.is_some();
        match (self.refs, has_okf) {
            (Some(_), true) => {
                Err("ambiguous front matter: both `refs:` and OKF `type:`/`kusara:` present".into())
            }
            (Some(refs), false) => Ok(Some((refs, OkfMeta::default()))),
            (None, false) => Ok(None),
            (None, true) => {
                let kind = self
                    .typ
                    .ok_or("OKF front matter missing required `type:`")?;
                let k = self
                    .kusara
                    .ok_or("OKF front matter missing required `kusara:` block")?;
                let refs = RefsBlock {
                    id: k.id,
                    kind,
                    title: self.title,
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
                    description: self.description,
                    resource: self.resource,
                    tags: self.tags,
                    timestamp: self.timestamp,
                };
                Ok(Some((refs, okf)))
            }
        }
    }
}
```

- [ ] **Step 4: Add OKF fields to `Doc`**

In `src/main.rs:292-304`, add four fields to `struct Doc` (after `modules`):

```rust
    modules: Vec<String>,
    description: Option<String>,
    resource: Option<String>,
    tags: Vec<String>,
    timestamp: Option<String>,
}
```

- [ ] **Step 5: Wire normalization into `build_graph`**

In `src/main.rs`, replace the block at lines 517-519:

```rust
            let Some(refs_block) = fm.refs else {
                continue;
            };
```

with:

```rust
            let (refs_block, okf_meta) = match fm.normalize() {
                Ok(Some(v)) => v,
                Ok(None) => continue,
                Err(msg) => {
                    errors.push(format!("{}: {msg}", rel.display()));
                    continue;
                }
            };
```

Then, in the `Doc { .. }` construction at lines 557-568, add the four OKF fields at the end:

```rust
                modules: refs_block.modules,
                description: okf_meta.description,
                resource: okf_meta.resource,
                tags: okf_meta.tags,
                timestamp: okf_meta.timestamp,
            };
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test integration okf_ 2>&1 | tail -20`
Expected: PASS (all six `okf_*` tests). Then `cargo test` — full suite green (legacy behavior unchanged).

- [ ] **Step 7: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/main.rs tests/integration.rs
git commit -m "feat: dual-read OKF-shaped and legacy refs frontmatter"
```

---

### Task 2: Surface OKF fields in `show` and JSON graph

Make the new OKF fields visible so they are not silently dropped.

**Files:**
- Modify: `src/main.rs:945-962` (`cmd_show`), `src/main.rs:1188-1225` (`JsonDoc` + `build_graph_json`)
- Test: `tests/integration.rs`

**Interfaces:**
- Consumes: `Doc.description/resource/tags/timestamp` from Task 1.
- Produces: `JsonDoc` gains `description`, `resource`, `tags`, `timestamp` (all skipped when empty/none).

- [ ] **Step 1: Write the failing test**

Add to `tests/integration.rs`:

```rust
#[test]
fn okf_fields_surface_in_show() {
    let dir = fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/specs/auth.md", OKF_DOC); // has description + tags
    ks(dir.path())
        .args(["show", "spec:auth"])
        .assert()
        .success()
        .stdout(predicate::str::contains("description: Authentication design"))
        .stdout(predicate::str::contains("tags:     auth, security"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration okf_fields_surface_in_show -v`
Expected: FAIL — `show` does not print `description`/`tags` yet.

- [ ] **Step 3: Print OKF fields in `cmd_show`**

In `src/main.rs`, immediately after the `title` block (after line 961 `}`), before `println!("path: ...")`, insert:

```rust
    if let Some(d) = &doc.description {
        println!("description: {d}");
    }
    if let Some(r) = &doc.resource {
        println!("resource: {r}");
    }
    if !doc.tags.is_empty() {
        println!("tags:     {}", doc.tags.join(", "));
    }
    if let Some(ts) = &doc.timestamp {
        println!("timestamp: {ts}");
    }
```

- [ ] **Step 4: Add OKF fields to JSON output**

In `struct JsonDoc` (`src/main.rs:1188-1206`), add after `modules`:

```rust
    modules: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    resource: Option<&'a String>,
    #[serde(skip_serializing_if = "<[String]>::is_empty")]
    tags: &'a [String],
    #[serde(skip_serializing_if = "Option::is_none")]
    timestamp: Option<&'a String>,
}
```

And in `build_graph_json`'s `JsonDoc { .. }` literal (`src/main.rs:1212-1225`), add after `modules: &d.modules,`:

```rust
            modules: &d.modules,
            description: d.description.as_ref(),
            resource: d.resource.as_ref(),
            tags: &d.tags,
            timestamp: d.timestamp.as_ref(),
```

Note: `<[String]>::is_empty` is called by serde as `f(&&[String])` → `&[String]` derefs to `[String]`; if clippy/compiler rejects the path, define a free helper `fn slice_is_empty<T>(s: &&[T]) -> bool { s.is_empty() }` and use `skip_serializing_if = "slice_is_empty"`.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test integration okf_fields_surface_in_show -v` → PASS
Run: `cargo test` → full suite green.

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/main.rs tests/integration.rs
git commit -m "feat: surface OKF reserved fields in show and graph json"
```

---

### Task 3: Emit the new shape from `kusara index`

Generated INDEX docs must be written in the OKF shape (so `kusara index` output is itself OKF-native and round-trips through the Task 1 reader).

**Files:**
- Modify: `src/main.rs:1258-1264` (per-kind index frontmatter), and the equivalent frontmatter emission in `write_map` (`src/main.rs:1099-1168` — locate its `out.push_str("---\nrefs:\n"...)` block).
- Test: `tests/integration.rs`

**Interfaces:** none new; changes literal emitted text only.

- [ ] **Step 1: Write the failing test**

Add to `tests/integration.rs`:

```rust
#[test]
fn generated_index_uses_okf_shape() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\ntype: spec\nkusara:\n  id: spec:a\n---\n# a\n",
    );
    ks(dir.path()).arg("index").assert().success();
    let idx = fs::read_to_string(dir.path().join("docs/specs/index.md")).unwrap();
    assert!(idx.contains("type: index"), "index frontmatter: {idx}");
    assert!(idx.contains("kusara:"), "index frontmatter: {idx}");
    assert!(!idx.contains("refs:"), "index must not use legacy shape: {idx}");
    // The generated index must itself validate (reader round-trips its output).
    ks(dir.path()).arg("validate").assert().success();
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration generated_index_uses_okf_shape -v`
Expected: FAIL — emitter still writes `refs:`.

- [ ] **Step 3: Rewrite the per-kind index frontmatter**

Replace `src/main.rs:1258-1264`:

```rust
        out.push_str("---\nrefs:\n");
        out.push_str(&format!("  id: index:{}\n", kind.name));
        out.push_str("  kind: index\n");
        out.push_str(&format!("  indexes_kind: {}\n", kind.name));
        out.push_str("  generated: true\n");
        out.push_str(&format!("  title: \"{title}\"\n"));
        out.push_str("---\n\n");
```

with:

```rust
        out.push_str("---\n");
        out.push_str("type: index\n");
        out.push_str(&format!("title: \"{title}\"\n"));
        out.push_str("kusara:\n");
        out.push_str(&format!("  id: index:{}\n", kind.name));
        out.push_str(&format!("  indexes_kind: {}\n", kind.name));
        out.push_str("  generated: true\n");
        out.push_str("---\n\n");
```

- [ ] **Step 4: Rewrite the global map frontmatter**

In `write_map` (`src/main.rs:1099-1168`), find the analogous `out.push_str("---\nrefs:\n")` block that emits the `map.md` frontmatter (global index: `id: index:map` or similar with `generated: true` and no `indexes_kind`). Replace it with the flat+`kusara:` form:

```rust
        out.push_str("---\n");
        out.push_str("type: index\n");
        out.push_str(&format!("title: \"{title}\"\n"));
        out.push_str("kusara:\n");
        out.push_str(&format!("  id: {id}\n"));   // whatever id the global map uses today
        out.push_str("  generated: true\n");
        out.push_str("---\n\n");
```

(Match the exact `id`/`title` variables already in scope there; only the shape changes, not the values. A global index has `generated: true` and no `indexes_kind`.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --test integration generated_index_uses_okf_shape -v` → PASS
Run: `cargo test` → full suite green (fix any existing index tests that asserted on the old `refs:` text by updating their expected strings to the new shape).

- [ ] **Step 6: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/main.rs tests/integration.rs
git commit -m "feat: emit OKF-shaped frontmatter from kusara index"
```

---

### Task 4: Deprecation warning for legacy `refs:` docs

Warn (without failing) when a legacy-shape doc is read, pointing at `kusara migrate`.

**Files:**
- Modify: `src/main.rs` — the legacy branch of `FrontMatter::normalize` cannot print (it lacks the path); emit the warning at the call site in `build_graph` where `rel` is known.
- Test: `tests/integration.rs`

**Interfaces:** none new.

- [ ] **Step 1: Write the failing test**

Add to `tests/integration.rs`:

```rust
#[test]
fn legacy_refs_emits_deprecation_warning() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\nrefs:\n  id: spec:a\n  kind: spec\n---\n# a\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .success() // legacy still valid...
        .stderr(predicate::str::contains("deprecated"))
        .stderr(predicate::str::contains("kusara migrate"));
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test --test integration legacy_refs_emits_deprecation_warning -v`
Expected: FAIL — no warning is printed.

- [ ] **Step 3: Distinguish legacy vs OKF at the call site**

Change `FrontMatter::normalize`'s success return to also report which shape was used. Update its signature to return `Result<Option<(RefsBlock, OkfMeta, bool)>, String>` where the `bool` is `is_legacy`. In the `(Some(refs), false)` arm return `Ok(Some((refs, OkfMeta::default(), true)))`; in the OKF arm return `Ok(Some((refs, okf, false)))`.

Then in `build_graph` update the match added in Task 1:

```rust
            let (refs_block, okf_meta, is_legacy) = match fm.normalize() {
                Ok(Some(v)) => v,
                Ok(None) => continue,
                Err(msg) => {
                    errors.push(format!("{}: {msg}", rel.display()));
                    continue;
                }
            };
            if is_legacy {
                eprintln!(
                    "warning: {}: legacy `refs:` front matter is deprecated; run `kusara migrate`",
                    rel.display()
                );
            }
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test --test integration legacy_refs_emits_deprecation_warning -v` → PASS
Run: `cargo test` → green. (If any existing legacy-shape fixtures now emit warnings that a test asserts stderr is empty, relax those assertions.)

- [ ] **Step 5: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/main.rs tests/integration.rs
git commit -m "feat: warn on deprecated legacy refs frontmatter"
```

---

### Task 5: `kusara migrate` command

Rewrite legacy `refs:` docs into the OKF shape, in place, idempotently, for Markdown and HTML. Add `--dry-run`.

**Files:**
- Modify: `src/main.rs:29-66` (add `Cmd::Migrate`), `src/main.rs:396-410` (dispatch), and add `cmd_migrate` + `emit_okf_frontmatter` + splice helpers near the other `cmd_*` functions.
- Test: `tests/integration.rs`

**Interfaces:**
- Consumes: `extract_frontmatter` (`main.rs:650`), `extract_html_metadata`/`HtmlMeta` (`main.rs:677`), `FrontMatter::normalize` (Task 1), `derive_scan_roots` (`main.rs:620`), `RefsBlock`, `OkfMeta`.
- Produces: `Cmd::Migrate { dry_run: bool }`; `fn cmd_migrate(root, doc_root, manifest, dry_run) -> Result<ExitCode>`; `fn emit_okf_frontmatter(rb: &RefsBlock, okf: &OkfMeta) -> String` (returns the YAML body between the fences, ending in `\n`); `fn splice_subslice(raw, inner, new_inner) -> String`.

- [ ] **Step 1: Write the failing tests**

Add to `tests/integration.rs`:

```rust
#[test]
fn migrate_rewrites_legacy_markdown() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\nrefs:\n  id: spec:a\n  kind: spec\n  title: \"A\"\n  depends_on: [spec:b]\n---\n# body\n",
    );
    write(
        dir.path(),
        "docs/specs/b.md",
        "---\nrefs:\n  id: spec:b\n  kind: spec\n---\n# b\n",
    );
    ks(dir.path()).arg("migrate").assert().success();
    let a = fs::read_to_string(dir.path().join("docs/specs/a.md")).unwrap();
    assert!(a.contains("type: spec"), "{a}");
    assert!(a.contains("kusara:"), "{a}");
    assert!(a.contains("id: spec:a"), "{a}");
    assert!(a.contains("depends_on:"), "{a}");
    assert!(!a.contains("refs:"), "{a}");
    assert!(a.contains("# body"), "body preserved: {a}");
    ks(dir.path()).arg("validate").assert().success();
}

#[test]
fn migrate_is_idempotent() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\nrefs:\n  id: spec:a\n  kind: spec\n---\n# a\n",
    );
    ks(dir.path()).arg("migrate").assert().success();
    let once = fs::read_to_string(dir.path().join("docs/specs/a.md")).unwrap();
    ks(dir.path()).arg("migrate").assert().success();
    let twice = fs::read_to_string(dir.path().join("docs/specs/a.md")).unwrap();
    assert_eq!(once, twice, "second migrate must be a no-op");
}

#[test]
fn migrate_dry_run_writes_nothing() {
    let dir = fixture(MIN_KINDS_MD);
    let original = "---\nrefs:\n  id: spec:a\n  kind: spec\n---\n# a\n";
    write(dir.path(), "docs/specs/a.md", original);
    ks(dir.path())
        .args(["migrate", "--dry-run"])
        .assert()
        .success();
    let after = fs::read_to_string(dir.path().join("docs/specs/a.md")).unwrap();
    assert_eq!(after, original, "--dry-run must not modify files");
}

#[test]
fn migrate_rewrites_legacy_html() {
    let dir = fixture(HTML_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.html",
        "<html><head><script type=\"application/kusara+yaml\">\nrefs:\n  id: spec:a\n  kind: spec\n</script></head><body>x</body></html>\n",
    );
    ks(dir.path()).arg("migrate").assert().success();
    let a = fs::read_to_string(dir.path().join("docs/specs/a.html")).unwrap();
    assert!(a.contains("type: spec"), "{a}");
    assert!(a.contains("kusara:"), "{a}");
    assert!(!a.contains("refs:"), "{a}");
    assert!(a.contains("<body>x</body>"), "html body preserved: {a}");
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --test integration migrate_ 2>&1 | tail -20`
Expected: FAIL — `migrate` subcommand does not exist (clap errors).

- [ ] **Step 3: Add the `Migrate` subcommand variant**

In `enum Cmd` (`src/main.rs:29-66`), add:

```rust
    /// Rewrite legacy `refs:` front matter into the OKF-native shape in place.
    Migrate {
        /// Print which files would change without writing them.
        #[arg(long)]
        dry_run: bool,
    },
```

- [ ] **Step 4: Dispatch to `cmd_migrate`**

In the dispatch match (`src/main.rs:396-410`), add an arm. `migrate` needs the manifest but builds its own file walk (it must see files regardless of parse state), so route it before the `graph` is required, or simply:

```rust
        Cmd::Migrate { dry_run } => cmd_migrate(root, doc_root, &manifest, *dry_run),
```

- [ ] **Step 5: Implement `cmd_migrate` and helpers**

Add near the other `cmd_*` functions:

```rust
/// Serialize the OKF-shaped front matter body (between the `---` fences) for a
/// normalized doc. Deterministic field order. Ends with a trailing newline.
fn emit_okf_frontmatter(rb: &RefsBlock, okf: &OkfMeta) -> String {
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

fn cmd_migrate(root: &Path, doc_root: &Path, manifest: &Manifest, dry_run: bool) -> Result<ExitCode> {
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
            // Only legacy docs need migration.
            let Some(refs_block) = fm.refs else {
                continue;
            };
            let okf = OkfMeta::default();
            let body = emit_okf_frontmatter(&refs_block, &okf);
            let new_yaml = match format {
                // Markdown yaml has no surrounding newlines inside the fences;
                // `body` already ends with `\n`, matching `extract_frontmatter`
                // (which excludes the trailing `\n` before `---`). Trim one.
                DocFormat::Markdown => body.trim_end_matches('\n').to_string(),
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
```

Note on `--dry-run`: this prints one `would migrate <path>` line per legacy doc rather than a full unified diff (kusara ships no diff dependency, and the Global Constraints forbid adding one). This is a deliberate, documented deviation from the spec's "unified diff" wording; update the spec's migrate section to match in Task 6.

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test --test integration migrate_ 2>&1 | tail -20` → all PASS.
Run: `cargo test` → full suite green.

Manual check of Markdown fence handling: after migrate, `docs/specs/a.md` must start with `---\ntype: spec\n...` and the closing `---` must sit on its own line before `# body`. If `extract_frontmatter`'s excluded-trailing-newline assumption produces a doubled or missing newline around the closing `---`, adjust the `DocFormat::Markdown` arm's trim/`\n` handling until `migrate` then `validate` both pass on a multi-line-body fixture.

- [ ] **Step 7: Lint and commit**

```bash
cargo clippy --all-targets -- -D warnings
git add src/main.rs tests/integration.rs
git commit -m "feat: add kusara migrate to rewrite legacy frontmatter"
```

---

### Task 6: Documentation and schema updates

Bring the canonical docs, README, and vendored skills in line with the OKF-native shape.

**Files:**
- Modify: `docs/refs.md`, `docs/kinds.md`, `README.md`, and the committed design spec `docs/superpowers/specs/2026-07-04-okf-compatible-frontmatter-design.md` (dry-run note).
- Modify (if vendored in-repo): the `refs-schema` / `kinds-manifest` skill sources.

**Interfaces:** none (documentation only).

- [ ] **Step 1: Rewrite `docs/refs.md` to the OKF-native shape**

Replace the canonical example and the `refs:` shape section with the flat + `kusara:` shape (mirror the "Canonical frontmatter shape" and field-ownership table from the design spec). Add a short appendix titled "Legacy `refs:` (deprecated)" showing the old shape and pointing at `kusara migrate`. Update the "Forbidden hand-edits" section to reference `kusara.generated` / `kusara.indexes_kind`.

- [ ] **Step 2: Note the rename in `docs/kinds.md`**

Add a sentence: the front matter field that selects a kind is `type:` (formerly `kind:`); it is still validated against the kinds listed here.

- [ ] **Step 3: Update `README.md` examples**

Update any `refs:`-shaped frontmatter example in `README.md` to the new shape. Add `kusara migrate` to the command list with a one-line description.

- [ ] **Step 4: Reconcile the design spec's dry-run wording**

In `docs/superpowers/specs/2026-07-04-okf-compatible-frontmatter-design.md`, change the `--dry-run` description from "prints a unified diff" to "lists the files that would change" to match the shipped behavior.

- [ ] **Step 5: Update vendored skills if present**

Check whether `refs-schema`/`kinds-manifest` skill bodies live in this repo (e.g. under a `skills/` or plugin directory). If so, update their schema bodies and `references/` copies to the OKF-native shape. If they are only in the external plugin cache, note in the commit message that the published skills need a follow-up release.

- [ ] **Step 6: Verify docs match behavior**

Run: `cargo test` (ensures examples that are also fixtures still parse). Manually skim `docs/refs.md` against `kusara show` output for a sample OKF doc.

- [ ] **Step 7: Commit**

```bash
git add docs/refs.md docs/kinds.md README.md docs/superpowers/specs/2026-07-04-okf-compatible-frontmatter-design.md
git commit -m "docs: document OKF-native frontmatter and migrate command"
```

---

## Self-Review

**Spec coverage:**
- Native flat shape + `kusara:` namespace → Task 1 (structs), Task 3 (emitters), Task 6 (docs). ✓
- Keep explicit `id:` under `kusara:` → Task 1 (`KusaraBlock.id` required). ✓
- `kind` → `type` bridge, validated vs kinds.md → Task 1 (`typ: Option<Kind>`, reused `manifest.knows` check). ✓
- New OKF reserved keys, unvalidated, surfaced → Task 1 (`OkfMeta`, `Doc` fields) + Task 2 (`show`/JSON). ✓
- Dual-read both shapes; both-present error; kusara-namespace unknown-key rejection; top-level tolerance → Task 1 (`normalize`, `deny_unknown_fields`). ✓
- Transitional deprecation warning → Task 4. ✓
- `kusara migrate` (md + html, idempotent, dry-run) → Task 5. ✓
- Docs/skills/README → Task 6. ✓
- Non-goals (no bundles, no path IDs, no reserved filenames, no content validation of resource/tags/timestamp) → respected; nothing in the tasks implements them. ✓

**Placeholder scan:** No TBD/TODO. The one soft spot — the exact `id`/`title` variables in `write_map` (Task 3 Step 4) — is called out explicitly with what to match, because the surrounding code was not fully quoted here; the implementer must read `write_map` (`src/main.rs:1099-1168`) and preserve its existing values while changing only the shape.

**Type consistency:** `FrontMatter::normalize` returns `Result<Option<(RefsBlock, OkfMeta, bool)>, String>` after Task 4; Task 1 introduces it as `(RefsBlock, OkfMeta)` and Task 4 explicitly widens it to add the `is_legacy` bool and updates the call site. `OkfMeta` fields (`description`/`resource`/`tags`/`timestamp`) are consistent across Doc, JsonDoc, `cmd_show`, and `emit_okf_frontmatter`. `KusaraBlock` (deny_unknown_fields, read side) and `KusaraOut` (skip-empty, write side) are intentionally distinct structs. ✓
