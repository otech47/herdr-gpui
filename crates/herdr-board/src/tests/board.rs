use super::*;
use crate::usage::{Kind, Window};

fn input<'a>(id: &'a str, label: &'a str, snapshot: &'a ClientShellSnapshot) -> DeviceInput<'a> {
    DeviceInput {
        source: source(id, label, snapshot),
        connecting: false,
        detail: None,
        route: &[],
    }
}

fn local() -> ClientShellSnapshot {
    let mut local = device(
        &[("w1", "tools"), ("w2", "fedi")],
        &[
            ("t1", "w1", "idle"),
            ("t2", "w1", "working"),
            ("t3", "w2", "blocked"),
            ("t4", "w2", "done"),
        ],
        &[
            ("t2", "claude", "fix the build"),
            ("t3", "codex", "review the patch"),
        ],
    );
    local.tabs[0].focused = true;
    local.agents[1].state_labels = vec![("blocked".into(), "needs approval".into())];
    local
}

fn mini() -> ClientShellSnapshot {
    device(
        &[("m1", "worksteps")],
        &[("t1", "m1", "blocked"), ("t5", "m1", "unknown")],
        &[("t1", "claude", "ship it")],
    )
}

fn built(devices: &[DeviceInput<'_>]) -> Board {
    let sources: Vec<Source<'_>> = devices.iter().map(|device| device.source).collect();
    let mut ledger = AttentionLedger::default();
    settle(&sources, &mut ledger, daemon_mark, |_, _| false);
    build(devices, &ledger, daemon_mark, &[])
}

fn refs(board: &Board) -> Vec<(&str, &str)> {
    board
        .attention
        .iter()
        .map(|row| (row.device_id.as_str(), row.tab_id.as_str()))
        .collect()
}

#[test]
fn rows_keep_daemon_order_in_their_space_and_carry_what_opens_them() {
    let (local, mini) = (local(), mini());
    let board = built(&[
        input("local", "Local", &local),
        input("ssh:mini", "Mini", &mini),
    ]);
    assert_eq!(board.version, Board::VERSION);
    let spaces: Vec<(&str, Vec<&str>)> = board.devices[0]
        .spaces
        .iter()
        .map(|space| {
            (
                space.label.as_str(),
                space.tabs.iter().map(|tab| tab.id.as_str()).collect(),
            )
        })
        .collect();
    assert_eq!(
        spaces,
        [("tools", vec!["t1", "t2"]), ("fedi", vec!["t3", "t4"])]
    );

    let shell = &board.devices[0].spaces[0].tabs[0];
    assert_eq!(
        (shell.title.as_str(), shell.state.as_str(), shell.focused),
        ("t1", "idle", true)
    );
    assert_eq!(shell.agent, None);
    // A shell tab opens by its own pane, an agent's by the agent's pane.
    assert_eq!(shell.pane_id.as_deref(), Some("s-t1"));
    let agent = &board.devices[0].spaces[1].tabs[0];
    assert_eq!(agent.title, "review the patch");
    assert_eq!(agent.pane_id.as_deref(), Some("p-t3"));
    assert_eq!(agent.terminal_id, agent.pane_id);
    assert_eq!(
        (
            agent.device_id.as_str(),
            agent.space_id.as_str(),
            agent.number
        ),
        ("local", "w2", 3)
    );
    assert_eq!(
        agent.agent,
        Some(AgentLabel {
            id: Some("codex".into()),
            name: "codex".into()
        })
    );
    // The integration's own words stand in for the state's name.
    assert_eq!(
        (
            agent.state.as_str(),
            agent.state_text.as_str(),
            agent.symbol.as_str()
        ),
        ("blocked", "needs approval", "\u{d7}")
    );
    assert_eq!(board.devices[0].state, "blocked");
    assert_eq!(board.devices[0].status, DeviceStatus::Connected);
}

#[test]
fn attention_lists_every_row_across_devices_by_rank() {
    let (local, mini) = (local(), mini());
    let board = built(&[
        input("local", "Local", &local),
        input("ssh:mini", "Mini", &mini),
    ]);
    assert_eq!(
        refs(&board),
        [
            ("local", "t3"),
            ("ssh:mini", "t1"),
            ("local", "t2"),
            ("local", "t4"),
            ("local", "t1"),
            ("ssh:mini", "t5"),
        ]
    );
    let ranks: Vec<u8> = board.devices[0]
        .spaces
        .iter()
        .flat_map(|space| &space.tabs)
        .map(|tab| tab.attention_rank)
        .collect();
    assert_eq!(ranks, [5, 2, 0, 3]);
}

#[test]
fn a_mark_the_daemon_cannot_see_changes_the_state_and_the_order() {
    let local = local();
    let devices = [input("local", "Local", &local)];
    let mark = |_: usize, agent: Option<&ClientShellAgent>, status: AgentStatus| match agent
        .and_then(|agent| agent.agent.as_deref())
    {
        Some("claude") if status != AgentStatus::Blocked => Mark::Limited,
        _ if status == AgentStatus::Idle => Mark::Monitoring,
        _ => Mark::Status(status),
    };
    let sources = [devices[0].source];
    let mut ledger = AttentionLedger::default();
    settle(&sources, &mut ledger, mark, |_, _| false);
    let board = build(&devices, &ledger, mark, &[]);
    let states: Vec<(&str, &str)> = board.devices[0]
        .spaces
        .iter()
        .flat_map(|space| &space.tabs)
        .map(|tab| (tab.id.as_str(), tab.state.as_str()))
        .collect();
    assert_eq!(
        states,
        [
            ("t1", "monitoring"),
            ("t2", "limited"),
            ("t3", "blocked"),
            ("t4", "done")
        ]
    );
    assert_eq!(
        refs(&board),
        [
            ("local", "t3"),
            ("local", "t2"),
            ("local", "t4"),
            ("local", "t1")
        ]
    );
}

#[test]
fn waiting_rows_come_latest_first_and_tag_their_space_and_device() {
    let (mut local, mut mini) = (local(), mini());
    local.tabs[2].agent_status = AgentStatus::Working;
    mini.tabs[0].agent_status = AgentStatus::Working;
    let mut ledger = AttentionLedger::default();
    let mut settled = |local: &ClientShellSnapshot, mini: &ClientShellSnapshot| {
        let sources = [
            source("local", "Local", local),
            source("ssh:mini", "Mini", mini),
        ];
        settle(&sources, &mut ledger, daemon_mark, |_, _| false);
    };
    settled(&local, &mini);
    mini.tabs[0].agent_status = AgentStatus::Blocked;
    settled(&local, &mini);
    local.tabs[2].agent_status = AgentStatus::Blocked;
    settled(&local, &mini);

    let mut devices = [
        input("local", "Local", &local),
        input("ssh:mini", "Mini", &mini),
    ];
    let board = build(&devices, &ledger, daemon_mark, &[]);
    let waiting: Vec<(&str, &str, &str)> = board
        .waiting
        .iter()
        .map(|row| {
            (
                row.row.id.as_str(),
                row.row.title.as_str(),
                row.place.as_str(),
            )
        })
        .collect();
    assert_eq!(
        waiting,
        [
            ("t3", "review the patch", "fedi"),
            ("t1", "ship it", "worksteps \u{b7} Mini")
        ]
    );
    assert_eq!(refs(&board)[..2], [("local", "t3"), ("ssh:mini", "t1")]);

    // A device that dropped keeps its last rows but no longer reports waiting.
    devices[1].source.connected = false;
    devices[1].detail = Some("ssh: connection\u{1b} refused");
    let board = build(&devices, &ledger, daemon_mark, &[]);
    assert_eq!(board.waiting.len(), 1);
    assert_eq!(board.devices[1].status, DeviceStatus::Disconnected);
    assert_eq!(
        board.devices[1].detail.as_deref(),
        Some("ssh: connection refused")
    );
    assert_eq!(board.devices[1].spaces[0].tabs.len(), 2);
    devices[1].connecting = true;
    devices[1].source.snapshot = None;
    let board = build(&devices, &ledger, daemon_mark, &[]);
    assert_eq!(board.devices[1].status, DeviceStatus::Connecting);
    assert!(board.devices[1].spaces.is_empty());
    devices[1].source.enabled = false;
    assert_eq!(
        build(&devices, &ledger, daemon_mark, &[]).devices[1].status,
        DeviceStatus::Disabled
    );
}

#[test]
fn untrusted_titles_are_stripped_and_bounded() {
    let mut local = local();
    local.agents[0].terminal_title_stripped = Some(format!("\u{202e}{}\u{7}", "x".repeat(500)));
    let board = built(&[input("local", "Local", &local)]);
    let title = &board.devices[0].spaces[0].tabs[1].title;
    assert_eq!(title.chars().count(), 199);
    assert!(title.chars().all(|c| c == 'x'));
}

#[test]
fn usage_rows_carry_two_gauges_and_whether_the_plan_is_used_up() {
    let local = local();
    let windows = [
        Window::new(Kind::Session, 100., Some(at(1_700_000_000)), None),
        Window::new(Kind::Weekly, 61., None, None),
        Window::new(Kind::Named("Fable weekly".into()), 85., None, None),
    ];
    let usage = [UsageInput {
        device_id: "local",
        agent: "claude",
        name: "Claude",
        windows: &windows,
    }];
    let board = build(
        &[input("local", "Local", &local)],
        &AttentionLedger::default(),
        daemon_mark,
        &usage,
    );
    let row = &board.usage[0];
    assert!(row.limited);
    let gauges: Vec<(&str, u32, &str, Option<u64>)> = row
        .gauges
        .iter()
        .map(|gauge| {
            (
                gauge.label.as_str(),
                gauge.percent,
                gauge.severity.as_str(),
                gauge.resets_at,
            )
        })
        .collect();
    assert_eq!(
        gauges,
        [
            ("5h", 100, "critical", Some(1_700_000_000)),
            ("Fable", 85, "critical", None)
        ]
    );
}

#[test]
fn the_json_shape_is_the_contract() {
    let (local, mini) = (local(), mini());
    let mut board = built(&[
        input("local", "Local", &local),
        input("ssh:mini", "Mini", &mini),
    ]);
    board.revision = 7;
    let json: serde_json::Value = serde_json::from_str(&board.to_json().unwrap()).unwrap();
    let keys = |value: &serde_json::Value| -> Vec<String> {
        let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        keys
    };
    assert_eq!(
        keys(&json),
        [
            "attention",
            "devices",
            "revision",
            "usage",
            "version",
            "waiting"
        ]
    );
    assert_eq!(json["version"], 1);
    assert_eq!(json["revision"], 7);
    assert_eq!(
        keys(&json["devices"][0]),
        [
            "detail", "id", "label", "route", "spaces", "state", "status"
        ]
    );
    assert_eq!(json["devices"][0]["status"], "connected");
    assert_eq!(
        keys(&json["devices"][0]["spaces"][0]),
        ["child", "focused", "id", "label", "number", "state", "tabs"]
    );
    assert_eq!(
        json["devices"][0]["spaces"][1]["tabs"][0],
        serde_json::json!({
            "id": "t3",
            "device_id": "local",
            "space_id": "w2",
            "pane_id": "p-t3",
            "terminal_id": "p-t3",
            "number": 3,
            "title": "review the patch",
            "agent": { "id": "codex", "name": "codex" },
            "state": "blocked",
            "state_text": "needs approval",
            "symbol": "\u{d7}",
            "attention_rank": 0,
            "focused": false
        })
    );
    assert_eq!(
        json["attention"][0],
        serde_json::json!({ "device_id": "local", "tab_id": "t3" })
    );
    assert_eq!(keys(&json["waiting"][0]), ["place", "row"]);
    assert_eq!(json["waiting"][0]["row"]["device_id"], "local");
    // What a client reads back is what was published.
    assert_eq!(serde_json::from_value::<Board>(json).unwrap(), board);
}
