use crate::keys::TriggerKey;
use crate::settings::{HyperkeySettings, OutputModifier, TapBehavior};
use serde::de::{IgnoredAny, MapAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

/// Outcome of parsing a settings document. A readable document whose values
/// are invalid yields settings with every invalid value reset to its default,
/// each reset reported in `repairs`. A document that cannot be used at all
/// (corrupt JSON, wrong value types, an unsupported schema) yields
/// `HyperkeySettings::defaults()` and a `fatal_error`; `file_corrupt` splits
/// that fallback between an unparseable document and a readable one. This
/// mirrors the C# `SettingsParseResult` record: `used_defaults`, `repaired`,
/// and `error` are its `UsedDefaults`, `Repaired`, and `Error` fields,
/// `repairs` is the per-value report `Error` is joined from, `file_corrupt`
/// is its `FileCorrupt` flag, and `fatal_error` isolates the unrecoverable
/// failure for callers that only care about the fatal path.
#[derive(Debug)]
pub struct ParseOutcome {
    pub settings: HyperkeySettings,
    /// True only when the document could not be read at all and the
    /// compiled-in defaults were loaded wholesale.
    pub used_defaults: bool,
    /// True when at least one value was reset to its default.
    pub repaired: bool,
    /// One message per repaired value, in field order.
    pub repairs: Vec<String>,
    /// The unrecoverable failure, present only when `used_defaults` is true.
    pub fatal_error: Option<String>,
    /// Splits `used_defaults` the way the C# `FileCorrupt` flag does: true
    /// only when the document could not be parsed at all, false when it
    /// parsed but cannot be used (for example a schema version written by a
    /// newer Hyperkey). Callers quarantine only a corrupt file; a readable
    /// one stays in place so its values can be recovered later.
    pub file_corrupt: bool,
    /// The single user-facing message, equivalent to the C# `Error` field:
    /// the fatal error for a corrupt document, the joined repair report when
    /// values were reset, or `None` for a clean parse.
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

/// Tolerant parse: an unversioned document migrates against the current
/// schema, and each invalid value is independently reset to its default and
/// reported in `repairs`. Only a document that cannot be read at all falls
/// back to defaults wholesale with a `fatal_error`.
pub fn parse(json: &str) -> ParseOutcome {
    match parse_inner(json) {
        Ok((settings, repairs)) => {
            let error = if repairs.is_empty() {
                None
            } else {
                Some(repairs.join("; "))
            };
            ParseOutcome {
                settings,
                used_defaults: false,
                repaired: !repairs.is_empty(),
                repairs,
                fatal_error: None,
                file_corrupt: false,
                error,
            }
        }
        Err((file_corrupt, error)) => ParseOutcome {
            settings: HyperkeySettings::defaults(),
            used_defaults: true,
            repaired: false,
            repairs: Vec::new(),
            fatal_error: Some(error.clone()),
            file_corrupt,
            error: Some(error),
        },
    }
}

/// Decodes a settings file's raw bytes the way the C# `File.ReadAllText` did
/// — a UTF-8 BOM is ignored, a UTF-16 LE or BE BOM (Notepad's "Unicode"
/// output) is transcoded, and UTF-32 BOMs are handled the same way — then
/// parses. Anything else is decoded as UTF-8 lossily, so a stray invalid
/// byte reaches the tolerant parser instead of failing at the read.
pub fn parse_bytes(bytes: &[u8]) -> ParseOutcome {
    parse(&decode_text(bytes))
}

fn decode_text(bytes: &[u8]) -> String {
    const UTF8_BOM: &[u8] = &[0xEF, 0xBB, 0xBF];
    // The C# `File.ReadAllText` also recognized UTF-32. Its LE BOM must be
    // checked before UTF-16 LE because they share the same two-byte lead.
    const UTF32_LE_BOM: &[u8] = &[0xFF, 0xFE, 0x00, 0x00];
    const UTF32_BE_BOM: &[u8] = &[0x00, 0x00, 0xFE, 0xFF];
    const UTF16_LE_BOM: &[u8] = &[0xFF, 0xFE];
    const UTF16_BE_BOM: &[u8] = &[0xFE, 0xFF];

    if bytes.starts_with(UTF32_LE_BOM) {
        let (units, _) = bytes[4..].as_chunks::<4>();
        return units
            .iter()
            .map(|unit| decode_utf32_unit(u32::from_le_bytes(*unit)))
            .collect();
    }
    if bytes.starts_with(UTF32_BE_BOM) {
        let (units, _) = bytes[4..].as_chunks::<4>();
        return units
            .iter()
            .map(|unit| decode_utf32_unit(u32::from_be_bytes(*unit)))
            .collect();
    }
    if bytes.starts_with(UTF16_LE_BOM) {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_le_bytes(*pair))
            .collect::<Vec<u16>>();
        return String::from_utf16_lossy(&units);
    }
    if bytes.starts_with(UTF16_BE_BOM) {
        let (pairs, _) = bytes[2..].as_chunks::<2>();
        let units = pairs
            .iter()
            .map(|pair| u16::from_be_bytes(*pair))
            .collect::<Vec<u16>>();
        return String::from_utf16_lossy(&units);
    }
    String::from_utf8_lossy(bytes.strip_prefix(UTF8_BOM).unwrap_or(bytes)).into_owned()
}

fn decode_utf32_unit(unit: u32) -> char {
    // Out-of-range code points — lone surrogates included — become the
    // replacement character, matching the lossy UTF-16 paths above.
    char::from_u32(unit).unwrap_or(char::REPLACEMENT_CHARACTER)
}

fn parse_inner(json: &str) -> Result<(HyperkeySettings, Vec<String>), (bool, String)> {
    // A document that cannot be parsed at all is corrupt; the flag flows to
    // `ParseOutcome::file_corrupt` so callers quarantine it like C# did.
    let document: PersistedSettingsRead =
        serde_json::from_str(json).map_err(|error| (true, error.to_string()))?;

    // A missing schemaVersion marks a document written before versioning
    // existed; it is migrated by validating its fields against the current
    // schema. A document that parsed but declares a version this build
    // cannot read is not corrupt: the false flag keeps the file in place so
    // a newer Hyperkey can still recover its values.
    let schema_version = document
        .schema_version
        .unwrap_or(i64::from(HyperkeySettings::CURRENT_SCHEMA_VERSION));
    if !(1..=i64::from(HyperkeySettings::CURRENT_SCHEMA_VERSION)).contains(&schema_version) {
        return Err((
            false,
            format!("Unsupported settings schema: {schema_version}."),
        ));
    }

    let defaults = HyperkeySettings::defaults();
    let mut repairs = Vec::new();

    let enabled = document.enabled.unwrap_or_else(|| {
        repairs.push("enabled was reset to its default".to_string());
        defaults.enabled()
    });
    let launch_at_startup = document.launch_at_startup.unwrap_or_else(|| {
        repairs.push("launchAtStartup was reset to its default".to_string());
        defaults.launch_at_startup()
    });
    // Documents written before launchToTray existed may omit it silently.
    let launch_to_tray = document
        .launch_to_tray
        .unwrap_or_else(|| defaults.launch_to_tray());
    let trigger = document
        .trigger
        .as_deref()
        .and_then(TriggerKey::try_parse)
        .unwrap_or_else(|| {
            repairs.push(format!(
                "trigger \"{}\" was reset to {}",
                document.trigger.as_deref().unwrap_or("missing"),
                defaults.trigger().display_label()
            ));
            defaults.trigger()
        });
    let modifiers = parse_modifiers(document.output_modifiers.as_deref(), &mut repairs);
    let tap_behavior = parse_tap_behavior(document.tap_behavior.as_deref(), &mut repairs);

    let settings = HyperkeySettings::create_with_schema_version(
        enabled,
        trigger,
        modifiers,
        launch_at_startup,
        launch_to_tray,
        tap_behavior,
        schema_version as i32,
    )
    .map_err(|error| (true, error))?;

    Ok((settings, repairs))
}

fn parse_modifiers(
    values: Option<&[Option<String>]>,
    repairs: &mut Vec<String>,
) -> Vec<OutputModifier> {
    let reset = |repairs: &mut Vec<String>| {
        repairs.push("output modifiers were reset to their defaults".to_string());
        HyperkeySettings::defaults().output_modifiers().to_vec()
    };

    let Some(values) = values else {
        return reset(repairs);
    };
    if values.is_empty() {
        return reset(repairs);
    }

    let mut parsed = Vec::with_capacity(values.len());
    let mut dropped_invalid = false;
    for value in values {
        // A null element is just another invalid entry: the C# field was
        // `string?[]` and dropped it individually rather than failing the
        // whole document.
        let modifier = match value.as_deref() {
            Some(name) if name.eq_ignore_ascii_case("Control") => OutputModifier::Control,
            Some(name) if name.eq_ignore_ascii_case("Alt") => OutputModifier::Alt,
            Some(name) if name.eq_ignore_ascii_case("Shift") => OutputModifier::Shift,
            _ => {
                dropped_invalid = true;
                continue;
            }
        };

        if parsed.contains(&modifier) {
            dropped_invalid = true;
            continue;
        }

        parsed.push(modifier);
    }

    if parsed.is_empty() {
        return reset(repairs);
    }

    if dropped_invalid {
        repairs.push("unknown or duplicate output modifiers were dropped".to_string());
    }

    parsed
}

fn parse_tap_behavior(value: Option<&str>, repairs: &mut Vec<String>) -> TapBehavior {
    match value {
        // Documents written before the field existed are allowed to omit it.
        None => TapBehavior::CapsLock,
        // Settings created before tap behavior was implemented used "Undecided".
        Some(value)
            if value.eq_ignore_ascii_case("CapsLock")
                || value.eq_ignore_ascii_case("Undecided") =>
        {
            TapBehavior::CapsLock
        }
        Some(value) => {
            repairs.push(format!("tap behavior \"{value}\" was reset to Caps Lock"));
            TapBehavior::CapsLock
        }
    }
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
/// A field holding the wrong JSON type fails the whole read, like the C#
/// `JsonException` path; a `null` field reads as absent, and a `null`
/// element inside `outputModifiers` is treated as one invalid value —
/// the C# field was `string?[]`.
#[derive(Default)]
struct PersistedSettingsRead {
    schema_version: Option<i64>,
    enabled: Option<bool>,
    trigger: Option<String>,
    output_modifiers: Option<Vec<Option<String>>>,
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
