use hyperkey_core::keys::TriggerKey;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyNameTextW, MAPVK_VK_TO_VSC, MapVirtualKeyW,
};

/// A user-facing name for a key, in the active keyboard layout's language
/// where Windows knows one (`GetKeyNameTextW`). `scan_code` and `extended`
/// are the hook's values; a zero scan code is looked up from the virtual
/// key. Falls back to the trigger allowlist's label, then to "Key 0x..".
pub fn key_label(virtual_key: u16, scan_code: u16, extended: bool) -> String {
    let scan = if scan_code != 0 {
        u32::from(scan_code)
    } else {
        // SAFETY: FFI query with no preconditions or retained state.
        unsafe { MapVirtualKeyW(u32::from(virtual_key), MAPVK_VK_TO_VSC) }
    };

    if scan != 0 {
        // lParam layout of WM_KEYDOWN: bits 16-23 scan code, bit 24 extended.
        let lparam = ((scan & 0xFF) << 16) | if extended { 1 << 24 } else { 0 };
        let mut buffer = [0u16; 64];
        // SAFETY: FFI call writing at most `buffer.len()` UTF-16 units.
        let length = unsafe { GetKeyNameTextW(lparam as i32, &mut buffer) };
        if length > 0 {
            let length = usize::min(length as usize, buffer.len());
            return String::from_utf16_lossy(&buffer[..length]);
        }
    }

    match TriggerKey::try_create(virtual_key) {
        Some(key) => key.display_label().into_owned(),
        None => format!("Key 0x{virtual_key:02X}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_keys_get_a_non_empty_label() {
        for vk in [0x41u16, 0x20, 0x14, 0x70] {
            assert!(!key_label(vk, 0, false).is_empty(), "0x{vk:02X}");
        }
    }

    #[test]
    fn unknown_keys_fall_back_to_hex() {
        // 0x88 is unassigned and has no scan code.
        assert_eq!("Key 0x88", key_label(0x88, 0, false));
    }
}
