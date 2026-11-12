use super::*;
use crate::usage::Host;

#[gpui::test]
fn an_agent_is_limited_only_while_it_waits_on_a_used_up_plan(cx: &mut gpui::TestAppContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = layout_tests::fixture_window(window, cx);
        view.usage.seed(
            Host::Local,
            &[("claude", 100.), ("codex", 100.), ("kimi", 99.)],
        );
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
    assert_eq!(claude.agent.as_deref(), Some("claude"));
    let mut codex = claude.clone();
    codex.agent = Some("codex".into());
    let mut kimi = claude.clone();
    kimi.agent = Some("kimi".into());
    let mut unknown = claude.clone();
    unknown.agent = None;
    let waiting = [AgentStatus::Idle, AgentStatus::Done, AgentStatus::Unknown];
    let busy = [AgentStatus::Working, AgentStatus::Blocked];
    view.read_with(cx, |view, _| {
        for status in waiting {
            assert_eq!(view.mark(0, Some(&claude), status), Mark::Status(status));
            assert_eq!(view.mark(0, Some(&codex), status), Mark::Limited);
            assert_eq!(view.mark(0, Some(&kimi), status), Mark::Status(status));
            assert_eq!(view.mark(0, Some(&unknown), status), Mark::Status(status));
            assert_eq!(view.mark(0, None, status), Mark::Status(status));
        }
        for status in busy {
            assert_eq!(view.mark(0, Some(&codex), status), Mark::Status(status));
        }
    });
    view.update(cx, |view, _| {
        let tab = crate::monitoring::Tab {
            id: claude.tab_id.clone(),
            socket: String::new(),
        };
        let stopped = crate::monitoring::Watched {
            stopped: true,
            ..Default::default()
        };
        view.monitoring
            .seed(Host::Local, [(tab, stopped)].into_iter().collect());
    });
    view.read_with(cx, |view, _| {
        for status in waiting {
            assert_eq!(view.mark(0, Some(&claude), status), Mark::Limited);
            // ssh:test was never read.
            assert_eq!(view.mark(1, Some(&claude), status), Mark::Status(status));
        }
        for status in busy {
            assert_eq!(view.mark(0, Some(&claude), status), Mark::Status(status));
        }
    });
    view.update(cx, |view, _| {
        view.usage.seed(Host::Local, &[("claude", 40.)]);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(
            view.mark(0, Some(&claude), AgentStatus::Idle),
            Mark::Status(AgentStatus::Idle)
        );
    });
}

#[test]
fn a_limited_agent_icon_takes_the_limited_gray_over_its_own_hue() {
    use crate::sidebar::agents::icon_tint;
    let theme = Theme::builtin("Catppuccin Mocha").unwrap();
    let indicators = Indicators::new(None, false, &theme);
    let text = theme.foreground;
    for icon in crate::icons::AgentIcon::ALL.iter().copied() {
        assert_eq!(
            icon_tint(icon, text, Mark::Limited, indicators, &theme),
            indicators.color(Mark::Limited)
        );
        for status in [
            AgentStatus::Working,
            AgentStatus::Idle,
            AgentStatus::Blocked,
        ] {
            assert_eq!(
                icon_tint(icon, text, Mark::Status(status), indicators, &theme),
                icon.color(text, &theme)
            );
        }
    }
}
