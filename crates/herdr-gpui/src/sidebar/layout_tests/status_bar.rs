use super::*;

#[gpui::test]
fn healthy_connection_status_is_quiet_but_diagnostics_remain(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Connected;
        view
    });
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    assert!(cx.debug_bounds("connection-message").is_none());
    assert!(cx.debug_bounds("status-theme").is_some());
    for status in [
        crate::state::ConnectionStatus::Connected,
        crate::state::ConnectionStatus::Disconnected,
    ] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.live.status = status;
                view.local_error = Some("Local operation failed".into());
                view.live.error = Some("Connection interrupted".into());
                cx.notify();
            });
            full_draw(window, cx).clear(cx);
        });
        assert!(cx.debug_bounds("connection-message").unwrap().size.width > px(0.));
    }
}

#[gpui::test]
fn usage_segments_stay_whole_in_a_crowded_bar(cx: &mut gpui::TestAppContext) {
    let (_view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.live.status = crate::state::ConnectionStatus::Disconnected;
        view.live.error = Some("The connection dropped while the daemon restarted".into());
        view.usage.seed(
            crate::usage::Host::Local,
            &[("claude", 20.), ("codex", 65.)],
        );
        view
    });
    let draw = |cx: &mut gpui::VisualTestContext| {
        cx.update(|window, cx| full_draw(window, cx).clear(cx));
    };
    let whole = |cx: &mut gpui::VisualTestContext| {
        let bar = cx.debug_bounds("connection-status").unwrap();
        let usage = cx.debug_bounds("usage").unwrap();
        let refresh = cx.debug_bounds("usage-refresh").unwrap();
        for provider in ["claude", "codex"] {
            let segment = cx.debug_bounds(format!("usage-{provider}").leak()).unwrap();
            assert!(segment.left() >= usage.left(), "{provider}");
            assert!(segment.right() <= usage.right(), "{provider}");
        }
        assert!(usage.right() <= refresh.left());
        assert!(refresh.right() <= bar.right());
        (bar, refresh.right() - usage.left())
    };
    cx.simulate_resize(size(px(1200.), px(400.)));
    draw(cx);
    let (bar, natural) = whole(cx);
    assert!(cx.debug_bounds("connection-message").unwrap().size.width > px(0.));

    // A bar just wider than the usage has no room left for the message or
    // the controls after it; those give way, the usage does not.
    let window = px(1200.) - bar.size.width + natural + px(40.);
    cx.simulate_resize(size(window, px(400.)));
    draw(cx);
    let (bar, _) = whole(cx);
    assert!(bar.size.width < natural + px(60.));
}
