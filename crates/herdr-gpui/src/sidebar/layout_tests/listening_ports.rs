use super::*;
use crate::sidebar::ports::{CLOSE_GRACE, OPEN_DELAY};
use std::time::Instant;

#[gpui::test]
fn listening_ports_hang_a_globe_on_their_row(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| fixture_window(window, cx));
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let (id, label, next) = cx.update(|_, cx| {
        let snapshot = view.read(cx).live.snapshot.clone().unwrap();
        let [first, second, ..] = snapshot.workspaces.as_slice() else {
            panic!("the fixture lists several workspaces");
        };
        (
            first.workspace_id.clone(),
            first.label.clone(),
            second.label.clone(),
        )
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    let poll = |cx: &mut gpui::VisualTestContext, now: Instant| {
        cx.update(|window, cx| view.update(cx, |view, cx| view.poll_ports_menu(now, window, cx)));
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    let row_key: &'static str = format!("row-{label}").leak();
    let column_key: &'static str = format!("column-{label}").leak();
    let below_key: &'static str = format!("row-{next}").leak();
    let row = cx.debug_bounds(row_key).unwrap();
    let column = cx.debug_bounds(column_key).unwrap();
    let below = cx.debug_bounds(below_key).unwrap();
    // Probe names are static; leaking a few in a test is fine.
    let globe: &'static str = format!("ports-local-{id}").leak();
    assert!(cx.debug_bounds(globe).is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            let ports = format!("L 1 *:3000 node\nL 1 127.0.0.1:5173 vite\nE 1 {id}\n");
            view.listening_ports.seed(
                crate::usage::Host::Local,
                crate::listening_ports::parse(&ports).unwrap(),
            );
            cx.notify();
        })
    });
    draw(cx);
    // The globe takes its room from the label column, not from the list.
    let globe_bounds = cx.debug_bounds(globe).unwrap();
    assert_eq!(cx.debug_bounds(row_key).unwrap(), row);
    assert_eq!(cx.debug_bounds(below_key).unwrap(), below);
    assert!(globe_bounds.top() >= row.top() && globe_bounds.bottom() <= row.bottom());
    assert!(globe_bounds.right() <= row.right());
    assert!(cx.debug_bounds(column_key).unwrap().right() <= globe_bounds.left());
    assert!(cx.debug_bounds(column_key).unwrap().right() < column.right());
    assert!(cx.debug_bounds("ports-menu").is_none());

    // Leaving before the delay cancels the dwell.
    cx.simulate_mouse_move(globe_bounds.center(), None, Modifiers::default());
    cx.simulate_mouse_move(below.center(), None, Modifiers::default());
    poll(cx, Instant::now() + OPEN_DELAY);
    assert!(cx.debug_bounds("ports-menu").is_none());

    // A rest on it opens the menu over it, after the delay.
    cx.simulate_mouse_move(globe_bounds.center(), None, Modifiers::default());
    poll(cx, Instant::now());
    assert!(cx.debug_bounds("ports-menu").is_none());
    poll(cx, Instant::now() + OPEN_DELAY);
    let menu = cx.debug_bounds("ports-menu").unwrap();
    assert_eq!(menu.top_right(), globe_bounds.top_right());
    assert!(menu.contains(&globe_bounds.center()));
    let line = cx
        .debug_bounds(format!("port-menu-local-{id}-3000").leak())
        .unwrap();
    assert!(menu.contains(&line.center()));
    assert!(
        cx.debug_bounds(format!("port-menu-local-{id}-5173").leak())
            .is_some()
    );

    // Moving into a line keeps it, and leaving both it and the globe closes
    // it only after the grace.
    cx.simulate_mouse_move(line.center(), None, Modifiers::default());
    poll(cx, Instant::now());
    assert!(cx.debug_bounds("ports-menu").is_some());
    let left = Instant::now();
    cx.simulate_mouse_move(below.center(), None, Modifiers::default());
    poll(cx, left);
    assert!(cx.debug_bounds("ports-menu").is_some());
    cx.simulate_mouse_move(line.center(), None, Modifiers::default());
    poll(cx, left + CLOSE_GRACE);
    assert!(cx.debug_bounds("ports-menu").is_some());
    let left = Instant::now();
    cx.simulate_mouse_move(below.center(), None, Modifiers::default());
    poll(cx, left);
    poll(cx, left + CLOSE_GRACE);
    assert!(cx.debug_bounds("ports-menu").is_none());
    assert!(cx.debug_bounds(globe).is_some());

    // Turning the feature off takes the globe away and gives the label its
    // room back.
    cx.update(|_, cx| {
        view.update(cx, |view, cx| {
            view.config.show_listening_ports = false;
            cx.notify();
        })
    });
    draw(cx);
    assert!(cx.debug_bounds(globe).is_none());
    assert_eq!(cx.debug_bounds(column_key).unwrap(), column);
}
