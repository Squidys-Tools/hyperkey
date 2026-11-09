//! The toolkit-free application controller: owns the settings, the engine, the
//! tray, and every user-visible message, and implements each operation the UI
//! or tray can trigger. Nothing here knows about GPUI.
use crate::copy::{
    CAPTURE_TRIGGER_HINT, IDLE_TRIGGER_HINT, INVALID_KEY_HINT, MODIFIER_CHOICE_HINT,
    MODIFIER_KEY_HINT, MODIFIER_REQUIRED_HINT,
};
use crate::platform::session_window::SessionEvent;
use crate::platform::{clipboard, startup, tray};
use crate::settings_store::{self, SettingsStore};
use futures::channel::mpsc::UnboundedSender;
use hyperkey_core::keys::{CaptureDecision, classify_capture};
use hyperkey_core::settings::{HyperkeySettings, OutputModifier};
use hyperkey_input::capture::{CapturedKey, KeyCapture};
use hyperkey_input::engine::{EngineEvent, InputEngine, InputEngineStatus};
use std::sync::mpsc::Receiver;

/// Everything background threads and the tray hand to the UI thread.
pub enum AppEvent {
    ToggleEnabled,
    OpenSettings,
    Quit,
    Session(SessionEvent),
    Engine(EngineEvent),
    Captured(CapturedKey),
}

pub struct Controller {
    settings: HyperkeySettings,
    store: SettingsStore,
    engine: InputEngine,
    tray: Option<tray::Tray>,
    events: UnboundedSender<AppEvent>,
    /// Banner for settings load/save problems: quarantines, repairs, and
    /// write failures.
    banner: Option<String>,
    /// Hook lifecycle and restart notes, shown under Diagnostics.
    diagnostic_message: Option<String>,
    /// Startup-registration failure.
    startup_error: Option<String>,
    /// Result of the last Copy details action.
    copy_status: Option<String>,
    /// True while the next key press rebinds the trigger.
    capturing: bool,
    /// Owns the capture hook; dropping it uninstalls the hook.
    capture: Option<KeyCapture>,
    trigger_hint: String,
    modifier_hint: &'static str,
    /// Whether startup registration is applied (off for dev isolation).
    manage_startup: bool,
    is_quitting: bool,
}

impl Controller {
    /// Loads settings, starts the engine when enabled, and registers the tray
    /// icon. Must run on the GPUI main thread.
    pub fn new(events: UnboundedSender<AppEvent>) -> Self {
        let store = SettingsStore::new();
        let load = store.load();
        let settings = load.settings;
        let manage_startup = !settings_store::is_isolated();

        let engine = InputEngine::new(settings.trigger(), settings.output_modifiers().to_vec());
        let startup_error = if manage_startup {
            startup::apply(settings.launch_at_startup()).err()
        } else {
            None
        };

        let mut diagnostic_message = None;
        if settings.enabled() && !engine.start() {
            diagnostic_message = Some(start_failure(
                &engine,
                "The keyboard hook could not be started.",
            ));
        }

        let running = engine.status() == InputEngineStatus::Running;
        let tray = match tray::Tray::create(&settings, running) {
            Ok(tray) => Some(tray),
            Err(_) => {
                diagnostic_message =
                    Some("The system tray icon could not be registered.".to_string());
                None
            }
        };

        Self {
            settings,
            store,
            engine,
            tray,
            events,
            banner: load.message,
            diagnostic_message,
            startup_error,
            copy_status: None,
            capturing: false,
            capture: None,
            trigger_hint: IDLE_TRIGGER_HINT.to_string(),
            modifier_hint: MODIFIER_CHOICE_HINT,
            manage_startup,
            is_quitting: false,
        }
    }

    // ---- read access for the UI ----------------------------------------

    pub fn settings(&self) -> &HyperkeySettings {
        &self.settings
    }

    pub fn engine_status(&self) -> InputEngineStatus {
        self.engine.status()
    }

    pub fn status_error(&self) -> Option<String> {
        self.engine.status_error()
    }

    pub fn last_recovery_message(&self) -> Option<String> {
        self.engine.last_recovery_message()
    }

    pub fn subscribe_engine(&self) -> Receiver<EngineEvent> {
        self.engine.subscribe()
    }

    pub fn banner(&self) -> Option<&str> {
        self.banner.as_deref()
    }

    /// The startup registration error first, then the engine lifecycle note.
    pub fn diagnostic_message(&self) -> Option<&str> {
        self.startup_error
            .as_deref()
            .or(self.diagnostic_message.as_deref())
    }

    pub fn copy_status(&self) -> Option<&str> {
        self.copy_status.as_deref()
    }

    pub fn is_capturing(&self) -> bool {
        self.capturing
    }

    pub fn trigger_hint(&self) -> &str {
        &self.trigger_hint
    }

    pub fn modifier_hint(&self) -> &str {
        self.modifier_hint
    }

    pub fn has_modifier(&self, modifier: OutputModifier) -> bool {
        self.settings.output_modifiers().contains(&modifier)
    }

    // ---- operations ----------------------------------------------------

    pub fn toggle_enabled(&mut self) {
        self.set_enabled(!self.settings.enabled());
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.update_settings(self.settings.with_enabled(enabled), true);
    }

    pub fn set_launch_at_startup(&mut self, launch: bool) {
        self.update_settings(self.settings.with_launch_at_startup(launch), true);
    }

    pub fn set_launch_to_tray(&mut self, to_tray: bool) {
        self.update_settings(self.settings.with_launch_to_tray(to_tray), true);
    }

    /// Selects or deselects one output modifier. Deselecting the last one is
    /// refused with a hint instead of producing an empty set.
    pub fn toggle_modifier(&mut self, modifier: OutputModifier, selected: bool) {
        let mut modifiers: Vec<OutputModifier> = self.settings.output_modifiers().to_vec();
        if selected {
            if !modifiers.contains(&modifier) {
                modifiers.push(modifier);
            }
        } else {
            modifiers.retain(|existing| *existing != modifier);
        }

        if modifiers.is_empty() {
            self.modifier_hint = MODIFIER_REQUIRED_HINT;
            return;
        }

        match self.settings.with_output_modifiers(modifiers) {
            Ok(settings) => {
                self.modifier_hint = MODIFIER_CHOICE_HINT;
                self.update_settings(settings, true);
            }
            Err(_) => self.modifier_hint = MODIFIER_REQUIRED_HINT,
        }
    }

    /// Starts listening for the next key press to use as the trigger. If the
    /// capture hook cannot be installed its message becomes the trigger hint.
    pub fn start_capture(&mut self) {
        if self.capturing {
            return;
        }

        let events = self.events.clone();
        match KeyCapture::start(move |key| {
            let _ = events.unbounded_send(AppEvent::Captured(key));
        }) {
            Ok(capture) => {
                self.capture = Some(capture);
                self.capturing = true;
                self.trigger_hint = CAPTURE_TRIGGER_HINT.to_string();
            }
            Err(message) => self.trigger_hint = message,
        }
    }

    /// Applies one captured key press. Presses that arrive after capture
    /// ended are ignored.
    pub fn handle_captured_key(&mut self, key: CapturedKey) {
        if !self.capturing {
            return;
        }

        match classify_capture(key.virtual_key, key.extended, key.modifiers_held) {
            CaptureDecision::Cancel => self.cancel_capture(),
            CaptureDecision::ModifierOrSystem => self.trigger_hint = MODIFIER_KEY_HINT.to_string(),
            CaptureDecision::Unsupported => self.trigger_hint = INVALID_KEY_HINT.to_string(),
            CaptureDecision::Accept(trigger) => match self.settings.with_trigger(trigger) {
                Ok(settings) => self.update_settings(settings, true),
                Err(_) => self.trigger_hint = INVALID_KEY_HINT.to_string(),
            },
        }
    }

    pub fn cancel_capture(&mut self) {
        self.capturing = false;
        self.capture = None;
        self.trigger_hint = IDLE_TRIGGER_HINT.to_string();
    }

    /// Refuses while disabled, prefers the engine's hook-only restart, and
    /// falls back to a full stop/start cycle.
    pub fn restart_hook(&mut self) {
        if !self.settings.enabled() {
            self.diagnostic_message =
                Some("Enable Hyperkey before restarting the keyboard hook.".to_string());
            return;
        }

        if self.engine.restart_hook() {
            self.diagnostic_message = Some("The keyboard hook restart was requested.".to_string());
            return;
        }

        self.engine.stop();
        self.diagnostic_message = Some(if self.engine.start() {
            "The keyboard hook was restarted successfully.".to_string()
        } else {
            start_failure(&self.engine, "The keyboard hook could not be restarted.")
        });
    }

    /// Releases the modifier layer and stops the engine, but deliberately does
    /// not persist `enabled = false`: the next launch reactivates unless the
    /// user toggles the setting off.
    pub fn emergency_disable(&mut self) {
        self.engine.emergency_disable();
        self.update_settings(self.settings.with_enabled(false), false);
    }

    pub fn copy_details(&mut self) {
        self.copy_status = Some(match clipboard::set_text(&self.diagnostic_details()) {
            Ok(()) => "Diagnostic details copied to the clipboard.".to_string(),
            Err(error) => format!("Diagnostic details could not be copied: {error}"),
        });
    }

    /// Keeps the tray icon in sync with hook health and reports a failure
    /// that has no other explanation yet.
    pub fn on_engine_event(&mut self, event: EngineEvent) {
        if let EngineEvent::StatusChanged { status, .. } = event {
            if status == InputEngineStatus::Failed && self.diagnostic_message.is_none() {
                self.diagnostic_message = self
                    .engine
                    .status_error()
                    .or_else(|| Some("The keyboard hook stopped.".to_string()));
            }
            self.refresh_tray();
        }
    }

    /// Sleep/resume, lock/unlock, and display changes reset the engine's
    /// input state. Raising the settings window is the app's job.
    pub fn on_session_event(&mut self, event: SessionEvent) {
        if event == SessionEvent::ReconcileEngine && !self.is_quitting {
            self.engine.reconcile();
        }
    }

    /// Releases everything held for the process lifetime. Safe to call twice.
    pub fn shutdown(&mut self) {
        if self.is_quitting {
            return;
        }
        self.is_quitting = true;
        self.cancel_capture();
        self.engine.emergency_disable();
        self.engine.stop();
        drop(self.tray.take());
    }

    /// The plain-text block copied by Copy details.
    pub fn diagnostic_details(&self) -> String {
        let none = || "None".to_string();
        let diagnostic = self
            .diagnostic_message()
            .map(str::to_string)
            .unwrap_or_else(none);
        let recovery = self.banner.clone().unwrap_or_else(none);
        let modifiers = self
            .settings
            .output_modifiers()
            .iter()
            .map(modifier_name)
            .collect::<Vec<_>>()
            .join(", ");

        [
            format!("Hyperkey version: {}", env!("CARGO_PKG_VERSION")),
            format!("Enabled: {}", bool_label(self.settings.enabled())),
            format!("Trigger: {}", self.settings.trigger().display_label()),
            format!("Output modifiers: {modifiers}"),
            format!("Input status: {:?}", self.engine.status()),
            format!(
                "Input status error: {}",
                self.engine.status_error().unwrap_or_else(none)
            ),
            format!(
                "Last automatic hook recovery: {}",
                self.engine.last_recovery_message().unwrap_or_else(none)
            ),
            format!(
                "Launch at login: {}",
                bool_label(self.settings.launch_at_startup())
            ),
            format!(
                "Launch to tray: {}",
                bool_label(self.settings.launch_to_tray())
            ),
            format!("Diagnostic message: {diagnostic}"),
            format!("Recovery message: {recovery}"),
        ]
        .join("\r\n")
    }

    /// Persists first, then applies the startup registration when it changed,
    /// reconfigures the engine when the input configuration changed, and
    /// starts or stops the engine to match `enabled`. Any settings change ends
    /// an in-flight trigger capture.
    fn update_settings(&mut self, settings: HyperkeySettings, persist: bool) {
        let input_changed = self.settings.trigger() != settings.trigger()
            || self.settings.output_modifiers() != settings.output_modifiers();
        let startup_changed = self.settings.launch_at_startup() != settings.launch_at_startup();
        self.settings = settings;

        if persist {
            self.banner = self.store.save(&self.settings).err();
        }

        if startup_changed && self.manage_startup {
            self.startup_error = startup::apply(self.settings.launch_at_startup()).err();
        }

        if input_changed {
            self.engine.configure(
                self.settings.trigger(),
                self.settings.output_modifiers().to_vec(),
            );
        }

        if self.settings.enabled() {
            self.diagnostic_message = if self.engine.start() {
                None
            } else {
                Some(start_failure(
                    &self.engine,
                    "The keyboard hook could not be started.",
                ))
            };
        } else {
            self.engine.stop();
            self.diagnostic_message = None;
        }

        self.cancel_capture();
        self.copy_status = None;
        self.refresh_tray();
    }

    fn refresh_tray(&mut self) {
        if let Some(tray) = &mut self.tray {
            tray.update(
                &self.settings,
                self.engine.status() == InputEngineStatus::Running,
            );
        }
    }
}

/// The engine's own failure reason, or `fallback` when it reported none.
fn start_failure(engine: &InputEngine, fallback: &str) -> String {
    engine
        .status_error()
        .unwrap_or_else(|| fallback.to_string())
}

/// .NET renders `bool` as `True`/`False`; the diagnostic block keeps that
/// casing so support requests stay byte-identical across versions.
fn bool_label(value: bool) -> &'static str {
    if value { "True" } else { "False" }
}

fn modifier_name(modifier: &OutputModifier) -> &'static str {
    match modifier {
        OutputModifier::Control => "Control",
        OutputModifier::Alt => "Alt",
        OutputModifier::Shift => "Shift",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperkey_core::keys::TriggerKey;
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicU64, Ordering};

    fn test_directory(name: &str) -> PathBuf {
        static SEQUENCE: AtomicU64 = AtomicU64::new(0);
        let sequence = SEQUENCE.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "hyperkey-app-test-{}-{name}-{sequence}",
            std::process::id()
        ))
    }

    /// A controller whose engine is never started, whose store writes to a
    /// temp directory, and which never touches the registry or the tray.
    /// `enabled` stays false except where a test changes it, so updates take
    /// the engine's stop/no-op branch instead of installing a real hook.
    fn controller_in(directory: &Path) -> Controller {
        let settings = HyperkeySettings::defaults().with_enabled(false);
        let engine = InputEngine::new(settings.trigger(), settings.output_modifiers().to_vec());
        let (events, _receiver) = futures::channel::mpsc::unbounded();
        Controller {
            settings,
            store: SettingsStore::at(directory),
            engine,
            tray: None,
            events,
            banner: None,
            diagnostic_message: None,
            startup_error: None,
            copy_status: None,
            capturing: false,
            capture: None,
            trigger_hint: IDLE_TRIGGER_HINT.to_string(),
            modifier_hint: MODIFIER_CHOICE_HINT,
            manage_startup: false,
            is_quitting: false,
        }
    }

    fn key(virtual_key: u16, extended: bool, modifiers_held: bool) -> CapturedKey {
        CapturedKey {
            virtual_key,
            scan_code: 0,
            extended,
            modifiers_held,
        }
    }

    #[test]
    fn emergency_disable_never_persists_the_disabled_state() {
        // The next launch must be enabled again, so the persisted settings
        // still say enabled even after the action ran.
        let directory = test_directory("emergency");
        let mut controller = controller_in(&directory);
        controller.settings = controller.settings.with_enabled(true);
        controller.store.save(&controller.settings).unwrap();

        controller.emergency_disable();

        assert!(!controller.settings.enabled());
        let loaded = controller.store.load();
        assert!(
            loaded.settings.enabled(),
            "the persisted file must still say enabled"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_last_modifier_cannot_be_deselected() {
        // The settings and the file are left untouched.
        let directory = test_directory("last-modifier");
        let mut controller = controller_in(&directory);
        controller.settings = controller
            .settings
            .with_output_modifiers(vec![OutputModifier::Control])
            .expect("a single modifier is valid");

        controller.toggle_modifier(OutputModifier::Control, false);

        assert_eq!(
            &[OutputModifier::Control],
            controller.settings.output_modifiers()
        );
        assert_eq!(MODIFIER_REQUIRED_HINT, controller.modifier_hint());
        assert!(!directory.join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_valid_modifier_toggle_is_persisted() {
        let directory = test_directory("modifier-toggle");
        let mut controller = controller_in(&directory);

        controller.toggle_modifier(OutputModifier::Alt, false);

        assert_eq!(
            &[OutputModifier::Control, OutputModifier::Shift],
            controller.settings.output_modifiers()
        );
        let loaded = controller.store.load();
        assert_eq!(
            controller.settings.output_modifiers(),
            loaded.settings.output_modifiers()
        );
        assert!(!directory.join("settings.json.tmp").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn escape_cancels_trigger_capture() {
        // Escape is the documented cancel gesture and can never itself
        // become the trigger.
        let directory = test_directory("escape-cancel");
        let mut controller = controller_in(&directory);
        controller.capturing = true;
        controller.trigger_hint = CAPTURE_TRIGGER_HINT.to_string();

        controller.handle_captured_key(key(0x1B, false, false));

        assert!(!controller.is_capturing());
        assert_eq!(IDLE_TRIGGER_HINT, controller.trigger_hint());
        assert_eq!(TriggerKey::CAPS_LOCK, controller.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn modifier_and_system_keys_reject_during_capture() {
        // A chord can never be the trigger, and the hint says so.
        let directory = test_directory("modifier-reject");
        let mut controller = controller_in(&directory);
        controller.capturing = true;

        // Left Ctrl on its own.
        controller.handle_captured_key(key(0xA2, false, false));
        assert!(controller.is_capturing());
        assert_eq!(MODIFIER_KEY_HINT, controller.trigger_hint());

        // A normal key pressed while a modifier is held rejects the same way.
        controller.trigger_hint = CAPTURE_TRIGGER_HINT.to_string();
        controller.handle_captured_key(key(0x4A, false, true));
        assert!(controller.is_capturing());
        assert_eq!(MODIFIER_KEY_HINT, controller.trigger_hint());
        assert_eq!(TriggerKey::CAPS_LOCK, controller.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn an_unsupported_key_keeps_capturing_with_the_invalid_hint() {
        // Enter is an extended-scan-code key here, which tap replay cannot
        // reproduce.
        let directory = test_directory("unsupported");
        let mut controller = controller_in(&directory);
        controller.capturing = true;

        controller.handle_captured_key(key(0x0D, true, false));

        assert!(controller.is_capturing());
        assert_eq!(INVALID_KEY_HINT, controller.trigger_hint());
        assert_eq!(TriggerKey::CAPS_LOCK, controller.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn keys_arriving_after_capture_ended_are_ignored() {
        let directory = test_directory("late-key");
        let mut controller = controller_in(&directory);

        controller.handle_captured_key(key(0x78, false, false));

        assert_eq!(TriggerKey::CAPS_LOCK, controller.settings.trigger());
        assert!(!directory.join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_printable_key_rebinds_the_trigger_and_persists() {
        let directory = test_directory("rebind");
        let mut controller = controller_in(&directory);
        controller.capturing = true;

        controller.handle_captured_key(key(0x78, false, false));

        let expected = TriggerKey::try_create(0x78).unwrap();
        assert_eq!(expected, controller.settings.trigger());
        assert!(!controller.is_capturing());
        assert_eq!(IDLE_TRIGGER_HINT, controller.trigger_hint());
        let loaded = controller.store.load();
        assert_eq!(expected, loaded.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn any_settings_change_ends_capture() {
        let directory = test_directory("change-ends-capture");
        let mut controller = controller_in(&directory);
        controller.capturing = true;
        controller.trigger_hint = CAPTURE_TRIGGER_HINT.to_string();

        controller.set_launch_to_tray(true);

        assert!(!controller.is_capturing());
        assert_eq!(IDLE_TRIGGER_HINT, controller.trigger_hint());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn restart_hook_while_disabled_refuses_with_a_message() {
        let directory = test_directory("restart-disabled");
        let mut controller = controller_in(&directory);

        controller.restart_hook();

        assert_eq!(
            Some("Enable Hyperkey before restarting the keyboard hook."),
            controller.diagnostic_message()
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn diagnostic_details_list_every_field() {
        // The copy-paste block support requests expect.
        let directory = test_directory("diagnostics");
        let controller = controller_in(&directory);

        let expected = [
            format!("Hyperkey version: {}", env!("CARGO_PKG_VERSION")),
            "Enabled: False".to_string(),
            "Trigger: Caps Lock".to_string(),
            "Output modifiers: Control, Alt, Shift".to_string(),
            "Input status: Stopped".to_string(),
            "Input status error: None".to_string(),
            "Last automatic hook recovery: None".to_string(),
            "Launch at login: True".to_string(),
            "Launch to tray: False".to_string(),
            "Diagnostic message: None".to_string(),
            "Recovery message: None".to_string(),
        ]
        .join("\r\n");

        assert_eq!(expected, controller.diagnostic_details());
        let _ = std::fs::remove_dir_all(&directory);
    }
}
