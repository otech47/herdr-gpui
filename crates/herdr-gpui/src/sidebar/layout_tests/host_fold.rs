use super::*;

#[gpui::test]
fn devices_start_folded_and_an_arrow_opens_one(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        let view = cx.new(|cx| {
            let mut view = fixture_window(window, cx);
            view.live.snapshot = Some(Arc::new(snapshot(2)));
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:test".into(),
                "Mini".into(),
                ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live.snapshot = view.live.snapshot.clone();
            view.endpoints.push(remote);
            view
        });
        cx.observe(&view, |_, _, cx| cx.notify()).detach();
        SidebarFixture(view)
    });
    let view = cx.update(|_, cx| fixture.read(cx).0.clone());
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    for header in ["host-local", "host-ssh:test"] {
        assert!(cx.debug_bounds(header).is_some(), "{header}");
    }
    for row in ["workspace-local-w0", "workspace-ssh:test-w0"] {
        assert!(cx.debug_bounds(row).is_none(), "{row}");
    }
    view.read_with(cx, |view, _| assert!(view.sidebar_workspaces().is_empty()));

    let arrow = cx.debug_bounds("collapse-host-ssh:test").unwrap().center();
    cx.simulate_click(arrow, Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("workspace-ssh:test-w0").is_some());
    assert!(cx.debug_bounds("workspace-local-w0").is_none());
    view.read_with(cx, |view, _| {
        assert_eq!(view.sidebar_workspaces(), [(1, "w0"), (1, "w1")]);
    });
}
