# Which relation field to use?

Decision table for picking between `implements`, `depends_on`, `related`, `provides`, `modules`.

| You want to express…                                                  | Use            | Strength |
|-----------------------------------------------------------------------|----------------|----------|
| "This doc fulfils requirement X."                                     | `implements`   | hard     |
| "This doc satisfies a higher-level spec/FR."                          | `implements`   | hard     |
| "Without doc X, this doc is incorrect."                               | `depends_on`   | hard     |
| "Reading X first is mandatory to use this correctly."                 | `depends_on`   | hard     |
| "X is a useful adjacent topic; not required."                         | `related`      | soft     |
| "I am the design of record for `src/foo/`."                           | `modules`      | hard     |
| "I declare requirement IDs that have no file of their own."           | `provides`     | hard     |

## Anti-patterns

- Listing every doc you mention in prose under `related:` → noise. Reserve `related:` for genuine see-also.
- Putting `implements:` in BOTH directions → choose one direction (downstream → upstream). The graph is single-directional for hard edges.
- Using `depends_on:` for soft "see-also" → use `related:` instead. `depends_on` participates in `kssni impact`; `related` only does so with `--include-related`.
- Setting `generated:` or `indexes_kind:` by hand → never. Index files are written by `kssni index` only.

## Quick example

```yaml
---
refs:
  id: ref:auth-session
  kind: reference
  title: "Auth session storage"
  implements:
    - fr:01-auth         # this reference doc satisfies FR 01
  depends_on:
    - spec:auth          # the auth spec must exist
  related:
    - ref:rbac           # adjacent topic
  modules:
    - src/auth/session.rs
    - src/auth/storage/
---
```
