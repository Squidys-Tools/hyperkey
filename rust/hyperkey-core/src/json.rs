use crate::keys::TriggerKey;
use crate::settings::{HyperkeySettings, OutputModifier, TapBehavior};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

#[derive(Debug)]
pub struct SettingsParseResult {
    pub settings: HyperkeySettings,
    pub used_defaults: bool,
    pub error: Option<String>,
}

/// Files on disk always declare the newest schema this binary writes.
/// Older schema versions remain readable (see `parse`).
pub fn serialize(settings: &HyperkeySettings) -> String {
    let document = PersistedSettingsWrite {
        schema_version: HyperkeySettings::CURRENT_SCHEMA_VERSION,
        enabled: settings.enabled(),
        trigger: to_wire_name_trigger(settings.trigger()).into_owned(),
        output_modifiers: settings
            .output_modifiers()
            .iter()
            .map(|modifier| to_wire_name_modifier(*modifier).to_string())
            .collect(),
        launch_at_startup: settings.launch_at_startup(),
        launch_to_tray: settings.launch_to_tray(),
        tap_behavior: to_wire_name_tap_behavior(settings.tap_behavior()).to_string(),
    };

    // Serializing plain values into a String cannot fail.
    serde_json::to_string_pretty(&document).expect("settings serialization is infallible")
}

/// Any parse failure returns the defaults alongside the error message, so a
/// corrupt or outdated file cannot take down the app.
pub fn parse(json: &str) -> SettingsParseResult {
    match parse_inner(json) {
        Ok(settings) => SettingsParseResult {
            settings,
            used_defaults: false,
            error: None,
        },
        Err(error) => SettingsParseResult {
            settings: HyperkeySettings::defaults(),
            used_defaults: true,
            error: Some(error),
        },
    }
}

fn parse_inner(json: &str) -> Result<HyperkeySettings, String> {
    let document: PersistedSettingsRead =
        serde_json::from_str(json).map_err(|error| error.to_string())?;

    // Schema validation runs before required-field checks, matching the C# order.
    let schema_version = match document.schema_version {
        Some(1) => 1,
        Some(2) => 2,
        Some(version) => {
            return Err(format!("Unsupported settings schema: {version}."));
        }
        None => {
            return Err("Unsupported settings schema: .".to_string());
        }
    };

    if document.enabled.is_none() || document.launch_at_startup.is_none() {
        return Err(
            "The settings document is missing a required boolean value.".to_string(),
        );
    }

    let trigger = parse_trigger(document.trigger.as_deref())?;
    let modifiers = parse_modifiers(document.output_modifiers.as_deref())?;
    let tap_behavior = parse_tap_behavior(document.tap_behavior.as_deref())?;

    HyperkeySettings::create_with_schema_version(
        document.enabled.unwrap_or(false),
        trigger,
        modifiers,
        document.launch_at_startup.unwrap_or(false),
        document.launch_to_tray.unwrap_or(false),
        tap_behavior,
        schema_version,
    )
}

fn parse_trigger(value: Option<&str>) -> Result<TriggerKey, String> {
    match value {
        Some(value) => TriggerKey::try_parse(value)
            .ok_or_else(|| format!("Unsupported trigger key: {value}.")),
        None => Err("Unsupported trigger key: missing.".to_string()),
    }
}

fn parse_tap_behavior(value: Option<&str>) -> Result<TapBehavior, String> {
    match value {
        // Settings created before tap behavior was implemented used "Undecided".
        Some(value)
            if value.eq_ignore_ascii_case("CapsLock")
                || value.eq_ignore_ascii_case("Undecided") =>
        {
            Ok(TapBehavior::CapsLock)
        }
        Some(value) => Err(format!("Unsupported tap behavior: {value}.")),
        None => Err("Unsupported tap behavior: missing.".to_string()),
    }
}

fn parse_modifiers(values: Option<&[String]>) -> Result<Vec<OutputModifier>, String> {
    let Some(values) = values else {
        return Err(
            "The settings document must contain one to three output modifiers.".to_string(),
        );
    };

    if values.is_empty() || values.len() > 3 {
        return Err(
            "The settings document must contain one to three output modifiers.".to_string(),
        );
    }

    let mut parsed = Vec::with_capacity(values.len());
    for value in values {
        let modifier = if value.eq_ignore_ascii_case("Control") {
            OutputModifier::Control
        } else if value.eq_ignore_ascii_case("Alt") {
            OutputModifier::Alt
        } else if value.eq_ignore_ascii_case("Shift") {
            OutputModifier::Shift
        } else {
            return Err(format!("Unsupported output modifier: {value}."));
        };

        if parsed.contains(&modifier) {
            return Err(
                "The settings document contains duplicate output modifiers.".to_string(),
            );
        }

        parsed.push(modifier);
    }

    Ok(parsed)
}

fn to_wire_name_trigger(trigger: TriggerKey) -> std::borrow::Cow<'static, str> {
    trigger.wire_name()
}

fn to_wire_name_modifier(modifier: OutputModifier) -> &'static str {
    match modifier {
        OutputModifier::Control => "Control",
        OutputModifier::Alt => "Alt",
        OutputModifier::Shift => "Shift",
    }
}

fn to_wire_name_tap_behavior(tap_behavior: TapBehavior) -> &'static str {
    match tap_behavior {
        TapBehavior::CapsLock => "CapsLock",
    }
}

#[derive(Serialize)]
struct PersistedSettingsWrite {
    #[serde(rename = "schemaVersion")]
    schema_version: i32,
    #[serde(rename = "enabled")]
    enabled: bool,
    #[serde(rename = "trigger")]
    trigger: String,
    #[serde(rename = "outputModifiers")]
    output_modifiers: Vec<String>,
    #[serde(rename = "launchAtStartup")]
    launch_at_startup: bool,
    #[serde(rename = "launchToTray")]
    launch_to_tray: bool,
    #[serde(rename = "tapBehavior")]
    tap_behavior: String,
}

/// Read side of the persisted document. Property names are matched
/// case-insensitively, like the C# `PropertyNameCaseInsensitive` option.
#[derive(Default)]
struct PersistedSettingsRead {
    schema_version: Option<i64>,
    enabled: Option<bool>,
    trigger: Option<String>,
    output_modifiers: Option<Vec<String>>,
    launch_at_startup: Option<bool>,
    launch_to_tray: Option<bool>,
    tap_behavior: Option<String>,
}

impl<'de> Deserialize<'de> for PersistedSettingsRead {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_map(PersistedSettingsVisitor)
    }
}

struct PersistedSettingsVisitor;

impl<'de> Visitor<'de> for PersistedSettingsVisitor {
    type Value = PersistedSettingsRead;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a settings document object")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: MapAccess<'de>,
    {
        let mut document = PersistedSettingsRead::default();
        while let Some(key) = map.next_key::<String>()? {
            if key.eq_ignore_ascii_case("schemaVersion") {
                document.schema_version = map.next_value()?;
            } else if key.eq_ignore_ascii_case("enabled") {
                document.enabled = map.next_value()?;
            } else if key.eq_ignore_ascii_case("trigger") {
                document.trigger = map.next_value()?;
            } else if key.eq_ignore_ascii_case("outputModifiers") {
                document.output_modifiers = map.next_value()?;
            } else if key.eq_ignore_ascii_case("launchAtStartup") {
                document.launch_at_startup = map.next_value()?;
            } else if key.eq_ignore_ascii_case("launchToTray") {
                document.launch_to_tray = map.next_value()?;
            } else if key.eq_ignore_ascii_case("tapBehavior") {
                document.tap_behavior = map.next_value()?;
            } else {
                map.next_value::<IgnoredAny>()?;
            }
        }

        Ok(document)
    }
}
