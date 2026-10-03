# Hyperkey for Windows

## Project status

This document records the current MVP design and implementation plan.

The product recreates the useful part of the Mac app Hyperkey: one underused key becomes a new modifier layer that works across Windows applications.

## MVP scope

The MVP will do one thing:

> Hold a selected trigger key to emit a selected combination of Ctrl, Alt, and Shift.

Included:

- Any normal keyboard key as the trigger (letters, digits, function keys,
  punctuation, Caps Lock, Scroll Lock, …). Modifier, system, and
  extended-scan-code keys are rejected. Escape is reserved as the
  rebind-cancel key.
- Any non-empty combination of Ctrl, Alt, and Shift as the output modifier layer.
- Background operation with a tray icon.
- A single settings window.
- Enable and disable controls.
- Works with native Win32 desktop applications.
- Light and dark system themes.
- An emergency disable path.

Not included:

- The Windows key in the output combination.
- Per-application profiles.
- Right Ctrl and Right Alt triggers.
- Macros, text expansion, app launching, Vim navigation, or window management.
- A scripting language or user-defined automation rules.
- Tabs or sidebar navigation in settings.

Tap behavior is defined as normal trigger-key input. The hook suppresses the physical tap while it decides whether the key is held, then replays a tagged trigger-key down/up pair for a tap so Windows still updates the selected lock key normally.

## Product principles

1. Hyperkey should disappear after setup.
2. The UI should explain the key combination without teaching keyboard-remapping theory.
3. The input path must be small, fast, and easy to disable.
4. Settings should expose only decisions the MVP actually supports.
5. Failure states must explain what happened and how to recover.

## Design direction

The chosen direction is a focused system utility. It should feel like a small, polished Windows background tool rather than a general automation suite.
The current leading direction is Quiet status: more whitespace with ample room for settings.

The production settings window should be one vertically scrollable list. It should not use tabs or a sidebar because the MVP has only a few settings.

### Settings structure

```text
Hyperkey
  Header: title, enabled toggle, and a badge that mirrors the toggle

Keyboard
  Trigger key: any supported key, rebound through the settings window
  Output: selected Ctrl / Alt / Shift modifiers

Startup
  Launch at Login
  Launch in tray at startup.

Diagnostics
  Hook status, restart hook, emergency disable, copy details

About
  Version
  Short help link or diagnostics entry, if needed
```

The top of the window should answer three questions immediately:

1. Is Hyperkey enabled?
2. Which key activates it?
3. Which modifiers does it emit?

The window should be compact enough to feel like a utility, but the content area should scroll when the About or diagnostics sections grow later.

## Native platform choice

Use C# with WPF and WPF UI.

WPF UI owns the settings window theme, layout, controls, and accessibility tree. Win32 interop owns the parts WPF does not specialize in:

- Low-level keyboard hooks.
- Synthesized keyboard events.
- The notification-area tray icon.
- A dedicated hook thread with its own message loop.
- Startup integration and process-level lifecycle work.

Use WPF UI controls first. Keep the UI library focused on the settings and tray surfaces; Win32 interop remains limited to the keyboard hook and input synthesis.

## Application shape

The app should run as one background process with a settings window that opens on demand.

```text
Hyperkey.App
├── App lifetime and single-instance handling
├── Tray icon and tray menu
├── Settings window
└── Settings persistence

Hyperkey.Input
├── Low-level keyboard hook
├── Trigger state machine
├── Modifier event synthesizer
└── Recovery and cleanup

Hyperkey.Core
├── Settings model
├── Key and modifier types
├── Input transition logic
└── Pure testable behavior
```

Keep `Hyperkey.Core` independent from WPF and P/Invoke. It should accept normalized key events and return decisions such as suppress, forward, press modifiers, or release modifiers. That gives the input behavior a normal unit-test surface.

## Input implementation

### Observation and suppression

Use a `WH_KEYBOARD_LL` hook installed with `SetWindowsHookEx`. The hook must run on a dedicated thread with a message loop. The callback should do very little work and immediately return.

The hook should:

1. Recognize physical trigger-key down and up events.
2. Suppress the original trigger-key event while Hyperkey owns the trigger.
3. Press the selected output modifiers when the trigger becomes active.
4. Forward the next key/keys while those modifiers are held.
5. Release all generated modifiers when the trigger key is released.
6. Pass unrelated key events through unchanged.

Use `SendInput` to synthesize the modifier events. Use scan-code-aware input where appropriate so the generated events are not tied to a particular keyboard layout.

Mark generated events with `dwExtraInfo` and ignore those marked events in the hook. This prevents the app from responding to its own synthetic input.

### State machine

```text
Idle
  Trigger key down
    → TriggerHeld

TriggerHeld
  another key down
    → HyperActive

HyperActive
  other key events
    → forward while the selected modifiers are held

TriggerHeld or HyperActive
  Trigger key up
    → release generated modifiers
    → Idle
```

The state machine must also recover when the normal release sequence is interrupted by sleep, lock, session changes, hook removal, or process shutdown.

### Important boundaries

The implementation must document these limits instead of pretending the hook controls every Windows surface:

- Secure desktop and login screens are out of scope.
- Elevated applications are unsupported in this MVP because `SendInput` is subject to UIPI.
- Games and software using lower-level input paths may not behave like ordinary desktop applications.
- Other keyboard remappers can interfere with the hook.
- The generated modifier layer always uses the **left-hand** Ctrl, Alt, and Shift keys. The
  synthesizer emits fixed left-side scan codes (`0x1D`, `0x38`, `0x2A`) and never virtual keys, so
  an application that distinguishes left from right sees the left variant.
- Right Ctrl and Right Alt cannot be used as triggers. Tap replay needs a plain scan code, and
  right-hand modifiers are extended-scan-code keys, so supporting them means reworking replay
  rather than widening the allowlist.

The app exposes a hook status readout and an Emergency disable button in Diagnostics. Emergency
disable releases every generated modifier before disabling the engine, and deliberately does not
persist the disabled state, so the next launch is enabled again.

## Settings model

Start with a versioned JSON file under the current user's local app data directory.

```json
{
  "schemaVersion": 2,
  "enabled": true,
  "trigger": "CapsLock",
  "outputModifiers": ["Control", "Alt", "Shift"],
  "launchAtStartup": true,
  "launchToTray": false
}
```

Schema 1 was the original fixed-trigger format and is still read on load. Schema 2 replaced it
when the trigger became rebindable. `tapBehavior` still exists in schema 2 but is now vestigial:
it only accepts `CapsLock`, and the legacy value `Undecided` is read as `CapsLock`. Because the
trigger key replays itself on a tap by definition, the field no longer changes behavior and is kept
only so existing settings files keep round-tripping. A future schema can drop it.

The native implementation should replace stringly typed values with enums or dedicated types. Parse and validate persisted JSON at the boundary, then pass trusted settings into the core engine.

Settings writes should be atomic enough that a process termination cannot leave a half-written file. If parsing fails, load safe defaults and show a recovery message in the settings window.

## Settings window behavior

The settings window should:

- Open from the tray icon.
- Open at a fixed 560x700 centered on screen. Window geometry is not persisted; there are no size or position fields in the settings model.
- Use Windows light and dark theme resources.
- Keep a clear enabled or disabled state at the top.
- Offer compact controls for the trigger key and output modifier combination.
- Use standard controls (CheckBox, ToggleSwitch, Button) with custom keycap-style templates. The toggles stay CheckBox-based so they remain keyboard and screen-reader operable. Layout is dividers and whitespace rather than cards or list rows.
- Scroll as the list grows.
- Have keyboard-accessible focus order.
- Avoid tab navigation, a sidebar, or hidden settings pages.

The tray menu should contain only the actions needed during daily use:

```text
Hyperkey: On / Off toggle
Open settings
Quit
```

## Implementation phases

Phases 1-3 are complete; see `CHANGELOG.md` for the shipped state of each. They are recorded here
as the original plan.

### Phase 1: native shell ✅ Complete

- Create the WPF desktop project and load WPF UI theme resources.
- Add single-instance handling.
- Add a hidden or tray-first startup path.
- Create the one-page scrollable settings window with WPF UI controls.
- Add the JSON settings model and persistence.
- Add light and dark theme resources.

Exit condition met: the app launches, opens settings from the tray, saves settings, and exits cleanly.

### Phase 2: input engine ✅ Complete

- Implement the pure trigger state machine.
- Add the low-level hook thread and message loop.
- Add suppression and tap replay for the selected trigger key. (Originally scoped to Caps Lock;
  widened to every supported trigger key when the trigger became rebindable.)
- Add Ctrl, Alt, and Shift press and release events.
- Tag and ignore generated events.
- Add emergency disable and cleanup.

Exit condition met: holding the selected trigger with a test key produces the selected modifier
shortcut without leaving stuck modifiers.

### Phase 3: recovery and diagnostics ✅ Complete

- Handle sleep and resume.
- Handle workstation lock and unlock.
- Reconcile modifier state after focus or session changes.
- Detect hook installation failure.
- Add a small diagnostics section to settings.
- Add conflict guidance for common remappers where detection is practical. (Shipped as static
  guidance in Diagnostics and the README. No detection is implemented, and none is planned.)

Exit condition met: failures are visible, recoverable, and do not require killing the process.

### Phase 4: packaging and polish ✅ Complete

- Build a conventional Windows installer with a simple one-line installation path. Verified by `scripts/verify-installer.ps1`.
- Configure per-user startup registration. Implemented in the native shell; the installer writes the Start Menu entry and the uninstaller removes the `Run` value.
- Apply the application icon to the setup, the installed executable, the settings window, and the tray. Done. The icon stops at 48 px, so adding 64 px and 256 px frames is future artwork, not future wiring.
- Test clean install, upgrade, uninstall, and startup behavior. Automated in CI by `scripts/verify-installer.ps1`.
- Keep the elevation limitation and uninstall data-cleanup behavior documented.

Exit condition met: a new Windows user can install, enable, test, and remove the app without opening a terminal.

Code signing was considered and deliberately dropped. A certificate was not worth buying for a
free utility, so Windows may show a SmartScreen warning on first run and the README tells users how
to proceed. This is a decision, not a blocked item.

The installer check is automated because the Phase 4 exit condition is a user journey, not a
unit-testable behavior: it installs the real setup silently, launches the app, proves the
single-instance guard, upgrades over the existing install, and asserts the uninstaller removes the
program files, the Start Menu shortcut, the launch-at-login value, and the settings directory. It
runs as its own CI job so a packaging regression fails a pull request instead of surfacing in a
user's download.

## Test plan

### Core behavior

- The selected trigger down suppresses ordinary trigger-key output.
- The selected trigger plus another key/keys emits the selected modifiers plus those keys.
- Key-up events release generated modifiers in the right order.
- Unrelated keys remain unchanged when Hyperkey is disabled.
- Repeated press and release cycles do not accumulate state.
- Synthetic events never re-enter the trigger logic.

### Recovery

- Release the trigger key while the target app changes.
- Lock and unlock Windows while Hyperkey is active.
- Sleep and resume while Hyperkey is active.
- Quit the app while modifiers are active.
- Disable Hyperkey while the trigger is held.
- Reinstall or remove the hook after an installation failure.

### Compatibility

- Notepad or another ordinary Win32 text editor.
- A browser.
- A terminal.
- A non-elevated application.
- Confirm an elevated application is unsupported and document the limitation.
- Multiple keyboard layouts.
- A laptop keyboard and an external keyboard.

### UI

- Settings opens from the tray.
- The enabled toggle updates the engine.
- Startup preference persists after restart.
- The list scrolls and keeps focus order.
- Light and dark themes remain readable.
- Disabled and error states are distinguishable.

## Resolved design decisions

These were open during the shell phase and have since been settled. They are recorded so the
reasoning is not lost:

1. **Output modifiers use the left-hand keys only.** The synthesizer emits fixed left-side scan codes and no virtual keys, so the physical side of the user's own Ctrl/Alt/Shift is not preserved. Revisit only if an application needs the distinction.
2. **A conventional Windows installer, not Scoop.** The Inno Setup definition ships; no Scoop manifest exists. A manifest remains a reasonable later addition.
3. **Elevated applications are unsupported and documented.** The app runs `asInvoker`, so `SendInput` cannot reach a higher-integrity window. Stated in the README, the changelog, and in-app.
4. **The icon ships as a placeholder.** `Assets/favicon.ico` is applied to the setup executable, the installed executable, the settings window, and the tray. It stops at 48 px; larger frames are artwork, not wiring.

## Research references

- [Hyperkey official site](https://hyperkey.app/)
- [WPF overview](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/overview/)
- [WPF UI](https://github.com/lepoco/wpfui)
- [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)
- [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [Raw Input overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input)
- [PowerToys Keyboard Manager limitations](https://learn.microsoft.com/en-us/windows/powertoys/keyboard-manager)