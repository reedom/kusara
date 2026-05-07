# kusara

Cross-reference graph tooling for Markdown specs and docs.

`kusara` reads YAML `refs:` frontmatter across a repository and answers dependency, doc-of-record, and per-kind index queries. Hand-edited frontmatter, machine-checked graph.

- Schema and conventions: [`docs/refs.md`](docs/refs.md)
- Kind manifest: [`docs/kinds.md`](docs/kinds.md)

## Install

```sh
cargo install --path .
```

Binary name: `kusara`.

## Quick start

1. Copy `docs/kinds.md` from this repo into your project's doc root and edit the kinds list to match your project layout.
2. Add `refs:` frontmatter to any Markdown file you want tracked. Minimal example:

   ```markdown
   ---
   refs:
     id: ref:auth-overview
     kind: reference
     title: "Auth subsystem overview"
     modules:
       - src/auth/
   ---

   # Auth Overview
   ...
   ```

3. Run:

   ```sh
   kusara validate
   kusara list
   kusara show ref:auth-overview
   kusara impact ref:auth-overview
   kusara touched src/auth/session.rs
   kusara index map           # writes map.md + ai/graph.json + ai/modules.md
   kusara index               # writes per-kind index.md files
   ```

## Subcommands

```sh
kusara validate
kusara impact <id> [<id>...] [--depth <N>] [--include-related]
kusara deps   <id> [<id>...] [--depth <N>] [--include-related]
kusara show   <id>
kusara touched <file> [<file>...] [--no-closure]
kusara list
kusara index map
kusara index
```

`--root <DIR>` (global): override the repo root (default: cwd).

## Configuration

`KUSARA_DOC_ROOT` (default `docs`) points at the directory containing `kinds.md` and where `map.md` / `ai/graph.json` are written.

## Claude Code plugin

This repo also ships a Claude Code plugin (`claude-plugin/`, listed via `.claude-plugin/marketplace.json`) with `/kusara:sync`, `/kusara:check`, and `/kusara:add-ref` slash commands plus auto-loaded schema skills. See [`claude-plugin/README.md`](claude-plugin/README.md).

## License

MIT
