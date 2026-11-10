#![allow(clippy::unwrap_used)]

use super::*;
use crate::browser::TabId;
use crate::browser::native::{Event, post_event};

fn report(action: &str, url: &str) -> String {
    serde_json::json!({ "kind": "link", "action": action, "url": url }).to_string()
}

#[test]
fn link_reports_carry_one_web_address() {
    let url = WebUrl::try_from("https://github.com/a/b/pull/1").unwrap();
    assert_eq!(
        parse(&report("open", url.as_str())),
        Some((Action::Open, url.clone()))
    );
    assert_eq!(
        parse(&report("copy", url.as_str())),
        Some((Action::Copy, url))
    );
}

#[test]
fn only_web_addresses_and_known_actions_are_links() {
    for other in [
        report("open", "file:///etc/passwd"),
        report("open", "javascript:alert(1)"),
        report("copy", "vscode://file/x"),
        report("copy", "not a url"),
        report("open", "https://a.test/ b"),
        report("run", "https://a.test/"),
        r#"{"kind":"link","action":"open"}"#.to_owned(),
        r#"{"kind":"cursor","cursor":"pointer"}"#.to_owned(),
        r#"{"kind":"cancel"}"#.to_owned(),
        "https://a.test/".to_owned(),
    ] {
        assert_eq!(parse(&other), None, "{other}");
    }
    let long = format!("https://a.test/{}", "x".repeat(MAX_MESSAGE_BYTES));
    assert_eq!(parse(&report("open", &long)), None);
}

#[test]
fn page_posts_become_link_reports() {
    let id = TabId::test(0);
    let url = WebUrl::try_from("https://a.test/x").unwrap();
    assert_eq!(
        post_event(id, report("open", url.as_str())),
        Some(Event::Link(id, Action::Open, url))
    );
    // Anything else is still the annotation picker's to read.
    let refused = report("open", "file:///etc/passwd");
    assert_eq!(
        post_event(id, refused.clone()),
        Some(Event::Posted(id, refused))
    );
}
