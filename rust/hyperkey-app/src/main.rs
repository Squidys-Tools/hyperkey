//! Hyperkey settings daemon: a tray-resident process that owns the keyboard
//! hook engine. iced's daemon mode keeps the app alive with no window open;
//! the settings window is shown from the tray and hidden when closed.
mod settings_store;
mod single_instance;
mod tray;

use hyperkey_core::settings::HyperkeySettings;
use hyperkey_input::engine::{InputEngine, InputEngineStatus};
use iced::widget::{button, column, container, text};
use iced::{Element, Fill, Size, Subscription, Task, window};
use settings_store::SettingsStore;
use std::time::Duration;

#[derive(Debug, Clone)]
enum Message {
    ToggleEnabled,
    OpenSettings,
    HideSettings,
    Quit,
    WindowCloseRequested(window::Id),
    Tick,
}

struct State {
    settings: HyperkeySettings,
    store: SettingsStore,
    engine: InputEngine,
    tray: Option<tray::Tray>,
    settings_window: Option<window::Id>,
    recovery_message: Option<String>,
    diagnostic_message: Option<String>,
    is_quitting: bool,
}

fn boot() -> (State, Task<Message>) {
    let store = SettingsStore::new();
    let load = store.load();
    let settings = load.settings;

    let engine = InputEngine::new(settings.trigger(), settings.output_modifiers().to_vec());
    let mut diagnostic_message = None;
    if settings.enabled() && !engine.start() {
        diagnostic_message = Some(
            engine
                .status_error()
                .unwrap_or_else(|| "The keyboard hook could not be started.".to_string()),
        );
    }

    let tray = match tray::Tray::create(&settings) {
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
        recovery_message: load.message,
        diagnostic_message,
        is_quitting: false,
    };

    if state.settings.launch_to_tray() {
        (state, Task::none())
    } else {
        let (id, task) = open_settings_window();
        state.settings_window = Some(id);
        (state, task.discard())
    }
}

fn update(state: &mut State, message: Message) -> Task<Message> {
    match message {
        Message::ToggleEnabled => {
            toggle_enabled(state);
            Task::none()
        }
        Message::OpenSettings => show_settings_window(state),
        Message::HideSettings => match state.settings_window {
            Some(id) => window::set_mode(id, window::Mode::Hidden),
            None => Task::none(),
        },
        Message::Quit => quit(state),
        Message::WindowCloseRequested(id) => {
            if state.settings_window == Some(id) {
                window::set_mode(id, window::Mode::Hidden)
            } else {
                window::close(id)
            }
        }
        // The engine has no status callback; the tick re-renders the status
        // line and mirrors the C# failed-status diagnostic.
        Message::Tick => {
            if state.engine.status() == InputEngineStatus::Failed
                && state.diagnostic_message.is_none()
            {
                state.diagnostic_message = state
                    .engine
                    .status_error()
                    .or_else(|| Some("The keyboard hook stopped.".to_string()));
            }
            Task::none()
        }
    }
}

fn view(state: &State, _window: window::Id) -> Element<'_, Message> {
    let mut content = column![
        text("Hyperkey").size(30),
        text(if state.settings.enabled() {
            "Enabled"
        } else {
            "Disabled"
        }),
        text(format!(
            "Trigger key: {}",
            state.settings.trigger().display_label()
        )),
        text(format!(
            "Output modifiers: {}",
            modifiers_label(&state.settings)
        )),
        text(format!(
            "Input engine: {}",
            status_label(state.engine.status())
        )),
    ]
    .spacing(10)
    .padding(24);

    if let Some(error) = state.engine.status_error() {
        content = content.push(text(format!("Engine error: {error}")));
    }
    if let Some(message) = &state.diagnostic_message {
        content = content.push(text(message.clone()));
    }
    if let Some(message) = &state.recovery_message {
        content = content.push(text(message.clone()));
    }
    content = content.push(text(format!(
        "Settings file: {}",
        state.store.settings_path().display()
    )));
    content = content.push(button("Close").on_press(Message::HideSettings));

    container(content).center(Fill).into()
}

fn title(_state: &State, _window: window::Id) -> String {
    "Hyperkey".to_string()
}

fn subscription(_state: &State) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(tray_events),
        Subscription::run(status_tick),
        window::close_requests().map(Message::WindowCloseRequested),
    ])
}

/// The engine has no status callback, so a slow tick refreshes the status line
/// and mirrors the C# failed-status diagnostic. The default thread-pool
/// executor has no timer, hence a dedicated sleeping thread.
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
                    if menu_sender.clone().try_send(message).is_err() {
                        break;
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
                        && sender.clone().try_send(Message::OpenSettings).is_err()
                    {
                        break;
                    }
                }
            });
    })
}

fn open_settings_window() -> (window::Id, Task<window::Id>) {
    window::open(window::Settings {
        size: Size::new(440.0, 420.0),
        min_size: Some(Size::new(360.0, 320.0)),
        position: window::Position::Centered,
        // Closing hides the window; the daemon keeps running in the tray.
        exit_on_close_request: false,
        ..Default::default()
    })
}

fn show_settings_window(state: &mut State) -> Task<Message> {
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

fn toggle_enabled(state: &mut State) {
    let enabled = !state.settings.enabled();
    state.settings = state.settings.with_enabled(enabled);

    match state.store.save(&state.settings) {
        Ok(()) => state.recovery_message = None,
        Err(error) => state.recovery_message = Some(error),
    }

    if enabled {
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

    if let Some(tray) = &state.tray {
        tray.update(&state.settings);
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

fn modifiers_label(settings: &HyperkeySettings) -> String {
    settings
        .output_modifiers()
        .iter()
        .map(|modifier| match modifier {
            hyperkey_core::settings::OutputModifier::Control => "Control",
            hyperkey_core::settings::OutputModifier::Alt => "Alt",
            hyperkey_core::settings::OutputModifier::Shift => "Shift",
        })
        .collect::<Vec<_>>()
        .join(" + ")
}

fn status_label(status: InputEngineStatus) -> &'static str {
    match status {
        InputEngineStatus::Stopped => "Stopped",
        InputEngineStatus::Starting => "Starting",
        InputEngineStatus::Running => "Running",
        InputEngineStatus::Stopping => "Stopping",
        InputEngineStatus::Failed => "Failed",
    }
}

fn main() -> iced::Result {
    // Only one Hyperkey may own the keyboard hook at a time.
    let _single_instance = match single_instance::acquire() {
        Some(guard) => guard,
        None => return Ok(()),
    };

    iced::daemon(boot, update, view)
        .title(title)
        .subscription(subscription)
        .run()
}
