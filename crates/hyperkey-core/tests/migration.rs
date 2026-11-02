//! Ported from the C# settings-migration tests — the parse-level cases. The
//! C# suite also covered `SettingsStore` behavior (the corrupt-file `.bak`
//! move, temp-file cleanup, store-level messages); the store lives in the app
//! crate, so those cases are not ported here.

use hyperkey_core::json;
use hyperkey_core::keys::TriggerKey;
use hyperkey_core::settings::{HyperkeySettings, OutputModifier, TapBehavior};

#[test]
fn unversioned_document_migrates_and_keeps_values() {
    const UNVERSIONED: &str = r#"
        {
          "enabled": false,
          "trigger": "F9",
          "outputModifiers": ["Control", "Shift"],
          "launchAtStartup": false,
          "launchToTray": true,
          "tapBehavior": "CapsLock"
        }
        "#;

    let result = json::parse(UNVERSIONED);

    assert!(!result.used_defaults);
    assert!(!result.repaired);
    assert_eq!(
        HyperkeySettings::CURRENT_SCHEMA_VERSION,
        result.settings.schema_version()
    );
    assert!(!result.settings.enabled());
    assert_eq!(
        TriggerKey::try_parse("F9").expect("Expected F9 to parse."),
        result.settings.trigger()
    );
    assert_eq!(
        &[OutputModifier::Control, OutputModifier::Shift],
        result.settings.output_modifiers()
    );
    assert!(result.settings.launch_to_tray());
}

#[test]
fn unversioned_document_missing_newer_fields_still_migrates() {
    const UNVERSIONED: &str = r#"
        {
          "enabled": true,
          "trigger": "ScrollLock",
          "outputModifiers": ["Alt"],
          "launchAtStartup": false
        }
        "#;

    let result = json::parse(UNVERSIONED);

    assert!(!result.used_defaults);
    assert!(!result.repaired);
    assert_eq!(TriggerKey::SCROLL_LOCK, result.settings.trigger());
    assert_eq!(
        &[OutputModifier::Alt],
        result.settings.output_modifiers()
    );
    assert!(!result.settings.launch_to_tray());
    assert_eq!(TapBehavior::CapsLock, result.settings.tap_behavior());
}

#[test]
fn invalid_trigger_resets_to_default_and_preserves_other_values() {
    const DOCUMENT: &str = r#"
        {
          "schemaVersion": 2,
          "enabled": false,
          "trigger": "Win",
          "outputModifiers": ["Alt"],
          "launchAtStartup": false,
          "launchToTray": true,
          "tapBehavior": "CapsLock"
        }
        "#;

    let result = json::parse(DOCUMENT);

    assert!(!result.used_defaults);
    assert!(result.repaired);
    assert_eq!(
        HyperkeySettings::defaults().trigger(),
        result.settings.trigger()
    );
    assert!(!result.settings.enabled());
    assert!(result.settings.launch_to_tray());
    assert_eq!(
        &[OutputModifier::Alt],
        result.settings.output_modifiers()
    );
}

#[test]
fn invalid_output_modifiers_reset_to_defaults() {
    const TEMPLATE: &str = r#"
        {
          "schemaVersion": 2,
          "enabled": true,
          "trigger": "CapsLock",
          "outputModifiers": MODIFIERS,
          "launchAtStartup": true
        }
        "#;

    let defaults = HyperkeySettings::defaults();
    let default_modifiers = defaults.output_modifiers().to_vec();

    let empty = json::parse(&TEMPLATE.replace("MODIFIERS", "[]"));
    assert!(!empty.used_defaults);
    assert!(empty.repaired);
    assert_eq!(default_modifiers, empty.settings.output_modifiers());

    let unknown_only = json::parse(&TEMPLATE.replace("MODIFIERS", "[\"Meta\"]"));
    assert!(unknown_only.repaired);
    assert_eq!(default_modifiers, unknown_only.settings.output_modifiers());

    let mixed = json::parse(
        &TEMPLATE.replace("MODIFIERS", "[\"Control\", \"Meta\", \"Shift\", \"Shift\"]"),
    );
    assert!(!mixed.used_defaults);
    assert!(mixed.repaired);
    assert_eq!(
        &[OutputModifier::Control, OutputModifier::Shift],
        mixed.settings.output_modifiers()
    );
}

#[test]
fn unknown_tap_behavior_resets_to_caps_lock() {
    const TEMPLATE: &str = r#"
        {
          "schemaVersion": 2,
          "enabled": true,
          "trigger": "CapsLock",
          "outputModifiers": ["Control"],
          "launchAtStartup": true,
          "tapBehavior": TAPBEHAVIOR
        }
        "#;

    let unknown = json::parse(&TEMPLATE.replace("TAPBEHAVIOR", "\"Invert\""));
    assert!(!unknown.used_defaults);
    assert!(unknown.repaired);
    assert_eq!(TapBehavior::CapsLock, unknown.settings.tap_behavior());

    let legacy = json::parse(&TEMPLATE.replace("TAPBEHAVIOR", "\"Undecided\""));
    assert!(!legacy.used_defaults);
    assert!(!legacy.repaired);
    assert_eq!(TapBehavior::CapsLock, legacy.settings.tap_behavior());
}

#[test]
fn missing_required_booleans_reset_to_defaults() {
    const DOCUMENT: &str = r#"
        {
          "schemaVersion": 2,
          "trigger": "CapsLock",
          "outputModifiers": ["Control"]
        }
        "#;

    let result = json::parse(DOCUMENT);
    let defaults = HyperkeySettings::defaults();

    assert!(!result.used_defaults);
    assert!(result.repaired);
    assert_eq!(defaults.enabled(), result.settings.enabled());
    assert_eq!(
        defaults.launch_at_startup(),
        result.settings.launch_at_startup()
    );
    assert_eq!(
        &[OutputModifier::Control],
        result.settings.output_modifiers()
    );
}

#[test]
fn unsupported_schema_version_falls_back_to_defaults() {
    const DOCUMENT: &str = r#"
        {
          "schemaVersion": 99,
          "enabled": false,
          "trigger": "F9",
          "outputModifiers": ["Alt"],
          "launchAtStartup": false,
          "launchToTray": true,
          "tapBehavior": "CapsLock"
        }
        "#;

    let result = json::parse(DOCUMENT);

    assert!(result.used_defaults);
    assert!(!result.repaired);
    assert_eq!(HyperkeySettings::defaults(), result.settings);
    assert!(
        result.fatal_error.is_some(),
        "Expected a schema error to be reported."
    );
}

#[test]
fn corrupt_json_falls_back_to_defaults() {
    let result = json::parse("{ \"enabled\": true, triger: [");

    assert!(result.used_defaults);
    assert!(!result.repaired);
    assert_eq!(HyperkeySettings::defaults(), result.settings);
    assert!(
        result.fatal_error.is_some(),
        "Expected a parse error to be reported."
    );
}
