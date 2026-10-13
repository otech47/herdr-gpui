//! The sidebar: spaces and agents, the rows that show them, and the hover
//! menu a resting pointer opens.

mod agents;
mod cell;
mod hover;
mod layout;
mod layouts;
mod metrics;
mod order;
mod ports;
pub(crate) mod preview;
mod rail;
mod render;
mod reorder;
mod row;
mod sections;
mod tab_reorder;
mod tabs;
mod tokens;
mod trailing;
mod view;
mod workspaces;

#[cfg(test)]
mod tests;

#[cfg(any(test, feature = "integration-test"))]
pub(crate) mod layout_tests;

#[cfg(all(feature = "integration-test", target_os = "macos"))]
pub(crate) mod native_tests;

pub(crate) use {
    agents::{Indicators, agent_name, state_label, status_indicator},
    hover::{HoverMenu, HoverRest},
    metrics::{ARROW_RESERVE, HOST_ARROW_WIDTH, HOST_GAP, ICON_RESERVE, LABEL_GAP, STATUS_WIDTH},
    ports::PortsMenu,
    rail::SidebarMode,
    reorder::WorkspaceDrag,
    row::{compact, github_mark, label_text},
    tab_reorder::TabRowDrag,
    view::SidebarView,
    workspaces::workspace_label,
};

pub(crate) use order::step as sidebar_step;

#[cfg(any(test, feature = "integration-test"))]
pub(crate) use metrics::LABEL_WIDTH;

pub(crate) use view::cached as cached_view;

use agents::{agents_sort, sorted_agents};
use metrics::*;
use row::{RowBadge, first_text};
use workspaces::visible_workspace_entries;

pub(crate) const DEVICE_FOOTER_HEIGHT: f32 = 40.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum SidebarDrag {
    Width {
        start: f32,
        width: f32,
    },
    /// The boundary between the two growing lists: spaces and agents, or
    /// tabs and agents when a tabs list is shown.
    Split,
    /// The spaces list's own height, above a tabs list.
    Spaces,
}
