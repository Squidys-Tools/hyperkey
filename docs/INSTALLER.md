# Hyperkey installer

The installer uses Inno Setup and is intended for a per-user x64 installation. It packages the Rust
`hyperkey-app` binary and installs it under:

```text
%LOCALAPPDATA%\Programs\Hyperkey
```

The staged executable keeps the C# name, `Hyperkey.App.exe`, and the setup keeps the original AppId
and install directory, so running the new installer over an existing C# install upgrades it in
place: the binary is swapped, the Start Menu shortcut and launch-at-login registration keep pointing
at the same path, and `settings.json` under `%LOCALAPPDATA%\Hyperkey` carries over unchanged (the
Rust app reads the same schema).

The uninstaller removes Hyperkey's per-user launch-at-login registry value and deletes the
`%LOCALAPPDATA%\Hyperkey` application-data directory, including settings and temporary files.

## Building the installer

From the repository root, package the application with:

```powershell
.\scripts\package-installer.ps1
```

The script reads the shared version from `Directory.Build.props`, builds the app with
`cargo build --release -p hyperkey-app` from `rust\`, stages the binary as
`publish\app\Hyperkey.App.exe`, and writes the installer to `publish\installer` as
`Hyperkey-Setup-<version>.exe`. It requires the Rust toolchain and Inno Setup 6, and finds Inno
Setup in `PATH`, under Program Files, or in a per-user install under `%LOCALAPPDATA%\Programs`.
Pass `-Configuration Debug` for a faster local build.

## Verifying the installer

The packaging behavior is covered end to end by a check instead of a manual pass:

```powershell
.\scripts\verify-installer.ps1
```

It builds the setup, installs it silently, and asserts on what a clean install puts on disk. It then
launches the installed app and confirms it stays running and that a second launch exits instead of
opening a second window, seeds settings and the launch-at-login value as a user would leave them,
installs a newer version over the top, and finally uninstalls and asserts that the program files,
the Start Menu shortcut, the launch-at-login value, the uninstall registration, and the settings
directory are all gone.

To exercise the real C# to Rust upgrade path, pass a previous release's setup as the baseline:

```powershell
gh release download v0.1.1 --pattern "Hyperkey-Setup-*.exe" --dir baseline
.\scripts\verify-installer.ps1 -BaselineSetupPath .\baseline\Hyperkey-Setup-0.1.1.exe
```

In that mode the script installs the C# baseline first, seeds user state, and upgrades it to a
version-bumped Rust build — the same path CI runs. Pass `-SkipUpgrade` to skip the second installer
build while iterating locally (not combinable with `-BaselineSetupPath`).

The script refuses to run against an existing installation or a running Hyperkey instance, and it
backs up `%LOCALAPPDATA%\Hyperkey` and the launch-at-login value before it starts and puts both back
when it finishes, so it is safe to run on a development machine.

If a run is interrupted part-way through — Ctrl+C, a crash, or a reboot — the uninstaller may already
have deleted your settings. The backup is the only copy, so copy it back:

```text
%TEMP%\hyperkey-installer-verification\appdata-backup\settings.json
  -> %LOCALAPPDATA%\Hyperkey\settings.json
```

The launch-at-login value is restored from the backup directory only if the run reached that step, so
re-enable "Launch at login" in settings afterwards if the value is missing.

CI runs this as its own job on every pull request, downloading the latest C# release
(`Hyperkey-Setup-0.1.1.exe` from the `v0.1.1` tag) as the upgrade baseline.

## Current status

Clean install, upgrade (including the C# to Rust upgrade), uninstall, and startup behavior are
verified automatically. Code signing was deliberately dropped, so Windows may show a SmartScreen
warning on first run; the README explains how to proceed.
