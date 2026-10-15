//! The tabs list's rows, one line each: a tab's state, the number cmd-1 to
//! cmd-9 count it by, then its title, with a close button at the end on the
//! selected device's own tabs. The view in the header chooses which tabs
//! are listed.

use super::{
    agent_name,
    cell::{AgentRow, Cell, RowContext, RowData, RowLayout},
    label_text,
    tokens::{ResolvedToken, TextRole, TokenKind},
    trailing::Trailing,
};
use crate::{
    HerdrWindow, NavigationTarget, config::TokenStyle, preferences::TabsView, reorder::preview,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{ClientShellAgent, ClientShellSnapshot, ClientShellTab};
use std::time::Instant;

mod views;
pub(crate) use views::ListedTab;

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

/// The selected device's listed tabs: the focused space's, in the daemon's
/// order.
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
        rows: &'static dyn RowLayout,
        row_cx: &RowContext<'_>,
        content_x: f32,
        cx: &mut Context<Self>,
    ) -> TabRows {
        let listed: Vec<(ListedTab<'_>, String)> = self
            .listed_tabs()
            .into_iter()
            .map(|row| {
                let name = tab_title(row.tab, row.snapshot);
                (row, name)
            })
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
        let multi = self.endpoints.len() > 1;
        let view = self.sidebar_tabs_view;
        let scroll = &self.sidebar_scroll[2];
        // A reorder is a position within one space, so the views across devices do not drag.
        let drag = self
            .tab_row_drag
            .as_ref()
            .filter(|_| view == TabsView::Device)
            .filter(|drag| drag.previewing(listed.iter().map(|(row, _)| row.tab.tab_id.as_str())))
            .and_then(|drag| {
                let dragged = listed
                    .iter()
                    .position(|(row, _)| row.tab.tab_id == drag.key)?;
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
        let mut elements = Vec::with_capacity(listed.len());
        let mut group = None;
        for (position, (row, name)) in listed.iter().enumerate() {
            let endpoint = &self.endpoints[row.endpoint];
            let own = row.endpoint == self.selected_endpoint;
            if view == TabsView::ByDevice && multi && group != Some(row.endpoint) {
                group = Some(row.endpoint);
                elements.push(tab_group_line(
                    &endpoint.id,
                    &endpoint.label,
                    content_x,
                    row_cx,
                ));
            }
            let device = (view == TabsView::All && multi).then_some(endpoint.label.as_str());
            let agent = row
                .snapshot
                .agents
                .iter()
                .find(|agent| agent.tab_id == row.tab.tab_id);
            let id = row.tab.tab_id.clone();
            let endpoint_id = endpoint.id.clone();
            let context_id = id.clone();
            let close_id = id.clone();
            let press_id = id.clone();
            let navigate_endpoint = endpoint_id.clone();
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
                    key: if own {
                        format!("tab-{id}")
                    } else {
                        format!("tab-{endpoint_id}-{id}")
                    },
                    name: name.as_str(),
                    icon: match agent {
                        Some(agent) => {
                            crate::icons::AgentIcon::from_identity(agent.agent.as_deref())
                        }
                        None => crate::icons::AgentIcon::Generic,
                    },
                    status: row.tab.agent_status,
                    place: None,
                    status_text: None,
                    lines: tab_lines(name, position + 1, device),
                }),
                &tab_cx,
            )
            .selected(own && row.tab.focused)
            .row()
            .when(own, |row| {
                row.child(trailing.place(self.tab_close_button(&close_id, cx)))
            })
            .id(SharedString::from(format!("tab-{endpoint_id}-{id}")))
            .when(own, |row| {
                row.on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        this.open_tab_menu(&context_id, None, event.position, window, cx);
                        this.menu.opening_right_click =
                            this.menu.page == Some(crate::menu::Page::Tab);
                    }),
                )
            })
            .on_click(cx.listener(move |this, _, window, cx| {
                this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Tab(&id), cx);
                window.focus(&this.focus, cx);
            }))
            .when(own && view == TabsView::Device, |row| {
                row.on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                        if event.click_count == 1 {
                            this.press_tab_row(&press_id, event.position, cx);
                        }
                    }),
                )
            })
            .when(carried, |row| row.cursor_grabbing())
            .when(shift != px(0.), |row| row.top(shift))
            .into_any_element();
            if carried {
                // Painted last so it floats over the rows it passes, while
                // its layout slot keeps the others' positions stable.
                elements.push(deferred(element).with_priority(1).into_any_element());
            } else {
                elements.push(element);
            }
        }
        TabRows {
            rows: elements,
            drop: drag.map(|(_, dragged)| (drop, dragged)),
            sliding,
        }
    }

    /// The tabs header's view label, absent with one device alone.
    pub(super) fn tabs_view_toggle(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.endpoints.len() < 2 {
            return None;
        }
        let theme = &self.theme;
        let toggle = div()
            .id("tabs-view")
            .debug_selector(|| "tabs-view".into())
            .flex_none()
            .min_w_0()
            .truncate()
            .text_color(rgb(theme.muted))
            .cursor_pointer()
            .hover(|style| style.text_color(rgb(theme.foreground)))
            .on_click(cx.listener(|this, _, _, cx| {
                cx.stop_propagation();
                this.sidebar_tabs_view = this.sidebar_tabs_view.next();
                this.sidebar_sections_modified = true;
                this.save_chrome();
                cx.notify();
            }))
            .child(label_text(self.sidebar_tabs_view.name()));
        Some(toggle.into_any_element())
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

fn tab_group_line(
    endpoint_id: &str,
    label: &str,
    content_x: f32,
    row_cx: &RowContext<'_>,
) -> AnyElement {
    let selector = format!("tabs-group-{endpoint_id}");
    div()
        .debug_selector(move || selector.clone())
        .flex_none()
        .px(px(content_x))
        .pt(px(row_cx.look.density.gap()))
        .text_color(rgb(row_cx.theme.muted))
        .child(label_text(label))
        .into_any_element()
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
/// without the bold an agent row would give it. A device name, in the view
/// listing every device's tabs together, sits muted before the title.
pub(super) fn tab_lines(
    title: &str,
    number: impl ToString,
    device: Option<&str>,
) -> Vec<Vec<ResolvedToken>> {
    let mut line = vec![
        ResolvedToken {
            kind: TokenKind::StateIcon,
            style: TokenStyle::default(),
        },
        ResolvedToken {
            kind: TokenKind::Text(SharedString::from(number.to_string()), TextRole::Agent),
            style: TokenStyle::default(),
        },
    ];
    if let Some(device) = device {
        line.push(ResolvedToken {
            kind: TokenKind::Text(SharedString::from(device.to_owned()), TextRole::Muted),
            style: TokenStyle::default(),
        });
    }
    line.push(ResolvedToken {
        kind: TokenKind::Text(SharedString::from(title.to_owned()), TextRole::Workspace),
        style: TokenStyle {
            bold: Some(false),
            ..TokenStyle::default()
        },
    });
    vec![line]
}
