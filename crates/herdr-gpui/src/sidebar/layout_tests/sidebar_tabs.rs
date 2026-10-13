//! The sidebar with the shown space's tabs listed under the spaces: the
//! spaces shrink to one line each, and every tab row carries its own close.

use super::*;
use crate::config::{Density, LayoutMode, Style};
use gpui::MouseButton;

fn tabs_fixture(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        view.config.layout.mode = LayoutMode::Classic {
            density: Density::Comfortable,
            style: Style::Rounded,
        };
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("w0".into());
        snapshot.workspaces[0].git_ahead_behind = Some((0, 18));
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

fn draw(cx: &mut gpui::VisualTestContext) {
    cx.update(|window, cx| {
        cx.default_global::<TextProbes>().0.clear();
        full_draw(window, cx).clear(cx);
    });
}

#[gpui::test]
fn spaces_take_one_line_each_under_a_tabs_list(cx: &mut gpui::TestAppContext) {
    let (view, cx) = tabs_fixture(cx);
    draw(cx);
    let line = view.read_with(cx, |view, _| view.config.sidebar.size * 4. / 3.);
    // Comfortable rounded rows: a trim of 3 inside the highlight and twice
    // that between rows. The spaces drop the density's own 4 and their
    // branch line; the tab rows under them keep both.
    let (trim, comfortable) = (3., 4.);
    let row = cx.debug_bounds("row-herdr").unwrap();
    assert_eq!(row.size.height, px(line + 2. * trim + 2. * trim));
    assert!(cx.debug_bounds("detail-herdr").is_none());
    let tab = cx.debug_bounds("row-tab-t0").unwrap();
    assert_eq!(
        tab.size.height,
        px(line + 2. * (comfortable + trim) + 2. * trim)
    );
    assert_eq!(tab.left(), row.left());
    assert_eq!(tab.right(), row.right());
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        assert!(probes.contains_key("main"));
        assert!(probes.contains_key("\u{2193}18"));
    });
    let name = cx.debug_bounds("name-herdr").unwrap();
    let upstream = cx.debug_bounds("upstream-herdr").unwrap();
    assert_eq!(name.size.height, px(line));
    assert_eq!(upstream.top(), name.top());
    assert!(upstream.left() >= name.right());

    view.update(cx, |view, cx| {
        view.config.show_sidebar_tabs = false;
        cx.notify();
    });
    draw(cx);
    let row = cx.debug_bounds("row-herdr").unwrap();
    assert_eq!(
        row.size.height,
        px(2. * line + 2. * (comfortable + trim) + 2. * trim)
    );
    assert!(cx.debug_bounds("detail-herdr").is_some());
}

#[gpui::test]
fn every_tab_row_closes_from_its_own_x(cx: &mut gpui::TestAppContext) {
    let (view, cx) = tabs_fixture(cx);
    draw(cx);
    for tab in ["t0", "t1"] {
        let row = cx.debug_bounds(format!("row-tab-{tab}").leak()).unwrap();
        let close = cx.debug_bounds(format!("tab-close-{tab}").leak()).unwrap();
        let column = cx.debug_bounds(format!("column-tab-{tab}").leak()).unwrap();
        assert_eq!(close.center().y, row.center().y);
        assert!(close.right() <= row.right());
        assert!(column.right() <= close.left());
    }
    let close = cx.debug_bounds("tab-close-t1").unwrap();
    cx.simulate_mouse_down(close.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(close.center(), MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        // The press reaches the close alone: the tab is asked to close, and
        // neither focused nor lifted by the row under the x.
        assert!(view.menu.close.is_some());
        assert_eq!(view.menu.page, Some(crate::menu::Page::ConfirmClose));
        assert!(view.pending_navigation.is_none());
        assert!(view.workspace_drag.is_none());
        assert!(view.tab_row_drag.is_none());
    });
}

#[gpui::test]
fn holding_a_tab_row_lifts_it_and_a_release_picks_the_gap(cx: &mut gpui::TestAppContext) {
    use gpui::point;

    let (view, cx) = tabs_fixture(cx);
    view.update(cx, |view, _| {
        view.live.status = crate::state::ConnectionStatus::Connected;
        view.live.supports_tab_move = true;
    });
    draw(cx);
    let target = |view: &Entity<HerdrWindow>, cx: &mut gpui::VisualTestContext| {
        view.read_with(cx, |view, _| {
            let drag = view.tab_row_drag.as_ref()?;
            assert!(drag.lifted);
            drag.target
        })
    };
    let first = cx.debug_bounds("row-tab-t0").unwrap();
    let second = cx.debug_bounds("row-tab-t1").unwrap();
    assert_eq!(second.top(), first.bottom());

    // A quick click stays a click.
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    cx.simulate_mouse_up(first.center(), MouseButton::Left, Modifiers::default());
    cx.executor().advance_clock(crate::reorder::LIFT_DELAY * 2);
    cx.run_until_parked();
    view.update(cx, |view, _| {
        assert!(view.tab_row_drag.is_none());
        assert!(view.pending_navigation.take().is_some());
    });

    // Held in place, the row lifts without moving.
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| {
        assert!(!view.tab_row_drag.as_ref().unwrap().lifted)
    });
    cx.executor().advance_clock(crate::reorder::LIFT_DELAY);
    cx.run_until_parked();
    draw(cx);
    assert_eq!(target(&view, cx), None);
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap(), second);

    // Past the second row's middle, it lands after it: the daemon's gap 2.
    let below = point(first.center().x, second.bottom() - px(2.));
    cx.simulate_mouse_move(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    assert_eq!(target(&view, cx), Some(2));
    // The second row closes the lifted one's place.
    let lifted = cx.debug_bounds("row-tab-t0").unwrap();
    assert_eq!(lifted.top() - first.top(), below.y - first.center().y);
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap().top(), first.top());

    // The release is the drop, not a click on the row under it. Nothing was
    // sent without a daemon, so every row is back in place.
    cx.simulate_mouse_up(below, MouseButton::Left, Modifiers::default());
    draw(cx);
    view.read_with(cx, |view, _| {
        assert!(view.tab_row_drag.is_none());
        assert!(view.pending_navigation.is_none());
    });
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), first);
    assert_eq!(cx.debug_bounds("row-tab-t1").unwrap(), second);

    // Moving a press lifts it without waiting, and the gap resolves against
    // the lifted frame's layout. Escape puts the row back.
    cx.simulate_mouse_down(second.center(), MouseButton::Left, Modifiers::default());
    let above = point(second.center().x, first.top() + px(2.));
    for _ in 0..2 {
        cx.simulate_mouse_move(above, MouseButton::Left, Modifiers::default());
        draw(cx);
    }
    assert_eq!(target(&view, cx), Some(0));
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap().top(), second.top());
    cx.simulate_keystrokes("escape");
    draw(cx);
    view.read_with(cx, |view, _| assert!(view.tab_row_drag.is_none()));
    assert_eq!(cx.debug_bounds("row-tab-t0").unwrap(), first);

    // A daemon without `tab.move` leaves the rows where they are.
    view.update(cx, |view, _| view.live.supports_tab_move = false);
    cx.simulate_mouse_down(first.center(), MouseButton::Left, Modifiers::default());
    view.read_with(cx, |view, _| assert!(view.tab_row_drag.is_none()));
    cx.simulate_mouse_up(first.center(), MouseButton::Left, Modifiers::default());
}
