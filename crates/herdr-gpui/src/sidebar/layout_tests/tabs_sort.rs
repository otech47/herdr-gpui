//! The tabs list's attention sort: the tabs that want an answer first, then
//! the rest by how much they need one, each group in the daemon's order.

use super::tabs_views::{draw, two_devices};
use super::*;
use crate::{
    preferences::{TabsSort, TabsView},
    state::ConnectionStatus,
    usage::Host,
};
use gpui::MouseButton;
use herdr_client::protocol::AgentStatus;

/// One device whose shown space holds six tabs, t0 to t5, with every status
/// once: idle, working, blocked, done, unknown, and a working claude tab
/// whose plan is used up.
fn six_tabs(cx: &mut gpui::TestAppContext) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        let mut local = snapshot(1);
        local.focused_workspace_id = Some("w0".into());
        let template = local.tabs[0].clone();
        local.tabs = [
            AgentStatus::Idle,
            AgentStatus::Working,
            AgentStatus::Blocked,
            AgentStatus::Done,
            AgentStatus::Unknown,
            AgentStatus::Working,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, status)| {
            let mut tab = template.clone();
            tab.tab_id = format!("t{i}");
            tab.label = format!("tab {}", i + 1);
            tab.custom_label = true;
            tab.focused = i == 0;
            tab.agent_status = status;
            tab
        })
        .collect();
        // p0 runs claude on t5, and claude's plan on this host is used up.
        local.agents.truncate(1);
        local.agents[0].tab_id = "t5".into();
        view.usage.seed(Host::Local, &[("claude", 100.)]);
        view.live.snapshot = Some(Arc::new(local));
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

fn listed(view: &HerdrWindow) -> Vec<String> {
    view.listed_tabs()
        .into_iter()
        .map(|row| row.tab.tab_id.clone())
        .collect()
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let position = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn the_attention_sort_ranks_blocked_limited_working_done_idle_unknown(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = six_tabs(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4", "t5"]);
        assert_eq!(view.listed_tab(1), None);
    });
    view.update(cx, |view, _| view.sidebar_tabs_sort = TabsSort::Attention);
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t2", "t5", "t1", "t3", "t0", "t4"]);
        assert_eq!(
            view.listed_tab(1),
            Some(("local".to_owned(), "t2".to_owned()))
        );
        assert_eq!(
            view.listed_tab(6),
            Some(("local".to_owned(), "t4".to_owned()))
        );
        assert_eq!(view.listed_tab(7), None);
    });
    draw(cx);
    let tops: Vec<Pixels> = [
        "row-tab-t2",
        "row-tab-t5",
        "row-tab-t1",
        "row-tab-t3",
        "row-tab-t0",
        "row-tab-t4",
    ]
    .into_iter()
    .map(|selector| cx.debug_bounds(selector).unwrap().top())
    .collect();
    assert!(tops.windows(2).all(|pair| pair[0] < pair[1]), "{tops:?}");
}

#[gpui::test]
fn the_header_toggle_flips_the_sort_and_remembers_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = six_tabs(cx);
    draw(cx);
    let toggle = cx.debug_bounds("tabs-sort").unwrap();
    let label = cx.debug_bounds("header-label-tabs").unwrap();
    assert!(label.right() <= toggle.left());
    assert!(cx.debug_bounds("tabs-view").is_none());
    for expected in [TabsSort::Attention, TabsSort::Manual] {
        click(cx, "tabs-sort");
        draw(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.sidebar_tabs_sort, expected);
            assert!(view.sidebar_sections_modified);
            assert!(!view.sidebar_collapsed.is(crate::preferences::Section::Tabs));
        });
        let first = cx.debug_bounds("row-tab-t0").unwrap().top();
        let blocked = cx.debug_bounds("row-tab-t2").unwrap().top();
        assert_eq!(blocked < first, expected == TabsSort::Attention);
    }
}

#[gpui::test]
fn a_sorted_list_arms_no_row_drag(cx: &mut gpui::TestAppContext) {
    let (view, cx) = six_tabs(cx);
    view.update(cx, |view, _| {
        view.live.status = ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
    });
    for (sort, armed) in [(TabsSort::Manual, true), (TabsSort::Attention, false)] {
        view.update(cx, |view, _| view.sidebar_tabs_sort = sort);
        draw(cx);
        let row = cx.debug_bounds("row-tab-t1").unwrap().center();
        cx.simulate_mouse_down(row, MouseButton::Left, Modifiers::default());
        view.read_with(cx, |view, _| {
            assert_eq!(view.tab_row_drag.is_some(), armed, "{sort:?}");
        });
        cx.simulate_mouse_up(row, MouseButton::Left, Modifiers::default());
        draw(cx);
    }
}

#[gpui::test]
fn across_devices_the_sort_mixes_rows_unless_the_view_groups_them(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_sort = TabsSort::Attention;
        let remote = Arc::make_mut(view.endpoints[1].live.snapshot.as_mut().unwrap());
        remote.tabs[1].agent_status = AgentStatus::Blocked;
        view.settle_attention();
    });
    for (tabs_view, expected) in [
        (TabsView::Device, vec!["t0", "t1"]),
        (TabsView::All, vec!["r1", "t0", "t1", "r0"]),
        (TabsView::ByDevice, vec!["t0", "t1", "r1", "r0"]),
    ] {
        view.update(cx, |view, _| view.sidebar_tabs_view = tabs_view);
        view.read_with(cx, |view, _| {
            assert_eq!(listed(view), expected, "{tabs_view:?}");
        });
    }
}
