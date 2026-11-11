//! The board a client draws: every device, its spaces and their tab rows,
//! the order the rows want attention in, the tabs waiting on an answer and
//! the usage gauges. Plain data, so it crosses a process boundary as JSON.

use crate::{
    AttentionLedger, ListedTab, Mark, Result, Sighting, Source, agent_name, attention_rank,
    safe_text, state_label, status_priority, status_symbol, status_text, tab_agent, tab_title,
    usage, waiting_elsewhere, waiting_place, workspace_entries, workspace_label,
    workspace_label_by_id,
};
use herdr_protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot, ClientShellTab};
use serde::{Deserialize, Serialize};
use std::{cmp::Reverse, time::UNIX_EPOCH};

/// Titles and labels are untrusted terminal text, bounded before they leave.
const TEXT_LIMIT: usize = 200;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Board {
    /// The contract's version. A client refuses a version it does not know.
    pub version: u32,
    /// Grows each time anything else in the board changes, and never falls.
    pub revision: u64,
    pub devices: Vec<Device>,
    /// Every tab row across the devices, the one that most wants attention first.
    pub attention: Vec<RowRef>,
    /// Tabs waiting on an answer, the latest to start waiting first.
    pub waiting: Vec<WaitingRow>,
    pub usage: Vec<UsageRow>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeviceStatus {
    Connected,
    Connecting,
    Disconnected,
    Disabled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Device {
    pub id: String,
    pub label: String,
    pub status: DeviceStatus,
    /// Why the device is not connected, when its connection said.
    pub detail: Option<String>,
    /// The SSH targets from the publisher's machine to the device, each
    /// resolved by the machine before it. Empty for the publisher's own.
    pub route: Vec<String>,
    /// The most urgent state among its tabs.
    pub state: String,
    pub spaces: Vec<Space>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Space {
    pub id: String,
    pub label: String,
    pub number: usize,
    /// A linked worktree, drawn indented under its repository's rows.
    pub child: bool,
    pub state: String,
    pub focused: bool,
    pub tabs: Vec<TabRow>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TabRow {
    pub id: String,
    pub device_id: String,
    pub space_id: String,
    /// The pane the row stands for: its agent's, else the tab's focused pane.
    pub pane_id: Option<String>,
    /// What `herdr terminal session control|observe` takes to open the row's
    /// terminal on its device.
    pub terminal_id: Option<String>,
    pub number: usize,
    pub title: String,
    pub agent: Option<AgentLabel>,
    /// `blocked`, `limited`, `working`, `done`, `monitoring`, `idle` or `unknown`.
    pub state: String,
    /// The words to show for the state: the agent's own when it gave any.
    pub state_text: String,
    pub symbol: String,
    /// Where the row sorts by attention, the lowest first.
    pub attention_rank: u8,
    pub focused: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentLabel {
    /// The agent's kind, such as `claude`, which names its icon.
    pub id: Option<String>,
    pub name: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowRef {
    pub device_id: String,
    pub tab_id: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WaitingRow {
    pub row: TabRow,
    /// The row's tag: its space, and its device when that is not the first.
    pub place: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct UsageRow {
    pub device_id: String,
    /// The agent kind the limits belong to, as in [`AgentLabel::id`].
    pub agent: String,
    pub name: String,
    /// A window has nothing left, so the agent's rows are marked limited.
    pub limited: bool,
    pub gauges: Vec<Gauge>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Gauge {
    pub label: String,
    pub title: String,
    pub percent: u32,
    /// `normal`, `warning` or `critical`.
    pub severity: String,
    /// Unix seconds.
    pub resets_at: Option<u64>,
}

#[derive(Clone, Copy, Debug)]
pub struct DeviceInput<'a> {
    pub source: Source<'a>,
    /// Still on its way to a first connection, not yet failed.
    pub connecting: bool,
    pub detail: Option<&'a str>,
    pub route: &'a [String],
}

#[derive(Clone, Copy, Debug)]
pub struct UsageInput<'a> {
    pub device_id: &'a str,
    pub agent: &'a str,
    pub name: &'a str,
    pub windows: &'a [usage::Window],
}

impl Board {
    pub const VERSION: u32 = 1;

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

/// Settle every tab's place before a [`build`]. `held` names a row that
/// keeps its place whatever its mark, such as the one the user is on.
pub fn settle(
    sources: &[Source<'_>],
    ledger: &mut AttentionLedger,
    mark: impl Fn(usize, Option<&ClientShellAgent>, AgentStatus) -> Mark,
    held: impl Fn(usize, &ClientShellTab) -> bool,
) {
    let mut sightings = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let Some(snapshot) = source.snapshot else {
            continue;
        };
        for tab in &snapshot.tabs {
            let agent = tab_agent(snapshot, &tab.tab_id);
            sightings.push(Sighting {
                device: source.id.to_owned(),
                tab: tab.tab_id.clone(),
                rank: attention_rank(mark(index, agent, tab.agent_status)),
                held: held(index, tab),
            });
        }
    }
    ledger.settle(sightings);
}

/// The first device is the client's own, which a waiting row's tag leaves
/// unnamed. The revision is the publisher's to set.
pub fn build(
    devices: &[DeviceInput<'_>],
    ledger: &AttentionLedger,
    mark: impl Fn(usize, Option<&ClientShellAgent>, AgentStatus) -> Mark,
    usage: &[UsageInput<'_>],
) -> Board {
    let sources: Vec<Source<'_>> = devices.iter().map(|device| device.source).collect();
    let row = |listed: &ListedTab<'_>| {
        let source = &sources[listed.endpoint];
        let agent = tab_agent(listed.snapshot, &listed.tab.tab_id);
        let mark = mark(listed.endpoint, agent, listed.tab.agent_status);
        let place = ledger.place(source.id, &listed.tab.tab_id, attention_rank(mark));
        (
            tab_row(source.id, listed, agent, mark, place.rank),
            place.since,
        )
    };

    let mut ordered = Vec::new();
    let mut board_devices = Vec::with_capacity(devices.len());
    for (index, device) in devices.iter().enumerate() {
        let source = &device.source;
        let mut spaces = Vec::new();
        let mut urgent = AgentStatus::Unknown;
        if let Some(snapshot) = source.snapshot.filter(|_| source.enabled) {
            for (workspace, child) in workspace_entries(&snapshot.workspaces) {
                let workspace = &snapshot.workspaces[workspace];
                let mut tabs = Vec::new();
                for tab in snapshot
                    .tabs
                    .iter()
                    .filter(|tab| tab.workspace_id == workspace.workspace_id)
                {
                    if status_priority(tab.agent_status) > status_priority(urgent) {
                        urgent = tab.agent_status;
                    }
                    let (row, since) = row(&ListedTab {
                        endpoint: index,
                        snapshot,
                        tab,
                    });
                    ordered.push((
                        row.attention_rank,
                        Reverse(since),
                        RowRef {
                            device_id: row.device_id.clone(),
                            tab_id: row.id.clone(),
                        },
                    ));
                    tabs.push(row);
                }
                spaces.push(Space {
                    id: workspace.workspace_id.clone(),
                    label: safe_text(workspace_label(workspace, child), TEXT_LIMIT),
                    number: workspace.number,
                    child,
                    state: status_text(workspace.agent_status).to_owned(),
                    focused: workspace.focused,
                    tabs,
                });
            }
        }
        board_devices.push(Device {
            id: source.id.to_owned(),
            label: safe_text(source.label, TEXT_LIMIT),
            status: match (source.enabled, source.connected, device.connecting) {
                (false, ..) => DeviceStatus::Disabled,
                (true, true, _) => DeviceStatus::Connected,
                (true, false, true) => DeviceStatus::Connecting,
                (true, false, false) => DeviceStatus::Disconnected,
            },
            detail: device.detail.map(|detail| safe_text(detail, TEXT_LIMIT)),
            route: device.route.to_vec(),
            state: status_text(urgent).to_owned(),
            spaces,
        });
    }
    // Stable, so rows of one place keep device order, then the daemon's.
    ordered.sort_by_key(|(rank, since, _)| (*rank, *since));

    let waiting = waiting_elsewhere(&sources, &[], ledger)
        .iter()
        .map(|listed| {
            let space = workspace_label_by_id(listed.snapshot, &listed.tab.workspace_id);
            let device = (listed.endpoint != 0).then_some(sources[listed.endpoint].label);
            WaitingRow {
                row: row(listed).0,
                place: safe_text(&waiting_place(space.unwrap_or(""), device), TEXT_LIMIT),
            }
        })
        .collect();

    Board {
        version: Board::VERSION,
        revision: 0,
        devices: board_devices,
        attention: ordered.into_iter().map(|(.., row)| row).collect(),
        waiting,
        usage: usage.iter().map(usage_row).collect(),
    }
}

fn tab_row(
    device: &str,
    listed: &ListedTab<'_>,
    agent: Option<&ClientShellAgent>,
    mark: Mark,
    rank: u8,
) -> TabRow {
    let tab = listed.tab;
    let pane = agent
        .map(|agent| agent.pane_id.clone())
        .or_else(|| tab_pane(listed.snapshot, &tab.tab_id));
    let word = status_text(mark);
    TabRow {
        id: tab.tab_id.clone(),
        device_id: device.to_owned(),
        space_id: tab.workspace_id.clone(),
        terminal_id: pane.clone(),
        pane_id: pane,
        number: tab.number,
        title: safe_text(&tab_title(tab, listed.snapshot), TEXT_LIMIT),
        agent: agent.map(|agent| AgentLabel {
            id: agent.agent.as_deref().map(|id| safe_text(id, TEXT_LIMIT)),
            name: safe_text(agent_name(agent), TEXT_LIMIT),
        }),
        state: word.to_owned(),
        state_text: match (agent, mark) {
            (Some(agent), Mark::Status(_)) => state_label(agent, word).into_owned(),
            _ => word.to_owned(),
        },
        symbol: status_symbol(mark).to_owned(),
        attention_rank: rank,
        focused: tab.focused,
    }
}

fn tab_pane(snapshot: &ClientShellSnapshot, tab: &str) -> Option<String> {
    let panes = || snapshot.panes.iter().filter(|pane| pane.tab_id == tab);
    panes()
        .find(|pane| pane.focused)
        .or_else(|| panes().next())
        .map(|pane| pane.pane_id.clone())
}

fn usage_row(input: &UsageInput<'_>) -> UsageRow {
    UsageRow {
        device_id: input.device_id.to_owned(),
        agent: input.agent.to_owned(),
        name: input.name.to_owned(),
        limited: usage::used_up(input.windows),
        gauges: usage::shown(input.windows)
            .into_iter()
            .map(|window| Gauge {
                label: window.kind.short().to_owned(),
                title: window.kind.title().to_owned(),
                percent: window.percent(),
                severity: usage::Severity::from(window.used).token().to_owned(),
                resets_at: window
                    .resets_at
                    .and_then(|at| at.duration_since(UNIX_EPOCH).ok())
                    .map(|since| since.as_secs()),
            })
            .collect(),
    }
}
