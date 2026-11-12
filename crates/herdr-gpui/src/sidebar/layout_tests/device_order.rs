//! The device order setting leads every device listing.

use super::tabs_views::{draw, two_devices};
use crate::preferences::TabsView;

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
