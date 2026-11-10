use super::*;

#[test]
fn agent_rows_name_their_place_then_their_agent() {
    let mut snapshot = snapshot(1);
    let agent = &mut snapshot.agents[0];
    agent.workspace_id = "w0".into();
    agent.tab_id = "t0".into();
    agent.display_agent = Some("Claude Code".into());
    agent.name = Some("review".into());
    agent.agent = Some("claude".into());
    agent.title = Some("Fix sidebar".into());
    let agent = snapshot.agents[0].clone();
    // Host first when there is one, then the workspace, then the tab. Only
    // the workspace is primary; upstream mutes what sits around it.
    fn labels<'a>(
        snapshot: &'a ClientShellSnapshot,
        host: Option<&'a str>,
    ) -> (Vec<(&'a str, bool)>, &'a str) {
        let agent = &snapshot.agents[0];
        agent_labels(agent_name(agent), agent_place(agent, snapshot), host)
    }
    assert_eq!(
        labels(&snapshot, None),
        (vec![("herdr", true), ("tab 1", false)], "Claude Code")
    );
    assert_eq!(
        labels(&snapshot, Some("remote")),
        (
            vec![("remote", false), ("herdr", true), ("tab 1", false)],
            "Claude Code"
        )
    );
    // One unnamed tab is noise, so only its workspace shows.
    snapshot.tabs.retain(|tab| tab.tab_id == "t0");
    assert_eq!(labels(&snapshot, None).0, vec![("herdr", true)]);
    snapshot.tabs[0].custom_label = true;
    assert_eq!(
        labels(&snapshot, None).0,
        vec![("herdr", true), ("tab 1", false)]
    );
    // The agent name falls back through the same order as upstream.
    for (display, name, kind, title, expected) in [
        (None, Some("review"), Some("claude"), None, "review"),
        (None, None, Some("claude"), Some("Fix sidebar"), "claude"),
        (None, None, None, Some("Fix sidebar"), "Fix sidebar"),
        (None, None, None, None, "agent"),
    ] {
        snapshot.agents[0] = ClientShellAgent {
            display_agent: display.map(str::to_owned),
            name: name.map(str::to_owned),
            agent: kind.map(str::to_owned),
            title: title.map(str::to_owned),
            ..agent.clone()
        };
        assert_eq!(labels(&snapshot, None).1, expected);
    }
    // Without its workspace the agent names the row itself.
    snapshot.workspaces.clear();
    assert_eq!(labels(&snapshot, None), (vec![("agent", true)], ""));
}

#[test]
fn text_fallback_skips_missing_and_blank_metadata() {
    assert_eq!(first_text([None, Some(" \t"), Some(" main ")], ""), "main");
    assert_eq!(first_text([None, Some("")], ""), "");
    assert_eq!(first_text([Some(" ")], "workspace"), "workspace");
}

/// An integration's label names the agent's current status; a label for
/// another status, a blank one, or none leaves the fallback. Labels are
/// untrusted: controls and bidi overrides go, and length is bounded.
#[test]
fn state_labels_name_the_current_status_as_safe_bounded_text() {
    let agent = |status, labels: &[(&str, &str)]| ClientShellAgent {
        pane_id: "p".into(),
        workspace_id: "w".into(),
        tab_id: "t".into(),
        name: None,
        display_agent: None,
        agent: None,
        title: None,
        terminal_title: None,
        terminal_title_stripped: None,
        agent_status: status,
        state_change_seq: 0,
        state_labels: labels
            .iter()
            .map(|(state, label)| ((*state).into(), (*label).into()))
            .collect(),
        tokens: Vec::new(),
        focused: false,
    };
    let labels = [("working", "deep in the mines"), ("blocked", "  ")];
    for (status, fallback, expected) in [
        (AgentStatus::Working, "working", "deep in the mines"),
        (AgentStatus::Blocked, "blocked", "blocked"),
        (AgentStatus::Done, "done", "done"),
        (AgentStatus::Unknown, "", ""),
    ] {
        assert_eq!(state_label(&agent(status, &labels), fallback), expected);
    }
    let hostile = agent(
        AgentStatus::Working,
        &[("working", "\u{1b}[31mred\u{202e}dlrow\n")],
    );
    assert_eq!(state_label(&hostile, "working"), "[31mreddlrow");
    let long = "x".repeat(10_000);
    let long = agent(AgentStatus::Idle, &[("idle", &long)]);
    assert_eq!(state_label(&long, "idle").chars().count(), 80);
}

fn ordered(snapshot: &ClientShellSnapshot, sort: AgentSort) -> Vec<&str> {
    sorted_agents(snapshot, sort)
        .into_iter()
        .map(|agent| agent.pane_id.as_str())
        .collect()
}

#[test]
fn a_plugin_view_order_replaces_the_local_sort_and_filters() {
    let mut snapshot = snapshot(2);
    snapshot.agents[1].agent_status = AgentStatus::Blocked;
    // Without a view the order is local, even when the daemon sent one.
    snapshot.agent_order = vec!["p1".into()];
    assert_eq!(ordered(&snapshot, AgentSort::Grouped), ["p0", "p1"]);
    assert_eq!(ordered(&snapshot, AgentSort::Priority), ["p1", "p0"]);

    snapshot.agent_view_label = Some("review".into());
    snapshot.agent_order = vec!["p1".into(), "gone".into(), "p0".into()];
    for sort in [AgentSort::Grouped, AgentSort::Priority] {
        // A pane the snapshot no longer lists as an agent is skipped.
        assert_eq!(ordered(&snapshot, sort), ["p1", "p0"]);
    }
    snapshot.agents[1].agent_status = AgentStatus::Idle;
    snapshot.agent_order = vec!["p0".into()];
    assert_eq!(ordered(&snapshot, AgentSort::Priority), ["p0"]);
    snapshot.agent_order.clear();
    assert!(ordered(&snapshot, AgentSort::Grouped).is_empty());
}
