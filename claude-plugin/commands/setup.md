---
description: Install or upgrade the kusara CLI binary from crates.io. User-confirmed; no silent installs.
argument-hint: "[--upgrade] [--from-source <path>]"
allowed-tools: Bash, Read
disable-model-invocation: true
---

# /kusara:setup — install or upgrade the kusara CLI

Install the `kusara` binary from crates.io so the other `/kusara:*` commands can run. Always show the user what you are about to do and wait for explicit confirmation before invoking `cargo install`.

## Inputs

Arguments: `$ARGUMENTS`

Flags:
- `--upgrade` → reinstall even if already present (force fresh build).
- `--from-source <path>` → install from a local source directory instead of crates.io. Path's `Cargo.toml` must have `[package].name = "kusara"`.

## Step 1 — Detect current state

1. Run `command -v kusara` and capture the path (if any).
2. If found, run `kusara --version` and capture output.
3. Run `command -v cargo`. Capture path + `cargo --version` if found.
4. Print a status block:
   ```
   kusara:  <path or "(not installed)"> [<version if any>]
   cargo:  <path or "(not installed)"> [<version if any>]
   ```

## Step 2 — Decide install source

Default: install from crates.io (`cargo install kusara --locked`).

If `--from-source <path>` was given:
1. Verify `<path>/Cargo.toml` exists and `[package].name` is `kusara`. If not, print the mismatch and stop.
2. Use `cargo install --path <path> --locked` instead.

Print the resolved install command before continuing.

## Step 3 — Decide whether to install

- If `kusara` is **not** installed → proceed to install.
- If `kusara` **is** installed and `--upgrade` was **not** given → print "kusara already installed at <path> (<version>). Pass `--upgrade` to force a reinstall." and stop with success.
- If `--upgrade` was given → proceed to reinstall.

## Step 4 — Confirm with the user

If `cargo` was not found in Step 1, do **not** ask for confirmation. Print:
- "cargo is not on $PATH. Install Rust toolchain via https://rustup.rs first, then re-run /kusara:setup."
- Stop.

Otherwise, print the exact command you will execute, e.g.:

```
About to run:
  cargo install kusara --locked
This will download and compile kusara from crates.io and place the binary in $CARGO_HOME/bin (typically ~/.cargo/bin).
```

Or for `--from-source`:

```
About to run:
  cargo install --path /Users/.../kusara --locked
```

Ask: "Proceed? (yes/no)" — wait for the user's reply. Only continue on an explicit affirmative.

## Step 5 — Install

Default (crates.io):

```sh
cargo install kusara --locked
```

With `--from-source <path>`:

```sh
cargo install --path "<path>" --locked
```

If `--upgrade`, append `--force` to either form.

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
- Default install source is crates.io. `--from-source` only accepts a local directory whose `Cargo.toml` declares `name = "kusara"`.
- This command does not call any other `/kusara:*` command and is not called by them; the others print a hint pointing here when the binary is missing.
