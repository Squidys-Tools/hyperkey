//! Maps iced's physical key codes to Win32 virtual-key codes for the trigger
//! rebind capture. iced reports keys as winit `Code`s (physical positions);
//! `TriggerKey` works in virtual-key codes, so capture needs this table.
//! Returning `None` means the key has no plain virtual-key mapping at all —
//! the caller shows the generic "can't be used" hint, identical to what the
//! C# window did when `KeyInterop.VirtualKeyFromKey` could not map a key.
use iced::keyboard::key::Code;

/// The Win32 virtual-key code for a physical key position, when one exists.
/// Includes keys outside the core trigger allowlist (arrows, the editing
/// cluster, numpad divide) so `TriggerKey::try_create` — not this table —
/// decides what is a legal trigger and the hint text stays accurate.
pub fn virtual_key_code(code: Code) -> Option<u16> {
    Some(match code {
        Code::Backspace => 0x08,
        Code::Tab => 0x09,
        Code::Enter | Code::NumpadEnter => 0x0D,
        Code::CapsLock => 0x14,
        Code::Escape => 0x1B,
        Code::Space => 0x20,
        Code::PageUp => 0x21,
        Code::PageDown => 0x22,
        Code::End => 0x23,
        Code::Home => 0x24,
        Code::ArrowLeft => 0x25,
        Code::ArrowUp => 0x26,
        Code::ArrowRight => 0x27,
        Code::ArrowDown => 0x28,
        Code::PrintScreen => 0x2C,
        Code::Insert => 0x2D,
        Code::Delete => 0x2E,
        Code::Digit0 => 0x30,
        Code::Digit1 => 0x31,
        Code::Digit2 => 0x32,
        Code::Digit3 => 0x33,
        Code::Digit4 => 0x34,
        Code::Digit5 => 0x35,
        Code::Digit6 => 0x36,
        Code::Digit7 => 0x37,
        Code::Digit8 => 0x38,
        Code::Digit9 => 0x39,
        Code::KeyA => 0x41,
        Code::KeyB => 0x42,
        Code::KeyC => 0x43,
        Code::KeyD => 0x44,
        Code::KeyE => 0x45,
        Code::KeyF => 0x46,
        Code::KeyG => 0x47,
        Code::KeyH => 0x48,
        Code::KeyI => 0x49,
        Code::KeyJ => 0x4A,
        Code::KeyK => 0x4B,
        Code::KeyL => 0x4C,
        Code::KeyM => 0x4D,
        Code::KeyN => 0x4E,
        Code::KeyO => 0x4F,
        Code::KeyP => 0x50,
        Code::KeyQ => 0x51,
        Code::KeyR => 0x52,
        Code::KeyS => 0x53,
        Code::KeyT => 0x54,
        Code::KeyU => 0x55,
        Code::KeyV => 0x56,
        Code::KeyW => 0x57,
        Code::KeyX => 0x58,
        Code::KeyY => 0x59,
        Code::KeyZ => 0x5A,
        Code::Numpad0 => 0x60,
        Code::Numpad1 => 0x61,
        Code::Numpad2 => 0x62,
        Code::Numpad3 => 0x63,
        Code::Numpad4 => 0x64,
        Code::Numpad5 => 0x65,
        Code::Numpad6 => 0x66,
        Code::Numpad7 => 0x67,
        Code::Numpad8 => 0x68,
        Code::Numpad9 => 0x69,
        Code::NumpadMultiply => 0x6A,
        Code::NumpadAdd => 0x6B,
        // VK_SEPARATOR: the numpad separator key exists on some layouts.
        Code::NumpadComma => 0x6C,
        Code::NumpadSubtract => 0x6D,
        Code::NumpadDecimal => 0x6E,
        Code::NumpadDivide => 0x6F,
        Code::F1 => 0x70,
        Code::F2 => 0x71,
        Code::F3 => 0x72,
        Code::F4 => 0x73,
        Code::F5 => 0x74,
        Code::F6 => 0x75,
        Code::F7 => 0x76,
        Code::F8 => 0x77,
        Code::F9 => 0x78,
        Code::F10 => 0x79,
        Code::F11 => 0x7A,
        Code::F12 => 0x7B,
        Code::F13 => 0x7C,
        Code::F14 => 0x7D,
        Code::F15 => 0x7E,
        Code::F16 => 0x7F,
        Code::F17 => 0x80,
        Code::F18 => 0x81,
        Code::F19 => 0x82,
        Code::F20 => 0x83,
        Code::F21 => 0x84,
        Code::F22 => 0x85,
        Code::F23 => 0x86,
        Code::F24 => 0x87,
        Code::NumLock => 0x90,
        Code::ScrollLock => 0x91,
        Code::Semicolon => 0xBA,
        Code::Equal => 0xBB,
        Code::Comma => 0xBC,
        Code::Minus => 0xBD,
        Code::Period => 0xBE,
        Code::Slash => 0xBF,
        Code::Backquote => 0xC0,
        Code::BracketLeft => 0xDB,
        Code::Backslash => 0xDC,
        Code::BracketRight => 0xDD,
        Code::Quote => 0xDE,
        Code::IntlBackslash => 0xE2,
        _ => return None,
    })
}

/// Modifier, OS, and IME-side keys that can never be the trigger and get the
/// more specific rejection hint, matching the C# IsModifierOrSystemKey list.
/// `Escape` is deliberately absent: the caller treats it as the cancel key.
pub fn is_modifier_or_system(code: Code) -> bool {
    matches!(
        code,
        Code::ShiftLeft
            | Code::ShiftRight
            | Code::ControlLeft
            | Code::ControlRight
            | Code::AltLeft
            | Code::AltRight
            | Code::SuperLeft
            | Code::SuperRight
            | Code::Meta
            | Code::Hyper
            | Code::ContextMenu
            | Code::Fn
            | Code::FnLock
            | Code::Convert
            | Code::NonConvert
            | Code::KanaMode
            | Code::Lang1
            | Code::Lang2
            | Code::Lang3
            | Code::Lang4
            | Code::Lang5
            | Code::Hiragana
            | Code::Katakana
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperkey_core::keys::TriggerKey;

    #[test]
    fn allowlist_keys_map_to_their_virtual_key_codes() {
        // Every Code this app can surface must round-trip through the core
        // allowlist; spot-check the boundaries of each contiguous run.
        let cases = [
            (Code::CapsLock, 0x14),
            (Code::ScrollLock, 0x91),
            (Code::NumLock, 0x90),
            (Code::Tab, 0x09),
            (Code::Space, 0x20),
            (Code::KeyA, 0x41),
            (Code::KeyZ, 0x5A),
            (Code::Digit0, 0x30),
            (Code::Digit9, 0x39),
            (Code::F1, 0x70),
            (Code::F24, 0x87),
            (Code::Numpad0, 0x60),
            (Code::Numpad9, 0x69),
            (Code::Semicolon, 0xBA),
            (Code::Quote, 0xDE),
        ];

        for (code, expected) in cases {
            assert_eq!(Some(expected), virtual_key_code(code), "{code:?}");
            assert!(
                TriggerKey::try_create(expected).is_some(),
                "{code:?} maps to 0x{expected:02X}, which is not in the trigger allowlist"
            );
        }
    }

    #[test]
    fn letter_key_codes_stay_inside_the_allowlist() {
        for (index, code) in [
            Code::KeyA, Code::KeyB, Code::KeyC, Code::KeyD, Code::KeyE, Code::KeyF,
            Code::KeyG, Code::KeyH, Code::KeyI, Code::KeyJ, Code::KeyK, Code::KeyL,
            Code::KeyM, Code::KeyN, Code::KeyO, Code::KeyP, Code::KeyQ, Code::KeyR,
            Code::KeyS, Code::KeyT, Code::KeyU, Code::KeyV, Code::KeyW, Code::KeyX,
            Code::KeyY, Code::KeyZ,
        ]
        .into_iter()
        .enumerate()
        {
            let virtual_key = virtual_key_code(code).unwrap();
            assert_eq!(0x41 + index as u16, virtual_key, "{code:?}");
            assert!(TriggerKey::try_create(virtual_key).is_some(), "{code:?}");
        }
    }

    #[test]
    fn keys_outside_the_allowlist_still_map_so_the_hint_is_accurate() {
        // Arrows and the editing cluster map to real virtual keys, then the
        // core allowlist rejects them — same path the C# window took.
        for code in [
            Code::ArrowUp,
            Code::ArrowDown,
            Code::Delete,
            Code::Insert,
            Code::NumpadDivide,
        ] {
            let virtual_key = virtual_key_code(code).unwrap();
            assert!(TriggerKey::try_create(virtual_key).is_none(), "{code:?}");
        }
    }

    #[test]
    fn modifier_and_system_keys_are_flagged() {
        for code in [
            Code::ShiftLeft,
            Code::ShiftRight,
            Code::ControlLeft,
            Code::ControlRight,
            Code::AltLeft,
            Code::AltRight,
            Code::SuperLeft,
            Code::SuperRight,
            Code::ContextMenu,
            Code::Fn,
        ] {
            assert!(is_modifier_or_system(code), "{code:?}");
        }
    }

    #[test]
    fn escape_is_not_a_modifier_or_system_key() {
        assert!(!is_modifier_or_system(Code::Escape));
        assert_eq!(Some(0x1B), virtual_key_code(Code::Escape));
    }
}
