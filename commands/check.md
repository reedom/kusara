---
description: Run kssni validate and produce a focused, actionable report. Read-only.
argument-hint: "[--quiet]"
allowed-tools: Bash, Read, Grep
disable-model-invocation: true
---

# /kssni:check — read-only audit

Run `kssni validate` and convert its output into an actionable report. Make **no edits**.

## Steps

0. Pre-flight: run `command -v kssni`. If non-zero, print `kssni binary not found on $PATH. Run /kssni:setup to install.` and stop.
1. Run `kssni validate`. Capture stdout + stderr + exit code.
2. If exit 0:
   - With `--quiet`: print "kssni: ok" and stop.
   - Without `--quiet`: also run `kssni list` and summarize counts per kind.
3. If exit non-zero, parse the output and group findings:
   - **Broken IDs** — `implements`/`depends_on`/`related` pointing at non-existent IDs.
   - **Schema errors** — wrong `kind`, malformed `id`, unknown fields.
   - **Module conflicts** — same module path claimed by incompatible docs.
   - **Other** — anything that does not fit above.
4. For each finding, output one line: `path:line — category — short fix hint`. Use the `refs-schema` skill knowledge to suggest fixes.
5. End with a one-line tally: `errors: N, warnings: M`.

Do not edit files. Do not run `kssni index` or `kssni index map`. Do not call `/kssni:sync`.
