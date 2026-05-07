use std::fs;
use std::path::Path;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

const MIN_KINDS_MD: &str = "# kinds\n\n```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.md\"]\n    id_pattern: \"spec:{slug}\"\n    index:\n      output: docs/specs/index.md\n  - name: ref\n    path_globs: [\"docs/ref/[a-z]*.md\"]\n    id_pattern: \"ref:{slug}\"\n  - name: req\n    declared_via: provides\n    id_pattern: \"req:{spec}:{n}\"\n  - name: index\n    declared_via: generated\n    id_pattern: \"index:{kind}\"\n```\n";

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
    let dir = fixture(
        "```yaml\nkinds:\n  - name: orphan\n    id_pattern: \"orphan\"\n```\n",
    );
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
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure();
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
    write(
        dir.path(),
        "docs/specs/foo.md",
        "# no front matter\n",
    );
    ks(dir.path())
        .arg("validate")
        .assert()
        .failure()
        .stderr(predicate::str::contains("has no `refs:` block"));
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
    assert!(!out.contains("spec:a"), "expected spec:a missing in:\n{out}");
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
        .stdout(predicate::str::contains("id:       spec:b"))
        .stdout(predicate::str::contains("kind:     spec"));
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
    ks(dir.path())
        .args(["index", "map"])
        .assert()
        .success();
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
