//! Clicks on links in a pane that prints while the button is down, so the
//! content revision at the release is not the one at the press.

use super::*;
use gpui::MouseButton;

const LINK: &str = "https://example.com/a";

/// A 40-column pane showing `text` on its first row, which the row-local
/// detector reads whole.
fn row(content_revision: u64, text: &str) -> Arc<PaneSurfaceFrame> {
    let snapshot = snapshot();
    let (width, height) = (40, 4);
    let mut symbols = text.chars();
    let rect = SurfaceRect {
        x: 0,
        y: 0,
        width,
        height,
    };
    Arc::new(PaneSurfaceFrame {
        boot_id: snapshot.boot_id,
        projection_revision: snapshot.revision,
        surface_revision: 1,
        frame: FrameData {
            width,
            height,
            cells: (0..usize::from(width) * usize::from(height))
                .map(|_| CellData {
                    symbol: symbols.next().unwrap_or(' ').to_string(),
                    fg: 0,
                    bg: 0,
                    modifier: 0,
                    skip: false,
                    hyperlink: None,
                })
                .collect(),
            cursor: None,
            hyperlinks: vec![],
            graphics: vec![],
        },
        panes: vec![PaneSurfacePane {
            pane_id: "w1:p1".into(),
            content_revision,
            rect,
            inner_rect: rect,
            scrollbar_rect: None,
            scroll: None,
            focused: true,
            mouse_reporting: false,
            sgr_pixel_mouse: false,
            alternate_screen_active: false,
            pixel_width: 400,
            pixel_height: 80,
        }],
        splits: vec![],
        popup: None,
        graphics: Default::default(),
    })
}

fn show(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, surface: Arc<PaneSurfaceFrame>) {
    view.update(cx, |view, _| view.live.surface = Some(surface));
    cx.update(|window, cx| {
        window.refresh();
        window.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn a_click_opens_a_link_in_a_pane_that_keeps_printing(cx: &mut gpui::TestAppContext) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    show(&view, cx, row(2, LINK));
    let link = at(&view, cx, 3, 0);
    cx.simulate_mouse_down(link, MouseButton::Left, Modifiers::secondary_key());
    show(&view, cx, row(4, LINK));
    cx.simulate_mouse_up(link, MouseButton::Left, Modifiers::secondary_key());
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.activate");
    assert_eq!(request["params"]["content_revision"], 4);
    assert!(cx.opened_url().is_none());
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_activated", "url": LINK, "handled": false}),
    );
    assert_eq!(cx.opened_url().as_deref(), Some(LINK));
}

#[gpui::test]
fn a_release_during_a_write_opens_the_address_read_here(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    show(&view, cx, row(2, LINK));
    let link = at(&view, cx, 3, 0);
    cx.simulate_mouse_down(link, MouseButton::Left, Modifiers::secondary_key());
    // An odd revision is a write in progress, which the daemon would refuse.
    show(&view, cx, row(5, LINK));
    cx.simulate_mouse_up(link, MouseButton::Left, Modifiers::secondary_key());
    view.read_with(cx, |view, _| assert!(view.links.activating.is_none()));
    assert_eq!(cx.opened_url().as_deref(), Some(LINK));
}

#[gpui::test]
fn a_link_that_scrolled_away_during_the_press_does_not_open(cx: &mut gpui::TestAppContext) {
    let peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    show(&view, cx, row(2, LINK));
    let link = at(&view, cx, 3, 0);
    cx.simulate_mouse_down(link, MouseButton::Left, Modifiers::secondary_key());
    show(&view, cx, row(4, "done"));
    cx.simulate_mouse_up(link, MouseButton::Left, Modifiers::secondary_key());
    view.read_with(cx, |view, _| assert!(view.links.activating.is_none()));
    assert!(cx.opened_url().is_none());
}

#[gpui::test]
fn a_wrapped_link_is_still_asked_about_after_output_during_the_press(
    cx: &mut gpui::TestAppContext,
) {
    let mut peer = MockPeer::advertising(&["pane.link.resolve", "pane.link.activate"]);
    let (view, cx) = window(&peer, true, cx);
    let continuation = at(&view, cx, 2, 1);
    cx.simulate_mouse_move(continuation, None, Modifiers::secondary_key());
    resolve_after_delay(cx);
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.resolve");
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_resolved", "regions": [
            {"row": 0, "start_col": 0, "end_col": 19},
            {"row": 1, "start_col": 0, "end_col": 11},
        ]}),
    );
    cx.simulate_mouse_down(continuation, MouseButton::Left, Modifiers::secondary_key());
    show(&view, cx, wrapped(4));
    view.read_with(cx, |view, _| assert!(view.hovered_daemon_link().is_none()));
    cx.simulate_mouse_up(continuation, MouseButton::Left, Modifiers::secondary_key());
    let request = peer.request();
    assert_eq!(request["method"], "pane.link.activate");
    assert_eq!(request["params"]["content_revision"], 4);
    assert_eq!(request["params"]["viewport_row"], 1);
    answer(
        &mut peer,
        &view,
        cx,
        &request,
        json!({"type": "pane_link_activated", "url": URL, "handled": false}),
    );
    assert_eq!(cx.opened_url().as_deref(), Some(URL));
}
