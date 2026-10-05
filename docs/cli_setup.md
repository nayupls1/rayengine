# Install the checkout CLI

Setup installs **the source in this checkout**, with its workspace package version
(currently 0.0.3), and makes `rayengine` available in subsequent terminals. It
uses `cargo install --locked --path crates/rayengine-cli --bin rayengine` with an
explicit root and the local Rust host target. Re-running builds updated checkout
sources even when their package version has not changed. Cargo keeps its usual
installation tracking and refuses conflicting binaries from another package;
setup does not force an overwrite.

Install Rust **1.89+** with [rustup](https://rustup.rs) first, then reopen your
terminal so `rustc` and `cargo` are available. The CLI does not link raylib or
require a display. You need the Rust host linker (a C toolchain on Linux,
Xcode Command Line Tools on macOS, or Visual Studio C++ Build Tools for an MSVC
Rust toolchain on Windows). Native **game** prerequisites are separate; see the
[SDK quickstart](../crates/rayengine/docs/quickstart.md).

Run from a trusted repository checkout. Setup requires no administrator rights
and only modifies user configuration. Installing Rust's host linker may need a
separate system installation.

## Linux and macOS

From the repository root, in Bash, Zsh, Fish or a POSIX shell:

```sh
bash scripts/setup.sh
```

Setup uses your login shell from `$SHELL`. Select a different terminal shell
with `--shell bash`, `--shell zsh`, `--shell fish` or `--shell sh`. It prints an
activation command for the current terminal. Copy that command, then run:

```sh
rayengine --help
command -v rayengine
```

A subprocess cannot change its parent shell's environment. To verify persistence,
open a **new terminal window**, without first exporting PATH in it, and run those
same commands. The command path should be the printed install root's
`bin/rayengine`. Close/reopen an existing terminal application if it keeps an old
environment. If another installation wins lookup, inspect its startup PATH and
remove or reorder that older entry as appropriate.

Setup writes a clearly marked block to these files:

| Shell | Startup configuration |
| --- | --- |
| Bash | `~/.bashrc`, plus the first existing `~/.bash_profile`, `~/.bash_login`, or `~/.profile` (creates `.profile` if none exist) |
| Zsh | `.zshrc` and `.zprofile` under `$ZDOTDIR`, or `~` when unset |
| Fish | `$XDG_CONFIG_HOME/fish/conf.d/rayengine-cli.fish`, or `~/.config/fish/conf.d/rayengine-cli.fish` |
| POSIX sh | `~/.profile` (login sessions) |

Each block adds the actual install root's `bin` directory only if it is absent
from that session's PATH. Re-running setup replaces its own block, preserving
other content, file permissions and symlinks. Blocks are written even if the
installer's current PATH already contains that directory: a temporary export
alone does not establish persistence. If your startup configuration returns or
exits before reaching the block, move the whole block before that return, then
verify a new terminal. A non-login POSIX shell needs its parent login session to
have loaded `.profile`.

Other shells require manual configuration; setup reports the supported choices
without installing or editing PATH. Run with `--shell sh` to configure a POSIX
login session, or select the shell used by your terminal. Paths with spaces,
quotes and shell metacharacters are quoted literally. Colons and newlines in
Unix install roots are rejected because they cannot safely form PATH entries.

## Windows (PowerShell)

From the repository root, run in Windows PowerShell 5.1 or PowerShell 7:

```powershell
& .\scripts\setup.ps1
rayengine --help
Get-Command rayengine -All
```

If execution policy blocks this trusted checkout script, allow it only for this
invocation/session. For example:

```powershell
powershell.exe -NoProfile -ExecutionPolicy Bypass -File .\scripts\setup.ps1
```

When launching a separate PowerShell process this way, its PATH activation does
not propagate to the parent terminal. Close **all terminal application windows**
and open a new terminal from the Start menu, then run `rayengine --help` and
`Get-Command rayengine -All`. Existing terminal applications and their child
shells can inherit stale PATH values; sign out and back in if an application
still retains the old environment.

Setup preserves the raw user PATH and its registry value type, including entries
such as `%USERPROFILE%\custom tools`. It updates the user PATH only when neither
the user nor machine PATH already contains the binary directory, broadcasts the
change to Windows, and activates it in the invoking PowerShell process.
Comparison ignores casing, trailing separators and equivalent environment
variable expansions. Machine PATH is never changed. Added entries are recorded
in `%LOCALAPPDATA%\rayengine\setup-path.json` for reversal; entries that were
already configured are never claimed by setup. A new root adds a new entry and
retains previous installations until you remove them. Semicolons, percent signs and newlines in
Windows install roots are rejected. Percent-variable references in a literal
root would expand to a different directory in persistent PATH.

## Installation location and updates

Both entry points explicitly choose the root in this order:

1. `--root DIR` (Bash) or `-Root DIR` (PowerShell).
2. `CARGO_INSTALL_ROOT`, when nonempty.
3. `CARGO_HOME`, when nonempty.
4. `~/.cargo` (Unix) or `%USERPROFILE%\.cargo` (Windows).

The binary lives in **`ROOT/bin`**, not necessarily `~/.cargo/bin`. Relative roots
resolve from the directory where you invoked setup. Setup deliberately overrides
Cargo's `install.root` config with `--root` so the installation and persistent PATH
always agree; pass that configured directory explicitly if you want to use it.
It leaves other Cargo configuration and download caches alone. See
[Cargo's installation behavior](https://doc.rust-lang.org/cargo/commands/cargo-install.html).

For an isolated development install:

```sh
bash scripts/setup.sh --root "$HOME/Tools/rayengine cli" --shell bash
# Optional faster development build (default is release):
bash scripts/setup.sh --root "$HOME/Tools/rayengine cli" --debug
```

```powershell
& .\scripts\setup.ps1 -Root "$env:LOCALAPPDATA\rayengine cli"
& .\scripts\setup.ps1 -Root "$env:LOCALAPPDATA\rayengine cli" -DebugBuild
```

A user-writable root needs no elevation. Keep using the same root to update that
installation. Unix setup replaces the selected shell's block when switching
roots; Windows retains its owned entries. If you configure multiple shells,
rerun setup for each one. The scripts report missing/old Rust, Cargo failures,
unwritable roots/configuration and missing/non-runnable binaries with next steps.
They preserve Cargo's detailed build diagnostics and verify the installed CLI
before changing PATH.

## Remove and reverse

First uninstall only this CLI, using **the root printed by setup**:

```sh
cargo uninstall rayengine-cli --root "$HOME/Tools/rayengine cli"
bash scripts/setup.sh --shell bash --remove-path
```

```powershell
cargo uninstall rayengine-cli --root "$env:LOCALAPPDATA\rayengine cli"
& .\scripts\setup.ps1 -RemovePath
```

On Unix, repeat `--remove-path` for each configured shell, with the same
`ZDOTDIR`/`XDG_CONFIG_HOME` if customized. It removes setup's marked blocks only;
any earlier Cargo/shell PATH configuration remains. Windows removes only entries
recorded in its ownership file, preserves other user entries and deletes that
file. Keep the state file until reversal; if deleted, remove the printed binary
directory manually from the **user** PATH in Windows Environment Variables.
Uninstall all roots you installed into if you changed roots. Do not delete a
shared Cargo root or its `bin` directory: other installed tools may use it.

Open a new terminal and check `command -v rayengine` (Unix) or
`Get-Command rayengine -ErrorAction SilentlyContinue` (PowerShell). With no other
installation, it should no longer resolve. Preexisting PATH entries and other
rayengine installations intentionally remain. Removal can be repeated and does
not require Rust; binary uninstallation requires Cargo. Current Unix shells keep
inherited PATH entries until you reopen them.

## Verification

The [CLI setup workflow](../.github/workflows/cli-setup.yml) runs on Linux, macOS
and Windows. It installs the real checkout into a temporary root with spaces,
repeats setup, invokes `rayengine --help` in fresh processes from persisted
configuration, and uninstalls/reverses setup. Unix tests exercise Bash, Zsh,
Fish and POSIX configuration, custom Cargo/install/config directories,
quoted paths, already configured PATH, missing prerequisites and installation
failures. Windows tests exercise raw PATH preservation, variable expansion,
case-insensitive equivalence, already configured environments and ownership
reversal in Windows PowerShell 5.1; PowerShell 7 also runs the PATH unit tests.

Run locally:

```sh
python3 scripts/tests/test_setup.py
# Requires Bash, Zsh, Fish, Rust and a linker:
python3 scripts/tests/setup_smoke.py
```

```powershell
& .\scripts\tests\test_setup.ps1 -Smoke
```

Tests isolate setup state and restore Windows user PATH; Unix tests use a temporary
home. These checks cover CLI installation only, not native game build/run support.
