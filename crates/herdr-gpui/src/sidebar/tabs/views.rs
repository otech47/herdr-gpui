//! Which tabs the list shows: the selected device's shown space alone, or
//! with that space's namesakes on the other devices. And in which order: the
//! daemon's, or the tabs that want attention first.

use super::attention::attention_rank;
use crate::{
    HerdrWindow,
    preferences::{TabsSort, TabsView},
    state::LiveState,
};
use herdr_client::protocol::{ClientShellSnapshot, ClientShellTab};
use std::cmp::Reverse;

/// A listed tab, with the device and snapshot it belongs to.
pub(crate) struct ListedTab<'a> {
    /// Its device's index among the window's endpoints.
    pub(crate) endpoint: usize,
    pub(crate) snapshot: &'a ClientShellSnapshot,
    pub(crate) tab: &'a ClientShellTab,
}

impl HerdrWindow {
    /// A device's live state: the window's own copy for the selected device.
    pub(crate) fn live_of(&self, endpoint: usize) -> &LiveState {
        if endpoint == self.selected_endpoint {
            &self.live
        } else {
            &self.endpoints[endpoint].live
        }
    }

    /// The listed tabs in numbering order: the selected device's shown space,
    /// then in the views across devices each other device's space of that name.
    /// The attention sort reorders them by their settled place, the daemon's
    /// order breaking ties, and keeps each device's rows together when the
    /// view groups them.
    pub(crate) fn listed_tabs(&self) -> Vec<ListedTab<'_>> {
        let Some(shown) = self.live.snapshot.as_deref() else {
            return Vec::new();
        };
        let Some(focused) = shown.focused_workspace_id.as_deref() else {
            return Vec::new();
        };
        let across = self.config.show_sidebar_tabs && self.sidebar_tabs_view != TabsView::Device;
        let label = across
            .then(|| workspace_label_by_id(shown, focused))
            .flatten();
        let mut listed = Vec::new();
        for (index, endpoint) in self.endpoints.iter().enumerate() {
            let (snapshot, workspace) = if index == self.selected_endpoint {
                (shown, focused)
            } else {
                let Some(label) = label else {
                    continue;
                };
                if !endpoint.enabled || !self.device_visible(&endpoint.id) {
                    continue;
                }
                let Some(snapshot) = endpoint.live.snapshot.as_deref() else {
                    continue;
                };
                let Some(namesake) = snapshot
                    .workspaces
                    .iter()
                    .find(|workspace| workspace.label == label)
                else {
                    continue;
                };
                (snapshot, namesake.workspace_id.as_str())
            };
            listed.extend(
                snapshot
                    .tabs
                    .iter()
                    .filter(|tab| tab.workspace_id == workspace)
                    .map(|tab| ListedTab {
                        endpoint: index,
                        snapshot,
                        tab,
                    }),
            );
        }
        if self.sidebar_tabs_sort == TabsSort::Attention {
            let grouped = self.sidebar_tabs_view == TabsView::ByDevice;
            listed.sort_by_key(|row| {
                let agent = row
                    .snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.tab_id == row.tab.tab_id);
                let rank = attention_rank(self.mark(row.endpoint, agent, row.tab.agent_status));
                let place =
                    self.attention
                        .place(&self.endpoints[row.endpoint].id, &row.tab.tab_id, rank);
                (
                    grouped.then_some(row.endpoint),
                    place.rank,
                    Reverse(place.since),
                )
            });
        }
        listed
    }

    /// Whether the list's numbers are its own rather than the daemon's: it
    /// counts across devices, or it is sorted.
    pub(crate) fn numbers_listed_tabs(&self) -> bool {
        self.config.show_sidebar_tabs
            && (self.sidebar_tabs_view != TabsView::Device
                || self.sidebar_tabs_sort == TabsSort::Attention)
    }

    /// The device and tab that cmd-`number` reaches while the list numbers
    /// its own rows.
    pub(crate) fn listed_tab(&self, number: u8) -> Option<(String, String)> {
        if !self.numbers_listed_tabs() {
            return None;
        }
        let row = self
            .listed_tabs()
            .into_iter()
            .nth(usize::from(number).checked_sub(1)?)?;
        Some(self.listed_ids(&row))
    }

    /// The device and tab that the next or previous tab command reaches
    /// while the list numbers its own rows: the focused tab's neighbor in the
    /// list, wrapping at either end.
    pub(crate) fn listed_tab_step(&self, forward: bool) -> Option<(String, String)> {
        if !self.numbers_listed_tabs() {
            return None;
        }
        let focused = self.live.snapshot.as_deref()?.focused_tab_id.as_deref()?;
        let listed = self.listed_tabs();
        let index = listed
            .iter()
            .position(|row| row.endpoint == self.selected_endpoint && row.tab.tab_id == focused)?;
        let step = if forward { 1 } else { listed.len() - 1 };
        Some(self.listed_ids(&listed[(index + step) % listed.len()]))
    }

    fn listed_ids(&self, row: &ListedTab<'_>) -> (String, String) {
        (
            self.endpoints[row.endpoint].id.clone(),
            row.tab.tab_id.clone(),
        )
    }
}

pub(crate) fn workspace_label_by_id<'a>(
    snapshot: &'a ClientShellSnapshot,
    workspace: &str,
) -> Option<&'a str> {
    snapshot
        .workspaces
        .iter()
        .find(|candidate| candidate.workspace_id == workspace)
        .map(|candidate| candidate.label.as_str())
}
