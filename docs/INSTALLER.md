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

The script reads the shared version from `Directory.Build.props`, publishes the app to `publish\win-x64`, and writes the installer to `publish\installer`. It requires the .NET SDK and Inno Setup 6.

## Current status

Packaging has not been validated yet; final install, upgrade, uninstall, and startup testing remains outstanding.