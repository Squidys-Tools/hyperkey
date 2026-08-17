# Hyperkey for Windows

This project is called Hyperkey. It's a Windows native port of the popular Mac app that turns a selected trigger key into a configurable modifier layer.

## Implemented

- WPF desktop project with WPF UI styling, targeting Windows 10 19041 or newer.
- Separate `Hyperkey.Core` settings model with typed enums and validated JSON parsing.
- Atomic settings writes under `%LOCALAPPDATA%\Hyperkey\settings.json`.
- Safe defaults and an in-app recovery message when settings cannot be loaded.
- Single-instance process guard.
- WPF UI tray icon and menu with enable/disable, settings, and quit actions.
- Scrollable settings window with light/dark theme resources, enabled state, configurable trigger and output modifiers, startup preference, and version information.
- Startup controls for launch at login and whether the settings window opens or stays in the tray.
- Pure configurable trigger state machine with suppression, activation, forwarding, and release decisions.
- Dedicated low-level keyboard hook thread with a Windows message loop.
- Scan-code-based output-modifier synthesis tagged so generated events are ignored by the hook.
- A trigger-key tap is replayed as normal input; holding it activates the modifier layer.
- Emergency disable and cleanup that reset the state machine and release generated modifiers.
- Hook status is reflected in the settings window when installation or synthesis fails.
- Sleep/resume and workstation lock/unlock recovery with a diagnostics section and hook restart controls.
- Shared version metadata used by the app and packaging workflow.
- Push-to-`main` CI that builds the solution and runs the core checks.

The installer definition and packaging script are in place, but installer validation, code signing, and final publishing polish remain open work. Uninstall cleanup removes the application-data directory. Elevated applications are unsupported in the MVP, and the final application icon is deferred.

The MVP still has the Windows input limits described in `DESIGN-IMPLEMENTATION-PLAN.md`: secure desktop screens are out of scope, elevated applications are unsupported, and other remappers or low-level game input can interfere.

## Build

Open `Hyperkey.sln` in Visual Studio with the .NET desktop development workload installed, or run `dotnet build Hyperkey.sln`.

Run the dependency-free core checks with:

```powershell
dotnet run --project tests\Hyperkey.Core.Tests\Hyperkey.Core.Tests.csproj
```

To package a self-contained x64 installer after Inno Setup 6 is installed:

```powershell
.\scripts\package-installer.ps1
```

This writes the published app to `publish\win-x64` and the installer to `publish\installer`.

The shared application version is defined in `Directory.Build.props`.

## Install and uninstall

The installer is per-user and installs Hyperkey under `%LOCALAPPDATA%\Programs\Hyperkey`, so it does not require administrator access. It creates a Start Menu shortcut and closes Hyperkey during upgrades or removal when possible.

Uninstall removes the installed files, the Start Menu shortcut, Hyperkey's launch-at-login registry value, and the entire `%LOCALAPPDATA%\Hyperkey` application-data directory. Settings are not retained after uninstall.

Elevated applications are unsupported in this MVP. Hyperkey is intended to operate with ordinary non-elevated applications.
