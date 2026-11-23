//! User-facing text: hints, labels, and status wording. Pure functions and
//! constants, so the wording is testable without a window.
use hyperkey_core::keys::ChordModifier;
use hyperkey_core::settings::OutputModifier;
use hyperkey_input::engine::InputEngineStatus;

pub const IDLE_TRIGGER_HINT: &str = "Select to rebind — press any key. Esc cancels.";
pub const CAPTURE_TRIGGER_HINT: &str = "Press the key to use as trigger. Esc cancels.";
pub const MODIFIER_KEY_HINT: &str = "Modifier and system keys can't be the trigger. Press a letter, digit, \
     function key, or punctuation — or press Esc to cancel.";
pub const INVALID_KEY_HINT: &str =
    "That key can't be used as the trigger. Try another key, or press Esc to cancel.";
pub const MODIFIER_CHOICE_HINT: &str = "Choose one or more modifiers.";
pub const MODIFIER_REQUIRED_HINT: &str = "Choose at least one modifier.";
pub const TYPING_SAFE_NOTE: &str =
    "Typing-safe: a key counts as a shortcut only after a full press or 200 ms.";
pub const TAP_ROW_TITLE: &str = "When tapped";
pub const TAP_BEHAVIOR_HINT: &str =
    "What a quick tap of the trigger does. Holding it longer than half a second sends nothing.";
pub const TAP_ESCAPE_LABEL: &str = "Escape";
pub const TAP_NOTHING_LABEL: &str = "Nothing";
pub const TRY_IT_TITLE: &str = "Try it";
pub const TRY_IT_WAITING: &str = "Waiting…";

/// The "Try it" row description.
pub fn try_it_hint(trigger_label: &str) -> String {
    format!("Hold {trigger_label} and press any key.")
}

/// A chord for display, e.g. "Ctrl + Alt + Shift + F": modifiers in the
/// fixed Win, Control, Alt, Shift order, each once, then the key.
pub fn chord_label(modifiers: &[ChordModifier], key_label: &str) -> String {
    let mut ordered = modifiers.to_vec();
    ordered.sort();
    ordered.dedup();
    let mut parts: Vec<&str> = ordered.iter().map(chord_modifier_label).collect();
    parts.push(key_label);
    parts.join(" + ")
}

fn chord_modifier_label(modifier: &ChordModifier) -> &'static str {
    match modifier {
        ChordModifier::Win => "Win",
        ChordModifier::Control => "Ctrl",
        ChordModifier::Alt => "Alt",
        ChordModifier::Shift => "Shift",
    }
}

/// The header badge: a one-word summary of whether Hyperkey is working.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Badge {
    On,
    Off,
    Starting,
    NeedsAttention,
}

impl Badge {
    pub fn of(enabled: bool, status: InputEngineStatus) -> Self {
        match (enabled, status) {
            (false, _) => Badge::Off,
            (true, InputEngineStatus::Failed) => Badge::NeedsAttention,
            (true, InputEngineStatus::Starting) => Badge::Starting,
            (true, _) => Badge::On,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Badge::On => "On",
            Badge::Off => "Off",
            Badge::Starting => "Starting…",
            Badge::NeedsAttention => "Needs attention",
        }
    }
}

/// The secondary hint under the header: what holding the trigger does, or
/// why it currently does nothing.
pub fn status_hint(
    enabled: bool,
    status: InputEngineStatus,
    trigger_label: &str,
    modifiers: &[OutputModifier],
    status_error: Option<&str>,
) -> String {
    if !enabled {
        return "The modifier layer is currently disabled.".to_string();
    }

    match status {
        InputEngineStatus::Starting => "Starting the keyboard hook...".to_string(),
        InputEngineStatus::Running => {
            let joined = ordered_modifier_labels(modifiers).join(" + ");
            format!("Hold {trigger_label} to use {joined}.")
        }
        InputEngineStatus::Stopping => "Stopping the keyboard hook...".to_string(),
        InputEngineStatus::Failed => format!(
            "The keyboard hook is unavailable. {}",
            status_error.unwrap_or("No error details were reported.")
        ),
        InputEngineStatus::Stopped => "The keyboard engine is stopped.".to_string(),
    }
}

/// Modifier labels in the fixed Control, Alt, Shift order.
fn ordered_modifier_labels(modifiers: &[OutputModifier]) -> Vec<&'static str> {
    [
        OutputModifier::Control,
        OutputModifier::Alt,
        OutputModifier::Shift,
    ]
    .iter()
    .filter(|modifier| modifiers.contains(modifier))
    .map(modifier_label)
    .collect()
}

pub fn modifier_label(modifier: &OutputModifier) -> &'static str {
    match modifier {
        OutputModifier::Control => "Ctrl",
        OutputModifier::Alt => "Alt",
        OutputModifier::Shift => "Shift",
    }
}

pub fn engine_status_label(status: InputEngineStatus) -> &'static str {
    match status {
        InputEngineStatus::Running => "Running",
        InputEngineStatus::Stopped => "Stopped",
        InputEngineStatus::Starting => "Starting",
        InputEngineStatus::Stopping => "Stopping",
        InputEngineStatus::Failed => "Needs attention",
    }
}

pub fn version_label() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_MODIFIERS: [OutputModifier; 3] = [
        OutputModifier::Control,
        OutputModifier::Alt,
        OutputModifier::Shift,
    ];

    #[test]
    fn the_badge_summarises_enabled_and_engine_state() {
        use InputEngineStatus::*;
        assert_eq!(Badge::of(false, Running), Badge::Off);
        assert_eq!(Badge::of(true, Running), Badge::On);
        assert_eq!(Badge::of(true, Stopping), Badge::On);
        assert_eq!(Badge::of(true, Starting), Badge::Starting);
        assert_eq!(Badge::of(true, Failed), Badge::NeedsAttention);
        assert_eq!(Badge::NeedsAttention.label(), "Needs attention");
        assert_eq!(Badge::Starting.label(), "Starting…");
    }

    #[test]
    fn engine_status_label_matches_the_csharp_wording() {
        assert_eq!(engine_status_label(InputEngineStatus::Running), "Running");
        assert_eq!(engine_status_label(InputEngineStatus::Stopped), "Stopped");
        assert_eq!(engine_status_label(InputEngineStatus::Starting), "Starting");
        assert_eq!(engine_status_label(InputEngineStatus::Stopping), "Stopping");
        assert_eq!(
            engine_status_label(InputEngineStatus::Failed),
            "Needs attention"
        );
    }

    #[test]
    fn status_hint_when_disabled() {
        assert_eq!(
            status_hint(
                false,
                InputEngineStatus::Stopped,
                "Caps Lock",
                &ALL_MODIFIERS,
                None,
            ),
            "The modifier layer is currently disabled."
        );
    }

    #[test]
    fn status_hint_when_running_joins_trigger_and_modifiers() {
        assert_eq!(
            status_hint(
                true,
                InputEngineStatus::Running,
                "Caps Lock",
                &ALL_MODIFIERS,
                None,
            ),
            "Hold Caps Lock to use Ctrl + Alt + Shift."
        );
    }

    #[test]
    fn status_hint_orders_modifiers_canonically() {
        let reordered = [OutputModifier::Shift, OutputModifier::Control];
        assert_eq!(
            status_hint(true, InputEngineStatus::Running, "F13", &reordered, None),
            "Hold F13 to use Ctrl + Shift."
        );
    }

    #[test]
    fn status_hint_transient_states() {
        assert_eq!(
            status_hint(
                true,
                InputEngineStatus::Starting,
                "Caps Lock",
                &ALL_MODIFIERS,
                None
            ),
            "Starting the keyboard hook..."
        );
        assert_eq!(
            status_hint(
                true,
                InputEngineStatus::Stopping,
                "Caps Lock",
                &ALL_MODIFIERS,
                None
            ),
            "Stopping the keyboard hook..."
        );
        assert_eq!(
            status_hint(
                true,
                InputEngineStatus::Stopped,
                "Caps Lock",
                &ALL_MODIFIERS,
                None
            ),
            "The keyboard engine is stopped."
        );
    }

    #[test]
    fn status_hint_when_failed_reports_the_error() {
        assert_eq!(
            status_hint(
                true,
                InputEngineStatus::Failed,
                "Caps Lock",
                &ALL_MODIFIERS,
                Some("the hook was unloaded"),
            ),
            "The keyboard hook is unavailable. the hook was unloaded"
        );
        assert_eq!(
            status_hint(
                true,
                InputEngineStatus::Failed,
                "Caps Lock",
                &ALL_MODIFIERS,
                None
            ),
            "The keyboard hook is unavailable. No error details were reported."
        );
    }

    #[test]
    fn version_label_is_v_plus_the_crate_version() {
        assert_eq!(version_label(), format!("v{}", env!("CARGO_PKG_VERSION")));
        assert!(version_label().starts_with('v'));
    }

    #[test]
    fn chord_label_orders_modifiers_then_the_key() {
        let reordered = [ChordModifier::Shift, ChordModifier::Control];
        assert_eq!(chord_label(&reordered, "F"), "Ctrl + Shift + F");
        assert_eq!(
            chord_label(
                &[
                    ChordModifier::Shift,
                    ChordModifier::Alt,
                    ChordModifier::Win,
                    ChordModifier::Control,
                    ChordModifier::Alt,
                ],
                "Space"
            ),
            "Win + Ctrl + Alt + Shift + Space"
        );
    }

    #[test]
    fn try_it_hint_names_the_trigger() {
        assert_eq!(
            try_it_hint("Caps Lock"),
            "Hold Caps Lock and press any key."
        );
    }
}
