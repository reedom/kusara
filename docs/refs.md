# Cross-Reference Schema

Machine-readable cross-references between roadmap, specs, FR pages, reference pages, and source modules. Markdown files in directories scanned by `kusara` SHOULD carry OKF-native front matter: OKF's reserved keys flat at the top level, plus a `kusara:` block holding kusara's typed graph.

[`kusara`](../README.md) consumes these blocks. Hand-edited, not generated.

Valid kinds are configured in [`kinds.md`](kinds.md) (resolved at runtime as `${KUSARA_DOC_ROOT}/kinds.md`, default `KUSARA_DOC_ROOT=docs`). Adding a kind = edit that manifest, no code change.

Legacy `refs:`-wrapped front matter (see the [appendix](#legacy-refs-deprecated)) is still read but deprecated: reading one prints a stderr warning and `kusara migrate` rewrites it in place.

## Canonical frontmatter shape

kusara front matter is a valid [Open Knowledge Format (OKF) v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf) concept document, natively. OKF's reserved keys sit flat at the top level; kusara's typed cross-reference graph lives under a single `kusara:` key, which OKF treats as an arbitrary extra key and ignores. This means an off-the-shelf OKF consumer can read any kusara doc, and kusara keeps its enforceable typed graph.

```yaml
---
type: fr                         # required. OKF `type` == kusara kind. Validated vs docs/kinds.md.
title: "Login flow"              # OKF reserved (shared with kusara).
description: "User login flow"   # OKF reserved, optional.
resource: "https://..."          # OKF reserved, optional -- canonical asset URI.
tags: [auth, security]           # OKF reserved, optional.
timestamp: 2026-07-04T00:00:00Z  # OKF reserved, optional -- ISO 8601.
kusara:                          # kusara graph layer -- an OKF "arbitrary extra key".
  id: fr:login                   # required. kusara's ID authority (path-independent).
  spec: auth                     # optional, parent spec (null for cross-spec).
  implements: [req:auth]         # hard edge: exists to satisfy these upstream artifacts.
  depends_on: [spec:auth]        # hard edge: would be wrong without these.
  related: []                    # soft edge: see-also.
  provides: []                   # sub-IDs declared in this file (file-less IDs).
  modules: [src/auth/session.rs] # source paths this doc is the design of record for.
  # generated / indexes_kind: machine-written only (kusara index); never hand-edited.
---
```

All list fields default to empty.

### Field ownership

| Field                 | Layer         | Required | Notes                                             |
|-----------------------|---------------|----------|----------------------------------------------------|
| `type`                | shared (flat) | yes      | == old `kind`; validated against `docs/kinds.md`. |
| `title`                | shared (flat) | no       | OKF reserved; kusara reads it from here.          |
| `description`          | OKF (flat)    | no       | Stored, surfaced in `show`/JSON; not validated.   |
| `resource`              | OKF (flat)    | no       | Stored, surfaced; not validated.                  |
| `tags`                  | OKF (flat)    | no       | Stored, surfaced; not validated.                  |
| `timestamp`             | OKF (flat)    | no       | Stored, surfaced; not validated.                  |
| `kusara.id`             | kusara        | yes      | Unique repo-wide; ID authority.                   |
| `kusara.spec`           | kusara        | no       | Parent spec.                                      |
| `kusara.implements`     | kusara        | no       | Hard edge.                                        |
| `kusara.depends_on`     | kusara        | no       | Hard edge.                                        |
| `kusara.related`        | kusara        | no       | Soft edge.                                        |
| `kusara.provides`       | kusara        | no       | File-less sub-IDs.                                |
| `kusara.modules`        | kusara        | no       | Doc-of-record source paths.                       |
| `kusara.generated`      | kusara        | no       | Machine-written only.                             |
| `kusara.indexes_kind`   | kusara        | no       | Machine-written only.                             |

### Unknown-key handling

- Unknown **top-level** keys are tolerated (OKF forward-compatibility: arbitrary extra keys are allowed at that layer).
- Unknown keys **inside `kusara:`** are rejected -- it is kusara's namespace, and a typo there is almost always a mistake.
- A doc with both a top-level `refs:` key and top-level `type:`/`kusara:` is rejected as ambiguous.

## HTML documents

kusara also scans `.html` / `.htm` files. Because HTML has no frontmatter, the
metadata block lives in an embedded data block: a `<script>` element whose `type`
is `application/kusara+yaml`. Its body is the **same** YAML used in Markdown
frontmatter (the flat OKF keys plus `kusara:`, exactly as above).

```html
<head>
<script type="application/kusara+yaml">
type: spec
kusara:
  id: spec:auth
  implements: [req:auth:1]
  modules: [src/auth/]
</script>
</head>
```

- A non-JavaScript script `type` is an inert data block: browsers never render or
  execute it.
- kusara scans the whole file for the first matching block (the `<head>`
  placement is convention, not required); the first block wins.
- The YAML should start at column 0 inside the script -- the generator is
  responsible for emitting it un-indented.
- Opt HTML files into a kind exactly like Markdown, via `path_globs` (e.g.
  `docs/specs/*.html`). All other behaviour (`modules`, `provides`, indexes, map,
  validation) is identical.

## Relation semantics

| Field | Meaning | Direction | Strength |
|---|---|---|---|
| `implements` | "I exist to satisfy this upstream artifact." | downstream → upstream | hard |
| `depends_on` | "I would be incorrect or incomplete without this." | downstream → upstream | hard |
| `related` | "See also." | bidirectional | soft |
| `provides` | "I declare these additional IDs inside my body." | self → child IDs | hard |
| `modules` | "I am the design of record for these source paths." | doc → code | hard |

`kusara impact` traverses the forward graph (`implements` + `depends_on`). `related` is informational by default; `--include-related` includes it in the traversal.

## ID grammar

```
id := <kind> | <kind>:<scope> | <kind>:<scope>:<sub>
```

`kind` MUST match a kind in [`kinds.md`](kinds.md). `scope` and `sub` shape is per-kind convention (`id_pattern`); validator enforces uniqueness only. `kusara.id` is where this value lives in front matter.

## Kinds manifest

The full set of kinds, path globs, and INDEX generation is declared in [`kinds.md`](kinds.md). The front matter field that selects a kind is the top-level `type:` (formerly `kind:`), still validated against the kinds listed there:

- **Add a new kind** → edit the YAML block in `kinds.md`.
- **Generate an INDEX** → add `index: { output: <path> }` to the kind's entry.
- **Remove a kind** → delete its entry; existing front matter using it will fail validation.

## Provides (sub-IDs)

When a doc enumerates child IDs in `kusara.provides`, downstream docs may reference any listed ID even though the child has no file of its own:

```yaml
---
type: spec
kusara:
  id: spec:my-spec
  spec: my-spec
  provides:
    - req:my-spec:1
    - req:my-spec:2
    - req:my-spec:1.6
---
```

The validator does not parse the body; `provides` is the source of truth for what IDs exist.

## Modules

`kusara.modules` declares "this doc is the documentation of record for that source path." `kusara touched <files>` reverses the relationship.

- No trailing slash: literal file path.
- Trailing slash: directory prefix (any file underneath).

```yaml
kusara:
  modules:
    - src/auth/session.rs        # exact file
    - src/auth/                  # any file under this directory
```

A file MAY appear in multiple docs' `modules:` lists; `kusara` surfaces all.

## Authoring workflow

1. Pick the primary `kusara.id` matching the kind's pattern in `kinds.md`.
2. List upstream artifacts this doc satisfies in `kusara.implements`.
3. List upstream artifacts this doc would be wrong without in `kusara.depends_on`.
4. List weak see-also links in `kusara.related`.
5. For docs of record for code, list source paths in `kusara.modules`.

Prose "Traceability" sections (if any) MUST agree with front matter. Cross-checking is a code-review responsibility.

## Generated index files

`kusara index` writes per-kind `index.md` for kinds with `index.output` set, and `kusara index map` writes the global `map.md` / `ai/modules.md`. Generated docs carry `type: index` and `kusara: { generated: true, indexes_kind: <kind> }` (per-kind indexes only; the global map/modules docs omit `indexes_kind`). Sibling `README.md` may carry human narrative.

### Forbidden hand-edits

Never set these by hand on regular docs. They are written exclusively by `kusara index`:

- `kusara.generated`
- `kusara.indexes_kind`

`kusara index map` writes `${KUSARA_DOC_ROOT}/map.md`, `${KUSARA_DOC_ROOT}/ai/graph.json`, and `${KUSARA_DOC_ROOT}/ai/modules.md`.

Naming: only `README.md` is capitalized; all other generated/config files lowercase.

## Tooling reference

```sh
kusara validate
kusara impact <id> [<id>...] [--depth <N>] [--include-related]
kusara deps   <id> [<id>...] [--depth <N>] [--include-related]
kusara show   <id>
kusara touched <file> [<file>...] [--no-closure]
kusara list
kusara index map     # writes map.md + ai/graph.json + ai/modules.md
kusara index         # writes per-kind index.md files
kusara migrate [--dry-run]  # rewrite legacy `refs:` docs to the OKF-native shape
```

## Out of scope

- Validator does not check prose `Traceability` agreement with front matter -- humans audit.
- Validator does not parse headings; `provides:` is the source of truth.
- `id_pattern` in `kinds.md` is documentation, not enforcement; uniqueness is the only ID check.
- No glob expansion in `modules:` -- literal paths and directory prefixes only.
- No automatic doc-to-doc propagation.

## Legacy `refs:` (deprecated)

Before the OKF-native shape, everything lived under a single top-level `refs:` key:

```yaml
---
refs:
  id: <kind>:<scope>[:<sub>]    # required, globally unique
  kind: <kind>                   # required, must match a name in docs/kinds.md
  title: "<free text>"           # optional, used by index / show output
  spec: <spec-name>              # optional, parent spec (null for cross-spec docs)
  provides:                      # optional, additional IDs declared inside this file
    - <id>
  implements:                    # optional, IDs this file fulfills
    - <id>
  depends_on:                    # optional, hard upstream IDs
    - <id>
  related:                       # optional, weak see-also (non-blocking)
    - <id>
  modules:                       # optional, repo-relative source paths or dir prefixes
    - <path>
    - <path-prefix>/             # trailing slash: any file under this directory
  generated: false               # set by `kusara index` on generated INDEX files
  indexes_kind: <kind>           # set by `kusara index` on per-kind INDEX files
---
```

This shape is still **read** (dual-read, for a transitional period) but is deprecated: parsing one prints a stderr warning naming the file and pointing here. `refs.kind` maps to top-level `type`, `refs.title` maps to top-level `title`, and everything else moves under `kusara:` unchanged. Run `kusara migrate` (add `--dry-run` to preview which files would change without writing) to rewrite legacy docs in place; the command is idempotent -- already-migrated docs are left untouched. It handles both Markdown and the HTML `<script type="application/kusara+yaml">` variant.

A doc with both a top-level `refs:` key and top-level `type:`/`kusara:` is rejected as ambiguous -- migrate fully, don't mix shapes in one file.

**Known limitation:** the YAML round-trip performed by `kusara migrate` drops comments inside the frontmatter block; body content below the frontmatter is untouched.
