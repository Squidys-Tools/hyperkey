//! Runtime wiring: one event channel fed by background threads (tray, session
//! window, engine, key capture) and drained on the GPUI main thread, plus the
//! settings window lifecycle. The process is a tray daemon, so the settings
//! window is created on demand and only ever hidden.
use crate::controller::{AppEvent, Controller};
use crate::platform::session_window::{self, SessionEvent};
use crate::platform::tray;
use crate::ui::settings_window::{FocusNext, FocusPrev, SettingsWindow};
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{
    App, AppContext, Application, Bounds, Entity, KeyBinding, TitlebarOptions, WindowBounds,
    WindowHandle, WindowOptions, px, size,
};
use hyperkey_input::engine::EngineEvent;
use std::sync::mpsc::Receiver;
use tray_icon::menu::MenuEvent;
use tray_icon::{MouseButton, TrayIconEvent};

/// Runs the application until the tray menu's Quit.
pub fn run(start_to_tray: bool) {
    Application::new().run(move |cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("tab", FocusNext, None),
            KeyBinding::new("shift-tab", FocusPrev, None),
        ]);

        let (events, mut incoming) = unbounded::<AppEvent>();
        let controller = cx.new(|_| Controller::new(events.clone()));
        let start_hidden = start_to_tray || controller.read(cx).settings().launch_to_tray();
        spawn_producers(&events, controller.read(cx).subscribe_engine());

        let mut window = None;
        if !start_hidden {
            open_settings(&controller, &mut window, cx);
        }

        // Lives for the whole process: dropping it would stop event handling.
        cx.spawn(async move |cx| {
            while let Some(event) = incoming.next().await {
                if cx
                    .update(|cx| handle(event, &controller, &mut window, cx))
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    });
}

/// Applies one event on the main thread.
fn handle(
    event: AppEvent,
    controller: &Entity<Controller>,
    window: &mut Option<WindowHandle<SettingsWindow>>,
    cx: &mut App,
) {
    match event {
        AppEvent::OpenSettings | AppEvent::Session(SessionEvent::ShowSettings) => {
            open_settings(controller, window, cx);
        }
        AppEvent::Quit => {
            controller.update(cx, |controller, _| controller.shutdown());
            cx.quit();
        }
        AppEvent::ToggleEnabled => update(controller, cx, Controller::toggle_enabled),
        AppEvent::Session(event) => update(controller, cx, |c| c.on_session_event(event)),
        AppEvent::Engine(event) => update(controller, cx, |c| c.on_engine_event(event)),
        AppEvent::Captured(key) => update(controller, cx, |c| c.handle_captured_key(key)),
    }
}

/// Mutates the controller and notifies its observers, so the window redraws.
fn update(controller: &Entity<Controller>, cx: &mut App, operation: impl FnOnce(&mut Controller)) {
    controller.update(cx, |controller, cx| {
        operation(controller);
        cx.notify();
    });
}

/// Shows the existing settings window, or opens one if none exists yet.
fn open_settings(
    controller: &Entity<Controller>,
    window: &mut Option<WindowHandle<SettingsWindow>>,
    cx: &mut App,
) {
    if let Some(handle) = window.as_ref()
        && handle
            .update(cx, |view, window, cx| view.reveal(window, cx))
            .is_ok()
    {
        cx.activate(true);
        return;
    }

    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::centered(
            None,
            size(px(560.0), px(700.0)),
            cx,
        ))),
        titlebar: Some(TitlebarOptions {
            title: Some("Hyperkey".into()),
            ..Default::default()
        }),
        window_min_size: Some(size(px(480.0), px(520.0))),
        ..Default::default()
    };
    let controller = controller.clone();
    if let Ok(handle) = cx.open_window(options, |window, cx| {
        cx.new(|cx| SettingsWindow::new(controller, window, cx))
    }) {
        *window = Some(handle);
        cx.activate(true);
    }
}

/// Starts the threads that turn tray, session, and engine notifications into
/// `AppEvent`s. Each ends when the receiving side is gone.
fn spawn_producers(events: &UnboundedSender<AppEvent>, engine: Receiver<EngineEvent>) {
    let sender = events.clone();
    spawn_thread("Hyperkey tray menu events", move || {
        let receiver = MenuEvent::receiver();
        while let Ok(event) = receiver.recv() {
            let event = match event.id.0.as_str() {
                tray::MENU_ID_TOGGLE => AppEvent::ToggleEnabled,
                tray::MENU_ID_OPEN_SETTINGS => AppEvent::OpenSettings,
                tray::MENU_ID_QUIT => AppEvent::Quit,
                _ => continue,
            };
            if sender.unbounded_send(event).is_err() {
                break;
            }
        }
    });

    let sender = events.clone();
    spawn_thread("Hyperkey tray icon events", move || {
        let receiver = TrayIconEvent::receiver();
        while let Ok(event) = receiver.recv() {
            if let TrayIconEvent::DoubleClick {
                button: MouseButton::Left,
                ..
            } = event
                && sender.unbounded_send(AppEvent::OpenSettings).is_err()
            {
                break;
            }
        }
    });

    let sender = events.clone();
    spawn_thread("Hyperkey engine events", move || {
        while let Ok(event) = engine.recv() {
            if sender.unbounded_send(AppEvent::Engine(event)).is_err() {
                break;
            }
        }
    });

    let sender = events.clone();
    session_window::spawn(move |event| {
        let _ = sender.unbounded_send(AppEvent::Session(event));
    });
}

fn spawn_thread(name: &str, body: impl FnOnce() + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name(name.to_string())
        .spawn(body);
}
