---
description: Install or upgrade the kusara CLI binary. User-confirmed; no silent builds.
argument-hint: "[--upgrade] [--from-source <path>]"
allowed-tools: Bash, Read
disable-model-invocation: true
---

# /kusara:setup — install or upgrade the kusara CLI

Install the `kusara` binary so the other `/kusara:*` commands can run. Always show the user what you are about to do and wait for explicit confirmation before invoking `cargo install`.

## Inputs

Arguments: `$ARGUMENTS`

Flags:
- `--upgrade` → reinstall even if already present (force fresh build).
- `--from-source <path>` → use that path as the cargo source. Default discovery order is in Step 2.

## Step 1 — Detect current state

1. Run `command -v kusara` and capture the path (if any).
2. If found, run `kusara --version` and capture output.
3. Run `command -v cargo`. Capture path + `cargo --version` if found.
4. Print a status block:
   ```
   kusara:  <path or "(not installed)"> [<version if any>]
   cargo:  <path or "(not installed)"> [<version if any>]
   ```

## Step 2 — Decide source path

Discover the source directory for `cargo install --path <dir>`:

1. If `--from-source <path>` was given, use it. Verify `<path>/Cargo.toml` exists and its `[package].name` is `kusara`.
2. Else if `$CLAUDE_PLUGIN_ROOT/Cargo.toml` exists and its `[package].name` is `kusara`, use `$CLAUDE_PLUGIN_ROOT` (the plugin lives in the kusara source repo — common case).
3. Else, source path is unknown. Tell the user:
   - "Plugin is installed standalone (not in the kusara source repo). Provide the path with `--from-source <path>`, or clone the repo:"
   - `git clone https://github.com/reedom/kusara && /kusara:setup --from-source ./kusara`
   - Stop.

Print the resolved source path before continuing.

## Step 3 — Decide whether to install

- If `kusara` is **not** installed → proceed to install.
- If `kusara` **is** installed and `--upgrade` was **not** given → print "kusara already installed at <path> (<version>). Pass `--upgrade` to force a reinstall." and stop with success.
- If `--upgrade` was given → proceed to reinstall.

## Step 4 — Confirm with the user

Before running `cargo install`, print the exact command you will execute, e.g.:

```
About to run:
  cargo install --path /Users/.../kusara --locked
This will compile kusara and place the binary in $CARGO_HOME/bin (typically ~/.cargo/bin).
```

Ask: "Proceed? (yes/no)" — wait for the user's reply. Only continue on an explicit affirmative.

If `cargo` was not found in Step 1, do **not** ask for confirmation. Instead print:
- "cargo is not on $PATH. Install Rust toolchain via https://rustup.rs first, then re-run /kusara:setup."
- Stop.

## Step 5 — Install

Run:

```sh
cargo install --path "<resolved-source-path>" --locked
```

If `--upgrade`, append `--force`:

```sh
cargo install --path "<resolved-source-path>" --locked --force
```

Stream the output to the user. Do not capture-and-replay; let the build progress show live.

## Step 6 — Verify

1. Run `command -v kusara`. Must succeed.
2. Run `kusara --version`. Must print a version.
3. If both succeed, print:
   ```
   kusara installed at <path> (<version>).
   Next: try /kusara:check to validate the project's docs.
   ```
4. If verification fails, print the captured output and tell the user to check `$PATH` includes `$CARGO_HOME/bin` (typically `~/.cargo/bin`).

## Hard rules

- Never run `cargo install` without an explicit user "yes" in Step 4 (unless skipping for the cargo-missing branch, which only prints).
- Never run `sudo`. Never modify `$PATH` for the user. Never write to shell rc files.
- Never download arbitrary binaries from the network. Source path must resolve to a local directory whose `Cargo.toml` declares `name = "kusara"`.
- This command does not call any other `/kusara:*` command and is not called by them; the others print a hint pointing here when the binary is missing.
