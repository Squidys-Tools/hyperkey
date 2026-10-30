//! Port of the settings cases added in tests/Hyperkey.Core.Tests/SettingsTests.cs.
//! Two cases assert the tolerant-parse semantics the C# PR for settings
//! migration defines: missing required booleans and invalid modifier sets are
//! repaired per value, not treated as fatal, so their expected outcomes are
//! the repaired results rather than `used_defaults`.

use hyperkey_core::json;
use hyperkey_core::keys::TriggerKey;
use hyperkey_core::settings::{HyperkeySettings, OutputModifier, TapBehavior};

#[test]
fn malformed_json_falls_back_to_defaults() {
    for json in ["", "not json", "{", "null", "[1,2,3]", "\"text\""] {
        let result = json::parse(json);

        assert!(
            result.used_defaults,
            "Expected defaults for malformed input: {json}"
        );
        assert_eq!(HyperkeySettings::defaults(), result.settings);
        assert!(result.fatal_error.is_some());
    }
}

#[test]
fn unsupported_schema_versions_fall_back_to_defaults() {
    let serialized = json::serialize(&HyperkeySettings::defaults());
    for version in [0, 3, 99] {
        let result = json::parse(&serialized.replace(
            "\"schemaVersion\": 2",
            &format!("\"schemaVersion\": {version}"),
        ));

        assert!(
            result.used_defaults,
            "Expected defaults for schemaVersion {version}."
        );
        assert_eq!(HyperkeySettings::defaults(), result.settings);
    }
}

#[test]
fn missing_required_fields_are_repaired() {
    // Tolerant-parse semantics: absent booleans reset to their defaults and
    // are reported as repairs; the rest of the document still applies.
    let without_enabled = json::parse(
        r#"{
          "schemaVersion": 2,
          "trigger": "CapsLock",
          "outputModifiers": ["Control"],
          "launchAtStartup": true,
          "tapBehavior": "CapsLock"
        }"#,
    );
    assert!(!without_enabled.used_defaults);
    assert!(without_enabled.repaired);
    assert!(without_enabled.settings.enabled());

    let without_launch_at_startup = json::parse(
        r#"{
          "schemaVersion": 2,
          "enabled": true,
          "trigger": "CapsLock",
          "outputModifiers": ["Control"],
          "tapBehavior": "CapsLock"
        }"#,
    );
    assert!(!without_launch_at_startup.used_defaults);
    assert!(without_launch_at_startup.repaired);
    assert!(without_launch_at_startup.settings.launch_at_startup());
}

#[test]
fn invalid_modifier_sets_are_repaired() {
    // Tolerant-parse semantics: an unreadable modifier list resets to the
    // defaults, while unknown or duplicate entries are dropped individually.
    const TEMPLATE: &str = r#"
        {
          "schemaVersion": 2,
          "enabled": true,
          "trigger": "CapsLock",
          "outputModifiers": MODIFIERS,
          "launchAtStartup": true,
          "tapBehavior": "CapsLock"
        }
        "#;

    let default_modifiers = HyperkeySettings::defaults().output_modifiers().to_vec();
    let cases: &[(&str, Vec<OutputModifier>)] = &[
        ("[]", default_modifiers.clone()),
        (
            "[\"Control\", \"Alt\", \"Shift\", \"Control\"]",
            vec![
                OutputModifier::Control,
                OutputModifier::Alt,
                OutputModifier::Shift,
            ],
        ),
        ("[\"Control\", \"Control\"]", vec![OutputModifier::Control]),
        ("[\"Win\"]", default_modifiers.clone()),
        ("null", default_modifiers.clone()),
    ];

    for (modifiers, expected) in cases {
        let result = json::parse(&TEMPLATE.replace("MODIFIERS", modifiers));

        assert!(
            !result.used_defaults,
            "Expected repair (not defaults) for outputModifiers {modifiers}."
        );
        assert!(result.repaired);
        assert_eq!(expected, &result.settings.output_modifiers().to_vec());
    }
}

#[test]
fn modifier_and_trigger_names_parse_case_insensitively() {
    let result = json::parse(
        r#"{
          "schemaVersion": 2,
          "enabled": true,
          "trigger": "capslock",
          "outputModifiers": ["control", "ALT", "sHiFt"],
          "launchAtStartup": false,
          "launchToTray": false,
          "tapBehavior": "capslock"
        }"#,
    );

    assert!(!result.used_defaults);
    assert!(!result.repaired);
    assert_eq!(TriggerKey::CAPS_LOCK, result.settings.trigger());
    assert_eq!(
        &[
            OutputModifier::Control,
            OutputModifier::Alt,
            OutputModifier::Shift
        ],
        result.settings.output_modifiers()
    );
    assert_eq!(TapBehavior::CapsLock, result.settings.tap_behavior());
}

#[test]
fn missing_launch_to_tray_defaults_to_false_on_current_schema() {
    let result = json::parse(
        r#"{
          "schemaVersion": 2,
          "enabled": true,
          "trigger": "CapsLock",
          "outputModifiers": ["Control"],
          "launchAtStartup": true,
          "tapBehavior": "CapsLock"
        }"#,
    );

    assert!(!result.used_defaults);
    assert!(!result.repaired);
    assert!(!result.settings.launch_to_tray());
}

#[test]
fn create_rejects_invalid_modifier_sets() {
    assert!(HyperkeySettings::defaults()
        .with_output_modifiers(Vec::new())
        .is_err());

    assert!(HyperkeySettings::defaults()
        .with_output_modifiers(vec![
            OutputModifier::Control,
            OutputModifier::Alt,
            OutputModifier::Shift,
            OutputModifier::Control,
        ])
        .is_err());

    assert!(HyperkeySettings::defaults()
        .with_output_modifiers(vec![OutputModifier::Alt, OutputModifier::Alt])
        .is_err());
}

#[test]
fn create_rejects_unsupported_trigger_and_schema() {
    assert!(TriggerKey::try_create(0x41).is_some());
    assert!(
        TriggerKey::try_create(0x5B).is_none(),
        "Expected LWin to be rejected as a trigger."
    );

    assert!(HyperkeySettings::create_with_schema_version(
        true,
        TriggerKey::CAPS_LOCK,
        vec![OutputModifier::Control],
        true,
        false,
        TapBehavior::CapsLock,
        0,
    )
    .is_err());
}

#[test]
fn utf8_bom_bytes_parse() {
    // A BOM-marked UTF-8 file is what a hand-edited save from most editors
    // produces; the C# File.ReadAllText silently dropped the marker.
    let json = r#"{"schemaVersion": 2, "enabled": false, "trigger": "CapsLock", "outputModifiers": ["Control"], "launchAtStartup": false, "tapBehavior": "CapsLock"}"#;
    let mut bytes = vec![0xEF, 0xBB, 0xBF];
    bytes.extend_from_slice(json.as_bytes());

    let result = json::parse_bytes(&bytes);

    assert!(!result.used_defaults);
    assert!(!result.repaired);
    assert!(!result.settings.enabled());
}

#[test]
fn utf16_bom_bytes_parse() {
    // Notepad's "Unicode" encoding writes UTF-16 with a BOM; the C# reader
    // transcoded it, so the Rust store must not degrade the same file to
    // defaults.
    let json = r#"{"schemaVersion": 2, "enabled": false, "trigger": "CapsLock", "outputModifiers": ["Control"], "launchAtStartup": false, "tapBehavior": "CapsLock"}"#;

    let mut le = vec![0xFF, 0xFE];
    le.extend(json.encode_utf16().flat_map(u16::to_le_bytes));
    let result = json::parse_bytes(&le);
    assert!(!result.used_defaults);
    assert!(!result.settings.enabled());

    let mut be = vec![0xFE, 0xFF];
    be.extend(json.encode_utf16().flat_map(u16::to_be_bytes));
    let result = json::parse_bytes(&be);
    assert!(!result.used_defaults);
    assert!(!result.settings.enabled());
}
