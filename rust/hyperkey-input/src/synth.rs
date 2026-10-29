use hyperkey_core::keys::TriggerKey;
use hyperkey_core::settings::OutputModifier;
use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE,
    MAPVK_VK_TO_VSC, MapVirtualKeyW, SendInput, VIRTUAL_KEY,
};

/// `dwExtraInfo` marker on every event this process synthesizes; the hook
/// ignores tagged events so generated input never re-enters the state machine.
pub const SYNTHETIC_INPUT_TAG: u64 = 0x48595045524B4559;

/// Sends the generated modifier layer through SendInput. Emits fixed left-hand
/// scan codes (Ctrl 0x1D, Alt 0x38, Shift 0x2A) so the output does not depend on
/// the active keyboard layout, and tracks what it pressed so cleanup can
/// release exactly those keys.
pub struct ModifierSynthesizer {
    pressed_modifiers: Mutex<HashSet<OutputModifier>>,
}

impl ModifierSynthesizer {
    pub fn new() -> Self {
        Self {
            pressed_modifiers: Mutex::new(HashSet::new()),
        }
    }

    pub fn try_press(&self, modifiers: &[OutputModifier]) -> Result<(), String> {
        let mut pressed = self.lock();

        let to_press: Vec<OutputModifier> = modifiers
            .iter()
            .copied()
            .filter(|modifier| !pressed.contains(modifier))
            .collect();
        let inputs = create_inputs(&to_press, false);
        let sent = send(&inputs);
        let sent_count = usize::min(sent as usize, to_press.len());
        for modifier in &to_press[..sent_count] {
            pressed.insert(*modifier);
        }

        if inputs.is_empty() || sent == inputs.len() as u32 {
            return Ok(());
        }

        let error = windows::core::Error::from_thread();
        release_tracked(&mut pressed, modifiers);
        Err(format!(
            "Windows rejected the generated modifier press. {error}"
        ))
    }

    pub fn try_release(&self, modifiers: &[OutputModifier]) -> Result<(), String> {
        let mut pressed = self.lock();

        let logical: Vec<OutputModifier> = modifiers
            .iter()
            .copied()
            .filter(|modifier| pressed.contains(modifier))
            .collect();
        let inputs = create_inputs(&logical, true);
        let sent = send(&inputs);
        let sent_count = usize::min(sent as usize, logical.len());
        for modifier in &logical[..sent_count] {
            pressed.remove(modifier);
        }

        if inputs.is_empty() || sent == inputs.len() as u32 {
            return Ok(());
        }

        let error = windows::core::Error::from_thread();
        release_tracked(&mut pressed, modifiers);
        Err(format!(
            "Windows rejected the generated modifier release. {error}"
        ))
    }

    pub fn release_all(&self, modifiers: &[OutputModifier]) {
        let mut pressed = self.lock();
        release_tracked(&mut pressed, modifiers);
    }

    pub fn try_replay_trigger(&self, trigger_key: TriggerKey) -> Result<(), String> {
        let Some(scan_code) = trigger_scan_code(trigger_key) else {
            return Err(
                "The trigger key could not be mapped to a replayable scan code.".to_string(),
            );
        };

        let inputs = [create_input(scan_code, false), create_input(scan_code, true)];
        if send(&inputs) == inputs.len() as u32 {
            return Ok(());
        }

        Err(format!(
            "Windows rejected the trigger-key tap replay. {}",
            windows::core::Error::from_thread()
        ))
    }

    fn lock(&self) -> MutexGuard<'_, HashSet<OutputModifier>> {
        // A poisoned lock still holds a usable modifier set; dropping it would
        // strand pressed modifiers, which is worse than recovering.
        self.pressed_modifiers
            .lock()
            .unwrap_or_else(|error| error.into_inner())
    }
}

impl Default for ModifierSynthesizer {
    fn default() -> Self {
        Self::new()
    }
}

fn create_inputs(modifiers: &[OutputModifier], is_key_up: bool) -> Vec<INPUT> {
    // Releases go out in the same logical order the modifiers were pressed; the
    // C# version double-reverses here, so this keeps its observable behavior.
    modifiers
        .iter()
        .map(|modifier| create_input(scan_code(*modifier), is_key_up))
        .collect()
}

fn create_input(scan_code: u16, is_key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(0),
                wScan: scan_code,
                dwFlags: if is_key_up {
                    KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP
                } else {
                    KEYEVENTF_SCANCODE
                },
                time: 0,
                dwExtraInfo: SYNTHETIC_INPUT_TAG as usize,
            },
        },
    }
}

fn scan_code(modifier: OutputModifier) -> u16 {
    match modifier {
        OutputModifier::Control => 0x1D,
        OutputModifier::Alt => 0x38,
        OutputModifier::Shift => 0x2A,
    }
}

fn trigger_scan_code(trigger_key: TriggerKey) -> Option<u16> {
    // Resolve the scan code from the active keyboard layout so replayed taps
    // match the physical key on any layout. Triggers are restricted to keys
    // with plain (non-extended) scan codes, so the high byte must be clear.
    // SAFETY: FFI query with no preconditions or retained state.
    let mapped = unsafe {
        MapVirtualKeyW(u32::from(trigger_key.virtual_key_code()), MAPVK_VK_TO_VSC)
    };
    if mapped == 0 || (mapped & 0xFF00) != 0 {
        None
    } else {
        Some(mapped as u16)
    }
}

fn send(inputs: &[INPUT]) -> u32 {
    // SAFETY: FFI call; the slice is a valid INPUT array for the duration.
    unsafe { SendInput(inputs, size_of::<INPUT>() as i32) }
}

fn release_tracked(
    pressed: &mut HashSet<OutputModifier>,
    modifiers: &[OutputModifier],
) {
    let logical: Vec<OutputModifier> = modifiers
        .iter()
        .copied()
        .filter(|modifier| pressed.contains(modifier))
        .collect();
    if !logical.is_empty() {
        let inputs = create_inputs(&logical, true);
        send(&inputs);
    }

    for modifier in &logical {
        pressed.remove(modifier);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::UI::Input::KeyboardAndMouse::INPUT_KEYBOARD;

    #[test]
    fn keyboard_inputs_are_tagged_scan_codes() {
        let input = create_input(0x1D, false);
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let keyboard = unsafe { input.Anonymous.ki };
        assert_eq!(keyboard.wVk, VIRTUAL_KEY(0));
        assert_eq!(keyboard.wScan, 0x1D);
        assert_eq!(keyboard.dwFlags, KEYEVENTF_SCANCODE);
        assert_eq!(keyboard.dwExtraInfo, SYNTHETIC_INPUT_TAG as usize);

        let released = create_input(0x1D, true);
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let keyboard = unsafe { released.Anonymous.ki };
        assert_eq!(keyboard.dwFlags, KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP);
    }
}
