use super::*;
use crate::browser::{Tab, native::LinkAction, view::page_links::Asked};
use gpui::ClipboardItem;

fn web(value: &str) -> WebUrl {
    WebUrl::try_from(value).unwrap()
}

fn link(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, action: LinkAction, address: &str) {
    cx.update(|_, cx| {
        view.update(cx, |view, cx| view.page_link(action, &web(address), cx));
    });
}

/// A tab showing `location`, as the store would hold it.
fn opener(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, location: Location) -> Tab {
    let tab_scope = view.read_with(cx, |view, _| scope(&view.endpoints[0]));
    cx.update(|_, cx| {
        let id = Store::update(cx, |store| {
            store.open(tab_scope, "w0", Some(location), None)
        })
        .unwrap();
        cx.global::<Store>().get(id).cloned().unwrap()
    })
}

#[gpui::test]
fn a_link_off_the_site_opens_in_the_system_browser(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    link(&view, cx, LinkAction::Open, "https://github.com/a/b/pull/1");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/a/b/pull/1")
    );

    // A double click reports the link twice and opens it once.
    link(&view, cx, LinkAction::Open, "https://github.com/a/b/pull/2");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/a/b/pull/1")
    );

    view.update(cx, |view, _| view.browser.opened_outside = None);
    link(&view, cx, LinkAction::Open, "https://github.com/a/b/pull/2");
    assert_eq!(
        cx.opened_url().as_deref(),
        Some("https://github.com/a/b/pull/2")
    );
}

#[gpui::test]
fn copying_a_link_puts_its_address_on_the_clipboard(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| cx.write_to_clipboard(ClipboardItem::new_string("before".into())));
    link(
        &view,
        cx,
        LinkAction::Copy,
        "https://a.test/docs?page=2#top",
    );
    assert_eq!(
        cx.read_from_clipboard()
            .and_then(|item| item.text())
            .as_deref(),
        Some("https://a.test/docs?page=2#top")
    );
    assert_eq!(cx.opened_url(), None, "copying opens nothing");
}

#[gpui::test]
fn a_window_a_page_asks_for_stays_only_on_its_site(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    let report = opener(&view, cx, url("http://localhost:8788/reports/a.html"));
    assert_eq!(
        Asked::by(&report, "http://localhost:8788/reports/b.html"),
        Some(Asked::Tab(web("http://localhost:8788/reports/b.html")))
    );
    assert_eq!(
        Asked::by(&report, "https://github.com/a/b"),
        Some(Asked::Outside(web("https://github.com/a/b")))
    );
    for refused in ["file:///etc/passwd", "vscode://file/x", "about:blank"] {
        assert_eq!(Asked::by(&report, refused), None, "{refused}");
    }
    // A tab with no address yet is on no site.
    let blank = Tab {
        location: None,
        ..report
    };
    assert_eq!(
        Asked::by(&blank, "http://localhost:8788/reports/b.html"),
        Some(Asked::Outside(web("http://localhost:8788/reports/b.html")))
    );
}
