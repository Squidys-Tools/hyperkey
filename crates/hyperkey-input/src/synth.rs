use hyperkey_core::keys::{KeyTransition, TriggerKey};
use hyperkey_core::machine::BufferedKey;
use hyperkey_core::settings::OutputModifier;
use std::collections::HashSet;
use std::sync::{Mutex, MutexGuard};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, KEYEVENTF_SCANCODE, KEYEVENTF_UNICODE, MAPVK_VK_TO_VSC, MapVirtualKeyW,
    SendInput, VIRTUAL_KEY,
};

/// `dwExtraInfo` marker on every event this process synthesizes; the hook
/// ignores tagged events so generated input never re-enters the state machine.
pub const SYNTHETIC_INPUT_TAG: u64 = 0x48595045524B4559;

/// `dwExtraInfo` marker on watchdog probe events only ("HYPERKRS"), distinct
/// from `SYNTHETIC_INPUT_TAG` so the hook can tell a liveness probe from a
/// generated modifier or trigger replay. It must not be "HYPERKHB": the C#
/// engine sends its F24 heartbeat with that tag, and sharing it would let
/// this hook swallow another app's probe.
pub const HEARTBEAT_INPUT_TAG: u64 = 0x48595045524B5253;

/// Virtual-key code used for watchdog probes: 0x88 is unassigned, is not in
/// the `TriggerKey` allowlist, and a stray key-up is inert, so a probe can
/// never act as a trigger or reach an app as meaningful input. (The C#
/// engine probes with F24, which collides with a real trigger choice.)
pub const HEARTBEAT_VIRTUAL_KEY: u16 = 0x88;

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
        // Prefer the layout's scan code so the replay matches the physical
        // key; a few allowlisted keys (such as Num Sep or Oem8 on many
        // layouts) have none, and those replay by virtual-key code instead.
        let inputs = match trigger_scan_code(trigger_key) {
            Some(scan_code) => [
                create_input(scan_code, false),
                create_input(scan_code, true),
            ],
            None => {
                let key = trigger_key.virtual_key_code();
                [
                    create_vk_input(key, false, false),
                    create_vk_input(key, false, true),
                ]
            }
        };
        send_tap(&inputs, "the trigger-key tap replay")
    }

    /// Sends a tagged Escape press and release (scan code 0x01).
    pub fn try_send_escape_tap(&self) -> Result<(), String> {
        let inputs = [
            create_input(ESCAPE_SCAN_CODE, false),
            create_input(ESCAPE_SCAN_CODE, true),
        ];
        send_tap(&inputs, "the Escape tap")
    }

    /// Replays suppressed physical key events, in order, with the virtual
    /// key, scan code, and extended flag the hook reported. Downs are replayed without their
    /// ups on purpose: the physical key-up arrives later and passes through.
    /// A partial send retries the skipped events once and reports either way.
    pub fn try_replay_keys(&self, keys: &[BufferedKey]) -> Result<(), String> {
        let inputs: Vec<INPUT> = keys.iter().map(create_replay_input).collect();
        let sent = send(&inputs);
        if inputs.is_empty() || sent == inputs.len() as u32 {
            return Ok(());
        }

        let skipped = &inputs[usize::min(sent as usize, inputs.len())..];
        let recovered = send(skipped) == skipped.len() as u32;
        Err(format!(
            "Windows rejected the replay of buffered keys{}. {}",
            if recovered {
                "; the skipped keys were injected afterward"
            } else {
                ""
            },
            windows::core::Error::from_thread()
        ))
    }

    /// Injects a tagged virtual-key 0x88 key-up that only exists to prove the
    /// low-level hook is still alive. The hook suppresses it on sight, and a
    /// stray key-up of an unassigned key is inert if the probe ever leaks
    /// past a dead hook.
    pub fn try_send_heartbeat_probe(&self) -> bool {
        send(&[create_probe_input()]) == 1
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
    let flags = if is_key_up {
        KEYEVENTF_SCANCODE | KEYEVENTF_KEYUP
    } else {
        KEYEVENTF_SCANCODE
    };
    keyboard_input(0, scan_code, flags)
}

const ESCAPE_SCAN_CODE: u16 = 0x01;

/// Sends a down/up pair. A partial send can leave the key-down injected with
/// no release; the events SendInput skipped are pushed again so the key does
/// not stay held. The anomaly is still reported either way, with the
/// recovery's outcome noted.
fn send_tap(inputs: &[INPUT; 2], what: &str) -> Result<(), String> {
    let sent = send(inputs);
    if sent == inputs.len() as u32 {
        return Ok(());
    }

    let skipped = &inputs[usize::min(sent as usize, inputs.len())..];
    let recovered = sent > 0 && send(skipped) == skipped.len() as u32;
    Err(format!(
        "Windows rejected {what}{}. {}",
        if recovered {
            "; the skipped release was injected afterward"
        } else {
            ""
        },
        windows::core::Error::from_thread()
    ))
}

/// `VK_PACKET`: the hook's virtual key for `KEYEVENTF_UNICODE` input, whose
/// scan code field carries the UTF-16 code unit instead of a scan code.
const VK_PACKET: u16 = 0xE7;

/// A buffered key replays with the virtual key and scan code the hook
/// reported. The virtual key drives the injected key, so layout-derived keys
/// round-trip and a remapper's injected virtual key is kept rather than
/// re-derived from the scan code; the scan code still reaches the target in
/// the message. Unicode input (`VK_PACKET`) replays as the character itself.
fn create_replay_input(key: &BufferedKey) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if key.transition == KeyTransition::Up {
        flags |= KEYEVENTF_KEYUP;
    }
    if key.key.0 == VK_PACKET {
        return keyboard_input(0, key.scan_code, flags | KEYEVENTF_UNICODE);
    }
    if key.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    keyboard_input(key.key.0, key.scan_code, flags)
}

fn create_vk_input(virtual_key: u16, extended: bool, is_key_up: bool) -> INPUT {
    let mut flags = KEYBD_EVENT_FLAGS(0);
    if extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if is_key_up {
        flags |= KEYEVENTF_KEYUP;
    }
    keyboard_input(virtual_key, 0, flags)
}

fn keyboard_input(virtual_key: u16, scan_code: u16, flags: KEYBD_EVENT_FLAGS) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(virtual_key),
                wScan: scan_code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: SYNTHETIC_INPUT_TAG as usize,
            },
        },
    }
}

/// The watchdog probe travels by virtual-key code rather than scan code:
/// 0x88 has no physical scan code and only the key-up is ever sent.
fn create_probe_input() -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: VIRTUAL_KEY(HEARTBEAT_VIRTUAL_KEY),
                wScan: 0,
                dwFlags: KEYEVENTF_KEYUP,
                time: 0,
                dwExtraInfo: HEARTBEAT_INPUT_TAG as usize,
            },
        },
    }
}

/// True only for a probe this process sent. The tag alone is not enough: a
/// tagged event for any other key passes through untouched.
pub fn is_heartbeat_probe(virtual_key: u32, extra_info: usize) -> bool {
    extra_info as u64 == HEARTBEAT_INPUT_TAG && virtual_key == u32::from(HEARTBEAT_VIRTUAL_KEY)
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
    let mapped =
        unsafe { MapVirtualKeyW(u32::from(trigger_key.virtual_key_code()), MAPVK_VK_TO_VSC) };
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

fn release_tracked(pressed: &mut HashSet<OutputModifier>, modifiers: &[OutputModifier]) {
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

    #[test]
    fn replay_inputs_keep_vk_scan_code_extended_flag_and_tag() {
        use hyperkey_core::keys::VirtualKey;
        let key = |scan_code, extended, transition| BufferedKey {
            key: VirtualKey(0x46),
            scan_code,
            extended,
            transition,
        };

        let input = create_replay_input(&key(0x21, false, KeyTransition::Down));
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let plain = unsafe { input.Anonymous.ki };
        assert_eq!(plain.wVk, VIRTUAL_KEY(0x46));
        assert_eq!(plain.wScan, 0x21);
        assert_eq!(plain.dwFlags, KEYBD_EVENT_FLAGS(0));
        assert_eq!(plain.dwExtraInfo, SYNTHETIC_INPUT_TAG as usize);

        let input = create_replay_input(&key(0x21, true, KeyTransition::Up));
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let extended_up = unsafe { input.Anonymous.ki };
        assert_eq!(extended_up.wVk, VIRTUAL_KEY(0x46));
        assert_eq!(extended_up.dwFlags, KEYEVENTF_EXTENDEDKEY | KEYEVENTF_KEYUP);

        // No scan code (injected by vk only): still the same virtual key.
        let input = create_replay_input(&key(0, false, KeyTransition::Up));
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let by_vk = unsafe { input.Anonymous.ki };
        assert_eq!(by_vk.wVk, VIRTUAL_KEY(0x46));
        assert_eq!(by_vk.wScan, 0);
        assert_eq!(by_vk.dwFlags, KEYEVENTF_KEYUP);
        assert_eq!(by_vk.dwExtraInfo, SYNTHETIC_INPUT_TAG as usize);
    }

    #[test]
    fn remapped_injected_keys_keep_their_virtual_key() {
        // A remapper injected VK_F13 with the original A key's scan code:
        // the replay must not undo the remap by re-deriving the vk.
        use hyperkey_core::keys::VirtualKey;
        let input = create_replay_input(&BufferedKey {
            key: VirtualKey(0x7C),
            scan_code: 0x1E,
            extended: false,
            transition: KeyTransition::Down,
        });
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let keyboard = unsafe { input.Anonymous.ki };
        assert_eq!(keyboard.wVk, VIRTUAL_KEY(0x7C));
        assert!(!keyboard.dwFlags.contains(KEYEVENTF_SCANCODE));
    }

    #[test]
    fn unicode_packets_replay_as_the_character() {
        use hyperkey_core::keys::VirtualKey;
        let packet = |transition| BufferedKey {
            key: VirtualKey(VK_PACKET),
            scan_code: u16::from(b'h'),
            extended: false,
            transition,
        };

        let input = create_replay_input(&packet(KeyTransition::Down));
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let down = unsafe { input.Anonymous.ki };
        assert_eq!(down.wVk, VIRTUAL_KEY(0));
        assert_eq!(down.wScan, 0x68);
        assert_eq!(down.dwFlags, KEYEVENTF_UNICODE);
        assert_eq!(down.dwExtraInfo, SYNTHETIC_INPUT_TAG as usize);

        let input = create_replay_input(&packet(KeyTransition::Up));
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let released = unsafe { input.Anonymous.ki };
        assert_eq!(released.dwFlags, KEYEVENTF_UNICODE | KEYEVENTF_KEYUP);
    }

    #[test]
    fn escape_tap_uses_scan_code_one() {
        let down = create_input(ESCAPE_SCAN_CODE, false);
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let keyboard = unsafe { down.Anonymous.ki };
        assert_eq!(keyboard.wScan, 0x01);
        assert_eq!(keyboard.dwFlags, KEYEVENTF_SCANCODE);
    }

    #[test]
    fn heartbeat_probe_is_a_tagged_unassigned_vk_keyup() {
        let input = create_probe_input();
        assert_eq!(input.r#type, INPUT_KEYBOARD);
        // SAFETY: ki is the active union member for INPUT_KEYBOARD.
        let keyboard = unsafe { input.Anonymous.ki };
        assert_eq!(keyboard.wVk, VIRTUAL_KEY(0x88));
        assert_eq!(keyboard.dwFlags, KEYEVENTF_KEYUP);
        assert_eq!(keyboard.dwExtraInfo, HEARTBEAT_INPUT_TAG as usize);
        assert_ne!(HEARTBEAT_INPUT_TAG, SYNTHETIC_INPUT_TAG);
        // 0x88 is unassigned and must stay out of the trigger allowlist so a
        // probe can never act as a trigger.
        assert!(!hyperkey_core::keys::TriggerKey::is_supported_key(0x88));
    }

    #[test]
    fn heartbeat_recognition_requires_our_tag_and_key() {
        assert!(is_heartbeat_probe(0x88, HEARTBEAT_INPUT_TAG as usize));
        // The C# engine's F24 heartbeat (VK 0x87) carries the "HYPERKHB" tag.
        // It must pass through this hook, not be swallowed.
        assert!(!is_heartbeat_probe(0x87, 0x48595045524B4842));
        // Our tag on the wrong key is not a probe either.
        assert!(!is_heartbeat_probe(0x87, HEARTBEAT_INPUT_TAG as usize));
        // Our key without our tag, or with the synthetic tag, is not a probe.
        assert!(!is_heartbeat_probe(0x88, 0));
        assert!(!is_heartbeat_probe(0x88, SYNTHETIC_INPUT_TAG as usize));
    }
}
