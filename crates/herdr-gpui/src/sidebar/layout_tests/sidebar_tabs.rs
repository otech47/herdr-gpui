//! The sidebar with the shown space's tabs listed under the spaces: every
//! tab row carries its own close.

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
