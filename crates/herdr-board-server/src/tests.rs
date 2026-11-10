#![allow(clippy::unwrap_used)]

use crate::{
    args::{Args, Endpoint},
    hub::Hub,
};
use herdr_client::{ConnectTarget, protocol::ClientShellSnapshot};
use std::sync::Arc;

mod args;
mod http;
mod hub;

fn endpoint(id: &str) -> Endpoint {
    Endpoint {
        id: id.to_owned(),
        label: id.to_uppercase(),
        enabled: true,
        target: ConnectTarget::Local,
    }
}

/// One space holding one tab in the given state.
fn snapshot(status: &str) -> Arc<ClientShellSnapshot> {
    let snapshot = serde_json::from_value(serde_json::json!({
        "boot_id": "server-test", "revision": 1,
        "update_install_command": "", "latest_release_notes_available": false,
        "integration_updates_available": false, "worktree_directory": "",
        "tab_bar_right": [], "tab_bar_right_separator": "", "agent_order": [],
        "commands": [], "panes": [], "agents": [],
        "workspaces": [{
            "workspace_id": "w0", "active_tab_id": "t0", "new_workspace_cwd": "/work",
            "number": 1, "label": "space", "custom_label": false, "branch": null,
            "worktree": null, "tokens": [], "focused": true, "agent_status": status
        }],
        "tabs": [{
            "tab_id": "t0", "workspace_id": "w0", "number": 1, "label": "tab",
            "custom_label": false, "zoomed": false, "focused": true, "agent_status": status
        }]
    }))
    .unwrap();
    Arc::new(snapshot)
}

fn hub(devices: &[&str]) -> Hub {
    Hub::starting_at(devices.iter().map(|id| endpoint(id)).collect(), 7).unwrap()
}

fn connect(hub: &Hub, device: usize, status: &str) {
    hub.update(device, |link| {
        link.connected = true;
        link.snapshot = Some(snapshot(status));
    })
    .unwrap();
}

fn board(json: &str) -> herdr_board::Board {
    serde_json::from_str(json).unwrap()
}
