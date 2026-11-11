//! The board as last published, and what each device last said.

use crate::{Result, devices::Endpoint};
use herdr_board::{AttentionLedger, Board, DeviceInput, Mark, Source};
use herdr_client::protocol::ClientShellSnapshot;
use std::{
    mem,
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

/// One device from when the hub began to follow it until it stops. A device
/// that is dropped and followed again is another one.
#[derive(Clone, Debug)]
pub(crate) struct Followed {
    pub(crate) endpoint: Endpoint,
    serial: u64,
}

pub(crate) struct Hub {
    state: Mutex<State>,
    changed: Condvar,
}

struct State {
    devices: Vec<Device>,
    followed: u64,
    ledger: AttentionLedger,
    /// The published board with its revision zeroed, to tell a change from a repeat.
    board: Board,
    published: Published,
}

#[derive(Clone)]
struct Device {
    endpoint: Endpoint,
    hops: Vec<String>,
    serial: u64,
    link: Link,
}

impl Hub {
    pub(crate) fn new() -> Result<Self> {
        // A client that outlives a restart must never see the revision fall,
        // so each run starts above anything an earlier run can have reached.
        let start = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis().try_into().unwrap_or(u64::MAX));
        Self::starting_at(start)
    }

    pub(crate) fn starting_at(revision: u64) -> Result<Self> {
        let mut state = State {
            devices: Vec::new(),
            followed: 0,
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

    /// Follow `endpoints` and nothing else, in that order. An endpoint that
    /// is unchanged keeps what its connection reported. Returns the devices
    /// that still need a connection.
    pub(crate) fn set_devices(&self, endpoints: Vec<Endpoint>) -> Result<Vec<Followed>> {
        let mut state = self.lock();
        let mut added = Vec::new();
        let mut devices = Vec::with_capacity(endpoints.len());
        for endpoint in endpoints {
            let kept = state
                .devices
                .iter()
                .find(|device| device.endpoint == endpoint);
            if let Some(device) = kept {
                devices.push(device.clone());
                continue;
            }
            let serial = state.followed;
            state.followed += 1;
            added.push(Followed {
                endpoint: endpoint.clone(),
                serial,
            });
            devices.push(Device {
                hops: endpoint.route.hops(),
                endpoint,
                serial,
                link: Link::default(),
            });
        }
        let before = mem::replace(&mut state.devices, devices);
        if let Err(error) = self.republish(&mut state) {
            // The caller connects only what is returned, so a device must not
            // stay followed without being returned.
            state.devices = before;
            return Err(error);
        }
        Ok(added)
    }

    pub(crate) fn follows(&self, device: &Followed) -> bool {
        self.lock()
            .devices
            .iter()
            .any(|known| known.serial == device.serial)
    }

    /// Record what a device's connection reported and publish the board if
    /// that changed it. A device no longer followed is ignored.
    pub(crate) fn update(&self, device: &Followed, change: impl FnOnce(&mut Link)) -> Result<()> {
        let mut state = self.lock();
        let Some(known) = state
            .devices
            .iter_mut()
            .find(|known| known.serial == device.serial)
        else {
            return Ok(());
        };
        change(&mut known.link);
        self.republish(&mut state)
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

    fn republish(&self, state: &mut State) -> Result<()> {
        let board = state.build();
        if board != state.board {
            state.board = board;
            state.published.revision += 1;
            state.publish()?;
            self.changed.notify_all();
        }
        Ok(())
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
            .map(|device| DeviceInput {
                source: Source {
                    id: &device.endpoint.id,
                    label: &device.endpoint.label,
                    enabled: device.endpoint.enabled,
                    visible: true,
                    connected: device.link.connected,
                    snapshot: device.link.snapshot.as_deref(),
                },
                connecting: device.link.connecting,
                detail: device.link.detail.as_deref(),
                route: &device.hops,
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
