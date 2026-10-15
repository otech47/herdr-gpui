//! Which tabs the list shows: the selected device's shown space alone, or
//! with that space's namesakes on the other devices.

use crate::{HerdrWindow, preferences::TabsView, state::LiveState};
use herdr_client::protocol::{ClientShellSnapshot, ClientShellTab};

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
        listed
    }

    /// The device and tab that cmd-`number` reaches while the list counts
    /// across devices.
    pub(crate) fn listed_tab(&self, number: u8) -> Option<(String, String)> {
        if !self.config.show_sidebar_tabs || self.sidebar_tabs_view == TabsView::Device {
            return None;
        }
        let row = self
            .listed_tabs()
            .into_iter()
            .nth(usize::from(number).checked_sub(1)?)?;
        Some((
            self.endpoints[row.endpoint].id.clone(),
            row.tab.tab_id.clone(),
        ))
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
