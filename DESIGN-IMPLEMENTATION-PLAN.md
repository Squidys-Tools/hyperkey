# Hyperkey for Windows

## Project status

This document records the current MVP design and implementation plan.

The product recreates the useful part of the Mac app Hyperkey: one underused key becomes a new modifier layer that works across Windows applications.

## MVP scope

The MVP will do one thing:

> Hold a selected trigger key to emit a selected combination of Ctrl, Alt, and Shift.

Included:

- Caps Lock and Scroll Lock as trigger-key choices.
- Any non-empty combination of Ctrl, Alt, and Shift as the output modifier layer.
- Background operation with a tray icon.
- A single settings window.
- Enable and disable controls.
- Native windows compatibility.
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
  Enabled status card

Keyboard
  Trigger key: Caps Lock or Scroll Lock
  Output: selected Ctrl / Alt / Shift modifiers

Startup
  Launch at Login
  Launch in tray at startup.

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
- A message-only window or message loop.
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

Keep `Hyperkey.Core` independent from WinUI and P/Invoke. It should accept normalized key events and return decisions such as suppress, forward, press modifiers, or release modifiers. That gives the input behavior a normal unit-test surface.

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

The app should expose a diagnostic state for the hook and an emergency disable shortcut that releases every generated modifier before disabling the engine.

## Settings model

Start with a versioned JSON file under the current user's local app data directory.

```json
{
  "schemaVersion": 1,
  "enabled": true,
  "trigger": "CapsLock",
  "outputModifiers": ["Control", "Alt", "Shift"],
  "launchAtStartup": true,
  "launchToTray": false,
  "tapBehavior": "CapsLock"
}
```

The native implementation should replace stringly typed values with enums or dedicated types. Parse and validate persisted JSON at the boundary, then pass trusted settings into the core engine.

Settings writes should be atomic enough that a process termination cannot leave a half-written file. If parsing fails, load safe defaults and show a recovery message in the settings window.

## Settings window behavior

The settings window should:

- Open from the tray icon.
- Remember its last size and position if that proves useful.
- Use Windows light and dark theme resources.
- Keep a clear enabled or disabled state at the top.
- Offer compact controls for the trigger key and output modifier combination.
- Use ordinary WinUI toggles, buttons, and list rows.
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

### Phase 1: native shell

- Create the WPF desktop project and load WPF UI theme resources.
- Add single-instance handling.
- Add a hidden or tray-first startup path.
- Create the one-page scrollable settings window with WPF UI controls.
- Add the JSON settings model and persistence.
- Add light and dark theme resources.

Exit condition: the app launches, opens settings from the tray, saves settings, and exits cleanly.

### Phase 2: input engine

- Implement the pure trigger state machine.
- Add the low-level hook thread and message loop.
- Add Caps Lock suppression.
- Add Ctrl, Alt, and Shift press and release events.
- Tag and ignore generated events.
- Add emergency disable and cleanup.

Exit condition: holding either supported trigger with a test key produces the selected modifier shortcut without leaving stuck modifiers.

### Phase 3: recovery and diagnostics

- Handle sleep and resume.
- Handle workstation lock and unlock.
- Reconcile modifier state after focus or session changes.
- Detect hook installation failure.
- Add a small diagnostics section to settings.
- Add conflict guidance for common remappers where detection is practical.

Exit condition: failures are visible, recoverable, and do not require killing the process.

### Phase 4: packaging and polish

- Build a conventional Windows installer with a simple one-line installation path. (Definition and packaging script added; validation remains.)
- Configure per-user startup registration. (Implemented in the native shell; installer integration remains.)
- Add application icon and tray assets when the final icon is available. (Deferred.)
- Code-sign the build.
- Test clean install, upgrade, uninstall, and startup behavior.
- Keep the elevation limitation and uninstall data-cleanup behavior documented.

Exit condition: a new Windows user can install, enable, test, and remove the app without opening a terminal.

## Test plan

### Core behavior

- The selected trigger down suppresses ordinary trigger-key output.
- The selected trigger plus another key/keys emits the selected modifiers plus those keys.
- Key-up events release generated modifiers in the right order.
- Unrelated keys remain unchanged when Hyperkey is disabled.
- Repeated press and release cycles do not accumulate state.
- Synthetic events never re-enter the trigger logic.

### Recovery

- Release Caps Lock while the target app changes.
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

## Open decisions

These should be resolved before the implementation leaves the shell phase:

1. Should the output modifier order use left-side modifiers only, or preserve the physical side where possible?
2. The first release will use a conventional Windows installer. Scoop support may be added later through a manifest.
3. Elevated applications are unsupported in the MVP and should be documented as such.
4. The final Windows icon treatment is deferred until the icon is supplied.

## Research references

- [Hyperkey official site](https://hyperkey.app/)
- [WPF overview](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/overview/)
- [WPF UI](https://github.com/lepoco/wpfui)
- [LowLevelKeyboardProc](https://learn.microsoft.com/en-us/windows/win32/winmsg/lowlevelkeyboardproc)
- [SendInput](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-sendinput)
- [Raw Input overview](https://learn.microsoft.com/en-us/windows/win32/inputdev/about-raw-input)
- [PowerToys Keyboard Manager limitations](https://learn.microsoft.com/en-us/windows/powertoys/keyboard-manager)
