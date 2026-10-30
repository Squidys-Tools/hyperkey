//! Single-instance guard, mirroring the C# named-mutex service. The handle is
//! held for the process lifetime so a second launch exits immediately — after
//! asking the running instance to raise its settings window.
use crate::session_window;
use std::iter::once;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::{CreateMutexW, ReleaseMutex};
use windows::core::PCWSTR;

pub struct SingleInstance(HANDLE);

/// Returns `None` when another Hyperkey instance already owns the mutex.
pub fn acquire() -> Option<SingleInstance> {
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
            None
        }
        Ok(handle) => Some(SingleInstance(handle)),
        Err(_) => None,
    }
}

/// Best-effort second-instance hand-off: broadcast the registered
/// show-settings message so the running daemon raises its window. Reports
/// whether the broadcast was posted.
pub fn notify_existing() -> bool {
    session_window::broadcast_show_settings()
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
