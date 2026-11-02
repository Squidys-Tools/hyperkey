#![cfg_attr(not(test), windows_subsystem = "windows")]
//! Hyperkey settings daemon: a tray-resident process that owns the keyboard
//! hook engine. iced's daemon mode keeps the app alive with no window open;
//! the settings window is shown from the tray and hidden when closed.
mod clipboard;
mod keymap;
mod session_window;
mod settings_store;
mod settings_view;
mod single_instance;
mod startup;
mod tray;

use hyperkey_core::keys::TriggerKey;
use hyperkey_core::settings::{HyperkeySettings, OutputModifier};
use hyperkey_input::engine::{InputEngine, InputEngineStatus};
use iced::keyboard::key::{Code, Named, Physical};
use iced::keyboard::{self, Key};
use iced::theme::{self, Palette};
use iced::{Color, Element, Size, Subscription, Task, Theme, window};
use session_window::SessionEvent;
use settings_store::SettingsStore;
use std::time::Duration;

/// Keyboard focus targets, in the order they appear on screen. iced's
/// buttons and togglers do not take keyboard focus themselves, so the app
/// keeps an explicit focus ring: Tab and Shift+Tab move it and Enter or
/// Space activates the focused control.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum FocusTarget {
    EnabledToggle,
    RebindTrigger,
    ModifierControl,
    ModifierAlt,
    ModifierShift,
    LaunchAtStartup,
    LaunchToTray,
    RestartHook,
    EmergencyDisable,
    CopyDetails,
}

const FOCUS_ORDER: &[FocusTarget] = &[
    FocusTarget::EnabledToggle,
    FocusTarget::RebindTrigger,
    FocusTarget::ModifierControl,
    FocusTarget::ModifierAlt,
    FocusTarget::ModifierShift,
    FocusTarget::LaunchAtStartup,
    FocusTarget::LaunchToTray,
    FocusTarget::RestartHook,
    FocusTarget::EmergencyDisable,
    FocusTarget::CopyDetails,
];

const IDLE_TRIGGER_HINT: &str = "Select to rebind — press any key. Esc cancels.";
const CAPTURE_TRIGGER_HINT: &str = "Press the key to use as trigger. Esc cancels.";
const MODIFIER_KEY_HINT: &str =
    "Modifier and system keys can't be the trigger. Press a letter, digit, \
     function key, or punctuation — or press Esc to cancel.";
const INVALID_KEY_HINT: &str =
    "That key can't be used as the trigger. Try another key, or press Esc to cancel.";
const MODIFIER_CHOICE_HINT: &str = "Choose one or more modifiers.";
const MODIFIER_REQUIRED_HINT: &str = "Choose at least one modifier.";

#[derive(Debug, Clone)]
enum Message {
    ToggleEnabled,
    EnabledToggled(bool),
    ModifierToggled(OutputModifier, bool),
    LaunchAtStartupToggled(bool),
    LaunchToTrayToggled(bool),
    StartTriggerCapture,
    KeyboardEvent(keyboard::Event),
    OpenSettings,
    Quit,
    WindowCloseRequested(window::Id),
    Tick,
    RestartHook,
    EmergencyDisable,
    CopyDetails,
    SessionEvent(SessionEvent),
    SystemTheme(theme::Mode),
}

struct State {
    settings: HyperkeySettings,
    store: SettingsStore,
    engine: InputEngine,
    tray: Option<tray::Tray>,
    settings_window: Option<window::Id>,
    /// Banner for settings load/save problems: fatal quarantines, repairs,
    /// and write failures.
    banner: Option<String>,
    /// Hook lifecycle and restart notes, shown under Diagnostics.
    diagnostic_message: Option<String>,
    /// Startup-registration failure, reported like the C# diagnostic slot.
    startup_error: Option<String>,
    /// Result of the last Copy details action.
    copy_status: Option<String>,
    capturing_trigger: bool,
    trigger_hint: &'static str,
    modifier_hint: &'static str,
    focus: Option<FocusTarget>,
    last_engine_status: InputEngineStatus,
    system_mode: theme::Mode,
    is_quitting: bool,
}

fn boot(start_to_tray: bool) -> (State, Task<Message>) {
    let store = SettingsStore::new();
    let load = store.load();
    let settings = load.settings;

    let engine = InputEngine::new(settings.trigger(), settings.output_modifiers().to_vec());
    let startup_error = startup::apply(settings.launch_at_startup()).err();

    let mut diagnostic_message = None;
    if settings.enabled() && !engine.start() {
        diagnostic_message = Some(
            engine
                .status_error()
                .unwrap_or_else(|| "The keyboard hook could not be started.".to_string()),
        );
    }

    let last_engine_status = engine.status();
    let running = last_engine_status == InputEngineStatus::Running;
    let tray = match tray::Tray::create(&settings, running) {
        Ok(tray) => Some(tray),
        Err(_) => {
            diagnostic_message =
                Some("The system tray icon could not be registered.".to_string());
            None
        }
    };

    let mut state = State {
        settings,
        store,
        engine,
        tray,
        settings_window: None,
        banner: load.message,
        diagnostic_message,
        startup_error,
        copy_status: None,
        capturing_trigger: false,
        trigger_hint: IDLE_TRIGGER_HINT,
        modifier_hint: MODIFIER_CHOICE_HINT,
        focus: None,
        last_engine_status,
        system_mode: theme::Mode::None,
        is_quitting: false,
    };

    let initial_theme = iced::system::theme().map(Message::SystemTheme);
    if start_to_tray || state.settings.launch_to_tray() {
        (state, initial_theme)
    } else {
        let (id, task) = open_settings_window();
        state.settings_window = Some(id);
        state.focus = Some(FocusTarget::EnabledToggle);
        (state, Task::batch([task.discard(), initial_theme]))
    }
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::ToggleEnabled => {
            toggle_enabled(state);
            Task::none()
        }
        Message::EnabledToggled(enabled) => {
            update_settings(state, state.settings.with_enabled(enabled), true);
            Task::none()
        }
        Message::ModifierToggled(modifier, selected) => {
            toggle_modifier(state, modifier, selected);
            Task::none()
        }
        Message::LaunchAtStartupToggled(launch) => {
            update_settings(state, state.settings.with_launch_at_startup(launch), true);
            Task::none()
        }
        Message::LaunchToTrayToggled(to_tray) => {
            update_settings(state, state.settings.with_launch_to_tray(to_tray), true);
            Task::none()
        }
        Message::StartTriggerCapture => {
            if !state.capturing_trigger {
                state.capturing_trigger = true;
                state.trigger_hint = CAPTURE_TRIGGER_HINT;
            }
            Task::none()
        }
        Message::KeyboardEvent(event) => {
            if state.capturing_trigger {
                capture_trigger_key(state, &event);
            } else {
                navigate_focus(state, &event);
            }
            Task::none()
        }
        Message::OpenSettings => show_settings_window(state),
        Message::Quit => quit(state),
        Message::WindowCloseRequested(id) => {
            if state.settings_window == Some(id) {
                window::set_mode(id, window::Mode::Hidden)
            } else {
                window::close(id)
            }
        }
        // The engine publishes status through subscribe(); the app still polls
        // on a slow tick to re-render the status line, mirror the C#
        // failed-status diagnostic, and keep the tray icon in sync with hook
        // health.
        Message::Tick => {
            let status = state.engine.status();
            if status != state.last_engine_status {
                state.last_engine_status = status;
                if status == InputEngineStatus::Failed && state.diagnostic_message.is_none() {
                    state.diagnostic_message = state
                        .engine
                        .status_error()
                        .or_else(|| Some("The keyboard hook stopped.".to_string()));
                }

                refresh_tray(state);
            }
            Task::none()
        }
        Message::RestartHook => {
            restart_hook(state);
            Task::none()
        }
        Message::EmergencyDisable => {
            apply_emergency_disable(state);
            Task::none()
        }
        Message::CopyDetails => {
            copy_details(state);
            Task::none()
        }
        Message::SessionEvent(SessionEvent::ReconcileEngine) => {
            if !state.is_quitting {
                state.engine.reconcile();
            }
            Task::none()
        }
        Message::SessionEvent(SessionEvent::ShowSettings) => show_settings_window(state),
        Message::SystemTheme(mode) => {
            state.system_mode = mode;
            Task::none()
        }
    }
}

/// Mirrors the C# `UpdateSettings`: persist first, then apply the startup
/// registration when it changed, reconfigure the engine when the input
/// configuration changed, and start or stop the engine to match `enabled`.
fn update_settings(state: &mut State, settings: HyperkeySettings, persist: bool) {
    let input_configuration_changed = state.settings.trigger() != settings.trigger()
        || state.settings.output_modifiers() != settings.output_modifiers();
    let launch_at_startup_changed =
        state.settings.launch_at_startup() != settings.launch_at_startup();
    state.settings = settings;

    if persist {
        match state.store.save(&state.settings) {
            Ok(()) => state.banner = None,
            Err(error) => state.banner = Some(error),
        }
    }

    if launch_at_startup_changed {
        state.startup_error = startup::apply(state.settings.launch_at_startup()).err();
    }

    if input_configuration_changed {
        state.engine.configure(
            state.settings.trigger(),
            state.settings.output_modifiers().to_vec(),
        );
    }

    if state.settings.enabled() {
        if state.engine.start() {
            state.diagnostic_message = None;
        } else {
            state.diagnostic_message = Some(
                state
                    .engine
                    .status_error()
                    .unwrap_or_else(|| "The keyboard hook could not be started.".to_string()),
            );
        }
    } else {
        state.engine.stop();
        state.diagnostic_message = None;
    }

    // A settings change ends any in-flight trigger capture, like the C#
    // ApplySettings → ExitTriggerCapture path.
    state.capturing_trigger = false;
    state.trigger_hint = IDLE_TRIGGER_HINT;
    state.copy_status = None;

    refresh_tray(state);
}

fn toggle_modifier(state: &mut State, modifier: OutputModifier, selected: bool) {
    let mut modifiers: Vec<OutputModifier> = state.settings.output_modifiers().to_vec();
    if selected {
        if !modifiers.contains(&modifier) {
            modifiers.push(modifier);
        }
    } else {
        modifiers.retain(|existing| *existing != modifier);
    }

    if modifiers.is_empty() {
        // The C# window re-checks the box and asks for at least one modifier.
        state.modifier_hint = MODIFIER_REQUIRED_HINT;
        return;
    }

    match state.settings.with_output_modifiers(modifiers) {
        Ok(settings) => {
            state.modifier_hint = MODIFIER_CHOICE_HINT;
            update_settings(state, settings, true);
        }
        Err(_) => state.modifier_hint = MODIFIER_REQUIRED_HINT,
    }
}

/// Key-press capture for trigger rebinding, mirroring the C#
/// PreviewKeyDown handler: every key sets the trigger when the core
/// allowlist accepts it, Escape cancels, and modifiers get the more
/// specific rejection hint. Repeats are ignored so the key that activated
/// capture cannot bind itself while it is still held.
fn capture_trigger_key(state: &mut State, event: &keyboard::Event) {
    let keyboard::Event::KeyPressed {
        key,
        physical_key,
        modifiers,
        repeat: false,
        ..
    } = event
    else {
        return;
    };

    // Presses without a physical code (IME/vendor keys) match the C#
    // KeyInterop failure path: the invalid-key hint, not silence.
    let Physical::Code(code) = physical_key else {
        state.trigger_hint = INVALID_KEY_HINT;
        return;
    };

    match code {
        Code::Escape => {
            state.capturing_trigger = false;
            state.trigger_hint = IDLE_TRIGGER_HINT;
        }
        // Mirroring the C# IsModifierOrSystemKey + HasHeldModifiers check:
        // a chord can never be the trigger.
        code if keymap::is_modifier_or_system(*code)
            || modifiers.control()
            || modifiers.alt()
            || modifiers.shift()
            || modifiers.logo() =>
        {
            state.trigger_hint = MODIFIER_KEY_HINT;
        }
        code => match keymap::trigger_virtual_key(key, *code).and_then(TriggerKey::try_create) {
            Some(trigger) => match state.settings.with_trigger(trigger) {
                Ok(settings) => update_settings(state, settings, true),
                Err(_) => state.trigger_hint = INVALID_KEY_HINT,
            },
            None => state.trigger_hint = INVALID_KEY_HINT,
        },
    }
}

/// Tab/Shift+Tab moves the focus ring; Enter and Space activate the focused
/// control, matching what a native focus model would do.
fn navigate_focus(state: &mut State, event: &keyboard::Event) {
    let keyboard::Event::KeyPressed {
        key, modifiers, repeat: false, ..
    } = event
    else {
        return;
    };

    match key.as_ref() {
        Key::Named(Named::Tab) => {
            let step: isize = if modifiers.shift() { -1 } else { 1 };
            let count = FOCUS_ORDER.len() as isize;
            let current = state
                .focus
                .and_then(|target| FOCUS_ORDER.iter().position(|entry| *entry == target))
                .unwrap_or(0) as isize;
            state.focus = Some(FOCUS_ORDER[((current + step + count) % count) as usize]);
        }
        Key::Named(Named::Enter) | Key::Named(Named::Space) => {
            activate_focused(state);
        }
        _ => {}
    }
}

fn activate_focused(state: &mut State) {
    match state.focus {
        Some(FocusTarget::EnabledToggle) => toggle_enabled(state),
        Some(FocusTarget::RebindTrigger) => {
            state.capturing_trigger = true;
            state.trigger_hint = CAPTURE_TRIGGER_HINT;
        }
        Some(FocusTarget::ModifierControl) => {
            toggle_modifier(state, OutputModifier::Control, !has_modifier(state, OutputModifier::Control));
        }
        Some(FocusTarget::ModifierAlt) => {
            toggle_modifier(state, OutputModifier::Alt, !has_modifier(state, OutputModifier::Alt));
        }
        Some(FocusTarget::ModifierShift) => {
            toggle_modifier(state, OutputModifier::Shift, !has_modifier(state, OutputModifier::Shift));
        }
        Some(FocusTarget::LaunchAtStartup) => {
            update_settings(
                state,
                state
                    .settings
                    .with_launch_at_startup(!state.settings.launch_at_startup()),
                true,
            );
        }
        Some(FocusTarget::LaunchToTray) => {
            update_settings(
                state,
                state
                    .settings
                    .with_launch_to_tray(!state.settings.launch_to_tray()),
                true,
            );
        }
        Some(FocusTarget::RestartHook) => restart_hook(state),
        Some(FocusTarget::EmergencyDisable) => apply_emergency_disable(state),
        Some(FocusTarget::CopyDetails) => copy_details(state),
        None => {}
    }
}

fn has_modifier(state: &State, modifier: OutputModifier) -> bool {
    state.settings.output_modifiers().contains(&modifier)
}

fn toggle_enabled(state: &mut State) {
    update_settings(
        state,
        state.settings.with_enabled(!state.settings.enabled()),
        true,
    );
}

/// Emergency disable releases the modifier layer and stops the engine, but
/// deliberately does not persist `enabled = false`: the next launch
/// reactivates unless the user toggles the setting off.
fn apply_emergency_disable(state: &mut State) {
    state.engine.emergency_disable();
    update_settings(state, state.settings.with_enabled(false), false);
}

fn copy_details(state: &mut State) {
    state.copy_status = match clipboard::set_text(&diagnostic_details(state)) {
        Ok(()) => Some("Diagnostic details copied to the clipboard.".to_string()),
        Err(error) => Some(format!(
            "Diagnostic details could not be copied: {error}"
        )),
    };
}

/// Mirrors the C# RestartInputEngine: refuse while disabled, prefer the
/// engine's hook-only restart, and fall back to a full stop/start cycle.
fn restart_hook(state: &mut State) {
    if !state.settings.enabled() {
        state.diagnostic_message =
            Some("Enable Hyperkey before restarting the keyboard hook.".to_string());
        return;
    }

    if state.engine.restart_hook() {
        state.diagnostic_message =
            Some("The keyboard hook restart was requested.".to_string());
        return;
    }

    state.engine.stop();
    state.diagnostic_message = if state.engine.start() {
        Some("The keyboard hook was restarted successfully.".to_string())
    } else {
        Some(
            state
                .engine
                .status_error()
                .unwrap_or_else(|| "The keyboard hook could not be restarted.".to_string()),
        )
    };
}

/// The C# DiagnosticDetails block, copied verbatim as plain text.
fn diagnostic_details(state: &State) -> String {
    let none = || "None".to_string();
    let diagnostic = state
        .startup_error
        .as_ref()
        .or(state.diagnostic_message.as_ref())
        .cloned()
        .unwrap_or_else(none);
    let recovery = state.banner.clone().unwrap_or_else(none);
    let status_error = state.engine.status_error().unwrap_or_else(none);
    let modifiers = state
        .settings
        .output_modifiers()
        .iter()
        .map(modifier_name)
        .collect::<Vec<_>>()
        .join(", ");

    [
        format!("Hyperkey version: {}", env!("CARGO_PKG_VERSION")),
        format!("Enabled: {}", bool_label(state.settings.enabled())),
        format!("Trigger: {}", state.settings.trigger().display_label()),
        format!("Output modifiers: {modifiers}"),
        format!("Input status: {:?}", state.engine.status()),
        format!("Input status error: {status_error}"),
        format!(
            "Last automatic hook recovery: {}",
            state.engine.last_recovery_message().unwrap_or_else(none)
        ),
        format!("Launch at login: {}", bool_label(state.settings.launch_at_startup())),
        format!("Launch to tray: {}", bool_label(state.settings.launch_to_tray())),
        format!("Diagnostic message: {diagnostic}"),
        format!("Recovery message: {recovery}"),
    ]
    .join("\r\n")
}

/// .NET renders `bool` as `True`/`False`; the diagnostic block keeps that
/// casing so it stays byte-identical to the C# output.
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

fn refresh_tray(state: &mut State) {
    if let Some(tray) = &mut state.tray {
        tray.update(
            &state.settings,
            state.engine.status() == InputEngineStatus::Running,
        );
    }
}

fn open_settings_window() -> (window::Id, Task<window::Id>) {
    window::open(window::Settings {
        size: Size::new(760.0, 540.0),
        min_size: Some(Size::new(720.0, 500.0)),
        position: window::Position::Centered,
        // Closing hides the window; the daemon keeps running in the tray.
        exit_on_close_request: false,
        icon: window::icon::from_rgba(tray::icon_rgba(true), 32, 32).ok(),
        ..Default::default()
    })
}

fn show_settings_window(state: &mut State) -> Task<Message> {
    if state.focus.is_none() {
        state.focus = Some(FocusTarget::EnabledToggle);
    }

    match state.settings_window {
        Some(id) => Task::batch([
            window::set_mode(id, window::Mode::Windowed),
            window::gain_focus(id),
        ]),
        None => {
            let (id, task) = open_settings_window();
            state.settings_window = Some(id);
            task.discard()
        }
    }
}

fn quit(state: &mut State) -> Task<Message> {
    if state.is_quitting {
        return Task::none();
    }

    state.is_quitting = true;
    state.engine.emergency_disable();
    state.engine.stop();
    drop(state.tray.take());
    iced::exit()
}

fn title(_state: &State, _window: window::Id) -> String {
    "Hyperkey".to_string()
}

fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(tray_events),
        Subscription::run(status_tick),
        Subscription::run(session_events),
        keyboard::listen().map(Message::KeyboardEvent),
        window::close_requests().map(Message::WindowCloseRequested),
        iced::system::theme_changes().map(Message::SystemTheme),
    ])
}

/// The engine offers subscribe() for status changes, but the daemon polls on
/// a slow tick to refresh the status line and tray icon and mirror the C#
/// failed-status diagnostic. The default thread-pool executor has no timer,
/// hence a dedicated sleeping thread.
fn status_tick() -> impl iced::futures::stream::Stream<Item = Message> {
    iced::stream::channel(4, |sender: iced::futures::channel::mpsc::Sender<Message>| async move {
        let _ = std::thread::Builder::new()
            .name("Hyperkey status tick".to_string())
            .spawn(move || loop {
                std::thread::sleep(Duration::from_millis(500));
                match sender.clone().try_send(Message::Tick) {
                    Ok(()) => {}
                    // A full channel drops a tick; only a dead receiver stops it.
                    Err(error) if error.is_disconnected() => break,
                    Err(_) => {}
                }
            });
    })
}

/// Bridges the tray-icon event channels into iced messages. tray-icon delivers
/// events on crossbeam channels; dedicated blocking threads forward them into
/// the subscription's mpsc channel so the daemon sees them immediately.
fn tray_events() -> impl iced::futures::stream::Stream<Item = Message> {
    iced::stream::channel(32, |sender: iced::futures::channel::mpsc::Sender<Message>| async move {
        let menu_sender = sender.clone();
        let _ = std::thread::Builder::new()
            .name("Hyperkey tray menu events".to_string())
            .spawn(move || {
                let receiver = tray_icon::menu::MenuEvent::receiver();
                while let Ok(event) = receiver.recv() {
                    let message = match event.id.0.as_str() {
                        tray::MENU_ID_TOGGLE => Message::ToggleEnabled,
                        tray::MENU_ID_OPEN_SETTINGS => Message::OpenSettings,
                        tray::MENU_ID_QUIT => Message::Quit,
                        _ => continue,
                    };
                    match menu_sender.clone().try_send(message) {
                        // A full channel drops one menu event; only a dead
                        // receiver stops the pump.
                        Err(error) if error.is_disconnected() => break,
                        _ => {}
                    }
                }
            });

        let _ = std::thread::Builder::new()
            .name("Hyperkey tray icon events".to_string())
            .spawn(move || {
                let receiver = tray_icon::TrayIconEvent::receiver();
                while let Ok(event) = receiver.recv() {
                    if let tray_icon::TrayIconEvent::DoubleClick {
                        button: tray_icon::MouseButton::Left,
                        ..
                    } = event
                    {
                        match sender.clone().try_send(Message::OpenSettings) {
                            Err(error) if error.is_disconnected() => break,
                            _ => {}
                        }
                    }
                }
            });
    })
}

/// Power, session, and cross-instance notifications arrive on the hidden
/// session window's thread; this bridge forwards them into the daemon.
fn session_events() -> impl iced::futures::stream::Stream<Item = Message> {
    iced::stream::channel(8, |sender: iced::futures::channel::mpsc::Sender<Message>| async move {
        session_window::spawn(move |event| {
            let _ = sender.clone().try_send(Message::SessionEvent(event));
        });
    })
}

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    settings_view::view(state)
}

/// The Hyperkey theme, following the system mode. In dark mode it uses the
/// design palette from the WPF prototype (canvas #1B1C20, accent #4C8DF6);
/// in light mode it keeps iced's light palette with the deeper accent
/// (#3B74D8). Before the first SystemTheme message arrives `Mode::None`
/// resolves to the light palette.
fn theme(state: &State, _window: window::Id) -> Theme {
    match state.system_mode {
        theme::Mode::Dark => Theme::custom(
            "Hyperkey Dark",
            Palette {
                background: settings_view::palette::CANVAS,
                text: Color::WHITE,
                primary: settings_view::palette::ACCENT,
                ..Palette::DARK
            },
        ),
        theme::Mode::None | theme::Mode::Light => Theme::custom(
            "Hyperkey",
            Palette {
                primary: settings_view::palette::ACCENT_STRONG,
                ..Palette::LIGHT
            },
        ),
    }
}

fn main() -> iced::Result {
    // Only one Hyperkey may own the keyboard hook at a time; the name is
    // shared with the C# build so the two cannot run side by side either.
    let _single_instance = match single_instance::acquire() {
        Ok(Some(guard)) => guard,
        Ok(None) => {
            // Ask the running instance to surface its settings window; fall
            // back to a visible message when the broadcast cannot be posted.
            if !single_instance::notify_existing() {
                session_window::message_box(
                    "Hyperkey is already running.",
                    "Hyperkey",
                );
            }
            return Ok(());
        }
        Err(error) => {
            session_window::message_box(
                &format!("Hyperkey could not start: {error}"),
                "Hyperkey",
            );
            return Ok(());
        }
    };

    let start_to_tray = std::env::args().any(|arg| arg == "--tray");

    iced::daemon(move || boot(start_to_tray), update, view)
        .title(title)
        .subscription(subscription)
        .theme(theme)
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use iced::keyboard::{Location, Modifiers};
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

    /// A state whose engine is never started and whose store writes to a
    /// temp directory, so `update` can run headless. `enabled` stays false
    /// except where a test changes it, so the update paths exercise the
    /// engine's stop/no-op branch instead of installing a real hook.
    fn state_in(directory: &Path) -> State {
        let settings = HyperkeySettings::defaults().with_enabled(false);
        let engine = InputEngine::new(settings.trigger(), settings.output_modifiers().to_vec());
        State {
            settings,
            store: SettingsStore::at(directory),
            engine,
            tray: None,
            settings_window: None,
            banner: None,
            diagnostic_message: None,
            startup_error: None,
            copy_status: None,
            capturing_trigger: false,
            trigger_hint: IDLE_TRIGGER_HINT,
            modifier_hint: MODIFIER_CHOICE_HINT,
            focus: None,
            last_engine_status: InputEngineStatus::Stopped,
            system_mode: theme::Mode::None,
            is_quitting: false,
        }
    }

    fn key_press(key: Key, code: Code, modifiers: Modifiers, repeat: bool) -> Message {
        Message::KeyboardEvent(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: key,
            physical_key: Physical::Code(code),
            location: Location::Standard,
            modifiers,
            text: None,
            repeat,
        })
    }

    #[test]
    fn emergency_disable_never_persists_the_disabled_state() {
        // The C# emergency path reset the engine without touching the file:
        // the next launch must be enabled again, so the persisted settings
        // still say enabled even after the action ran.
        let directory = test_directory("emergency");
        let mut state = state_in(&directory);
        state.settings = state.settings.with_enabled(true);
        state.store.save(&state.settings).unwrap();

        let _ = update(&mut state, Message::EmergencyDisable);

        assert!(!state.settings.enabled());
        let loaded = state.store.load();
        assert!(
            loaded.settings.enabled(),
            "the persisted file must still say enabled"
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn the_last_modifier_cannot_be_deselected() {
        // The C# window re-checked the box instead of producing an empty
        // modifier set: the settings and the file are left untouched.
        let directory = test_directory("last-modifier");
        let mut state = state_in(&directory);
        state.settings = state
            .settings
            .with_output_modifiers(vec![OutputModifier::Control])
            .expect("a single modifier is valid");

        let _ = update(
            &mut state,
            Message::ModifierToggled(OutputModifier::Control, false),
        );

        assert_eq!(
            &[OutputModifier::Control],
            state.settings.output_modifiers()
        );
        assert_eq!(MODIFIER_REQUIRED_HINT, state.modifier_hint);
        assert!(!directory.join("settings.json").exists());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_valid_modifier_toggle_is_persisted() {
        let directory = test_directory("modifier-toggle");
        let mut state = state_in(&directory);

        let _ = update(
            &mut state,
            Message::ModifierToggled(OutputModifier::Alt, false),
        );

        assert_eq!(
            &[OutputModifier::Control, OutputModifier::Shift],
            state.settings.output_modifiers()
        );
        let loaded = state.store.load();
        assert_eq!(
            state.settings.output_modifiers(),
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
        let mut state = state_in(&directory);
        state.capturing_trigger = true;
        state.trigger_hint = CAPTURE_TRIGGER_HINT;

        let _ = update(
            &mut state,
            key_press(
                Key::Named(Named::Escape),
                Code::Escape,
                Modifiers::empty(),
                false,
            ),
        );

        assert!(!state.capturing_trigger);
        assert_eq!(IDLE_TRIGGER_HINT, state.trigger_hint);
        assert_eq!(TriggerKey::CAPS_LOCK, state.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn modifier_and_system_keys_reject_during_capture() {
        // The C# IsModifierOrSystemKey + HasHeldModifiers check: a chord can
        // never be the trigger, and the hint says so.
        let directory = test_directory("modifier-reject");
        let mut state = state_in(&directory);
        state.capturing_trigger = true;

        let _ = update(
            &mut state,
            key_press(
                Key::Named(Named::Control),
                Code::ControlLeft,
                Modifiers::empty(),
                false,
            ),
        );
        assert!(state.capturing_trigger);
        assert_eq!(MODIFIER_KEY_HINT, state.trigger_hint);

        // A normal key pressed while a modifier is held rejects the same way.
        let _ = update(
            &mut state,
            key_press(
                Key::Character("j".into()),
                Code::KeyJ,
                Modifiers::CTRL,
                false,
            ),
        );
        assert!(state.capturing_trigger);
        assert_eq!(MODIFIER_KEY_HINT, state.trigger_hint);
        assert_eq!(TriggerKey::CAPS_LOCK, state.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_held_key_repeat_cannot_bind_itself() {
        // The press that started capture auto-repeats while held; repeats
        // are ignored so the activating key cannot bind itself.
        let directory = test_directory("repeat");
        let mut state = state_in(&directory);
        state.capturing_trigger = true;

        let _ = update(
            &mut state,
            key_press(
                Key::Named(Named::Enter),
                Code::Enter,
                Modifiers::empty(),
                true,
            ),
        );

        assert!(state.capturing_trigger);
        assert_eq!(TriggerKey::CAPS_LOCK, state.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn a_printable_key_rebinds_the_trigger_and_persists() {
        // A normal key captures to its virtual key and is saved. A named key
        // is used rather than a Character: VkKeyScanW resolves characters
        // through the active layout, so 'j' would not be VK_J on e.g. Dvorak.
        let directory = test_directory("rebind");
        let mut state = state_in(&directory);
        state.capturing_trigger = true;

        let _ = update(
            &mut state,
            key_press(
                Key::Named(Named::F9),
                Code::F9,
                Modifiers::empty(),
                false,
            ),
        );

        let expected = TriggerKey::try_create(0x78).unwrap();
        assert_eq!(expected, state.settings.trigger());
        assert!(!state.capturing_trigger);
        assert_eq!(IDLE_TRIGGER_HINT, state.trigger_hint);
        let loaded = state.store.load();
        assert_eq!(expected, loaded.settings.trigger());
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn tab_and_shift_tab_move_the_focus_ring() {
        // The explicit focus ring: Tab moves forward and wraps; Shift+Tab
        // moves backward and wraps the other way.
        let directory = test_directory("focus-ring");
        let mut state = state_in(&directory);
        state.focus = Some(FocusTarget::EnabledToggle);

        let _ = update(
            &mut state,
            key_press(Key::Named(Named::Tab), Code::Tab, Modifiers::empty(), false),
        );
        assert_eq!(Some(FocusTarget::RebindTrigger), state.focus);

        state.focus = Some(FocusTarget::CopyDetails);
        let _ = update(
            &mut state,
            key_press(Key::Named(Named::Tab), Code::Tab, Modifiers::empty(), false),
        );
        assert_eq!(Some(FocusTarget::EnabledToggle), state.focus);

        let _ = update(
            &mut state,
            key_press(Key::Named(Named::Tab), Code::Tab, Modifiers::SHIFT, false),
        );
        assert_eq!(Some(FocusTarget::CopyDetails), state.focus);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn restart_hook_while_disabled_reports_the_csharp_message() {
        // C# RestartInputEngine refused the request while disabled instead
        // of silently starting the hook.
        let directory = test_directory("restart-disabled");
        let mut state = state_in(&directory);

        let _ = update(&mut state, Message::RestartHook);

        assert_eq!(
            "Enable Hyperkey before restarting the keyboard hook.",
            state.diagnostic_message.as_deref().unwrap()
        );
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn diagnostic_details_match_the_csharp_block() {
        // The C# DiagnosticDetails text, field for field — the copy-paste
        // block support requests expect.
        let directory = test_directory("diagnostics");
        let state = state_in(&directory);

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

        assert_eq!(expected, diagnostic_details(&state));
        let _ = std::fs::remove_dir_all(&directory);
    }
}
