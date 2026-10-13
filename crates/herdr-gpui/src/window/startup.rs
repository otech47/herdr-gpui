//! What a window was opened to show, applied once its daemon answers.
use crate::{
    Command, HerdrWindow, NavigationTarget, OwnedNavigationTarget, controls, window::Startup,
};
use gpui::{Context, Window};
use herdr_client::protocol::ClientShellSnapshot;

impl HerdrWindow {
    /// Applies the startup request once the surface is ready, and notes the
    /// space the window shows so it reopens there.
    pub(crate) fn settle_startup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        if self.startup.is_some() && self.navigation_ready() {
            match self.startup.take() {
                Some(Startup::Show { workspace, tab }) => {
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
        let local = self.endpoints[self.selected_endpoint].id == crate::endpoint::LOCAL;
        let shown = local
            .then(|| snapshot.focused_workspace_id.clone())
            .flatten();
        crate::window_state::WindowState::note_workspace(window, shown, cx);
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
