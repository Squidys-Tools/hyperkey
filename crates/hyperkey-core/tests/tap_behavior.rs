//! Tap-behavior settings: wire names, schema-3 migration, round-trips.

use hyperkey_core::json;
use hyperkey_core::settings::{HyperkeySettings, TapBehavior};

fn document(schema: Option<i32>, tap_behavior: Option<&str>) -> String {
    let mut fields = vec![
        r#""enabled": true"#.to_string(),
        r#""trigger": "CapsLock""#.to_string(),
        r#""outputModifiers": ["Control", "Alt"]"#.to_string(),
        r#""launchAtStartup": false"#.to_string(),
        r#""launchToTray": true"#.to_string(),
    ];
    if let Some(schema) = schema {
        fields.push(format!(r#""schemaVersion": {schema}"#));
    }
    if let Some(tap_behavior) = tap_behavior {
        fields.push(format!(r#""tapBehavior": {tap_behavior}"#));
    }
    format!("{{{}}}", fields.join(", "))
}

#[test]
fn default_tap_behavior_is_send_trigger() {
    assert_eq!(TapBehavior::SendTrigger, TapBehavior::default());
    assert_eq!(
        TapBehavior::SendTrigger,
        HyperkeySettings::defaults().tap_behavior()
    );
}

#[test]
fn current_schema_version_is_three() {
    assert_eq!(3, HyperkeySettings::CURRENT_SCHEMA_VERSION);
}

#[test]
fn every_tap_behavior_round_trips_with_its_wire_name() {
    for (behavior, wire) in [
        (TapBehavior::SendTrigger, "Trigger"),
        (TapBehavior::Escape, "Escape"),
        (TapBehavior::Nothing, "Nothing"),
    ] {
        let settings = HyperkeySettings::defaults().with_tap_behavior(behavior);
        assert_eq!(behavior, settings.tap_behavior());

        let serialized = json::serialize(&settings);
        assert!(
            serialized.contains(&format!("\"tapBehavior\": \"{wire}\"")),
            "Expected {behavior:?} to serialize as {wire}: {serialized}"
        );
        assert!(serialized.contains("\"schemaVersion\": 3"));

        let parsed = json::parse(&serialized);
        assert!(!parsed.used_defaults);
        assert!(!parsed.repaired, "{:?}", parsed.repairs);
        assert_eq!(settings, parsed.settings);
    }
}

#[test]
fn wire_names_parse_case_insensitively() {
    for (wire, expected) in [
        ("Trigger", TapBehavior::SendTrigger),
        ("trigger", TapBehavior::SendTrigger),
        ("CapsLock", TapBehavior::SendTrigger),
        ("CAPSLOCK", TapBehavior::SendTrigger),
        ("Undecided", TapBehavior::SendTrigger),
        ("undecided", TapBehavior::SendTrigger),
        ("Escape", TapBehavior::Escape),
        ("eScApE", TapBehavior::Escape),
        ("Nothing", TapBehavior::Nothing),
        ("nothing", TapBehavior::Nothing),
        ("None", TapBehavior::Nothing),
        ("NONE", TapBehavior::Nothing),
    ] {
        let parsed = json::parse(&document(Some(3), Some(&format!("\"{wire}\""))));
        assert!(!parsed.used_defaults, "{wire}");
        assert!(!parsed.repaired, "{wire}: {:?}", parsed.repairs);
        assert_eq!(expected, parsed.settings.tap_behavior(), "{wire}");
    }
}

#[test]
fn missing_or_null_tap_behavior_is_send_trigger_without_a_repair() {
    for tap in [None, Some("null")] {
        let parsed = json::parse(&document(Some(3), tap));
        assert!(!parsed.used_defaults);
        assert!(!parsed.repaired, "{:?}", parsed.repairs);
        assert_eq!(TapBehavior::SendTrigger, parsed.settings.tap_behavior());
    }
}

#[test]
fn unknown_tap_behavior_is_repaired_to_send_trigger() {
    for wire in ["\"Invert\"", "\"\"", "\"Esc\""] {
        let parsed = json::parse(&document(Some(3), Some(wire)));
        assert!(!parsed.used_defaults, "{wire}");
        assert!(parsed.repaired, "{wire}");
        assert_eq!(1, parsed.repairs.len());
        assert!(
            parsed.repairs[0].contains("tap behavior"),
            "{:?}",
            parsed.repairs
        );
        assert_eq!(TapBehavior::SendTrigger, parsed.settings.tap_behavior());
    }
}

#[test]
fn schema_two_documents_migrate_and_keep_their_values() {
    let parsed = json::parse(&document(Some(2), Some("\"CapsLock\"")));
    assert!(!parsed.used_defaults);
    assert!(!parsed.repaired);
    assert_eq!(2, parsed.settings.schema_version());
    assert_eq!(TapBehavior::SendTrigger, parsed.settings.tap_behavior());
    assert!(parsed.settings.launch_to_tray());

    // Saving a migrated file writes the current schema and the new name.
    let rewritten = json::serialize(&parsed.settings);
    assert!(rewritten.contains("\"schemaVersion\": 3"));
    assert!(rewritten.contains("\"tapBehavior\": \"Trigger\""));
    let reparsed = json::parse(&rewritten);
    assert_eq!(3, reparsed.settings.schema_version());
    assert_eq!(TapBehavior::SendTrigger, reparsed.settings.tap_behavior());
}

#[test]
fn schema_one_and_unversioned_documents_still_read() {
    let v1 = json::parse(&document(Some(1), Some("\"Undecided\"")));
    assert!(!v1.used_defaults);
    assert_eq!(1, v1.settings.schema_version());
    assert_eq!(TapBehavior::SendTrigger, v1.settings.tap_behavior());

    let unversioned = json::parse(&document(None, Some("\"Escape\"")));
    assert!(!unversioned.used_defaults);
    assert_eq!(3, unversioned.settings.schema_version());
    assert_eq!(TapBehavior::Escape, unversioned.settings.tap_behavior());
}

#[test]
fn schema_four_is_rejected_without_quarantine() {
    let parsed = json::parse(&document(Some(4), Some("\"Escape\"")));
    assert!(parsed.used_defaults);
    assert!(!parsed.file_corrupt);
}

#[test]
fn with_tap_behavior_keeps_every_other_value() {
    let base = HyperkeySettings::defaults()
        .with_enabled(false)
        .with_launch_to_tray(true);
    let changed = base.with_tap_behavior(TapBehavior::Nothing);
    assert_eq!(TapBehavior::Nothing, changed.tap_behavior());
    assert_eq!(base.with_tap_behavior(TapBehavior::SendTrigger), base);
    assert!(!changed.enabled());
    assert!(changed.launch_to_tray());
    assert_eq!(base.trigger(), changed.trigger());
    assert_eq!(base.output_modifiers(), changed.output_modifiers());
}

#[test]
fn create_accepts_every_tap_behavior() {
    for behavior in [
        TapBehavior::SendTrigger,
        TapBehavior::Escape,
        TapBehavior::Nothing,
    ] {
        let defaults = HyperkeySettings::defaults();
        let created = HyperkeySettings::create(
            true,
            defaults.trigger(),
            defaults.output_modifiers().to_vec(),
            true,
            false,
            behavior,
        )
        .expect("every tap behavior is valid");
        assert_eq!(behavior, created.tap_behavior());
    }
}
