# kssni

Cross-reference graph tooling for Markdown specs and docs.

`kssni` reads YAML `refs:` frontmatter across a repository and answers dependency, doc-of-record, and per-kind index queries. Hand-edited frontmatter, machine-checked graph.

- Schema and conventions: [`docs/refs.md`](docs/refs.md)
- Kind manifest: [`docs/kinds.md`](docs/kinds.md)

## Install

```sh
cargo install --path .
```

Binary name: `kssni`.

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
   kssni validate
   kssni list
   kssni show ref:auth-overview
   kssni impact ref:auth-overview
   kssni touched src/auth/session.rs
   kssni index map           # writes map.md + ai/graph.json + ai/modules.md
   kssni index               # writes per-kind index.md files
   ```

## Subcommands

```sh
kssni validate
kssni impact <id> [<id>...] [--depth <N>] [--include-related]
kssni deps   <id> [<id>...] [--depth <N>] [--include-related]
kssni show   <id>
kssni touched <file> [<file>...] [--no-closure]
kssni list
kssni index map
kssni index
```

`--root <DIR>` (global): override the repo root (default: cwd).

## Configuration

`KSSNI_DOC_ROOT` (default `docs`) points at the directory containing `kinds.md` and where `map.md` / `ai/graph.json` are written.

## Claude Code plugin

This repo also ships a Claude Code plugin (`.claude-plugin/`) with `/kssni:sync`, `/kssni:check`, and `/kssni:add-ref` slash commands plus auto-loaded schema skills. See [`.claude-plugin/README.md`](.claude-plugin/README.md).

## License

MIT
