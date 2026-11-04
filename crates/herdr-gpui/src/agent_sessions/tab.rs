use super::*;
use crate::browser::{Location, Pick, Slot, Store, Tab, TabId};
use gpui::{App, FocusHandle, prelude::*};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct SavedSession {
    endpoint: String,
    pub(super) device: String,
    pub(super) entry: Entry,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(crate) reports: Vec<crate::browser::WebUrl>,
}

impl SavedSession {
    pub(crate) fn title(&self) -> String {
        self.entry.title.clone()
    }
    pub(crate) fn description(&self) -> String {
        format!("{} · {}", self.device, self.entry.cwd)
    }
}

enum Update {
    Watched(Result<(Source, Observation), Error>),
    Opened(Result<(Source, OpenResult), Error>),
}

pub(crate) struct SessionTab {
    pub(super) saved: SavedSession,
    pub(super) observation: Option<Observation>,
    pub(super) output: String,
    pub(super) error: Option<String>,
    pending: bool,
    pub(super) opening: bool,
    pub(super) auto_continue: bool,
    pub(super) next: Instant,
    pub(super) focus: FocusHandle,
    pub(super) scroll: ScrollHandle,
    sender: mpsc::Sender<Update>,
    receiver: mpsc::Receiver<Update>,
    cancel: Arc<AtomicBool>,
    handoff: Option<(Source, Destination, Instant)>,
}

impl Drop for SessionTab {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl SessionTab {
    fn new(saved: SavedSession, auto_continue: bool, cx: &mut Context<HerdrWindow>) -> Self {
        let (sender, receiver) = mpsc::channel();
        Self {
            saved,
            observation: None,
            output: String::new(),
            error: None,
            pending: false,
            opening: false,
            auto_continue,
            next: Instant::now(),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            sender,
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
            handoff: None,
        }
    }

    pub(super) fn ready(&self) -> bool {
        !self.opening
            && self.handoff.is_none()
            && self.error.is_none()
            && self
                .observation
                .as_ref()
                .is_some_and(|o| o.state.can_open())
    }

    pub(super) fn label(&self) -> &'static str {
        if self.opening || self.handoff.is_some() {
            "Opening"
        } else if self.error.is_some() {
            "Reconnect"
        } else {
            match self.observation.as_ref().map(|o| o.state) {
                Some(Activity::Ready) => "Ready to continue",
                Some(Activity::Attached) => "Open tab",
                Some(Activity::Watch) => "Watching",
                Some(Activity::Unknown) => "Read-only",
                Some(Activity::Missing) => "Unavailable",
                None => "Connecting",
            }
        }
    }
}

impl HerdrWindow {
    pub(crate) fn focus_session_tab(&self, id: TabId, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab) = self.session_tabs.get(&id) {
            window.focus(&tab.focus, cx);
        }
    }

    pub(crate) fn active_session_tab(&self, cx: &App) -> Option<TabId> {
        let Pick::Page(id) = self.active_page_session(cx)? else {
            return None;
        };
        cx.try_global::<Store>()?
            .get(id)?
            .location
            .as_ref()
            .and_then(|location| matches!(location, Location::AgentSession { .. }).then_some(id))
    }

    pub(super) fn open_agent_session(
        &mut self,
        row: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(row) = self
            .agent_sessions
            .as_ref()
            .and_then(|p| p.rows.get(row))
            .cloned()
        else {
            return;
        };
        let Some((scope, workspace)) = self.browser_key() else {
            self.local_error = Some("Open a workspace before opening a session.".into());
            return;
        };
        let saved = SavedSession {
            endpoint: row.host.id,
            device: row.host.label,
            entry: row.entry,
            reports: Vec::new(),
        };
        let existing = cx.try_global::<Store>().and_then(|store| {
            store.in_workspace(&scope, &workspace).find(|tab| {
                matches!(&tab.location, Some(Location::AgentSession { session })
                    if session.entry.agent == saved.entry.agent && session.entry.session == saved.entry.session)
            }).map(|tab| tab.id)
        });
        let opened = existing.or_else(|| {
            Store::update(cx, |store| {
                store.open(
                    scope,
                    &workspace,
                    Some(Location::AgentSession {
                        session: saved.clone(),
                    }),
                    None,
                )
            })
        });
        let Some(id) = opened else {
            return;
        };
        self.session_tabs
            .entry(id)
            .or_insert_with(|| SessionTab::new(saved, true, cx));
        self.dismiss_menu(window, cx);
        self.show_browser_tab(id, window, cx);
        self.sidebar_scroll[2].scroll_to_bottom();
        if let Some(tab) = self.session_tabs.get(&id) {
            window.focus(&tab.focus, cx);
        }
        tracing::info!("Agent session tab opened");
        cx.notify();
    }

    pub(super) fn poll_session_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(store) = cx.try_global::<Store>() else {
            return;
        };
        self.session_tabs.retain(|id, _| store.get(*id).is_some());
        if let Some((scope, workspace)) = self.browser_key() {
            let missing: Vec<_> = store
                .in_workspace(&scope, &workspace)
                .filter_map(|tab| {
                    if self.session_tabs.contains_key(&tab.id) {
                        return None;
                    }
                    match &tab.location {
                        Some(Location::AgentSession { session }) => Some((tab.id, session.clone())),
                        _ => None,
                    }
                })
                .collect();
            for (id, saved) in missing {
                self.session_tabs
                    .insert(id, SessionTab::new(saved, false, cx));
            }
        }
        let sources = self.session_sources();
        let active = self.active_session_tab(cx);
        let mut continue_ids = vec![];
        let mut reports = Vec::new();
        for (&id, tab) in &mut self.session_tabs {
            while let Ok(update) = tab.receiver.try_recv() {
                match update {
                    Update::Watched(result) => {
                        tab.pending = false;
                        tab.next = Instant::now()
                            + Duration::from_secs(if active == Some(id) { 3 } else { 15 });
                        match result {
                            Ok((source, observation)) => {
                                if tab.observation.is_none()
                                    || tab.scroll.logical_scroll_bottom().1 <= gpui::px(1.)
                                {
                                    tab.scroll.scroll_to_bottom();
                                }
                                tab.output.clone_from(&observation.output);
                                reports
                                    .push((id, crate::browser::report_urls(&observation.output)));
                                tab.saved.endpoint = source.id;
                                tab.saved.device = source.label;
                                tab.observation = Some(observation);
                                tab.error = None;
                                if tab.auto_continue && tab.ready() {
                                    continue_ids.push(id);
                                }
                            }
                            Err(error) => {
                                tracing::warn!(operation = "watch", %error, "Agent session request failed");
                                tab.error = Some(error.to_string());
                                tab.observation = None;
                            }
                        }
                        // The initial click may open a stopped session. A running
                        // or restored watch never gains an automatic handoff later.
                        tab.auto_continue = false;
                    }
                    Update::Opened(result) => {
                        tab.opening = false;
                        match result {
                            Ok((source, result)) => {
                                tab.observation = Some(Observation {
                                    state: Activity::Attached,
                                    output: tab.output.clone(),
                                });
                                tab.saved.endpoint = source.id.clone();
                                tab.handoff = Some((source, result.opened, Instant::now()));
                                tracing::info!("Agent session continuation acknowledged");
                            }
                            Err(error) => {
                                tracing::warn!(operation = "continue", %error, "Agent session request failed");
                                tab.error = Some(error.to_string());
                                tab.observation = None;
                            }
                        }
                    }
                }
                cx.notify();
            }
            if tab.pending || tab.opening || tab.handoff.is_some() || Instant::now() < tab.next {
                continue;
            }
            let Some(source) = sources.iter().find(|s| s.id == tab.saved.endpoint).cloned() else {
                tab.error = Some(
                    "This device is disconnected. Reconnect it in Devices to continue.".into(),
                );
                tab.auto_continue = false;
                tab.next = Instant::now() + Duration::from_secs(10);
                continue;
            };
            tab.pending = true;
            let row = Row {
                host: source,
                entry: tab.saved.entry.clone(),
            };
            let (sources, sender, cancel) =
                (sources.clone(), tab.sender.clone(), tab.cancel.clone());
            cx.background_executor()
                .spawn(async move {
                    let result = observe(&row, &sources, |source| {
                        source.request(
                            serde_json::json!({"operation":"watch", "key":row.entry}),
                            &cancel,
                        )
                    });
                    let _ = sender.send(Update::Watched(result));
                })
                .detach();
        }
        for id in continue_ids {
            self.continue_session_tab(id, cx);
        }
        for (id, urls) in reports {
            if !urls.is_empty() {
                Store::update(cx, |store| store.discover_session_pages(id, urls));
            }
        }
        self.finish_session_handoffs(window, cx);
    }

    pub(super) fn continue_session_tab(&mut self, id: TabId, cx: &mut Context<Self>) {
        let sources = self.session_sources();
        let Some(tab) = self.session_tabs.get_mut(&id) else {
            return;
        };
        if !tab.ready() {
            return;
        }
        let Some(source) = sources.iter().find(|s| s.id == tab.saved.endpoint).cloned() else {
            return;
        };
        let row = Row {
            host: source,
            entry: tab.saved.entry.clone(),
        };
        tab.opening = true;
        let (sender, cancel) = (tab.sender.clone(), tab.cancel.clone());
        cx.background_executor()
            .spawn(async move {
                let result = (|| {
                    let (source, observation) = observe(&row, &sources, |source| {
                        source.request(
                            serde_json::json!({"operation":"watch", "key":row.entry}),
                            &cancel,
                        )
                    })?;
                    if !observation.state.can_open() {
                        return Err(Error::Refused(
                            "The session is still in use. Keep watching until it finishes.".into(),
                        ));
                    }
                    let opened = source.request(
                        serde_json::json!({"operation":"open", "key":row.entry}),
                        &cancel,
                    )?;
                    Ok((source, opened))
                })();
                let _ = sender.send(Update::Opened(result));
            })
            .detach();
        cx.notify();
    }

    pub(crate) fn render_session_tab(
        &self,
        slot: Slot,
        tab: &Tab,
        gap: f32,
        cx: &mut Context<Self>,
    ) -> gpui::AnyElement {
        let id = tab.id;
        let Some(watch) = self.session_tabs.get(&id) else {
            return gpui::div().into_any_element();
        };
        gpui::div()
            .id(gpui::SharedString::from(slot.selector("agent-session-tab")))
            .size_full()
            .min_w_0()
            .pl(gpui::px(gap))
            .track_focus(&watch.focus)
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if let Some(tab) = this.session_tabs.get(&id) {
                        window.focus(&tab.focus, cx);
                    }
                }),
            )
            .on_key_down(|event, window, cx| {
                if !event.keystroke.modifiers.platform || event.keystroke.key == "v" {
                    cx.stop_propagation();
                    window.prevent_default();
                }
            })
            .child(self.render_session_watch(id, watch, cx))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests;

mod handoff;
