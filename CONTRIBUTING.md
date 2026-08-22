# Contributing to Hyperkey

Thanks for helping out. This page covers how the codebase is organized, how to build and test it, and what to know before changing the input engine.

For what the app does and how to use it, see the [README](README.md). The design docs under `docs/` explain the reasoning behind most decisions. `DESIGN.md` covers MVP scope, architecture, and the input implementation. `CHANGELOG.md` tracks phase status and known limitations. `INSTALLER.md` documents the installer.

## Repository layout

```text
src/Hyperkey.App      WPF app: lifetime, tray icon, settings window (WPF UI)
src/Hyperkey.Input    Keyboard hook thread, trigger state machine glue, SendInput synthesis
src/Hyperkey.Core     Pure settings model and input transition logic, no Win32 or UI dependencies
tests/                Dependency-free test runner for Hyperkey.Core
installer/            Inno Setup definition
scripts/              Packaging script
docs/                 Design documents
```

The key architectural rule: `Hyperkey.Core` stays free of WPF and P/Invoke. It takes normalized key events in and returns decisions (suppress, forward, press modifiers, release modifiers) out. All platform work happens in `Hyperkey.Input`. This is what keeps the core unit-testable.

## Prerequisites

- .NET 8 SDK
- Windows 10 19041 or newer to run the app
- [Inno Setup 6](https://jrsoftware.org/isinfo.php), only if you want to build the installer

## Build

Open `Hyperkey.sln` in Visual Studio with the .NET desktop workload, or:

```powershell
dotnet build Hyperkey.sln -c Release -p:Platform=x64
```

Run the app from `src\Hyperkey.App\bin\x64\Release\net8.0-windows\Hyperkey.App.exe`.

## Tests

Tests live in `tests\Hyperkey.Core.Tests` as a plain console program with no framework dependency. Each check is a method called from `Main`; a failure throws and sets the exit code.

```powershell
dotnet run --project tests\Hyperkey.Core.Tests\Hyperkey.Core.Tests.csproj
```

If you change trigger behavior, add a case there. The existing cases cover the state machine phases, tap replay, modifier press/release ordering, synthetic-event immunity, and repeated-cycle stability.

Behavior changes that only touch `Hyperkey.Input` or `Hyperkey.App` can't be covered this way, so describe manual test steps in your PR: which keys you tried, in which apps.

## Things to keep intact

These invariants are easy to break and expensive to lose:

- **Atomic settings writes.** Settings saves must never leave a half-written `%LOCALAPPDATA%\Hyperkey\settings.json`, even on process kill.
- **Synthetic events are tagged.** Every event Hyperkey sends through `SendInput` carries a `dwExtraInfo` marker, and the hook ignores marked events. Without this the hook reacts to its own output and loops.
- **Recovery paths reset everything.** Sleep, resume, lock, unlock, and emergency disable must release all generated modifiers, not just stop listening. Stuck Ctrl+Alt+Shift is the worst failure mode this app can produce.
- **The hook callback stays cheap.** It runs on a dedicated thread with a message loop and should return fast; real work belongs in the state machine.

## CI and releases

- Pushes to `main` run `.github/workflows/ci.yml`: restore, x64 Release build, core tests.
- Tagging `v*` runs `.github/workflows/release.yml`: it publishes the app, builds the installer, and attaches `Hyperkey-Setup-*.exe` to a GitHub release.

To build the installer locally:

```powershell
.\scripts\package-installer.ps1
```

That writes the publish output to `publish\win-x64` and the installer to `publish\installer`. The version number comes from `Directory.Build.props`; pass `-Version` to override it.

## Submitting changes

1. Fork, branch, make your change.
2. Build and run the core tests.
3. Open a PR describing what changed and how you verified it. For input-engine changes, include the manual steps above.

If you're planning something larger than a fix, like new trigger keys or per-app profiles, open an issue first so we can agree on scope before you write code. The design docs record which features are deliberately out of MVP scope.
