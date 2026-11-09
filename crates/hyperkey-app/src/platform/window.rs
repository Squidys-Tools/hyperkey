//! Hide and show a GPUI window through its native handle. GPUI quits the app
//! when its last window is destroyed, so the settings window is only ever
//! hidden, never closed.
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOW, ShowWindow};

/// Hides the window, keeping it alive for later.
pub fn hide(window: &impl HasWindowHandle) {
    show_command(window, SW_HIDE);
}

/// Makes a hidden window visible again.
pub fn show(window: &impl HasWindowHandle) {
    show_command(window, SW_SHOW);
}

fn show_command(
    window: &impl HasWindowHandle,
    command: windows::Win32::UI::WindowsAndMessaging::SHOW_WINDOW_CMD,
) {
    let Ok(handle) = window.window_handle() else {
        return;
    };
    if let RawWindowHandle::Win32(win32) = handle.as_raw() {
        // SAFETY: FFI call on a window handle owned by this process.
        let _ = unsafe { ShowWindow(HWND(win32.hwnd.get() as *mut _), command) };
    }
}
