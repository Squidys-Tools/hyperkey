//! Single-instance guard, mirroring the C# named-mutex service. The handle is
//! held for the process lifetime so a second launch exits immediately — after
//! asking the running instance to raise its settings window.
use crate::session_window;
use std::iter::once;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows::core::PCWSTR;

pub struct SingleInstance(HANDLE);

/// Returns `Ok(None)` when another Hyperkey instance already owns the mutex
/// and `Err` when Windows could not create it at all — a create failure is
/// not a running instance and must not look like one.
pub fn acquire() -> Result<Option<SingleInstance>, String> {
    let name: Vec<u16> = "Local\\Hyperkey.SingleInstance"
        .encode_utf16()
        .chain(once(0))
        .collect();
    // SAFETY: FFI call; `name` is a valid nul-terminated UTF-16 buffer.
    let handle = unsafe { CreateMutexW(None, true, PCWSTR(name.as_ptr())) };
    match handle {
        Ok(handle) if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS => {
            // SAFETY: FFI cleanup for a handle we own but will not keep.
            let _ = unsafe { CloseHandle(handle) };
            Ok(None)
        }
        Ok(handle) => Ok(Some(SingleInstance(handle))),
        Err(error) => Err(format!("The single-instance guard could not be created: {error}")),
    }
}

/// Best-effort second-instance hand-off: broadcast the registered
/// show-settings message so the running daemon raises its window. A second
/// launch can beat the first instance's hidden session window, so the post is
/// retried a few times over about a second. Reports whether a broadcast was
/// posted.
pub fn notify_existing() -> bool {
    for attempt in 0..5 {
        if session_window::broadcast_show_settings() {
            return true;
        }
        if attempt + 1 < 5 {
            std::thread::sleep(std::time::Duration::from_millis(200));
        }
    }
    false
}

impl Drop for SingleInstance {
    fn drop(&mut self) {
        // SAFETY: FFI cleanup; releasing first so the named mutex is freed
        // cleanly for the next process.
        unsafe {
            let _ = ReleaseMutex(self.0);
            let _ = CloseHandle(self.0);
        }
    }
}
