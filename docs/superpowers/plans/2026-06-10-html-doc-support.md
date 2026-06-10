# HTML Doc Support Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let kusara ingest spec documents written in HTML by reading the `refs:` metadata from an embedded `<script type="application/kusara+yaml">` block, with full parity to Markdown frontmatter.

**Architecture:** Everything downstream of metadata extraction in `src/main.rs` is already format-agnostic (it operates on `Doc` + `rel_path`). The feature adds one new extractor (`extract_html_metadata`) plus extension-based dispatch in `build_graph`; both extractors yield the same `refs:` YAML string that flows into the existing `serde_yaml_ng::from_str::<FrontMatter>` call. No graph, validation, or output changes.

**Tech Stack:** Rust, `serde_yaml_ng`, `walkdir`; integration tests via `assert_cmd` + `predicates` + `tempfile`.

---

## File Structure

- Modify: `src/main.rs`
  - Add `HTML_META_TYPE` const, `HtmlMeta` enum, `extract_html_metadata`, `script_type_matches` (near `extract_frontmatter`, ~line 630).
  - Add `DocFormat` enum and replace the extension gate + YAML extraction in `build_graph` (~lines 456–498).
  - Add unit tests in the existing `#[cfg(test)] mod tests` block (~line 1264).
- Modify: `tests/integration.rs` — add HTML integration tests.
- Modify: `docs/refs.md` — document the HTML storage convention.
- Modify: `README.md` — note HTML support.
- Modify: `claude-plugin/skills/refs-schema/references/refs.md` — mirror the convention.

---

## Task 1: HTML metadata extractor (unit-tested)

**Files:**
- Modify: `src/main.rs` (add functions near line 630; add unit tests in `mod tests` ~line 1264)

- [ ] **Step 1: Write the failing unit tests**

Add to the `#[cfg(test)] mod tests` block in `src/main.rs`:

```rust
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
    let raw = "<script id=\"m\" type=\"application/kusara+yaml\" defer>\nrefs:\n  id: a\n</script>";
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
    assert!(matches!(
        extract_html_metadata(raw),
        HtmlMeta::Unterminated
    ));
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --lib html_meta`
Expected: FAIL — `cannot find function ` extract_html_metadata`` / `cannot find type `HtmlMeta``.

- [ ] **Step 3: Implement the extractor**

Add near `extract_frontmatter` (after line 636) in `src/main.rs`:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --lib html_meta`
Expected: PASS (9 tests).

- [ ] **Step 5: Commit**

```bash
git add src/main.rs
git commit -m "feat: add HTML metadata extractor"
```

---

## Task 2: Dispatch HTML in build_graph (integration-tested)

**Files:**
- Modify: `src/main.rs` (extension gate + YAML extraction in `build_graph`, ~lines 456–498)
- Modify: `tests/integration.rs` (add HTML fixtures + tests)

- [ ] **Step 1: Write the failing integration tests**

Add to `tests/integration.rs`. First add this constant near `MIN_KINDS_MD` (top of file):

```rust
const HTML_KINDS_MD: &str = "# kinds\n\n```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.html\"]\n    id_pattern: \"spec:{slug}\"\n  - name: req\n    declared_via: provides\n    id_pattern: \"req:{spec}:{n}\"\n  - name: index\n    declared_via: generated\n    id_pattern: \"index:{kind}\"\n```\n";

const MIXED_KINDS_MD: &str = "# kinds\n\n```yaml\nkinds:\n  - name: spec\n    path_globs: [\"docs/specs/*.md\", \"docs/specs/*.html\"]\n    id_pattern: \"spec:{slug}\"\n  - name: req\n    declared_via: provides\n    id_pattern: \"req:{spec}:{n}\"\n  - name: index\n    declared_via: generated\n    id_pattern: \"index:{kind}\"\n```\n";
```

Then add these tests (after the existing `validate_clean_repo` test):

```rust
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
        .stdout(predicate::str::contains("id:       spec:foo"))
        .stdout(predicate::str::contains("kind:     spec"))
        .stdout(predicate::str::contains("path:     docs/specs/foo.html"));
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
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test --test integration html`
Expected: FAIL — the `.html` files are not scanned, so `spec:foo` is unknown and `validate` reports "has no `refs:` block".

- [ ] **Step 3: Implement the extension dispatch**

In `src/main.rs`, add this enum next to the other small types (e.g. just above `fn build_graph`, ~line 417):

```rust
/// Source syntax kusara knows how to read metadata from.
#[derive(Clone, Copy)]
enum DocFormat {
    Markdown,
    Html,
}
```

Then in `build_graph`, replace the extension gate (the current lines):

```rust
            let path = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("md") {
                continue;
            }
```

with:

```rust
            let path = entry.path();
            let format = match path.extension().and_then(|s| s.to_str()) {
                Some("md") => DocFormat::Markdown,
                Some("html") | Some("htm") => DocFormat::Html,
                _ => continue,
            };
```

And replace the current YAML extraction block:

```rust
            let yaml = match extract_frontmatter(&raw) {
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
            };
```

with:

```rust
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
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test --test integration html`
Expected: PASS (3 tests).

- [ ] **Step 5: Run the full suite to confirm no regressions**

Run: `cargo test`
Expected: PASS (all existing + new tests).

- [ ] **Step 6: Commit**

```bash
git add src/main.rs tests/integration.rs
git commit -m "feat: scan HTML docs for embedded refs metadata"
```

---

## Task 3: Error parity for HTML (integration-tested)

**Files:**
- Modify: `tests/integration.rs`

- [ ] **Step 1: Write the failing tests**

Add to `tests/integration.rs` (after the tests from Task 2):

```rust
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
        .stderr(predicate::str::contains("has no `refs:` block"));
}
```

- [ ] **Step 2: Run the tests to verify they pass**

These exercise behavior already implemented in Task 2 (the `Unterminated` arm and the format-agnostic glob-coverage check). Run: `cargo test --test integration html`
Expected: PASS. If `validate_malformed_html_metadata_reports_error` fails, the `Unterminated` arm in Task 2 is wrong — fix there.

- [ ] **Step 3: Commit**

```bash
git add tests/integration.rs
git commit -m "test: cover HTML metadata error parity"
```

---

## Task 4: Documentation

**Files:**
- Modify: `docs/refs.md`
- Modify: `README.md`
- Modify: `claude-plugin/skills/refs-schema/references/refs.md`

- [ ] **Step 1: Document the convention in `docs/refs.md`**

In `docs/refs.md`, immediately after the "## Front-matter shape" section (before "## Relation semantics"), add:

````markdown
## HTML documents

kusara also scans `.html` / `.htm` files. Because HTML has no frontmatter, the
`refs:` block lives in an embedded data block: a `<script>` element whose `type`
is `application/kusara+yaml`. Its body is the **same** YAML used in Markdown
frontmatter (top-level `refs:` key included).

```html
<head>
<script type="application/kusara+yaml">
refs:
  id: spec:auth
  kind: spec
  implements: [req:auth:1]
  modules: [src/auth/]
</script>
</head>
```

- A non-JavaScript script `type` is an inert data block: browsers never render or
  execute it.
- kusara scans the whole file for the first matching block (the `<head>`
  placement is convention, not required); the first block wins.
- The YAML should start at column 0 inside the script — the generator is
  responsible for emitting it un-indented.
- Opt HTML files into a kind exactly like Markdown, via `path_globs` (e.g.
  `docs/specs/*.html`). All other behaviour (`modules`, `provides`, indexes, map,
  validation) is identical.
````

- [ ] **Step 2: Note HTML support in `README.md`**

Find the section of `README.md` that describes which files kusara scans (search for "front matter" or "Markdown"). Add one sentence:

```markdown
kusara also scans `.html` / `.htm` docs: the `refs:` block lives in a
`<script type="application/kusara+yaml">` data block instead of frontmatter. See
[docs/refs.md](docs/refs.md#html-documents).
```

- [ ] **Step 3: Mirror the convention in the plugin skill reference**

Apply the same "## HTML documents" section from Step 1 to
`claude-plugin/skills/refs-schema/references/refs.md` (same insertion point —
after the front-matter shape section).

- [ ] **Step 4: Verify docs build is consistent**

Run: `cargo test`
Expected: PASS (docs changes don't affect tests; this confirms the tree is still green before commit).

- [ ] **Step 5: Commit**

```bash
git add docs/refs.md README.md claude-plugin/skills/refs-schema/references/refs.md
git commit -m "docs: document HTML doc metadata convention"
```

---

## Final verification

- [ ] Run `cargo test` — all tests pass.
- [ ] Run `cargo clippy --all-targets` — no new warnings.
- [ ] Run `cargo fmt --check` — formatting clean.
- [ ] Manual smoke: in a scratch dir with a `docs/specs/*.html` kind, create an HTML spec with a metadata block and confirm `kusara validate` and `kusara show <id>` work.
