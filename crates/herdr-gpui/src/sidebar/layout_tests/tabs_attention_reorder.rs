//! Dragging a tab row under the attention sort: a row moves only among the
//! rows of its rank, the drop holds in the ledger until a row changes rank,
//! and the daemon is asked to put the tab beside its own device's neighbor.

use super::tabs_attention::{five_tabs, listed};
use super::tabs_views::{draw, two_devices};
use super::tabs_views_reorder::lift;
use super::*;
use crate::{
    preferences::{TabsSort, TabsView},
    sidebar::tab_reorder::Hold,
    state::ConnectionStatus,
};
use gpui::{MouseButton, point};
use herdr_client::protocol::AgentStatus;

fn pair(device: &str, tab: &str) -> (String, String) {
    (device.into(), tab.into())
}

#[gpui::test]
fn a_row_moves_only_among_the_rows_of_its_rank(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
    });
    draw(cx);
    let done = cx.debug_bounds("row-tab-t2").unwrap();
    let third = cx.debug_bounds("row-tab-t3").unwrap();
    let fourth = cx.debug_bounds("row-tab-t4").unwrap();
    lift(cx, fourth.center());

    // Carried past the idle row above it and on over a done one, it lands
    // first among the idle rows; the done row stays put.
    let above = point(fourth.center().x, done.top() + px(2.));
    cx.simulate_mouse_move(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        let drag = view.tab_row_drag.as_ref().unwrap();
        assert_eq!(drag.target, Some(3));
        let moved = view.tab_move(&drag.key, &drag.boot, 3).unwrap();
        assert_eq!(
            (moved.endpoint, moved.tab.as_str(), moved.insert),
            (0, "t4", Some(3))
        );
        assert!(matches!(
            moved.hold,
            Hold::Ledger(5, ref tabs) if *tabs == [pair("local", "t4"), pair("local", "t3")]
        ));
    });
    assert_eq!(cx.debug_bounds("row-tab-t2").unwrap(), done);
    assert_eq!(cx.debug_bounds("row-tab-t3").unwrap().top(), fourth.top());

    // Dropped, the order holds without the daemon's answer.
    cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.tab_row_drag.is_none());
        assert_eq!(listed(view), ["t0", "t1", "t2", "t4", "t3"]);
    });
    assert_eq!(cx.debug_bounds("row-tab-t4").unwrap(), third);
    assert_eq!(cx.debug_bounds("row-tab-t3").unwrap(), fourth);

    // Until a row changes rank: a finished tab takes the top of its new group.
    view.update(cx, |view, _| {
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot
            .tabs
            .iter_mut()
            .find(|tab| tab.tab_id == "t3")
            .unwrap()
            .agent_status = AgentStatus::Done;
        view.settle_attention();
        assert_eq!(listed(view), ["t0", "t3", "t1", "t2", "t4"]);
    });
}

#[gpui::test]
fn a_rank_of_one_row_does_not_arm(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
    });
    draw(cx);
    let working = cx.debug_bounds("row-tab-t0").unwrap();
    lift(cx, working.center());
    view.read_with(cx, |view, _| assert!(view.tab_row_drag.is_none()));
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), working);
}

#[gpui::test]
fn across_devices_the_drop_lands_beside_the_tab_s_own_neighbor(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::All;
        view.sidebar_tabs_sort = TabsSort::Attention;
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
        view.endpoints[1].live.status = ConnectionStatus::Connected;
        view.endpoints[1].live.supports_tab_move = true;
    });
    draw(cx);
    let first = cx.debug_bounds("row-tab-t0").unwrap();
    let remote_last = cx.debug_bounds("row-tab-ssh:test-r1").unwrap();
    lift(cx, first.center());

    // Every row shares the rank, so the local row may pass the remote ones.
    // The daemon puts it after the local row it now follows.
    let below = point(first.center().x, remote_last.bottom() - px(2.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        let drag = view.tab_row_drag.as_ref().unwrap();
        assert_eq!(drag.target, Some(4));
        let moved = view.tab_move(&drag.key, &drag.boot, 4).unwrap();
        assert_eq!(
            (moved.endpoint, moved.tab.as_str(), moved.insert),
            (0, "t0", Some(2))
        );
        assert!(matches!(
            moved.hold,
            Hold::Ledger(2, ref tabs) if *tabs == [
                pair("local", "t1"),
                pair("ssh:test", "r0"),
                pair("ssh:test", "r1"),
                pair("local", "t0"),
            ]
        ));
    });
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t1", "r0", "r1", "t0"]);
    });
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), remote_last);

    // A remote row carried to the top lands before its device's first row.
    let remote_last = cx.debug_bounds("row-tab-ssh:test-r1").unwrap();
    lift(cx, remote_last.center());
    let above = point(remote_last.center().x, first.top() + px(2.));
    cx.simulate_mouse_move(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        let drag = view.tab_row_drag.as_ref().unwrap();
        assert_eq!(drag.target, Some(0));
        let moved = view.tab_move(&drag.key, &drag.boot, 0).unwrap();
        assert_eq!(
            (moved.endpoint, moved.tab.as_str(), moved.insert),
            (1, "r1", Some(0))
        );
    });
    cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["r1", "t1", "r0", "t0"]);
    });
    assert_eq!(cx.debug_bounds("row-tab-ssh:test-r1").unwrap(), first);
}
