//! Where a page's links go. An address on another site opens in the system
//! browser, and a window the page asks for on its own site opens as a tab.
use super::store;
use crate::{
    HerdrWindow,
    browser::{Location, Pick, Store, Tab, TabId, WebUrl, native::LinkAction},
};
use gpui::{ClipboardItem, Context, Window};
use std::time::{Duration, Instant};

/// A double click, or a page that asks again, opens one browser tab.
const OPEN_INTERVAL: Duration = Duration::from_millis(400);

#[derive(Debug, PartialEq, Eq)]
pub(in crate::browser) enum Asked {
    Tab(WebUrl),
    Outside(WebUrl),
}

impl Asked {
    /// `None` for an address no page may open, such as `file:`.
    pub(in crate::browser) fn by(opener: &Tab, address: &str) -> Option<Self> {
        let url = WebUrl::try_from(address).ok()?;
        let stays = opener
            .location
            .as_ref()
            .is_some_and(|location| location.same_site(&url));
        Some(if stays {
            Self::Tab(url)
        } else {
            Self::Outside(url)
        })
    }
}

impl HerdrWindow {
    pub(in crate::browser) fn page_link(
        &mut self,
        action: LinkAction,
        url: &WebUrl,
        cx: &mut Context<Self>,
    ) {
        match action {
            LinkAction::Open => self.open_outside(url, cx),
            LinkAction::Copy => {
                cx.write_to_clipboard(ClipboardItem::new_string(url.as_str().to_owned()));
            }
        }
    }

    fn open_outside(&mut self, url: &WebUrl, cx: &mut Context<Self>) {
        let now = Instant::now();
        let again = self
            .browser
            .opened_outside
            .is_some_and(|last| now.duration_since(last) < OPEN_INTERVAL);
        if again {
            tracing::debug!("Ignored a repeated request to open the system browser");
            return;
        }
        self.browser.opened_outside = Some(now);
        cx.open_url(url.as_str());
    }

    pub(super) fn page_asked_for_window(
        &mut self,
        id: TabId,
        address: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(opener) = store(cx).and_then(|store| store.get(id)).cloned() else {
            return;
        };
        let url = match Asked::by(&opener, address) {
            Some(Asked::Tab(url)) => url,
            Some(Asked::Outside(url)) => return self.open_outside(&url, cx),
            None => return,
        };
        // The new tab opens where its opener shows.
        let group = self.group_showing(&Pick::Page(id), cx);
        let opened = Store::update(cx, |store| {
            store.open(
                opener.scope,
                &opener.workspace_id,
                Some(Location::Web { url }),
                opener.origin,
            )
        });
        let Some(opened) = opened else {
            return;
        };
        if let Some(session) = opener.session {
            Store::update(cx, |store| store.attach_to_session(opened, session));
        }
        self.show_browser_tab_in(group, opened, window, cx);
    }
}
