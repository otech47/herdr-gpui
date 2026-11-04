use super::{Location, Pick, Scope, Store, Tab, TabId};

impl Store {
    pub(crate) fn discover_session_pages(
        &mut self,
        parent: TabId,
        urls: Vec<super::super::WebUrl>,
    ) {
        let Some(tab) = self.get(parent).cloned() else {
            return;
        };
        let Some(Location::AgentSession { mut session }) = tab.location else {
            return;
        };
        let source = super::super::PageSource {
            scope: tab.scope,
            workspace: tab.workspace_id,
            session: Pick::Page(parent),
            pane: None,
        };
        let before = session.reports.len();
        for url in urls {
            if session.reports.len() >= 64 {
                break;
            }
            if !session.reports.contains(&url)
                && self.open_session_page(&source, url.clone()).is_some()
            {
                session.reports.push(url);
            }
        }
        if session.reports.len() != before {
            if let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == parent) {
                tab.location = Some(Location::AgentSession { session });
            }
            self.save();
        }
    }

    pub(crate) fn transfer_session_pages(
        &mut self,
        parent: TabId,
        source: &super::super::PageSource,
    ) -> Vec<TabId> {
        let mut pages = Vec::new();
        for tab in &mut self.tabs {
            if tab.session == Some(Pick::Page(parent)) {
                tab.scope = source.scope.clone();
                tab.workspace_id.clone_from(&source.workspace);
                tab.origin.clone_from(&source.pane);
                tab.session = Some(source.session.clone());
                pages.push(tab.id);
            }
        }
        if !pages.is_empty() {
            self.save();
        }
        pages
    }

    pub(crate) fn unbound_pages(&self) -> impl Iterator<Item = &Tab> {
        self.tabs.iter().filter(|tab| {
            tab.session.is_none()
                && tab.origin.is_some()
                && tab.location.as_ref().is_none_or(Location::is_page)
        })
    }

    pub(crate) fn session_pages<'a>(
        &'a self,
        scope: &'a Scope,
        workspace: &'a str,
        session: &'a Pick,
    ) -> impl Iterator<Item = &'a Tab> + 'a {
        self.in_workspace(scope, workspace)
            .filter(move |tab| tab.session.as_ref() == Some(session))
    }

    /// A page can belong to a terminal or a watched conversation in the
    /// same workspace, never to another report.
    pub(crate) fn attach_to_session(&mut self, id: TabId, session: Pick) -> bool {
        let Some(tab) = self.get(id) else {
            return false;
        };
        if tab.session.as_ref() == Some(&session)
            || !super::super::groups::valid_pick(&session)
            || tab
                .location
                .as_ref()
                .is_some_and(|location| !location.is_page())
        {
            return false;
        }
        if let Pick::Page(parent) = session
            && self.get(parent).is_none_or(|parent| {
                parent.scope != tab.scope
                    || parent.workspace_id != tab.workspace_id
                    || !matches!(parent.location, Some(Location::AgentSession { .. }))
            })
        {
            return false;
        }
        let Some(tab) = self.tabs.iter_mut().find(|tab| tab.id == id) else {
            return false;
        };
        tab.session = Some(session);
        self.save();
        true
    }

    pub(crate) fn detach_session(&mut self, scope: &Scope, workspace: &str, session: &Pick) {
        let mut changed = false;
        for tab in &mut self.tabs {
            if &tab.scope == scope
                && tab.workspace_id == workspace
                && tab.session.as_ref() == Some(session)
            {
                tab.session = None;
                changed = true;
            }
        }
        if changed {
            self.save();
        }
    }
}
