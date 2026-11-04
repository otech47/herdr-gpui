#![allow(clippy::unwrap_used)]
use super::*;

#[test]
fn discovery_only_lists_private_revealed_files_and_preserves_their_links() {
    let output = "[review](http://host.tail123.ts.net:8788/.agent-scratch/report%20one.html)\nhttps://example.com/report.html http://localhost:3000/app.html http://host.tail123.ts.net:8788/folder/ https://evilts.net:8788/a.html";
    let urls = report_urls(output);
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0].report_title().as_deref(), Some("report one.html"));
    assert_eq!(
        urls[0].as_str(),
        "http://host.tail123.ts.net:8788/.agent-scratch/report%20one.html"
    );
}

#[test]
fn two_sessions_can_review_the_same_report_independently() {
    let mut store = Store::default();
    let mut source = PageSource {
        scope: Scope::endpoint("local"),
        workspace: "w1".into(),
        session: Pick::Herdr("t1".into()),
        pane: Some("p1".into()),
    };
    let url = WebUrl::try_from("http://host.tail123.ts.net:8788/report.html").unwrap();
    let first = store.open_session_page(&source, url.clone()).unwrap();
    assert_eq!(store.open_session_page(&source, url.clone()), Some(first));
    source.session = Pick::Herdr("t2".into());
    source.pane = Some("p2".into());
    assert_ne!(store.open_session_page(&source, url), Some(first));
}
