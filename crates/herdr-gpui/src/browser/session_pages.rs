use super::{Location, Pick, Scope, Slot, Store, Tab, view::store};
use crate::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn session_for_pick(&self, pick: &Pick, cx: &App) -> Option<Pick> {
        match pick {
            Pick::Herdr(_) => Some(pick.clone()),
            Pick::Page(id) => {
                let tab = store(cx)?.get(*id)?;
                tab.session.clone().or_else(|| {
                    matches!(tab.location, Some(Location::AgentSession { .. }))
                        .then(|| pick.clone())
                })
            }
        }
    }

    pub(crate) fn active_page_session(&self, cx: &App) -> Option<Pick> {
        self.session_for_pick(&self.group_pick(self.active_group()?)?, cx)
    }

    pub(super) fn page_session_for_pane(
        &self,
        scope: &Scope,
        workspace: &str,
        pane: &str,
    ) -> Option<Pick> {
        self.endpoints
            .iter()
            .enumerate()
            .find_map(|(index, endpoint)| {
                if &super::scope(endpoint) != scope {
                    return None;
                }
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let snapshot = live.snapshot.as_ref()?;
                let pane = snapshot
                    .panes
                    .iter()
                    .find(|candidate| candidate.pane_id == pane)?;
                snapshot
                    .tabs
                    .iter()
                    .find(|tab| tab.tab_id == pane.tab_id && tab.workspace_id == workspace)
                    .map(|tab| Pick::Herdr(tab.tab_id.clone()))
            })
    }

    // Pane IDs are device-local. A snapshot must identify the origin's session.
    pub(super) fn associate_session_pages(&mut self, cx: &mut Context<Self>) {
        let bindings: Vec<_> = store(cx)
            .into_iter()
            .flat_map(Store::unbound_pages)
            .filter_map(|tab| {
                let session = self.page_session_for_pane(
                    &tab.scope,
                    &tab.workspace_id,
                    tab.origin.as_deref()?,
                )?;
                Some((tab.id, session))
            })
            .collect();
        if !bindings.is_empty() {
            Store::update(cx, |store| {
                for (page, session) in bindings {
                    store.attach_to_session(page, session);
                }
            });
        }
    }

    pub(crate) fn session_page_count(
        &self,
        scope: &Scope,
        workspace: &str,
        session: &Pick,
        cx: &App,
    ) -> usize {
        store(cx).map_or(0, |store| {
            store.session_pages(scope, workspace, session).count()
        })
    }

    pub(crate) fn show_session(
        &mut self,
        slot: Slot,
        session: Pick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match session {
            Pick::Herdr(tab) => self.choose_herdr_tab(slot.id, &tab, window, cx),
            Pick::Page(id) => {
                let page = self
                    .remembered_session_page(&Pick::Page(id), cx)
                    .unwrap_or(id);
                self.show_browser_tab_in(Some(slot.id), page, window, cx);
                if page == id {
                    self.focus_session_tab(id, window, cx);
                }
            }
        }
    }

    fn show_session_terminal(
        &mut self,
        slot: Slot,
        session: Pick,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(layout) = self.ensure_layout() {
            layout.remember_session_page(session.clone(), None);
        }
        self.show_session(slot, session, window, cx);
    }

    pub(crate) fn render_session_pages(
        &mut self,
        slot: Slot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let pick = self.group_pick(slot.id)?;
        let session = self.session_for_pick(&pick, cx)?;
        let (scope, workspace) = self.browser_key()?;
        let pages: Vec<Tab> = store(cx)
            .into_iter()
            .flat_map(|store| store.session_pages(&scope, &workspace, &session))
            .cloned()
            .collect();
        let scroll = self.browser.session_scroll.handle(slot.id);
        let index = pages
            .iter()
            .position(|tab| pick == Pick::Page(tab.id))
            .map_or(0, |index| index + 1);
        if self
            .browser
            .session_scroll
            .reveal(slot.id, &pick, index, false)
        {
            window.request_animation_frame();
        }
        let terminal = session.clone();
        let selected = pick == session;
        let foreground = self.theme.foreground;
        let active = self.theme.active;
        let background = self.theme.surface;
        let primary = self.theme.primary();
        let mut row = div()
            .id(SharedString::from(slot.selector("session-pages")))
            .debug_selector(move || slot.selector("session-pages"))
            .flex()
            .flex_none()
            .items_center()
            .h(px(34.))
            .w_full()
            .min_w_0()
            .overflow_x_scroll()
            .track_scroll(&scroll)
            .bg(rgb(background))
            .border_b_1()
            .border_color(rgb(active))
            .text_size(px(self.config.ui.size))
            .text_color(rgb(foreground))
            .child(
                div()
                    .id(SharedString::from(slot.selector("session-terminal")))
                    .debug_selector(move || slot.selector("session-terminal"))
                    .flex_none()
                    .h_full()
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .border_b_2()
                    .border_color(rgb(if selected { primary } else { background }))
                    .when(selected, |row| row.bg(rgb(active)))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(active)))
                    .child(if matches!(session, Pick::Herdr(_)) {
                        "Terminal"
                    } else {
                        "Session"
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_session_terminal(slot, terminal.clone(), window, cx);
                    })),
            );
        for tab in pages {
            let id = tab.id;
            let selected = pick == Pick::Page(id);
            row = row.child(
                div()
                    .id(SharedString::from(format!("session-page-{id}")))
                    .debug_selector(move || slot.selector(&format!("session-page-{id}")))
                    .flex_none()
                    .h_full()
                    .pl(px(12.))
                    .pr(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .border_b_2()
                    .border_color(rgb(if selected { primary } else { background }))
                    .when(selected, |row| row.bg(rgb(active)))
                    .cursor_pointer()
                    .hover(|style| style.bg(rgb(active)))
                    .child(super::view::tab_label(&tab.title))
                    .child(
                        div()
                            .id("close-session-page")
                            .debug_selector(move || {
                                slot.selector(&format!("close-session-page-{id}"))
                            })
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(4.))
                            .hover(|style| style.bg(rgb(background)))
                            .child(
                                svg()
                                    .path("icons/close.svg")
                                    .size(px(12.))
                                    .text_color(rgb(foreground)),
                            )
                            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                            .on_click(cx.listener(move |this, _, window, cx| {
                                cx.stop_propagation();
                                this.close_browser_tab(id, window, cx);
                            })),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.show_browser_tab_in(Some(slot.id), id, window, cx);
                    })),
            );
        }
        Some(row.into_any_element())
    }
}
