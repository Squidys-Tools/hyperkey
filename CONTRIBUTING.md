# Contributing to Hyperkey

Thanks for helping out. This page covers how the codebase is organized, how to build and test it, and what to know before changing the input engine.

For what the app does and how to use it, see the [README](README.md). The design docs under `docs/` explain the reasoning behind most decisions. `DESIGN.md` covers MVP scope, architecture, and the input implementation. `CHANGELOG.md` tracks phase status and known limitations. `INSTALLER.md` documents the installer.

## Repository layout

```text
crates/hyperkey-app    iced settings window, tray icon, startup, single-instance guard
crates/hyperkey-input  Keyboard hook thread, SendInput synthesis, watchdog
crates/hyperkey-core   Pure settings model and trigger logic, no Win32 or UI dependencies
installer/             Inno Setup definition
scripts/               Packaging and installer verification scripts
docs/                  Design documents
```

The key architectural rule: `hyperkey-core` stays free of Win32 and UI code. It takes normalized key events in and returns decisions (suppress, forward, press modifiers, release modifiers) out. All platform work happens in `hyperkey-input`. This is what keeps the core unit-testable.

## Prerequisites

- A stable Rust toolchain (`stable-x86_64-pc-windows-msvc`), with clippy
- Windows 10 19041 or newer to run the app
- [Inno Setup 6](https://jrsoftware.org/isinfo.php), only if you want to build the installer

## Build

```powershell
cargo build -p hyperkey-app --release
```

Run the app from `target\release\hyperkey-app.exe`.

Lint the way CI does:

```powershell
cargo clippy --workspace --all-targets -- -D warnings
```

## Tests

```powershell
cargo test --workspace
```

Tests live under `crates/*/tests` as ordinary `cargo test` targets. The `hyperkey-core` cases cover the state machine phases, tap replay, modifier press/release ordering, synthetic-event immunity, repeated-cycle stability, and settings parsing and migration. `hyperkey-input` has a loopback test that drives the real hook end to end.

If you change trigger behavior, add a case there. Behavior that only shows up through the real hook or the UI can't be covered this way, so describe manual test steps in your PR: which keys you tried, in which apps.

## Things to keep intact

These invariants are easy to break and expensive to lose:

- **Atomic settings writes.** Settings saves must never leave a half-written `%LOCALAPPDATA%\Hyperkey\settings.json`, even on process kill.
- **Synthetic events are tagged.** Every event Hyperkey sends through `SendInput` carries a `dwExtraInfo` marker, and the hook ignores marked events. Without this the hook reacts to its own output and loops.
- **Recovery paths reset everything.** Sleep, resume, lock, unlock, and emergency disable must release all generated modifiers, not just stop listening. Stuck Ctrl+Alt+Shift is the worst failure mode this app can produce.
- **The hook callback stays cheap.** It runs on a dedicated thread with a message loop and should return fast; real work belongs in the state machine.

## CI and releases

- Pushes to `main` and pull requests run `.github/workflows/ci.yml`: clippy, the workspace tests, a release build of `hyperkey-app`, and a job that installs, upgrades, and uninstalls the real installer. The upgrade check installs the last C# release (0.1.1) as the baseline and upgrades it to the Rust build.
- Tagging `v*` runs `.github/workflows/release.yml`: it builds the installer and attaches `Hyperkey-Setup-*.exe` to a GitHub release.

To build the installer locally:

```powershell
.\scripts\package-installer.ps1
```

That builds `hyperkey-app`, stages the binary as `publish\app\Hyperkey.App.exe`, and writes the installer to `publish\installer` as `Hyperkey-Setup-<version>.exe`. The version number comes from `[workspace.package]` `version` in the root `Cargo.toml`; pass `-Version` to override it.

To check what the installer actually does:

```powershell
.\scripts\verify-installer.ps1
```

This installs, launches, upgrades, and uninstalls Hyperkey for real, so it is slow and temporarily changes the machine's startup entry and settings; it normally runs in CI. It refuses to run against an existing install or a running Hyperkey instance, and it restores your settings and launch-at-login value when it finishes, so it is safe on a development machine. See [INSTALLER.md](docs/INSTALLER.md) for what it checks.

## Submitting changes

1. Fork, branch, make your change.
2. Run clippy and the workspace tests.
3. Open a PR describing what changed and how you verified it. For input-engine changes, include the manual steps above.

If you're planning something larger than a fix, like new trigger keys or per-app profiles, open an issue first so we can agree on scope before you write code. The design docs record which features are deliberately out of MVP scope.
