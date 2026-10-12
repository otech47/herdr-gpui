//! What a window was opened to show, applied once its daemon answers.
use crate::{Command, HerdrWindow, NavigationTarget, controls, window::Startup};
use gpui::{Context, Window};

impl HerdrWindow {
    /// Applies the startup request once the surface is ready, and notes the
    /// space the window shows so it reopens there.
    pub(crate) fn settle_startup(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(snapshot) = self.live.snapshot.clone() else {
            return;
        };
        if self.startup.is_some() && self.navigation_ready() {
            match self.startup.take() {
                Some(Startup::Show(workspace)) => {
                    let known = snapshot
                        .workspaces
                        .iter()
                        .any(|candidate| candidate.workspace_id == workspace);
                    if known && snapshot.focused_workspace_id.as_deref() != Some(workspace.as_str())
                    {
                        self.pending_navigation = Some(NavigationTarget::Workspace(workspace));
                    }
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
