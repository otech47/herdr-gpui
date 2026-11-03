use super::tab::SessionTab;
use super::*;
use crate::browser::{Location, Store, TabId};
use gpui::{Div, FontWeight, SharedString, div, prelude::*, px, rgb};

impl HerdrWindow {
    pub(crate) fn render_agent_sessions(&self, cx: &mut Context<Self>) -> Div {
        let Some(picker) = &self.agent_sessions else {
            return div();
        };
        let theme = &self.theme;
        let mut list = div()
            .id("agent-session-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&picker.scroll)
            .p(px(8.));
        let mut previous_group = None;
        for (position, &index) in picker.filtered.iter().enumerate() {
            let row = &picker.rows[index];
            let background = row.entry.background();
            let heading = previous_group != Some(background);
            previous_group = Some(background);
            let project = row.entry.cwd.rsplit('/').next().unwrap_or(&row.entry.cwd);
            let details = if background {
                format!(
                    "{} · {} · {} · {}",
                    project, row.host.label, row.entry.agent, row.entry.source
                )
            } else {
                format!("{} · {} · {}", project, row.host.label, row.entry.agent)
            };
            list = list.child(
                div()
                    .when(heading, |d| {
                        d.child(
                            div()
                                .px(px(10.))
                                .py(px(6.))
                                .text_color(rgb(theme.muted))
                                .text_size(px(self.config.ui.size * 0.9))
                                .child(if background {
                                    "From Jarvis & Roboz"
                                } else {
                                    "Recent conversations"
                                }),
                        )
                    })
                    .child(
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
                                    .gap(px(12.))
                                    .child(
                                        div()
                                            .flex_1()
                                            .min_w_0()
                                            .child(div().truncate().child(row.entry.title.clone()))
                                            .child(
                                                div()
                                                    .pt(px(3.))
                                                    .text_color(rgb(theme.muted))
                                                    .truncate()
                                                    .child(details),
                                            ),
                                    )
                                    .child(
                                        div().flex_none().text_color(rgb(theme.muted)).child(
                                            if row.entry.running { "Watch" } else { "Open" },
                                        ),
                                    ),
                            )
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.open_agent_session(index, window, cx)
                            })),
                    ),
            );
        }
        if picker.filtered.is_empty() {
            list = list.child(div().p(px(16.)).text_color(rgb(theme.muted)).child(
                if picker.remaining > 0 {
                    "Finding conversations on your devices..."
                } else {
                    "No matching conversations. Try another task or project name."
                },
            ));
        }
        for error in &picker.errors {
            list = list.child(
                div()
                    .p(px(10.))
                    .text_color(rgb(theme.muted))
                    .child(error.clone()),
            );
        }
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
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
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_size(px(self.config.ui.size * 1.35))
                                    .child("Open session"),
                            )
                            .child(
                                div()
                                    .id("agent-sessions-refresh")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .text_color(rgb(theme.muted))
                                    .child(if picker.remaining > 0 {
                                        "Refreshing..."
                                    } else {
                                        "Refresh"
                                    })
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.refresh_agent_sessions(cx)
                                    })),
                            )
                            .child(
                                div()
                                    .id("agent-sessions-close")
                                    .px_2()
                                    .py_1()
                                    .cursor_pointer()
                                    .child("Close")
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.dismiss_menu(window, cx)
                                    })),
                            ),
                    )
                    .child(div().pt(px(12.)).child(picker.search.clone())),
            )
            .child(list)
            .child(
                div()
                    .px(px(16.))
                    .py(px(12.))
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child("Running sessions open as read-only tabs."),
            )
    }

    pub(super) fn render_session_watch(
        &self,
        id: TabId,
        watch: &SessionTab,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        let ready = watch.ready();
        let narrow = self.viewport_width < 760.;
        let (status, explanation) = if watch.opening {
            (
                "Opening session...",
                "Preparing the original agent and workspace.",
            )
        } else if watch.error.is_some() {
            (
                "Connection needs attention",
                "Your last output is kept here. Reconnect to check this session again.",
            )
        } else {
            match watch.observation.as_ref().map(|o| o.state) {
                Some(Activity::Ready) => (
                    "Ready to continue",
                    "This session is available. Continue when you are ready.",
                ),
                Some(Activity::Attached) => (
                    "Already open in Herdr",
                    "This conversation has an interactive tab.",
                ),
                Some(Activity::Watch) => (
                    "Running · read-only",
                    "You can keep working in other tabs. Herdr will show when this session is ready.",
                ),
                Some(Activity::Unknown) => (
                    "Session still in use · read-only",
                    "An agent may still own this conversation. Waiting for a safe handoff.",
                ),
                Some(Activity::Missing) => (
                    "Session unavailable",
                    "The saved conversation is no longer on this device.",
                ),
                None => ("Connecting to session...", "Checking its device and agent."),
            }
        };
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(rgb(theme.background))
            .child(
                div()
                    .p(px(20.))
                    .flex_none()
                    .border_b_1()
                    .border_color(rgb(theme.active))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(12.))
                            .when(narrow, |d| d.flex_col().items_start())
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .when(narrow, |d| d.w_full())
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_size(px(self.config.ui.size * 1.3))
                                            .truncate()
                                            .child(watch.saved.entry.title.clone()),
                                    )
                                    .child(
                                        div()
                                            .pt(px(4.))
                                            .text_color(rgb(theme.muted))
                                            .truncate()
                                            .child(format!(
                                                "{} · {} · {}",
                                                watch.saved.device,
                                                watch.saved.entry.agent,
                                                watch.saved.entry.source
                                            )),
                                    ),
                            )
                            .when(ready, |d| {
                                d.child(
                                    div()
                                        .id("session-continue")
                                        .debug_selector(|| "session-continue".into())
                                        .px(px(14.))
                                        .py(px(9.))
                                        .rounded(px(crate::config::corners::CONTROL))
                                        .bg(rgb(theme.primary_wash()))
                                        .cursor_pointer()
                                        .child(
                                            if watch
                                                .observation
                                                .as_ref()
                                                .is_some_and(|o| o.state == Activity::Attached)
                                            {
                                                "Open tab"
                                            } else {
                                                "Continue session"
                                            },
                                        )
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            this.continue_session_tab(id, cx)
                                        })),
                                )
                            })
                            .when(watch.error.is_some(), |d| {
                                d.child(
                                    div()
                                        .id("session-retry")
                                        .px_2()
                                        .py_1()
                                        .cursor_pointer()
                                        .child("Retry")
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(tab) = this.session_tabs.get_mut(&id) {
                                                tab.next = Instant::now();
                                            }
                                            cx.notify();
                                        })),
                                )
                            }),
                    )
                    .child(
                        div()
                            .mt(px(14.))
                            .debug_selector(|| "session-watch-status".into())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(status),
                    )
                    .child(
                        div()
                            .pt(px(4.))
                            .text_color(rgb(theme.muted))
                            .child(explanation),
                    ),
            )
            .child(
                div()
                    .id(SharedString::from(format!("session-output-{id}")))
                    .debug_selector(|| "session-watch-output".into())
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .track_scroll(&watch.scroll)
                    .p(px(24.))
                    .child(div().max_w(px(900.)).child(if watch.output.is_empty() {
                        "Waiting for the first update...".to_owned()
                    } else {
                        watch.output.clone()
                    })),
            )
            .when_some(watch.error.as_ref(), |d, error| {
                d.child(
                    div()
                        .px(px(20.))
                        .pb(px(12.))
                        .text_color(rgb(theme.muted))
                        .child(error.clone()),
                )
            })
            .child(
                div()
                    .px(px(20.))
                    .py(px(10.))
                    .flex_none()
                    .border_t_1()
                    .border_color(rgb(theme.active))
                    .text_color(rgb(theme.muted))
                    .child("Read-only. Typing, paste and interrupt keys are disabled."),
            )
    }

    pub(crate) fn session_sidebar_rows(&self, cx: &mut Context<Self>) -> Vec<gpui::AnyElement> {
        let Some((scope, workspace)) = self.browser_key() else {
            return vec![];
        };
        let Some(store) = cx.try_global::<Store>() else {
            return vec![];
        };
        let tabs: Vec<_> = store
            .in_workspace(&scope, &workspace)
            .filter(|tab| matches!(tab.location, Some(Location::AgentSession { .. })))
            .cloned()
            .collect();
        let active = self.active_session_tab(cx);
        tabs.into_iter()
            .map(|tab| {
                let id = tab.id;
                let status = self
                    .session_tabs
                    .get(&id)
                    .map_or("Connecting", SessionTab::label);
                div()
                    .id(SharedString::from(format!("watched-session-{id}")))
                    .debug_selector(move || format!("watched-session-{id}"))
                    .mx(px(8.))
                    .px(px(10.))
                    .py(px(7.))
                    .rounded(px(crate::config::corners::CONTROL))
                    .when(active == Some(id), |d| d.bg(rgb(self.theme.active)))
                    .hover(|d| d.bg(rgb(self.theme.active)))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(div().truncate().child(tab.title))
                            .child(
                                div()
                                    .text_size(px(self.config.ui.size * 0.9))
                                    .text_color(rgb(self.theme.muted))
                                    .child(status),
                            ),
                    )
                    .child(
                        div()
                            .id("close-watch")
                            .px_1()
                            .child("×")
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_browser_tab(id, window, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_browser_tab(id, window, cx);
                        if let Some(tab) = this.session_tabs.get(&id) {
                            window.focus(&tab.focus, cx);
                        }
                    }))
                    .into_any_element()
            })
            .collect()
    }
}
