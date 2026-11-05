use super::*;
use crate::usage::Host;

/// A local host and a remote one named `label`, both with a held load
/// reading, in a sidebar `width` wide. `show` is the load setting.
fn hosts_with_load<'a>(
    width: f32,
    label: &str,
    show: bool,
    cx: &'a mut gpui::TestAppContext,
) -> &'a mut gpui::VisualTestContext {
    let label = label.to_owned();
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
                label,
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

/// Each host row keeps its load on the name's line, in the same width.
fn assert_compact(width: f32, label: &str, cx: &mut gpui::TestAppContext) -> Pixels {
    let plain: Vec<Pixels> = {
        let cx = hosts_with_load(width, label, false, cx);
        HOSTS
            .iter()
            .map(|(_, row, _, _)| cx.debug_bounds(row).unwrap().size.height)
            .collect()
    };
    let cx = hosts_with_load(width, label, true, cx);
    let mut widths = Vec::new();
    for ((id, row, arrow, load), plain) in HOSTS.into_iter().zip(plain) {
        let row = cx.debug_bounds(row).unwrap();
        let arrow = cx.debug_bounds(arrow).unwrap();
        let load = cx.debug_bounds(load).unwrap();
        assert!(
            (load.center().y - arrow.center().y).abs() <= px(1.),
            "{id}: load sits beside the name"
        );
        assert!(load.right() <= row.right(), "{id}: load stays in the row");
        assert_eq!(row.size.height, plain, "{id}");
        widths.push(load.size.width);
    }
    assert_eq!(widths[0], widths[1]);
    widths[0]
}

#[gpui::test]
fn every_host_row_shows_the_same_compact_load(cx: &mut gpui::TestAppContext) {
    let short = assert_compact(340., "Devbox", cx);
    let long = assert_compact(340., "Devbox contributors on the netcup server", cx);
    let narrow = assert_compact(232., "Devbox contributors on the netcup server", cx);
    assert_eq!(short, long);
    assert_eq!(short, narrow);
}

#[gpui::test]
fn hovering_a_host_s_load_shows_its_card_at_once(cx: &mut gpui::TestAppContext) {
    let cx = hosts_with_load(340., "Devbox", true, cx);
    assert!(cx.debug_bounds("system-load-card").is_none());
    let load = cx.debug_bounds("host-load-ssh:test").unwrap();
    cx.simulate_mouse_move(load.center(), None, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    let card = cx.debug_bounds("system-load-card").unwrap();
    assert!(card.size.width > load.size.width);
}
