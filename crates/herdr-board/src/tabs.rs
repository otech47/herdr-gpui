//! Which tabs a list shows: one device's shown space alone, or with that
//! space's namesakes on the other devices. In which order: the daemon's, or
//! the tabs that want attention first. And which tabs wait on an answer
//! somewhere the list does not show.

use crate::{AttentionLedger, agent_name, agent_names, text::clipped};
use herdr_protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot, ClientShellTab};
use std::cmp::Reverse;

/// One device as the lists see it.
#[derive(Clone, Copy, Debug)]
pub struct Source<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub enabled: bool,
    /// Whether a device filter lets its tabs join another device's list.
    pub visible: bool,
    pub connected: bool,
    pub snapshot: Option<&'a ClientShellSnapshot>,
}

/// A listed tab, with the device and snapshot it belongs to.
#[derive(Clone, Copy, Debug)]
pub struct ListedTab<'a> {
    /// Its device's index among the sources.
    pub endpoint: usize,
    pub snapshot: &'a ClientShellSnapshot,
    pub tab: &'a ClientShellTab,
}

/// The listed tabs in numbering order: the selected device's shown space,
/// then with `across` each other device's space of that name.
pub fn listed_tabs<'a>(
    sources: &[Source<'a>],
    selected: usize,
    across: bool,
) -> Vec<ListedTab<'a>> {
    let Some(shown) = sources.get(selected).and_then(|source| source.snapshot) else {
        return Vec::new();
    };
    let Some(focused) = shown.focused_workspace_id.as_deref() else {
        return Vec::new();
    };
    let label = across
        .then(|| workspace_label_by_id(shown, focused))
        .flatten();
    let mut listed = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        let (snapshot, workspace) = if index == selected {
            (shown, focused)
        } else {
            let Some(label) = label else {
                continue;
            };
            if !source.enabled || !source.visible {
                continue;
            }
            let Some(snapshot) = source.snapshot else {
                continue;
            };
            let Some(namesake) = snapshot
                .workspaces
                .iter()
                .find(|workspace| workspace.label == label)
            else {
                continue;
            };
            (snapshot, namesake.workspace_id.as_str())
        };
        listed.extend(
            snapshot
                .tabs
                .iter()
                .filter(|tab| tab.workspace_id == workspace)
                .map(|tab| ListedTab {
                    endpoint: index,
                    snapshot,
                    tab,
                }),
        );
    }
    listed
}

/// Reorder rows by their settled place, the order they came in breaking
/// ties. `grouped` keeps each device's rows together. `rank` is where an
/// unsettled row would sort.
pub fn sort_by_attention(
    listed: &mut [ListedTab<'_>],
    sources: &[Source<'_>],
    ledger: &AttentionLedger,
    grouped: bool,
    rank: impl Fn(&ListedTab<'_>) -> u8,
) {
    listed.sort_by_key(|row| {
        let place = ledger.place(sources[row.endpoint].id, &row.tab.tab_id, rank(row));
        (
            grouped.then_some(row.endpoint),
            place.rank,
            Reverse(place.since),
        )
    });
}

/// Blocked tabs `listed` does not hold, on every enabled and connected
/// device, the latest to start waiting first.
pub fn waiting_elsewhere<'a>(
    sources: &[Source<'a>],
    listed: &[ListedTab<'_>],
    ledger: &AttentionLedger,
) -> Vec<ListedTab<'a>> {
    let mut waiting = Vec::new();
    for (index, source) in sources.iter().enumerate() {
        if !source.enabled || !source.connected {
            continue;
        }
        let Some(snapshot) = source.snapshot else {
            continue;
        };
        waiting.extend(
            snapshot
                .tabs
                .iter()
                .filter(|tab| tab.agent_status == AgentStatus::Blocked)
                .filter(|tab| {
                    !listed
                        .iter()
                        .any(|row| row.endpoint == index && row.tab.tab_id == tab.tab_id)
                })
                .map(|tab| ListedTab {
                    endpoint: index,
                    snapshot,
                    tab,
                }),
        );
    }
    waiting.sort_by_key(|row| {
        Reverse(
            ledger
                .place(sources[row.endpoint].id, &row.tab.tab_id, 0)
                .since,
        )
    });
    waiting
}

/// Where a waiting tab is: its space, and its device when that is not the
/// client's own, each cut short so the title keeps the row's width.
pub fn waiting_place(space: &str, device: Option<&str>) -> String {
    let space = clipped(space);
    match device {
        Some(device) => format!("{space} \u{b7} {}", clipped(device)),
        None => space,
    }
}

pub fn workspace_label_by_id<'a>(
    snapshot: &'a ClientShellSnapshot,
    workspace: &str,
) -> Option<&'a str> {
    snapshot
        .workspaces
        .iter()
        .find(|candidate| candidate.workspace_id == workspace)
        .map(|candidate| candidate.label.as_str())
}

/// The agent a tab's row stands for.
pub fn tab_agent<'a>(
    snapshot: &'a ClientShellSnapshot,
    tab_id: &str,
) -> Option<&'a ClientShellAgent> {
    snapshot.agents.iter().find(|agent| agent.tab_id == tab_id)
}

/// What a tab is called in a list: its agent's own terminal title, else the
/// tab's name when renamed, else the agent, else the shell's directory.
pub fn tab_title(tab: &ClientShellTab, snapshot: &ClientShellSnapshot) -> String {
    let agent = tab_agent(snapshot, &tab.tab_id);
    if let Some(title) = agent.and_then(agent_title) {
        return title;
    }
    if tab.custom_label {
        return tab.label.clone();
    }
    if let Some(agent) = agent {
        return agent_name(agent).to_owned();
    }
    let cwd = snapshot
        .panes
        .iter()
        .find(|pane| pane.tab_id == tab.tab_id)
        .and_then(|pane| pane.foreground_cwd.as_deref().or(pane.cwd.as_deref()));
    match cwd
        .and_then(|cwd| std::path::Path::new(cwd).file_name())
        .and_then(|name| name.to_str())
    {
        Some(name) => name.to_owned(),
        None => tab.label.clone(),
    }
}

/// Agents set a default title such as "Claude Code" until a session is named,
/// and a tab name says more than that.
const DEFAULT_TITLES: [&str; 4] = ["claude code", "codex", "kimi code", "kimi"];

fn agent_title(agent: &ClientShellAgent) -> Option<String> {
    let title = agent.terminal_title_stripped.as_deref()?.trim();
    let default = DEFAULT_TITLES
        .into_iter()
        .chain(agent_names(agent).into_iter().flatten())
        .any(|name| title.eq_ignore_ascii_case(name.trim()));
    (!title.is_empty() && !default).then(|| title.to_owned())
}
