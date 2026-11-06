use super::*;
use crate::browser::native::{Event, MAX_POST_BYTES, post_event};
use crate::browser::{Pages, TabId};

fn report(cursor: &str) -> String {
    serde_json::json!({ "kind": "cursor", "cursor": cursor }).to_string()
}

#[test]
fn css_cursors_map_to_native_ones() {
    for (keyword, native) in [
        ("pointer", CursorStyle::PointingHand),
        ("text", CursorStyle::IBeam),
        ("crosshair", CursorStyle::Crosshair),
        ("grab", CursorStyle::OpenHand),
        ("grabbing", CursorStyle::ClosedHand),
        ("not-allowed", CursorStyle::OperationNotAllowed),
        ("col-resize", CursorStyle::ResizeColumn),
        ("nwse-resize", CursorStyle::ResizeUpLeftDownRight),
        ("default", CursorStyle::Arrow),
        ("wait", CursorStyle::Arrow),
        ("", CursorStyle::Arrow),
    ] {
        assert_eq!(parse(&report(keyword)), Some(native), "{keyword}");
    }
}

#[test]
fn only_short_cursor_reports_are_cursors() {
    let picked = r#"{"kind":"pick","target":{"kind":"element","selector":"a"}}"#;
    for other in [
        picked,
        r#"{"kind":"cancel"}"#,
        r#"{"kind":"cursor"}"#,
        "pointer",
        "not json",
    ] {
        assert_eq!(parse(other), None, "{other}");
    }
    assert_eq!(parse(&report(&"x".repeat(MAX_MESSAGE_BYTES))), None);
}

#[test]
fn page_posts_become_cursor_reports_or_picker_messages() {
    let id = TabId::test(0);
    assert_eq!(
        post_event(id, report("pointer")),
        Some(Event::Cursor(id, CursorStyle::PointingHand))
    );
    let picked = r#"{"kind":"cancel"}"#.to_owned();
    assert_eq!(
        post_event(id, picked.clone()),
        Some(Event::Posted(id, picked))
    );
    assert_eq!(post_event(id, "x".repeat(MAX_POST_BYTES + 1)), None);
}

#[test]
fn a_closed_page_keeps_no_cursor() {
    let mut pages = Pages::default();
    let id = TabId::test(0);
    assert!(!pages.set_cursor(id, CursorStyle::PointingHand));
    assert_eq!(pages.cursor(id), CursorStyle::Arrow);
}
