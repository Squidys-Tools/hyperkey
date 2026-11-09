//! Reusable settings-window pieces. Each interactive piece is one focus stop
//! with an accent focus ring; Enter and Space activate it through GPUI's
//! built-in keyboard click, so a single `on_click` covers mouse and keyboard.
use super::theme::{CARD_RADIUS, CONTROL_RADIUS, Theme, space};
use crate::copy::Badge;
use gpui::{
    AnyElement, App, ClickEvent, Div, ElementId, FocusHandle, FontWeight, Hsla, IntoElement,
    ParentElement, Pixels, RenderOnce, SharedString, Stateful, Styled, Window, div, prelude::*, px,
    transparent_black,
};

pub type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App) + 'static>;

/// A focusable, clickable element whose transparent 2px border becomes the
/// accent focus ring. `radius` is the visible control's radius.
fn focus_ring(
    id: impl Into<ElementId>,
    focus: &FocusHandle,
    theme: &Theme,
    radius: Pixels,
    on_click: ClickHandler,
) -> Stateful<Div> {
    let ring = theme.accent;
    div()
        .id(id)
        .track_focus(focus)
        .cursor_pointer()
        .p(px(1.0))
        .border_2()
        .border_color(transparent_black())
        .rounded(radius + px(3.0))
        .focus(move |style| style.border_color(ring))
        .on_click(on_click)
}

/// A small filled circle, used for status indicators.
pub fn status_dot(color: Hsla) -> Div {
    div().size(px(8.0)).flex_none().rounded_full().bg(color)
}

/// On/off switch.
#[derive(IntoElement)]
pub struct Switch {
    id: ElementId,
    theme: Theme,
    on: bool,
    focus: FocusHandle,
    on_click: ClickHandler,
}

impl Switch {
    pub fn new(
        id: impl Into<ElementId>,
        theme: Theme,
        on: bool,
        focus: &FocusHandle,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            on,
            focus: focus.clone(),
            on_click: Box::new(on_click),
        }
    }
}

impl RenderOnce for Switch {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let (base, hover, active, knob) = if self.on {
            (
                theme.accent,
                theme.accent_hover,
                theme.accent_active,
                theme.on_accent,
            )
        } else {
            (
                theme.control,
                theme.control_hover,
                theme.control_active,
                theme.secondary,
            )
        };

        let track = div()
            .id("visual")
            .w(px(40.0))
            .h(px(22.0))
            .px(px(3.0))
            .flex()
            .items_center()
            .rounded_full()
            .bg(base)
            .hover(move |style| style.bg(hover))
            .active(move |style| style.bg(active))
            .when(self.on, |track| track.justify_end())
            .child(div().size(px(16.0)).rounded_full().bg(knob));

        focus_ring(self.id, &self.focus, &theme, px(11.0), self.on_click).child(track)
    }
}

/// A selectable pill, used for the Ctrl / Alt / Shift modifiers.
#[derive(IntoElement)]
pub struct ChipToggle {
    id: ElementId,
    theme: Theme,
    label: SharedString,
    selected: bool,
    focus: FocusHandle,
    on_click: ClickHandler,
}

impl ChipToggle {
    pub fn new(
        id: impl Into<ElementId>,
        theme: Theme,
        label: impl Into<SharedString>,
        selected: bool,
        focus: &FocusHandle,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            label: label.into(),
            selected,
            focus: focus.clone(),
            on_click: Box::new(on_click),
        }
    }
}

impl RenderOnce for ChipToggle {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let (base, hover, active, text) = if self.selected {
            (
                theme.accent,
                theme.accent_hover,
                theme.accent_active,
                theme.on_accent,
            )
        } else {
            (
                theme.control,
                theme.control_hover,
                theme.control_active,
                theme.text,
            )
        };

        let chip = div()
            .id("visual")
            .min_w(px(52.0))
            .h(px(30.0))
            .px(space::MD)
            .flex()
            .items_center()
            .justify_center()
            .rounded(CONTROL_RADIUS)
            .bg(base)
            .text_color(text)
            .font_weight(FontWeight::MEDIUM)
            .hover(move |style| style.bg(hover))
            .active(move |style| style.bg(active))
            .child(self.label);

        focus_ring(self.id, &self.focus, &theme, CONTROL_RADIUS, self.on_click).child(chip)
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum ButtonKind {
    Default,
    Danger,
}

/// A text button, optionally in the danger color.
#[derive(IntoElement)]
pub struct Button {
    id: ElementId,
    theme: Theme,
    label: SharedString,
    kind: ButtonKind,
    focus: FocusHandle,
    on_click: ClickHandler,
}

impl Button {
    pub fn new(
        id: impl Into<ElementId>,
        theme: Theme,
        label: impl Into<SharedString>,
        focus: &FocusHandle,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            label: label.into(),
            kind: ButtonKind::Default,
            focus: focus.clone(),
            on_click: Box::new(on_click),
        }
    }

    pub fn kind(mut self, kind: ButtonKind) -> Self {
        self.kind = kind;
        self
    }
}

impl RenderOnce for Button {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let (text, hover, active) = match self.kind {
            ButtonKind::Default => (theme.text, theme.control_hover, theme.control_active),
            ButtonKind::Danger => (
                theme.danger,
                theme.danger.opacity(0.18),
                theme.danger.opacity(0.28),
            ),
        };

        let button = div()
            .id("visual")
            .h(px(32.0))
            .px(space::LG)
            .flex()
            .items_center()
            .justify_center()
            .rounded(CONTROL_RADIUS)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.control)
            .text_color(text)
            .whitespace_nowrap()
            .hover(move |style| style.bg(hover))
            .active(move |style| style.bg(active))
            .child(self.label);

        focus_ring(self.id, &self.focus, &theme, CONTROL_RADIUS, self.on_click).child(button)
    }
}

/// The trigger-key control: the key's name drawn as a keycap, with an accent
/// border while it waits for a key press.
#[derive(IntoElement)]
pub struct KeycapButton {
    id: ElementId,
    theme: Theme,
    label: SharedString,
    capturing: bool,
    focus: FocusHandle,
    on_click: ClickHandler,
}

impl KeycapButton {
    pub fn new(
        id: impl Into<ElementId>,
        theme: Theme,
        label: impl Into<SharedString>,
        capturing: bool,
        focus: &FocusHandle,
        on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        Self {
            id: id.into(),
            theme,
            label: label.into(),
            capturing,
            focus: focus.clone(),
            on_click: Box::new(on_click),
        }
    }
}

impl RenderOnce for KeycapButton {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let (edge, text) = if self.capturing {
            (theme.accent, theme.accent)
        } else {
            (theme.hairline, theme.text)
        };

        let cap = div()
            .id("visual")
            .min_w(px(112.0))
            .h(px(34.0))
            .px(space::LG)
            .flex()
            .items_center()
            .justify_center()
            .rounded(CONTROL_RADIUS)
            .border_1()
            .border_b_2()
            .border_color(edge)
            .bg(theme.control)
            .text_color(text)
            .font_weight(FontWeight::SEMIBOLD)
            .whitespace_nowrap()
            .hover(move |style| style.bg(theme.control_hover))
            .active(move |style| style.bg(theme.control_active))
            .child(self.label);

        focus_ring(self.id, &self.focus, &theme, CONTROL_RADIUS, self.on_click).child(cap)
    }
}

/// A titled group: a small caption above a rounded card whose rows are
/// separated by hairlines.
#[derive(IntoElement)]
pub struct SectionCard {
    theme: Theme,
    title: SharedString,
    rows: Vec<AnyElement>,
}

impl SectionCard {
    pub fn new(theme: Theme, title: impl Into<SharedString>) -> Self {
        Self {
            theme,
            title: title.into(),
            rows: Vec::new(),
        }
    }
}

impl ParentElement for SectionCard {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.rows.extend(elements);
    }
}

impl RenderOnce for SectionCard {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let mut card = div()
            .flex()
            .flex_col()
            .rounded(CARD_RADIUS)
            .border_1()
            .border_color(theme.hairline)
            .bg(theme.card);

        for (index, row) in self.rows.into_iter().enumerate() {
            if index > 0 {
                card = card.child(div().h(px(1.0)).mx(space::LG).bg(theme.hairline));
            }
            card = card.child(row);
        }

        div()
            .flex()
            .flex_col()
            .gap(space::SM)
            .child(
                div()
                    .px(space::XS)
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(theme.secondary)
                    .child(self.title),
            )
            .child(card)
    }
}

/// One row in a card: a title and optional description on the left, a
/// control (or any element) on the right.
#[derive(IntoElement)]
pub struct SettingRow {
    theme: Theme,
    title: SharedString,
    description: Option<SharedString>,
    description_color: Option<Hsla>,
    trailing: Option<AnyElement>,
}

impl SettingRow {
    pub fn new(theme: Theme, title: impl Into<SharedString>) -> Self {
        Self {
            theme,
            title: title.into(),
            description: None,
            description_color: None,
            trailing: None,
        }
    }

    pub fn description(mut self, description: impl Into<SharedString>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Overrides the description's default secondary color.
    pub fn description_color(mut self, color: Hsla) -> Self {
        self.description_color = Some(color);
        self
    }

    pub fn trailing(mut self, element: impl IntoElement) -> Self {
        self.trailing = Some(element.into_any_element());
        self
    }
}

impl RenderOnce for SettingRow {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let description_color = self.description_color.unwrap_or(theme.secondary);

        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(space::LG)
            .px(space::LG)
            .py(space::MD)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(div().text_color(theme.text).child(self.title))
                    .when_some(self.description, |column, description| {
                        column.child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(17.0))
                                .text_color(description_color)
                                .child(description),
                        )
                    }),
            )
            .children(
                self.trailing
                    .map(|trailing| div().flex_none().child(trailing)),
            )
    }
}

/// A warning strip for settings load/save problems.
#[derive(IntoElement)]
pub struct Banner {
    theme: Theme,
    message: SharedString,
}

impl Banner {
    pub fn new(theme: Theme, message: impl Into<SharedString>) -> Self {
        Self {
            theme,
            message: message.into(),
        }
    }
}

impl RenderOnce for Banner {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        div()
            .flex()
            .gap(space::MD)
            .p(space::MD)
            .rounded(CARD_RADIUS)
            .border_1()
            .border_color(theme.warning.opacity(0.55))
            .bg(theme.warning.opacity(0.12))
            .text_size(px(13.0))
            .line_height(px(18.0))
            .text_color(theme.text)
            .child(div().mt(px(5.0)).child(status_dot(theme.warning)))
            .child(div().flex_1().min_w_0().child(self.message))
    }
}

/// The header pill summarising whether Hyperkey is working.
#[derive(IntoElement)]
pub struct StatusBadge {
    theme: Theme,
    badge: Badge,
}

impl StatusBadge {
    pub fn new(theme: Theme, badge: Badge) -> Self {
        Self { theme, badge }
    }
}

impl RenderOnce for StatusBadge {
    fn render(self, _window: &mut Window, _cx: &mut App) -> impl IntoElement {
        let theme = self.theme;
        let tone = match self.badge {
            Badge::On => theme.success,
            Badge::Off => theme.secondary,
            Badge::Starting => theme.warning,
            Badge::NeedsAttention => theme.danger,
        };

        div()
            .h(px(22.0))
            .px(space::MD)
            .flex()
            .items_center()
            .gap(space::SM)
            .rounded_full()
            .bg(tone.opacity(0.14))
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(tone)
            .child(status_dot(tone))
            .child(self.badge.label())
    }
}
