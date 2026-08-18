# Changelog & Roadmap

This document tracks the implementation phases and current project status.

## Current Status

**Version:** 0.1.0  
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
- [x] Caps Lock and Scroll Lock suppression
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

**Exit condition met:** Failures are visible, recoverable, and do not require killing the process.

---

### Phase 4: Packaging & Polish 🔄 In Progress

- [x] Installer definition (Inno Setup)
- [x] Packaging script (`scripts/package-installer.ps1`)
- [x] Per-user startup registration (native shell)
- [ ] Installer validation and testing
- [ ] Code signing
- [ ] Application icon and tray assets (deferred)
- [ ] Clean install, upgrade, uninstall, and startup testing

**Exit condition:** A new Windows user can install, enable, test, and remove the app without opening a terminal.

---

## Known Limitations (MVP)

- Secure desktop and login screens are out of scope
- Elevated applications are unsupported (UIPI limitation)
- Games with low-level input paths may not work correctly
- Other keyboard remappers can interfere with the hook
- Windows key is not included in output modifier combinations

---

## Future Considerations (Post-MVP)

- Per-application profiles
- Right Ctrl and Right Alt triggers
- Macros and text expansion
- App launching and window management
- Scoop package manager support
- Additional trigger key options

---

## Version History

| Version | Date | Phase | Notes |
|---------|------|-------|-------|
| 0.1.0 | - | 4 | Initial MVP, phases 1-3 complete |