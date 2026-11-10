#![allow(clippy::unwrap_used)]

use super::{
    STATUS_DOT_UNKNOWN, STATUS_WIDTH,
    agents::{Indicators, Mark, status_indicator, status_style, status_symbol, status_text},
    layout_tests,
    sections::header,
};
use crate::{
    config::{FontConfig, Theme},
    contrast::Contrast,
    herdr_settings::{IndicatorStyle, Settings},
};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellWorkspace};

mod limited;
mod monitoring;
mod row_text;
mod statuses;
