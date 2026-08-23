# Hyperkey for Windows

Hyperkey turns a key you rarely use into a new modifier layer, like Ctrl or Alt but entirely your own. It's a Windows port of the Mac app [Hyperkey](https://hyperkey.app/).

## How it works

Pick a trigger key (Caps Lock or Scroll Lock) and a set of modifiers (Ctrl, Alt, Shift, any combination).

Hold the trigger key and every other key you press acts as if those modifiers were held too. For example, with Caps Lock as the trigger and Ctrl+Alt+Shift as the output:

- Hold Caps Lock, press F, and Windows sees Ctrl+Alt+Shift+F.
- Tap Caps Lock on its own and it still toggles caps like normal.

Your real Ctrl, Alt, and Shift keys keep working exactly as before. Hyperkey adds a layer on top instead of replacing anything.

## Install

1. Download `Hyperkey-Setup-x64.exe` from the [releases page](https://github.com/Squidys-Tools/hyperkey/releases).
2. Run it. The installer is per-user and needs no administrator access.
3. Hyperkey starts in the system tray. Click the tray icon to open settings.

The installer isn't code-signed yet, so Windows may show a SmartScreen warning on first run. Choose "More info" and "Run anyway" to continue.

Requires Windows 10 version 19041 or newer.

## Using Hyperkey

Settings live in one window, opened from the tray icon:

- **Enabled** turns the whole thing on or off without quitting.
- **Trigger key** picks between Caps Lock and Scroll Lock.
- **Output modifiers** picks which of Ctrl, Alt, or Shift the layer sends. Pick at least one.
- **Launch at login** starts Hyperkey when you sign in. **Launch to tray** keeps the settings window closed at startup.

Settings are saved under `%LOCALAPPDATA%\Hyperkey\settings.json` and survive restarts.

## If something goes wrong

Open settings and scroll to **Diagnostics**:

- **Restart hook** re-installs the keyboard listener if it stopped working, for example after some sleep/resume cycles.
- **Emergency disable** turns everything off and releases any modifiers Hyperkey was holding. Use this if keys ever seem stuck.
- **Copy details** copies diagnostic text for a bug report.

Some things Hyperkey can't do, by design of how Windows input works:

- Administrator-running applications ignore Hyperkey's generated keys.
- The Windows login screen and other secure desktops are out of scope.
- Some games read the keyboard through lower-level paths and may not see the extra modifiers.
- Other remapping tools (PowerToys Keyboard Manager, for example) can fight over the same keys. Pick one tool per job.

## Uninstall

Uninstall Hyperkey from Windows Settings, "Apps", "Installed apps". This removes the app files, the Start Menu shortcut, its launch-at-login entry, and the settings folder. Your settings are not kept after uninstall.

## Development

Hyperkey is free and open source. If you want to build it yourself or help out, see [CONTRIBUTING.md](CONTRIBUTING.md).
