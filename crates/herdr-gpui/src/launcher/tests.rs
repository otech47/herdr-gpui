#![allow(clippy::unwrap_used)]

use super::{Choice, LaunchPlan, api_socket, plan};
use crate::{
    HerdrWindow,
    config::{Config, Launcher},
    menu::Page,
    sidebar::layout_tests::fixture_window,
};
use gpui::{Keystroke, TestAppContext, VisualTestContext};
use herdr_client::ConnectTarget;
use std::path::Path;

const MINI: &str = "ssh:0123456789abcdef0123456789abcdef";

fn launcher(key: &str, ask_device: bool) -> Launcher {
    Launcher {
        key: Keystroke::parse(key).unwrap(),
        label: "claude in tools".into(),
        command: "herdr-launch tools ~/tools claude".into(),
        ask_device,
    }
}

fn mini() -> crate::endpoint::Endpoint {
    crate::endpoint::Endpoint::new(
        MINI.into(),
        "Mini".into(),
        ConnectTarget::Ssh {
            target: "oz-mini".into(),
            session: "default".into(),
        },
        true,
    )
}

fn env(plan: &LaunchPlan, name: &str) -> Option<String> {
    plan.env
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.clone())
}

fn press(view: &gpui::Entity<HerdrWindow>, cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            if view.menu.page == Some(Page::Launcher) {
                let event = gpui::KeyDownEvent {
                    keystroke: keystroke.clone(),
                    is_held: false,
                    prefer_character_input: false,
                };
                view.launcher_key(&event, window, cx);
            } else if let Some(index) = view.launcher_for(&keystroke) {
                view.open_launcher(index, window, cx);
            }
        });
    });
}

#[test]
fn the_plan_names_the_device_and_its_host() {
    let launcher = launcher("ctrl-cmd-2", true);
    let local = plan(
        &launcher,
        &Choice {
            id: "local".into(),
            label: "Local".into(),
            host: None,
        },
        Some("w8"),
        Some("/tmp/herdr.sock"),
    );
    assert_eq!(local.command, "herdr-launch tools ~/tools claude");
    assert_eq!(env(&local, "HERDR_LAUNCH_DEVICE").as_deref(), Some("local"));
    assert_eq!(env(&local, "HERDR_LAUNCH_HOST"), None);
    assert_eq!(
        env(&local, "HERDR_ACTIVE_WORKSPACE_ID").as_deref(),
        Some("w8")
    );
    assert_eq!(
        env(&local, "HERDR_SOCKET_PATH").as_deref(),
        Some("/tmp/herdr.sock")
    );
    let remote = plan(
        &launcher,
        &Choice {
            id: MINI.into(),
            label: "Mini".into(),
            host: Some("oz-mini".into()),
        },
        None,
        None,
    );
    assert_eq!(env(&remote, "HERDR_LAUNCH_DEVICE").as_deref(), Some(MINI));
    assert_eq!(
        env(&remote, "HERDR_LAUNCH_DEVICE_LABEL").as_deref(),
        Some("Mini")
    );
    assert_eq!(
        env(&remote, "HERDR_LAUNCH_HOST").as_deref(),
        Some("oz-mini")
    );
    assert_eq!(env(&remote, "HERDR_ACTIVE_WORKSPACE_ID"), None);
}

#[test]
fn the_command_line_socket_sits_beside_the_client_socket() {
    assert_eq!(
        api_socket(Path::new("/home/me/.config/herdr/herdr-client.sock")).as_deref(),
        Some(Path::new("/home/me/.config/herdr/herdr.sock"))
    );
    assert_eq!(
        api_socket(Path::new(
            "/home/me/.config/herdr/sessions/work/herdr-client.sock"
        ))
        .as_deref(),
        Some(Path::new("/home/me/.config/herdr/sessions/work/herdr.sock"))
    );
    assert_eq!(api_socket(Path::new("/tmp/custom.sock")), None);
}

#[gpui::test]
fn without_a_saved_device_the_chord_runs_locally_at_once(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.config = Config {
            launchers: vec![launcher("ctrl-cmd-2", true)],
            ..view.config.clone()
        };
        let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
        snapshot.focused_workspace_id = Some("w0".into());
        view.live.snapshot = Some(std::sync::Arc::new(snapshot));
    });
    press(&view, cx, "ctrl-cmd-2");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.launched.len(), 1);
        assert_eq!(
            env(&view.launched[0], "HERDR_LAUNCH_DEVICE").as_deref(),
            Some("local")
        );
        assert_eq!(
            env(&view.launched[0], "HERDR_ACTIVE_WORKSPACE_ID").as_deref(),
            Some("w0")
        );
        // The fixture's device is not a socket, so the script finds the
        // daemon on its own.
        assert_eq!(env(&view.launched[0], "HERDR_SOCKET_PATH"), None);
    });
}

#[gpui::test]
fn with_a_saved_device_the_chord_asks_and_the_pick_runs(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(mini());
        view.config = Config {
            launchers: vec![launcher("ctrl-cmd-2", true)],
            ..view.config.clone()
        };
    });
    press(&view, cx, "ctrl-cmd-2");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::Launcher));
        assert_eq!(view.menu.launcher.as_ref().unwrap().selected, 0);
        assert!(view.launched.is_empty());
    });
    press(&view, cx, "down");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.launcher.as_ref().unwrap().selected, 1);
    });
    press(&view, cx, "enter");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.launched.len(), 1);
        assert_eq!(
            env(&view.launched[0], "HERDR_LAUNCH_DEVICE").as_deref(),
            Some(MINI)
        );
        assert_eq!(
            env(&view.launched[0], "HERDR_LAUNCH_HOST").as_deref(),
            Some("oz-mini")
        );
        assert_eq!(env(&view.launched[0], "HERDR_SOCKET_PATH"), None);
    });
    press(&view, cx, "ctrl-cmd-2");
    press(&view, cx, "1");
    view.update(cx, |view, _| {
        assert_eq!(view.launched.len(), 2);
        assert_eq!(
            env(&view.launched[1], "HERDR_LAUNCH_DEVICE").as_deref(),
            Some("local")
        );
    });
    press(&view, cx, "ctrl-cmd-2");
    press(&view, cx, "escape");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.launched.len(), 2);
    });
}

#[gpui::test]
fn an_entry_that_does_not_ask_skips_the_prompt(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(mini());
        view.config = Config {
            launchers: vec![launcher("ctrl-cmd-2", false)],
            ..view.config.clone()
        };
    });
    press(&view, cx, "ctrl-cmd-2");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.launched.len(), 1);
        assert_eq!(
            env(&view.launched[0], "HERDR_LAUNCH_DEVICE").as_deref(),
            Some("local")
        );
    });
}

#[gpui::test]
fn only_a_configured_chord_is_a_launcher(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(mini());
        view.config = Config {
            launchers: vec![launcher("ctrl-cmd-2", true)],
            ..view.config.clone()
        };
        assert_eq!(view.launcher_for(&Keystroke::parse("cmd-t").unwrap()), None);
        assert_eq!(
            view.launcher_for(&Keystroke::parse("ctrl-cmd-3").unwrap()),
            None
        );
        assert_eq!(
            view.launcher_for(&Keystroke::parse("ctrl-cmd-2").unwrap()),
            Some(0)
        );
    });
}
