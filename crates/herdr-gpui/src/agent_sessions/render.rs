use super::*;
use gpui::{Div, FontWeight, div, prelude::*, px, rgb};

impl HerdrWindow {
    pub(crate) fn render_agent_sessions(&self, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &self.menu.agent_sessions else {
            return div();
        };
        let theme = &self.theme;
        let watching = picker.watch.is_some();
        let mut panel = div().flex().flex_col().size_full().min_h_0();
        let title = if watching { "Session" } else { "Open session" };
        panel =
            panel.child(
                div()
                    .p(px(16.))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .child(
                                div()
                                    .flex_1()
                                    .text_size(px(self.config.ui.size * 1.35))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(title),
                            )
                            .child(
                                div()
                                    .id("agent-sessions-close")
                                    .px_2()
                                    .py_1()
                                    .rounded(px(crate::config::corners::CONTROL))
                                    .when(!picker.opening, |d| {
                                        d.cursor_pointer().hover(|s| s.bg(rgb(theme.active)))
                                    })
                                    .text_color(rgb(if picker.opening {
                                        theme.muted
                                    } else {
                                        theme.foreground
                                    }))
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dismiss_menu(window, cx)
                                    })),
                            ),
                    )
                    .when(!watching, |d| {
                        d.child(div().pt(px(12.)).child(picker.search.clone()))
                    }),
            );
        if let Some(watch) = &picker.watch {
            let ready = watch
                .observation
                .as_ref()
                .is_some_and(|o| o.state.can_open())
                && watch.error.is_none();
            let (status, explanation) = if picker.opening {
                (
                    "Opening session...",
                    "Preparing its tab on the original device and workspace.",
                )
            } else if watch
                .observation
                .as_ref()
                .is_some_and(|o| o.state == Activity::Attached)
            {
                (
                    "Already open in Herdr",
                    "Switch to the existing tab to interact with this session.",
                )
            } else if ready {
                (
                    "Ready to continue",
                    "Open this conversation in a tab when you are ready.",
                )
            } else if watch.error.is_some() {
                (
                    "Connection unavailable · read-only",
                    "Interaction stays disabled until the session can be checked.",
                )
            } else if watch.observation.is_none() {
                (
                    "Checking session · read-only",
                    "Checking whether this conversation is available to continue.",
                )
            } else if watch
                .observation
                .as_ref()
                .is_some_and(|o| o.state == Activity::Missing)
            {
                (
                    "Session unavailable",
                    "The saved conversation is no longer on this device. Return to the list to refresh it.",
                )
            } else if watch
                .observation
                .as_ref()
                .is_some_and(|o| o.state == Activity::Unknown)
            {
                (
                    "Activity unverified · read-only",
                    "Herdr cannot yet confirm that this session is available to continue.",
                )
            } else {
                (
                    "Watching · read-only",
                    "This session is still running. Wait until it finishes to continue.",
                )
            };
            panel =
                panel.child(
                    div()
                        .p(px(16.))
                        .flex_none()
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(watch.row.entry.title.clone()),
                        )
                        .child(div().pt(px(4.)).text_color(rgb(theme.muted)).child(format!(
                            "{} · {} · {}",
                            watch.row.host.label, watch.row.entry.agent, watch.row.entry.source
                        )))
                        .child(
                            div()
                                .pt(px(4.))
                                .text_color(rgb(theme.muted))
                                .truncate()
                                .child(watch.row.entry.cwd.clone()),
                        )
                        .child(
                            div()
                                .mt(px(12.))
                                .p(px(12.))
                                .rounded(px(crate::config::corners::CONTROL))
                                .bg(rgb(theme.active))
                                .debug_selector(|| "session-watch-status".into())
                                .child(div().font_weight(FontWeight::SEMIBOLD).child(status))
                                .child(div().pt(px(4.)).child(explanation))
                                .when(!ready, |d| {
                                    d.child(div().pt(px(4.)).text_color(rgb(theme.muted)).child(
                                        "Typing, paste and interrupt shortcuts are disabled.",
                                    ))
                                }),
                        ),
                );
            let output = watch
                .observation
                .as_ref()
                .map(|o| o.output.as_str())
                .filter(|s| !s.is_empty())
                .unwrap_or("No recent output yet. This view updates automatically.");
            panel = panel.child(
                div()
                    .id("session-watch-output")
                    .debug_selector(|| "session-watch-output".into())
                    .flex_1()
                    .min_h_0()
                    .px(px(16.))
                    .pb(px(12.))
                    .overflow_y_scroll()
                    .track_scroll(&picker.scroll)
                    .child(output.to_owned()),
            );
            if let Some(error) = &watch.error {
                panel = panel.child(
                    div()
                        .px(px(16.))
                        .pb(px(8.))
                        .text_color(rgb(theme.muted))
                        .child(error.clone()),
                );
            }
            panel = panel.child(
                div()
                    .p(px(12.))
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .id("session-back")
                            .px_2()
                            .py_1()
                            .cursor_pointer()
                            .child("Back to sessions")
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.back_to_agent_sessions(window, cx)
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("session-continue")
                            .debug_selector(|| "session-continue".into())
                            .px(px(12.))
                            .py(px(8.))
                            .rounded(px(crate::config::corners::CONTROL))
                            .bg(rgb(if ready && !picker.opening {
                                theme.primary_wash()
                            } else {
                                theme.active
                            }))
                            .text_color(rgb(if ready && !picker.opening {
                                theme.foreground
                            } else {
                                theme.muted
                            }))
                            .when(ready && !picker.opening, |d| {
                                d.cursor_pointer().on_click(
                                    cx.listener(|this, _, _, cx| this.continue_agent_session(cx)),
                                )
                            })
                            .child(if picker.opening {
                                "Opening..."
                            } else if ready {
                                if watch
                                    .observation
                                    .as_ref()
                                    .is_some_and(|o| o.state == Activity::Attached)
                                {
                                    "Open existing tab"
                                } else {
                                    "Continue session"
                                }
                            } else {
                                "Waiting for session"
                            }),
                    ),
            );
        } else {
            let mut list = div()
                .id("agent-session-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&picker.scroll)
                .p(px(8.));
            if picker.remaining > 0 {
                list = list.child(
                    div()
                        .p(px(8.))
                        .text_color(rgb(theme.muted))
                        .child("Finding sessions on your devices..."),
                );
            }
            if picker.remaining == 0 && picker.filtered.is_empty() {
                list = list.child(
                    div()
                        .p(px(8.))
                        .text_color(rgb(theme.muted))
                        .child("No matching sessions."),
                );
            }
            for (position, &index) in picker.filtered.iter().enumerate() {
                let row = &picker.rows[index];
                list = list.child(
                    div()
                        .id(("agent-session", index))
                        .debug_selector(move || format!("agent-session-{index}"))
                        .p(px(10.))
                        .rounded(px(crate::config::corners::CONTROL))
                        .cursor_pointer()
                        .when(position == picker.selected, |d| d.bg(rgb(theme.active)))
                        .hover(|d| d.bg(rgb(theme.active)))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(8.))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .truncate()
                                        .child(row.entry.title.clone()),
                                )
                                .child(
                                    div().flex_none().text_color(rgb(theme.muted)).child("View"),
                                ),
                        )
                        .child(
                            div()
                                .pt(px(3.))
                                .text_color(rgb(theme.muted))
                                .truncate()
                                .child(format!(
                                    "{} · {} · {} · {}",
                                    row.host.label,
                                    row.entry.agent,
                                    row.entry.source,
                                    row.entry.cwd.rsplit('/').next().unwrap_or(&row.entry.cwd)
                                )),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.watch_agent_session(index, window, cx)
                        })),
                );
            }
            for error in &picker.errors {
                list = list.child(
                    div()
                        .p(px(8.))
                        .text_color(rgb(theme.muted))
                        .child(error.clone()),
                );
            }
            panel = panel.child(list).child(
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child(
                        "Recent conversations from all devices. Running sessions open read-only.",
                    ),
            );
        }
        panel
    }
}
