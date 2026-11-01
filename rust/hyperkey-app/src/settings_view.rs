//! The settings window, ported from the C# `MainWindow.xaml`: a single
//! vertically scrollable column — header with the enabled toggle, trigger
//! key, output modifiers, startup toggles, the recovery banner, diagnostics,
//! and the about footer. Toolkit controls only; colors come from the iced
//! palette so light and dark themes both stay readable.
use crate::{FocusTarget, Message, State};
use hyperkey_core::settings::OutputModifier;
use hyperkey_input::engine::InputEngineStatus;
use iced::border::Radius;
use iced::widget::text::{LineHeight, Wrapping};
use iced::widget::{button, column, container, row, rule, scrollable, text, toggler};
use iced::{Alignment, Background, Border, Color, Element, Fill, Font, Theme};

const SECTION_SPACING: f32 = 16.0;
const CONTENT_PADDING: f32 = 24.0;
const CAPTION_SIZE: f32 = 13.0;
const HEADING_SIZE: f32 = 12.0;

pub fn view(state: &State) -> Element<'_, Message> {
    let settings = &state.settings;
    let enabled = settings.enabled();

    let header = row![
        column![
            text("Hyperkey").size(28).font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            }),
            text("One key becomes a new modifier layer.").style(caption_text),
        ]
        .spacing(2)
        .width(Fill),
        column![
            focus_wrapped(
                toggler(enabled)
                    .on_toggle(Message::EnabledToggled)
                    .size(22.0),
                focused(state, FocusTarget::EnabledToggle),
            ),
            text(if enabled { "On" } else { "Off" })
                .size(CAPTION_SIZE)
                .style(caption_text)
                .align_x(text::Alignment::Center)
                .width(Fill),
        ]
        .spacing(6)
        .align_x(Alignment::Center),
    ]
    .align_y(Alignment::Center);

    let trigger_label = if state.capturing_trigger {
        "Press a key…".to_string()
    } else {
        settings.trigger().display_label().to_string()
    };
    let bind_button = button(
        text(trigger_label)
        .size(18)
        .font(Font::MONOSPACE)
        .width(Fill)
        .align_x(text::Alignment::Center),
    )
    .padding([12, 20])
    .width(Fill)
    .on_press(Message::StartTriggerCapture)
    .style(move |theme, status| {
        bind_button_style(theme, status, focused(state, FocusTarget::RebindTrigger))
    });
    let trigger_section = section(
        "TRIGGER KEY",
        column![
            bind_button,
            text(state.trigger_hint).size(CAPTION_SIZE).style(if state
                .capturing_trigger
            {
                accent_text
            } else {
                caption_text
            }),
        ]
        .spacing(6),
    );

    let modifiers = column![
        row![
            keycap(state, OutputModifier::Control, "Ctrl", FocusTarget::ModifierControl),
            keycap(state, OutputModifier::Alt, "Alt", FocusTarget::ModifierAlt),
            keycap(state, OutputModifier::Shift, "Shift", FocusTarget::ModifierShift),
        ]
        .spacing(8),
        text(state.modifier_hint).size(CAPTION_SIZE).style(caption_text),
    ]
    .spacing(6);
    let modifier_section = section("OUTPUT MODIFIERS", modifiers);

    let startup_section = section(
        "STARTUP",
        column![
            toggle_row(
                "Launch at login",
                "Start Hyperkey in the background when you sign in.",
                settings.launch_at_startup(),
                Message::LaunchAtStartupToggled,
                focused(state, FocusTarget::LaunchAtStartup),
            ),
            toggle_row(
                "Launch to tray",
                "Keep the settings window closed when Hyperkey starts.",
                settings.launch_to_tray(),
                Message::LaunchToTrayToggled,
                focused(state, FocusTarget::LaunchToTray),
            ),
        ]
        .spacing(14),
    );

    let status_label = engine_status_label(state.engine.status());
    let mut diagnostics = column![
        row![
            text("Hook status:").size(CAPTION_SIZE).style(caption_text),
            text(status_label).size(CAPTION_SIZE).font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            }),
        ]
        .spacing(4),
    ]
    .spacing(10);

    // The C# DiagnosticMessage slot reports the startup registration error
    // first, then falls through to the engine lifecycle message.
    let diagnostic = state
        .startup_error
        .as_ref()
        .or(state.diagnostic_message.as_ref());
    if let Some(message) = diagnostic {
        diagnostics =
            diagnostics.push(text(message.clone()).size(CAPTION_SIZE).style(caption_text));
    }
    if let Some(error) = state.engine.status_error() {
        diagnostics = diagnostics.push(text(error).size(CAPTION_SIZE).style(danger_text));
    }
    if let Some(message) = state.engine.last_recovery_message() {
        diagnostics =
            diagnostics.push(text(message).size(CAPTION_SIZE).style(caption_text));
    }
    if let Some(status) = &state.copy_status {
        diagnostics = diagnostics.push(text(status.clone()).size(CAPTION_SIZE).style(caption_text));
    }

    diagnostics = diagnostics
        .push(
            text("Elevated applications are unsupported in this MVP. \
                  Other keyboard remappers may also interfere.")
                .size(CAPTION_SIZE)
                .style(caption_text),
        )
        .push(
            row![
                action_button(
                    "Restart hook",
                    Message::RestartHook,
                    focused(state, FocusTarget::RestartHook),
                    false,
                ),
                action_button(
                    "Emergency disable",
                    Message::EmergencyDisable,
                    focused(state, FocusTarget::EmergencyDisable),
                    true,
                ),
                action_button(
                    "Copy details",
                    Message::CopyDetails,
                    focused(state, FocusTarget::CopyDetails),
                    false,
                ),
            ]
            .spacing(8),
        );
    let diagnostics_section = section("DIAGNOSTICS", diagnostics);

    let about = column![
        text(format!("Version {}", env!("CARGO_PKG_VERSION")))
            .size(CAPTION_SIZE)
            .style(caption_text),
        text("The keyboard hook runs only while Hyperkey is enabled.")
            .size(CAPTION_SIZE)
            .style(caption_text),
    ]
    .spacing(4);

    let mut content = column![
        header,
        trigger_section,
        modifier_section,
        rule::horizontal(1),
        startup_section,
    ]
    .spacing(SECTION_SPACING)
    .padding(CONTENT_PADDING)
    .max_width(720);

    if let Some(message) = &state.banner {
        content = content.push(
            container(
                text(message.clone())
                    .size(CAPTION_SIZE)
                    .line_height(LineHeight::Relative(1.3))
                    .wrapping(Wrapping::Word),
            )
            .padding(14)
            .width(Fill)
            .style(banner_style),
        );
    }

    let content = content
        .push(rule::horizontal(1))
        .push(diagnostics_section)
        .push(about);

    container(scrollable(content))
        .width(Fill)
        .height(Fill)
        .into()
}

fn section<'a>(heading: &'a str, content: impl Into<Element<'a, Message>>) -> Element<'a, Message> {
    column![
        text(heading)
            .size(HEADING_SIZE)
            .font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            })
            .style(section_heading_text),
        content.into(),
    ]
    .spacing(8)
    .into()
}

fn toggle_row<'a>(
    title: &'a str,
    caption: &'a str,
    value: bool,
    on_toggle: impl Fn(bool) -> Message + 'a,
    is_focused: bool,
) -> Element<'a, Message> {
    row![
        column![
            text(title).font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            }),
            text(caption).size(CAPTION_SIZE).style(caption_text),
        ]
        .spacing(2)
        .width(Fill),
        focus_wrapped(
            toggler(value).on_toggle(on_toggle).size(20.0),
            is_focused,
        ),
    ]
    .align_y(Alignment::Center)
    .into()
}

/// The keycap-styled modifier picker: a bordered cell per modifier, filled
/// with the accent color while selected.
fn keycap<'a>(
    state: &State,
    modifier: OutputModifier,
    label: &'a str,
    focus: FocusTarget,
) -> Element<'a, Message> {
    let selected = state.settings.output_modifiers().contains(&modifier);
    let is_focused = focused(state, focus);
    button(
        text(label)
            .size(15)
            .font(Font {
                weight: iced::font::Weight::Semibold,
                ..Font::DEFAULT
            })
            .width(Fill)
            .align_x(text::Alignment::Center),
    )
    .padding([10, 0])
    .width(Fill)
    .on_press(Message::ModifierToggled(modifier, !selected))
    .style(move |theme, status| keycap_style(theme, status, selected, is_focused))
    .into()
}

fn action_button<'a>(
    label: &'a str,
    message: Message,
    is_focused: bool,
    danger: bool,
) -> Element<'a, Message> {
    button(
        text(label)
            .size(CAPTION_SIZE)
            .align_x(text::Alignment::Center),
    )
    .padding([7, 14])
    .on_press(message)
    .style(move |theme, status| action_style(theme, status, is_focused, danger))
    .into()
}

/// The focus ring: a 2px accent border around togglers, which cannot draw
/// one themselves.
fn focus_wrapped<'a>(
    content: impl Into<Element<'a, Message>>,
    is_focused: bool,
) -> Element<'a, Message> {
    container(content)
        .padding(3)
        .style(move |theme| container::Style {
            border: Border {
                color: if is_focused {
                    theme.palette().primary
                } else {
                    Color::TRANSPARENT
                },
                width: 2.0,
                radius: Radius::from(6.0),
            },
            ..Default::default()
        })
        .into()
}

fn focused(state: &State, target: FocusTarget) -> bool {
    state.focus == Some(target)
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

// ---- styles -------------------------------------------------------------

fn muted(theme: &Theme) -> Color {
    let mut color = theme.palette().text;
    color.a = 0.65;
    color
}

fn caption_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(muted(theme)),
    }
}

fn accent_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().primary),
    }
}

fn danger_text(theme: &Theme) -> text::Style {
    text::Style {
        color: Some(theme.palette().danger),
    }
}

fn section_heading_text(theme: &Theme) -> text::Style {
    let mut color = theme.palette().text;
    color.a = 0.55;
    text::Style { color: Some(color) }
}

fn bind_button_style(theme: &Theme, status: button::Status, is_focused: bool) -> button::Style {
    let palette = theme.extended_palette();
    let pair = palette.background.weak;
    let mut style = button::Style {
        background: Some(Background::Color(pair.color)),
        text_color: palette.background.base.text,
        border: Border {
            color: palette.background.strong.color,
            width: 1.0,
            radius: Radius::from(8.0),
        },
        ..Default::default()
    };
    match status {
        button::Status::Hovered | button::Status::Pressed => {
            style.background = Some(Background::Color(palette.background.strong.color));
        }
        _ => {}
    }
    if is_focused {
        style.border.color = theme.palette().primary;
        style.border.width = 2.0;
    }
    style
}

fn keycap_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
    is_focused: bool,
) -> button::Style {
    let palette = theme.extended_palette();
    let mut style = if selected {
        button::primary(theme, status)
    } else {
        let pair = palette.background.weak;
        button::Style {
            background: Some(Background::Color(pair.color)),
            text_color: palette.background.base.text,
            border: Border {
                color: palette.background.strong.color,
                width: 1.0,
                radius: Radius::from(8.0),
            },
            ..Default::default()
        }
    };
    if matches!(status, button::Status::Hovered | button::Status::Pressed) && !selected {
        style.background = Some(Background::Color(palette.background.strong.color));
    }
    style.border.radius = Radius::from(8.0);
    if is_focused {
        style.border.color = theme.palette().primary;
        style.border.width = 2.0;
    }
    style
}

fn action_style(
    theme: &Theme,
    status: button::Status,
    is_focused: bool,
    danger: bool,
) -> button::Style {
    let mut style = button::secondary(theme, status);
    style.border.radius = Radius::from(6.0);
    if danger {
        style.text_color = theme.palette().danger;
        style.border.color = theme.palette().danger;
        style.border.width = 1.0;
    }
    if is_focused {
        style.border.color = theme.palette().primary;
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

