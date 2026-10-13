//! The shown space's tabs as sidebar rows, one line each: its state, the
//! number cmd-1 to cmd-9 count it by, then its title, with a close button at
//! the end.

use super::{
    agent_name,
    cell::{AgentRow, Cell, RowContext, RowData, RowLayout},
    label_text,
    tokens::{ResolvedToken, TextRole, TokenKind},
    trailing::Trailing,
};
use crate::{HerdrWindow, NavigationTarget, config::TokenStyle, reorder::preview};
use gpui::{prelude::*, *};
use herdr_client::protocol::{ClientShellAgent, ClientShellSnapshot, ClientShellTab};
use std::time::Instant;

/// Each listed row as its own drop unit, its child index and painted shift.
pub(super) type DropRows = Vec<(usize, usize, Pixels)>;

/// The tabs list's rows, and what a drag among them needs from this frame.
pub(super) struct TabRows {
    pub(super) rows: Vec<AnyElement>,
    /// While a drag previews, the rows and the carried row's index.
    pub(super) drop: Option<(DropRows, usize)>,
    /// Whether a row is still sliding, so the next frame is drawn.
    pub(super) sliding: bool,
}

/// The tabs the list shows: the focused space's, in the daemon's order.
pub(super) fn shown_tabs(snapshot: &ClientShellSnapshot) -> Vec<&ClientShellTab> {
    let focused = snapshot.focused_workspace_id.as_deref();
    snapshot
        .tabs
        .iter()
        .filter(|tab| Some(tab.workspace_id.as_str()) == focused)
        .collect()
}

impl HerdrWindow {
    pub(super) fn tab_rows(
        &self,
        snapshot: &ClientShellSnapshot,
        endpoint_id: &str,
        rows: &'static dyn RowLayout,
        row_cx: &RowContext<'_>,
        content_x: f32,
        cx: &mut Context<Self>,
    ) -> TabRows {
        let listed: Vec<(&ClientShellTab, String)> = shown_tabs(snapshot)
            .into_iter()
            .map(|tab| (tab, tab_title(tab, snapshot)))
            .collect();
        if listed.is_empty() {
            return TabRows {
                rows: vec![
                    div()
                        .px(px(content_x))
                        .text_color(rgb(row_cx.theme.muted))
                        .child(label_text("no tabs"))
                        .into_any_element(),
                ],
                drop: None,
                sliding: false,
            };
        }
        let scroll = &self.sidebar_scroll[2];
        let drag = self
            .tab_row_drag
            .as_ref()
            .filter(|drag| drag.previewing(listed.iter().map(|(tab, _)| tab.tab_id.as_str())))
            .and_then(|drag| {
                let dragged = listed.iter().position(|(tab, _)| tab.tab_id == drag.key)?;
                Some((drag, dragged))
            });
        let floating = drag.is_some_and(|(drag, _)| drag.floating());
        // Each row's shift for the drop in preview, from the heights the last
        // frame laid out: shifting moves rows, never resizes them.
        let shifts = drag.map_or_else(Vec::new, |(drag, dragged)| {
            let heights: Vec<f32> = (0..listed.len())
                .map(|row| {
                    scroll
                        .bounds_for_item(row)
                        .map_or(0., |bounds| f32::from(bounds.size.height))
                })
                .collect();
            preview(&heights, dragged, drag.target.unwrap_or(dragged))
        });
        let now = Instant::now();
        let mut sliding = false;
        let mut drop = Vec::new();
        let trailing = Trailing::new(row_cx);
        let tab_cx = trailing.context(row_cx);
        let rows = listed
            .iter()
            .enumerate()
            .map(|(position, (tab, name))| {
                let agent = snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.tab_id == tab.tab_id);
                let id = tab.tab_id.clone();
                let context_id = id.clone();
                let close_id = id.clone();
                let press_id = id.clone();
                let navigate_endpoint = endpoint_id.to_owned();
                let carried = floating && drag.is_some_and(|(_, dragged)| dragged == position);
                let shift = match drag {
                    Some((drag, _)) if carried => {
                        drag.pin(&id, f32::from(drag.offset()), now);
                        drag.offset()
                    }
                    Some((drag, _)) => {
                        let (at, moving) = drag.slide(&id, shifts[position], now);
                        sliding |= moving;
                        px(at)
                    }
                    None => px(0.),
                };
                if drag.is_some() {
                    drop.push((position, position, shift));
                }
                let element = Cell::new(
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
                    &tab_cx,
                )
                .selected(tab.focused)
                .row()
                .child(trailing.place(self.tab_close_button(&close_id, cx)))
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        if event.click_count == 1 {
                            this.press_tab_row(&press_id, event.position, cx);
                        }
                    }),
                )
                .when(carried, |row| row.cursor_grabbing())
                .when(shift != px(0.), |row| row.top(shift))
                .into_any_element();
                if carried {
                    // Painted last so it floats over the rows it passes, while
                    // its layout slot keeps the others' positions stable.
                    deferred(element).with_priority(1).into_any_element()
                } else {
                    element
                }
            })
            .collect();
        TabRows {
            rows,
            drop: drag.map(|(_, dragged)| (drop, dragged)),
            sliding,
        }
    }

    /// The x at the end of a tab's row. It closes the tab the way the row's
    /// menu does, and a press on it stays here rather than focusing the tab.
    fn tab_close_button(&self, id: &str, cx: &mut Context<Self>) -> Stateful<Div> {
        let (muted, foreground) = (self.theme.muted, self.theme.foreground);
        let selector = format!("tab-close-{id}");
        let close_id = id.to_owned();
        div()
            .id(SharedString::from(selector.clone()))
            .debug_selector(move || selector.clone())
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::SMALL))
            .hover(move |button| button.bg(rgba((foreground << 8) | 0x24)))
            .child(
                svg()
                    .path("icons/close.svg")
                    .size(px(10.))
                    .text_color(rgb(muted)),
            )
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_tab_close(&close_id, window, cx);
            }))
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
