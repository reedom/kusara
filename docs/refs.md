# Cross-Reference Schema (`refs`)

Machine-readable cross-references between roadmap, specs, FR pages, reference pages, and source modules. Markdown files in directories scanned by `kusara` SHOULD carry a `refs:` block in their YAML front matter.

[`kusara`](../README.md) consumes these blocks. Hand-edited, not generated.

Valid kinds are configured in [`kinds.md`](kinds.md) (resolved at runtime as `${KUSARA_DOC_ROOT}/kinds.md`, default `KUSARA_DOC_ROOT=docs`). Adding a kind = edit that manifest, no code change.

## Front-matter shape

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

All list fields default to empty.

`generated:` and `indexes_kind:` are written by `kusara index` on generated INDEX files (kind `index`). Do not set them by hand on regular docs.

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

`kind` MUST match a kind in [`kinds.md`](kinds.md). `scope` and `sub` shape is per-kind convention (`id_pattern`); validator enforces uniqueness only.

## Kinds manifest

The full set of kinds, path globs, and INDEX generation is declared in [`kinds.md`](kinds.md):

- **Add a new kind** → edit the YAML block in `kinds.md`.
- **Generate an INDEX** → add `index: { output: <path> }` to the kind's entry.
- **Remove a kind** → delete its entry; existing front matter using it will fail validation.

## Provides (sub-IDs)

When a doc enumerates child IDs in `provides:`, downstream docs may reference any listed ID even though the child has no file of its own:

```yaml
refs:
  id: spec:my-spec
  kind: spec
  spec: my-spec
  provides:
    - req:my-spec:1
    - req:my-spec:2
    - req:my-spec:1.6
```

The validator does not parse the body; `provides` is the source of truth for what IDs exist.

## Modules

`modules:` declares "this doc is the documentation of record for that source path." `kusara touched <files>` reverses the relationship.

- No trailing slash: literal file path.
- Trailing slash: directory prefix (any file underneath).

```yaml
modules:
  - src/auth/session.rs        # exact file
  - src/auth/                  # any file under this directory
```

A file MAY appear in multiple docs' `modules:` lists; `kusara` surfaces all.

## Authoring workflow

1. Pick the primary `id` matching the kind's pattern in `kinds.md`.
2. List upstream artifacts this doc satisfies in `implements:`.
3. List upstream artifacts this doc would be wrong without in `depends_on:`.
4. List weak see-also links in `related:`.
5. For docs of record for code, list source paths in `modules:`.

Prose "Traceability" sections (if any) MUST agree with front matter. Cross-checking is a code-review responsibility.

## Generated index files

`kusara index` writes per-kind `index.md` for kinds with `index.output` set. Graph nodes (`kind: index`, `generated: true`); never hand-edit. Sibling `README.md` may carry human narrative.

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
```

## Out of scope

- Validator does not check prose `Traceability` agreement with front matter — humans audit.
- Validator does not parse headings; `provides:` is the source of truth.
- `id_pattern` in `kinds.md` is documentation, not enforcement; uniqueness is the only ID check.
- No glob expansion in `modules:` — literal paths and directory prefixes only.
- No automatic doc-to-doc propagation.
