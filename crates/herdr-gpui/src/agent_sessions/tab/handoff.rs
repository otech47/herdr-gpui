use super::*;
use crate::browser::PageSource;

impl HerdrWindow {
    pub(super) fn finish_session_handoffs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let ids: Vec<_> = self
            .session_tabs
            .iter()
            .filter_map(|(&id, tab)| tab.handoff.as_ref().map(|_| id))
            .collect();
        for id in ids {
            let Some((source, opened, started)) = self
                .session_tabs
                .get(&id)
                .and_then(|tab| tab.handoff.as_ref())
            else {
                continue;
            };
            let endpoint =
                self.endpoints.iter().enumerate().find(|(_, e)| {
                    e.enabled && e.id == source.id && e.generation == source.generation
                });
            let target = endpoint.and_then(|(index, endpoint)| {
                let live = if index == self.selected_endpoint {
                    &self.live
                } else {
                    &endpoint.live
                };
                let pane = live
                    .snapshot
                    .as_ref()?
                    .panes
                    .iter()
                    .find(|pane| pane.pane_id == opened.pane_id)?;
                Some(PageSource {
                    scope: crate::browser::scope(endpoint),
                    workspace: pane.workspace_id.clone(),
                    session: Pick::Herdr(pane.tab_id.clone()),
                    pane: Some(pane.pane_id.clone()),
                })
            });
            let Some(target) = target else {
                if (endpoint.is_none() || started.elapsed() > Duration::from_secs(10))
                    && let Some(tab) = self.session_tabs.get_mut(&id)
                {
                    tab.handoff = None;
                    tab.error = Some("The tab opened, but this device has not connected yet. Reconnect the device, then open the tab again.".into());
                }
                continue;
            };
            let endpoint = source.id.clone();
            let active = self.active_session_tab(cx) == Some(id);
            let selected = self.active_group().and_then(|group| self.group_pick(group));
            let pages = Store::update(cx, |store| store.transfer_session_pages(id, &target));
            let selected_page = selected.and_then(|pick| match pick {
                Pick::Page(page) if pages.contains(&page) => Some(page),
                _ => None,
            });
            self.forget_transferred_pages(&pages, cx);
            self.close_browser_tab(id, window, cx);
            self.session_tabs.remove(&id);
            if active {
                if let Some(pane) = &target.pane {
                    self.navigate_endpoint(&endpoint, NavigationTarget::Pane(pane), cx);
                }
                if let Some(page) = selected_page {
                    self.show_browser_tab(page, window, cx);
                } else {
                    window.focus(&self.focus, cx);
                }
            }
        }
    }
}
