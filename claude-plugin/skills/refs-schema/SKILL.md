---
name: refs-schema
description: Authoritative schema for the kusara `refs:` YAML frontmatter block. Use whenever editing, authoring, validating, or interpreting `refs:` frontmatter in Markdown docs scanned by kusara — including fields like `id`, `kind`, `implements`, `depends_on`, `related`, `provides`, `modules`, `generated`, `indexes_kind`. Also use when answering questions about kusara cross-reference semantics or relation strength (hard vs soft).
---

# kusara `refs:` frontmatter schema

This skill is the single source of truth for the shape and semantics of `refs:` blocks. Use it before editing frontmatter or judging validator errors. The body below is the schema; the `references/` directory carries the verbatim project doc.

## Shape

```yaml
---
refs:
  id: <kind>:<scope>[:<sub>]    # required, globally unique
  kind: <kind>                   # required, must match docs/kinds.md
  title: "<free text>"           # optional
  spec: <spec-name>              # optional, parent spec (null for cross-spec)
  provides:                      # optional, sub-IDs declared in this file
    - <id>
  implements:                    # optional, upstream IDs satisfied
    - <id>
  depends_on:                    # optional, hard upstream deps
    - <id>
  related:                       # optional, weak see-also
    - <id>
  modules:                       # optional, source paths or dir prefixes
    - <path>
    - <path-prefix>/             # trailing slash = directory prefix
  generated: false               # set by `kusara index` only
  indexes_kind: <kind>           # set by `kusara index` only
---
```

All list fields default to empty.

## Relation semantics

| Field        | Meaning                                       | Direction               | Strength |
|--------------|-----------------------------------------------|-------------------------|----------|
| `implements` | "I exist to satisfy this upstream artifact."  | downstream → upstream   | hard     |
| `depends_on` | "I would be wrong without this."              | downstream → upstream   | hard     |
| `related`    | "See also."                                   | bidirectional           | soft     |
| `provides`   | "I declare these IDs inside my body."         | self → child IDs        | hard     |
| `modules`    | "I am the doc of record for these paths."     | doc → code              | hard     |

`kusara impact` traverses `implements + depends_on`. `related` joins traversal only with `--include-related`.

## ID grammar

```
id := <kind> | <kind>:<scope> | <kind>:<scope>:<sub>
```

`kind` MUST exist in `docs/kinds.md`. Validator enforces global uniqueness only; `id_pattern` in the kinds manifest is documentation, not enforcement.

## Modules paths

- No trailing slash → exact file (e.g., `src/auth/session.rs`).
- Trailing slash → directory prefix (e.g., `src/auth/` matches any file under it).
- A path MAY appear in multiple docs' `modules:` lists.

## Provides (sub-IDs)

`provides:` is the only way to declare IDs that have no file of their own (typical for `kind: req`). Any downstream `implements:`/`depends_on:` may reference a provided ID. The validator does not parse the body — `provides:` is the source of truth for which sub-IDs exist.

## Forbidden hand-edits

Never set these by hand on regular docs. They are written exclusively by `kusara index`:
- `generated:`
- `indexes_kind:`

## Authoring checklist

When adding or updating a `refs:` block:

1. `id` matches the kind's `id_pattern` and is unique repo-wide.
2. `kind` is one of the names listed in `docs/kinds.md`.
3. `implements:` lists the upstream artifacts this doc satisfies (hard).
4. `depends_on:` lists artifacts this doc would be incorrect without (hard).
5. `related:` lists weak see-also links (soft).
6. `modules:` lists source paths this doc is the design of record for.
7. Run `kusara validate` after the edit.

## Validator behaviour

- Catches: unknown `kind`, duplicate `id`, dangling `implements`/`depends_on`/`related` (target not in graph), schema errors.
- Does NOT catch: prose drift in "Traceability" sections, incorrect prose narration, body content vs frontmatter mismatch. Humans audit those.

## When this skill is wrong

If the body above conflicts with the project doc, the project doc wins. Read `references/refs.md` (the verbatim copy) for the canonical text, and update this skill to match.

## References

- `references/refs.md` — verbatim copy of `docs/refs.md` from the kusara repo (the canonical schema).
- `references/relations-cheatsheet.md` — quick lookup table for which relation to use.
