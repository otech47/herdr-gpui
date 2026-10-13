//! Browser tabs a control request asks for, through the Unix control socket.

use super::{scope, store};
use crate::{
    HerdrWindow, NavigationTarget,
    browser::{Location, Store, TabId},
    control::{Placed, Target},
};
use gpui::*;

impl HerdrWindow {
    /// The endpoint and workspace a control request names, if this window
    /// shows it. `strict` requires the caller's own daemon; otherwise any
    /// endpoint showing the named workspace qualifies. Only the Unix control
    /// socket asks.
    fn browser_target(&self, target: &Target<'_>, strict: bool) -> Option<(usize, String)> {
        self.endpoints
            .iter()
            .enumerate()
            .find_map(|(index, endpoint)| {
                if strict {
                    let matches = match target.daemon {
                        Some(daemon) => {
                            endpoint.connection.target.socket_path().ok().as_deref() == Some(daemon)
                        }
                        None => index == self.selected_endpoint,
                    };
                    if !matches {
                        return None;
                    }
                } else if target.workspace.is_none() {
                    return None;
                }
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let snapshot = live.snapshot.as_ref()?;
                let workspace = match target.workspace {
                    Some(id) => id,
                    None => snapshot.focused_workspace_id.as_deref()?,
                };
                snapshot
                    .workspaces
                    .iter()
                    .any(|candidate| candidate.workspace_id == workspace)
                    .then(|| (index, workspace.to_owned()))
            })
    }

    /// Brings this window to the front if the workspace a control request
    /// names is the one it shows, and selects the named tab of it.
    pub(crate) fn raise_if_showing(
        &mut self,
        target: &Target<'_>,
        tab: Option<&str>,
        strict: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((index, workspace)) = self.browser_target(target, strict) else {
            return false;
        };
        if index != self.selected_endpoint {
            return false;
        }
        let shown = self
            .live
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.focused_workspace_id.as_deref())
            == Some(workspace.as_str());
        if !shown {
            return false;
        }
        if let Some(tab) = tab {
            let endpoint = self.endpoints[index].id.clone();
            self.navigate_endpoint(&endpoint, NavigationTarget::Tab(tab), cx);
        }
        window.activate_window();
        true
    }

    /// Shows the workspace a control request names, at the named tab: in a
    /// window of its own under `new_space_in_window`, else in this window,
    /// brought to the front.
    pub(crate) fn show_requested_workspace(
        &mut self,
        target: &Target<'_>,
        tab: Option<&str>,
        strict: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((index, workspace)) = self.browser_target(target, strict) else {
            return false;
        };
        if self.config.new_space_in_window {
            crate::app::open_window_for(
                self.endpoints[index].connection.target.clone(),
                Some(crate::window::Startup::Show {
                    workspace,
                    tab: tab.map(str::to_owned),
                }),
                cx,
            );
            return true;
        }
        let endpoint = self.endpoints[index].id.clone();
        let navigation = match tab {
            Some(tab) => NavigationTarget::Tab(tab),
            None => NavigationTarget::Workspace(workspace.as_str()),
        };
        if !self.navigate_endpoint(&endpoint, navigation, cx) {
            return false;
        }
        window.activate_window();
        true
    }

    /// Opens the tab a control request asks for, if this window shows its
    /// workspace. `None` leaves the request to another window.
    pub(crate) fn open_requested_browser_tab(
        &mut self,
        target: &Target<'_>,
        strict: bool,
        location: &Location,
        focus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Placed> {
        let (index, workspace) = self.browser_target(target, strict)?;
        let scope = scope(&self.endpoints[index]);
        // An agent showing the same page again gets its tab back, reloaded,
        // rather than another tab for every revision.
        let origin = target.pane;
        let before =
            store(cx).and_then(|store| store.opened_before(&scope, &workspace, origin, location));
        let id = match before {
            Some(id) => {
                #[cfg(any(target_os = "macos", windows))]
                self.browser.pages.reload(id, cx);
                id
            }
            None => {
                let opened = Store::update(cx, |store| {
                    store.open(
                        scope,
                        &workspace,
                        Some(location.clone()),
                        origin.map(str::to_owned),
                    )
                });
                let Some(id) = opened else {
                    return Some(Placed::Full);
                };
                id
            }
        };
        if focus {
            let focused = self
                .live
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.focused_workspace_id.as_deref());
            if index != self.selected_endpoint {
                let endpoint = self.endpoints[index].id.clone();
                self.navigate_endpoint(&endpoint, NavigationTarget::Workspace(&workspace), cx);
            } else if focused != Some(workspace.as_str()) {
                self.navigate(NavigationTarget::Workspace(&workspace), cx);
            }
            // Recorded against the workspace, so the tab shows once the
            // navigation lands even if it is still in flight.
            self.show_browser_tab(id, window, cx);
        }
        cx.notify();
        Some(Placed::Opened {
            workspace_id: workspace,
        })
    }

    /// Reloads this window's pages for `tabs`, as an agent asks after
    /// editing a page it showed.
    pub(crate) fn reload_browser_tabs(&mut self, tabs: &[TabId], cx: &mut Context<Self>) {
        #[cfg(any(target_os = "macos", windows))]
        for id in tabs {
            self.browser.pages.reload(*id, cx);
        }
        #[cfg(not(any(target_os = "macos", windows)))]
        let _ = (tabs, cx);
    }
}
