Hyperkey is a Windows-only Rust app. Commands below are PowerShell, run from the repository root.

## Setup

- A stable Rust toolchain (`stable-x86_64-pc-windows-msvc`) with clippy.
- Inno Setup 6 is required for anything under `scripts\`. It is installed per-user at
  `%LOCALAPPDATA%\Programs\Inno Setup 6`, which is not on PATH by default.

## Repository layout

- `Cargo.toml` — Cargo workspace at the repository root. The shared version lives in
  `[workspace.package]` `version` here.
- `crates\hyperkey-core` — pure settings and trigger logic; no Win32 or UI dependencies.
- `crates\hyperkey-input` — keyboard hook thread, SendInput synthesis, watchdog.
- `crates\hyperkey-app` — iced settings window, tray, startup, single instance.
- `installer\` — Inno Setup script.
- `scripts\` — installer packaging and verification.
- `docs\` — design documents.

## Commands

```powershell
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo build -p hyperkey-app --release
```

## Working rules

- Keep `hyperkey-core` free of Win32 and UI code. Platform work belongs in `hyperkey-input`.
- The version lives only in `[workspace.package]` `version` in the root `Cargo.toml`.
- Do not run `scripts\*.ps1` or any `Hyperkey-Setup-*.exe` on this machine without the
  owner's OK. `verify-installer.ps1` installs, launches, upgrades, and uninstalls the real
  app and temporarily changes the machine's startup entry and settings; it runs in CI.
