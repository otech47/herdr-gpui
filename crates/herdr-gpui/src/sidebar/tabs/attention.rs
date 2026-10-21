//! Where each tab sits under the attention sort: its mark's rank, and within
//! a rank the tab that reached it last first. The focused tab keeps its row
//! until focus leaves it, so the list never moves under the row the user is
//! on.

use crate::{HerdrWindow, sidebar::agents::Mark};
use herdr_client::protocol::AgentStatus;
use std::collections::HashMap;

/// A tab's place in the attention sort: the rank it holds, and the tick it
/// took it on. Zero is a tab seen in that rank from the start.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct Place {
    pub(crate) rank: u8,
    pub(crate) since: u64,
}

/// The places of every tab the window's devices list, by device id and tab id.
#[derive(Default)]
pub(crate) struct AttentionLedger {
    places: HashMap<String, HashMap<String, Place>>,
    clock: u64,
}

/// A tab's rank this frame, and whether its row is held where it was.
pub(crate) struct Sighting {
    pub(crate) device: String,
    pub(crate) tab: String,
    pub(crate) rank: u8,
    pub(crate) held: bool,
}

impl AttentionLedger {
    /// A tab's place, or where an unsettled tab would sort.
    pub(crate) fn place(&self, device: &str, tab: &str, rank: u8) -> Place {
        self.places
            .get(device)
            .and_then(|tabs| tabs.get(tab))
            .copied()
            .unwrap_or(Place { rank, since: 0 })
    }

    /// Settle this frame's sightings. A tab that changed rank takes the
    /// latest tick, a held tab keeps its place, and tabs no longer sighted
    /// drop out.
    pub(crate) fn settle(&mut self, sightings: impl IntoIterator<Item = Sighting>) {
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
}

impl HerdrWindow {
    /// Settle every tab's place for this frame. The focused tab on the
    /// selected device is held.
    pub(crate) fn settle_attention(&mut self) {
        let focused = self
            .live
            .snapshot
            .as_deref()
            .and_then(|snapshot| snapshot.focused_tab_id.as_deref());
        let mut sightings = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let Some(snapshot) = self.live_of(index).snapshot.as_deref() else {
                continue;
            };
            for tab in &snapshot.tabs {
                let agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.tab_id == tab.tab_id);
                sightings.push(Sighting {
                    device: endpoint.id.clone(),
                    tab: tab.tab_id.clone(),
                    rank: attention_rank(self.mark(index, agent, tab.agent_status)),
                    held: index == self.selected_endpoint && focused == Some(tab.tab_id.as_str()),
                });
            }
        }
        self.attention.settle(sightings);
    }
}

/// Where a mark sorts under the attention sort, the lowest first: a tab
/// waiting on an answer, then one whose plan is used up, then still working,
/// finished, idle, and unknown.
pub(crate) fn attention_rank(mark: Mark) -> u8 {
    match mark {
        Mark::Status(AgentStatus::Blocked) => 0,
        Mark::Limited => 1,
        Mark::Status(AgentStatus::Working) => 2,
        Mark::Status(AgentStatus::Done) => 3,
        Mark::Status(AgentStatus::Idle) => 4,
        Mark::Status(AgentStatus::Unknown) => 5,
    }
}
