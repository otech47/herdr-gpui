//! The rows of the view listing every device's tabs together: the device's
//! name, drawn small at the line's end, takes the number's place.

use super::tabs_views::{draw, two_devices};
use super::*;
use crate::preferences::TabsView;

#[gpui::test]
fn the_all_views_rows_end_in_a_small_device_name(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| view.sidebar_tabs_view = TabsView::All);
    draw(cx);
    // t1 runs no agent, so its row is titled after the tab itself.
    let local_line = cx.debug_bounds("name-tab-t1").unwrap();
    let remote_line = cx.debug_bounds("name-tab-ssh:test-r0").unwrap();
    let local_icon = cx.debug_bounds("agent-icon-tab-t1").unwrap();
    let remote_icon = cx.debug_bounds("agent-icon-tab-ssh:test-r0").unwrap();
    let local = cx.debug_bounds("name-tab-t1-tag").unwrap();
    let mini = cx.debug_bounds("name-tab-ssh:test-r0-tag").unwrap();
    cx.update(|_, cx| {
        let probes = &cx.global::<TextProbes>().0;
        let (title, _, _) = probes["tab 2"];
        let (remote_title, _, _) = probes["remote r0"];
        assert_eq!(title.left(), remote_title.left());
        assert_eq!(title.left(), local_icon.right() + px(4.));
        assert_eq!(remote_icon.left(), local_icon.left());
        // The name ends the line on a row with an x (local) and one without (mini).
        assert!(local.left() > title.right());
        assert!(mini.left() > remote_title.right());
        assert_eq!(local.right(), mini.right());
        assert_eq!(local.right(), local_line.right());
        assert_eq!(mini.right(), remote_line.right());
        assert_eq!(local.center().y, local_line.center().y);
    });
}

#[gpui::test]
fn the_device_views_rows_keep_their_number(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    for tabs_view in [TabsView::Device, TabsView::ByDevice] {
        view.update(cx, |view, _| view.sidebar_tabs_view = tabs_view);
        draw(cx);
        cx.update(|_, cx| {
            let probes = &cx.global::<TextProbes>().0;
            let (number, _, _) = probes["2"];
            let (title, _, _) = probes["tab 2"];
            assert!(number.right() < title.left(), "{tabs_view:?}");
        });
    }
}
