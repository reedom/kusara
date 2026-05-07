---
description: Author or update the kusara `refs:` frontmatter block on a Markdown file.
argument-hint: "<file> [--kind <kind>] [--id <id>]"
allowed-tools: Bash, Read, Edit, Write, Grep, Glob
disable-model-invocation: true
---

# /kusara:add-ref — guided frontmatter authoring

Add or update a `refs:` block in `<file>`. Schema knowledge comes from the `refs-schema` skill; kind list from `kinds-manifest`.

## Steps

0. Pre-flight: run `command -v kusara`. If non-zero, print `kusara binary not found on $PATH. Run /kusara:setup to install.` and stop.
1. Parse `$ARGUMENTS`. Require exactly one positional `<file>`. If missing or file does not exist, abort with usage hint.
2. Reference the `refs-schema` and `kinds-manifest` skills before any edit. Do not improvise the schema.
3. Read `<file>`. Detect:
   - Has YAML frontmatter? Has a `refs:` block inside it?
   - If `refs:` exists, this is an **update**; otherwise an **insert**.
4. Determine the target kind:
   - If `--kind` provided, use it. Validate against `kinds-manifest` skill.
   - Else, infer from the file's path against the `path_globs` in `${KUSARA_DOC_ROOT}/kinds.md` (default `docs/kinds.md`). If no match, ask the user once for the kind.
5. Determine the `id`:
   - If `--id` provided, use it.
   - Else derive from the kind's `id_pattern` and the file's slug.
   - Verify uniqueness: run `kusara list` and confirm the id is not present.
6. For an **insert**: prepend a YAML frontmatter block with the minimal valid `refs:` (`id`, `kind`, `title`). Title defaults to the first H1 in the file or the filename.
7. For an **update**: merge fields conservatively. Never remove existing `implements`, `depends_on`, `related`, `provides`, or `modules` entries unless the user asked. Add only what was requested.
8. Run `kusara validate`. If it fails on the edited file, revert via `git checkout -- <file>` and report the error.
9. Print the resulting `refs:` block.

Constraints:
- Never set `generated:` or `indexes_kind:` — those are written by `kusara index`.
- Do not edit any file other than `<file>`.
- Do not run `kusara index` or `kusara index map`.
