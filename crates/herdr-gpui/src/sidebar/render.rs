//! Laying out the sidebar: the two lists, their headings, and the drag handle
//! that resizes the panel. Render works from prepared state and the bounded
//! caches only.

use super::{
    HOST_ARROW_WIDTH, HOST_GAP, STATUS_WIDTH, SidebarDrag, agent_name,
    agents::{Indicators, agent_place, state_label, status_text},
    agents_sort,
    cell::{AgentRow, Cell, Fold, RowContext, RowData, RowState, WorkspaceRow, layout_for},
    label_text, layout, line_height,
    reorder::{self, Plan},
    row::{RowIcon, RowLift, RowTree, removing_dot},
    sections::header_height,
    sidebar_width, sorted_agents,
    tokens::{self, SpaceContext},
    trailing::Trailing,
    visible_workspace_entries,
    workspaces::{displayed_workspace_status, workspace_badge, workspace_label},
};
use crate::{HerdrWindow, NavigationTarget, fonts::StyledFont, preferences::Section};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn render_sidebar(
        &self,
        indicators: Indicators,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let width = sidebar_width(self.sidebar_width, f32::from(window.viewport_size().width));
        let split = self.sidebar_split.unwrap_or(0.5).clamp(0.1, 0.9);
        let look = layout::for_mode(self.config.layout.mode);
        let rows = layout_for(self.config.layout.mode);
        // The row a workspace menu was opened for keeps looking hovered while
        // the pointer is over the menu.
        let menu_target = self.workspace_menu_target();
        let layout = look.density;
        let content_x = look.content_x();
        // The label yields room to the arrow and to the trailing status dot.
        let host_label_width =
            (look.content_width(width) - HOST_ARROW_WIDTH - 2. * HOST_GAP - STATUS_WIDTH).max(0.);
        let view = cx.entity().downgrade();
        let font = &self.config.sidebar;
        let agents_custom = self.config.usage.inline
            && self.config.sidebar_layout.agents != crate::config::AgentLayout::default();
        let spaces_custom = self.config.usage.inline
            && self.config.sidebar_layout.spaces != crate::config::SpaceLayout::default();
        let theme = &self.theme;
        let sidebar_tabs = self.config.show_sidebar_tabs;
        // Under a tabs list the spaces are an index: one line each.
        let spaces_look = if sidebar_tabs {
            layout::for_spaces(self.config.layout.mode)
        } else {
            look
        };
        let agents_shown = self.config.show_agents;
        let spaces_open = self.section_open(Section::Spaces);
        let tabs_open = self.section_open(Section::Tabs);
        let agents_open = self.section_open(Section::Agents);
        // Grow factors below one leave free space unclaimed, so a list whose
        // partner is folded or hidden takes the whole share.
        let above_open = if sidebar_tabs { tabs_open } else { spaces_open };
        let above_grow = if agents_shown && agents_open {
            split
        } else {
            1.
        };
        let agents_grow = if above_open { 1. - split } else { 1. };
        let header_h = header_height(font, look);
        // With a tabs list below them, spaces take what they need, up to a
        // share of the window, and the tabs list grows instead.
        let spaces_max_h = f32::from(window.viewport_size().height) * 0.4;
        let spaces_height = self.sidebar_spaces_height;
        let mut spaces = div()
            .id("spaces-scroll")
            .debug_selector(|| "spaces-scroll".into())
            .flex()
            .flex_col()
            .map(|list| match (sidebar_tabs, spaces_height) {
                (true, Some(height)) => list.flex_none().h(px(height)),
                (true, None) => list.flex_none().max_h(px(spaces_max_h)),
                (false, _) => list.flex_1().min_h_0(),
            })
            .overflow_y_scroll();
        let mut tabs = div()
            .id("tabs-scroll")
            .debug_selector(|| "tabs-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        let mut agents = div()
            .id("agents-scroll")
            .debug_selector(|| "agents-scroll".into())
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .overflow_y_scroll();
        spaces = spaces.track_scroll(&self.sidebar_scroll[0]);
        agents = agents.track_scroll(&self.sidebar_scroll[1]);
        tabs = tabs.track_scroll(&self.sidebar_scroll[2]);
        let multi = self.endpoints.len() > 1;
        let mut agent_count = 0;
        // A plugin view hid every agent, rather than there being none.
        let mut filtered = false;
        // Child positions of the highlighted rows, for the one-time reveal below.
        // Agent rows are counted by `agent_count`, which indexes that list.
        let mut space_rows = 0usize;
        let mut highlighted = [None; 2];
        // A lifted workspace row: each drop unit's rows in the spaces list, and
        // the move every gap makes, for the pointer handler below.
        let mut drop_rows = Vec::new();
        let mut drop_requests = Vec::new();
        let mut drop_dragged = 0;
        let now = std::time::Instant::now();
        let mut sliding = false;
        for (endpoint_index, endpoint) in self.endpoints.iter().enumerate() {
            if !self.device_visible(&endpoint.id) {
                continue;
            }
            let selected = endpoint_index == self.selected_endpoint;
            let endpoint_id = endpoint.id.clone();
            if multi {
                let collapse_id = endpoint_id.clone();
                let select_id = endpoint_id.clone();
                let menu_id = endpoint_id.clone();
                let removing = self.menu.removing_devices.contains(&endpoint.id);
                let host = crate::usage::Host::from(&endpoint.connection.target);
                let load = self
                    .config
                    .show_system_load
                    .then(|| self.system_load.get(&host))
                    .flatten();
                // Densities with detail lines give the load its own line;
                // compact ones fit gauges between the name and the status.
                let load_line = load.filter(|_| layout.workspace_details());
                let gauges = load.filter(|_| load_line.is_none()).map(|reading| {
                    crate::system_load::gauges(
                        reading,
                        theme,
                        super::metrics::glyph_width(font),
                        (font.size * 0.8).round(),
                    )
                });
                // The removal pulse and the gauges take their room from the
                // label, not from the status.
                let label_width = (host_label_width
                    - if removing {
                        STATUS_WIDTH + HOST_GAP
                    } else {
                        0.
                    }
                    - gauges.as_ref().map_or(0., |(width, _)| width + HOST_GAP))
                .max(0.);
                let lines = 1. + if load_line.is_some() { 1. } else { 0. };
                spaces = spaces.child(
                    div()
                        .id(SharedString::from(format!("host-{endpoint_id}")))
                        .debug_selector(|| format!("host-{endpoint_id}"))
                        .h(px(lines * line_height(font)
                            + 2. * layout.host_padding()
                            + look.chrome_height()))
                        .flex_none()
                        .relative()
                        .flex()
                        .flex_col()
                        .justify_center()
                        .px(px(content_x))
                        // Hosts mark selection only; they do not join the rows'
                        // hover group.
                        .child(look.highlight(
                            &format!("host-{endpoint_id}"),
                            RowState {
                                selected,
                                ..RowState::default()
                            },
                            theme,
                        ))
                        .text_color(rgb(if endpoint.enabled {
                            theme.foreground
                        } else {
                            theme.muted
                        }))
                        .cursor_pointer()
                        .on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                                cx.stop_propagation();
                                this.open_host_menu(&menu_id, event.position, window, cx);
                                this.menu.opening_right_click =
                                    this.menu.page == Some(crate::menu::Page::Host);
                            }),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(HOST_GAP))
                                .child(
                                    div()
                                        .id(SharedString::from(format!(
                                            "collapse-host-{endpoint_id}"
                                        )))
                                        .w(px(HOST_ARROW_WIDTH))
                                        .flex_none()
                                        .child(label_text(if endpoint.collapsed {
                                            "\u{25b8}"
                                        } else {
                                            "\u{25be}"
                                        }))
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            if let Some(endpoint) = this
                                                .endpoints
                                                .iter_mut()
                                                .find(|e| e.id == collapse_id)
                                            {
                                                endpoint.collapsed = !endpoint.collapsed;
                                            }
                                            cx.notify();
                                        })),
                                )
                                .when(removing, |row| {
                                    row.child(removing_dot("host-removing", theme))
                                })
                                .child(
                                    div()
                                        // As with workspace labels, avoid zero-basis text measurement.
                                        .w(px(label_width))
                                        .flex_none()
                                        .overflow_hidden()
                                        .child(
                                            div()
                                                .w(px(label_width))
                                                .truncate()
                                                .child(label_text(&endpoint.label)),
                                        ),
                                )
                                .when_some(gauges.zip(load), |row, ((_, gauges), reading)| {
                                    row.child(
                                        div()
                                            .id(SharedString::from(format!(
                                                "host-load-{endpoint_id}"
                                            )))
                                            .flex_none()
                                            .child(gauges)
                                            .tooltip(crate::system_load::tooltip(
                                                reading, &host, theme,
                                            )),
                                    )
                                })
                                .child(
                                    div()
                                        .debug_selector(|| format!("host-status-{endpoint_id}"))
                                        .size(px(STATUS_WIDTH))
                                        .flex_none()
                                        .rounded_full()
                                        .bg(rgb(if endpoint.live.status.is_connected() {
                                            crate::menu::online(theme)
                                        } else {
                                            theme.muted
                                        })),
                                ),
                        )
                        .when_some(load_line, |row, reading| {
                            row.child(
                                div()
                                    .id(SharedString::from(format!("host-load-{endpoint_id}")))
                                    .h(px(line_height(font)))
                                    .flex()
                                    .items_center()
                                    .pl(px(HOST_ARROW_WIDTH + HOST_GAP))
                                    .overflow_hidden()
                                    .child(crate::system_load::line(
                                        reading,
                                        theme,
                                        Some(super::metrics::glyph_width(font)),
                                    ))
                                    .tooltip(crate::system_load::tooltip(reading, &host, theme)),
                            )
                        })
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.select_endpoint(&select_id, cx);
                            window.focus(&this.focus, cx);
                        })),
                );
                space_rows += 1;
            }
            let row_cx = RowContext {
                indicators,
                font,
                theme,
                look,
                width,
                host: (multi && endpoint_id != crate::endpoint::LOCAL)
                    .then_some(endpoint.label.as_str()),
            };
            let spaces_cx = RowContext {
                look: spaces_look,
                ..row_cx
            };
            let daemon = crate::listening_ports::Daemon::from(&endpoint.connection.target);
            let live = if selected { &self.live } else { &endpoint.live };
            let Some(snapshot) = &live.snapshot else {
                continue;
            };
            let collapsed_repos = if endpoint_index == 0 {
                &self.collapsed_repos
            } else {
                &endpoint.collapsed_repos
            };
            let entries = visible_workspace_entries(&snapshot.workspaces, collapsed_repos);
            let drag = self.workspace_drag.as_ref().filter(|drag| {
                selected
                    && drag.previewing(
                        snapshot
                            .workspaces
                            .iter()
                            .map(|workspace| workspace.workspace_id.as_str()),
                    )
            });
            let floating = drag.is_some_and(|drag| drag.floating());
            let plan = drag.and_then(|drag| Plan::new(&snapshot.workspaces, &drag.key));
            // Each unit's shift for the drop in preview, from the heights the
            // last frame laid out: shifting moves rows, never resizes them.
            let base = space_rows;
            let shifts = plan.as_ref().map_or_else(Vec::new, |plan| {
                drop_requests = (0..=plan.len())
                    .map(|slot| plan.request(&snapshot.workspaces, slot))
                    .collect();
                drop_dragged = plan.dragged();
                let mut heights = vec![0.; plan.len()];
                for (position, entry) in entries.iter().enumerate() {
                    if let (Some(unit), Some(bounds)) = (
                        plan.unit_of(entry.0),
                        self.sidebar_scroll[0].bounds_for_item(base + position),
                    ) {
                        heights[unit] += f32::from(bounds.size.height);
                        if spaces_custom && !entry.1 {
                            heights[unit] += f32::from(self.config.sidebar_layout.spaces.row_gap)
                                * line_height(font);
                        }
                    }
                }
                let slot = drag
                    .and_then(|drag| drag.target.as_ref())
                    .map_or(plan.dragged(), |target| target.slot);
                crate::reorder::preview(&heights, plan.dragged(), slot)
            });
            // A child closes the group when no child follows it.
            let closes: Vec<bool> = (0..entries.len())
                .map(|position| {
                    entries[position].1 && !entries.get(position + 1).is_some_and(|next| next.1)
                })
                .collect();
            // Gap separates one repository from the next, not a parent from
            // the linked worktrees under it. Reset per host.
            let mut previous_indented = None;
            for (position, (index, indented, group)) in entries.into_iter().enumerate() {
                if multi && endpoint.collapsed {
                    break;
                }
                let workspace = &snapshot.workspaces[index];
                if selected && workspace.focused {
                    highlighted[0] = Some(space_rows);
                }
                let unit = plan.as_ref().and_then(|plan| plan.unit_of(index));
                // The lifted row and the rows it carries, such as its group's
                // children, follow the pointer together while it floats.
                let carried = unit.is_some_and(|unit| unit == drop_dragged) && floating;
                let shift = match (drag, unit) {
                    (Some(drag), Some(_)) if carried => {
                        drag.pin(&workspace.workspace_id, f32::from(drag.offset()), now);
                        drag.offset()
                    }
                    (Some(drag), Some(unit)) => {
                        let (at, moving) = drag.slide(&workspace.workspace_id, shifts[unit], now);
                        sliding |= moving;
                        px(at)
                    }
                    _ => px(0.),
                };
                if let Some(unit) = unit {
                    drop_rows.push((unit, space_rows, shift));
                }
                space_rows += 1;
                let id = workspace.workspace_id.clone();
                let press_id = id.clone();
                let context_id = id.clone();
                let hover_id = id.clone();
                let context_endpoint = endpoint_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                let collapse_endpoint = endpoint_id.clone();
                let grouped = group.is_some() || indented;
                let tree = match (indented, closes[position]) {
                    (false, _) => RowTree::None,
                    (true, false) => RowTree::Child,
                    (true, true) => RowTree::LastChild,
                };
                let fold = group.map(|key| Fold {
                    id: SharedString::from(format!("collapse-{endpoint_id}-{id}")).into(),
                    index,
                    collapsed: collapsed_repos.contains(&key),
                    toggle: Box::new(cx.listener(move |this, _, _, cx| {
                        let collapsed = if collapse_endpoint == crate::endpoint::LOCAL {
                            &mut this.collapsed_repos
                        } else if let Some(endpoint) = this
                            .endpoints
                            .iter_mut()
                            .find(|e| e.id == collapse_endpoint)
                        {
                            &mut endpoint.collapsed_repos
                        } else {
                            return;
                        };
                        if !collapsed.remove(&key) {
                            collapsed.insert(key.clone());
                        }
                        cx.notify();
                    })),
                });
                let label = workspace_label(workspace, indented);
                let shown_status = if self.config.usage.inline {
                    displayed_workspace_status(&snapshot.workspaces, workspace, collapsed_repos)
                } else {
                    workspace.agent_status
                };
                let lines = if spaces_custom {
                    tokens::space_rows(
                        &self.config.sidebar_layout.spaces,
                        SpaceContext {
                            label,
                            branch: workspace.branch.as_deref(),
                            status: shown_status,
                            ahead_behind: workspace.git_ahead_behind,
                            tokens: &workspace.tokens,
                            indented,
                        },
                    )
                } else {
                    Vec::new()
                };
                let gap = if spaces_custom {
                    match previous_indented.replace(indented) {
                        Some(_) if !indented => {
                            f32::from(self.config.sidebar_layout.spaces.row_gap) * line_height(font)
                        }
                        _ => 0.,
                    }
                } else {
                    0.
                };
                let removing_row = selected
                    && self.live.status.is_connected()
                    && self.removal.as_ref().is_some_and(|removal| {
                        removal.pending_for(
                            (self.selection_epoch, endpoint.generation),
                            &snapshot.boot_id,
                            &workspace.workspace_id,
                        )
                    });
                // Ports hang behind a globe at the row's end, so a listening
                // workspace keeps the one line every other row has.
                let listed = (self.config.show_listening_ports && !removing_row)
                    .then(|| self.listening_ports.get(&daemon, &workspace.workspace_id))
                    .flatten();
                let trailing = listed.map(|_| Trailing::new(&spaces_cx));
                let workspace_cx = trailing
                    .as_ref()
                    .map_or(spaces_cx, |trailing| trailing.context(&spaces_cx));
                let globe = trailing.as_ref().zip(listed).map(|(trailing, listed)| {
                    let id = (endpoint_id.as_str(), workspace.workspace_id.as_str());
                    self.ports_globe(trailing, id, listed, cx)
                });
                let element = Cell::new(
                    rows,
                    RowData::Workspace(WorkspaceRow {
                        workspace,
                        label,
                        tree,
                        icon: if indented {
                            RowIcon::None
                        } else {
                            self.avatars
                                .as_ref()
                                .filter(|_| endpoint_index == 0)
                                .and_then(|avatars| avatars.image(&workspace.new_workspace_cwd))
                                .map_or(RowIcon::Mark, RowIcon::Avatar)
                        },
                        fold,
                        grouped,
                        badge: workspace_badge(
                            workspace,
                            (endpoint_index == self.selected_endpoint)
                                .then_some(&self.menu.pr_cache),
                            &self.git,
                            (&self.teleport_marks, &endpoint.id),
                            theme,
                        ),
                        removing: removing_row,
                        status: shown_status,
                        lines,
                    }),
                    &workspace_cx,
                )
                .selected(selected && workspace.focused)
                .highlighted(selected && menu_target == Some(id.as_str()))
                .lift(match (carried, floating) {
                    (true, _) => RowLift::Lifted,
                    (false, true) => RowLift::Passed,
                    (false, false) => RowLift::Resting,
                })
                .row()
                .when_some(globe, |row, globe| row.child(globe))
                .on_mouse_down(
                    MouseButton::Right,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        cx.stop_propagation();
                        if this.navigate_endpoint(
                            &context_endpoint,
                            NavigationTarget::Workspace(&context_id),
                            cx,
                        ) {
                            this.open_workspace_menu(&context_id, event.position, window, cx);
                            this.menu.opening_right_click =
                                this.menu.page == Some(crate::menu::Page::Workspace);
                        }
                    }),
                )
                .when(
                    self.menu.page == Some(crate::menu::Page::Workspace),
                    |row| {
                        let view = cx.entity().downgrade();
                        let endpoint = endpoint_id.clone();
                        let workspace = id.clone();
                        row.child(
                            canvas(
                                |_, _, _| (),
                                move |bounds, _, window, _| {
                                    let bounds = bounds.intersect(&window.content_mask().bounds);
                                    window.on_mouse_event(
                                        move |event: &MouseDownEvent, phase, window, cx| {
                                            // The overlay dismisses first in bubble order. Use
                                            // clipped row geometry because it occludes our hitbox.
                                            if phase == DispatchPhase::Bubble
                                                && event.button == MouseButton::Right
                                                && bounds.contains(&event.position)
                                            {
                                                let _ = view.update(cx, |this, cx| {
                                                    cx.stop_propagation();
                                                    if this.navigate_endpoint(
                                                        &endpoint,
                                                        NavigationTarget::Workspace(&workspace),
                                                        cx,
                                                    ) {
                                                        this.open_workspace_menu(
                                                            &workspace,
                                                            event.position,
                                                            window,
                                                            cx,
                                                        );
                                                        this.menu.opening_right_click = this
                                                            .menu
                                                            .page
                                                            == Some(crate::menu::Page::Workspace);
                                                    }
                                                });
                                            }
                                        },
                                    );
                                },
                            )
                            .absolute()
                            .inset_0()
                            .size_full(),
                        )
                    },
                )
                .id(SharedString::from(format!("workspace-{endpoint_id}-{id}")))
                .when(multi, |row| {
                    row.debug_selector(|| format!("workspace-{endpoint_id}-{id}"))
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.navigate_endpoint(
                        &navigate_endpoint,
                        NavigationTarget::Workspace(&id),
                        cx,
                    );
                    window.focus(&this.focus, cx);
                }))
                // Only the selected endpoint's rows arm the hover menu:
                // another endpoint's menu would have to select it first, and
                // resting the pointer must not switch which daemon is shown.
                // The feature is opt-in, so rows stay unarmed without it.
                .when(selected && self.config.features.sidebar_hover_menu, |row| {
                    row.on_hover(cx.listener(move |this, hovered: &bool, window, _| {
                        this.hover_workspace(&hover_id, *hovered, window);
                    }))
                })
                // Holding a press lifts the row for reordering. Another
                // endpoint's rows would have to select it first.
                .when(selected, |row| {
                    row.on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            if event.click_count == 1 {
                                this.press_workspace(&press_id, event.position, cx);
                            }
                        }),
                    )
                })
                .when(carried, |row| row.cursor_grabbing())
                .when(gap > 0., |row| row.mt(px(gap)))
                .when(shift != px(0.), |row| row.top(shift))
                .into_any_element();
                spaces = if carried {
                    // Painted last so it floats over the rows it passes, while
                    // its layout slot keeps the others' positions stable.
                    spaces.child(deferred(element).with_priority(1))
                } else {
                    spaces.child(element)
                };
            }
            if sidebar_tabs && selected && tabs_open {
                tabs = tabs.children(self.tab_rows(
                    snapshot,
                    &endpoint_id,
                    rows,
                    &row_cx,
                    content_x,
                    cx,
                ));
            }
            if !self.config.show_agents {
                continue;
            }
            filtered |= snapshot.agent_view_label.is_some();
            for agent in sorted_agents(snapshot, self.agent_sort) {
                let lines = if agents_custom {
                    let Some(lines) = tokens::agent_rows(
                        &self.config.sidebar_layout.agents,
                        agent,
                        snapshot,
                        row_cx.host,
                    ) else {
                        continue;
                    };
                    lines
                } else {
                    Vec::new()
                };
                if selected && agent.focused {
                    highlighted[1] = Some(agent_count);
                }
                let gap = if agents_custom && agent_count > 0 {
                    f32::from(self.config.sidebar_layout.agents.row_gap) * line_height(font)
                } else {
                    0.
                };
                agent_count += 1;
                let id = agent.pane_id.clone();
                let navigate_endpoint = endpoint_id.clone();
                agents = agents.child(
                    Cell::new(
                        rows,
                        RowData::Agent(AgentRow {
                            key: format!("agent-{id}"),
                            name: agent_name(agent),
                            icon: crate::icons::AgentIcon::from_identity(agent.agent.as_deref()),
                            status: agent.agent_status,
                            place: agent_place(agent, snapshot),
                            status_text: self
                                .config
                                .sidebar_layout
                                .agents
                                .shows_status_text(agent.agent.as_deref())
                                .then(|| state_label(agent, status_text(agent.agent_status))),
                            lines,
                        }),
                        &row_cx,
                    )
                    .selected(selected && agent.focused)
                    .row()
                    .when(gap > 0., |row| row.mt(px(gap)))
                    .id(SharedString::from(format!("agent-{endpoint_id}-{id}")))
                    .when(multi, |row| {
                        row.debug_selector(|| format!("agent-{endpoint_id}-{id}"))
                    })
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.navigate_endpoint(&navigate_endpoint, NavigationTarget::Pane(&id), cx);
                        window.focus(&this.focus, cx);
                    })),
                );
            }
        }
        if sliding {
            window.request_animation_frame();
        }
        // Follow the selection, but only once a frame has measured the viewport:
        // the handle resolves the request against the previous frame's bounds, so
        // an unmeasured list would scroll to a meaningless offset. Recording what
        // was revealed keeps later frames from undoing the user's own scrolling.
        for (list, row) in highlighted.iter().enumerate() {
            let Some(row) = *row else { continue };
            if self.sidebar_revealed[list].get() != Some(row)
                && self.sidebar_scroll[list].bounds().size.height > px(0.)
            {
                let scroll = &self.sidebar_scroll[list];
                let visible = scroll.bounds_for_item(row).is_some_and(|bounds| {
                    let offset = scroll.offset().y;
                    bounds.bottom() + offset > scroll.bounds().top()
                        && bounds.top() + offset < scroll.bounds().bottom()
                });
                // Even a partially visible worktree is already seen. GPUI's
                // reveal also moves clipped rows, so only request it off-screen.
                if !visible {
                    scroll.scroll_to_item(row);
                }
                self.sidebar_revealed[list].set(Some(row));
            }
        }
        if agent_count == 0 {
            agents = agents.child(
                div()
                    .px(px(content_x))
                    .text_color(rgb(theme.muted))
                    .truncate()
                    .child(label_text(if filtered {
                        "no matching agents"
                    } else {
                        "no agents"
                    })),
            );
        }
        let footer = self.spaces_footer(font, look, content_x, cx);
        let top_controls = self.top_controls(cx);
        div()
            .id("sidebar")
            .debug_selector(|| "sidebar".into())
            .relative()
            .w(px(width))
            .flex_none()
            .h_full()
            .min_h_0()
            .overflow_hidden()
            .flex()
            .flex_col()
            .text_font(font)
            .text_size(px(font.size))
            .line_height(px(line_height(font)))
            .text_color(rgb(theme.foreground))
            .bg(rgb(theme.sidebar_background()))
            .border_r_1()
            .border_color(rgb(theme.active))
            // Zero flex bases keep long workspace lists from displacing agents.
            .child(
                div()
                    .debug_selector(|| "spaces-section".into())
                    .flex()
                    .flex_col()
                    .map(|section| {
                        if sidebar_tabs || !spaces_open {
                            section.flex_none()
                        } else {
                            section
                                .flex_1()
                                .map(|mut section| {
                                    section.style().flex_grow = Some(above_grow);
                                    section
                                })
                                .min_h_0()
                        }
                    })
                    .overflow_hidden()
                    .child(self.section_header(
                        Section::Spaces,
                        sidebar_tabs.then(|| top_controls.into_any_element()),
                        font,
                        theme,
                        look,
                        cx,
                    ))
                    .when(spaces_open, |section| section.child(spaces))
                    .when(!sidebar_tabs, |section| section.child(footer)),
            )
            .when(
                sidebar_tabs && spaces_open && (tabs_open || agents_shown && agents_open),
                |sidebar| {
                    sidebar.child(self.section_handle(
                        SidebarDrag::Spaces,
                        "sidebar-spaces-resize",
                        theme,
                        cx,
                    ))
                },
            )
            .when(sidebar_tabs, |sidebar| {
                sidebar.child(
                    div()
                        .debug_selector(|| "tabs-section".into())
                        .flex()
                        .flex_col()
                        .map(|section| {
                            if tabs_open {
                                section
                                    .flex_1()
                                    .map(|mut section| {
                                        section.style().flex_grow = Some(above_grow);
                                        section
                                    })
                                    .min_h_0()
                            } else {
                                section.flex_none()
                            }
                        })
                        .overflow_hidden()
                        .child(self.section_header(Section::Tabs, None, font, theme, look, cx))
                        .when(tabs_open, |section| section.child(tabs)),
                )
            })
            .when(agents_shown, |sidebar| {
                sidebar
                    .when(above_open && agents_open, |sidebar| {
                        sidebar.child(self.section_handle(
                            SidebarDrag::Split,
                            "sidebar-split-resize",
                            theme,
                            cx,
                        ))
                    })
                    .child(
                        div()
                            .debug_selector(|| "agents-section".into())
                            .flex()
                            .flex_col()
                            .map(|section| {
                                if agents_open {
                                    section
                                        .flex_1()
                                        .map(|mut section| {
                                            section.style().flex_grow = Some(agents_grow);
                                            section
                                        })
                                        .min_h_0()
                                } else {
                                    section.flex_none()
                                }
                            })
                            .overflow_hidden()
                            .child(self.section_header(
                                Section::Agents,
                                Some(agents_sort(self, cx).into_any_element()),
                                font,
                                theme,
                                look,
                                cx,
                            ))
                            .when(agents_open, |section| section.child(agents)),
                    )
            })
            .child(self.render_device_footer(cx))
            .child(
                div()
                    .id("sidebar-resize")
                    .debug_selector(|| "sidebar-resize".into())
                    .absolute()
                    .right_0()
                    .top_0()
                    .h_full()
                    .w(px(6.))
                    .cursor(CursorStyle::ResizeLeftRight)
                    .hover(|s| s.bg(rgba(crate::panel_resize::RESIZE_HOVER)))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.sidebar_modified = true;
                            if event.click_count == 2 {
                                this.sidebar_drag = None;
                                this.sidebar_width = None;
                                this.save_sidebar_width();
                            } else {
                                this.sidebar_drag = Some(SidebarDrag::Width {
                                    start: f32::from(event.position.x),
                                    width,
                                });
                            }
                            cx.notify();
                        }),
                    ),
            )
            .child(
                canvas(
                    |_, _, _| (),
                    move |bounds, _, window, _| {
                        // Capture globally so dragging continues outside the narrow divider,
                        // and terminal handlers never receive the resize gesture's release.
                        let moving = view.clone();
                        window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                            if phase == DispatchPhase::Capture {
                                let _ = moving.update(cx, |this, cx| {
                                    let scroll = this.sidebar_scroll[0].clone();
                                    if this.move_workspace_drag(
                                        event.position,
                                        event.pressed_button == Some(MouseButton::Left),
                                        |lift| {
                                            reorder::resolve(
                                                &drop_rows,
                                                &drop_requests,
                                                drop_dragged,
                                                &scroll,
                                                lift,
                                            )
                                        },
                                        cx,
                                    ) {
                                        cx.stop_propagation();
                                    } else if let Some(drag) = this.sidebar_drag {
                                        match drag {
                                            SidebarDrag::Width { start, width } => {
                                                this.sidebar_width = Some(sidebar_width(
                                                    Some(
                                                        width + f32::from(event.position.x) - start,
                                                    ),
                                                    f32::from(window.viewport_size().width),
                                                ));
                                            }
                                            SidebarDrag::Split | SidebarDrag::Spaces => this
                                                .drag_section(
                                                    drag,
                                                    f32::from(event.position.y),
                                                    bounds,
                                                    header_h,
                                                ),
                                        }
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                        let released = view.clone();
                        window.on_mouse_event(move |event: &MouseUpEvent, phase, _, cx| {
                            if phase == DispatchPhase::Capture && event.button == MouseButton::Left
                            {
                                let _ = released.update(cx, |this, cx| {
                                    // A lifted row's release is its drop, not a click.
                                    if this.release_workspace_drag(cx) {
                                        cx.stop_propagation();
                                    } else if this.sidebar_drag.take().is_some() {
                                        this.save_chrome();
                                        cx.stop_propagation();
                                        cx.notify();
                                    }
                                });
                            }
                        });
                    },
                )
                .absolute()
                .size_full(),
            )
    }
}
