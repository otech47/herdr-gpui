//! Which tabs the list shows: the selected device's shown space alone, or
//! with that space's namesakes on the other devices. And in which order: the
//! daemon's, or the tabs that want attention first.

use super::attention::attention_rank;
use crate::{
    HerdrWindow,
    preferences::{TabsSort, TabsView},
    state::LiveState,
};
use herdr_board::{Source, tab_agent};

pub(crate) use herdr_board::{ListedTab, workspace_label_by_id};

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
        let sources = self.sources();
        let across = self.config.show_sidebar_tabs && self.sidebar_tabs_view != TabsView::Device;
        let mut listed = herdr_board::listed_tabs(&sources, self.selected_endpoint, across);
        let grouped = self.sidebar_tabs_view == TabsView::ByDevice;
        let attention = self.sidebar_tabs_sort == TabsSort::Attention;
        if attention {
            herdr_board::sort_by_attention(
                &mut listed,
                &sources,
                &self.attention,
                grouped,
                |row| self.listed_rank(row),
            );
        }
        // An ungrouped attention sort mixes devices, so it has no device order.
        if grouped || !attention {
            let order = self.device_order();
            listed.sort_by_key(|row| order.iter().position(|&index| index == row.endpoint));
        }
        listed
    }

    /// Every device as the tab lists see it, in the window's endpoint order.
    pub(crate) fn sources(&self) -> Vec<Source<'_>> {
        self.endpoints
            .iter()
            .enumerate()
            .map(|(index, endpoint)| {
                let live = self.live_of(index);
                Source {
                    id: &endpoint.id,
                    label: &endpoint.label,
                    enabled: endpoint.enabled,
                    visible: self.device_visible(&endpoint.id),
                    connected: live.status.is_connected(),
                    snapshot: live.snapshot.as_deref(),
                }
            })
            .collect()
    }

    /// Where a listed tab's mark sorts under the attention sort.
    pub(crate) fn listed_rank(&self, row: &ListedTab<'_>) -> u8 {
        let agent = tab_agent(row.snapshot, &row.tab.tab_id);
        attention_rank(self.mark(row.endpoint, agent, row.tab.agent_status))
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
