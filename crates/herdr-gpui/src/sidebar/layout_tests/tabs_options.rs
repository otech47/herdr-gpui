use super::tabs_views::{draw, two_devices};
use super::*;
use crate::{menu::Page, preferences::TabsSort};

#[gpui::test]
fn tabs_header_owns_new_tab_and_options_and_fits_a_narrow_sidebar(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| view.sidebar_width = Some(220.));
    draw(cx);
    let header = cx.debug_bounds("header-tabs").unwrap();
    let new_tab = cx.debug_bounds("sidebar-new-tab").unwrap();
    let options = cx.debug_bounds("tabs-options").unwrap();
    assert!(new_tab.top() >= header.top() && new_tab.bottom() <= header.bottom());
    assert!(new_tab.right() <= options.left());
    assert!(options.right() <= header.right());
    assert!(cx.debug_bounds("tabs-sort").is_none());
    assert!(cx.debug_bounds("tabs-view").is_none());
}

#[gpui::test]
fn options_support_keyboard_selection_and_escape_without_folding_tabs(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = two_devices(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_tabs_options(point(px(200.), px(100.)), window, cx);
        })
    });
    draw(cx);
    cx.simulate_keystrokes("enter");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_tabs_sort, TabsSort::Attention);
        assert!(view.menu.page.is_none());
        assert!(view.sidebar_sections_modified);
        assert!(!view.sidebar_collapsed.is(crate::preferences::Section::Tabs));
    });
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.open_tabs_options(point(px(200.), px(100.)), window, cx);
        })
    });
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::TabsOptions))
    });
    cx.simulate_keystrokes("down escape");
    draw(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_tabs_sort, TabsSort::Attention);
        assert!(view.menu.page.is_none());
    });
}
