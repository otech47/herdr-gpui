//! The model and reasoning effort each agent runs on, for the sidebar's hover
//! cards. One worker thread per daemon runs `scripts/agent_models.py` on the
//! agents' host through a long-lived `sh`, as the port scanners do.

mod render;

#[cfg(test)]
mod tests;

use crate::{
    Error, Result,
    system_load::Stop,
    usage::{Host, Shell},
};
use herdr_client::{ConnectTarget, shell_quote};
pub(crate) use render::tooltip;
use serde::Deserialize;
use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

const HELPER: &str = include_str!("../../../scripts/agent_models.py");
/// A switched model shows within this long; an unchanged transcript costs
/// the helper one `stat`.
const INTERVAL: Duration = Duration::from_secs(10);
const RETRY: Duration = Duration::from_secs(30);
const STEP_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const SHOW_DELAY: Duration = Duration::ZERO;
/// Daemons are few; an unbounded endpoint list still cannot start more workers.
const SOURCE_LIMIT: usize = 16;
/// Bytes of a refusal kept for the log.
const REFUSAL_LIMIT: usize = 120;

/// A daemon whose agents are read: the host they run on, and the session
/// its CLI names, none for the host's default.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Source {
    host: Host,
    session: Option<String>,
}

impl Source {
    /// None for a daemon the release CLI cannot be pointed at: one behind a
    /// custom socket, or a development installation's session.
    pub(crate) fn of(target: &ConnectTarget) -> Option<Self> {
        match target {
            ConnectTarget::Local => Some(Self {
                host: Host::Local,
                session: None,
            }),
            ConnectTarget::Session {
                name,
                development: false,
            } => Some(Self {
                host: Host::Local,
                session: Some(name.clone()),
            }),
            ConnectTarget::Ssh { target, session } => Some(Self {
                host: Host::Ssh(target.clone()),
                session: Some(session.clone()),
            }),
            ConnectTarget::Session { .. } | ConnectTarget::Socket(_) => None,
        }
    }

    fn cli(&self) -> Vec<&str> {
        let mut cli = vec!["herdr"];
        if let Some(session) = &self.session {
            cli.extend(["--session", session]);
        }
        cli
    }
}

/// What one agent runs on, as people say it: `Opus 5.5` and `xhigh`.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub(crate) struct Model {
    pub(crate) model: String,
    /// Empty when the transcript names none.
    #[serde(default)]
    pub(crate) effort: String,
}

type Panes = HashMap<String, Model>;

#[derive(Deserialize)]
struct Answer {
    panes: Panes,
    /// What the helper learned about each transcript, handed back next time.
    known: serde_json::Value,
}

fn ask(shell: &mut Shell, source: &Source, known: &serde_json::Value) -> Result<Answer> {
    let request = serde_json::json!({ "herdr": source.cli(), "known": known });
    let step = format!(
        "python3 -c {} {}",
        shell_quote(HELPER),
        shell_quote(&request.to_string())
    );
    let output = shell.run(&step, STEP_TIMEOUT).map_err(failed)?;
    if !output.success {
        let text = output.stdout.trim();
        let end = text.floor_char_boundary(REFUSAL_LIMIT);
        return Err(Error::AgentModelsRefused(text[..end].to_owned()));
    }
    serde_json::from_str(&output.stdout).map_err(Error::AgentModelsAnswer)
}

fn failed(error: Error) -> Error {
    Error::AgentModels(Box::new(error))
}

struct Worker {
    stop: Arc<Stop>,
    results: mpsc::Receiver<Result<Panes>>,
}

impl Drop for Worker {
    /// The thread ends after its current pass; nothing waits for it here.
    fn drop(&mut self) {
        self.stop.stop();
    }
}

fn spawn(source: Source) -> Option<Worker> {
    let stop = Arc::new(Stop::default());
    let (sender, results) = mpsc::sync_channel(2);
    let shared = stop.clone();
    let spawned = thread::Builder::new()
        .name("herdr-agent-models".into())
        .spawn(move || run(&source, &shared, &sender));
    match spawned {
        Ok(_) => Some(Worker { stop, results }),
        Err(error) => {
            tracing::warn!(category = "agent-models", %error, "could not start an agent model reader");
            None
        }
    }
}

fn run(source: &Source, stop: &Stop, results: &mpsc::SyncSender<Result<Panes>>) {
    let mut shell: Option<Shell> = None;
    let mut known = serde_json::Value::Null;
    loop {
        let opened = match shell.take() {
            Some(shell) => Ok(shell),
            None => crate::listening_ports::open(&source.host).map_err(failed),
        };
        let result = opened.and_then(|mut opened| {
            let answer = ask(&mut opened, source, &known);
            // A failed step leaves the shell unusable; a refusal does not.
            if !matches!(answer, Err(Error::AgentModels(_))) {
                shell = Some(opened);
            }
            answer
        });
        let result = result.map(|answer| {
            known = answer.known;
            answer.panes
        });
        let due = Instant::now() + if result.is_ok() { INTERVAL } else { RETRY };
        match results.try_send(result) {
            // The UI is behind; it will take the next one.
            Ok(()) | Err(mpsc::TrySendError::Full(_)) => {}
            Err(mpsc::TrySendError::Disconnected(_)) => return,
        }
        if !stop.wait(due) {
            return;
        }
    }
}

#[derive(Default)]
struct Reading {
    panes: Panes,
    /// Why the latest pass failed, as last logged; the last good answer
    /// stays shown.
    error: Option<String>,
}

impl Reading {
    /// Whether anything shown changed.
    fn apply(&mut self, result: Result<Panes>) -> bool {
        match result {
            Ok(panes) => {
                self.error = None;
                if panes == self.panes {
                    return false;
                }
                self.panes = panes;
                true
            }
            Err(error) => {
                let mut text = error.to_string();
                let mut source = std::error::Error::source(&error);
                while let Some(cause) = source {
                    text.push(' ');
                    text.push_str(&cause.to_string());
                    source = cause.source();
                }
                if self.error.as_deref() != Some(text.as_str()) {
                    tracing::warn!(category = "agent-models", error = %text, "could not read agent models");
                    self.error = Some(text);
                }
                false
            }
        }
    }
}

#[derive(Default)]
struct Monitor {
    /// None once its thread could not start or has ended.
    worker: Option<Worker>,
    reading: Reading,
}

#[derive(Default)]
pub(crate) struct AgentModels {
    monitors: HashMap<Source, Monitor>,
}

impl AgentModels {
    /// What the agent in `pane` of the daemon behind `target` runs on; None
    /// before the first answer or when its transcript names no model.
    pub fn get(&self, target: &ConnectTarget, pane: &str) -> Option<&Model> {
        let source = Source::of(target)?;
        self.monitors.get(&source)?.reading.panes.get(pane)
    }

    /// Reads exactly `sources` (at most [`SOURCE_LIMIT`]): starts workers for
    /// new ones, stops and forgets the rest, and takes finished answers.
    /// Returns whether anything shown changed.
    pub fn poll(&mut self, sources: impl IntoIterator<Item = Source>) -> bool {
        let mut wanted: Vec<Source> = Vec::new();
        for source in sources {
            if wanted.len() == SOURCE_LIMIT {
                break;
            }
            if !wanted.contains(&source) {
                wanted.push(source);
            }
        }
        let mut changed = false;
        self.monitors.retain(|source, monitor| {
            let keep = wanted.contains(source);
            changed |= !keep && !monitor.reading.panes.is_empty();
            keep
        });
        for source in wanted {
            self.monitors
                .entry(source)
                .or_insert_with_key(|source| Monitor {
                    worker: spawn(source.clone()),
                    reading: Reading::default(),
                });
        }
        for monitor in self.monitors.values_mut() {
            while let Some(worker) = &monitor.worker {
                match worker.results.try_recv() {
                    Ok(result) => changed |= monitor.reading.apply(result),
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => monitor.worker = None,
                }
            }
        }
        changed
    }

    #[cfg(test)]
    pub(crate) fn seed(&mut self, source: Source, panes: Panes) {
        let reading = Reading { panes, error: None };
        self.monitors.insert(
            source,
            Monitor {
                worker: None,
                reading,
            },
        );
    }
}
