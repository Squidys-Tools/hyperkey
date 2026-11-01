//! A hidden top-level window on a dedicated thread that receives the system
//! notifications the settings daemon cares about:
//!
//! - `WM_POWERBROADCAST` resume events (sleep/resume),
//! - `WM_WTSSESSION_CHANGE` lock/unlock (registered through WTS),
//! - `WM_DISPLAYCHANGE` (display or session geometry changes),
//! - a `RegisterWindowMessage` named `Hyperkey.ShowSettings` that a second
//!   Hyperkey instance broadcasts so the first one raises its settings window.
//!
//! A real (but never-shown) top-level window is used instead of a message-only
//! window because only top-level windows receive these broadcast messages.
//! Each notification is forwarded to the iced daemon through a plain callback.
use std::iter::once;
use std::mem::size_of;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::RemoteDesktop::{
    NOTIFY_FOR_THIS_SESSION, WTSRegisterSessionNotification,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CREATESTRUCTW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GWLP_USERDATA, GetMessageW,
    GetWindowLongPtrW, HWND_BROADCAST, MSG, PBT_APMRESUMEAUTOMATIC, PBT_APMRESUMECRITICAL,
    PBT_APMRESUMESUSPEND, PostMessageW, RegisterClassExW, RegisterWindowMessageW,
    SetWindowLongPtrW, TranslateMessage, WINDOW_EX_STYLE, WM_DISPLAYCHANGE, WM_NCCREATE,
    WM_POWERBROADCAST, WM_WTSSESSION_CHANGE, WNDCLASSEXW, WS_OVERLAPPED, WTS_SESSION_LOCK,
    WTS_SESSION_UNLOCK,
};
use windows::core::PCWSTR;

const CLASS_NAME: &str = "Hyperkey.SessionWindow";
const SHOW_SETTINGS_MESSAGE: &str = "Hyperkey.ShowSettings";

/// What the daemon should do in response to a session notification.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum SessionEvent {
    /// Sleep/resume, session lock/unlock, or a display change: the engine must
    /// release generated modifiers and reset its state machine. The C# app
    /// additionally stopped the hook while Windows was locked; the port
    /// reconciles instead — a low-level hook cannot fire on the secure
    /// desktop anyway, and the watchdog heals any hook Windows removed —
    /// so no pause diagnostic exists here.
    ReconcileEngine,
    /// A second Hyperkey instance asked this one to raise its settings window.
    ShowSettings,
}

/// Starts the hidden-window thread. `notify` is invoked on that thread; it
/// must be cheap and non-blocking (the app forwards to an iced channel).
/// There is no shutdown path: the thread and window die with the process.
pub fn spawn(notify: impl Fn(SessionEvent) + Send + 'static) {
    let _ = std::thread::Builder::new()
        .name("Hyperkey session window".to_string())
        .spawn(move || run(Box::new(notify)));
}

/// Registers (or resolves) the cross-process show-settings message and
/// broadcasts it to every top-level window. The running Hyperkey instance's
/// session window listens for it; returns false when Windows cannot hand out
/// the message id or the broadcast fails.
pub fn broadcast_show_settings() -> bool {
    let name = wide(SHOW_SETTINGS_MESSAGE);
    // SAFETY: FFI call with a valid nul-terminated name; id 0 means failure.
    let message = unsafe { RegisterWindowMessageW(PCWSTR(name.as_ptr())) };
    if message == 0 {
        return false;
    }

    // SAFETY: FFI call broadcasting to all top-level windows.
    unsafe { PostMessageW(Some(HWND_BROADCAST), message, WPARAM(0), LPARAM(0)) }.is_ok()
}

/// Shows a plain system message box. Used only when a second instance cannot
/// reach the first, so the launch does not fail silently.
pub fn message_box(text: &str, caption: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MessageBoxW};

    let text = wide(text);
    let caption = wide(caption);
    // SAFETY: FFI call; both buffers are nul-terminated and outlive the call.
    unsafe {
        let _ = MessageBoxW(None, PCWSTR(text.as_ptr()), PCWSTR(caption.as_ptr()), MB_ICONINFORMATION);
    }
}

struct Context {
    notify: Box<dyn Fn(SessionEvent) + Send>,
    show_settings: u32,
}

fn run(notify: Box<dyn Fn(SessionEvent) + Send>) {
    // SAFETY: standard Win32 window creation and message pump. The class name
    // buffer outlives the class registration; `context` is intentionally
    // leaked for the window's lifetime (the process owns it).
    unsafe {
        let class_name = wide(CLASS_NAME);
        let instance = GetModuleHandleW(None)
            .map(|module| windows::Win32::Foundation::HINSTANCE(module.0))
            .unwrap_or_default();

        let class = WNDCLASSEXW {
            cbSize: size_of::<WNDCLASSEXW>() as u32,
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance,
            lpszClassName: PCWSTR(class_name.as_ptr()),
            ..Default::default()
        };
        if RegisterClassExW(&class) == 0 {
            return;
        }

        let show_settings = {
            let name = wide(SHOW_SETTINGS_MESSAGE);
            RegisterWindowMessageW(PCWSTR(name.as_ptr()))
        };

        let context = Box::into_raw(Box::new(Context {
            notify,
            show_settings,
        }));

        let title = wide("Hyperkey session notifications");
        let window = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            PCWSTR(class_name.as_ptr()),
            PCWSTR(title.as_ptr()),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            Some(context.cast()),
        );
        let Ok(window) = window else {
            drop(Box::from_raw(context));
            return;
        };

        // Best-effort: lock/unlock notifications only arrive if this succeeds.
        let _ = WTSRegisterSessionNotification(window, NOTIFY_FOR_THIS_SESSION);

        let mut message = MSG::default();
        loop {
            // BOOL is -1 on error, 0 on WM_QUIT, positive otherwise.
            if GetMessageW(&mut message, None, 0, 0).0 <= 0 {
                break;
            }

            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

unsafe extern "system" fn wnd_proc(
    window: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        if message == WM_NCCREATE {
            let create = &*(lparam.0 as *const CREATESTRUCTW);
            SetWindowLongPtrW(window, GWLP_USERDATA, create.lpCreateParams as isize);
            return DefWindowProcW(window, message, wparam, lparam);
        }

        let context = GetWindowLongPtrW(window, GWLP_USERDATA) as *const Context;
        if !context.is_null() {
            let context = &*context;
            let code = wparam.0 as u32;
            let reconcile = (message == WM_POWERBROADCAST
                && matches!(
                    code,
                    PBT_APMRESUMEAUTOMATIC | PBT_APMRESUMECRITICAL | PBT_APMRESUMESUSPEND
                ))
                || (message == WM_WTSSESSION_CHANGE
                    && matches!(code, WTS_SESSION_LOCK | WTS_SESSION_UNLOCK))
                || message == WM_DISPLAYCHANGE;
            if reconcile {
                (context.notify)(SessionEvent::ReconcileEngine);
            } else if context.show_settings != 0 && message == context.show_settings {
                (context.notify)(SessionEvent::ShowSettings);
            }
        }

        DefWindowProcW(window, message, wparam, lparam)
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(once(0)).collect()
}
