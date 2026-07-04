# OKF-Compatible Frontmatter — Design

- **Date:** 2026-07-04
- **Status:** Approved (design), pending implementation plan
- **Author:** brainstorming session
- **Scope:** kusara frontmatter format only. No changes to the graph/validate/impact/index engine semantics.

## Summary

Make kusara's Markdown frontmatter a valid [Open Knowledge Format (OKF) v0.1](https://github.com/GoogleCloudPlatform/knowledge-catalog/tree/main/okf) concept document, natively. A kusara doc becomes an OKF concept out of the box: OKF's reserved keys sit flat at the top level, and kusara's typed cross-reference graph lives under a single `kusara:` namespace key that OKF treats as an "arbitrary extra key" and ignores.

The internal graph model (`Doc`, `Graph`, and every downstream command) is unchanged. This is a **frontmatter shape change plus a compatibility layer**, not a graph-semantics change.

## Motivation

kusara and OKF disagree on where knowledge metadata and relationships live:

- **kusara** is a *typed graph*: explicit `id: <kind>:<scope>`, a `kind:` validated against `docs/kinds.md`, and machine-traversable edges (`implements`, `depends_on`, `related`, `provides`, `modules`). The whole value proposition is enforceable relations in frontmatter.
- **OKF** is deliberately the opposite: flat frontmatter (`type`, `title`, `description`, `resource`, `tags`, `timestamp`, plus arbitrary extra keys), IDs *derived from file path*, and relationships expressed as prose Markdown links rather than typed frontmatter edges. It optimizes for vendor-neutral consumption (catalogs, RAG).

Adopting OKF natively lets kusara docs be consumed by any OKF-aware tool (Google's knowledge-catalog consumers, RAG pipelines, catalogs) **without losing** kusara's typed graph. The two layers coexist: OKF reads the flat reserved keys; kusara reads its namespace.

## Decisions

These were settled during brainstorming and are the fixed constraints for the implementation plan:

1. **Direction: adopt OKF natively.** kusara's own frontmatter becomes OKF-shaped. (Not export-only, not import-only, not a per-file superset.)
2. **Identity: keep explicit `id:`.** kusara retains an explicit, path-independent ID as its ID authority. OKF's path-derived IDs are not adopted. This preserves `provides:` (file-less sub-IDs, essential for `kind: req`) and ID stability across file moves.
3. **Rollout: transitional dual-read + `kusara migrate`.** The parser accepts both the legacy `refs:` shape and the new OKF shape. A `kusara migrate` command rewrites docs in place. Legacy is deprecated over a release or two, not hard-cut.
4. **Rename `kind` → `type`.** OKF's required `type` field carries the kusara kind slug (`fr`, `req`, `spec`, …), still validated against `docs/kinds.md`. `type` is the single shared bridge field.
5. **Key layout: OKF reserved keys flat, kusara graph namespaced.** Only `type` and `title` are shared and flat. kusara's graph fields nest under one `kusara:` key. This is collision-proof against future OKF reserved keys and keeps the OKF concept pristine.

## Canonical frontmatter shape

```yaml
---
type: fr                         # required. OKF `type` == kusara kind. Validated vs docs/kinds.md.
title: "Login flow"              # OKF reserved (shared with kusara).
description: "User login flow"   # OKF reserved (new, optional).
resource: "https://..."         # OKF reserved (new, optional) — canonical asset URI.
tags: [auth, security]           # OKF reserved (new, optional).
timestamp: 2026-07-04T00:00:00Z  # OKF reserved (new, optional) — ISO 8601.
kusara:                          # kusara graph layer — OKF-arbitrary extra key.
  id: fr:login                   # required. kusara's ID authority (path-independent).
  implements: [req:auth]         # hard edge: exists to satisfy these upstream artifacts.
  depends_on: [spec:auth]        # hard edge: would be wrong without these.
  related: []                    # soft edge: see-also.
  provides: []                   # sub-IDs declared in this file (file-less IDs).
  modules: [src/auth/session.rs] # source paths this doc is the design of record for.
  spec: auth                     # parent spec (null for cross-spec).
  # generated / indexes_kind: machine-written only (kusara index); never hand-edited.
---
```

### Field ownership

| Field                         | Layer          | Required | Notes                                             |
|-------------------------------|----------------|----------|---------------------------------------------------|
| `type`                        | shared (flat)  | yes      | == old `kind`; validated against `docs/kinds.md`. |
| `title`                       | shared (flat)  | no       | OKF reserved; kusara reads it from here.          |
| `description`                 | OKF (flat)     | no       | Stored, surfaced; not validated.                  |
| `resource`                    | OKF (flat)     | no       | Stored, surfaced; not validated.                  |
| `tags`                        | OKF (flat)     | no       | Stored, surfaced; not validated.                  |
| `timestamp`                   | OKF (flat)     | no       | Stored, surfaced; not validated.                  |
| `kusara.id`                   | kusara         | yes      | Unique repo-wide; ID authority.                   |
| `kusara.implements`           | kusara         | no       | Hard edge.                                        |
| `kusara.depends_on`           | kusara         | no       | Hard edge.                                        |
| `kusara.related`              | kusara         | no       | Soft edge.                                        |
| `kusara.provides`             | kusara         | no       | File-less sub-IDs.                                |
| `kusara.modules`              | kusara         | no       | Doc-of-record source paths.                       |
| `kusara.spec`                 | kusara         | no       | Parent spec.                                       |
| `kusara.generated`            | kusara         | no       | Machine-written only.                             |
| `kusara.indexes_kind`         | kusara         | no       | Machine-written only.                             |

## Architecture

### Parsing (dual-read)

The heart of the change lives in the frontmatter deserialization boundary. Downstream of it, the internal `Doc` model is unchanged, so `Graph`, `validate`, `impact`, `deps`, `touched`, `show`, `index`, and JSON output need no semantic changes.

- `FrontMatter` becomes a shape-detecting deserializer over the first YAML frontmatter block:
  - Top-level `refs:` present → **legacy path**, deserialize the existing `RefsBlock`.
  - Top-level `type:` or `kusara:` present → **OKF path**, deserialize a new struct holding the flat OKF reserved keys plus a nested `KusaraBlock` (the graph fields).
  - Both `refs:` and `kusara:`/`type:` present → **error** (ambiguous shape).
  - Neither present → treated as "no kusara metadata" exactly as today.
- Both paths normalize into the same internal `Doc`. New OKF fields (`description`, `resource`, `tags`, `timestamp`) are added to `Doc` as optional data carried through to `show`/JSON/index output.
- The HTML source format (`<script type="application/kusara+yaml">`) reuses the identical detection and both-shape support — the change is entirely inside the YAML parse, not the extraction.

### `kusara migrate` (new subcommand)

- Walks the doc tree (same walk as `validate`/`index`).
- Rewrites each legacy `refs:` doc into the flat + `kusara:` shape:
  - `refs.kind` → top-level `type`.
  - `refs.title` → top-level `title`.
  - Remaining `refs.*` fields → under `kusara:`.
  - OKF reserved keys are left absent unless already present.
- Idempotent: already-migrated (OKF-shape) docs are left untouched.
- Handles both Markdown and HTML variants.
- `--dry-run` lists the files that would change and writes nothing; default writes in place.
- **Known limitation, surfaced in `--help` and this spec:** YAML round-trip drops comments inside the frontmatter block. Body content below the frontmatter is untouched.

### Validation & deprecation

- `type` is required and validated against `docs/kinds.md` (same rule the old `kind` had).
- `kusara.id` is required and must be unique repo-wide (same as old `id`).
- Legacy `refs:` docs continue to validate successfully but emit a **deprecation warning** naming the file and pointing at `kusara migrate`. Planned removal is a release or two out (tracked separately, not in this spec).
- Unknown **top-level** keys are tolerated (OKF forward-compatibility — arbitrary keys are allowed).
- Unknown keys **inside `kusara:`** are rejected — it is kusara's namespace and typos there are almost always mistakes.

### Emitters

- `kusara index` writes generated INDEX docs in the new shape (`type: index`, `kusara: { generated: true, indexes_kind: … }`).
- JSON graph output (`ai/graph.json`) and `kusara show` surface the new OKF reserved fields where useful.

## Testing strategy

- Dual-read: a legacy `refs:` doc and an OKF-shape doc parse to the same `Doc`.
- Ambiguity: a doc with both `refs:` and `kusara:` is rejected with a clear error.
- HTML variant: both shapes parse inside `<script type="application/kusara+yaml">`.
- `migrate`: legacy → OKF output is correct; running twice is a no-op (idempotent); `--dry-run` writes nothing and lists the files that would change.
- Deprecation: validating a legacy doc emits the warning and still succeeds.
- OKF keys: `description`/`resource`/`tags`/`timestamp` round-trip through parse → `Doc` → `show`/JSON.
- `kusara.*` unknown-key rejection; top-level unknown-key tolerance.

## Documentation updates (part of the work)

- Rewrite `docs/refs.md` (canonical schema) to the new shape; keep a "legacy `refs:` (deprecated)" appendix during the transition.
- Note the `kind` → `type` rename in `docs/kinds.md`.
- Update the two kusara skills (`refs-schema`, `kinds-manifest`) and their verbatim `references/` copies.
- Update README examples.

## Non-goals (YAGNI)

- No OKF export/import of full OKF *bundles* (this is native adoption, not a bundle bridge).
- No adoption of OKF's path-derived IDs.
- No adoption of OKF's reserved `index.md` / `log.md` filenames (kusara keeps `map.md` and its per-kind index outputs).
- No validation of the *contents* of `resource` / `tags` / `timestamp` (freeform, optional).
- No prose-link-as-relation parsing (kusara relations stay typed under `kusara:`).

## Open questions

None blocking. The deprecation-removal release is deferred to a future decision.
