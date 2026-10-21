//! Tabs whose agent left a shell running while it waits: a background
//! command, a watch on CI, a timer. The tabs list marks such a tab as
//! monitoring, between done and idle, so an idle tab with nothing pending
//! tells apart from one that is still responsible for something.
//!
//! Every host has its own worker thread listing its processes every few
//! seconds through one long-lived `sh`: a local one for this machine, the
//! host's own over SSH for a remote one. The UI thread only starts and stops
//! workers and takes their answers on later ticks, as the port scanners do.

mod scan;

#[cfg(test)]
mod tests;

use crate::{
    Error, Result,
    listening_ports::Daemon,
    system_load::Stop,
    usage::{Host, Shell},
};
pub(crate) use scan::{Tab, Watched, Watchers, parse};
use std::{
    collections::HashMap,
    sync::{Arc, mpsc},
    thread,
    time::{Duration, Instant},
};

/// Background work starts and ends at human pace; listing every process
/// with its environment is not free on a busy machine.
const INTERVAL: Duration = Duration::from_secs(5);
/// A host that could not be listed is tried again after this long.
const RETRY: Duration = Duration::from_secs(30);
const STEP_TIMEOUT: Duration = Duration::from_secs(15);
/// Hosts are few; an unbounded endpoint list still cannot start more workers.
const HOST_LIMIT: usize = 16;

/// Whether a process named `name` is the agent the daemon labels `agent`.
/// Cursor's executable is `agent`; a label may be followed by a suffix, as
/// `cursor-agent` and `claude-code` are.
fn runs(agent: &str, name: &str) -> bool {
    let name = name.strip_suffix(".exe").unwrap_or(name);
    name == agent
        || name
            .strip_prefix(agent)
            .is_some_and(|rest| rest.starts_with('-'))
        || (agent == "cursor" && name == "agent")
}

fn open(host: &Host) -> Result<Shell> {
    #[cfg(windows)]
    if *host == Host::Local {
        return Err(Error::MonitoringUnsupported);
    }
    crate::listening_ports::open(host).map_err(failed)
}

fn scan(shell: &mut Shell) -> Result<Watchers> {
    let output = shell.run(scan::COMMAND, STEP_TIMEOUT).map_err(failed)?;
    parse(&output.stdout)
}

fn failed(error: Error) -> Error {
    Error::Monitoring(Box::new(error))
}

struct Worker {
    stop: Arc<Stop>,
    results: mpsc::Receiver<Result<Watchers>>,
}

impl Drop for Worker {
    /// The thread ends after its current scan; nothing waits for it here.
    fn drop(&mut self) {
        self.stop.stop();
    }
}

fn spawn(host: Host) -> Option<Worker> {
    let stop = Arc::new(Stop::default());
    let (sender, results) = mpsc::sync_channel(2);
    let shared = stop.clone();
    let spawned = thread::Builder::new()
        .name("herdr-monitoring".into())
        .spawn(move || run(&host, &shared, &sender));
    match spawned {
        Ok(_) => Some(Worker { stop, results }),
        Err(error) => {
            tracing::warn!(category = "monitoring", %error, "could not start a process scanner");
            None
        }
    }
}

fn run(host: &Host, stop: &Stop, results: &mpsc::SyncSender<Result<Watchers>>) {
    let mut shell: Option<Shell> = None;
    loop {
        let opened = match shell.take() {
            Some(shell) => Ok(shell),
            None => open(host),
        };
        let result = opened.and_then(|mut opened| {
            let watchers = scan(&mut opened);
            // A failed step leaves the shell unusable; a missing tool does not.
            if !matches!(watchers, Err(Error::Monitoring(_))) {
                shell = Some(opened);
            }
            watchers
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

/// One host's latest scan.
#[derive(Default)]
struct Reading {
    watchers: Watchers,
    /// Why the latest scan failed, as last logged; the last good scan stays
    /// shown.
    error: Option<String>,
}

impl Reading {
    /// Whether anything shown changed.
    fn apply(&mut self, result: Result<Watchers>) -> bool {
        match result {
            Ok(watchers) => {
                self.error = None;
                if watchers == self.watchers {
                    return false;
                }
                self.watchers = watchers;
                true
            }
            Err(error) => {
                // The display boundary: the cause says what went wrong.
                let mut text = error.to_string();
                let mut source = std::error::Error::source(&error);
                while let Some(cause) = source {
                    text.push(' ');
                    text.push_str(&cause.to_string());
                    source = cause.source();
                }
                // Nothing shown changes; the reason is logged once, not every retry.
                if self.error.as_deref() != Some(text.as_str()) {
                    tracing::warn!(category = "monitoring", error = %text, "could not list a host's processes");
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

/// Every scanned host's watched tabs, keyed by host.
#[derive(Default)]
pub(crate) struct Monitoring {
    monitors: HashMap<Host, Monitor>,
}

impl Monitoring {
    /// Whether tab `tab` of `daemon`, whose agent the daemon labels `agent`,
    /// has a shell running under that agent or a disowned timer.
    pub fn watching(&self, daemon: &Daemon, tab: &str, agent: &str) -> bool {
        let Some(monitor) = self.monitors.get(daemon.host()) else {
            return false;
        };
        monitor.reading.watchers.iter().any(|(owner, watched)| {
            owner.id == tab
                && daemon.owns(&owner.socket)
                && (watched.disowned || watched.under.iter().any(|name| runs(agent, name)))
        })
    }

    /// Shows `watchers` for `host` as if a scan had found them, with no worker.
    #[cfg(any(test, feature = "integration-test"))]
    pub(crate) fn seed(&mut self, host: Host, watchers: Watchers) {
        self.monitors.insert(
            host,
            Monitor {
                worker: None,
                reading: Reading {
                    watchers,
                    error: None,
                },
            },
        );
    }

    /// Scans exactly `hosts` (at most [`HOST_LIMIT`]): starts workers for new
    /// ones, stops and forgets the rest, and takes finished scans. Returns
    /// whether anything shown changed.
    pub fn poll(&mut self, hosts: impl IntoIterator<Item = Host>) -> bool {
        let mut wanted: Vec<Host> = Vec::new();
        for host in hosts {
            if wanted.len() == HOST_LIMIT {
                break;
            }
            // Several sessions on one machine share its scan.
            if !wanted.contains(&host) {
                wanted.push(host);
            }
        }
        let mut changed = false;
        self.monitors.retain(|host, monitor| {
            let keep = wanted.contains(host);
            changed |= !keep && !monitor.reading.watchers.is_empty();
            keep
        });
        for host in wanted {
            self.monitors
                .entry(host)
                .or_insert_with_key(|host| Monitor {
                    worker: spawn(host.clone()),
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
}
