//! A device's row in the spaces list: its fold arrow, name, load and status.

use super::{
    HOST_ARROW_WIDTH, HOST_GAP, STATUS_WIDTH, cell::RowState, label_text, layout::SidebarLook,
    line_height, row::removing_dot,
};
use crate::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(super) fn host_row(
        &self,
        endpoint_index: usize,
        small: bool,
        look: SidebarLook,
        spaces_look: SidebarLook,
        width: f32,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let endpoint = &self.endpoints[endpoint_index];
        let font = &self.config.sidebar;
        let theme = &self.theme;
        let selected = endpoint_index == self.selected_endpoint;
        let endpoint_id = endpoint.id.clone();
        let collapse_id = endpoint_id.clone();
        let select_id = endpoint_id.clone();
        let menu_id = endpoint_id.clone();
        let small_line = (line_height(font) * 0.8).round();
        // The label yields room to the arrow and to the trailing status dot.
        let host_label_width =
            (look.content_width(width) - HOST_ARROW_WIDTH - 2. * HOST_GAP - STATUS_WIDTH).max(0.);
        let removing = self.menu.removing_devices.contains(&endpoint.id);
        let host = crate::usage::Host::from(&endpoint.connection.target);
        let load = self
            .config
            .show_system_load
            .then(|| self.system_load.get(&host))
            .flatten();
        // The removal pulse and the load take their room from the
        // label, not from the status.
        let name_room = (host_label_width
            - if removing {
                STATUS_WIDTH + HOST_GAP
            } else {
                0.
            }
            - HOST_GAP)
            .max(0.);
        let load_size = (font.size * 0.85).round();
        let load_view = load.map(|reading| {
            crate::system_load::compact(
                reading,
                theme,
                load_size,
                super::metrics::glyph_width(font) * load_size / font.size,
            )
        });
        let label_width =
            (name_room - load_view.as_ref().map_or(-HOST_GAP, |(width, _)| *width)).max(0.);
        div()
            .id(SharedString::from(format!("host-{endpoint_id}")))
            .debug_selector(|| format!("host-{endpoint_id}"))
            .h(px(if small {
                small_line + 2.
            } else {
                line_height(font)
                    + 2. * spaces_look.density.host_padding()
                    + spaces_look.chrome_height()
            }))
            .when(small, |row| {
                row.text_size(px((font.size * 0.85).round()))
                    .line_height(px(small_line))
            })
            .flex_none()
            .relative()
            .flex()
            .flex_col()
            .justify_center()
            .px(px(look.content_x()))
            // Hosts mark selection only; they do not join the rows'
            // hover group.
            .child(spaces_look.highlight(
                &format!("host-{endpoint_id}"),
                RowState {
                    selected,
                    ..RowState::default()
                },
                theme,
            ))
            .text_color(rgb(if endpoint.enabled && !small {
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
                    this.menu.opening_right_click = this.menu.page == Some(crate::menu::Page::Host);
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(HOST_GAP))
                    .child(
                        div()
                            .id(SharedString::from(format!("collapse-host-{endpoint_id}")))
                            .debug_selector(|| format!("collapse-host-{endpoint_id}"))
                            .w(px(HOST_ARROW_WIDTH))
                            .flex_none()
                            .child(label_text(if endpoint.collapsed {
                                "\u{25b8}"
                            } else {
                                "\u{25be}"
                            }))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                if let Some(endpoint) =
                                    this.endpoints.iter_mut().find(|e| e.id == collapse_id)
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
                    .when_some(load_view, |row, (_, view)| {
                        row.child(
                            div()
                                .id(SharedString::from(format!("host-load-{endpoint_id}")))
                                .debug_selector(|| format!("host-load-{endpoint_id}"))
                                .flex_none()
                                .child(view)
                                .tooltip(crate::system_load::tooltip(
                                    cx.weak_entity(),
                                    host.clone(),
                                    theme,
                                ))
                                .tooltip_show_delay(std::time::Duration::ZERO),
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
            .on_click(cx.listener(move |this, _, window, cx| {
                this.select_endpoint(&select_id, cx);
                window.focus(&this.focus, cx);
            }))
    }
}
