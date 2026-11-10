//! The state a row shows. Status comes from the daemon's snapshot, never from
//! guessing at terminal output.

use herdr_protocol::AgentStatus;

/// The daemon's status, or what the daemon cannot see: that the agent's plan
/// is used up, which a client reads from the host's usage, or that an idle
/// agent left a shell running, which it reads from the host's processes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mark {
    Status(AgentStatus),
    Limited,
    Monitoring,
}

impl From<AgentStatus> for Mark {
    fn from(status: AgentStatus) -> Self {
        Self::Status(status)
    }
}

impl Mark {
    /// Every mark, in the order of [`Mark::index`].
    pub const ALL: [Self; 7] = [
        Self::Status(AgentStatus::Unknown),
        Self::Status(AgentStatus::Idle),
        Self::Status(AgentStatus::Working),
        Self::Status(AgentStatus::Done),
        Self::Status(AgentStatus::Blocked),
        Self::Limited,
        Self::Monitoring,
    ];

    /// A dense index, for a palette with one slot per mark.
    pub fn index(self) -> usize {
        match self {
            Self::Status(AgentStatus::Unknown) => 0,
            Self::Status(AgentStatus::Idle) => 1,
            Self::Status(AgentStatus::Working) => 2,
            Self::Status(AgentStatus::Done) => 3,
            Self::Status(AgentStatus::Blocked) => 4,
            Self::Limited => 5,
            Self::Monitoring => 6,
        }
    }
}

/// Attention first, then the most recent change, as upstream orders it.
pub fn status_priority(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Done => 3,
        AgentStatus::Working => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}

pub fn status_symbol(mark: impl Into<Mark>) -> &'static str {
    match mark.into() {
        Mark::Status(AgentStatus::Working) => "\u{25d0}",
        Mark::Status(AgentStatus::Blocked) => "\u{d7}",
        Mark::Status(AgentStatus::Done) => "\u{2713}",
        Mark::Status(AgentStatus::Idle) => "\u{25cb}",
        Mark::Status(AgentStatus::Unknown) => "\u{b7}",
        Mark::Limited => "\u{2298}",
        Mark::Monitoring => "\u{25ce}",
    }
}

/// The word the daemon's `state_text` token shows for a status when its
/// sidebar config asks for it. Lowercase, matching the daemon's status names
/// and what the terminal client prints.
pub fn status_text(mark: impl Into<Mark>) -> &'static str {
    match mark.into() {
        Mark::Status(AgentStatus::Working) => "working",
        Mark::Status(AgentStatus::Blocked) => "blocked",
        Mark::Status(AgentStatus::Done) => "done",
        Mark::Status(AgentStatus::Idle) => "idle",
        Mark::Status(AgentStatus::Unknown) => "unknown",
        Mark::Limited => "limited",
        Mark::Monitoring => "monitoring",
    }
}
