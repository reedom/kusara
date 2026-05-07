# kusara Claude Code plugin

Document-management workflows for [kusara](../README.md), packaged as a Claude Code plugin.

The plugin lives inside the kusara repo so anyone cloning gets it auto-discovered when they open the repo with Claude Code. The repo also ships a `marketplace.json` so users can install via Claude Code's marketplace mechanism without cloning.

## Install

Pick one:

**1. As a Claude Code marketplace** (recommended for users who only want the slash commands):

```text
/plugin marketplace add reedom/kusara
/plugin install kusara@kusara
```

**2. From the local repo** (developers hacking on kusara itself):

```sh
git clone https://github.com/reedom/kusara
# then open the cloned directory in Claude Code — the plugin auto-discovers.
```

## What's inside

### Slash commands

- `/kusara:setup [--upgrade] [--from-source <path>]` — install or upgrade the `kusara` CLI binary via `cargo install`. User-confirmed; no silent builds. Run this once before the others. The other commands pre-flight `command -v kusara` and point here on miss.
- `/kusara:sync [files...] [--ref] [--dry-run]` — main workflow. Auto-detects (or accepts) changed files, runs `kusara validate`, computes affected docs via `kusara touched`/`kusara impact`, applies maintenance edits (frontmatter by default; prose too unless `--ref`), regenerates indexes, re-validates. Fans out to parallel `doc-maintainer` agents when 4+ docs are affected.
- `/kusara:check [--quiet]` — read-only audit. Runs `kusara validate` and groups findings into broken IDs / schema errors / module conflicts.
- `/kusara:add-ref <file> [--kind <kind>] [--id <id>]` — guided `refs:` frontmatter authoring for a single file.

All three commands set `disable-model-invocation: true`. They run only on explicit user invocation.

### Skills (auto-loaded)

- `refs-schema` — authoritative schema for the `refs:` frontmatter block. Triggers when editing or interpreting `refs:` fields.
- `kinds-manifest` — knowledge of `${KUSARA_DOC_ROOT}/kinds.md` format. Triggers when picking a kind or answering kind/path-glob questions.

Both ship with a `references/` directory carrying verbatim copies of the kusara repo's `docs/refs.md` and `docs/kinds.md` for offline accuracy.

### Agents

- `doc-maintainer` — updates exactly one Markdown doc to reflect changes in given source files. Fanned out in parallel by `/kusara:sync` when many docs are affected. Always called with explicit `(changed_files, affected_doc_path, mode)` — never autonomously.

## Prerequisites

- `kusara` binary on `$PATH`. Easiest: run `/kusara:setup` once. Manual: `cargo install --path .` from the repo root.
- Rust toolchain (`cargo`) for building from source. Install via [rustup](https://rustup.rs).
- Project repo carrying `${KUSARA_DOC_ROOT}/kinds.md` (default `docs/kinds.md`) and Markdown docs with `refs:` frontmatter.

## Typical loop

```text
edit code / docs
        ↓
/kusara:sync                # pre-validate → touched → maintain → index → re-validate
        ↓
review diff, commit
```

Use `/kusara:check` between rounds for a read-only health pulse.

## Layout

```text
.claude-plugin/
  plugin.json         ← plugin manifest
  marketplace.json    ← marketplace listing (single-plugin repo)
  README.md           ← you are here
commands/
  setup.md
  sync.md
  check.md
  add-ref.md
skills/
  refs-schema/
    SKILL.md
    references/
      refs.md
      relations-cheatsheet.md
  kinds-manifest/
    SKILL.md
    references/
      kinds.md
agents/
  doc-maintainer.md
```

## License

MIT — same as the kusara project. See [`../LICENSE`](../LICENSE).
