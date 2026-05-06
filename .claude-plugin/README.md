# kssni Claude Code plugin

Document-management workflows for [kssni](../README.md), packaged as a Claude Code plugin.

The plugin lives inside the kssni repo so anyone cloning gets it auto-discovered when they open the repo with Claude Code.

## What's inside

### Slash commands

- `/kssni:setup [--upgrade] [--from-source <path>]` — install or upgrade the `kssni` CLI binary via `cargo install`. User-confirmed; no silent builds. Run this once before the others. The other commands pre-flight `command -v kssni` and point here on miss.
- `/kssni:sync [files...] [--ref] [--dry-run]` — main workflow. Auto-detects (or accepts) changed files, runs `kssni validate`, computes affected docs via `kssni touched`/`kssni impact`, applies maintenance edits (frontmatter by default; prose too unless `--ref`), regenerates indexes, re-validates. Fans out to parallel `doc-maintainer` agents when 4+ docs are affected.
- `/kssni:check [--quiet]` — read-only audit. Runs `kssni validate` and groups findings into broken IDs / schema errors / module conflicts.
- `/kssni:add-ref <file> [--kind <kind>] [--id <id>]` — guided `refs:` frontmatter authoring for a single file.

All three commands set `disable-model-invocation: true`. They run only on explicit user invocation.

### Skills (auto-loaded)

- `refs-schema` — authoritative schema for the `refs:` frontmatter block. Triggers when editing or interpreting `refs:` fields.
- `kinds-manifest` — knowledge of `${KSSNI_DOC_ROOT}/kinds.md` format. Triggers when picking a kind or answering kind/path-glob questions.

Both ship with a `references/` directory carrying verbatim copies of the kssni repo's `docs/refs.md` and `docs/kinds.md` for offline accuracy.

### Agents

- `doc-maintainer` — updates exactly one Markdown doc to reflect changes in given source files. Fanned out in parallel by `/kssni:sync` when many docs are affected. Always called with explicit `(changed_files, affected_doc_path, mode)` — never autonomously.

## Prerequisites

- `kssni` binary on `$PATH`. Easiest: run `/kssni:setup` once. Manual: `cargo install --path .` from the repo root.
- Rust toolchain (`cargo`) for building from source. Install via [rustup](https://rustup.rs).
- Project repo carrying `${KSSNI_DOC_ROOT}/kinds.md` (default `docs/kinds.md`) and Markdown docs with `refs:` frontmatter.

## Typical loop

```text
edit code / docs
        ↓
/kssni:sync                # pre-validate → touched → maintain → index → re-validate
        ↓
review diff, commit
```

Use `/kssni:check` between rounds for a read-only health pulse.

## Layout

```text
.claude-plugin/
  plugin.json
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

MIT — same as the kssni project. See [`../LICENSE`](../LICENSE).
