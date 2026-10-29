//! The sidebar's sections: each header folds its list away, and a handle
//! between two open lists drags the boundary.

use super::{DEVICE_FOOTER_HEIGHT, SidebarDrag, label_text, layout::SidebarLook, line_height};
use crate::{
    Command, HerdrWindow,
    config::{FontConfig, Theme},
    preferences::Section,
};
use gpui::{prelude::*, *};

pub(super) const HANDLE_HEIGHT: f32 = 6.;
/// The least a dragged spaces list keeps, so its handle stays reachable.
const MIN_SPACES_HEIGHT: f32 = 24.;

impl HerdrWindow {
    pub(super) fn section_open(&self, section: Section) -> bool {
        !self.sidebar_collapsed.is(section)
    }

    pub(crate) fn toggle_section(&mut self, section: Section, cx: &mut Context<Self>) {
        self.sidebar_collapsed.toggle(section);
        self.sidebar_sections_modified = true;
        self.save_chrome();
        cx.notify();
    }

    /// A section's header: its fold toggle and label, with `controls` at the
    /// far end. The label is the toggle, so the controls keep their own clicks.
    pub(super) fn section_header(
        &self,
        section: Section,
        controls: Option<AnyElement>,
        font: &FontConfig,
        theme: &Theme,
        look: SidebarLook,
        cx: &mut Context<Self>,
    ) -> Div {
        let label = section.name();
        let folded = !self.section_open(section);
        header_shell(label, font, theme, look)
            .justify_between()
            .child(
                div()
                    .id(SharedString::from(format!("section-toggle-{label}")))
                    .debug_selector(|| format!("section-toggle-{label}"))
                    .cursor_pointer()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    // Layout tests pin the label's left edge to the rows' status dots.
                    .child(
                        div()
                            .debug_selector(|| format!("header-label-{label}"))
                            .child(label_text(&look.header_label(label))),
                    )
                    .child(div().flex_none().child(label_text(if folded {
                        "\u{25b8}"
                    } else {
                        "\u{25be}"
                    })))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_section(section, cx))),
            )
            .children(controls)
    }

    /// The handle below a list. A double click forgets the dragged size.
    pub(super) fn section_handle(
        &self,
        drag: SidebarDrag,
        id: &'static str,
        theme: &Theme,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .debug_selector(|| id.into())
            .h(px(HANDLE_HEIGHT))
            .flex_none()
            .cursor(CursorStyle::ResizeUpDown)
            .border_t_1()
            .border_color(rgb(theme.active))
            .hover(|s| s.bg(rgba(crate::panel_resize::RESIZE_HOVER)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    match drag {
                        SidebarDrag::Split => this.sidebar_split_modified = true,
                        SidebarDrag::Spaces => this.sidebar_sections_modified = true,
                        SidebarDrag::Width { .. } => {}
                    }
                    if event.click_count == 2 {
                        this.sidebar_drag = None;
                        match drag {
                            SidebarDrag::Split => this.sidebar_split = None,
                            SidebarDrag::Spaces => this.sidebar_spaces_height = None,
                            SidebarDrag::Width { .. } => {}
                        }
                        this.save_chrome();
                    } else {
                        this.sidebar_drag = Some(drag);
                    }
                    cx.notify();
                }),
            )
    }

    /// Move a section boundary to the pointer's `y`. Lists are measured from
    /// the previous frame, so the handle lands where the pointer is.
    pub(super) fn drag_section(
        &mut self,
        drag: SidebarDrag,
        y: f32,
        sidebar: Bounds<Pixels>,
        header: f32,
    ) {
        let half = HANDLE_HEIGHT / 2.;
        match drag {
            SidebarDrag::Spaces => {
                let top = f32::from(self.sidebar_scroll[0].bounds().top());
                // Leave the headers and handles below room to stay visible.
                let floor = f32::from(sidebar.bottom())
                    - DEVICE_FOOTER_HEIGHT
                    - 2. * header
                    - 2. * HANDLE_HEIGHT;
                let most = (floor - top).max(MIN_SPACES_HEIGHT);
                self.sidebar_spaces_height = Some((y - top - half).clamp(MIN_SPACES_HEIGHT, most));
            }
            SidebarDrag::Split if self.config.show_sidebar_tabs => {
                // The two sections share the space from the tabs header down
                // to the agents list's end, less the handle between them.
                let top = f32::from(self.sidebar_scroll[2].bounds().top()) - header;
                let bottom = f32::from(self.sidebar_scroll[1].bounds().bottom());
                let free = (bottom - top - HANDLE_HEIGHT).max(1.);
                self.sidebar_split = Some(((y - top - half) / free).clamp(0.1, 0.9));
            }
            SidebarDrag::Split => {
                let height =
                    (f32::from(sidebar.size.height) - HANDLE_HEIGHT - DEVICE_FOOTER_HEIGHT).max(1.);
                self.sidebar_split =
                    Some(((y - f32::from(sidebar.origin.y) - half) / height).clamp(0.1, 0.9));
            }
            SidebarDrag::Width { .. } => {}
        }
    }
}

impl HerdrWindow {
    /// The spaces list's footer: a new space, and the menu hugging the
    /// sidebar's edge as in the terminal client.
    pub(super) fn spaces_footer(
        &self,
        font: &FontConfig,
        look: SidebarLook,
        content_x: f32,
        cx: &mut Context<Self>,
    ) -> Div {
        let theme = &self.theme;
        div()
            .flex_none()
            .h(px(line_height(font) + 2. * look.density.footer_padding()))
            .px(px(content_x))
            .flex()
            .items_center()
            .justify_between()
            .text_color(rgb(theme.muted))
            .gap(px(20.))
            .child(
                div()
                    .id("new-workspace")
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    .child("new")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.command(Command::Workspace, window, cx)
                    })),
            )
            .child(
                div()
                    .id("sidebar-menu")
                    .debug_selector(|| "sidebar-menu".into())
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    .child(label_text("menu"))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        this.menu.anchor = event.position();
                        this.open_menu(window, cx);
                    })),
            )
    }

    /// The app menu stays beside the spaces header.
    pub(super) fn top_controls(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        div()
            .flex()
            .items_center()
            .gap(px(20.))
            .text_color(rgb(theme.muted))
            .child(
                div()
                    .id("sidebar-menu-top")
                    .cursor_pointer()
                    .hover(|s| s.text_color(rgb(theme.foreground)))
                    .child(label_text("menu"))
                    .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                        this.menu.anchor = event.position();
                        this.open_menu(window, cx);
                    })),
            )
    }
}

pub(super) fn header_height(font: &FontConfig, look: SidebarLook) -> f32 {
    line_height(font) + 2. * look.density.header_padding()
}

/// A header's frame, without its label.
pub(super) fn header_shell(
    label: &'static str,
    font: &FontConfig,
    theme: &Theme,
    look: SidebarLook,
) -> Div {
    div()
        .debug_selector(|| format!("header-{label}"))
        .flex_none()
        .h(px(header_height(font, look)))
        .px(px(look.content_x()))
        .flex()
        .items_center()
        .text_size(px(font.size))
        .text_color(rgb(theme.muted))
}

#[cfg(test)]
pub(super) fn header(
    label: &'static str,
    font: &FontConfig,
    theme: &Theme,
    look: SidebarLook,
) -> Div {
    header_shell(label, font, theme, look).child(
        div()
            .debug_selector(|| format!("header-label-{label}"))
            .child(label_text(&look.header_label(label))),
    )
}
