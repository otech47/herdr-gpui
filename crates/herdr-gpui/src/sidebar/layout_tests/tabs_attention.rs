//! The attention sort's places: a tab that changes group takes the top of
//! its new group, the focused row holds until focus leaves it, a tab whose
//! agent left a shell running sorts above the idle ones, and the next and
//! previous tab commands step through the list as drawn.

use super::tabs_views::two_devices;
use super::*;
use crate::{
    monitoring::{Tab, Watched, Watchers},
    preferences::{TabsSort, TabsView},
    usage::Host,
};
use herdr_client::protocol::AgentStatus;

/// One device whose shown space holds five tabs: t0 working, t1 and t2 done,
/// t3 and t4 idle, with t0 focused and the attention sort settled once.
pub(super) fn five_tabs(
    cx: &mut gpui::TestAppContext,
) -> (Entity<HerdrWindow>, &mut gpui::VisualTestContext) {
    let (view, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        view.sidebar_tabs_sort = TabsSort::Attention;
        let mut local = snapshot(1);
        local.focused_workspace_id = Some("w0".into());
        local.focused_tab_id = Some("t0".into());
        let template = local.tabs[0].clone();
        local.tabs = [
            AgentStatus::Working,
            AgentStatus::Done,
            AgentStatus::Done,
            AgentStatus::Idle,
            AgentStatus::Idle,
        ]
        .into_iter()
        .enumerate()
        .map(|(i, status)| {
            let mut tab = template.clone();
            tab.tab_id = format!("t{i}");
            tab.label = format!("tab {}", i + 1);
            tab.custom_label = true;
            tab.focused = i == 0;
            tab.agent_status = status;
            tab
        })
        .collect();
        local.agents.clear();
        view.live.snapshot = Some(Arc::new(local));
        view.settle_attention();
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    (view, cx)
}

pub(super) fn listed(view: &HerdrWindow) -> Vec<String> {
    view.listed_tabs()
        .into_iter()
        .map(|row| row.tab.tab_id.clone())
        .collect()
}

/// Give a tab a status and focus, as the daemon's next snapshot would, then
/// settle the frame.
fn update(view: &mut HerdrWindow, tab: &str, status: AgentStatus, focus: Option<&str>) {
    let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
    snapshot
        .tabs
        .iter_mut()
        .find(|t| t.tab_id == tab)
        .unwrap()
        .agent_status = status;
    if let Some(focus) = focus {
        snapshot.focused_tab_id = Some(focus.into());
        for t in &mut snapshot.tabs {
            t.focused = t.tab_id == focus;
        }
    }
    view.settle_attention();
}

#[gpui::test]
fn a_tab_that_changes_group_takes_the_top_of_its_new_group(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4"]);
    });
    view.update(cx, |view, _| update(view, "t2", AgentStatus::Idle, None));
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4"]);
    });
    view.update(cx, |view, _| update(view, "t1", AgentStatus::Idle, None));
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4"]);
    });
    view.update(cx, |view, _| update(view, "t4", AgentStatus::Done, None));
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t4", "t1", "t2", "t3"]);
    });
    view.update(cx, |view, _| update(view, "t4", AgentStatus::Idle, None));
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t4", "t1", "t2", "t3"]);
    });
}

#[gpui::test]
fn the_focused_row_holds_until_focus_leaves_it(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        update(view, "t2", AgentStatus::Idle, Some("t2"))
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4"]);
        assert_eq!(view.listed_tab_step(true).unwrap().1, "t3");
        assert_eq!(view.listed_tab_step(false).unwrap().1, "t1");
    });
    view.update(cx, |view, _| {
        update(view, "t4", AgentStatus::Idle, Some("t4"))
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4"]);
    });
    view.update(cx, |view, _| update(view, "t4", AgentStatus::Blocked, None));
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t3", "t4"]);
    });
    view.update(cx, |view, _| {
        update(view, "t0", AgentStatus::Working, Some("t0"))
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t4", "t0", "t1", "t2", "t3"]);
    });
}

#[gpui::test]
fn the_next_and_previous_commands_step_through_the_list_as_drawn(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        update(view, "t3", AgentStatus::Blocked, None);
        update(view, "t4", AgentStatus::Done, None);
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t3", "t0", "t4", "t1", "t2"]);
        assert_eq!(view.listed_tab_step(true).unwrap().1, "t4");
        assert_eq!(view.listed_tab_step(false).unwrap().1, "t3");
    });
    view.update(cx, |view, _| {
        update(view, "t2", AgentStatus::Idle, Some("t2"))
    });
    view.read_with(cx, |view, _| {
        assert_eq!(view.listed_tab_step(true).unwrap().1, "t3");
        assert_eq!(view.listed_tab_step(false).unwrap().1, "t1");
    });
    view.update(cx, |view, _| view.sidebar_tabs_sort = TabsSort::Manual);
    view.read_with(cx, |view, _| {
        assert_eq!(view.listed_tab_step(true), None);
    });
}

#[gpui::test]
fn across_devices_the_step_crosses_into_the_other_device(cx: &mut gpui::TestAppContext) {
    let (view, cx) = two_devices(cx);
    view.update(cx, |view, _| {
        view.sidebar_tabs_view = TabsView::All;
        let local = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        local.focused_tab_id = Some("t1".into());
        view.settle_attention();
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "r0", "r1"]);
        assert_eq!(
            view.listed_tab_step(true),
            Some(("ssh:test".to_owned(), "r0".to_owned()))
        );
        assert_eq!(
            view.listed_tab_step(false),
            Some(("local".to_owned(), "t0".to_owned()))
        );
    });
}

#[gpui::test]
fn a_tab_the_daemon_drops_leaves_the_ledger(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        update(view, "t4", AgentStatus::Done, None);
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.tabs.retain(|t| t.tab_id != "t4");
        view.settle_attention();
        assert_eq!(view.attention.place("local", "t4", 9).since, 0);
        assert_eq!(view.attention.place("local", "t4", 9).rank, 9);
    });
}

#[gpui::test]
fn a_tab_whose_agent_left_a_shell_running_sorts_above_the_idle_ones(cx: &mut gpui::TestAppContext) {
    let (view, cx) = five_tabs(cx);
    view.update(cx, |view, _| {
        let mut agent = snapshot(1).agents[0].clone();
        agent.tab_id = "t4".into();
        Arc::make_mut(view.live.snapshot.as_mut().unwrap())
            .agents
            .push(agent);
        view.monitoring.seed(
            Host::Local,
            Watchers::from([(
                Tab {
                    id: "t4".into(),
                    socket: String::new(),
                },
                Watched {
                    under: vec!["claude".into()],
                    disowned: false,
                },
            )]),
        );
        view.settle_attention();
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t4", "t3"]);
    });
    view.update(cx, |view, _| {
        view.monitoring.seed(Host::Local, Watchers::new());
        view.settle_attention();
    });
    view.read_with(cx, |view, _| {
        assert_eq!(listed(view), ["t0", "t1", "t2", "t4", "t3"]);
        assert_eq!(view.attention.place("local", "t4", 0).rank, 5);
    });
}
