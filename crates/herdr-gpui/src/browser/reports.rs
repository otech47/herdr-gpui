use super::{Location, Pick, Scope, Store, TabId, WebUrl};
use crate::HerdrWindow;
use gpui::{Context, Window};

#[derive(Clone)]
pub(crate) struct PageSource {
    pub scope: Scope,
    pub workspace: String,
    pub session: Pick,
    pub pane: Option<String>,
}

impl WebUrl {
    // Only viewable files on the private file server are attached automatically.
    pub(crate) fn report_title(&self) -> Option<String> {
        let host = self.0.host_str()?;
        if self.0.port() != Some(8788)
            || !(host.ends_with(".ts.net") || matches!(host, "localhost" | "127.0.0.1" | "::1"))
        {
            return None;
        }
        let name = self.0.path_segments()?.next_back()?;
        let name = percent_encoding::percent_decode_str(name)
            .decode_utf8()
            .ok()?;
        let extension = name.rsplit('.').next()?.to_ascii_lowercase();
        matches!(
            extension.as_str(),
            "html" | "htm" | "pdf" | "png" | "jpg" | "jpeg" | "webp" | "svg" | "txt" | "md"
        )
        .then(|| name.into_owned())
    }
}

/// Discover only revealed files, without fetching or executing them.
pub(crate) fn report_urls(output: &str) -> Vec<WebUrl> {
    output
        .split(|ch: char| {
            ch.is_whitespace()
                || matches!(
                    ch,
                    '(' | ')' | '<' | '>' | '"' | '\'' | '`' | '[' | ']' | '\u{1b}'
                )
        })
        .filter(|text| text.starts_with("http://") || text.starts_with("https://"))
        .filter_map(|text| WebUrl::try_from(text.trim_end_matches(['.', ',', ';'])).ok())
        .filter(|url| url.report_title().is_some())
        .take(64)
        .collect()
}

impl Store {
    pub(crate) fn open_session_page(&mut self, source: &PageSource, url: WebUrl) -> Option<TabId> {
        let location = Location::Web { url };
        if let Some(id) = self
            .session_pages(&source.scope, &source.workspace, &source.session)
            .find(|tab| tab.location.as_ref() == Some(&location))
            .map(|tab| tab.id)
        {
            return Some(id);
        }
        let id = self.open(
            source.scope.clone(),
            &source.workspace,
            Some(location),
            source.pane.clone(),
        )?;
        self.attach_to_session(id, source.session.clone());
        Some(id)
    }
}

impl HerdrWindow {
    pub(crate) fn page_source_for_pane(&self, pane: &str) -> Option<PageSource> {
        let (scope, workspace) = self.browser_key()?;
        let session = self.page_session_for_pane(&scope, &workspace, pane)?;
        Some(PageSource {
            scope,
            workspace,
            session,
            pane: Some(pane.to_owned()),
        })
    }

    pub(crate) fn open_session_link(
        &mut self,
        source: &PageSource,
        url: WebUrl,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = Store::update(cx, |store| store.open_session_page(source, url)) {
            self.show_browser_tab(id, window, cx);
        }
    }
}

#[cfg(test)]
mod tests;
