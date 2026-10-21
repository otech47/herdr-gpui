#![allow(clippy::unwrap_used)]

use super::{
    STATUS_DOT_UNKNOWN, STATUS_WIDTH, agent_name,
    agents::{
        Indicators, Mark, agent_labels, agent_place, status_indicator, status_style, status_symbol,
        status_text,
    },
    layout_tests,
    sections::header,
    workspace_label,
    workspaces::workspace_entries,
};
use crate::{
    config::{FontConfig, Theme},
    contrast::Contrast,
    herdr_settings::{IndicatorStyle, Settings},
};
use herdr_client::protocol::{
    AgentStatus, ClientShellAgent, ClientShellSnapshot, ClientShellWorkspace,
};

mod hierarchy;
mod limited;
mod monitoring;
mod row_text;
mod statuses;
