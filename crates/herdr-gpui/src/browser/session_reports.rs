//! The pages a session's agent linked, behind the Reports button of its strip.
use super::{GroupId, Location, PageSource, Pick, Scope, Slot, Store, WebUrl, view::store};
use crate::{HerdrWindow, menu::Page};
use gpui::{prelude::*, *};

pub(crate) const WIDTH: f32 = 340.;
/// Rows the list shows before it scrolls.
const VISIBLE_ROWS: f32 = 12.;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Entry {
    pub(crate) url: WebUrl,
    pub(crate) title: SharedString,
    /// When the agent last linked it, in Unix seconds; 0 when unknown.
    at: u64,
    /// The pane whose agent linked it, which notes on the page go back to.
    pane: Option<String>,
    /// Whether it is open as a page of the session.
    pub(crate) open: bool,
}

pub(crate) struct ReportsMenu {
    group: GroupId,
    scope: Scope,
    workspace: String,
    session: Pick,
    entries: Vec<Entry>,
    selected: Option<usize>,
    scroll: ScrollHandle,
}

impl HerdrWindow {
    /// Has the report links read of the agents in the sessions this window
    /// shows, on the daemon it is looking at.
    pub(crate) fn view_session_reports(&mut self, cx: &App) {
        let tabs: Vec<String> = self
            .group_slots()
            .into_iter()
            .filter_map(|slot| self.session_for_pick(&self.group_pick(slot.id)?, cx))
            .filter_map(|session| match session {
                Pick::Herdr(tab) => Some(tab),
                Pick::Page(_) => None,
            })
            .collect();
        let panes: Vec<String> = self
            .live
            .snapshot
            .iter()
            .flat_map(|snapshot| &snapshot.agents)
            .filter(|agent| tabs.contains(&agent.tab_id))
            .map(|agent| agent.pane_id.clone())
            .collect();
        self.agent_models.view(
            &self.endpoints[self.selected_endpoint].connection.target,
            &panes,
        );
    }

    /// The pages `session` linked, newest first: a terminal's from its
    /// agents' transcripts, a watched conversation's from what it printed.
    pub(crate) fn session_reports(&self, session: &Pick, cx: &App) -> Vec<Entry> {
        let mut entries: Vec<Entry> = match session {
            Pick::Herdr(tab) => {
                let target = &self.endpoints[self.selected_endpoint].connection.target;
                self.live
                    .snapshot
                    .iter()
                    .flat_map(|snapshot| &snapshot.agents)
                    .filter(|agent| &agent.tab_id == tab)
                    .flat_map(|agent| {
                        self.agent_models
                            .reports(target, &agent.pane_id)
                            .iter()
                            .map(move |report| (report, &agent.pane_id))
                    })
                    .filter_map(|(report, pane)| {
                        let url = WebUrl::try_from(report.url.as_str()).ok()?;
                        let title = super::store::clean_title(&report.title);
                        Entry::new(url, &title, report.at, Some(pane.clone()))
                    })
                    .collect()
            }
            Pick::Page(id) => match store(cx).and_then(|store| store.get(*id)) {
                Some(super::Tab {
                    location: Some(Location::AgentSession { session }),
                    ..
                }) => session
                    .reports
                    .iter()
                    .rev()
                    .filter_map(|url| Entry::new(url.clone(), "", 0, None))
                    .collect(),
                _ => Vec::new(),
            },
        };
        entries.sort_by_key(|entry| std::cmp::Reverse(entry.at));
        let mut listed: Vec<Entry> = Vec::with_capacity(entries.len());
        for entry in entries {
            if !listed.iter().any(|earlier| earlier.url == entry.url) {
                listed.push(entry);
            }
        }
        if let (Some(store), Some((scope, workspace))) = (store(cx), self.browser_key()) {
            for entry in &mut listed {
                entry.open = store.session_pages(&scope, &workspace, session).any(
                    |tab| matches!(&tab.location, Some(Location::Web { url }) if *url == entry.url),
                );
            }
        }
        listed
    }

    /// The strip's button, which is there only for a session with reports.
    pub(super) fn render_reports_button(
        &self,
        slot: Slot,
        session: &Pick,
        cx: &mut Context<Self>,
    ) -> Option<Stateful<Div>> {
        let count = self.session_reports(session, cx).len();
        if count == 0 {
            return None;
        }
        let (muted, active) = (self.theme.muted, self.theme.active);
        Some(
            div()
                .id(SharedString::from(slot.selector("session-reports")))
                .debug_selector(move || slot.selector("session-reports"))
                .flex_none()
                .h_full()
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(6.))
                .text_color(rgb(muted))
                .cursor_pointer()
                .hover(|style| style.bg(rgb(active)))
                .child(format!("Reports {count}"))
                .child(
                    svg()
                        .path("icons/chevron-down.svg")
                        .size(px(10.))
                        .text_color(rgb(muted)),
                )
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                    cx.stop_propagation();
                    this.open_reports_menu(slot, event.position(), window, cx);
                })),
        )
    }

    pub(crate) fn open_reports_menu(
        &mut self,
        slot: Slot,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .group_pick(slot.id)
            .and_then(|pick| self.session_for_pick(&pick, cx))
        else {
            return;
        };
        let Some((scope, workspace)) = self.browser_key() else {
            return;
        };
        let entries = self.session_reports(&session, cx);
        if entries.is_empty() || !self.open_menu(window, cx) {
            return;
        }
        self.menu.anchor = anchor;
        self.menu.page = Some(Page::Reports);
        self.menu.reports = Some(ReportsMenu {
            group: slot.id,
            scope,
            workspace,
            session,
            entries,
            selected: None,
            scroll: ScrollHandle::new(),
        });
    }

    /// Shows the report at `index` as a page of its session, in the group
    /// whose strip listed it. One already open is loaded again, as its file
    /// may have been written since.
    fn open_listed_report(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(menu) = self.menu.reports.take() else {
            return;
        };
        self.dismiss_menu(window, cx);
        let Some(entry) = menu.entries.get(index) else {
            return;
        };
        if self.browser_key() != Some((menu.scope.clone(), menu.workspace.clone())) {
            return;
        }
        let source = PageSource {
            scope: menu.scope,
            workspace: menu.workspace,
            session: menu.session,
            pane: entry.pane.clone(),
        };
        let location = Location::Web {
            url: entry.url.clone(),
        };
        let opened = Store::update(cx, |store| store.open_session_location(&source, location));
        if let Some((id, reused)) = opened {
            self.reload_reopened_tab(id, reused, cx);
            self.show_browser_tab_in(Some(menu.group), id, window, cx);
        }
    }

    pub(crate) fn reports_menu_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(menu) = &mut self.menu.reports else {
            return;
        };
        cx.stop_propagation();
        window.prevent_default();
        let count = menu.entries.len();
        let key = event.keystroke.key.as_str();
        match key {
            "escape" => self.dismiss_menu(window, cx),
            "up" | "down" if count > 0 => {
                let selected = match (menu.selected, key) {
                    (None, "up") => count - 1,
                    (None, _) => 0,
                    (Some(index), "up") => (index + count - 1) % count,
                    (Some(index), _) => (index + 1) % count,
                };
                menu.selected = Some(selected);
                menu.scroll.scroll_to_item(selected);
                cx.notify();
            }
            "enter" => {
                if let Some(index) = menu.selected {
                    self.open_listed_report(index, window, cx);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn render_reports_menu(&self, cx: &mut Context<Self>) -> Div {
        let Some(menu) = &self.menu.reports else {
            return div();
        };
        let theme = &self.theme;
        let (muted, primary) = (theme.muted, theme.primary());
        let row_height = self.config.ui.line_height() + 12.;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        let mut list = div()
            .id("session-report-list")
            .flex()
            .flex_col()
            .max_h(px(row_height * VISIBLE_ROWS))
            .overflow_y_scroll()
            .track_scroll(&menu.scroll);
        for (index, entry) in menu.entries.iter().enumerate() {
            let age = (entry.at > 0).then(|| {
                crate::checkpoint::age(
                    i64::try_from(entry.at).unwrap_or(i64::MAX),
                    i64::try_from(now).unwrap_or(i64::MAX),
                )
            });
            list = list.child(
                div()
                    .id(("session-report", index))
                    .debug_selector(move || format!("session-report-{index}"))
                    .flex_none()
                    .min_h(px(row_height))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(menu.selected == Some(index), |row| {
                        row.bg(rgb(theme.active))
                    })
                    .hover(|row| row.bg(rgb(theme.active)))
                    .child(
                        div()
                            .debug_selector(move || format!("session-report-open-{index}"))
                            .flex_none()
                            .size(px(6.))
                            .rounded_full()
                            .when(entry.open, |dot| dot.bg(rgb(primary)))
                            .when(!entry.open, |dot| {
                                dot.border_1().border_color(rgba((muted << 8) | 0x80))
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(entry.title.clone()),
                    )
                    .children(age.map(|age| div().flex_none().text_color(rgb(muted)).child(age)))
                    .on_hover(cx.listener(move |this, hovered, _, cx| {
                        if *hovered && let Some(menu) = &mut this.menu.reports {
                            menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.open_listed_report(index, window, cx);
                    })),
            );
        }
        div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(8.))
                    .py(px(6.))
                    .text_color(rgb(muted))
                    .child("Reports"),
            )
            .child(list)
    }
}

impl Entry {
    /// None for an address the app would not show as a session's page.
    fn new(url: WebUrl, title: &str, at: u64, pane: Option<String>) -> Option<Self> {
        let file = url.report_title()?;
        let title = if title.is_empty() {
            file
        } else {
            title.to_owned()
        };
        Some(Self {
            url,
            title: title.into(),
            at,
            pane,
            open: false,
        })
    }
}
