//! Tabs waiting on an answer outside the listed space: every window lists
//! them above its own tabs, latest first, until they stop waiting, and a
//! press moves to them.

use super::tabs_views::{draw, two_devices};
use super::tabs_views_reorder::lift;
use super::*;
use crate::{NavigationTarget, preferences::TabsView, state::ConnectionStatus};
use gpui::MouseButton;
use herdr_client::protocol::AgentStatus;

/// The two-device window, both connected, with an idle tab b0 in the local
/// second space and an idle tab rb in the Mini's "elsewhere" space.
fn spread(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.live.status = ConnectionStatus::Connected;
        view.endpoints[1].live.status = ConnectionStatus::Connected;
        add_tab(view, 0, "b0", "w1");
        add_tab(view, 1, "rb", "w0");
        view.settle_attention();
    });
    (view, cx)
}

fn snapshot_of(view: &mut HerdrWindow, endpoint: usize) -> &mut ClientShellSnapshot {
    let live = if endpoint == view.selected_endpoint {
        &mut view.live
    } else {
        &mut view.endpoints[endpoint].live
    };
    Arc::make_mut(live.snapshot.as_mut().unwrap())
}

fn add_tab(view: &mut HerdrWindow, endpoint: usize, id: &str, workspace: &str) {
    let snapshot = snapshot_of(view, endpoint);
    let mut tab = snapshot.tabs[0].clone();
    tab.tab_id = id.into();
    tab.workspace_id = workspace.into();
    tab.label = format!("blocked {id}");
    tab.custom_label = true;
    tab.focused = false;
    tab.agent_status = AgentStatus::Idle;
    snapshot.tabs.push(tab);
}

fn set_status(view: &mut HerdrWindow, endpoint: usize, id: &str, status: AgentStatus) {
    snapshot_of(view, endpoint)
        .tabs
        .iter_mut()
        .find(|tab| tab.tab_id == id)
        .unwrap()
        .agent_status = status;
    view.settle_attention();
}

fn waiting(view: &HerdrWindow) -> Vec<String> {
    view.waiting_elsewhere(&view.listed_tabs())
        .into_iter()
        .map(|row| row.tab.tab_id.clone())
        .collect()
}

#[gpui::test]
fn blocked_tabs_elsewhere_wait_above_the_list_latest_first(cx: &mut gpui::TestAppContext) {
    let (view, cx) = spread(cx);
    draw(cx);
    assert!(cx.debug_bounds("tabs-waiting").is_none());
    view.update(cx, |view, _| {
        assert!(waiting(view).is_empty());
        set_status(view, 0, "b0", AgentStatus::Blocked);
        set_status(view, 1, "rb", AgentStatus::Blocked);
        assert_eq!(waiting(view), ["rb", "b0"]);
    });
    draw(cx);
    let label = cx.debug_bounds("tabs-waiting").unwrap();
    let rb = cx.debug_bounds("row-waiting-ssh:test-rb").unwrap();
    let b0 = cx.debug_bounds("row-waiting-local-b0").unwrap();
    let t0 = cx.debug_bounds("row-tab-t0").unwrap();
    assert!(label.bottom() <= rb.top());
    assert!(rb.bottom() <= b0.top());
    assert!(b0.bottom() < t0.top());
    assert_eq!(b0.left(), t0.left());
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        for text in [
            "blocked rb",
            "elsewhere · Mini",
            "blocked b0",
            "herdr-gpui-side…",
            "1",
            "2",
        ] {
            assert!(probes.contains_key(text), "{text} not painted");
        }
        assert!(!probes.contains_key("3"));
    });
    view.update(cx, |view, _| {
        set_status(view, 1, "rb", AgentStatus::Working);
        assert_eq!(waiting(view), ["b0"]);
        set_status(view, 0, "b0", AgentStatus::Done);
    });
    draw(cx);
    assert!(cx.debug_bounds("tabs-waiting").is_none());
}

#[gpui::test]
fn a_listed_tab_waits_in_the_list_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = spread(cx);
    view.update(cx, |view, _| {
        set_status(view, 0, "t1", AgentStatus::Blocked);
        set_status(view, 1, "r0", AgentStatus::Blocked);
        assert_eq!(waiting(view), ["r0"]);
        view.sidebar_tabs_view = TabsView::All;
        assert!(waiting(view).is_empty());
    });
}

#[gpui::test]
fn a_disconnected_or_disabled_device_has_nothing_waiting(cx: &mut gpui::TestAppContext) {
    let (view, cx) = spread(cx);
    view.update(cx, |view, _| {
        set_status(view, 1, "rb", AgentStatus::Blocked);
        assert_eq!(waiting(view), ["rb"]);
        view.endpoints[1].live.status = ConnectionStatus::Disconnected;
        assert!(waiting(view).is_empty());
        view.endpoints[1].live.status = ConnectionStatus::Connected;
        view.endpoints[1].enabled = false;
        assert!(waiting(view).is_empty());
    });
}

#[gpui::test]
fn the_numbers_still_count_the_list(cx: &mut gpui::TestAppContext) {
    let (view, cx) = spread(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::All;
        set_status(view, 0, "b0", AgentStatus::Blocked);
        assert_eq!(view.listed_tab(1), Some(("local".into(), "t0".into())));
        assert_eq!(view.listed_tab(3), Some(("ssh:test".into(), "r0".into())));
        assert_eq!(view.listed_tab(5), None);
    });
}

#[gpui::test]
fn a_press_moves_to_the_waiting_tab(cx: &mut gpui::TestAppContext) {
    let (view, cx) = spread(cx);
    view.update(cx, |view, _| {
        set_status(view, 1, "rb", AgentStatus::Blocked)
    });
    draw(cx);
    let position = cx.debug_bounds("row-waiting-ssh:test-rb").unwrap().center();
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
    cx.run_until_parked();
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Tab("rb".into()))
        );
    });
}

#[gpui::test]
fn a_row_below_the_waiting_tabs_still_drags(cx: &mut gpui::TestAppContext) {
    let (view, cx) = spread(cx);
    view.update(cx, |view, _| {
        view.live.supports_tab_move = true;
        // With two rows a wrong child offset still passes, since the
        // waiting label is as tall as a row.
        add_tab(view, 0, "t2", "w0");
        set_status(view, 0, "b0", AgentStatus::Blocked);
    });
    draw(cx);
    let first = cx.debug_bounds("row-tab-t0").unwrap();
    let second = cx.debug_bounds("row-tab-t1").unwrap();
    let third = cx.debug_bounds("row-tab-t2").unwrap();
    lift(cx, third.center());
    let above = point(third.center().x, first.top() + px(2.));
    cx.simulate_mouse_move(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.tab_row_drag.as_ref().unwrap().target, Some(0));
    });
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap().top(), second.top());
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap().top(), third.top());
    cx.simulate_mouse_up(above, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(cx.debug_bounds("row-tab-t2").unwrap(), third);
}
