//! The provider the status bar leads with: the agent in the focused pane.

use super::{model::Provider, registry};
use crate::window::HerdrWindow;
use herdr_client::protocol::ClientShellSnapshot;

impl HerdrWindow {
    pub(crate) fn focused_provider(&self) -> Option<Provider> {
        focused_provider(self.live.snapshot.as_deref()?)
    }
}

/// The provider of the agent in the focused pane, else of one in the focused
/// tab, when its service has a provider.
pub(super) fn focused_provider(snapshot: &ClientShellSnapshot) -> Option<Provider> {
    let pane = snapshot.focused_pane_id.as_deref();
    let tab = snapshot.focused_tab_id.as_deref();
    let agents = &snapshot.agents;
    let agent = agents
        .iter()
        .find(|agent| Some(agent.pane_id.as_str()) == pane)
        .or_else(|| {
            agents
                .iter()
                .find(|agent| Some(agent.tab_id.as_str()) == tab)
        })?;
    registry::find(agent.agent.as_deref()?)
}
