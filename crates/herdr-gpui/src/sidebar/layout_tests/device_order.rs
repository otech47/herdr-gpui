//! Folded devices with nothing in the tabs list shrink, and the device order
//! setting leads every device listing.

use super::{
    tabs_views::{draw, two_devices},
    *,
};
use crate::preferences::TabsView;

fn height(cx: &mut gpui::VisualTestContext, selector: &'static str) -> Pixels {
    cx.debug_bounds(selector).unwrap().size.height
}

#[gpui::test]
fn a_folded_device_without_listed_tabs_is_a_small_line(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| view.sidebar_tabs_view = TabsView::Device);
    draw(cx);
    assert!(height(cx, "host-ssh:test") < height(cx, "host-local"));

    // The views across devices list the device's own "herdr" space.
    view.update(cx, |view, _| view.sidebar_tabs_view = TabsView::All);
    draw(cx);
    assert_eq!(height(cx, "host-ssh:test"), height(cx, "host-local"));
}

#[gpui::test]
fn space_rows_nest_under_the_device_name_at_its_height(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| view.endpoints[0].collapsed = false);
    draw(cx);
    let host = cx.debug_bounds("host-local").unwrap();
    let space = cx.debug_bounds("workspace-local-w0").unwrap();
    assert_eq!(space.size.height, host.size.height);
    assert_eq!(
        space.left() - host.left(),
        px(super::super::HOST_ARROW_WIDTH + super::super::HOST_GAP)
    );
    assert_eq!(space.right(), host.right());
}

#[gpui::test]
fn the_device_order_leads_the_sidebar_tabs_and_launcher(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.config.device_order = vec!["mini".into()];
        view.sidebar_tabs_view = TabsView::ByDevice;
    });
    draw(cx);
    let mini = cx.debug_bounds("host-ssh:test").unwrap();
    let local = cx.debug_bounds("host-local").unwrap();
    assert!(mini.bottom() <= local.top());
    let remote_tab = cx.debug_bounds("row-tab-ssh:test-r0").unwrap();
    let local_tab = cx.debug_bounds("row-tab-t0").unwrap();
    assert!(remote_tab.bottom() <= local_tab.top());
    view.read_with(cx, |view, _| {
        let choices: Vec<_> = view
            .launcher_choices()
            .into_iter()
            .map(|choice| choice.id)
            .collect();
        assert_eq!(choices, ["ssh:test", "local"]);
    });
}
