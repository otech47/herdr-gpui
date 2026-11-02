//! Saved agent conversations, with a read-only view before explicit continuation.
#[cfg(any(test, feature = "integration-test"))]
mod fixtures;
mod render;
#[cfg(test)]
mod tests;

use crate::{HerdrWindow, NavigationTarget, menu::Page, search_input::SearchInput};
use gpui::{AppContext, Context, Entity, KeyDownEvent, Point, ScrollHandle, Subscription, Window};
use herdr_client::{ConnectTarget, shell_quote};
use serde::{Deserialize, Serialize};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

const HELPER: &str = include_str!("../../../scripts/agent_sessions.py");

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error(transparent)]
    Host(#[from] herdr_client::Error),
    #[error("The device returned an invalid session response: {0}")]
    Decode(#[from] serde_json::Error),
    #[error("{0}")]
    Refused(String),
}

#[derive(Clone)]
struct Source {
    id: String,
    label: String,
    generation: u64,
    host: crate::teleport::Host,
    cli: Vec<String>,
}

impl Source {
    fn request<T: serde::de::DeserializeOwned>(
        &self,
        mut request: serde_json::Value,
        cancel: &AtomicBool,
    ) -> Result<T, Error> {
        request["herdr"] = serde_json::json!(self.cli);
        let script = format!(
            "python3 -c {} {}",
            shell_quote(HELPER),
            shell_quote(&request.to_string())
        );
        let output = self.host.capture(&script, cancel)?;
        let value: serde_json::Value = serde_json::from_slice(&output)?;
        if let Some(error) = value["error"].as_str() {
            return Err(Error::Refused(error.to_owned()));
        }
        Ok(serde_json::from_value(value)?)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct Entry {
    agent: String,
    session: String,
    title: String,
    cwd: String,
    source: String,
    modified: f64,
}

#[derive(Deserialize)]
struct Listing {
    entries: Vec<Entry>,
}

#[derive(Clone)]
struct Row {
    host: Source,
    entry: Entry,
}

fn observe(
    row: &Row,
    sources: &[Source],
    mut probe: impl FnMut(&Source) -> Result<Observation, Error>,
) -> Result<(Source, Observation), Error> {
    let original = probe(&row.host)?;
    if original.state == Activity::Attached {
        return Ok((row.host.clone(), original));
    }
    let mut selected = (row.host.clone(), original);
    // These stores sync between devices. A stopped local copy cannot grant
    // permission to resume while the same conversation runs on another host.
    if matches!(row.entry.agent.as_str(), "claude" | "kimi") {
        let mut unavailable = None;
        for source in sources.iter().filter(|s| s.id != row.host.id) {
            match probe(source) {
                Ok(observation) if observation.state == Activity::Attached => {
                    return Ok((source.clone(), observation));
                }
                Ok(observation)
                    if matches!(observation.state, Activity::Watch | Activity::Unknown) =>
                {
                    selected = (source.clone(), observation);
                }
                Ok(observation)
                    if selected.1.state == Activity::Missing
                        && observation.state == Activity::Ready =>
                {
                    selected = (source.clone(), observation);
                }
                Ok(_) => {}
                Err(_) => unavailable = Some(source.label.clone()),
            }
        }
        if selected.1.state.can_open()
            && let Some(label) = unavailable
        {
            return Err(Error::Refused(format!(
                "Cannot verify this synced session on {label}. Continuation stays disabled until that device is reachable."
            )));
        }
    }
    Ok(selected)
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum Activity {
    Ready,
    Attached,
    Watch,
    Unknown,
    Missing,
}

impl Activity {
    fn can_open(self) -> bool {
        matches!(self, Self::Ready | Self::Attached)
    }
}

#[derive(Clone, Deserialize)]
struct Observation {
    state: Activity,
    output: String,
}

#[derive(Deserialize)]
struct OpenResult {
    opened: Destination,
}

#[derive(Deserialize)]
struct Destination {
    pane_id: String,
}

enum Reply {
    Listed(Source, Result<Listing, Error>),
    Watched(Result<(Source, Observation), Error>),
    Opened(Result<(Source, OpenResult), Error>),
}

struct Watch {
    row: Row,
    observation: Option<Observation>,
    pending: bool,
    next: Instant,
    error: Option<String>,
}

pub(crate) struct Picker {
    search: Entity<SearchInput>,
    _subscription: Subscription,
    rows: Vec<Row>,
    sources: Vec<Source>,
    filtered: Vec<usize>,
    selected: usize,
    remaining: usize,
    errors: Vec<String>,
    watch: Option<Watch>,
    opening: bool,
    scroll: ScrollHandle,
    sender: mpsc::Sender<Reply>,
    receiver: mpsc::Receiver<Reply>,
    cancel: Arc<AtomicBool>,
}

impl Drop for Picker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Picker {
    pub(crate) fn opening(&self) -> bool {
        self.opening
    }

    fn filter(&mut self, query: &str) {
        let words: Vec<_> = query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        self.filtered = self
            .rows
            .iter()
            .enumerate()
            .filter(|(_, row)| {
                let fields = format!(
                    "{} {} {} {} {}",
                    row.entry.title,
                    row.entry.cwd,
                    row.entry.agent,
                    row.entry.source,
                    row.host.label
                )
                .to_lowercase();
                words.iter().all(|word| fields.contains(word))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
    }
}

impl HerdrWindow {
    pub(crate) fn open_agent_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        let search = cx.new(SearchInput::new);
        search.update(cx, |input, cx| {
            input.set_placeholder("Search tasks, projects, agents or devices...", cx);
            input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            window.focus(&input.focus, cx);
        });
        let subscription = cx.subscribe(
            &search,
            |this, search, _: &crate::search_input::Changed, cx| {
                if let Some(picker) = &mut this.menu.agent_sessions {
                    picker.filter(search.read(cx).text());
                }
                cx.notify();
            },
        );
        let (sender, receiver) = mpsc::channel();
        let mut picker = Picker {
            search,
            _subscription: subscription,
            rows: vec![],
            sources: vec![],
            filtered: vec![],
            selected: 0,
            remaining: 0,
            errors: vec![],
            watch: None,
            opening: false,
            scroll: ScrollHandle::new(),
            sender,
            receiver,
            cancel: Arc::new(AtomicBool::new(false)),
        };
        let mut hosts = vec![];
        for endpoint in self.endpoints.iter().filter(|e| e.enabled) {
            let target = &endpoint.connection.target;
            let Ok(host) = crate::teleport::host_for(target) else {
                continue;
            };
            if hosts.contains(&host) {
                continue;
            }
            hosts.push(host.clone());
            let mut cli = vec!["herdr".to_owned()];
            if let ConnectTarget::Session { name, .. } | ConnectTarget::Ssh { session: name, .. } =
                target
            {
                cli.extend(["--session".to_owned(), name.clone()]);
            }
            let source = Source {
                id: endpoint.id.clone(),
                label: endpoint.label.clone(),
                generation: endpoint.generation,
                host,
                cli,
            };
            let sender = picker.sender.clone();
            picker.sources.push(source.clone());
            let cancel = picker.cancel.clone();
            picker.remaining += 1;
            cx.background_executor()
                .spawn(async move {
                    let result = source.request(serde_json::json!({"operation": "list"}), &cancel);
                    let _ = sender.send(Reply::Listed(source, result));
                })
                .detach();
        }
        self.menu.agent_sessions = Some(picker);
        self.menu.page = Some(Page::AgentSessions);
        cx.notify();
    }

    pub(crate) fn poll_agent_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.menu.agent_sessions else {
            return;
        };
        let mut destination = None;
        while let Ok(reply) = picker.receiver.try_recv() {
            match reply {
                Reply::Listed(source, result) => {
                    picker.remaining = picker.remaining.saturating_sub(1);
                    match result {
                        Ok(listing) => {
                            for entry in listing.entries.into_iter().take(200) {
                                if !picker.rows.iter().any(|r| {
                                    r.entry.agent == entry.agent && r.entry.session == entry.session
                                }) {
                                    picker.rows.push(Row {
                                        host: source.clone(),
                                        entry,
                                    });
                                }
                            }
                        }
                        Err(error) => picker.errors.push(format!("{}: {error}", source.label)),
                    }
                    picker
                        .rows
                        .sort_by(|a, b| b.entry.modified.total_cmp(&a.entry.modified));
                    picker.filter(picker.search.read(cx).text());
                }
                Reply::Watched(result) => {
                    if let Some(watch) = &mut picker.watch {
                        watch.pending = false;
                        watch.next = Instant::now() + Duration::from_secs(3);
                        match result {
                            Ok((source, observation)) => {
                                if watch.observation.is_none()
                                    || picker.scroll.logical_scroll_bottom().1 <= gpui::px(1.)
                                {
                                    picker.scroll.scroll_to_bottom();
                                }
                                watch.row.host = source;
                                watch.observation = Some(observation);
                                watch.error = None;
                            }
                            Err(error) => {
                                watch.error = Some(error.to_string());
                                watch.observation = None;
                            }
                        }
                    }
                }
                Reply::Opened(result) => {
                    picker.opening = false;
                    if let Some(watch) = &mut picker.watch {
                        match result {
                            Ok((source, result)) => destination = Some((source, result.opened)),
                            Err(error) => {
                                watch.error = Some(error.to_string());
                                watch.observation = None;
                            }
                        }
                    }
                }
            }
            cx.notify();
        }
        if let Some(watch) = &mut picker.watch
            && !watch.pending
            && !picker.opening
            && Instant::now() >= watch.next
        {
            watch.pending = true;
            let row = watch.row.clone();
            let sources = picker.sources.clone();
            let sender = picker.sender.clone();
            let cancel = picker.cancel.clone();
            cx.background_executor()
                .spawn(async move {
                    let result = observe(&row, &sources, |source| {
                        source.request(
                            serde_json::json!({"operation":"watch", "key":row.entry}),
                            &cancel,
                        )
                    });
                    let _ = sender.send(Reply::Watched(result));
                })
                .detach();
        }
        if let Some((source, opened)) = destination {
            let valid = self
                .endpoints
                .iter()
                .any(|e| e.id == source.id && e.generation == source.generation);
            if valid {
                self.dismiss_menu(window, cx);
                self.navigate_endpoint(&source.id, NavigationTarget::Pane(&opened.pane_id), cx);
            } else if let Some(picker) = &mut self.menu.agent_sessions {
                picker
                    .errors
                    .push("The device changed. Refresh before opening this session.".into());
            }
        }
    }

    fn watch_agent_session(&mut self, row: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.menu.agent_sessions else {
            return;
        };
        let Some(row) = picker.rows.get(row).cloned() else {
            return;
        };
        picker.watch = Some(Watch {
            row,
            observation: None,
            pending: false,
            next: Instant::now(),
            error: None,
        });
        picker.scroll.set_offset(Point::default());
        window.focus(&self.menu.focus, cx);
        cx.notify();
    }

    fn continue_agent_session(&mut self, cx: &mut Context<Self>) {
        let Some(picker) = &mut self.menu.agent_sessions else {
            return;
        };
        let Some(watch) = &picker.watch else {
            return;
        };
        if picker.opening
            || !watch
                .observation
                .as_ref()
                .is_some_and(|o| o.state.can_open())
        {
            return;
        }
        let row = watch.row.clone();
        let Some(endpoint) = self.endpoints.iter().find(|e| e.id == row.host.id) else {
            return;
        };
        if endpoint.generation != row.host.generation {
            return;
        }
        picker.opening = true;
        let sources = picker.sources.clone();
        let sender = picker.sender.clone();
        let cancel = picker.cancel.clone();
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
                            "This session is not ready. Keep watching until it finishes.".into(),
                        ));
                    }
                    let result = source.request(
                        serde_json::json!({"operation":"open", "key":row.entry}),
                        &cancel,
                    )?;
                    Ok((source, result))
                })();
                let _ = sender.send(Reply::Opened(result));
            })
            .detach();
        cx.notify();
    }

    pub(crate) fn agent_sessions_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.menu.agent_sessions else {
            return;
        };
        if picker.watch.is_none()
            && (picker.search.read(cx).is_composing()
                || !matches!(
                    event.keystroke.key.as_str(),
                    "escape" | "up" | "down" | "enter"
                ))
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        if picker.opening {
            return;
        }
        if picker.watch.is_some() {
            // Watch mode deliberately has no keyboard action that can resume,
            // submit, steer or interrupt the worker, even after it finishes.
            if event.keystroke.key == "escape" {
                self.back_to_agent_sessions(window, cx);
            }
            return;
        }
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "up" => {
                picker.selected = picker.selected.saturating_sub(1);
                picker
                    .scroll
                    .scroll_to_item(picker.selected + usize::from(picker.remaining > 0));
            }
            "down" => {
                picker.selected =
                    (picker.selected + 1).min(picker.filtered.len().saturating_sub(1));
                picker
                    .scroll
                    .scroll_to_item(picker.selected + usize::from(picker.remaining > 0));
            }
            "enter" => {
                if let Some(row) = picker.filtered.get(picker.selected).copied() {
                    self.watch_agent_session(row, window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    fn back_to_agent_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // A new picker owns a new reply channel, so a late observation can
        // never become the readiness state of a different conversation.
        self.open_agent_sessions(window, cx);
    }
}
