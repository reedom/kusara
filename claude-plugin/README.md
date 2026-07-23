# kusara Claude Code plugin

Document-management workflows for [kusara](../README.md), packaged as a Claude Code plugin.

The plugin lives inside the kusara repo and is distributed via the repo's `.claude-plugin/marketplace.json`. Users install through the Claude Code marketplace mechanism; developers hacking on kusara itself can load it directly with `--plugin-dir`.

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
cd kusara
claude --plugin-dir ./claude-plugin
```

## What's inside

### Slash commands

- `/kusara:setup [--upgrade] [--from-source <path>]` — install or upgrade the `kusara` CLI binary via `cargo install`. User-confirmed; no silent builds. Run this once before the others. The other commands pre-flight `command -v kusara` and point here on miss.
- `/kusara:sync [files...] [--ref] [--dry-run]` — main workflow. Auto-detects (or accepts) changed files, runs `kusara validate`, computes affected docs via `kusara touched`/`kusara impact`, applies maintenance edits (frontmatter by default; prose too unless `--ref`), regenerates indexes, re-validates. Fans out to parallel `doc-maintainer` agents when 4+ docs are affected.
- `/kusara:check [--quiet]` — read-only audit. Runs `kusara validate` and groups findings into broken IDs / schema errors / module conflicts.
- `/kusara:add-ref <file> [--kind <kind>] [--id <id>]` — guided `kusara:` frontmatter authoring for a single file.

All three commands set `disable-model-invocation: true`. They run only on explicit user invocation.

### Skills (auto-loaded)

- `refs-schema` — authoritative schema for the OKF-native frontmatter shape (flat `type:` + `kusara:` block). Legacy `refs:`-wrapped frontmatter is still read but deprecated (`kusara migrate` rewrites it in place). Triggers when editing or interpreting kusara frontmatter fields.
- `kinds-manifest` — knowledge of `${KUSARA_DOC_ROOT}/kinds.md` format. Triggers when picking a kind or answering kind/path-glob questions.

Both ship with a `references/` directory carrying verbatim copies of the kusara repo's `docs/refs.md` and `docs/kinds.md` for offline accuracy.

### Agents

- `doc-maintainer` — updates exactly one Markdown doc to reflect changes in given source files. Fanned out in parallel by `/kusara:sync` when many docs are affected. Always called with explicit `(changed_files, affected_doc_path, mode)` — never autonomously.

## Prerequisites

- `kusara` binary on `$PATH`. Easiest: run `/kusara:setup` once. Manual: `cargo install --path .` from the repo root.
- Rust toolchain (`cargo`) for building from source. Install via [rustup](https://rustup.rs).
- Project repo carrying `${KUSARA_DOC_ROOT}/kinds.md` (default `docs/kinds.md`) and Markdown docs with OKF-native frontmatter (flat `type:` + `kusara:` block; HTML docs carry the same YAML in a `<script type="application/kusara+yaml">` data block). Legacy `refs:`-wrapped frontmatter is still read but deprecated — run `kusara migrate` to rewrite it.

## Typical loop

```text
edit code / docs
        ↓
/kusara:sync                # pre-validate → touched → maintain → index → re-validate
        ↓
review diff, commit
```

Use `/kusara:check` between rounds for a read-only health pulse.

## Recommended project hooks

For a kusara-managed repo, wire the CLI's hook adapters into the project's
`.claude/settings.json` so every Claude Code session in that repo keeps the doc
graph honest without anyone remembering to run `/kusara:sync`:

- `SessionStart` — onboarding guard. If the `kusara` binary or this plugin is
  missing, the session opens with an instruction to install them instead of
  failing quietly later.
- `PostToolUse` (`Edit|Write|MultiEdit`) — `kusara hook postedit` journals each
  edited file. No graph load, no output; near-zero cost per edit.
- `Stop` — `kusara hook stop` batch-checks the turn's edits once: validate
  failures block the stop (downgraded to plain context on the second attempt,
  so it cannot loop), and otherwise the affected docs of record are injected
  as non-blocking context. Silent when nothing relevant changed.

Every command is guarded with `command -v kusara`, so teammates who cloned the
repo but have not installed the CLI see one setup nudge at session start
instead of an error on every edit.

```json
{
  "hooks": {
    "SessionStart": [
      {
        "matcher": "startup",
        "hooks": [
          {
            "type": "command",
            "command": "missing=\"\"; command -v kusara >/dev/null 2>&1 || missing=\"kusara CLI is not installed. Run /kusara:setup or \\`cargo install kusara --locked\\` (used to validate and sync the doc graph; see docs/kinds.md)\"; grep -q '\"kusara@kusara\"' \"$HOME/.claude/plugins/installed_plugins.json\" 2>/dev/null || missing=\"$missing${missing:+\\n}kusara plugin is not installed. Run \\`/plugin install kusara@kusara\\`\"; if [ -n \"$missing\" ]; then printf \"%b\\n\" \"$missing\" >&2; exit 2; fi"
          }
        ]
      }
    ],
    "PostToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [
          {
            "type": "command",
            "command": "command -v kusara >/dev/null 2>&1 && kusara hook postedit || true"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "command -v kusara >/dev/null 2>&1 && kusara hook stop --note 'Manifest: docs/kinds.md / schema: kusara:refs-schema skill' || true"
          }
        ]
      }
    ]
  }
}
```

Caveats:

- The plugin-installed check greps Claude Code's internal
  `~/.claude/plugins/installed_plugins.json`, which is an implementation
  detail: it can false-negative for developers loading the plugin via
  `--plugin-dir`, and its location or format may change in future Claude Code
  releases. Treat that half of the guard as a best-effort nudge; drop it if it
  misfires for your team.
- These hooks belong in a kusara-managed project's own settings, not in this
  plugin. Plugin-bundled hooks would fire in every repo the user opens, and in
  a repo without `docs/kinds.md` the Stop adapter reports "cannot check this
  turn's edits" on every turn that edits a file.

## Layout

```text
.claude-plugin/
  marketplace.json    ← marketplace listing (single-plugin repo)
claude-plugin/
  .claude-plugin/
    plugin.json       ← plugin manifest
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
