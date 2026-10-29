//! The tabs list's rows, one line each: a tab's state, the number cmd-1 to
//! cmd-9 count it by, then its title, with a close button at the end on the
//! selected device's own tabs. The header's options menu chooses the devices
//! to list and the order of their tabs.

use super::{
    agent_name,
    cell::{AgentRow, Cell, RowContext, RowData, RowLayout},
    label_text,
    tab_reorder::row_key,
    tokens::{ResolvedToken, TextRole, TokenKind},
    trailing::Trailing,
};
use crate::{
    HerdrWindow, NavigationTarget, config::TokenStyle, preferences::TabsView, reorder::preview,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{ClientShellAgent, ClientShellSnapshot, ClientShellTab};
use std::{ops::Range, time::Instant};

pub(super) mod attention;
mod views;
pub(crate) use views::{ListedTab, workspace_label_by_id};

/// Each listed row as its own drop unit, its child index and painted shift.
pub(super) type DropRows = Vec<(usize, usize, Pixels)>;

/// The tabs list's rows, and what a drag among them needs from this frame.
pub(super) struct TabRows {
    pub(super) rows: Vec<AnyElement>,
    /// While a drag previews, the rows, the carried row's index and the
    /// positions of the rows it may land among.
    pub(super) drop: Option<(DropRows, usize, Range<usize>)>,
    /// Whether a row is still sliding, so the next frame is drawn.
    pub(super) sliding: bool,
}

impl HerdrWindow {
    pub(super) fn tab_rows(
        &self,
        rows: &'static dyn RowLayout,
        row_cx: &RowContext<'_>,
        content_x: f32,
        cx: &mut Context<Self>,
    ) -> TabRows {
        let listed = self.listed_tabs();
        let names: Vec<String> = listed
            .iter()
            .map(|row| tab_title(row.tab, row.snapshot))
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
        let grouped = view == TabsView::ByDevice && multi;
        let scroll = &self.sidebar_scroll[2];
        let keys: Vec<String> = listed
            .iter()
            .map(|row| row_key(&self.endpoints[row.endpoint].id, &row.tab.tab_id))
            .collect();
        let endpoints: Vec<usize> = listed.iter().map(|row| row.endpoint).collect();
        // Each row's child index in the list, past the device lines above it.
        let children: Vec<usize> = {
            let (mut group, mut next) = (None, 0);
            endpoints
                .iter()
                .map(|&endpoint| {
                    if grouped && group != Some(endpoint) {
                        group = Some(endpoint);
                        next += 1;
                    }
                    next += 1;
                    next - 1
                })
                .collect()
        };
        let drag = self.tab_row_drag.as_ref().and_then(|drag| {
            let dragged = keys.iter().position(|key| *key == drag.key)?;
            let among = self.drag_rows(&listed, dragged);
            drag.previewing(keys[among.clone()].iter().map(String::as_str))
                .then_some((drag, dragged, among))
        });
        let floating = drag.as_ref().is_some_and(|(drag, ..)| drag.floating());
        // Each row's shift for the drop in preview, from the heights the last
        // frame laid out: shifting moves rows, never resizes them.
        let shifts = drag.as_ref().map_or_else(Vec::new, |(drag, dragged, _)| {
            let heights: Vec<f32> = children
                .iter()
                .map(|&child| {
                    scroll
                        .bounds_for_item(child)
                        .map_or(0., |bounds| f32::from(bounds.size.height))
                })
                .collect();
            preview(&heights, *dragged, drag.target.unwrap_or(*dragged))
        });
        let now = Instant::now();
        let mut sliding = false;
        let mut drop = Vec::new();
        let trailing = Trailing::new(row_cx);
        let tab_cx = trailing.context(row_cx);
        let mut elements = Vec::with_capacity(listed.len());
        let mut group = None;
        for (position, (row, name)) in listed.iter().zip(&names).enumerate() {
            let endpoint = &self.endpoints[row.endpoint];
            let own = row.endpoint == self.selected_endpoint;
            if grouped && group != Some(row.endpoint) {
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
            let press_endpoint = row.endpoint;
            let navigate_endpoint = endpoint_id.clone();
            let carried = floating
                && drag
                    .as_ref()
                    .is_some_and(|(_, dragged, _)| *dragged == position);
            let shift = match &drag {
                Some((drag, ..)) if carried => {
                    drag.pin(&keys[position], f32::from(drag.offset()), now);
                    drag.offset()
                }
                Some((drag, ..)) => {
                    let (at, moving) = drag.slide(&keys[position], shifts[position], now);
                    sliding |= moving;
                    px(at)
                }
                None => px(0.),
            };
            if drag.is_some() {
                drop.push((position, children[position], shift));
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
                    status: self.mark(row.endpoint, agent, row.tab.agent_status),
                    place: None,
                    status_text: None,
                    lines: tab_lines(name, position + 1, device),
                    icon_line: Some(0),
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    if event.click_count == 1 {
                        this.press_tab_row(press_endpoint, &press_id, event.position, cx);
                    }
                }),
            )
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
            drop: drag.map(|(_, dragged, among)| (drop, dragged, among)),
            sliding,
        }
    }

    pub(super) fn tabs_header_controls(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (foreground, muted, surface) =
            (self.theme.foreground, self.theme.muted, self.theme.surface);
        Some(
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .text_color(rgb(muted))
                .child(
                    div()
                        .id("sidebar-new-tab")
                        .debug_selector(|| "sidebar-new-tab".into())
                        .cursor_pointer()
                        .hover(move |s| s.text_color(rgb(foreground)))
                        .child(label_text("new tab"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            cx.stop_propagation();
                            this.command(crate::Command::Tab, window, cx);
                        })),
                )
                .child(
                    div()
                        .id("tabs-options")
                        .debug_selector(|| "tabs-options".into())
                        .size(px(20.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(crate::config::corners::SMALL))
                        .cursor_pointer()
                        .hover(move |s| s.text_color(rgb(foreground)))
                        .child(
                            svg()
                                .path("icons/settings.svg")
                                .size(px(12.))
                                .text_color(rgb(muted)),
                        )
                        .tooltip(move |_, cx| {
                            cx.new(|_| crate::usage::Hint {
                                text: "Tab order and device grouping".into(),
                                foreground,
                                surface,
                            })
                            .into()
                        })
                        .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                            cx.stop_propagation();
                            this.open_tabs_options(event.position(), window, cx);
                        })),
                )
                .into_any_element(),
        )
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

/// A tab's one sidebar line: the position cmd-`number` counts, in the agent
/// role beside the agent's icon, then the title in the name color without an
/// agent row's bold. Listing every device's tabs together, the device's name
/// takes the number's place, small at the line's end so the titles line up.
pub(super) fn tab_lines(
    title: &str,
    number: impl ToString,
    device: Option<&str>,
) -> Vec<Vec<ResolvedToken>> {
    let plain = |kind| ResolvedToken {
        kind,
        style: TokenStyle::default(),
    };
    let title = ResolvedToken {
        kind: TokenKind::Text(SharedString::from(title.to_owned()), TextRole::Workspace),
        style: TokenStyle {
            bold: Some(false),
            ..TokenStyle::default()
        },
    };
    let line = match device {
        Some(device) => vec![
            plain(TokenKind::StateIcon),
            title,
            plain(TokenKind::Tag(
                SharedString::from(device.to_owned()),
                TextRole::Muted,
            )),
        ],
        None => vec![
            plain(TokenKind::StateIcon),
            plain(TokenKind::Text(
                SharedString::from(number.to_string()),
                TextRole::Agent,
            )),
            title,
        ],
    };
    vec![line]
}
