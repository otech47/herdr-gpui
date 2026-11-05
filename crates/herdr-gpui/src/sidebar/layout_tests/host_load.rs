use super::*;
use crate::usage::Host;

/// A local host and a remote one, both with a held load reading, in a sidebar
/// `width` wide. `show` is the load setting.
fn hosts_with_load(
    width: f32,
    show: bool,
    cx: &mut gpui::TestAppContext,
) -> &mut gpui::VisualTestContext {
    let (_, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.config.layout.mode = crate::config::LayoutMode::new(
                crate::config::Density::Comfortable,
                crate::config::Style::Rounded,
            );
            view.sidebar_width = Some(width);
            view.config.show_system_load = show;
            view.live.snapshot = Some(Arc::new(snapshot(1)));
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Devbox".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = view.live.snapshot.clone();
            view.endpoints.push(remote);
            view.system_load.hold(Host::Local, 40., 60.);
            view.system_load.hold(Host::Ssh("unused".into()), 5., 20.);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    cx.simulate_resize(size(px(1200.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx
}

const HOSTS: [(&str, &str, &str, &str); 2] = [
    (
        "local",
        "host-local",
        "collapse-host-local",
        "host-load-local",
    ),
    (
        "ssh:test",
        "host-ssh:test",
        "collapse-host-ssh:test",
        "host-load-ssh:test",
    ),
];

/// The load graph's width in the fixture's 12 pixel sidebar font.
fn graph_width() -> f32 {
    crate::system_load::line_width(12. * 0.62)
}

fn assert_one_line(width: f32, cx: &mut gpui::TestAppContext) {
    let plain: Vec<Pixels> = {
        let cx = hosts_with_load(width, false, cx);
        HOSTS
            .iter()
            .map(|(_, row, _, _)| cx.debug_bounds(row).unwrap().size.height)
            .collect()
    };
    let cx = hosts_with_load(width, true, cx);
    for ((id, row, arrow, load), plain) in HOSTS.into_iter().zip(plain) {
        let arrow = cx.debug_bounds(arrow).unwrap();
        let load = cx.debug_bounds(load).unwrap();
        assert!(
            (load.center().y - arrow.center().y).abs() <= px(1.),
            "{id}: load sits beside the name"
        );
        assert_eq!(cx.debug_bounds(row).unwrap().size.height, plain, "{id}");
    }
}

#[gpui::test]
fn a_wide_host_row_keeps_the_load_graph_on_the_name_line(cx: &mut gpui::TestAppContext) {
    assert_one_line(340., cx);
    let cx = hosts_with_load(340., true, cx);
    let load = cx.debug_bounds("host-load-ssh:test").unwrap();
    assert!((load.size.width - px(graph_width())).abs() <= px(1.));
    assert!(cx.debug_bounds("system-load-gauges").is_none());
}

#[gpui::test]
fn a_narrow_host_row_trades_the_graph_for_gauges(cx: &mut gpui::TestAppContext) {
    assert_one_line(232., cx);
    let cx = hosts_with_load(232., true, cx);
    let load = cx.debug_bounds("host-load-ssh:test").unwrap();
    assert!(load.size.width < px(graph_width() / 2.));
    assert!(cx.debug_bounds("system-load-gauges").is_some());
}
