//! The shown space's tabs as sidebar rows, one line each: its state, the
//! number cmd-1 to cmd-9 count it by, then its title.

use super::{
    agent_name,
    cell::{AgentRow, Cell, RowContext, RowData, RowLayout},
    label_text,
    tokens::{ResolvedToken, TextRole, TokenKind},
};
use crate::{HerdrWindow, NavigationTarget, config::TokenStyle};
use gpui::{prelude::*, *};
use herdr_client::protocol::{ClientShellAgent, ClientShellSnapshot, ClientShellTab};

impl HerdrWindow {
    pub(super) fn tab_rows(
        &self,
        snapshot: &ClientShellSnapshot,
        endpoint_id: &str,
        rows: &'static dyn RowLayout,
        row_cx: &RowContext<'_>,
        content_x: f32,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let focused_workspace = snapshot.focused_workspace_id.as_deref();
        let listed: Vec<(&ClientShellTab, String)> = snapshot
            .tabs
            .iter()
            .filter(|tab| Some(tab.workspace_id.as_str()) == focused_workspace)
            .map(|tab| (tab, tab_title(tab, snapshot)))
            .collect();
        if listed.is_empty() {
            return vec![
                div()
                    .px(px(content_x))
                    .text_color(rgb(row_cx.theme.muted))
                    .child(label_text("no tabs"))
                    .into_any_element(),
            ];
        }
        listed
            .iter()
            .enumerate()
            .map(|(position, (tab, name))| {
                let agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.tab_id == tab.tab_id);
                let id = tab.tab_id.clone();
                let context_id = id.clone();
                let navigate_endpoint = endpoint_id.to_owned();
                Cell::new(
                    rows,
                    RowData::Agent(AgentRow {
                        key: format!("tab-{id}"),
                        name: name.as_str(),
                        icon: match agent {
                            Some(agent) => {
                                crate::icons::AgentIcon::from_identity(agent.agent.as_deref())
                            }
                            None => crate::icons::AgentIcon::Generic,
                        },
                        status: tab.agent_status,
                        place: None,
                        status_text: None,
                        lines: tab_lines(name, position + 1),
                    }),
                    row_cx,
                )
                .selected(tab.focused)
                .row()
                .id(SharedString::from(format!("tab-{endpoint_id}-{id}")))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_tab_menu(&context_id, None, event.position, window, cx);
                        this.menu.opening_right_click =
                            this.menu.page == Some(crate::menu::Page::Tab);
                    }),
                )
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Tab(&id), cx);
                    window.focus(&this.focus, cx);
                }))
                .into_any_element()
            })
            .collect()
    }
}

/// What a tab is called in the sidebar: its own name when renamed, else its
/// agent's terminal title, else the agent, else the shell's directory.
pub(super) fn tab_title(tab: &ClientShellTab, snapshot: &ClientShellSnapshot) -> String {
    if tab.custom_label {
        return tab.label.clone();
    }
    if let Some(agent) = snapshot
        .agents
        .iter()
        .find(|agent| agent.tab_id == tab.tab_id)
    {
        return agent_title(agent).unwrap_or_else(|| agent_name(agent).to_owned());
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

fn agent_title(agent: &ClientShellAgent) -> Option<String> {
    agent
        .terminal_title_stripped
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(str::to_owned)
}

/// A tab's one sidebar line. The position takes the agent role so the row
/// draws its agent's icon beside it; the title stays in the name color
/// without the bold an agent row would give it.
pub(super) fn tab_lines(title: &str, number: impl ToString) -> Vec<Vec<ResolvedToken>> {
    vec![vec![
        ResolvedToken {
            kind: TokenKind::StateIcon,
            style: TokenStyle::default(),
        },
        ResolvedToken {
            kind: TokenKind::Text(SharedString::from(number.to_string()), TextRole::Agent),
            style: TokenStyle::default(),
        },
        ResolvedToken {
            kind: TokenKind::Text(SharedString::from(title.to_owned()), TextRole::Workspace),
            style: TokenStyle {
                bold: Some(false),
                ..TokenStyle::default()
            },
        },
    ]]
}
