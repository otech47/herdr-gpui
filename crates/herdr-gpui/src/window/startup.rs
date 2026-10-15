//! What a window was opened to show, applied once its daemon answers.
use crate::{
    Command, HerdrWindow, NavigationTarget, OwnedNavigationTarget, controls, window::Startup,
    window_state::Shown,
};
use gpui::{Context, Window};
use herdr_client::protocol::ClientShellSnapshot;

impl HerdrWindow {
    /// A window opened for a space stays off the shared last chosen device.
    pub(crate) fn open_for(&mut self, startup: Startup) {
        self.startup = Some(startup);
        self.catalog.pin();
    }

    /// Applies the startup request once the surface is ready, and notes the
    /// space the window shows so it reopens there.
    pub(crate) fn settle_startup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settle_device_startup(cx) {
            return;
        }
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        if self.startup.is_some() && self.navigation_ready() {
            match self.startup.take() {
                Some(Startup::Show { workspace, tab, .. }) => {
                    self.pending_navigation = shown_navigation(&snapshot, workspace, tab);
                }
                Some(Startup::NewWorkspace) => {
                    if let Some((method, params)) = controls::request(Command::Workspace, &snapshot)
                    {
                        self.request_focus_change(method.as_str(), None, |handle, boot| {
                            handle.request(boot, method, params)
                        });
                    }
                }
                None => {}
            }
        }
        if self.startup.is_some() {
            return;
        }
        let endpoint = &self.endpoints[self.selected_endpoint];
        let device = (endpoint.id != crate::endpoint::LOCAL).then(|| endpoint.id.clone());
        let shown = snapshot
            .focused_workspace_id
            .clone()
            .map(|workspace| Shown { workspace, device });
        crate::window_state::WindowState::note_workspace(window, shown, cx);
    }

    /// A startup onto a saved device's space waits for the catalog to list
    /// the device, then moves the window there. Returns whether it is still
    /// waiting, or just moved.
    fn settle_device_startup(&mut self, cx: &mut Context<Self>) -> bool {
        let waiting = match &self.startup {
            Some(Startup::Show {
                device: Some(device),
                ..
            }) => *device != self.endpoints[self.selected_endpoint].id,
            _ => false,
        };
        if !waiting {
            return false;
        }
        if !self.catalog_loaded() {
            return true;
        }
        let Some(Startup::Show {
            workspace,
            tab,
            device: Some(device),
        }) = self.startup.take()
        else {
            return false;
        };
        let listed = self
            .endpoints
            .iter()
            .any(|endpoint| endpoint.id == device && endpoint.enabled);
        if !listed {
            // Waiting on a device no longer in the catalog would never end.
            return false;
        }
        let target = match &tab {
            Some(tab) => NavigationTarget::Tab(tab.as_str()),
            None => NavigationTarget::Workspace(workspace.as_str()),
        };
        if !self.move_to_device(&device, target, cx) {
            self.startup = Some(Startup::Show {
                workspace,
                tab,
                device: Some(device),
            });
        }
        true
    }
}

/// The navigation that shows `workspace` at `tab`: the tab when the snapshot
/// lists it under that workspace, else the workspace when it exists, and
/// nothing when the snapshot already shows it.
pub(crate) fn shown_navigation(
    snapshot: &ClientShellSnapshot,
    workspace: String,
    tab: Option<String>,
) -> Option<OwnedNavigationTarget> {
    let tab = tab.filter(|tab| {
        snapshot
            .tabs
            .iter()
            .any(|candidate| &candidate.tab_id == tab && candidate.workspace_id == workspace)
    });
    if let Some(tab) = tab {
        return (snapshot.focused_tab_id.as_deref() != Some(tab.as_str()))
            .then_some(NavigationTarget::Tab(tab));
    }
    let known = snapshot
        .workspaces
        .iter()
        .any(|candidate| candidate.workspace_id == workspace);
    (known && snapshot.focused_workspace_id.as_deref() != Some(workspace.as_str()))
        .then_some(NavigationTarget::Workspace(workspace))
}
