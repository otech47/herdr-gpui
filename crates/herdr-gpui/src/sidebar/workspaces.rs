//! Which workspaces the tree shows and how each one is named. A repository and
//! its linked worktrees form one group, folded or unfolded locally: collapsing
//! changes this client's view only, never the daemon's state.
//!
//! Grouping follows the TUI: every checkout that is not a linked worktree is a
//! parent, never another parent's child, and a repository only forms a group
//! once it has both a parent and at least one linked worktree open. Membership
//! comes from the daemon alone, so a checkout the daemon restored while it was
//! unavailable keeps its group.

use super::{RowBadge, row::PrBadge};
use crate::config::Theme;
use herdr_client::protocol::ClientShellWorkspace;

pub(crate) use herdr_board::workspace_label;
pub(super) use herdr_board::{
    displayed_workspace_status, grouped_keys, visible_workspace_entries, workspace_entries,
};

/// Cached pull request, dirty and teleported marks for a worktree row, if the prefetch and
/// the Git probe already have them. Rendering only reads: a missing entry
/// simply shows nothing, never a stale or guessed state.
/// `cache` is `None` for a row on a device other than the selected one: the
/// cache holds only that device's lookups, and the same path and branch can
/// exist on another host.
pub(super) fn workspace_badge(
    workspace: &ClientShellWorkspace,
    cache: Option<&crate::pull_request::Cache>,
    git: &crate::git::Git,
    marks: (&crate::teleport::Marks, &str),
    theme: &Theme,
) -> Option<RowBadge> {
    let key = workspace.worktree.as_ref()?.key.as_str();
    let branch = workspace.branch.as_deref()?;
    let (marks, endpoint) = marks;
    RowBadge::new(
        cache
            .and_then(|cache| cache.peek(key, branch))
            .map(|pr| PrBadge::new(pr, theme)),
        git.dirty(key, branch).unwrap_or(false),
        marks.find(endpoint, key, branch).is_some(),
    )
}
