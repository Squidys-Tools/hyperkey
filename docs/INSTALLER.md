# Hyperkey installer

The installer uses Inno Setup and is intended for a per-user x64 installation. It installs the self-contained application under:

```text
%LOCALAPPDATA%\Programs\Hyperkey
```

The uninstaller removes Hyperkey's per-user launch-at-login registry value and deletes the `%LOCALAPPDATA%\Hyperkey` application-data directory, including settings and temporary files.

## Building the installer

From the repository root, package the application with:

```powershell
.\scripts\package-installer.ps1
```

The script reads the shared version from `Directory.Build.props`, publishes the app to `publish\win-x64`, and writes the installer to `publish\installer`. It requires the .NET SDK and Inno Setup 6, and finds Inno Setup in `PATH`, under Program Files, or in a per-user install under `%LOCALAPPDATA%\Programs`.

## Verifying the installer

`installer/Hyperkey.iss` had never been run end to end, so the packaging behavior is now covered by a
check instead of a manual pass:

```powershell
.\scripts\verify-installer.ps1
```

It builds the setup, installs it silently, and asserts on what a clean install puts on disk. It then
launches the installed app and confirms it stays running and that a second launch exits instead of
opening a second window, seeds settings and the launch-at-login value as a user would leave them,
installs a newer version over the top, and finally uninstalls and asserts that the program files, the
Start Menu shortcut, the launch-at-login value, and the settings directory are all gone.

The script refuses to run against an existing installation or a running Hyperkey instance, and it
backs up `%LOCALAPPDATA%\Hyperkey` and the launch-at-login value before it starts and puts both back
when it finishes, so it is safe to run on a development machine. Pass `-SkipUpgrade` to skip the
second installer build while iterating locally.

CI runs this as its own job on every pull request.

## Current status

Clean install, upgrade, uninstall, and startup behavior are verified automatically. Code signing is
still outstanding, so Windows may show a SmartScreen warning on first run.
