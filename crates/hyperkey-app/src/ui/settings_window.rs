//! The settings window: one vertically scrollable column of section cards.
use super::components::{
    Banner, Button, ButtonKind, ChipToggle, KeycapButton, SectionCard, SettingRow, StatusBadge,
    Switch, status_dot,
};
use super::theme::{FONT, Theme, space};
use crate::controller::Controller;
use crate::copy::{self, Badge};
use crate::platform;
use gpui::{
    App, ClickEvent, Context, Entity, FocusHandle, FontWeight, Hsla, IntoElement, Render,
    Subscription, Window, actions, div, prelude::*, px,
};
use hyperkey_core::settings::OutputModifier;
use hyperkey_input::engine::InputEngineStatus;

actions!(hyperkey, [FocusNext, FocusPrev]);

/// One focus handle per control, numbered in visual order.
struct Focus {
    root: FocusHandle,
    enabled: FocusHandle,
    trigger: FocusHandle,
    control: FocusHandle,
    alt: FocusHandle,
    shift: FocusHandle,
    startup: FocusHandle,
    tray: FocusHandle,
    restart: FocusHandle,
    emergency: FocusHandle,
    copy: FocusHandle,
}

impl Focus {
    fn new(cx: &mut Context<SettingsWindow>) -> Self {
        let mut index = 0;
        let mut stop = || {
            index += 1;
            cx.focus_handle().tab_index(index).tab_stop(true)
        };
        Self {
            enabled: stop(),
            trigger: stop(),
            control: stop(),
            alt: stop(),
            shift: stop(),
            startup: stop(),
            tray: stop(),
            restart: stop(),
            emergency: stop(),
            copy: stop(),
            root: cx.focus_handle(),
        }
    }
}

pub struct SettingsWindow {
    controller: Entity<Controller>,
    focus: Focus,
    _subscriptions: [Subscription; 2],
}

impl SettingsWindow {
    pub fn new(
        controller: Entity<Controller>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // Focus starts on the root so no ring shows until the keyboard is
        // used; the first Tab lands on the enabled switch.
        let focus = Focus::new(cx);
        window.focus(&focus.root);

        // GPUI quits when its last window closes, so closing only hides.
        let closing = controller.clone();
        window.on_window_should_close(cx, move |window, cx| {
            closing.update(cx, |controller, cx| {
                controller.cancel_capture();
                cx.notify();
            });
            platform::window::hide(&*window);
            false
        });

        let view = cx.entity();
        let appearance = window.observe_window_appearance(move |_, cx| {
            view.update(cx, |_, cx| cx.notify());
        });
        let observed = cx.observe(&controller, |_, _, cx| cx.notify());

        Self {
            controller,
            focus,
            _subscriptions: [appearance, observed],
        }
    }

    /// Shows the hidden window again with focus back on the root. A capture
    /// still running is cancelled first: activation sends a synthetic Alt tap.
    pub fn reveal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.act(cx, Controller::cancel_capture);
        platform::window::show(&*window);
        window.activate_window();
        window.focus(&self.focus.root);
    }

    /// Runs a controller operation, then re-renders everything observing it.
    fn act(&self, cx: &mut Context<Self>, operation: impl FnOnce(&mut Controller)) {
        self.controller.update(cx, |controller, cx| {
            operation(controller);
            cx.notify();
        });
    }

    /// A click handler that runs `operation` on the controller.
    fn on_click(
        cx: &Context<Self>,
        operation: impl Fn(&mut Controller) + 'static,
    ) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &ClickEvent, _, cx| this.act(cx, &operation))
    }

    fn header(&self, theme: Theme, cx: &Context<Self>) -> impl IntoElement {
        let controller = self.controller.read(cx);
        let settings = controller.settings();
        let status = controller.engine_status();
        let hint = copy::status_hint(
            settings.enabled(),
            status,
            settings.trigger().display_label().as_ref(),
            settings.output_modifiers(),
            controller.status_error().as_deref(),
        );

        div()
            .flex()
            .flex_col()
            .gap(space::SM)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(space::MD)
                            .child(
                                div()
                                    .text_size(px(24.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child("Hyperkey"),
                            )
                            .child(StatusBadge::new(
                                theme,
                                Badge::of(settings.enabled(), status),
                            )),
                    )
                    .child(Switch::new(
                        "enabled",
                        theme,
                        settings.enabled(),
                        &self.focus.enabled,
                        Self::on_click(cx, Controller::toggle_enabled),
                    )),
            )
            .child(
                div()
                    .text_color(theme.secondary)
                    .line_height(px(20.0))
                    .child(hint),
            )
    }

    fn keyboard_card(&self, theme: Theme, cx: &Context<Self>) -> impl IntoElement {
        let controller = self.controller.read(cx);
        let capturing = controller.is_capturing();
        let key_label = if capturing {
            "Press a key…".to_string()
        } else {
            controller.settings().trigger().display_label().to_string()
        };

        let chip = |id: &'static str, modifier: OutputModifier, focus: &FocusHandle| {
            ChipToggle::new(
                id,
                theme,
                copy::modifier_label(&modifier),
                controller.has_modifier(modifier),
                focus,
                Self::on_click(cx, move |controller| {
                    let selected = !controller.has_modifier(modifier);
                    controller.toggle_modifier(modifier, selected);
                }),
            )
        };

        let trigger_row = SettingRow::new(theme, "Trigger key")
            .description(controller.trigger_hint().to_string())
            .when(capturing, |row| row.description_color(theme.accent))
            .trailing(KeycapButton::new(
                "trigger",
                theme,
                key_label,
                capturing,
                &self.focus.trigger,
                Self::on_click(cx, Controller::start_capture),
            ));

        let modifiers_row = SettingRow::new(theme, "Output modifiers")
            .description(controller.modifier_hint().to_string())
            .trailing(
                div()
                    .flex()
                    .gap(space::SM)
                    .child(chip("ctrl", OutputModifier::Control, &self.focus.control))
                    .child(chip("alt", OutputModifier::Alt, &self.focus.alt))
                    .child(chip("shift", OutputModifier::Shift, &self.focus.shift)),
            );

        SectionCard::new(theme, "Keyboard")
            .child(trigger_row)
            .child(modifiers_row)
    }

    fn startup_card(&self, theme: Theme, cx: &Context<Self>) -> impl IntoElement {
        let settings = self.controller.read(cx).settings();

        SectionCard::new(theme, "Startup")
            .child(
                SettingRow::new(theme, "Open on login").trailing(Switch::new(
                    "launch-at-startup",
                    theme,
                    settings.launch_at_startup(),
                    &self.focus.startup,
                    Self::on_click(cx, |controller| {
                        let launch = !controller.settings().launch_at_startup();
                        controller.set_launch_at_startup(launch);
                    }),
                )),
            )
            .child(
                SettingRow::new(theme, "Start in tray")
                    .description("When hidden, reopen Hyperkey from the system tray.")
                    .trailing(Switch::new(
                        "launch-to-tray",
                        theme,
                        settings.launch_to_tray(),
                        &self.focus.tray,
                        Self::on_click(cx, |controller| {
                            let to_tray = !controller.settings().launch_to_tray();
                            controller.set_launch_to_tray(to_tray);
                        }),
                    )),
            )
    }

    fn diagnostics_card(&self, theme: Theme, cx: &Context<Self>) -> impl IntoElement {
        let controller = self.controller.read(cx);
        let status = controller.engine_status();
        let dot = match status {
            InputEngineStatus::Running => theme.success,
            InputEngineStatus::Failed => theme.danger,
            InputEngineStatus::Starting | InputEngineStatus::Stopping => theme.warning,
            InputEngineStatus::Stopped => theme.secondary,
        };

        let note = |color: Hsla, text: String| {
            div()
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(color)
                .child(text)
        };

        // Status and actions come first so the buttons never move when
        // messages appear or clear below them.
        let status_row = div()
            .flex()
            .items_center()
            .gap(space::SM)
            .child(status_dot(dot))
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(copy::engine_status_label(status)),
            );

        let notes = div()
            .flex()
            .flex_col()
            .gap(space::SM)
            .px(space::LG)
            .py(space::MD)
            .children(
                controller
                    .diagnostic_message()
                    .map(|message| note(theme.secondary, message.to_string())),
            )
            .children(
                controller
                    .status_error()
                    .map(|error| note(theme.danger, error)),
            )
            .children(
                controller
                    .last_recovery_message()
                    .map(|message| note(theme.secondary, message)),
            )
            .children(
                controller
                    .copy_status()
                    .map(|status| note(theme.secondary, status.to_string())),
            )
            .child(note(
                theme.secondary,
                "Elevated applications are unsupported in this MVP. \
                 Other keyboard remappers may also interfere."
                    .to_string(),
            ));

        let actions = div()
            .flex()
            .flex_wrap()
            .gap(space::SM)
            .child(Button::new(
                "restart-hook",
                theme,
                "Restart hook",
                &self.focus.restart,
                Self::on_click(cx, Controller::restart_hook),
            ))
            .child(
                Button::new(
                    "emergency-disable",
                    theme,
                    "Emergency disable",
                    &self.focus.emergency,
                    Self::on_click(cx, Controller::emergency_disable),
                )
                .kind(ButtonKind::Danger),
            )
            .child(Button::new(
                "copy-details",
                theme,
                "Copy details",
                &self.focus.copy,
                Self::on_click(cx, Controller::copy_details),
            ));

        let summary = div()
            .flex()
            .flex_col()
            .gap(space::MD)
            .px(space::LG)
            .py(space::MD)
            .child(status_row)
            .child(actions);

        SectionCard::new(theme, "Diagnostics")
            .child(summary)
            .child(notes)
    }

    fn about_card(theme: Theme) -> impl IntoElement {
        SectionCard::new(theme, "About").child(
            SettingRow::new(theme, "Version").trailing(
                div()
                    .text_color(theme.secondary)
                    .child(copy::version_label()),
            ),
        )
    }
}

impl Render for SettingsWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = Theme::for_appearance(window.appearance());
        let banner = self.controller.read(cx).banner().map(str::to_string);

        div()
            .id("settings-root")
            .track_focus(&self.focus.root)
            .on_action(cx.listener(|_, _: &FocusNext, window, _| window.focus_next()))
            .on_action(cx.listener(|_, _: &FocusPrev, window, _| window.focus_prev()))
            .size_full()
            .flex()
            .flex_col()
            .bg(theme.canvas)
            .text_color(theme.text)
            .font_family(FONT)
            .text_size(px(14.0))
            .child(
                div()
                    .id("settings-scroll")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(space::XL)
                            .max_w(px(640.0))
                            .mx_auto()
                            .px(space::XL)
                            .py(space::XL)
                            .child(self.header(theme, cx))
                            .children(banner.map(|message| Banner::new(theme, message)))
                            .child(self.keyboard_card(theme, cx))
                            .child(self.startup_card(theme, cx))
                            .child(self.diagnostics_card(theme, cx))
                            .child(Self::about_card(theme)),
                    ),
            )
    }
}
