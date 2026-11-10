//! The session board: which devices, spaces and tabs a client lists, what
//! each row is called, which state it shows and in which order. Every
//! decision here is made from daemon snapshots alone, without a GUI, so the
//! desktop sidebar and a remote client draw the same model.
#![doc = include_str!("../README.md")]

mod agents;
mod attention;
mod board;
mod error;
mod mark;
mod tabs;
mod text;
pub mod usage;
mod workspaces;

#[cfg(test)]
mod tests;

pub use herdr_protocol as protocol;

pub use agents::{
    AgentSort, agent_labels, agent_name, agent_names, agent_place, sorted_agents, state_label,
};
pub use attention::{AttentionLedger, Place, Sighting, attention_rank};
pub use board::{
    AgentLabel, Board, Device, DeviceInput, DeviceStatus, Gauge, RowRef, Space, TabRow, UsageInput,
    UsageRow, WaitingRow, build, settle,
};
pub use error::{Error, Result};
pub use mark::{Mark, status_priority, status_symbol, status_text};
pub use tabs::{
    ListedTab, Source, listed_tabs, sort_by_attention, tab_agent, tab_title, waiting_elsewhere,
    waiting_place, workspace_label_by_id,
};
pub use text::{first_text, safe_text};
pub use workspaces::{
    displayed_workspace_status, grouped_keys, visible_workspace_entries, workspace_entries,
    workspace_label,
};
