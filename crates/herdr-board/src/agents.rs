//! The agents list: how it is sorted, and how each agent's place and status
//! are labelled.

use crate::{first_text, safe_text, status_priority, status_text};
use herdr_protocol::{ClientShellAgent, ClientShellSnapshot};

/// How the agents panel orders its rows, as in the terminal client: grouped
/// keeps the daemon's workspace order, priority floats the agents that want
/// attention. Herdr's `ui.agent_panel_sort` seeds it until the user toggles
/// it, which deserializes upstream's spellings ("spaces", alias "workspaces").
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Deserialize)]
pub enum AgentSort {
    #[default]
    #[serde(rename = "spaces", alias = "workspaces")]
    Grouped,
    #[serde(rename = "priority")]
    Priority,
}

impl std::fmt::Display for AgentSort {
    /// Also the stored spelling, which `AgentSort::parse` reads back.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Grouped => "grouped",
            Self::Priority => "priority",
        })
    }
}

impl AgentSort {
    pub fn toggled(self) -> Self {
        match self {
            Self::Grouped => Self::Priority,
            Self::Priority => Self::Grouped,
        }
    }

    /// An unreadable or unknown stored value is no override at all, so the
    /// daemon's setting applies rather than a guessed sort.
    pub fn parse(value: Option<&serde_json::Value>) -> Option<Self> {
        match value.and_then(serde_json::Value::as_str)? {
            "grouped" => Some(Self::Grouped),
            "priority" => Some(Self::Priority),
            _ => None,
        }
    }
}

/// The agents of one endpoint in the order the panel paints them.
///
/// A plugin's agent view (`agent.view.set`) is authoritative while the daemon
/// names it: the panel shows exactly the panes in `agent_order`, in that
/// order, so agents its filter excluded stay hidden and an empty order means
/// none match. Upstream's client keys this on the label, not on the order
/// being non-empty, and drops ids the snapshot has no agent for. Without a
/// view the local sort applies.
pub fn sorted_agents(snapshot: &ClientShellSnapshot, sort: AgentSort) -> Vec<&ClientShellAgent> {
    if snapshot.agent_view_label.is_some() {
        return snapshot
            .agent_order
            .iter()
            .filter_map(|pane_id| {
                snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == *pane_id)
            })
            .collect();
    }
    let mut ordered: Vec<_> = snapshot.agents.iter().collect();
    if sort == AgentSort::Priority {
        ordered.sort_by_key(|agent| {
            (
                std::cmp::Reverse(status_priority(agent.agent_status)),
                std::cmp::Reverse(agent.state_change_seq),
            )
        });
    }
    ordered
}

/// What an agent is called wherever it is listed.
pub fn agent_name(agent: &ClientShellAgent) -> &str {
    first_text(agent_names(agent), "agent")
}

pub fn agent_names(agent: &ClientShellAgent) -> [Option<&str>; 4] {
    [
        agent.display_agent.as_deref(),
        agent.name.as_deref(),
        agent.agent.as_deref(),
        agent.title.as_deref(),
    ]
}

/// Where an agent runs: its workspace, and its tab when that earns a place,
/// which upstream decides by the workspace having several tabs or the user
/// naming it. `None` once the agent's workspace has gone.
pub fn agent_place<'a>(
    agent: &ClientShellAgent,
    snapshot: &'a ClientShellSnapshot,
) -> Option<(&'a str, Option<&'a str>)> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == agent.workspace_id)
        .count();
    let tab = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == agent.tab_id)
        .filter(|tab| tabs > 1 || tab.custom_label)
        .map(|tab| tab.label.as_str());
    Some((workspace.label.as_str(), tab))
}

/// Upstream's default agent rows: host, workspace and tab on the first line,
/// the agent itself on the second. A pane whose workspace has gone leaves the
/// agent to name the row.
pub fn agent_labels<'a>(
    name: &'a str,
    place: Option<(&'a str, Option<&'a str>)>,
    host: Option<&'a str>,
) -> (Vec<(&'a str, bool)>, &'a str) {
    let Some((workspace, tab)) = place else {
        return (vec![(name, true)], "");
    };
    // Only the workspace carries the row's weight: upstream paints the host and
    // tab around it in its secondary color.
    let segments = [(host, false), (Some(workspace), true), (tab, false)]
        .into_iter()
        .filter_map(|(text, primary)| Some((text?, primary)))
        .filter(|(text, _)| !text.is_empty())
        .collect();
    (segments, name)
}

/// Herdr's own cap on a state label, so a longer one came from a peer that
/// ignored it and is cut where the terminal client would have cut it.
const STATE_LABEL_LIMIT: usize = 80;

/// The words an agent's integration chose for its current status, as Herdr's
/// `state_labels` metadata, else `fallback`. Labels are untrusted display
/// text: bounded and stripped of controls and bidi overrides.
pub fn state_label<'a>(agent: &ClientShellAgent, fallback: &'a str) -> std::borrow::Cow<'a, str> {
    let key = status_text(agent.agent_status);
    agent
        .state_labels
        .iter()
        .find(|(state, _)| state == key)
        .map(|(_, label)| safe_text(label, STATE_LABEL_LIMIT))
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty())
        .map_or(
            std::borrow::Cow::Borrowed(fallback),
            std::borrow::Cow::Owned,
        )
}
