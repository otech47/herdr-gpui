//! A sidebar fixture that stays open for looking at natively: two devices
//! sharing a space, the tabs list in one of its views. `--sidebar-test`
//! with `HERDR_TEST_SIDEBAR_HOLD` set to `all`, `by-device` or `device`,
//! and `HERDR_TEST_SIDEBAR_SORT` set to `attention` for the sorted list.

use super::*;
use crate::{
    config::{Density, LayoutMode, Style},
    endpoint::Endpoint,
    preferences::{TabsSort, TabsView},
};

pub(super) fn held_view() -> Option<TabsView> {
    Some(
        match std::env::var("HERDR_TEST_SIDEBAR_HOLD").ok()?.as_str() {
            "all" => TabsView::All,
            "by-device" => TabsView::ByDevice,
            _ => TabsView::Device,
        },
    )
}

fn tabs(
    workspace: &str,
    prefix: &str,
    titles: &[(&str, Option<&str>, &str)],
) -> Result<(Vec<ClientShellTab>, Vec<ClientShellAgent>)> {
    let mut tabs = Vec::new();
    let mut agents = Vec::new();
    for (index, (title, agent, status)) in titles.iter().enumerate() {
        let tab_id = format!("{prefix}{index}");
        tabs.push(serde_json::from_value(serde_json::json!({
            "tab_id": tab_id, "workspace_id": workspace, "number": index + 1,
            "label": title, "custom_label": true, "zoomed": false,
            "focused": index == 0, "agent_status": status
        }))?);
        if let Some(agent) = agent {
            agents.push(serde_json::from_value(serde_json::json!({
                "pane_id": format!("{prefix}p{index}"), "workspace_id": workspace,
                "tab_id": tab_id, "name": title, "display_agent": agent, "agent": agent,
                "agent_status": status, "state_change_seq": 0, "state_labels": [],
                "tokens": [], "focused": false
            }))?);
        }
    }
    Ok((tabs, agents))
}

fn populate(this: &mut HerdrWindow, view: TabsView) -> Result<()> {
    this.config.layout.mode = LayoutMode::new(Density::Comfortable, Style::Rounded);
    this.config.show_sidebar_tabs = true;
    this.config.show_tab_strip = false;
    this.config.show_agents = false;
    this.sidebar_width = Some(314.);
    this.sidebar_tabs_view = view;
    if std::env::var("HERDR_TEST_SIDEBAR_SORT").is_ok_and(|sort| sort == "attention") {
        this.sidebar_tabs_sort = TabsSort::Attention;
    }

    let mut local = sidebar::layout_tests::snapshot(2);
    local.workspaces[0].label = "fedi".into();
    local.workspaces[1].label = "tools".into();
    local.focused_workspace_id = Some("w0".into());
    (local.tabs, local.agents) = tabs(
        "w0",
        "t",
        &[
            ("deployment-safety-net", Some("claude"), "working"),
            ("action-items", Some("claude"), "idle"),
            ("pr 12382 review", Some("codex"), "working"),
            ("manifold mg seats", Some("claude"), "blocked"),
            ("fedi", None, "idle"),
            ("nix develop", None, "idle"),
            ("cargo nextest fedimint-server", Some("claude"), "working"),
            ("worktree cleanup", Some("kimi"), "done"),
        ],
    )?;
    this.live.status = ConnectionStatus::Connected;
    this.live.snapshot = Some(Arc::new(local));
    this.endpoints[0].live = this.live.clone();

    let mut remote = Endpoint::new(
        "ssh:mini".into(),
        "Mini".into(),
        ConnectTarget::Ssh {
            target: "unused".into(),
            session: "default".into(),
        },
        true,
    );
    let mut snapshot = sidebar::layout_tests::snapshot(1);
    snapshot.workspaces[0].label = "fedi".into();
    (snapshot.tabs, snapshot.agents) = tabs(
        "w0",
        "r",
        &[
            ("overnight operator run", Some("claude"), "working"),
            ("e2e tests", None, "idle"),
            ("mini build", Some("codex"), "idle"),
        ],
    )?;
    remote.live.status = ConnectionStatus::Connected;
    remote.live.snapshot = Some(Arc::new(snapshot));
    this.endpoints.push(remote);
    Ok(())
}

pub(super) fn hold_sidebar(handle: WindowHandle<HerdrWindow>, cx: &mut App, view: TabsView) {
    let result = handle.update(cx, |this, window, cx| {
        populate(this, view)?;
        window.resize(size(px(1000.), px(700.)));
        cx.notify();
        Ok::<(), anyhow::Error>(())
    });
    match result {
        Ok(Ok(())) => eprintln!("SIDEBAR holding {view:?}"),
        other => {
            eprintln!("SIDEBAR hold FAIL: {other:?}");
            std::process::exit(1);
        }
    }
}
