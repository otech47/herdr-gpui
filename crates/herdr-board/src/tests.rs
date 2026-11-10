#![allow(clippy::unwrap_used)]

use crate::{protocol::*, *};
use std::time::{Duration, SystemTime};

mod agents;
mod attention;
mod board;
mod hierarchy;
mod tabs;
mod usage;

const REPO_KEY: &str = "/fixture/agent-launcher/.git";

/// The sidebar's layout fixture: `workspace_count` spaces, two tabs in the
/// first and two agents.
fn snapshot(workspace_count: usize) -> ClientShellSnapshot {
    serde_json::from_value(serde_json::json!({
        "boot_id": "layout-test", "revision": 1,
        "update_install_command": "", "latest_release_notes_available": false,
        "integration_updates_available": false, "worktree_directory": "",
        "tab_bar_right": [], "tab_bar_right_separator": "", "agent_order": [],
        "tabs": (0..2).map(|i| serde_json::json!({
            "tab_id": format!("t{i}"), "workspace_id": "w0", "number": i + 1,
            "label": format!("tab {}", i + 1), "custom_label": false,
            "zoomed": false, "focused": i == 0, "agent_status": "working"
        })).collect::<Vec<_>>(),
        "panes": [], "commands": [],
        "workspaces": (0..workspace_count).map(|i| serde_json::json!({
            "workspace_id": format!("w{i}"), "active_tab_id": "t0", "new_workspace_cwd": "/tmp",
            "number": i + 1,
            "label": match i { 0 => "herdr", 1 => "herdr-gpui-sidebar-rendering-regression-investigation", 3..=5 => "agent-launcher", _ => "another workspace" },
            "custom_label": false,
            "branch": match i { 0 => "main", 2 => "1256789", 3 => "develop", 4 => "worktree/sidebar-child", 5 => "worktree/sidebar-child-with-a-long-readable-branch-name", _ => "fix/sidebar-label-width-and-overflow-regression" },
            "worktree": if (3..=5).contains(&i) { serde_json::json!({
                "key": REPO_KEY, "label": "agent-launcher", "is_linked_worktree": i != 3
            }) } else { serde_json::Value::Null },
            "tokens": [], "focused": i == 0, "agent_status": "working"
        })).collect::<Vec<_>>(),
        "agents": (["review", "Investigate sidebar rendering and verify long agent labels"].into_iter().enumerate().map(|(i, name)| serde_json::json!({
            "pane_id": format!("p{i}"), "workspace_id": if i == 0 { "w0" } else { "w1" },
            "tab_id": if i == 0 { "t0" } else { "none" },
            "name": name, "display_agent": if i == 0 { "Claude Code" } else { "agent" }, "agent": "claude",
            "agent_status": "working", "state_change_seq": 0, "state_labels": [],
            "tokens": [], "focused": false
        })).collect::<Vec<_>>())
    })).unwrap()
}

/// A device's snapshot: spaces by `(id, label)`, tabs by `(id, space, status)`
/// in daemon order, and agents by `(tab, kind, terminal title)`, each in the
/// pane `p-<tab>`. Every tab also has a shell pane `s-<tab>` in `/work/<tab>`.
fn device(
    spaces: &[(&str, &str)],
    tabs: &[(&str, &str, &str)],
    agents: &[(&str, &str, &str)],
) -> ClientShellSnapshot {
    let status = |tab: &str| {
        tabs.iter()
            .find(|(id, ..)| *id == tab)
            .map_or("unknown", |(.., status)| status)
    };
    serde_json::from_value(serde_json::json!({
        "boot_id": "board-test", "revision": 1,
        "update_install_command": "", "latest_release_notes_available": false,
        "integration_updates_available": false, "worktree_directory": "",
        "tab_bar_right": [], "tab_bar_right_separator": "", "agent_order": [],
        "commands": [],
        "focused_workspace_id": spaces.first().map(|(id, _)| *id),
        "workspaces": spaces.iter().enumerate().map(|(i, (id, label))| serde_json::json!({
            "workspace_id": id, "active_tab_id": "", "new_workspace_cwd": "/work",
            "number": i + 1, "label": label, "custom_label": false, "branch": null,
            "worktree": null, "tokens": [], "focused": i == 0, "agent_status": "idle"
        })).collect::<Vec<_>>(),
        "tabs": tabs.iter().enumerate().map(|(i, (id, space, status))| serde_json::json!({
            "tab_id": id, "workspace_id": space, "number": i + 1,
            "label": format!("tab {}", i + 1), "custom_label": false,
            "zoomed": false, "focused": false, "agent_status": status
        })).collect::<Vec<_>>(),
        "panes": tabs.iter().map(|(id, space, _)| serde_json::json!({
            "pane_id": format!("s-{id}"), "workspace_id": space, "tab_id": id,
            "cwd": format!("/work/{id}"), "focused": false, "right_click_passthrough": false
        })).collect::<Vec<_>>(),
        "agents": agents.iter().map(|(tab, kind, title)| serde_json::json!({
            "pane_id": format!("p-{tab}"), "workspace_id": "", "tab_id": tab,
            "agent": kind, "terminal_title_stripped": title,
            "agent_status": status(tab), "state_change_seq": 0, "state_labels": [],
            "tokens": [], "focused": false
        })).collect::<Vec<_>>()
    }))
    .unwrap()
}

fn source<'a>(id: &'a str, label: &'a str, snapshot: &'a ClientShellSnapshot) -> Source<'a> {
    Source {
        id,
        label,
        enabled: true,
        visible: true,
        connected: true,
        snapshot: Some(snapshot),
    }
}

fn daemon_mark(_: usize, _: Option<&ClientShellAgent>, status: AgentStatus) -> Mark {
    Mark::Status(status)
}

fn ids<'a>(rows: &'a [ListedTab<'_>]) -> Vec<(usize, &'a str)> {
    rows.iter()
        .map(|row| (row.endpoint, row.tab.tab_id.as_str()))
        .collect()
}

fn at(seconds: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(seconds)
}
