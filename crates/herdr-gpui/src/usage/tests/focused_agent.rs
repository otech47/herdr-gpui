use super::super::focus::focused_provider;
use herdr_client::protocol::ClientShellSnapshot;

fn snapshot() -> ClientShellSnapshot {
    serde_json::from_str(include_str!(
        "../../../../herdr-protocol/tests/fixtures/endpoint-snapshot-v1.json"
    ))
    .unwrap()
}

#[test]
fn the_bar_leads_with_the_focused_panes_agent_else_its_tabs() {
    let lead = |snapshot: &ClientShellSnapshot| focused_provider(snapshot).map(|p| p.id());
    let mut snapshot = snapshot();
    assert_eq!(lead(&snapshot), Some("claude"));
    // A split without an agent still belongs to the agent's tab.
    snapshot.focused_pane_id = Some("w1:p2".into());
    assert_eq!(lead(&snapshot), Some("claude"));
    snapshot.focused_tab_id = Some("w1:t2".into());
    assert_eq!(lead(&snapshot), None);
    // An agent without a usage provider leaves the bar to the closest limits.
    snapshot.focused_pane_id = Some("w1:p1".into());
    snapshot.agents[0].agent = Some("mastracode".into());
    assert_eq!(lead(&snapshot), None);
    snapshot.agents[0].agent = None;
    assert_eq!(lead(&snapshot), None);
}
