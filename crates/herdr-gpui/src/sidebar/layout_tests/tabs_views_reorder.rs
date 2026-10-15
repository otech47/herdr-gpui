//! Dragging a tab row in the views across devices: a row stays among its
//! device's rows, and a saved device's rows reorder on that device.

use super::tabs_views::{draw, two_devices};
use super::*;
use crate::{preferences::TabsView, reorder::LIFT_DELAY, state::ConnectionStatus};
use gpui::{MouseButton, Point, point};

fn lift(cx: &mut gpui::VisualTestContext, at: Point<Pixels>) {
    cx.simulate_mouse_down(at, MouseButton::Left, Modifiers::default());
    cx.executor().advance_clock(LIFT_DELAY);
    cx.run_until_parked();
    draw(cx);
}

#[gpui::test]
fn in_the_all_view_a_row_stays_among_its_devices_rows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::All;
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
    });
    draw(cx);
    let first = cx.debug_bounds("row-tab-t0").unwrap();
    let second = cx.debug_bounds("row-tab-t1").unwrap();
    let remote_first = cx.debug_bounds("row-tab-ssh:test-r0").unwrap();
    let remote_last = cx.debug_bounds("row-tab-ssh:test-r1").unwrap();
    lift(cx, first.center());

    // Carried past every remote row, it lands after the last local one and
    // the remote rows stay put.
    let below = point(first.center().x, remote_last.bottom() - px(2.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        let drag = view.tab_row_drag.as_ref().unwrap();
        assert_eq!(drag.target, Some(2));
        let moved = view.tab_move(&drag.key, &drag.boot, 2).unwrap();
        assert_eq!(
            (moved.endpoint, moved.tab.as_str(), moved.insert),
            (0, "t0", 2)
        );
        assert_eq!(moved.order, ["local/t0", "local/t1"]);
    });
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap().top(), first.top());
    assert_eq!(
        cx.debug_bounds("row-tab-ssh:test-r0").unwrap(),
        remote_first
    );
    assert_eq!(cx.debug_bounds("row-tab-ssh:test-r1").unwrap(), remote_last);

    // Without a daemon nothing is sent, so the release puts every row back.
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.tab_row_drag.is_none());
        assert!(view.pending_navigation.is_none());
    });
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), first);
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap(), second);
}

#[gpui::test]
fn a_saved_devices_row_drags_among_that_devices_rows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::All;
        view.endpoints[1].live.status = ConnectionStatus::Connected;
        view.endpoints[1].live.supports_tab_move = true;
    });
    draw(cx);
    let first = cx.debug_bounds("row-tab-t0").unwrap();
    let remote_first = cx.debug_bounds("row-tab-ssh:test-r0").unwrap();
    let remote_last = cx.debug_bounds("row-tab-ssh:test-r1").unwrap();
    lift(cx, remote_last.center());

    // Carried above every local row, it lands first among the remote rows:
    // that device's gap 0.
    let above = point(remote_last.center().x, first.top() + px(2.));
    cx.simulate_mouse_move(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        let drag = view.tab_row_drag.as_ref().unwrap();
        assert_eq!(drag.target, Some(2));
        let moved = view.tab_move(&drag.key, &drag.boot, 2).unwrap();
        assert_eq!(
            (moved.endpoint, moved.tab.as_str(), moved.insert),
            (1, "r1", 0)
        );
        assert_eq!(moved.order, ["ssh:test/r0", "ssh:test/r1"]);
    });
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), first);
    assert_eq!(
        cx.debug_bounds("row-tab-ssh:test-r0").unwrap().top(),
        remote_last.top()
    );

    cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.tab_row_drag.is_none());
        assert_eq!(view.selected_endpoint, 0);
    });
    assert_eq!(
        cx.debug_bounds("row-tab-ssh:test-r0").unwrap(),
        remote_first
    );
    assert_eq!(cx.debug_bounds("row-tab-ssh:test-r1").unwrap(), remote_last);
}

#[gpui::test]
fn the_grouped_view_drags_past_its_device_lines(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::ByDevice;
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
    });
    draw(cx);
    let first = cx.debug_bounds("row-tab-t0").unwrap();
    let second = cx.debug_bounds("row-tab-t1").unwrap();
    assert!(cx.debug_bounds("tabs-group-local").unwrap().bottom() <= first.top());
    lift(cx, first.center());

    let below = point(first.center().x, second.bottom() - px(2.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.tab_row_drag.as_ref().unwrap().target, Some(2));
    });
    // The second row closes exactly the lifted one's place; a shift that
    // counted the device line above would overshoot.
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap().top(), first.top());
    let lifted = cx.debug_bounds("row-tab-t0").unwrap();
    assert_eq!(lifted.top() - first.top(), below.y - first.center().y);

    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), first);
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap(), second);
}
