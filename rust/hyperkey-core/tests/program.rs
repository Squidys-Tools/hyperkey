//! Port of tests/Hyperkey.Core.Tests/Program.cs — one Rust test per C# case.

use hyperkey_core::json;
use hyperkey_core::keys::{KeyTransition, KeyboardEvent, TriggerKey, VirtualKey};
use hyperkey_core::machine::{
    self, InputDecision, TriggerMachineState, TriggerPhase, TriggerTransition,
};
use hyperkey_core::settings::{HyperkeySettings, OutputModifier, TapBehavior};

fn modifiers() -> &'static [OutputModifier] {
    static MODIFIERS: &[OutputModifier] = &[
        OutputModifier::Control,
        OutputModifier::Alt,
        OutputModifier::Shift,
    ];
    MODIFIERS
}

fn process(state: TriggerMachineState, input: KeyboardEvent) -> TriggerTransition {
    machine::process(state, &input, modifiers())
}

fn process_with(
    state: TriggerMachineState,
    input: KeyboardEvent,
    trigger_key: TriggerKey,
) -> TriggerTransition {
    machine::process_with_trigger(state, &input, trigger_key, modifiers())
}

fn caps_lock(transition: KeyTransition) -> KeyboardEvent {
    KeyboardEvent::new(VirtualKey::CAPS_LOCK, transition)
}

fn key(code: u16, transition: KeyTransition) -> KeyboardEvent {
    KeyboardEvent::new(VirtualKey(code), transition)
}

#[test]
fn caps_lock_down_is_suppressed_and_enters_trigger_held() {
    let transition = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));

    assert_eq!(TriggerPhase::TriggerHeld, transition.state.phase);
    assert!(matches!(transition.decision, InputDecision::Suppress));
}

#[test]
fn caps_lock_tap_replays_normal_caps_lock_and_returns_to_idle() {
    let held = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));
    let released = process(held.state, caps_lock(KeyTransition::Up));

    assert!(matches!(held.decision, InputDecision::Suppress));
    assert!(matches!(released.decision, InputDecision::ReplayTrigger));
    assert_eq!(TriggerPhase::Idle, released.state.phase);
}

#[test]
fn caps_lock_tap_behavior_round_trips_and_reads_legacy_settings() {
    let serialized = json::serialize(&HyperkeySettings::defaults());
    assert!(
        serialized.contains("\"tapBehavior\": \"CapsLock\""),
        "Expected the current tap behavior to serialize as CapsLock."
    );

    let round_trip = json::parse(&serialized);
    assert!(!round_trip.used_defaults);
    assert_eq!(TapBehavior::CapsLock, round_trip.settings.tap_behavior());
    assert!(!round_trip.settings.launch_to_tray());

    let tray_settings = HyperkeySettings::defaults().with_launch_to_tray(true);
    let tray_round_trip = json::parse(&json::serialize(&tray_settings));
    assert!(!tray_round_trip.used_defaults);
    assert!(tray_round_trip.settings.launch_to_tray());

    let legacy = serialized.replace(
        "\"tapBehavior\": \"CapsLock\"",
        "\"tapBehavior\": \"Undecided\"",
    );
    let legacy_result = json::parse(&legacy);
    assert!(!legacy_result.used_defaults);
    assert_eq!(TapBehavior::CapsLock, legacy_result.settings.tap_behavior());

    let legacy_without_launch_to_tray = r#"
        {
          "schemaVersion": 1,
          "enabled": true,
          "trigger": "CapsLock",
          "outputModifiers": ["Control", "Alt", "Shift"],
          "launchAtStartup": true,
          "tapBehavior": "Undecided"
        }
        "#;
    let missing_launch_to_tray_result = json::parse(legacy_without_launch_to_tray);
    assert!(!missing_launch_to_tray_result.used_defaults);
    assert!(!missing_launch_to_tray_result.settings.launch_to_tray());
}

#[test]
fn trigger_and_output_modifiers_round_trip() {
    let configured = HyperkeySettings::defaults()
        .with_trigger(TriggerKey::SCROLL_LOCK)
        .expect("ScrollLock is a supported trigger")
        .with_output_modifiers(vec![OutputModifier::Control, OutputModifier::Shift])
        .expect("two distinct modifiers are valid");
    let result = json::parse(&json::serialize(&configured));

    assert!(!result.used_defaults);
    assert_eq!(TriggerKey::SCROLL_LOCK, result.settings.trigger());
    assert_eq!(
        vec![OutputModifier::Control, OutputModifier::Shift],
        result.settings.output_modifiers()
    );
}

#[test]
fn first_key_down_presses_modifiers_and_forwards_the_key() {
    let held = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));
    let activated = process(held.state, key(0x41, KeyTransition::Down));

    let InputDecision::PressAndForward(press_modifiers) = &activated.decision else {
        panic!("Expected PressAndForward, got {:?}.", activated.decision);
    };
    assert_eq!(TriggerPhase::HyperActive, activated.state.phase);
    assert_eq!(modifiers(), press_modifiers.as_slice());
}

#[test]
fn configured_scroll_lock_acts_as_the_trigger() {
    let caps_lock_result = process_with(
        TriggerMachineState::IDLE,
        caps_lock(KeyTransition::Down),
        TriggerKey::SCROLL_LOCK,
    );
    let scroll_lock_result = process_with(
        TriggerMachineState::IDLE,
        KeyboardEvent::new(VirtualKey::SCROLL_LOCK, KeyTransition::Down),
        TriggerKey::SCROLL_LOCK,
    );

    assert_eq!(TriggerPhase::Idle, caps_lock_result.state.phase);
    assert!(matches!(caps_lock_result.decision, InputDecision::PassThrough));
    assert_eq!(TriggerPhase::TriggerHeld, scroll_lock_result.state.phase);
    assert!(matches!(scroll_lock_result.decision, InputDecision::Suppress));
}

#[test]
fn hyper_active_forwards_other_keys_until_caps_lock_release() {
    let held = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));
    let active = process(held.state, key(0x41, KeyTransition::Down));
    let forwarded = process(active.state, key(0x41, KeyTransition::Up));
    let released = process(forwarded.state, caps_lock(KeyTransition::Up));

    assert!(matches!(forwarded.decision, InputDecision::Forward));
    assert_eq!(TriggerPhase::HyperActive, forwarded.state.phase);
    assert!(matches!(released.decision, InputDecision::ReleaseAndSuppress(_)));
    assert_eq!(TriggerPhase::Idle, released.state.phase);
}

#[test]
fn unrelated_keys_pass_through_while_idle() {
    let transition = process(TriggerMachineState::IDLE, key(0x41, KeyTransition::Down));

    assert_eq!(TriggerPhase::Idle, transition.state.phase);
    assert!(matches!(transition.decision, InputDecision::PassThrough));
}

#[test]
fn trigger_repeat_remains_suppressed() {
    let held = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));
    let repeated = process(held.state, caps_lock(KeyTransition::Down));

    assert_eq!(TriggerPhase::TriggerHeld, repeated.state.phase);
    assert!(matches!(repeated.decision, InputDecision::Suppress));
}

#[test]
fn key_up_before_activation_passes_through() {
    let held = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));
    let unrelated_key_up = process(held.state, key(0x41, KeyTransition::Up));

    assert_eq!(TriggerPhase::TriggerHeld, unrelated_key_up.state.phase);
    assert!(matches!(
        unrelated_key_up.decision,
        InputDecision::PassThrough
    ));
}

#[test]
fn synthetic_events_never_change_the_state() {
    let synthetic_caps_lock =
        KeyboardEvent::synthetic(VirtualKey::CAPS_LOCK, KeyTransition::Down);
    let transition = process(TriggerMachineState::IDLE, synthetic_caps_lock);

    assert_eq!(TriggerPhase::Idle, transition.state.phase);
    assert!(matches!(transition.decision, InputDecision::PassThrough));
}

#[test]
fn synthetic_events_pass_through_while_hyper_active() {
    let held = process(TriggerMachineState::IDLE, caps_lock(KeyTransition::Down));
    let active = process(held.state, key(0x41, KeyTransition::Down));
    let synthetic = process(
        active.state,
        KeyboardEvent::synthetic(VirtualKey(0x42), KeyTransition::Down),
    );

    assert_eq!(TriggerPhase::HyperActive, synthetic.state.phase);
    assert!(matches!(synthetic.decision, InputDecision::PassThrough));
}

#[test]
fn repeated_cycles_do_not_accumulate_state() {
    let mut state = TriggerMachineState::IDLE;
    for _ in 0..10 {
        let held = process(state, caps_lock(KeyTransition::Down));
        let active = process(held.state, key(0x41, KeyTransition::Down));
        let released = process(active.state, caps_lock(KeyTransition::Up));
        state = released.state;

        assert_eq!(TriggerPhase::Idle, state.phase);
        assert!(matches!(released.decision, InputDecision::ReleaseAndSuppress(_)));
    }
}

#[test]
fn arbitrary_letter_trigger_runs_full_cycle() {
    let trigger = TriggerKey::try_create(0x4A).expect("Expected J to be a supported trigger.");
    assert_eq!("J", trigger.wire_name());

    let held = process_with(TriggerMachineState::IDLE, key(0x4A, KeyTransition::Down), trigger);
    assert_eq!(TriggerPhase::TriggerHeld, held.state.phase);
    assert!(matches!(held.decision, InputDecision::Suppress));

    let tapped = process_with(held.state, key(0x4A, KeyTransition::Up), trigger);
    assert_eq!(TriggerPhase::Idle, tapped.state.phase);
    assert!(matches!(tapped.decision, InputDecision::ReplayTrigger));

    let reheld = process_with(TriggerMachineState::IDLE, key(0x4A, KeyTransition::Down), trigger);
    let active = process_with(reheld.state, key(0x4B, KeyTransition::Down), trigger);
    assert_eq!(TriggerPhase::HyperActive, active.state.phase);
    let InputDecision::PressAndForward(press_modifiers) = &active.decision else {
        panic!("Expected PressAndForward, got {:?}.", active.decision);
    };
    assert_eq!(modifiers(), press_modifiers.as_slice());

    let released = process_with(active.state, key(0x4A, KeyTransition::Up), trigger);
    assert_eq!(TriggerPhase::Idle, released.state.phase);
    assert!(matches!(released.decision, InputDecision::ReleaseAndSuppress(_)));

    // Other letters are ordinary keys while J is the trigger.
    let unrelated = process_with(TriggerMachineState::IDLE, key(0x41, KeyTransition::Down), trigger);
    assert_eq!(TriggerPhase::Idle, unrelated.state.phase);
    assert!(matches!(unrelated.decision, InputDecision::PassThrough));
}

#[test]
fn function_key_trigger_tap_replays() {
    let trigger = TriggerKey::try_create(0x78).expect("Expected F9 to be a supported trigger.");
    assert_eq!("F9", trigger.wire_name());
    assert_eq!("F9", trigger.display_label());

    let held = process_with(TriggerMachineState::IDLE, key(0x78, KeyTransition::Down), trigger);
    assert!(matches!(held.decision, InputDecision::Suppress));

    let released = process_with(held.state, key(0x78, KeyTransition::Up), trigger);
    assert_eq!(TriggerPhase::Idle, released.state.phase);
    assert!(matches!(released.decision, InputDecision::ReplayTrigger));
}

#[test]
fn trigger_key_allowlist_accepts_normal_keys() {
    let accepted: &[(u16, &str, &str)] = &[
        (0x08, "Back", "Backspace"),
        (0x09, "Tab", "Tab"),
        (0x0D, "Enter", "Enter"),
        (0x14, "CapsLock", "Caps Lock"),
        (0x20, "Space", "Space"),
        (0x30, "D0", "0"),
        (0x33, "D3", "3"),
        (0x39, "D9", "9"),
        (0x41, "A", "A"),
        (0x4A, "J", "J"),
        (0x5A, "Z", "Z"),
        (0x60, "NumPad0", "NumPad0"),
        (0x65, "NumPad5", "NumPad5"),
        (0x6A, "NumPadMultiply", "Num *"),
        (0x6E, "NumPadDecimal", "Num ."),
        (0x70, "F1", "F1"),
        (0x78, "F9", "F9"),
        (0x87, "F24", "F24"),
        (0x90, "NumLock", "Num Lock"),
        (0x91, "ScrollLock", "Scroll Lock"),
        (0xBA, "OemSemicolon", ";"),
        (0xBC, "OemComma", ","),
        (0xBF, "OemQuestion", "/"),
        (0xC0, "OemTilde", "`"),
        (0xDB, "OemOpenBrackets", "["),
        (0xDC, "OemBackslash", "\\"),
        (0xDE, "OemQuotes", "'"),
    ];

    for &(code, wire, label) in accepted {
        let trigger = TriggerKey::try_create(code)
            .unwrap_or_else(|| panic!("Expected 0x{code:02X} to be a supported trigger."));
        assert_eq!(wire, trigger.wire_name());
        assert_eq!(label, trigger.display_label());
        assert!(
            TriggerKey::is_supported_key(code),
            "Expected 0x{code:02X} to report supported."
        );
        let parsed = TriggerKey::try_parse(wire)
            .unwrap_or_else(|| panic!("Expected wire name {wire} to parse."));
        assert_eq!(trigger, parsed);
        assert!(
            TriggerKey::try_parse(&wire.to_lowercase()).is_some(),
            "Expected wire name {wire} to parse case-insensitively."
        );
    }

    assert_eq!("CapsLock", TriggerKey::CAPS_LOCK.wire_name());
    assert_eq!("Caps Lock", TriggerKey::CAPS_LOCK.display_label());
    assert_eq!("ScrollLock", TriggerKey::SCROLL_LOCK.wire_name());
    assert_eq!("Scroll Lock", TriggerKey::SCROLL_LOCK.display_label());
    assert_eq!("Caps Lock", TriggerKey::CAPS_LOCK.to_string());
}

#[test]
fn trigger_key_allowlist_rejects_reserved_keys() {
    // Modifiers, system-reserved, extended-scan-code, media, and synthetic codes.
    let rejected: &[u16] = &[
        0x00, 0x10, 0x11, 0x12, 0x13, 0x1B, 0x25, 0x26, 0x27, 0x28, 0x2C, 0x2D, 0x2E, 0x5B,
        0x5C, 0x5D, 0x6F, 0xA0, 0xA1, 0xA2, 0xA3, 0xA4, 0xA5, 0xAD, 0xB3, 0xE7, 0xFF,
    ];

    for &code in rejected {
        assert!(
            TriggerKey::try_create(code).is_none(),
            "Expected 0x{code:02X} to be rejected as a trigger."
        );
        assert!(
            !TriggerKey::is_supported_key(code),
            "Expected 0x{code:02X} to report unsupported."
        );
    }

    assert!(
        TriggerKey::try_parse("").is_none(),
        "Expected a missing trigger name to be rejected."
    );
    assert!(
        TriggerKey::try_parse("Win").is_none(),
        "Expected an unknown trigger name to be rejected."
    );
    assert!(
        TriggerKey::try_parse("Shift").is_none(),
        "Expected a modifier name to be rejected as a trigger."
    );
}

#[test]
fn trigger_wire_names_round_trip() {
    for wire in [
        "F9",
        "D3",
        "Space",
        "OemComma",
        "Enter",
        "J",
        "CapsLock",
        "ScrollLock",
    ] {
        let trigger =
            TriggerKey::try_parse(wire).unwrap_or_else(|| panic!("Expected {wire} to parse."));
        let configured = HyperkeySettings::defaults()
            .with_trigger(trigger)
            .expect("parsed triggers are supported");
        let result = json::parse(&json::serialize(&configured));

        assert!(!result.used_defaults);
        assert_eq!(trigger, result.settings.trigger());
    }

    let serialized = json::serialize(&HyperkeySettings::defaults());
    assert!(
        serialized.contains("\"schemaVersion\": 2"),
        "Expected new settings documents to declare schema version 2."
    );
}

#[test]
fn legacy_schema_version_one_still_loads() {
    let legacy_version_one = r#"
        {
          "schemaVersion": 1,
          "enabled": true,
          "trigger": "ScrollLock",
          "outputModifiers": ["Control"],
          "launchAtStartup": false,
          "launchToTray": true,
          "tapBehavior": "CapsLock"
        }
        "#;
    let result = json::parse(legacy_version_one);

    assert!(!result.used_defaults);
    assert_eq!(TriggerKey::SCROLL_LOCK, result.settings.trigger());
    assert_eq!(1, result.settings.schema_version());
    assert!(result.settings.launch_to_tray());
}

#[test]
fn unknown_trigger_name_falls_back_to_defaults() {
    let serialized = json::serialize(&HyperkeySettings::defaults())
        .replace("\"trigger\": \"CapsLock\"", "\"trigger\": \"Win\"");
    let result = json::parse(&serialized);

    assert!(result.used_defaults);
    assert_eq!(HyperkeySettings::defaults(), result.settings);
}
