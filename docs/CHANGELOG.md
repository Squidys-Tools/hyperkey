# Changelog & Roadmap

This document tracks the implementation phases and current project status.

## Current Status

**Version:** 0.1.1
**Phase:** 4 - Packaging and Polish (In Progress)

---

## Implementation Phases

### Phase 1: Native Shell ✅ Complete

- [x] WPF desktop project with WPF UI theme resources
- [x] Single-instance process guard
- [x] Tray-first startup path
- [x] Scrollable settings window with WPF UI controls
- [x] JSON settings model and atomic persistence
- [x] Light and dark theme resources
- [x] Startup controls (launch at login, launch to tray)

**Exit condition met:** App launches, opens settings from tray, saves settings, and exits cleanly.

---

### Phase 2: Input Engine ✅ Complete

- [x] Pure trigger state machine
- [x] Low-level keyboard hook thread with message loop
- [x] Trigger key suppression and tap replay (any supported trigger key)
- [x] Ctrl, Alt, and Shift press/release synthesis
- [x] Generated event tagging and ignore logic
- [x] Emergency disable and cleanup

**Exit condition met:** Holding trigger key with test key produces modifier shortcut without stuck modifiers.

---

### Phase 3: Recovery & Diagnostics ✅ Complete

- [x] Sleep and resume handling
- [x] Workstation lock and unlock handling
- [x] Modifier state reconciliation
- [x] Hook installation failure detection
- [x] Diagnostics section in settings
- [x] Hook restart controls
- [x] Conflict guidance for common remappers (static text in Diagnostics and the README; no detection is implemented)

**Exit condition met:** Failures are visible, recoverable, and do not require killing the process.

---

### Phase 4: Packaging & Polish ✅ Complete

- [x] Installer definition (Inno Setup)
- [x] Packaging script (`scripts/package-installer.ps1`)
- [x] Per-user startup registration (native shell)
- [x] Rebindable trigger key (any normal keyboard key; settings schema v2)
- [x] Installer validation and testing (`scripts/verify-installer.ps1`, run in CI)
- [x] Clean install, upgrade, uninstall, and startup testing (same check)
- [x] Application icon on the setup, installed executable, settings window, and tray

**Exit condition met:** A new Windows user can install, enable, test, and remove the app without opening a terminal.

The exit condition is covered by an automated check rather than a manual pass. `scripts/verify-installer.ps1`
builds the setup, installs it silently, launches the app, proves the single-instance guard, upgrades
over the existing install, and asserts the uninstaller leaves nothing behind. CI runs it on every
pull request.

**Code signing was considered and deliberately dropped.** A certificate was not worth buying for a
free utility, so Windows may show a SmartScreen warning on first run. The README explains how to
proceed. This is a decision, not an open item.

**Icons are a placeholder, not final artwork.** `Assets/favicon.ico` (48/32/16 px, 32-bit colour) is
applied to the setup executable, the installed executable, the settings window, and the tray. It is
already wired everywhere; what is missing is larger frames, which is artwork rather than code. The
tray icon is also delivered as a single 32 px raster derived from the executable rather than picking
a frame by DPI.

---

## Known Limitations (MVP)

- Secure desktop and login screens are out of scope
- Elevated applications are unsupported (UIPI limitation)
- Games with low-level input paths may not work correctly
- Other keyboard remappers can interfere with the hook
- Windows key is not included in output modifier combinations
- The generated layer always uses the left-hand Ctrl, Alt, and Shift keys, so an application that distinguishes left from right sees the left variant

---

## Future Considerations (Post-MVP)

- Per-application profiles
- Right Ctrl and Right Alt triggers. Not a backlog item waiting for a wider allowlist: tap replay
  needs a plain scan code and these are extended-scan-code keys, so replay has to be reworked first.
- Larger icon frames (64 px, 256 px) and a tray icon that selects a frame by DPI
- Macros and text expansion
- App launching and window management
- Scoop package manager support

---

## Version History

| Version | Date | Phase | Notes |
|---------|------|-------|-------|
| Unreleased | - | 4 | Rebindable trigger key, main window redesign with theme tokens, press-to-rebind button, key-up synthesis for released modifiers, automated installer verification |
| 0.1.1 | - | 4 | Lazy-load settings window for lower tray-idle memory usage |
| 0.1.0 | - | 4 | Initial MVP, phases 1-3 complete |
