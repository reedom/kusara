# HTML Doc Support — Design

Date: 2026-06-10
Status: Approved (design); implementation pending

## Problem

kusara only ingests Markdown. `build_graph` (`src/main.rs`) skips any file whose
extension is not `md`, then reads the leading `---\n…\n---\n` YAML frontmatter and
deserializes the `refs:` block.

We want to scan documents written directly in HTML — primarily spec files. These
HTML files are **machine-generated and are the source of record**: there is no
Markdown behind them, so the `refs:` metadata has to travel inside (or beside) the
HTML, and kusara has to learn to read it.

## Constraints and findings

- The `refs:` block is a *nested* structure: scalars (`id`, `kind`, `title`,
  `spec`), lists (`provides`, `implements`, `depends_on`, `related`, `modules`),
  and booleans (`generated`, `indexes_kind`). Flat `<meta name=… content=…>` tags
  cannot express lists/nesting without a bespoke encoding, so they are rejected.
- Everything downstream of metadata extraction is already **format-agnostic**:
  the graph builder, validator, `impact`/`deps`/`show`/`touched`, and index/map
  generation operate on `Doc` + `rel_path`, never on file syntax. The only
  Markdown assumption lives in the read step of `build_graph`.
- The HTML is machine-generated, so human-editability of the metadata is a
  non-goal; robust parsing and clean generator injection are the priorities.

## Decision

Store the metadata as an **embedded data block**: a `<script>` element carrying
the exact same `refs:` YAML used in Markdown frontmatter.

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

### Why this shape

- The script body is the **same YAML mapping** as Markdown frontmatter, top-level
  `refs:` key included, so it deserializes through the existing `FrontMatter`
  struct with **zero new deserialization code** — one canonical metadata format.
- `type="application/kusara+yaml"` is namespaced (never collides with other
  embedded YAML) and, being a non-JavaScript script type, is an inert "data block"
  per the HTML spec — browsers neither render nor execute it.
- Self-contained: the spec file describes itself; nothing can drift away from it.

### Alternatives considered

- **Sidecar `.refs.yaml` file** next to each `.html`. Smallest kusara change (no
  HTML parsing at all) but two files per doc that can drift, move, or separate;
  the HTML alone stops being self-describing. Rejected.
- **Flat `<meta name="kusara:*">` tags.** Standard-looking but flat — lists and
  nesting need a custom encoding and a bespoke parser, diverging from the single
  YAML model. Most new code, least format parity. Rejected.

## Architecture

The whole feature is: **add a second extractor and dispatch by file extension.**

1. **Extension gate.** In `build_graph`, accept `.html` and `.htm` in addition to
   `.md`.
2. **Dispatch.** Choose the extractor by extension:
   - `md` → existing `extract_frontmatter`.
   - `html` / `htm` → new `extract_html_metadata`.
   Both return the `refs:` YAML string.
3. **Shared tail.** The returned YAML flows into the *identical*
   `serde_yaml_ng::from_str::<FrontMatter>` call and the existing `Doc`
   construction. Graph build, validation, and all output layers are untouched.

### New function

`extract_html_metadata(raw: &str) -> Option<&str>`

- Hand-rolled scanner (no HTML parser dependency), matching the style of the
  existing `extract_frontmatter` / `extract_fenced_yaml`.
- Finds the first `<script … type="application/kusara+yaml" …>` and captures its
  inner text up to `</script>`.
- Tag name and `type` attribute name matched **case-insensitively** (HTML is
  case-insensitive); the generator emits canonical lowercase.
- Scans the **whole file** for the first matching block (does not require `<head>`;
  `<head>` is convention). First block wins, mirroring "first frontmatter wins."
- Script raw-text content needs no HTML-entity decoding; the YAML must not contain
  the literal sequence `</script>` (it never does).
- Returns `None` when no block is present.

### Error parity with Markdown

- A file with an opening marker tag but no closing `</script>` reports a parse
  error (mirrors the "missing closing `---`" case).
- A file with no block at all is skipped silently (mirrors a Markdown file with no
  frontmatter).

## kinds.md opt-in

HTML specs are opted in exactly like Markdown — through a kind's `path_globs`
(e.g. `docs/specs/*.html`). The validator's "every globbed file must carry a
`refs:` block" check is already format-agnostic, so it covers HTML for free. No
manifest schema change.

## Out of scope

- No HTML rendering, serving, or DOM parsing.
- No Markdown↔HTML conversion (HTML is generated upstream).
- No flat `<meta>` support, no sidecar files.
- `modules`, `provides`, indexes, map, and links are path-based and already work.

## Testing (TDD)

Unit tests for `extract_html_metadata`:
- happy path
- attribute-order variation (`type` among other attributes)
- missing closing tag → parse error
- no block → `None`
- multiple blocks → first wins
- case-insensitive tag and attribute name

Integration test: an `.html` spec node validates, `show`s, and participates in
`impact` / `touched` alongside Markdown docs.

## Docs to update in the same change

- `docs/refs.md` — the embedded-script storage convention for HTML docs.
- `README.md` — note HTML support.
- `claude-plugin/skills/refs-schema/references/refs.md` — mirror the convention.
