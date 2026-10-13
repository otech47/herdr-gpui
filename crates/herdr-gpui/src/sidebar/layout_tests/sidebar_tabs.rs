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
    });
}
