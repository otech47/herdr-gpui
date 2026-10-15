use super::*;
use crate::{NavigationTarget, control::Target, endpoint::Endpoint};
use herdr_client::ConnectTarget;

/// A window showing the local "herdr" space, with a saved device whose
/// second space is also called "herdr".
fn windows_with_a_namesake(
    cx: &mut gpui::TestAppContext,
) -> (gpui::Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        let mut local_snapshot = snapshot(3);
        local_snapshot.focused_workspace_id = Some("w0".into());
        view.live.snapshot = Some(Arc::new(local_snapshot));
        let mut remote = Endpoint::new(
            "ssh:test".into(),
            "Remote".into(),
            ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        );
        let mut remote_snapshot = snapshot(3);
        remote_snapshot.workspaces[0].label = "elsewhere".into();
        remote_snapshot.workspaces[1].label = "herdr".into();
        remote_snapshot.tabs[1].workspace_id = "w1".into();
        remote.live.snapshot = Some(Arc::new(remote_snapshot));
        view.endpoints.push(remote);
        view
    })
}

#[gpui::test]
fn a_request_for_a_namesake_space_on_another_device_switches_this_window(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = windows_with_a_namesake(cx);
    let target = Target {
        daemon: None,
        workspace: Some("w1"),
        pane: None,
        device: Some("ssh:test"),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.raise_if_showing(&target, Some("t1"), true, window, cx));
            assert!(view.raise_if_showing_namesake(&target, Some("t1"), true, window, cx));
            assert_eq!(view.selected_endpoint, 1);
            assert_eq!(
                view.pending_navigation,
                Some(NavigationTarget::Tab("t1".into()))
            );
        });
    });
}

#[gpui::test]
fn a_request_for_a_space_named_differently_leaves_this_window_alone(cx: &mut gpui::TestAppContext) {
    let (view, cx) = windows_with_a_namesake(cx);
    let target = Target {
        daemon: None,
        workspace: Some("w0"),
        pane: None,
        device: Some("ssh:test"),
    };
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            assert!(!view.raise_if_showing_namesake(&target, None, true, window, cx));
            assert_eq!(view.selected_endpoint, 0);
            assert!(view.pending_navigation.is_none());
            let unknown = Target {
                device: Some("ssh:other"),
                ..target
            };
            assert!(!view.raise_if_showing_namesake(&unknown, None, true, window, cx));
            assert_eq!(view.selected_endpoint, 0);
        });
    });
}
