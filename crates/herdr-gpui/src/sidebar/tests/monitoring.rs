use super::*;
use crate::{
    monitoring::{Tab, Watched, Watchers},
    usage::Host,
};

fn watchers(entries: &[(&str, &str)]) -> Watchers {
    entries
        .iter()
        .map(|&(tab, under)| {
            (
                Tab {
                    id: tab.into(),
                    socket: String::new(),
                },
                Watched {
                    under: vec![under.into()],
                    disowned: false,
                },
            )
        })
        .collect()
}

#[gpui::test]
fn an_idle_agent_with_a_shell_running_under_it_is_monitoring(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = layout_tests::fixture_window(window, cx);
        view.monitoring
            .seed(Host::Local, watchers(&[("t0", "claude"), ("t1", "nix")]));
        view.endpoints.push(crate::endpoint::Endpoint::new(
            "ssh:test".into(),
            "Mini".into(),
            herdr_client::ConnectTarget::Ssh {
                target: "unused".into(),
                session: "default".into(),
            },
            true,
        ));
        view
    });
    let snapshot = layout_tests::snapshot(1);
    let claude = snapshot.agents[0].clone();
    assert_eq!(
        (claude.tab_id.as_str(), claude.agent.as_deref()),
        ("t0", Some("claude"))
    );
    let mut wrapped = claude.clone();
    wrapped.tab_id = "t1".into();
    let mut codex = claude.clone();
    codex.agent = Some("codex".into());
    let mut unknown = claude.clone();
    unknown.agent = None;
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.mark(0, Some(&claude), AgentStatus::Idle),
            Mark::Monitoring
        );
        for status in [
            AgentStatus::Working,
            AgentStatus::Done,
            AgentStatus::Blocked,
            AgentStatus::Unknown,
        ] {
            assert_eq!(view.mark(0, Some(&claude), status), Mark::Status(status));
        }
        for agent in [&wrapped, &codex, &unknown] {
            assert_eq!(
                view.mark(0, Some(agent), AgentStatus::Idle),
                Mark::Status(AgentStatus::Idle)
            );
        }
        assert_eq!(
            view.mark(0, None, AgentStatus::Idle),
            Mark::Status(AgentStatus::Idle)
        );
        // ssh:test was never scanned.
        assert_eq!(
            view.mark(1, Some(&claude), AgentStatus::Idle),
            Mark::Status(AgentStatus::Idle)
        );
    });
}

#[gpui::test]
fn a_used_up_plan_outranks_a_shell_left_running(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = layout_tests::fixture_window(window, cx);
        view.usage.seed(Host::Local, &[("claude", 100.)]);
        view.monitoring
            .seed(Host::Local, watchers(&[("t0", "claude")]));
        view
    });
    let claude = layout_tests::snapshot(1).agents[0].clone();
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.mark(0, Some(&claude), AgentStatus::Idle),
            Mark::Limited
        );
    });
}

#[test]
fn a_monitoring_mark_takes_a_hollow_blue_from_every_palette() {
    assert_eq!(
        status_style(
            Mark::Monitoring,
            &Theme::builtin("Catppuccin Mocha").unwrap()
        )
        .2,
        0x89b4fa
    );
    for name in ["Default", "Nord", "Dracula", "Catppuccin Mocha"] {
        let mut theme = Theme::builtin(name).unwrap();
        let (diameter, filled, color) = status_style(Mark::Monitoring, &theme);
        assert_eq!(
            (diameter, filled, color),
            (STATUS_WIDTH, false, theme.ink(0x89b4fa)),
            "{name}"
        );
        theme.palette.fill(0x123456);
        assert_eq!(
            Indicators::new(None, false, &theme).color(Mark::Monitoring),
            color
        );
        for mark in [
            Mark::Status(AgentStatus::Working),
            Mark::Status(AgentStatus::Blocked),
            Mark::Status(AgentStatus::Done),
            Mark::Status(AgentStatus::Idle),
            Mark::Status(AgentStatus::Unknown),
            Mark::Limited,
        ] {
            assert_ne!(status_style(mark, &theme).2, color, "{name} {mark:?}");
        }
    }
    assert_eq!(status_symbol(Mark::Monitoring), "\u{25ce}");
    assert_eq!(status_text(Mark::Monitoring), "monitoring");
    let settings = Settings::parse_text("[theme]\nname = 'catppuccin'\n").unwrap();
    assert_eq!(settings.mark_color(Mark::Monitoring, false), 0x89b4fa);
    let settings = Settings::parse_text("[theme.custom]\nblue = '#345678'\n").unwrap();
    assert_eq!(settings.mark_color(Mark::Monitoring, false), 0x345678);
}
