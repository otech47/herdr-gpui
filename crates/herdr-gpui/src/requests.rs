//! Requests sent to agents from outside Herdr, such as a message on a phone,
//! read from a JSON list that a server keeps at `requests_url`. Each request
//! names the tab of the agent session working on it.
use serde::Deserialize;
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

const EVERY: Duration = Duration::from_secs(20);
const LIMIT: u64 = 1 << 20;

#[derive(Clone, Debug, Deserialize, PartialEq)]
pub(crate) struct Request {
    pub id: String,
    pub created: f64,
    #[serde(default)]
    pub text: String,
    pub state: String,
    #[serde(default)]
    pub summary: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub host_name: String,
    #[serde(default)]
    pub tab_id: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
}

impl Request {
    /// Neither reported done by its agent nor closed.
    pub(crate) fn open(&self) -> bool {
        !matches!(self.state.as_str(), "done" | "closed")
    }

    /// The agent asked for an answer, or the request never reached one.
    pub(crate) fn wants_you(&self) -> bool {
        matches!(self.state.as_str(), "blocked" | "failed")
    }

    /// The line under the request: the agent's report, else why it failed.
    pub(crate) fn outcome(&self) -> Option<&str> {
        self.summary.as_deref().or(self.error.as_deref())
    }
}

#[derive(Deserialize)]
struct Listing {
    requests: Vec<Request>,
    #[serde(default)]
    error: Option<String>,
}

#[derive(Default)]
pub(crate) struct Requests {
    url: Option<String>,
    pub(crate) items: Vec<Request>,
    /// Why the list could not be read, or why its server cannot read requests.
    pub(crate) error: Option<String>,
    pending: Option<Receiver<Result<Listing, String>>>,
    next: Option<Instant>,
}

impl Requests {
    pub(crate) fn new(url: Option<String>) -> Self {
        Self {
            url: url.filter(|url| !url.is_empty()),
            ..Self::default()
        }
    }

    pub(crate) fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    pub(crate) fn enabled(&self) -> bool {
        self.url.is_some()
    }

    pub(crate) fn set_url(&mut self, url: Option<String>) {
        let url = url.filter(|url| !url.is_empty());
        if url != self.url {
            *self = Self::new(url);
        }
    }

    pub(crate) fn open_count(&self) -> usize {
        self.items.iter().filter(|r| r.open()).count()
    }

    /// Asks for the list on the next poll.
    pub(crate) fn refresh(&mut self) {
        self.next = None;
    }

    /// Takes a finished read, and starts one when it is due. True when what
    /// the list shows changed.
    pub(crate) fn poll(&mut self, now: Instant) -> bool {
        self.poll_with(now, fetch)
    }

    fn poll_with(
        &mut self,
        now: Instant,
        read: impl FnOnce(&str) -> Result<Listing, String> + Send + 'static,
    ) -> bool {
        let Some(url) = self.url.clone() else {
            return false;
        };
        let mut changed = false;
        if let Some(pending) = &self.pending {
            match pending.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    let (items, error) = match result {
                        Ok(listing) => (listing.requests, listing.error),
                        Err(error) => (self.items.clone(), Some(error)),
                    };
                    changed = items != self.items || error != self.error;
                    self.items = items;
                    self.error = error;
                }
                Err(TryRecvError::Empty) => return false,
                Err(TryRecvError::Disconnected) => self.pending = None,
            }
        }
        if self.next.is_none_or(|next| now >= next) {
            self.next = Some(now + EVERY);
            let (tx, rx) = mpsc::sync_channel(1);
            let spawned = thread::Builder::new()
                .name("herdr-requests".into())
                .spawn(move || {
                    let _ = tx.send(read(&url));
                });
            if spawned.is_ok() {
                self.pending = Some(rx);
            }
        }
        changed
    }
}

fn fetch(url: &str) -> Result<Listing, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .build()
        .into();
    let mut response = agent.get(url).call().map_err(|e| e.to_string())?;
    let body = response
        .body_mut()
        .with_config()
        .limit(LIMIT)
        .read_to_vec()
        .map_err(|e| e.to_string())?;
    serde_json::from_slice(&body).map_err(|e| e.to_string())
}

/// How long ago `created`, in seconds since the epoch, was: "4m", "3h", "2d".
pub(crate) fn age(created: f64, now: f64) -> String {
    let seconds = (now - created).max(0.) as u64;
    match seconds {
        0..60 => "now".into(),
        60..3600 => format!("{}m", seconds / 60),
        3600..86400 => format!("{}h", seconds / 3600),
        _ => format!("{}d", seconds / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(id: &str, state: &str) -> Request {
        Request {
            id: id.into(),
            created: 0.,
            text: "fix the build".into(),
            state: state.into(),
            summary: None,
            error: None,
            host_name: "Studio".into(),
            tab_id: Some("w2:t13".into()),
            title: None,
        }
    }

    fn settle(requests: &mut Requests, now: Instant) -> bool {
        for _ in 0..200 {
            if requests.pending.is_none() {
                break;
            }
            if requests.poll_with(now, |_| unreachable!()) {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn a_list_without_an_address_never_reads() {
        let mut requests = Requests::new(Some(String::new()));
        assert!(!requests.enabled());
        assert!(!requests.poll_with(Instant::now(), |_| unreachable!()));
    }

    #[test]
    fn a_read_replaces_the_list_and_a_failed_one_keeps_it() {
        let now = Instant::now();
        let mut requests = Requests::new(Some("http://jarvis/v1/whatsapp/requests".into()));
        requests.poll_with(now, |_| {
            Ok(Listing {
                requests: vec![request("a", "working"), request("b", "done")],
                error: None,
            })
        });
        assert!(settle(&mut requests, now));
        assert_eq!(requests.open_count(), 1);
        requests.refresh();
        requests.poll_with(now, |_| Err("connection refused".into()));
        assert!(settle(&mut requests, now));
        assert_eq!(requests.items.len(), 2);
        assert_eq!(requests.error.as_deref(), Some("connection refused"));
    }

    #[test]
    fn reads_wait_for_their_interval() {
        let now = Instant::now();
        let mut requests = Requests::new(Some("http://jarvis".into()));
        requests.poll_with(now, |_| Err("down".into()));
        settle(&mut requests, now);
        requests.poll_with(now + Duration::from_secs(5), |_| unreachable!());
        assert!(requests.pending.is_none());
        requests.poll_with(now + EVERY, |_| Err("down".into()));
        assert!(requests.pending.is_some());
    }

    #[test]
    fn the_server_list_parses_with_fields_this_app_ignores() -> anyhow::Result<()> {
        let listing: Listing = serde_json::from_str(
            r#"{"requests": [{"id": "2540df84b1004641", "created": 1795017303.0, "text": "car report",
                "state": "blocked", "summary": "Which car?", "error": null, "host": "oz-studio",
                "host_name": "Studio", "workspace_id": "w2", "tab_id": "w2:t13", "pane_id": "w2:p13",
                "title": "car", "agent_state": "idle"}], "error": null}"#,
        )?;
        let first = &listing.requests[0];
        assert!(first.wants_you() && first.open());
        assert_eq!(first.outcome(), Some("Which car?"));
        Ok(())
    }

    #[test]
    fn ages_read_in_the_largest_whole_unit() {
        assert_eq!(age(100., 130.), "now");
        assert_eq!(age(0., 600.), "10m");
        assert_eq!(age(0., 7200.), "2h");
        assert_eq!(age(0., 200_000.), "2d");
    }
}
