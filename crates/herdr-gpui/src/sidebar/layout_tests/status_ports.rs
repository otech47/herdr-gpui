//! The focused space's listening ports in the status bar: one globe, whose
//! menu stands on its top edge while the pointer rests on it.

use super::*;
use crate::sidebar::ports::{CLOSE_GRACE, OPEN_DELAY};
use std::time::Instant;

#[gpui::test]
fn the_status_bar_keeps_the_focused_space_s_ports_behind_a_globe(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("w0".into());
        let ports = "L 1 *:3000 node\nL 1 127.0.0.1:5173 vite\nE 1 w0\n";
        view.listening_ports.seed(
            crate::usage::Host::Local,
            crate::listening_ports::parse(ports).unwrap(),
        );
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    let poll = |cx: &mut gpui::VisualTestContext, now: Instant| {
        cx.update(|window, cx| view.update(cx, |view, cx| view.poll_ports_menu(now, window, cx)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    draw(cx);
    let bar = cx.debug_bounds("connection-status").unwrap();
    let globe = cx.debug_bounds("ports-bar-local-w0").unwrap();
    assert!(bar.contains(&globe.center()));
    assert!(cx.debug_bounds("port-local-w0-3000").is_none());
    assert!(cx.debug_bounds("ports-menu").is_none());

    // A rest on the globe opens the menu on its top edge, after the delay.
    cx.simulate_mouse_move(globe.center(), None, Modifiers::default());
    poll(cx, Instant::now());
    assert!(cx.debug_bounds("ports-menu").is_none());
    poll(cx, Instant::now() + OPEN_DELAY);
    let menu = cx.debug_bounds("ports-menu").unwrap();
    assert_eq!(menu.bottom_right(), globe.top_right());
    let line = cx.debug_bounds("port-menu-local-w0-3000").unwrap();
    assert!(menu.contains(&line.center()));
    assert!(cx.debug_bounds("port-menu-local-w0-5173").is_some());

    // Leaving both the globe and the menu closes it after the grace.
    cx.simulate_mouse_move(line.center(), None, Modifiers::default());
    poll(cx, Instant::now() + CLOSE_GRACE);
    assert!(cx.debug_bounds("ports-menu").is_some());
    let left = Instant::now();
    let away = point(bar.left() + px(4.), bar.center().y);
    cx.simulate_mouse_move(away, None, Modifiers::default());
    poll(cx, left);
    assert!(cx.debug_bounds("ports-menu").is_some());
    poll(cx, left + CLOSE_GRACE);
    assert!(cx.debug_bounds("ports-menu").is_none());
    assert!(cx.debug_bounds("ports-bar-local-w0").is_some());

    // The row's globe opens its own menu, hanging over the row.
    let row_globe = cx.debug_bounds("ports-local-w0").unwrap();
    cx.simulate_mouse_move(row_globe.center(), None, Modifiers::default());
    poll(cx, Instant::now() + OPEN_DELAY);
    let menu = cx.debug_bounds("ports-menu").unwrap();
    assert_eq!(menu.top_right(), row_globe.top_right());
}
