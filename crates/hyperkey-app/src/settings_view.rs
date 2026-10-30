//! The settings window, re-ported to match the WPF dark prototype: a
//! two-column layout with a decorative keycap panel on the left and the
//! settings sections separated by hairline dividers on the right. Colors
//! come from the prototype's dark palette; in light system mode the same
//! layout runs on iced's light palette with the deeper accent.
use crate::{FocusTarget, Message, State};
use hyperkey_core::settings::OutputModifier;
use hyperkey_input::engine::InputEngineStatus;
use iced::border::Radius;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, checkbox, column, container, row, rule, scrollable, text, Space};
use iced::{Alignment, Background, Border, Color, Element, Fill, Font, Padding, Theme};

/// The dark design palette from the WPF prototype. `main` reuses `CANVAS`,
/// `ACCENT`, and `ACCENT_STRONG` when it builds the dark theme.
pub(crate) mod palette {
    use iced::Color;

    /// The window background in dark mode (#1B1C20).
    pub const CANVAS: Color = Color::from_rgb8(0x1B, 0x1C, 0x20);
    /// Checkbox, button, and field fill (#303136).
    pub const CONTROL: Color = Color::from_rgb8(0x30, 0x31, 0x36);
    /// Hovered control fill; doubles as the divider color (#3A3B40).
    pub const CONTROL_HOVER: Color = Color::from_rgb8(0x3A, 0x3B, 0x40);
    /// Secondary text, decorations, and the field chevron (#A5A6AC).
    pub const SECONDARY_TEXT: Color = Color::from_rgb8(0xA5, 0xA6, 0xAC);
    /// The accent in dark mode (#4C8DF6).
    pub const ACCENT: Color = Color::from_rgb8(0x4C, 0x8D, 0xF6);
    /// Pressed or checked+hovered accent; also the light-mode accent
    /// (#3B74D8).
    pub const ACCENT_STRONG: Color = Color::from_rgb8(0x3B, 0x74, 0xD8);
    /// Text and glyphs drawn on the accent (#FFFFFF).
    pub const ACCENT_TEXT: Color = Color::WHITE;
}

const LEFT_COLUMN_WIDTH: f32 = 222.0;
const COLUMN_GAP: f32 = 34.0;
const SECTION_GAP: f32 = 16.0;
const KEYCAP_WIDTH: f32 = 150.0;
const KEYCAP_HEIGHT: f32 = 120.0;
const FIELD_WIDTH: f32 = 220.0;
const FIELD_HEIGHT: f32 = 46.0;
const STATUS_WIDTH: f32 = 260.0;
const CHECKBOX_SIZE: f32 = 26.0;
const CHECKBOX_SPACING: f32 = 12.0;
const LABEL_SIZE: f32 = 17.0;
const CAPTION_SIZE: f32 = 13.0;
const HINT_SIZE: f32 = 11.0;
const BUTTON_SIZE: f32 = 13.0;
const STATUS_SIZE: f32 = 14.0;

pub fn view(state: &State) -> Element<'_, Message> {
    container(
        row![left_column(state), right_column(state)]
            .spacing(COLUMN_GAP)
            .height(Fill),
    )
    .padding(Padding::ZERO.top(20.0).right(28.0).bottom(24.0).left(28.0))
    .width(Fill)
    .height(Fill)
    .into()
}

/// The decorative left column: keycap illustration, modifier labels, the
/// sparkle motif, and the wordmark at the bottom.
fn left_column(state: &State) -> Element<'_, Message> {
    // The keycap illustration: a rounded square with a small dot near the
    // top and the trigger label near the bottom, like the prototype.
    let keycap = container(
        column![
            container(
                container(Space::new())
                    .width(10.0)
                    .height(10.0)
                    .style(dot_style),
            )
            .padding(Padding::ZERO.top(18.0).left(20.0)),
            Space::new().height(Fill),
            text(keycap_label(state.settings.trigger().display_label().as_ref()))
                .size(16.0)
                .style(secondary_text)
                .width(Fill)
                .align_x(text::Alignment::Center),
        ]
        .height(Fill),
    )
    .width(KEYCAP_WIDTH)
    .height(KEYCAP_HEIGHT)
    .padding(Padding::ZERO.bottom(14.0))
    .style(keycap_style);

    // The keycap and the modifier labels share the keycap's width so the
    // labels line up with its left edge.
    let keycap_block = container(
        column![
            keycap,
            text("Ctrl   Alt   Shift")
                .size(15.0)
                .style(secondary_text)
                .width(KEYCAP_WIDTH),
        ]
        .spacing(24.0),
    )
    .center_x(Fill);

    column![
        keycap_block,
        Space::new().height(Fill),
        container(sparkles()).center_x(Fill),
        Space::new().height(18.0),
        text("Hyperkey").size(12.0).style(wordmark_text),
    ]
    .width(LEFT_COLUMN_WIDTH)
    .height(Fill)
    .align_x(Alignment::Center)
    .into()
}

/// The two four-point sparkles — a large one with a smaller companion —
/// drawn with text glyphs since the canvas feature is not enabled.
fn sparkles<'a>() -> Element<'a, Message> {
    container(
        row![
            text("✦")
                .size(110.0)
                .line_height(LineHeight::Relative(1.0))
                .style(sparkle_text),
            column![
                Space::new().height(Fill),
                text("✦")
                    .size(40.0)
                    .line_height(LineHeight::Relative(1.0))
                    .style(sparkle_text),
            ]
            .height(Fill),
        ]
        .spacing(4.0),
    )
    .width(160.0)
    .height(150.0)
    .into()
}

/// The scrollable right column: the banner, then the settings sections in
/// the prototype's order, separated by hairline dividers.
fn right_column(state: &State) -> Element<'_, Message> {
    let settings = &state.settings;
    let mut content = column![].spacing(SECTION_GAP).width(Fill);

    // Repair and fatal messages lead the settings column.
    if let Some(message) = &state.banner {
        content = content.push(banner(message));
    }

    let enable_row = row![
        photo_checkbox(
            "Enable hyper key",
            settings.enabled(),
            Message::EnabledToggled,
            focused(state, FocusTarget::EnabledToggle),
        ),
        Space::new().width(Fill),
        column![
            text(status_text(settings.enabled()))
                .size(STATUS_SIZE)
                .font(Font {
                    weight: iced::font::Weight::Semibold,
                    ..Font::DEFAULT
                })
                .style(accent_text)
                .width(Fill)
                .align_x(text::Alignment::Right),
            text(status_hint(
                settings.enabled(),
                state.engine.status(),
                settings.trigger().display_label().as_ref(),
                settings.output_modifiers(),
                state.engine.status_error().as_deref(),
            ))
            .size(HINT_SIZE)
            .style(secondary_text)
            .width(Fill)
            .align_x(text::Alignment::Right)
            .line_height(LineHeight::Relative(1.3))
            .wrapping(Wrapping::Word),
        ]
        .spacing(4.0)
        .width(STATUS_WIDTH),
    ]
    .align_y(Alignment::Center);

    content = content.push(enable_row).push(divider());

    // The dropdown-looking field is the rebind control: clicking it or
    // pressing Enter while focused starts key capture.
    let field_label = if state.capturing_trigger {
        "Press a key…".to_string()
    } else {
        settings.trigger().display_label().to_string()
    };
    let field_focused = focused(state, FocusTarget::RebindTrigger);
    let capturing = state.capturing_trigger;
    let field = button(
        row![
            text(field_label).size(LABEL_SIZE).width(Fill),
            text("⌄").size(LABEL_SIZE).style(secondary_text),
        ]
        .align_y(Alignment::Center),
    )
    .padding(Padding::ZERO.left(18.0).right(16.0))
    .width(FIELD_WIDTH)
    .height(FIELD_HEIGHT)
    .on_press(Message::StartTriggerCapture)
    .style(move |theme, status| field_style(theme, status, field_focused, capturing));

    let remap_section = column![
        row![
            text("Remap physical key to hyper key:")
                .size(LABEL_SIZE)
                .width(Fill),
            field,
        ]
        .align_y(Alignment::Center),
        text(state.trigger_hint)
            .size(HINT_SIZE)
            .style(if state.capturing_trigger {
                accent_text
            } else {
                secondary_text
            })
            .width(Fill)
            .align_x(text::Alignment::Right),
    ]
    .spacing(8.0);

    content = content.push(remap_section).push(divider());

    let modifiers_section = column![
        text("Apply hyper key modifiers to keypress events and:").size(LABEL_SIZE),
        container(
            row![
                modifier_checkbox(state, OutputModifier::Control, "Ctrl", FocusTarget::ModifierControl),
                modifier_checkbox(state, OutputModifier::Alt, "Alt", FocusTarget::ModifierAlt),
                modifier_checkbox(state, OutputModifier::Shift, "Shift", FocusTarget::ModifierShift),
            ]
            .spacing(28.0),
        )
        .padding(Padding::ZERO.top(14.0)),
        container(text(state.modifier_hint).size(12.0).style(secondary_text))
            .padding(Padding::ZERO.top(10.0)),
    ];

    content = content.push(modifiers_section).push(divider());

    let startup_section = row![
        column![
            photo_checkbox(
                "Open on login",
                settings.launch_at_startup(),
                Message::LaunchAtStartupToggled,
                focused(state, FocusTarget::LaunchAtStartup),
            ),
            container(photo_checkbox(
                "Start in tray",
                settings.launch_to_tray(),
                Message::LaunchToTrayToggled,
                focused(state, FocusTarget::LaunchToTray),
            ))
            .padding(Padding::ZERO.top(14.0)),
            container(
                text("When hidden, reopen Hyperkey from the system tray.")
                    .size(CAPTION_SIZE)
                    .style(secondary_text),
            )
            .padding(Padding::ZERO.top(8.0).left(38.0)),
        ]
        .width(Fill),
        text(version_label()).size(LABEL_SIZE).style(secondary_text),
    ]
    .align_y(Alignment::Start);

    content = content.push(startup_section).push(divider());

    content = content.push(diagnostics_row(state));

    // The prototype leaves 24px below the last section inside the scroll
    // area; the column spacing above covers 16 of it.
    content = content.push(Space::new().height(8.0));

    container(scrollable(content))
        .width(Fill)
        .height(Fill)
        .into()
}

/// The diagnostics row: the status column on the left, the three action
/// buttons on the right.
fn diagnostics_row(state: &State) -> Element<'_, Message> {
    let mut diagnostics = column![
        text("Diagnostics").size(CAPTION_SIZE).style(secondary_text),
        text(engine_status_label(state.engine.status()))
            .size(STATUS_SIZE)
            .font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            })
            .style(accent_text),
    ]
    .spacing(4.0)
    .width(Fill);

    // The C# DiagnosticMessage slot reports the startup registration error
    // first, then falls through to the engine lifecycle message.
    let diagnostic = state
        .startup_error
        .as_ref()
        .or(state.diagnostic_message.as_ref());
    if let Some(message) = diagnostic {
        diagnostics =
            diagnostics.push(text(message.clone()).size(CAPTION_SIZE).style(secondary_text));
    }
    if let Some(error) = state.engine.status_error() {
        diagnostics = diagnostics.push(text(error).size(CAPTION_SIZE).style(danger_text));
    }
    if let Some(message) = state.engine.last_recovery_message() {
        diagnostics =
            diagnostics.push(text(message).size(CAPTION_SIZE).style(secondary_text));
    }
    if let Some(status) = &state.copy_status {
        diagnostics = diagnostics.push(text(status.clone()).size(CAPTION_SIZE).style(secondary_text));
    }
    diagnostics = diagnostics.push(
        text("Elevated applications are unsupported in this MVP. \
              Other keyboard remappers may also interfere.")
            .size(CAPTION_SIZE)
            .style(secondary_text)
            .line_height(LineHeight::Relative(1.3))
            .wrapping(Wrapping::Word),
    );

    row![
        diagnostics,
        row![
            photo_button(
                "Restart hook",
                Message::RestartHook,
                focused(state, FocusTarget::RestartHook),
            ),
            photo_button(
                "Emergency disable",
                Message::EmergencyDisable,
                focused(state, FocusTarget::EmergencyDisable),
            ),
            photo_button(
                "Copy details",
                Message::CopyDetails,
                focused(state, FocusTarget::CopyDetails),
            ),
        ]
        .spacing(8.0),
    ]
    .spacing(20.0)
    .align_y(Alignment::Start)
    .into()
}

/// A 26px checkbox with the prototype's rounded box, accent check fill,
/// and accent focus ring on the box.
fn photo_checkbox<'a>(
    label: &'a str,
    checked: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
    is_focused: bool,
) -> Element<'a, Message> {
    checkbox(checked)
        .label(label)
        .on_toggle(on_toggle)
        .size(CHECKBOX_SIZE)
        .spacing(CHECKBOX_SPACING)
        .text_size(LABEL_SIZE)
        .style(move |theme, status| photo_checkbox_style(theme, status, is_focused))
        .into()
}

fn modifier_checkbox<'a>(
    state: &State,
    modifier: OutputModifier,
    label: &'a str,
    focus: FocusTarget,
) -> Element<'a, Message> {
    let checked = state.settings.output_modifiers().contains(&modifier);
    photo_checkbox(
        label,
        checked,
        move |value| Message::ModifierToggled(modifier, value),
        focused(state, focus),
    )
}

/// The prototype's standard button: control fill, accent on hover, the
/// deeper accent when pressed, and a 2px accent focus ring.
fn photo_button<'a>(
    label: &'a str,
    message: Message,
    is_focused: bool,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(BUTTON_SIZE)
            .align_x(text::Alignment::Center),
    )
    .padding([8.0, 14.0])
    .on_press(message)
    .style(move |theme, status| photo_button_style(theme, status, is_focused))
    .into()
}

/// The hairline between settings sections.
fn divider<'a>() -> Element<'a, Message> {
    rule::horizontal(1.0)
        .style(|theme| rule::Style {
            color: divider_color(theme),
            radius: Radius::from(0.0),
            fill_mode: rule::FillMode::Full,
            snap: true,
        })
        .into()
}

fn banner<'a>(message: &'a str) -> Element<'a, Message> {
    container(
        text(message)
            .size(CAPTION_SIZE)
            .line_height(LineHeight::Relative(1.3))
            .wrapping(Wrapping::Word),
    )
    .padding(14.0)
    .width(Fill)
    .style(banner_style)
    .into()
}

fn focused(state: &State, target: FocusTarget) -> bool {
    state.focus == Some(target)
}

/// The status line next to "Enable hyper key", matching the C#
/// `StatusText` assignment.
fn status_text(enabled: bool) -> &'static str {
    if enabled { "Hyperkey is on" } else { "Hyperkey is off" }
}

/// The secondary hint under the status line — a port of the C#
/// `GetStatusDescription`.
fn status_hint(
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

/// Modifier labels in the fixed Control, Alt, Shift order — the same order
/// the C# window writes them to settings.
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

fn modifier_label(modifier: &OutputModifier) -> &'static str {
    match modifier {
        OutputModifier::Control => "Ctrl",
        OutputModifier::Alt => "Alt",
        OutputModifier::Shift => "Shift",
    }
}

fn engine_status_label(status: InputEngineStatus) -> &'static str {
    match status {
        InputEngineStatus::Running => "Running",
        InputEngineStatus::Stopped => "Stopped",
        InputEngineStatus::Starting => "Starting",
        InputEngineStatus::Stopping => "Stopping",
        // Matches the C# wording: failures surface as "Needs attention".
        InputEngineStatus::Failed => "Needs attention",
    }
}

/// The keycap caption is the trigger label lowercased, like the C#
/// `KeycapLabel` (`GetTriggerLabel(...).ToLowerInvariant()`).
fn keycap_label(display_label: &str) -> String {
    display_label.to_lowercase()
}

fn version_label() -> String {
    format!("v{}", env!("CARGO_PKG_VERSION"))
}

// ---- styles -------------------------------------------------------------

/// Whether the active theme is the dark design palette.
fn is_dark(theme: &Theme) -> bool {
    let c = theme.palette().background;
    0.2126 * c.r + 0.7152 * c.g + 0.0722 * c.b < 0.5
}

fn control_fill(theme: &Theme) -> Color {
    if is_dark(theme) {
        palette::CONTROL
    } else {
        theme.extended_palette().background.weak.color
    }
}

fn control_hover(theme: &Theme) -> Color {
    if is_dark(theme) {
        palette::CONTROL_HOVER
    } else {
        theme.extended_palette().background.strong.color
    }
}

fn divider_color(theme: &Theme) -> Color {
    if is_dark(theme) {
        palette::CONTROL_HOVER
    } else {
        theme.extended_palette().background.strong.color
    }
}

fn secondary_color(theme: &Theme) -> Color {
    if is_dark(theme) {
        palette::SECONDARY_TEXT
    } else {
        muted(theme)
    }
}

fn accent(theme: &Theme) -> Color {
    theme.palette().primary
}

fn accent_strong(theme: &Theme) -> Color {
    if is_dark(theme) {
        palette::ACCENT_STRONG
    } else {
        theme.extended_palette().primary.strong.color
    }
}

/// Text and glyphs drawn on the accent fill.
fn on_accent(theme: &Theme) -> Color {
    if is_dark(theme) {
        palette::ACCENT_TEXT
    } else {
        theme.extended_palette().primary.base.text
    }
}

fn muted(theme: &Theme) -> Color {
    let mut color = theme.palette().text;
    color.a = 0.65;
    color
}

fn secondary_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(secondary_color(theme)),
    }
}

fn accent_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(accent(theme)),
    }
}

fn danger_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().danger),
    }
}

/// The wordmark and sparkles use a fainter secondary tone.
fn wordmark_text(theme: &Theme) -> text::Style {
    let mut color = secondary_color(theme);
    color.a *= 0.75;
    text::Style { color: Some(color) }
}

fn sparkle_text(theme: &Theme) -> text::Style {
    let mut color = secondary_color(theme);
    color.a *= 0.7;
    text::Style { color: Some(color) }
}

/// The keycap illustration: a rounded-square outline in the secondary tone.
fn keycap_style(theme: &Theme) -> container::Style {
    container::Style {
        border: Border {
            color: secondary_color(theme),
            width: 2.0,
            radius: Radius::from(16.0),
        },
        ..Default::default()
    }
}

/// The small filled dot near the top of the keycap.
fn dot_style(theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(secondary_color(theme))),
        border: Border {
            radius: Radius::from(5.0),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// The prototype's checkbox: 26px rounded box, control fill, accent when
/// checked, stronger accent on checked-hover, and a 2px accent ring on
/// keyboard focus.
fn photo_checkbox_style(
    theme: &Theme,
    status: checkbox::Status,
    is_focused: bool,
) -> checkbox::Style {
    let (fill, edge) = match status {
        checkbox::Status::Active { is_checked: true } => (accent(theme), accent(theme)),
        checkbox::Status::Active { is_checked: false } => {
            (control_fill(theme), control_fill(theme))
        }
        checkbox::Status::Hovered { is_checked: true } => {
            (accent_strong(theme), accent_strong(theme))
        }
        checkbox::Status::Hovered { is_checked: false } => {
            (control_hover(theme), control_hover(theme))
        }
        checkbox::Status::Disabled { is_checked: true } => {
            let mut color = accent(theme);
            color.a *= 0.45;
            (color, color)
        }
        checkbox::Status::Disabled { is_checked: false } => {
            let mut color = control_fill(theme);
            color.a *= 0.45;
            (color, color)
        }
    };

    let mut border = Border {
        color: edge,
        width: 1.0,
        radius: Radius::from(7.0),
    };
    if is_focused {
        border.color = accent(theme);
        border.width = 2.0;
    }

    checkbox::Style {
        background: Background::Color(fill),
        icon_color: on_accent(theme),
        border,
        text_color: Some(theme.palette().text),
    }
}

/// The rebind field, styled like the prototype's combo box: control fill,
/// rounded corners, hover fill, and an accent border while capturing or
/// keyboard-focused.
fn field_style(
    theme: &Theme,
    status: button::Status,
    is_focused: bool,
    capturing: bool,
) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(control_fill(theme))),
        text_color: theme.palette().text,
        border: Border {
            color: control_fill(theme),
            width: 1.0,
            radius: Radius::from(8.0),
        },
        ..Default::default()
    };
    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(control_hover(theme)));
            style.border.color = control_hover(theme);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(accent_strong(theme)));
            style.border.color = accent_strong(theme);
            style.text_color = on_accent(theme);
        }
        button::Status::Disabled => {
            let mut color = control_fill(theme);
            color.a *= 0.45;
            style.background = Some(Background::Color(color));
            style.text_color = muted(theme);
        }
        button::Status::Active => {}
    }
    if capturing {
        style.border.color = accent(theme);
    }
    if is_focused {
        style.border.color = accent(theme);
        style.border.width = 2.0;
    }
    style
}

/// The prototype's standard button style.
fn photo_button_style(
    theme: &Theme,
    status: button::Status,
    is_focused: bool,
) -> button::Style {
    let mut style = button::Style {
        background: Some(Background::Color(control_fill(theme))),
        text_color: theme.palette().text,
        border: Border {
            color: divider_color(theme),
            width: 1.0,
            radius: Radius::from(8.0),
        },
        ..Default::default()
    };
    match status {
        button::Status::Hovered => {
            style.background = Some(Background::Color(accent(theme)));
            style.border.color = accent(theme);
            style.text_color = on_accent(theme);
        }
        button::Status::Pressed => {
            style.background = Some(Background::Color(accent_strong(theme)));
            style.border.color = accent_strong(theme);
            style.text_color = on_accent(theme);
        }
        button::Status::Disabled => {
            let mut color = control_fill(theme);
            color.a *= 0.45;
            style.background = Some(Background::Color(color));
            style.text_color = muted(theme);
        }
        button::Status::Active => {}
    }
    if is_focused {
        style.border.color = accent(theme);
        style.border.width = 2.0;
    }
    style
}

/// The caution banner behind recovery and repair messages.
fn banner_style(theme: &Theme) -> container::Style {
    let warning = theme.palette().warning;
    container::Style {
        background: Some(Background::Color(Color {
            a: 0.12,
            ..warning
        })),
        border: Border {
            color: Color { a: 0.6, ..warning },
            width: 1.0,
            radius: Radius::from(8.0),
        },
        text_color: Some(theme.palette().text),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyperkey_core::keys::TriggerKey;

    const ALL_MODIFIERS: [OutputModifier; 3] = [
        OutputModifier::Control,
        OutputModifier::Alt,
        OutputModifier::Shift,
    ];

    #[test]
    fn status_text_follows_the_enabled_flag() {
        assert_eq!(status_text(true), "Hyperkey is on");
        assert_eq!(status_text(false), "Hyperkey is off");
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
            status_hint(true, InputEngineStatus::Starting, "Caps Lock", &ALL_MODIFIERS, None),
            "Starting the keyboard hook..."
        );
        assert_eq!(
            status_hint(true, InputEngineStatus::Stopping, "Caps Lock", &ALL_MODIFIERS, None),
            "Stopping the keyboard hook..."
        );
        assert_eq!(
            status_hint(true, InputEngineStatus::Stopped, "Caps Lock", &ALL_MODIFIERS, None),
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
            status_hint(true, InputEngineStatus::Failed, "Caps Lock", &ALL_MODIFIERS, None),
            "The keyboard hook is unavailable. No error details were reported."
        );
    }

    #[test]
    fn keycap_label_lowercases_the_trigger_display_label() {
        assert_eq!(
            keycap_label(TriggerKey::CAPS_LOCK.display_label().as_ref()),
            "caps lock"
        );
        assert_eq!(
            keycap_label(TriggerKey::SCROLL_LOCK.display_label().as_ref()),
            "scroll lock"
        );
        assert_eq!(keycap_label("F13"), "f13");
    }

    #[test]
    fn version_label_is_v_plus_the_crate_version() {
        assert_eq!(version_label(), format!("v{}", env!("CARGO_PKG_VERSION")));
        assert!(version_label().starts_with('v'));
    }
}
