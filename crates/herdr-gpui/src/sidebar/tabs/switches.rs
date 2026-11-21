use super::ListedTab;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(in crate::sidebar) struct Member {
    pub(super) provider: &'static str,
    pub(super) state: &'static str,
    pub(super) first: bool,
}

type RowKey<'a> = (usize, &'a str);
pub(in crate::sidebar) type Members<'a> = HashMap<RowKey<'a>, Member>;

fn membership(row: &ListedTab<'_>) -> Option<(uuid::Uuid, u64, Member)> {
    let (workspace, tab) = row.tab.tab_id.split_once(':')?;
    if workspace != row.tab.workspace_id {
        return None;
    }
    let tokens = &row
        .snapshot
        .workspaces
        .iter()
        .find(|ws| ws.workspace_id == workspace)?
        .tokens;
    let (_, value) = tokens
        .iter()
        .find(|(key, _)| key.strip_prefix("switch_") == Some(tab))?;
    let mut parts = value.split_ascii_whitespace();
    let task = parts.next()?.parse().ok()?;
    let provider = match parts.next()? {
        "claude" => "Claude",
        "codex" => "Codex",
        "kimi" => "Kimi",
        _ => return None,
    };
    let state = match parts.next()? {
        "previous" => "Previous",
        "opening" => "Opening",
        "preparing" => "Preparing",
        "ready" => "Ready",
        "failed" => "Switch failed",
        _ => return None,
    };
    let ordinal = tab.strip_prefix('t')?.parse().ok()?;
    parts.next().is_none().then_some((
        task,
        ordinal,
        Member {
            provider,
            state,
            first: false,
        },
    ))
}

// A group takes its earliest member's sorted slot. Within it, tabs follow creation order.
pub(super) fn group<'a>(listed: &mut [ListedTab<'a>]) -> Members<'a> {
    let mut groups = HashMap::new();
    for (index, row) in listed.iter().enumerate() {
        if let Some((task, ordinal, member)) = membership(row) {
            groups
                .entry((row.endpoint, row.tab.workspace_id.as_str(), task))
                .or_insert_with(Vec::new)
                .push((index, ordinal, member));
        }
    }
    let mut members = HashMap::new();
    let mut slots = HashMap::new();
    for entries in groups.values_mut().filter(|entries| entries.len() > 1) {
        let slot = entries[0].0;
        entries.sort_by_key(|entry| entry.1);
        for (offset, &(index, _, mut member)) in entries.iter().enumerate() {
            let row = listed[index];
            let key = (row.endpoint, row.tab.tab_id.as_str());
            member.first = offset == 0;
            members.insert(key, member);
            slots.insert(key, (slot, offset));
        }
    }
    if !members.is_empty() {
        for (index, row) in listed.iter().enumerate() {
            slots
                .entry((row.endpoint, row.tab.tab_id.as_str()))
                .or_insert((index, 0));
        }
        listed.sort_by_key(|row| slots[&(row.endpoint, row.tab.tab_id.as_str())]);
    }
    members
}
