//! Where each tab sits under the attention sort: its mark's rank, and within
//! a rank the tab that reached it last first, unless a row was dragged
//! within its rank, which holds the rank in the dropped order. A held tab
//! keeps its row, so the list never moves under the row the user is on.

use crate::Mark;
use herdr_protocol::AgentStatus;
use std::collections::HashMap;

/// A tab's place in the attention sort: the rank it holds, and the tick it
/// took it on. Zero is a tab seen in that rank from the start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Place {
    pub rank: u8,
    pub since: u64,
}

/// The places of every tab a client's devices list, by device id and tab id.
#[derive(Default)]
pub struct AttentionLedger {
    places: HashMap<String, HashMap<String, Place>>,
    clock: u64,
}

/// A tab's rank this frame, and whether its row is held where it was.
pub struct Sighting {
    pub device: String,
    pub tab: String,
    pub rank: u8,
    pub held: bool,
}

impl AttentionLedger {
    /// A tab's place, or where an unsettled tab would sort.
    pub fn place(&self, device: &str, tab: &str, rank: u8) -> Place {
        self.places
            .get(device)
            .and_then(|tabs| tabs.get(tab))
            .copied()
            .unwrap_or(Place { rank, since: 0 })
    }

    /// Settle this frame's sightings. A tab that changed rank takes the
    /// latest tick, a held tab keeps its place, and tabs no longer sighted
    /// drop out.
    pub fn settle(&mut self, sightings: impl IntoIterator<Item = Sighting>) {
        let mut places: HashMap<String, HashMap<String, Place>> = HashMap::new();
        for sighting in sightings {
            let old = self
                .places
                .get(&sighting.device)
                .and_then(|tabs| tabs.get(&sighting.tab))
                .copied();
            let place = match old {
                Some(place) if sighting.held || place.rank == sighting.rank => place,
                Some(_) => {
                    self.clock += 1;
                    Place {
                        rank: sighting.rank,
                        since: self.clock,
                    }
                }
                None => Place {
                    rank: sighting.rank,
                    since: 0,
                },
            };
            places
                .entry(sighting.device)
                .or_default()
                .insert(sighting.tab, place);
        }
        self.places = places;
    }

    /// Places the rows of one rank, by device id and tab id, in the order
    /// they were dropped in. The order holds until one of them changes rank.
    pub fn reorder(&mut self, rank: u8, tabs: impl IntoIterator<Item = (String, String)>) {
        let tabs: Vec<(String, String)> = tabs.into_iter().collect();
        for (device, tab) in tabs.into_iter().rev() {
            self.clock += 1;
            self.places.entry(device).or_default().insert(
                tab,
                Place {
                    rank,
                    since: self.clock,
                },
            );
        }
    }
}

/// Where a mark sorts under the attention sort, the lowest first: a tab
/// waiting on an answer, then one whose plan is used up, then still working,
/// finished, idle with a shell still running, idle, and unknown.
pub fn attention_rank(mark: Mark) -> u8 {
    match mark {
        Mark::Status(AgentStatus::Blocked) => 0,
        Mark::Limited => 1,
        Mark::Status(AgentStatus::Working) => 2,
        Mark::Status(AgentStatus::Done) => 3,
        Mark::Monitoring => 4,
        Mark::Status(AgentStatus::Idle) => 5,
        Mark::Status(AgentStatus::Unknown) => 6,
    }
}
