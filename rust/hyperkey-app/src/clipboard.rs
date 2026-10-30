//! Copies the diagnostics summary to the clipboard as `CF_UNICODETEXT`.
//! Implemented directly on the Win32 clipboard API so the app needs no
//! clipboard crate: open, empty, hand a moveable global buffer to the
//! clipboard owner, close.
use std::iter::once;
use std::ptr;
use windows::Win32::Foundation::{GlobalFree, HANDLE};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

/// Places `text` on the system clipboard. Any failure is returned as a
/// user-facing message; the clipboard is always closed again afterwards.
pub fn set_text(text: &str) -> Result<(), String> {
    // SAFETY: every handle below is a valid owned handle for the duration of
    // the call; the global buffer stays alive until it is handed to the
    // clipboard (or freed on failure).
    unsafe {
        OpenClipboard(None)
            .map_err(|error| format!("The clipboard could not be opened: {error}"))?;

        let result = fill_clipboard(text);
        let _ = CloseClipboard();
        result
    }
}

unsafe fn fill_clipboard(text: &str) -> Result<(), String> {
    // SAFETY: see `set_text`.
    unsafe {
        EmptyClipboard().map_err(|error| format!("The clipboard could not be emptied: {error}"))?;

        let utf16: Vec<u16> = text.encode_utf16().chain(once(0)).collect();
        let handle = GlobalAlloc(GMEM_MOVEABLE, utf16.len() * size_of::<u16>())
            .map_err(|error| format!("Clipboard memory could not be allocated: {error}"))?;

        let destination = GlobalLock(handle);
        if destination.is_null() {
            let _ = GlobalFree(Some(handle));
            return Err("Clipboard memory could not be locked.".to_string());
        }

        ptr::copy_nonoverlapping(utf16.as_ptr(), destination.cast(), utf16.len());
        let _ = GlobalUnlock(handle);

        match SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(handle.0))) {
            Ok(_) => Ok(()),
            // On failure the buffer is still ours: free it so it cannot leak.
            Err(error) => {
                let _ = GlobalFree(Some(handle));
                Err(format!("The clipboard could not be set: {error}"))
            }
        }
    }
}
