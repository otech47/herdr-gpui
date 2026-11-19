#![allow(clippy::unwrap_used)]

use super::command_line;
use crate::{
    HerdrWindow,
    config::{Config, NewTab, NewTabAgent, NewTabSpace},
    controls::Command,
    launcher::LaunchPlan,
    menu::Page,
    sidebar::layout_tests::fixture_window,
};
use gpui::{Keystroke, TestAppContext, VisualTestContext};
use herdr_client::ConnectTarget;

const MINI: &str = "ssh:0123456789abcdef0123456789abcdef";

fn space(label: &str, dir: &str, wrap: &str) -> NewTabSpace {
    NewTabSpace {
        label: label.into(),
        dir: dir.into(),
        wrap: wrap.into(),
    }
}

fn agent(key: char, label: &str, command: &str) -> NewTabAgent {
    NewTabAgent {
        key,
        label: label.into(),
        command: command.into(),
    }
}

fn new_tab() -> NewTab {
    NewTab {
        command: "~/.local/bin/herdr-launch".into(),
        spaces: vec![
            space("tools", "~/tools", ""),
            space("fedi", "~/fedibtc/fedi", "nix develop -c"),
        ],
        agents: vec![
            agent('q', "Shell", ""),
            agent('w', "Claude", "claude"),
            agent('e', "Codex", "codex"),
            agent('r', "Kimi", "kimi"),
        ],
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

/// A window on the fixture's space "herdr", with the Mini saved beside it.
fn window(cx: &mut TestAppContext) -> (gpui::Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    view.update(cx, |view, _| {
        view.endpoints.push(mini());
        view.config = Config {
            new_tab: Some(new_tab()),
            ..view.config.clone()
        };
        let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
        snapshot.focused_workspace_id = Some("w0".into());
        view.live.snapshot = Some(std::sync::Arc::new(snapshot));
    });
    (view, cx)
}

fn open(view: &gpui::Entity<HerdrWindow>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| view.command(Command::Tab, window, cx));
    });
}

fn press(view: &gpui::Entity<HerdrWindow>, cx: &mut VisualTestContext, key: &str) {
    let keystroke = Keystroke::parse(key).unwrap();
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let event = gpui::KeyDownEvent {
                keystroke,
                is_held: false,
                prefer_character_input: false,
            };
            view.new_tab_key(&event, window, cx);
        });
    });
}

#[test]
fn the_command_line_wraps_an_agent_and_leaves_a_shell_bare() {
    let tab = new_tab();
    let fedi = &tab.spaces[1];
    assert_eq!(
        command_line(&tab.command, fedi, &tab.agents[2]),
        "~/.local/bin/herdr-launch 'fedi' '~/fedibtc/fedi' 'nix develop -c codex'"
    );
    assert_eq!(
        command_line(&tab.command, &tab.spaces[0], &tab.agents[0]),
        "~/.local/bin/herdr-launch 'tools' '~/tools'"
    );
    assert_eq!(
        command_line(&tab.command, &space("it's", "~", ""), &tab.agents[1]),
        r"~/.local/bin/herdr-launch 'it'\''s' '~' 'claude'"
    );
}

#[gpui::test]
fn new_tab_opens_on_this_device_this_space_and_the_first_agent(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, Some(Page::NewTab));
        let prompt = view.menu.new_tab.as_ref().unwrap();
        let labels: Vec<_> = prompt.spaces.iter().map(|s| s.label.as_str()).collect();
        // The window's own space joins the configured ones.
        assert_eq!(labels, ["tools", "fedi", "herdr"]);
        assert_eq!(prompt.space, 2);
        assert_eq!(prompt.device, 0);
        assert_eq!(prompt.agent, 1);
        assert_eq!(prompt.spaces[2].dir, "/tmp");
    });
}

#[gpui::test]
fn keys_pick_each_row_and_enter_runs_the_script(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    for key in ["e", "2", "right", "right", "1", "2"] {
        press(&view, cx, key);
    }
    view.update(cx, |view, _| {
        let prompt = view.menu.new_tab.as_ref().unwrap();
        assert_eq!((prompt.device, prompt.agent, prompt.space), (1, 2, 1));
    });
    press(&view, cx, "enter");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert_eq!(view.launched.len(), 1);
        let plan = &view.launched[0];
        assert_eq!(
            plan.command,
            "~/.local/bin/herdr-launch 'fedi' '~/fedibtc/fedi' 'nix develop -c codex'"
        );
        assert_eq!(env(plan, "HERDR_LAUNCH_DEVICE").as_deref(), Some(MINI));
        assert_eq!(env(plan, "HERDR_LAUNCH_HOST").as_deref(), Some("oz-mini"));
    });
    // The space remembers its agent for the next prompt.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let snapshot = view.live.snapshot.as_ref().unwrap();
            let mut snapshot = (**snapshot).clone();
            snapshot.workspaces[0].label = "fedi".into();
            view.live.snapshot = Some(std::sync::Arc::new(snapshot));
            view.command(Command::Tab, window, cx);
        });
    });
    view.update(cx, |view, _| {
        let prompt = view.menu.new_tab.as_ref().unwrap();
        assert_eq!((prompt.space, prompt.agent), (1, 2));
    });
}

#[gpui::test]
fn a_shell_here_is_an_ordinary_tab_and_escape_cancels(cx: &mut TestAppContext) {
    let (view, cx) = window(cx);
    open(&view, cx);
    press(&view, cx, "q");
    press(&view, cx, "enter");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert!(view.launched.is_empty());
    });
    open(&view, cx);
    press(&view, cx, "escape");
    view.update(cx, |view, _| {
        assert_eq!(view.menu.page, None);
        assert!(view.launched.is_empty());
    });
}
