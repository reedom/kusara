---
description: Auto-maintain docs related to changed files using kusara (frontmatter + prose updates).
argument-hint: "[<file>...] [--ref] [--dry-run]"
allowed-tools: Bash, Read, Edit, Write, Grep, Glob, Agent
disable-model-invocation: true
---

# /kusara:sync — maintain related docs after changes

You are running the kusara sync workflow. Apply the steps below in order. Do not skip steps. Do not ask the user mid-workflow unless a hard ambiguity blocks progress.

## Inputs

Arguments: `$ARGUMENTS`

Flags:
- `--ref` → frontmatter-only mode (do **not** edit prose).
- `--dry-run` → produce the edit plan + diff preview, apply nothing.

Positional args = explicit list of changed files. If no positional args remain after stripping flags, auto-detect changed files via `git status --porcelain` (uncommitted + staged, excluding deleted).

## Step 0 — Pre-flight: kusara present?

Run `command -v kusara`. If it exits non-zero:
- Print: `kusara binary not found on $PATH. Run /kusara:setup to install.`
- Stop. Do not proceed.

## Step 1 — Resolve changed files

1. Parse `$ARGUMENTS`. Separate flags from positional file paths.
2. If no positional paths: run `git status --porcelain | awk '$1 !~ /^D/ {print $2}'` to collect changed paths. Filter to existing files.
3. If the list is empty, report "no changes detected" and stop.
4. Print the resolved file list (one per line) before continuing.

## Step 2 — Pre-flight validate

Run `kusara validate`. If it exits non-zero:
- Print the validator output verbatim.
- Stop. Tell the user to fix validation errors first (suggest `/kusara:check` for a focused report).

Rationale: don't compound damage on a broken graph.

## Step 3 — Compute affected docs

Run `kusara touched <file1> <file2> ...` with the resolved file list. This returns docs whose `modules:` cover the changed files (closure included by default).

For each Markdown file in the input that is itself a doc tracked by kusara (has a `refs:` block), also run `kusara impact <id>` for that doc's id and merge the result into the affected set.

Result: a deduplicated set of doc paths + their ids. Call this `AFFECTED`.

If `AFFECTED` is empty, report "no related docs need maintenance" and stop.

Print `AFFECTED` (path — id — title).

## Step 4 — Dispatch edits

Threshold: if `len(AFFECTED) >= 4`, dispatch parallel `doc-maintainer` agents — one per affected doc. Otherwise, edit inline.

For each affected doc, the work unit is:

> Given (changed_files, affected_doc_path, mode={prose|ref}), update affected_doc to reflect the changes. Update `refs:` frontmatter (`related:`, `depends_on:`, `modules:`) as warranted. In `prose` mode, also update body text — e.g., "See also" sections, references to renamed APIs, mention of new modules. Do **not** invent facts: only adjust what is supported by the diff of changed_files. Preserve existing IDs and ordering where unchanged.

Mode: `--ref` flag → `ref`, otherwise → `prose`.

### Inline path (len(AFFECTED) < 4)

For each affected doc, in sequence:
1. Read the doc.
2. Read the changed files (or `git diff` of them) to ground the edit.
3. Reference the `refs-schema` skill before editing frontmatter.
4. Apply edits via `Edit`. If `--dry-run`, print the proposed diff instead.

### Parallel path (len(AFFECTED) >= 4)

Dispatch agents concurrently in a single message with multiple `Agent` tool uses (subagent_type = `doc-maintainer`). Each agent gets its own `(changed_files, affected_doc_path, mode)` tuple. Wait for all to return.

If `--dry-run`, instruct each agent to return a unified diff instead of writing.

## Step 5 — Regenerate indexes

Skip if `--dry-run`.

Run:
```sh
kusara index map
kusara index
```

Both must succeed. If either fails, surface the error and stop.

## Step 6 — Post-flight validate

Skip if `--dry-run`.

Run `kusara validate`. If non-zero:
- Print errors.
- Show which docs were edited in step 4.
- Recommend reverting via `git checkout -- <files>` and re-running with `--dry-run` to diagnose.

## Step 7 — Summary

Print a concise report:
- Changed files (count, list)
- Affected docs (count)
- Edits applied per doc (one-line each: `path — frontmatter|prose|both`)
- Indexes regenerated: yes/no
- Validate: pass/fail

End. Do not commit. Leave staging to the user.

## Notes

- Never bypass `kusara validate`. Both the pre-flight and post-flight gates are mandatory.
- Do not fabricate `id`s, `depends_on`, or `modules` paths. Every value must trace to existing graph data or the diff being processed.
- Respect `disable-model-invocation: true` — this command runs only on explicit user invocation. Sub-agents you dispatch must not re-invoke `/kusara:sync`.
