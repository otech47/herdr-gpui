//! The board as last published, and what each device last said.

use crate::{Result, args::Endpoint};
use herdr_board::{AttentionLedger, Board, DeviceInput, Mark, Source};
use herdr_client::protocol::ClientShellSnapshot;
use std::{
    sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

/// What a device's connection last reported.
#[derive(Clone, Debug, Default)]
pub(crate) struct Link {
    pub(crate) connected: bool,
    pub(crate) connecting: bool,
    pub(crate) detail: Option<String>,
    pub(crate) snapshot: Option<Arc<ClientShellSnapshot>>,
}

/// A board ready to send.
#[derive(Clone, Debug)]
pub(crate) struct Published {
    pub(crate) revision: u64,
    pub(crate) json: Arc<str>,
}

pub(crate) struct Hub {
    state: Mutex<State>,
    changed: Condvar,
}

struct State {
    devices: Vec<(Endpoint, Link)>,
    ledger: AttentionLedger,
    /// The published board with its revision zeroed, to tell a change from a repeat.
    board: Board,
    published: Published,
}

impl Hub {
    pub(crate) fn new(endpoints: Vec<Endpoint>) -> Result<Self> {
        // A client that outlives a restart must never see the revision fall,
        // so each run starts above anything an earlier run can have reached.
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis().try_into().unwrap_or(u64::MAX));
        Self::starting_at(endpoints, start)
    }

    pub(crate) fn starting_at(endpoints: Vec<Endpoint>, revision: u64) -> Result<Self> {
        let mut state = State {
            devices: endpoints
                .into_iter()
                .map(|endpoint| (endpoint, Link::default()))
                .collect(),
            ledger: AttentionLedger::default(),
            board: Board::default(),
            published: Published {
                revision,
                json: Arc::from(""),
            },
        };
        state.board = state.build();
        state.publish()?;
        Ok(Self {
            state: Mutex::new(state),
            changed: Condvar::new(),
        })
    }

    pub(crate) fn current(&self) -> Published {
        self.lock().published.clone()
    }

    /// Record what a device's connection reported and publish the board if
    /// that changed it.
    pub(crate) fn update(&self, device: usize, change: impl FnOnce(&mut Link)) -> Result<()> {
        let mut state = self.lock();
        let Some((_, link)) = state.devices.get_mut(device) else {
            return Ok(());
        };
        change(link);
        let board = state.build();
        if board != state.board {
            state.board = board;
            state.published.revision += 1;
            state.publish()?;
            self.changed.notify_all();
        }
        Ok(())
    }

    /// The board once its revision passes `after`, or `None` when `wait`
    /// runs out first.
    pub(crate) fn newer_than(&self, after: u64, wait: Duration) -> Option<Published> {
        let deadline = Instant::now() + wait;
        let mut state = self.lock();
        loop {
            if state.published.revision > after {
                return Some(state.published.clone());
            }
            let left = deadline.checked_duration_since(Instant::now())?;
            state = self
                .changed
                .wait_timeout(state, left)
                .unwrap_or_else(PoisonError::into_inner)
                .0;
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl State {
    fn build(&mut self) -> Board {
        let inputs: Vec<DeviceInput<'_>> = self
            .devices
            .iter()
            .map(|(endpoint, link)| DeviceInput {
                source: Source {
                    id: &endpoint.id,
                    label: &endpoint.label,
                    enabled: endpoint.enabled,
                    visible: true,
                    connected: link.connected,
                    snapshot: link.snapshot.as_deref(),
                },
                connecting: link.connecting,
                detail: link.detail.as_deref(),
            })
            .collect();
        let sources: Vec<Source<'_>> = inputs.iter().map(|input| input.source).collect();
        // Only the daemon's own status is known here: used-up plans and
        // monitored shells are the desktop app's to detect.
        let mark = |_: usize, _: Option<&_>, status| Mark::Status(status);
        herdr_board::settle(&sources, &mut self.ledger, mark, |_, _| false);
        herdr_board::build(&inputs, &self.ledger, mark, &[])
    }

    fn publish(&mut self) -> Result<()> {
        let board = Board {
            revision: self.published.revision,
            ..self.board.clone()
        };
        self.published.json = Arc::from(board.to_json()?);
        Ok(())
    }
}
