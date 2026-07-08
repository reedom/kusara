use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const MIN_KINDS_MD: &str = "# kinds\n\n```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.md\"]\n    id_pattern: \"spec:{slug}\"\n    index:\n      output: docs/specs/index.md\n  - name: ref\n    path_globs: [\"docs/ref/[a-z]*.md\"]\n    id_pattern: \"ref:{slug}\"\n  - name: req\n    declared_via: provides\n    id_pattern: \"req:{spec}:{n}\"\n  - name: index\n    declared_via: generated\n    id_pattern: \"index:{kind}\"\n```\n";

const HTML_KINDS_MD: &str = "# kinds\n\n```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.html\"]\n    id_pattern: \"spec:{slug}\"\n  - name: req\n    declared_via: provides\n    id_pattern: \"req:{spec}:{n}\"\n  - name: index\n    declared_via: generated\n    id_pattern: \"index:{kind}\"\n```\n";

const MIXED_KINDS_MD: &str = "# kinds\n\n```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.md\", \"docs/specs/*.html\"]\n    id_pattern: \"spec:{slug}\"\n  - name: req\n    declared_via: provides\n    id_pattern: \"req:{spec}:{n}\"\n  - name: index\n    declared_via: generated\n    id_pattern: \"index:{kind}\"\n```\n";

fn fixture(kinds_md: &str) -> TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    write(dir.path(), "docs/kinds.md", kinds_md);
    dir
}

fn write(root: &Path, rel: &str, body: &str) {
    let path = root.join(rel);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).expect("create_dir_all");
    }
    fs::write(&path, body).expect("write");
}

fn ks(root: &Path) -> Command {
    let mut cmd = Command::cargo_bin("kusara").expect("bin");
    cmd.arg("--root").arg(root).env_remove("KUSARA_DOC_ROOT");
    cmd
}

// ---------------------------------------------------------------------------
// Manifest loading
// ---------------------------------------------------------------------------

#[test]
fn manifest_missing_fails_with_context() {
    let dir = tempfile::tempdir().unwrap();
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("kinds.md"));
}

#[test]
fn manifest_no_yaml_fence_fails() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "docs/kinds.md", "# no fenced yaml here\n");
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no ```yaml"));
}

#[test]
fn manifest_duplicate_kind_fails() {
    let dir = fixture(
        "```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.md\"]\n  - name: spec\n    path_globs: [\"docs/other/*.md\"]\n```\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate kind"));
}

#[test]
fn manifest_missing_globs_and_declared_via_fails() {
    let dir = fixture("```yaml\nkinds:\n  - name: orphan\n    id_pattern: \"orphan\"\n```\n");
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "neither `path_globs` nor `declared_via`",
        ));
}

#[test]
fn manifest_unknown_declared_via_fails() {
    let dir = fixture(
        "```yaml\nkinds:\n  - name: bogus\n    declared_via: providess\n    id_pattern: \"bogus\"\n```\n",
    );
    ks(dir.path()).arg("validate").assert().failure();
}

// ---------------------------------------------------------------------------
// Validate
// ---------------------------------------------------------------------------

#[test]
fn validate_clean_repo() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.md",
        "---\nrefs:\n  id: spec:foo\n  kind: spec\n  spec: foo\n  title: Foo\n---\n\n# Foo\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK (1 docs)"));
}

#[test]
fn validate_clean_html_spec() {
    let dir = fixture(HTML_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.html",
        "<head>\n<script type=\"application/kusara+yaml\">\nrefs:\n  id: spec:foo\n  kind: spec\n  title: Foo\n</script>\n</head>\n<body>Foo</body>\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK (1 docs)"));
}

#[test]
fn html_spec_shows_metadata() {
    let dir = fixture(HTML_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.html",
        "<head>\n<script type=\"application/kusara+yaml\">\nrefs:\n  id: spec:foo\n  kind: spec\n  title: Foo\n</script>\n</head>\n",
    );
    ks(dir.path())
        .args(["show", "spec:foo"])
        .assert()
        .success()
        .stdout(predicate::str::contains("id:           spec:foo"))
        .stdout(predicate::str::contains("kind:         spec"))
        .stdout(predicate::str::contains(
            "path:         docs/specs/foo.html",
        ));
}

#[test]
fn html_and_markdown_specs_share_one_graph() {
    let dir = fixture(MIXED_KINDS_MD);
    // a.html depends on b.md — a cross-format edge.
    write(
        dir.path(),
        "docs/specs/a.html",
        "<script type=\"application/kusara+yaml\">\nrefs:\n  id: spec:a\n  kind: spec\n  depends_on:\n    - spec:b\n</script>\n",
    );
    write(
        dir.path(),
        "docs/specs/b.md",
        "---\nrefs:\n  id: spec:b\n  kind: spec\n---\n",
    );
    ks(dir.path()).arg("validate").assert().success();
    ks(dir.path())
        .args(["impact", "spec:b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spec:a"));
}

#[test]
fn validate_malformed_html_metadata_reports_error() {
    let dir = fixture(HTML_KINDS_MD);
    // Opening marker tag but no closing </script>.
    write(
        dir.path(),
        "docs/specs/foo.html",
        "<head>\n<script type=\"application/kusara+yaml\">\nrefs:\n  id: spec:foo\n  kind: spec\n</head>\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("missing closing `</script>`"));
}

#[test]
fn validate_strict_glob_coverage_html() {
    let dir = fixture(HTML_KINDS_MD);
    // Matched by the glob but carries no metadata block at all.
    write(
        dir.path(),
        "docs/specs/foo.html",
        "<html><body>no metadata</body></html>\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("has no kusara front matter"));
}

#[test]
fn validate_html_invalid_yaml_reports_parse_error() {
    let dir = fixture(HTML_KINDS_MD);
    // Well-formed block, but the YAML body is invalid (unclosed flow sequence).
    // Must surface the same parse error as malformed Markdown front matter.
    write(
        dir.path(),
        "docs/specs/foo.html",
        "<script type=\"application/kusara+yaml\">\nrefs:\n  id: [unclosed\n</script>\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("front matter parse error"));
}

#[test]
fn validate_dangling_reference() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.md",
        "---\nrefs:\n  id: spec:foo\n  kind: spec\n  depends_on:\n    - spec:does-not-exist\n---\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("dangling reference"))
        .stderr(predicate::str::contains("spec:does-not-exist"));
}

#[test]
fn validate_duplicate_id() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\nrefs:\n  id: spec:dup\n  kind: spec\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/b.md",
        "---\nrefs:\n  id: spec:dup\n  kind: spec\n---\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("duplicate id `spec:dup`"));
}

#[test]
fn validate_unknown_kind() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.md",
        "---\nrefs:\n  id: bogus:foo\n  kind: bogus\n---\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown kind"));
}

#[test]
fn validate_missing_module_path() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.md",
        "---\nrefs:\n  id: spec:foo\n  kind: spec\n  modules:\n    - src/does/not/exist.rs\n---\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not exist"));
}

#[test]
fn validate_strict_glob_coverage() {
    let dir = fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/specs/foo.md", "# no front matter\n");
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("has no kusara front matter"));
}

#[test]
fn validate_malformed_frontmatter_reports_error() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/foo.md",
        "---\nid: spec:foo\nkind: spec\n# missing closing fence\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("malformed front matter"));
}

#[test]
fn validate_provides_collision() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\nrefs:\n  id: spec:a\n  kind: spec\n  provides:\n    - req:a:1\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/b.md",
        "---\nrefs:\n  id: spec:b\n  kind: spec\n  provides:\n    - req:a:1\n---\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("provided by both"));
}

#[test]
fn validate_index_doc_invariant_violation() {
    let dir = fixture(MIN_KINDS_MD);
    // kind=index but generated=false — must fail.
    write(
        dir.path(),
        "docs/ref/bogus-index.md",
        "---\nrefs:\n  id: index:bogus\n  kind: index\n  generated: false\n---\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("must agree"));
}

// ---------------------------------------------------------------------------
// Traversal
// ---------------------------------------------------------------------------

fn dir_with_chain() -> TempDir {
    let dir = fixture(MIN_KINDS_MD);
    // a depends on b depends on c
    write(
        dir.path(),
        "docs/specs/a.md",
        "---\nrefs:\n  id: spec:a\n  kind: spec\n  depends_on:\n    - spec:b\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/b.md",
        "---\nrefs:\n  id: spec:b\n  kind: spec\n  depends_on:\n    - spec:c\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/c.md",
        "---\nrefs:\n  id: spec:c\n  kind: spec\n---\n",
    );
    dir
}

#[test]
fn impact_walks_to_top() {
    let dir = dir_with_chain();
    ks(dir.path())
        .args(["impact", "spec:c"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spec:b"))
        .stdout(predicate::str::contains("spec:a"));
}

#[test]
fn deps_walks_downward() {
    let dir = dir_with_chain();
    ks(dir.path())
        .args(["deps", "spec:a"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spec:b"))
        .stdout(predicate::str::contains("spec:c"));
}

#[test]
fn impact_unknown_id_errors() {
    let dir = dir_with_chain();
    ks(dir.path())
        .args(["impact", "spec:does-not-exist"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown id"));
}

#[test]
fn impact_depth_zero_reports_none() {
    let dir = dir_with_chain();
    ks(dir.path())
        .args(["impact", "spec:c", "--depth", "0"])
        .assert()
        .success()
        .stdout(predicate::str::contains("(none)"));
}

#[test]
fn impact_depth_one_stops_at_first_layer() {
    let dir = dir_with_chain();
    let assert = ks(dir.path())
        .args(["impact", "spec:c", "--depth", "1"])
        .assert()
        .success();
    let out = String::from_utf8_lossy(&assert.get_output().stdout).to_string();
    assert!(out.contains("spec:b"), "expected spec:b in:\n{out}");
    assert!(
        !out.contains("spec:a"),
        "expected spec:a missing in:\n{out}"
    );
}

// ---------------------------------------------------------------------------
// Show / list / touched / index
// ---------------------------------------------------------------------------

#[test]
fn show_prints_doc_metadata() {
    let dir = dir_with_chain();
    ks(dir.path())
        .args(["show", "spec:b"])
        .assert()
        .success()
        .stdout(predicate::str::contains("id:           spec:b"))
        .stdout(predicate::str::contains("kind:         spec"));
}

#[test]
fn show_unknown_id_errors() {
    let dir = dir_with_chain();
    ks(dir.path())
        .args(["show", "nope"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown id"));
}

#[test]
fn list_prints_every_id() {
    let dir = dir_with_chain();
    ks(dir.path())
        .arg("list")
        .assert()
        .success()
        .stdout(predicate::str::contains("spec:a"))
        .stdout(predicate::str::contains("spec:b"))
        .stdout(predicate::str::contains("spec:c"));
}

#[test]
fn touched_finds_doc_of_record_and_closure() {
    let dir = fixture(MIN_KINDS_MD);
    fs::create_dir_all(dir.path().join("src/auth")).unwrap();
    fs::write(dir.path().join("src/auth/session.rs"), "").unwrap();
    write(
        dir.path(),
        "docs/specs/auth.md",
        "---\nrefs:\n  id: spec:auth\n  kind: spec\n  modules:\n    - src/auth/\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/parent.md",
        "---\nrefs:\n  id: spec:parent\n  kind: spec\n  depends_on:\n    - spec:auth\n---\n",
    );
    ks(dir.path())
        .args(["touched", "src/auth/session.rs"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spec:auth"))
        .stdout(predicate::str::contains("Transitively affected"))
        .stdout(predicate::str::contains("spec:parent"));
}

#[test]
fn touched_no_closure_skips_indirect() {
    let dir = fixture(MIN_KINDS_MD);
    fs::create_dir_all(dir.path().join("src/auth")).unwrap();
    fs::write(dir.path().join("src/auth/session.rs"), "").unwrap();
    write(
        dir.path(),
        "docs/specs/auth.md",
        "---\nrefs:\n  id: spec:auth\n  kind: spec\n  modules:\n    - src/auth/\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/parent.md",
        "---\nrefs:\n  id: spec:parent\n  kind: spec\n  depends_on:\n    - spec:auth\n---\n",
    );
    ks(dir.path())
        .args(["touched", "src/auth/session.rs", "--no-closure"])
        .assert()
        .success()
        .stdout(predicate::str::contains("spec:auth"))
        .stdout(predicate::str::contains("Transitively affected").not());
}

#[test]
fn index_map_writes_three_artifacts() {
    let dir = dir_with_chain();
    ks(dir.path()).args(["index", "map"]).assert().success();
    assert!(dir.path().join("docs/map.md").exists());
    assert!(dir.path().join("docs/ai/graph.json").exists());
    assert!(dir.path().join("docs/ai/modules.md").exists());
    let map = fs::read_to_string(dir.path().join("docs/map.md")).unwrap();
    assert!(map.contains("spec:a"), "map.md missing spec:a:\n{map}");
    let json = fs::read_to_string(dir.path().join("docs/ai/graph.json")).unwrap();
    assert!(
        json.contains("\"id\": \"spec:a\""),
        "graph.json missing spec:a id field:\n{json}"
    );
}

#[test]
fn index_default_writes_per_kind_index() {
    let dir = dir_with_chain();
    ks(dir.path())
        .arg("index")
        .assert()
        .success()
        .stdout(predicate::str::contains("specs/index.md"));
    let body = fs::read_to_string(dir.path().join("docs/specs/index.md")).unwrap();
    assert!(body.contains("indexes_kind: spec"));
    assert!(body.contains("spec:a"));
}

#[test]
fn index_then_validate_roundtrips() {
    let dir = dir_with_chain();
    ks(dir.path()).args(["index", "map"]).assert().success();
    ks(dir.path()).arg("index").assert().success();
    ks(dir.path()).arg("validate").assert().success();
}

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
    assert!(
        !idx.contains("refs:"),
        "index must not use legacy shape: {idx}"
    );
    // The generated index must itself validate (reader round-trips its output).
    ks(dir.path()).arg("validate").assert().success();
}

// ---------------------------------------------------------------------------
// Env override
// ---------------------------------------------------------------------------

#[test]
fn kusara_doc_root_env_override_loads_alternate_manifest() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "specs/kinds.md", MIN_KINDS_MD);
    Command::cargo_bin("kusara")
        .unwrap()
        .arg("--root")
        .arg(dir.path())
        .env("KUSARA_DOC_ROOT", "specs")
        .arg("validate")
        .assert()
        .success();
}

#[test]
fn kusara_doc_root_invalid_unicode_bails() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    let dir = fixture(MIN_KINDS_MD);
    let bad = OsString::from_vec(vec![0xff, 0xfe]);
    Command::cargo_bin("kusara")
        .unwrap()
        .arg("--root")
        .arg(dir.path())
        .env("KUSARA_DOC_ROOT", bad)
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("KUSARA_DOC_ROOT"));
}

// ---------------------------------------------------------------------------
// OKF-shape frontmatter (dual-read)
// ---------------------------------------------------------------------------

// NOTE: written as a single-line literal (not split across source lines with
// `\` continuations) because Rust's string continuation strips ALL leading
// whitespace on the following source line, which would eat the YAML nesting
// indentation under `kusara:`.
const OKF_DOC: &str = "---\ntype: spec\ntitle: \"Auth\"\ndescription: \"Authentication design\"\ntags: [auth, security]\nkusara:\n  id: spec:auth\n---\n\n# Auth\n";

#[test]
fn okf_fields_surface_in_show() {
    let dir = fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/specs/auth.md", OKF_DOC); // has description + tags
    ks(dir.path())
        .args(["show", "spec:auth"])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "description:  Authentication design",
        ))
        .stdout(predicate::str::contains("tags:         auth, security"));
}

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
        .stdout(predicate::str::contains("kind:         spec"))
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
    ks(dir.path()).arg("validate").assert().failure();
}

#[test]
fn type_without_kusara_is_skipped() {
    let dir = fixture(MIN_KINDS_MD);
    // Placed directly under docs/ (not under docs/specs/, which is covered by
    // the `spec` kind's `path_globs` and would trip the separate "matches a
    // kind glob but has no kusara front matter" strict-coverage check in
    // `cmd_validate` -- a different, unrelated invariant). This location only
    // exercises `FrontMatter::normalize`'s `(None, None) => Ok(None)` skip path.
    write(dir.path(), "docs/x.md", "---\ntype: spec\n---\n# x\n");
    ks(dir.path())
        .arg("validate")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK (0 docs)"));
    ks(dir.path())
        .args(["show", "spec:x"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unknown id"));
}

#[test]
fn kusara_without_type_is_error() {
    let dir = fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/specs/x.md",
        "---\nkusara:\n  id: spec:x\n---\n# x\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("type"));
}

#[test]
fn okf_fields_in_graph_json() {
    let dir = fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/specs/auth.md", OKF_DOC); // has description + tags
    write(
        dir.path(),
        "docs/specs/plain.md",
        "---\ntype: spec\nkusara:\n  id: spec:plain\n---\n# Plain\n",
    );
    ks(dir.path()).args(["index", "map"]).assert().success();
    let json_str = fs::read_to_string(dir.path().join("docs/ai/graph.json")).unwrap();
    let json: serde_json::Value = serde_json::from_str(&json_str).expect("valid json");
    let docs = json["docs"].as_array().expect("docs array");

    let auth = docs
        .iter()
        .find(|d| d["id"] == "spec:auth")
        .expect("spec:auth present");
    assert_eq!(auth["description"], "Authentication design");
    assert_eq!(auth["tags"], serde_json::json!(["auth", "security"]));

    let plain = docs
        .iter()
        .find(|d| d["id"] == "spec:plain")
        .expect("spec:plain present");
    assert!(
        plain.get("description").is_none(),
        "plain doc must omit description: {plain}"
    );
    assert!(
        plain.get("tags").is_none(),
        "plain doc must omit tags: {plain}"
    );
}

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

// ---------------------------------------------------------------------------
// Migrate
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Claude Code hook adapters
// ---------------------------------------------------------------------------

fn postedit_payload(session: &str, file: &str) -> String {
    format!(
        r#"{{"session_id":"{session}","tool_name":"Edit","tool_input":{{"file_path":"{file}"}}}}"#
    )
}

fn stop_payload(session: &str, active: bool) -> String {
    format!(r#"{{"session_id":"{session}","stop_hook_active":{active}}}"#)
}

fn journal_contents(dir: &Path) -> String {
    let mut out = String::new();
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    for e in entries {
        out += &fs::read_to_string(e.unwrap().path()).unwrap();
    }
    out
}

fn run_postedit(root: &Path, journal: &Path, session: &str, file: &str) {
    ks(root)
        .args(["hook", "postedit", "--journal-dir"])
        .arg(journal)
        .write_stdin(postedit_payload(session, file))
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn hook_postedit_records_relative_path_without_manifest() {
    // No kinds.md on purpose: postedit must not need the graph.
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("journal");
    let abs = dir.path().join("src/a.rs").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    assert_eq!(journal_contents(&journal), "src/a.rs\n");
}

#[cfg(unix)]
#[test]
fn hook_postedit_journal_dir_and_file_are_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("journal");
    let abs = dir.path().join("src/a.rs").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    let dir_mode = fs::metadata(&journal).unwrap().permissions().mode() & 0o777;
    assert_eq!(dir_mode, 0o700, "journal dir must be user-private");
    let entry = fs::read_dir(&journal).unwrap().next().unwrap().unwrap();
    let file_mode = entry.metadata().unwrap().permissions().mode() & 0o777;
    assert_eq!(file_mode, 0o600, "journal file must be user-private");
}

#[test]
fn hook_postedit_ignores_files_outside_root() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("journal");
    run_postedit(dir.path(), &journal, "s1", "/somewhere/else/a.rs");
    assert_eq!(journal_contents(&journal), "");
}

#[test]
fn hook_postedit_rejects_parent_dir_traversal() {
    let dir = tempfile::tempdir().unwrap();
    let journal = dir.path().join("journal");
    // Lexically under root, but resolves outside of it.
    run_postedit(dir.path(), &journal, "s1", "src/../../outside/a.rs");
    let abs_escape = format!("{}/src/../../outside/a.rs", dir.path().display());
    run_postedit(dir.path(), &journal, "s1", &abs_escape);
    assert_eq!(journal_contents(&journal), "");
}

#[test]
fn hook_postedit_tolerates_garbage_and_empty_stdin() {
    let dir = tempfile::tempdir().unwrap();
    for stdin in ["", "not json", r#"{"tool_input":{}}"#] {
        ks(dir.path())
            .args(["hook", "postedit"])
            .write_stdin(stdin)
            .assert()
            .success()
            .stdout(predicate::str::is_empty());
    }
}

#[test]
fn hook_stop_silent_without_journal() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn hook_stop_reports_docs_of_record_for_source_edit() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    fs::create_dir_all(dir.path().join("src/auth")).unwrap();
    fs::write(dir.path().join("src/auth/session.rs"), "").unwrap();
    write(
        dir.path(),
        "docs/specs/auth.md",
        "---\nrefs:\n  id: spec:auth\n  kind: spec\n  modules:\n    - src/auth/\n---\n",
    );
    let abs = dir.path().join("src/auth/session.rs").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::contains("additionalContext"))
        .stdout(predicate::str::contains("spec:auth"))
        .stdout(predicate::str::contains("src/auth/session.rs"));
    // Journal is consumed: a second stop is silent.
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn hook_stop_lists_linked_docs_for_edited_managed_doc() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    write(
        dir.path(),
        "docs/specs/auth.md",
        "---\nrefs:\n  id: spec:auth\n  kind: spec\n---\n",
    );
    write(
        dir.path(),
        "docs/specs/parent.md",
        "---\nrefs:\n  id: spec:parent\n  kind: spec\n  depends_on:\n    - spec:auth\n---\n",
    );
    let abs = dir.path().join("docs/specs/auth.md").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::contains("content drift"))
        .stdout(predicate::str::contains("spec:auth"))
        .stdout(predicate::str::contains("spec:parent"));
}

#[test]
fn hook_stop_silent_for_unmanaged_edit() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    let abs = dir.path().join("README.md").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn hook_stop_blocks_on_validate_error() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    // Managed path without refs front matter -> validate error.
    write(dir.path(), "docs/specs/orphan.md", "# no refs here\n");
    let abs = dir
        .path()
        .join("docs/specs/orphan.md")
        .display()
        .to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""decision":"block""#))
        .stdout(predicate::str::contains("orphan.md"));
}

#[test]
fn hook_stop_downgrades_block_when_stop_hook_active() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    write(dir.path(), "docs/specs/orphan.md", "# no refs here\n");
    let abs = dir
        .path()
        .join("docs/specs/orphan.md")
        .display()
        .to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s1", true))
        .assert()
        .success()
        .stdout(predicate::str::contains("additionalContext"))
        .stdout(predicate::str::contains("block").not());
}

#[test]
fn hook_stop_appends_note() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    fs::create_dir_all(dir.path().join("src/auth")).unwrap();
    fs::write(dir.path().join("src/auth/session.rs"), "").unwrap();
    write(
        dir.path(),
        "docs/specs/auth.md",
        "---\nrefs:\n  id: spec:auth\n  kind: spec\n  modules:\n    - src/auth/\n---\n",
    );
    let abs = dir.path().join("src/auth/session.rs").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    ks(dir.path())
        .args([
            "hook",
            "stop",
            "--note",
            "Decision table: .claude/rules/refs.md",
            "--journal-dir",
        ])
        .arg(&journal)
        .write_stdin(stop_payload("s1", false))
        .assert()
        .success()
        .stdout(predicate::str::contains("Decision table"));
}

#[test]
fn hook_stop_sessions_are_isolated() {
    let dir = fixture(MIN_KINDS_MD);
    let journal = dir.path().join("journal");
    fs::create_dir_all(dir.path().join("src/auth")).unwrap();
    fs::write(dir.path().join("src/auth/session.rs"), "").unwrap();
    write(
        dir.path(),
        "docs/specs/auth.md",
        "---\nrefs:\n  id: spec:auth\n  kind: spec\n  modules:\n    - src/auth/\n---\n",
    );
    let abs = dir.path().join("src/auth/session.rs").display().to_string();
    run_postedit(dir.path(), &journal, "s1", &abs);
    // A different session sees no journal.
    ks(dir.path())
        .args(["hook", "stop", "--journal-dir"])
        .arg(&journal)
        .write_stdin(stop_payload("s2", false))
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

// ---------------------------------------------------------------------------
// Stale
// ---------------------------------------------------------------------------

fn git_in(root: &Path, args: &[&str], epoch: &str) {
    let date = format!("@{epoch} +0000");
    let status = std::process::Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .env("GIT_AUTHOR_NAME", "test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env_remove("GIT_DIR")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

fn git_fixture(kinds_md: &str) -> TempDir {
    let dir = fixture(kinds_md);
    git_in(dir.path(), &["init", "-q"], "1000000000");
    dir
}

fn git_commit_all(root: &Path, msg: &str, epoch: &str) {
    git_in(root, &["add", "-A"], epoch);
    git_in(root, &["commit", "-q", "-m", msg, "--no-verify"], epoch);
}

const STALE_DOC: &str =
    "---\nrefs:\n  id: ref:auth\n  kind: ref\n  modules:\n    - src/auth/\n---\n\n# Auth\n";

#[test]
fn stale_flags_doc_older_than_module() {
    let dir = git_fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/ref/auth.md", STALE_DOC);
    write(dir.path(), "src/auth/session.rs", "// v1\n");
    git_commit_all(dir.path(), "doc + code", "1000000100");
    write(dir.path(), "src/auth/session.rs", "// v2\n");
    git_commit_all(dir.path(), "code only", "1000000200");
    ks(dir.path()).arg("stale").assert().code(1).stdout(
        predicate::str::contains("docs/ref/auth.md")
            .and(predicate::str::contains("ref:auth"))
            .and(predicate::str::contains("src/auth/"))
            .and(predicate::str::contains("1 stale doc(s)")),
    );
}

#[test]
fn stale_ok_when_doc_is_newest() {
    let dir = git_fixture(MIN_KINDS_MD);
    write(dir.path(), "src/auth/session.rs", "// v1\n");
    git_commit_all(dir.path(), "code", "1000000100");
    write(dir.path(), "docs/ref/auth.md", STALE_DOC);
    git_commit_all(dir.path(), "doc", "1000000200");
    ks(dir.path())
        .arg("stale")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn stale_same_commit_is_fresh() {
    let dir = git_fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/ref/auth.md", STALE_DOC);
    write(dir.path(), "src/auth/session.rs", "// v1\n");
    git_commit_all(dir.path(), "doc + code together", "1000000100");
    ks(dir.path()).arg("stale").assert().success();
}

#[test]
fn stale_ignores_docs_without_modules() {
    let dir = git_fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/ref/plain.md",
        "---\nrefs:\n  id: ref:plain\n  kind: ref\n---\n",
    );
    git_commit_all(dir.path(), "doc", "1000000100");
    write(dir.path(), "src/lib.rs", "// newer\n");
    git_commit_all(dir.path(), "code", "1000000200");
    ks(dir.path()).arg("stale").assert().success();
}

#[test]
fn stale_uncommitted_doc_is_fresh() {
    let dir = git_fixture(MIN_KINDS_MD);
    write(dir.path(), "src/auth/session.rs", "// v1\n");
    git_commit_all(dir.path(), "code", "1000000100");
    // Doc exists only in the working tree; it cannot be stale yet.
    write(dir.path(), "docs/ref/auth.md", STALE_DOC);
    ks(dir.path()).arg("stale").assert().success();
}

#[test]
fn stale_exact_file_module() {
    let dir = git_fixture(MIN_KINDS_MD);
    write(
        dir.path(),
        "docs/ref/auth.md",
        "---\nrefs:\n  id: ref:auth\n  kind: ref\n  modules:\n    - src/auth/session.rs\n---\n",
    );
    write(dir.path(), "src/auth/session.rs", "// v1\n");
    git_commit_all(dir.path(), "doc + code", "1000000100");
    write(dir.path(), "src/auth/session.rs", "// v2\n");
    git_commit_all(dir.path(), "code only", "1000000200");
    ks(dir.path())
        .arg("stale")
        .assert()
        .code(1)
        .stdout(predicate::str::contains("src/auth/session.rs"));
}

#[test]
fn stale_outside_git_repo_fails() {
    let dir = fixture(MIN_KINDS_MD);
    write(dir.path(), "docs/ref/auth.md", STALE_DOC);
    write(dir.path(), "src/auth/session.rs", "// v1\n");
    ks(dir.path())
        .arg("stale")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("git"));
}
