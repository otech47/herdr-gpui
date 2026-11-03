//! Saved agent conversations, with a read-only view before explicit continuation.
#[cfg(any(test, feature = "integration-test"))]
mod fixtures;
mod render;
mod tab;
pub(crate) use tab::{SavedSession, SessionTab};
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
        let started = Instant::now();
        let result = (|| {
            let output = self.host.capture(&script, cancel)?;
            let value: serde_json::Value = serde_json::from_slice(&output)?;
            if let Some(error) = value["error"].as_str() {
                return Err(Error::Refused(error.to_owned()));
            }
            Ok(serde_json::from_value(value)?)
        })();
        tracing::debug!(operation = request["operation"].as_str().unwrap_or("unknown"),
            endpoint = %self.id, elapsed_ms = started.elapsed().as_millis() as u64,
            success = result.is_ok(), "Agent session request completed");
        result
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct Entry {
    agent: String,
    session: String,
    title: String,
    cwd: String,
    source: String,
    modified: u64,
    #[serde(default)]
    running: bool,
}

impl Entry {
    fn background(&self) -> bool {
        matches!(self.source.as_str(), "Jarvis" | "Roboz")
    }
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
    if !matches!(row.entry.agent.as_str(), "claude" | "kimi") {
        return Ok((row.host.clone(), probe(&row.host)?));
    }
    // These stores sync between devices. A stopped local copy cannot grant
    // permission to resume while the same conversation runs on another host.
    let mut selected: Option<(Source, Observation)> = None;
    let mut unavailable = None;
    for source in std::iter::once(&row.host).chain(sources.iter().filter(|s| s.id != row.host.id)) {
        match probe(source) {
            Ok(observation) => {
                if selected.as_ref().is_none_or(|(_, previous)| {
                    observation.state.precedence() > previous.state.precedence()
                }) {
                    selected = Some((source.clone(), observation));
                }
            }
            Err(error) => unavailable = Some((source.label.clone(), error)),
        }
    }
    match (selected, unavailable) {
        (Some((_, observation)), Some((label, _))) if observation.state.can_open() => {
            Err(Error::Refused(format!(
                "Cannot verify this synced session on {label}. Continuation stays disabled until that device is reachable."
            )))
        }
        (Some(selected), _) => Ok(selected),
        (None, Some((_, error))) => Err(error),
        (None, None) => Err(Error::Refused(
            "No device could locate this session.".into(),
        )),
    }
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

    fn precedence(self) -> u8 {
        match self {
            Self::Missing => 0,
            Self::Ready => 1,
            Self::Attached => 2,
            Self::Unknown => 3,
            Self::Watch => 4,
        }
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
    scroll: ScrollHandle,
    sender: mpsc::Sender<Reply>,
    receiver: mpsc::Receiver<Reply>,
    cancel: Arc<AtomicBool>,
    refreshed: Instant,
}

impl Drop for Picker {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}

impl Picker {
    fn filter(&mut self, query: &str) {
        let words: Vec<_> = query
            .to_lowercase()
            .split_whitespace()
            .map(str::to_owned)
            .collect();
        let mut seen = std::collections::HashSet::new();
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
                    && seen.insert((row.entry.agent.clone(), row.entry.session.clone()))
            })
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
    }
}

impl HerdrWindow {
    fn session_sources(&self) -> Vec<Source> {
        let mut sources: Vec<Source> = vec![];
        for endpoint in self.endpoints.iter().filter(|e| e.enabled) {
            let target = &endpoint.connection.target;
            let Ok(host) = crate::teleport::host_for(target) else {
                continue;
            };
            if sources.iter().any(|source| source.host == host) {
                continue;
            }
            let mut cli = vec!["herdr".to_owned()];
            if let ConnectTarget::Session { name, .. } | ConnectTarget::Ssh { session: name, .. } =
                target
            {
                cli.extend(["--session".to_owned(), name.clone()]);
            }
            sources.push(Source {
                id: endpoint.id.clone(),
                label: endpoint.label.clone(),
                generation: endpoint.generation,
                host,
                cli,
            });
        }
        sources
    }

    pub(crate) fn open_agent_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open_menu(window, cx) {
            return;
        }
        if self.agent_sessions.is_none() {
            let search = cx.new(SearchInput::new);
            search.update(cx, |input, cx| {
                input.set_placeholder("Find a task, project or agent...", cx);
                input.set_appearance(self.config.ui.clone(), self.theme.clone(), cx);
            });
            let subscription = cx.subscribe(
                &search,
                |this, search, _: &crate::search_input::Changed, cx| {
                    if let Some(picker) = &mut this.agent_sessions {
                        picker.filter(search.read(cx).text());
                        picker.scroll.set_offset(Point::default());
                    }
                    cx.notify();
                },
            );
            let (sender, receiver) = mpsc::channel();
            self.agent_sessions = Some(Picker {
                search,
                _subscription: subscription,
                rows: vec![],
                sources: vec![],
                filtered: vec![],
                selected: 0,
                remaining: 0,
                errors: vec![],
                scroll: ScrollHandle::new(),
                sender,
                receiver,
                cancel: Arc::new(AtomicBool::new(false)),
                refreshed: Instant::now() - Duration::from_secs(60),
            });
        }
        self.menu.page = Some(Page::AgentSessions);
        if let Some(picker) = &self.agent_sessions {
            let focus = picker.search.read(cx).focus.clone();
            window.focus(&focus, cx);
            if picker.refreshed.elapsed() > Duration::from_secs(3) {
                self.refresh_agent_sessions(cx);
            }
        }
        cx.notify();
    }

    fn refresh_agent_sessions(&mut self, cx: &mut Context<Self>) {
        let sources = self.session_sources();
        let Some(picker) = &mut self.agent_sessions else {
            return;
        };
        if picker.remaining > 0 {
            return;
        }
        picker.sources = sources.clone();
        let previous_len = picker.rows.len();
        picker
            .rows
            .retain(|row| sources.iter().any(|source| source.id == row.host.id));
        if picker.rows.len() != previous_len {
            picker.filter(picker.search.read(cx).text());
        }
        picker.errors.clear();
        picker.remaining = sources.len();
        picker.refreshed = Instant::now();
        for source in sources {
            let sender = picker.sender.clone();
            let cancel = picker.cancel.clone();
            cx.background_executor()
                .spawn(async move {
                    let result = source.request(serde_json::json!({"operation":"list"}), &cancel);
                    let _ = sender.send(Reply::Listed(source, result));
                })
                .detach();
        }
        cx.notify();
    }

    pub(crate) fn poll_agent_sessions(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(picker) = &mut self.agent_sessions {
            while let Ok(Reply::Listed(source, result)) = picker.receiver.try_recv() {
                picker.remaining = picker.remaining.saturating_sub(1);
                let selected = picker
                    .filtered
                    .get(picker.selected)
                    .and_then(|i| picker.rows.get(*i))
                    .map(|r| r.entry.session.clone());
                match result {
                    Ok(listing) => {
                        picker.rows.retain(|row| row.host.id != source.id);
                        for entry in listing.entries.into_iter().take(300) {
                            picker.rows.push(Row {
                                host: source.clone(),
                                entry,
                            });
                        }
                    }
                    Err(error) => {
                        tracing::warn!(operation = "list", %error, "Agent session request failed");
                        picker.errors.push(format!(
                            "{} is unavailable. Retry to reconnect.",
                            source.label
                        ));
                    }
                }
                picker.rows.sort_by(|a, b| {
                    b.entry
                        .background()
                        .cmp(&a.entry.background())
                        .then_with(|| b.entry.modified.cmp(&a.entry.modified))
                });
                picker.filter(picker.search.read(cx).text());
                if let Some(selected) = selected {
                    picker.selected = picker
                        .filtered
                        .iter()
                        .position(|i| picker.rows[*i].entry.session == selected)
                        .unwrap_or(0);
                }
                cx.notify();
            }
        }
        self.poll_session_tabs(window, cx);
    }

    pub(crate) fn agent_sessions_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = &mut self.agent_sessions else {
            return;
        };
        if picker.search.read(cx).is_composing()
            || !matches!(
                event.keystroke.key.as_str(),
                "escape" | "up" | "down" | "enter"
            )
        {
            return;
        }
        cx.stop_propagation();
        window.prevent_default();
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "up" | "down" => {
                picker.selected = if event.keystroke.key == "up" {
                    picker.selected.saturating_sub(1)
                } else {
                    (picker.selected + 1).min(picker.filtered.len().saturating_sub(1))
                };
                picker.scroll.scroll_to_item(picker.selected);
            }
            "enter" => {
                if let Some(row) = picker.filtered.get(picker.selected).copied() {
                    self.open_agent_session(row, window, cx);
                }
            }
            _ => {}
        }
        cx.notify();
    }
}
