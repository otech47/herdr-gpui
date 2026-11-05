//! The status bar at the foot of the window body: connection state, usage,
//! listening ports, system load and the chrome buttons.

use super::STATUS_GLYPH;
use crate::{APP_VERSION, HerdrWindow, state::ConnectionStatus};
use gpui::{prelude::*, *};
use std::time::Duration;

const ISSUE_URL: &str =
    "https://github.com/penso/herdr-gpui/issues/new?template=bug_report.yml&version=";

impl HerdrWindow {
    pub(super) fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let status = (!matches!(self.live.status, ConnectionStatus::Connected)
            || self.local_error.is_some()
            || self.live.error.is_some())
        .then(|| self.live.status_text(self.local_error.as_deref()));
        div()
            .id("connection-status")
            .debug_selector(|| "connection-status".into())
            .flex()
            .flex_none()
            .h(px((self.config.ui.size * 1.5 + 4.).max(22.)))
            .overflow_hidden()
            .items_center()
            .gap(px(6.))
            .px_3()
            .bg(rgb(self.theme.surface))
            .text_color(rgb(self.theme.foreground))
            .children(self.render_usage(cx))
            .when_some(
                self.prefix_armed
                    .then(|| self.keymap().prefix_label())
                    .flatten(),
                |bar, prefix| {
                    bar.child(
                        div()
                            .debug_selector(|| "prefix-armed".into())
                            .flex_none()
                            .px(px(6.))
                            .rounded(px(crate::config::corners::SMALL))
                            .bg(rgb(self.theme.active))
                            .child(prefix),
                    )
                },
            )
            // The mode has no control on screen, so it says how it works.
            .when(self.resize_mode, |bar| {
                bar.child(
                    div()
                        .debug_selector(|| "resize-mode".into())
                        .flex_none()
                        .px(px(6.))
                        .rounded(px(crate::config::corners::SMALL))
                        .bg(rgb(self.theme.active))
                        .child("Resize"),
                )
                .child(
                    div()
                        .flex_none()
                        .text_color(rgb(self.theme.muted))
                        .child("h j k l or arrows resize, Esc ends"),
                )
            })
            .when(!self.live.status.is_connected(), |bar| {
                bar.child(
                    if matches!(self.live.status, ConnectionStatus::StartingDaemon) {
                        div()
                            .size(px(8.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(self.theme.ink(self.theme.palette[3])))
                            .with_animation(
                                "daemon-starting-loader",
                                Animation::new(Duration::from_secs(1)).repeat(),
                                |dot, delta| {
                                    dot.opacity(0.3 + 0.7 * (delta * std::f32::consts::PI).sin())
                                },
                            )
                            .into_any_element()
                    } else {
                        div()
                            .size(px(6.))
                            .flex_none()
                            .rounded_full()
                            .bg(rgb(self.theme.ink(self.theme.palette[1])))
                            .into_any_element()
                    },
                )
            })
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .when_some(status, |row, status| {
                        row.child(
                            div()
                                .debug_selector(|| "connection-message".into())
                                .child(status),
                        )
                    }),
            )
            .children(self.render_listening_ports(cx))
            .children(self.render_system_load(cx))
            .when(crate::caffeine::SUPPORTED, |bar| {
                let awake = crate::caffeine::active(cx);
                let (foreground, surface) = (self.theme.foreground, self.theme.surface);
                bar.child(
                    div()
                        .id("status-caffeine")
                        .debug_selector(|| "status-caffeine".into())
                        .flex_none()
                        .flex()
                        .items_center()
                        .px_2()
                        .h_full()
                        .cursor_pointer()
                        .hover(|s| s.bg(rgb(self.theme.active)))
                        .child(
                            svg()
                                .path(if awake {
                                    "icons/coffee-full.svg"
                                } else {
                                    "icons/coffee.svg"
                                })
                                .size(px(STATUS_GLYPH))
                                .flex_none()
                                .text_color(rgb(if awake {
                                    self.theme.primary()
                                } else {
                                    self.theme.foreground
                                })),
                        )
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::usage::Hint {
                                text: if awake {
                                    "Keeping the display awake".into()
                                } else {
                                    "Keep the display awake".into()
                                },
                                foreground,
                                surface,
                            })
                            .into()
                        })
                        .on_click(cx.listener(|this, _, _, cx| {
                            if let Err(error) = crate::caffeine::toggle(cx) {
                                this.show_flash(
                                    super::super::Flash::warning(error.to_string()),
                                    cx,
                                );
                            }
                        })),
                )
            })
            .child(
                div()
                    .id("status-theme")
                    .debug_selector(|| "status-theme".into())
                    .flex_shrink_1()
                    .min_w(px(33.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px_2()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        svg()
                            .path("icons/theme.svg")
                            .size(px(STATUS_GLYPH))
                            .flex_none()
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .child(div().min_w_0().truncate().child("Theme"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_theme_picker(window, cx);
                    })),
            )
            .child(
                div()
                    .id("status-keybinds")
                    .debug_selector(|| "status-keybinds".into())
                    .flex_shrink_1()
                    .min_w(px(33.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px_2()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        svg()
                            .path("icons/keyboard.svg")
                            .size(px(STATUS_GLYPH))
                            .flex_none()
                            .text_color(rgb(self.theme.foreground)),
                    )
                    .child(div().min_w_0().truncate().child("Shortcuts"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_keybinds(window, cx);
                    })),
            )
            .child(
                div()
                    .id("report-issue")
                    .debug_selector(|| "report-issue".into())
                    .flex_shrink_1()
                    .min_w(px(33.))
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .px_2()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    .child(
                        div()
                            .size(px(12.))
                            .flex_none()
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded_full()
                            .border_1()
                            .border_color(rgb(self.theme.foreground))
                            .child(
                                div()
                                    .size(px(3.))
                                    .rounded_full()
                                    .bg(rgb(self.theme.foreground)),
                            ),
                    )
                    .child(div().min_w_0().truncate().child("Report issue"))
                    .on_click(|_, _, cx| {
                        cx.open_url(&format!("{ISSUE_URL}{}", APP_VERSION.replace('+', "%2B"),));
                    }),
            )
            .child(
                div()
                    .id("status-version")
                    .debug_selector(|| "status-version".into())
                    .flex_none()
                    .whitespace_nowrap()
                    .cursor_pointer()
                    .hover(|s| s.bg(rgb(self.theme.active)))
                    // A waiting update is the one status here worth
                    // interrupting for, so it takes the accent color
                    // the rest of the chrome reserves for chosen rows.
                    .text_color(rgb(if self.updater.update_available() {
                        self.theme.primary()
                    } else {
                        self.theme.muted
                    }))
                    .child(if self.updater.update_available() {
                        "Update available"
                    } else {
                        APP_VERSION
                    })
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_app_update(false, window, cx);
                    })),
            )
    }
}
