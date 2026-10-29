//! The tabs list's three views: the selected device's shown space, every
//! device's space of that name in one list, and the same grouped by device.

use super::*;
use crate::{NavigationTarget, controls::Command, preferences::TabsView};
use gpui::MouseButton;

/// A window on the local "herdr" space, with a saved device whose second
/// space is also called "herdr" and holds the tabs r0 and r1.
pub(super) fn two_devices(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        let mut local = snapshot(2);
        local.focused_workspace_id = Some("w0".into());
        view.live.snapshot = Some(Arc::new(local));
        let mut remote = crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Mini".into(),
            ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        );
        let mut remote_snapshot = snapshot(2);
        remote_snapshot.workspaces[0].label = "elsewhere".into();
        remote_snapshot.workspaces[1].label = "herdr".into();
        for (tab, id) in remote_snapshot.tabs.iter_mut().zip(["r0", "r1"]) {
            tab.tab_id = id.into();
            tab.workspace_id = "w1".into();
            tab.label = format!("remote {id}");
            tab.custom_label = true;
        }
        remote.live.snapshot = Some(Arc::new(remote_snapshot));
        view.endpoints.push(remote);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

pub(super) fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        full_draw(window, cx).clear(cx);
    });
}

fn click(cx: &mut gpui::VisualTestContext, selector: &'static str) {
    let position = cx.debug_bounds(selector).unwrap().center();
    cx.simulate_mouse_down(position, MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(position, MouseButton::Left, Modifiers::default());
}

#[gpui::test]
fn the_all_view_lists_every_devices_tabs_and_a_click_moves_there(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| view.sidebar_tabs_view = TabsView::All);
    draw(cx);
    let rows: Vec<_> = [
        "row-tab-t0",
        "row-tab-t1",
        "row-tab-ssh:test-r0",
        "row-tab-ssh:test-r1",
    ]
    .into_iter()
    .map(|selector| cx.debug_bounds(selector).unwrap())
    .collect();
    for pair in rows.windows(2) {
        assert!(pair[0].bottom() <= pair[1].top());
    }
    assert_eq!(rows[0].left(), rows[2].left());
    assert!(cx.debug_bounds("tab-close-t0").is_some());
    assert!(cx.debug_bounds("tab-close-r0").is_none());
    assert!(cx.debug_bounds("tabs-group-local").is_none());
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        for text in ["Local", "Mini", "remote r0"] {
            assert!(probes.contains_key(text), "{text} not painted");
        }
        // The device's name takes the number's place.
        assert!(!probes.contains_key("3"));
    });
    click(cx, "row-tab-ssh:test-r0");
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Tab("r0".into()))
        );
        assert!(view.tab_row_drag.is_none());
    });
}

#[gpui::test]
fn the_by_device_view_names_each_device_above_its_tabs(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| view.sidebar_tabs_view = TabsView::ByDevice);
    draw(cx);
    let local = cx.debug_bounds("tabs-group-local").unwrap();
    let remote = cx.debug_bounds("tabs-group-ssh:test").unwrap();
    let t0 = cx.debug_bounds("row-tab-t0").unwrap();
    let t1 = cx.debug_bounds("row-tab-t1").unwrap();
    let r0 = cx.debug_bounds("row-tab-ssh:test-r0").unwrap();
    assert!(local.bottom() <= t0.top());
    assert!(t1.bottom() <= remote.top());
    assert!(remote.bottom() <= r0.top());
    assert_eq!(t0.left(), r0.left());
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        assert!(probes.contains_key("remote r0"));
    });
}

#[gpui::test]
fn the_device_view_lists_the_selected_devices_tabs_and_the_header_cycles_the_views(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = two_devices(cx);
    draw(cx);
    assert!(cx.debug_bounds("row-tab-t0").is_some());
    assert!(cx.debug_bounds("row-tab-ssh:test-r0").is_none());
    assert!(cx.debug_bounds("tabs-group-local").is_none());
    let toggle = cx.debug_bounds("tabs-options").unwrap();
    let label = cx.debug_bounds("header-label-tabs").unwrap();
    assert!(label.right() <= toggle.left());
    for expected in [TabsView::All, TabsView::ByDevice, TabsView::Device] {
        click(cx, "tabs-options");
        draw(cx);
        click(
            cx,
            match expected {
                TabsView::Device => "tabs-option-2",
                TabsView::All => "tabs-option-3",
                TabsView::ByDevice => "tabs-option-4",
            },
        );
        draw(cx);
        view.read_with(cx, |view, _| {
            assert_eq!(view.sidebar_tabs_view, expected);
            assert!(view.sidebar_sections_modified);
            assert!(!view.sidebar_collapsed.is(crate::preferences::Section::Tabs));
        });
        assert_eq!(
            cx.debug_bounds("row-tab-ssh:test-r0").is_some(),
            expected != TabsView::Device
        );
    }
}

#[gpui::test]
fn one_device_alone_has_no_view_to_choose(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        view.sidebar_tabs_view = TabsView::All;
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("w0".into());
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    draw(cx);
    assert!(cx.debug_bounds("tabs-view").is_none());
    assert!(cx.debug_bounds("row-tab-t0").is_some());
    assert!(cx.debug_bounds("tabs-group-local").is_none());
}

#[gpui::test]
fn cmd_numbers_count_across_devices_only_in_the_views_that_list_them(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = two_devices(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.listed_tab(3), None);
    });
    view.update(cx, |view, _| view.sidebar_tabs_view = TabsView::All);
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.listed_tab(1),
            Some(("local".to_owned(), "t0".to_owned()))
        );
        assert_eq!(
            view.listed_tab(4),
            Some(("ssh:test".to_owned(), "r1".to_owned()))
        );
        assert_eq!(view.listed_tab(0), None);
        assert_eq!(view.listed_tab(5), None);
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::TabNumber(3), window, cx)
        });
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(
            view.pending_navigation,
            Some(NavigationTarget::Tab("r0".into()))
        );
    });
}
