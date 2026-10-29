use crate::{
    HerdrWindow,
    preferences::{TabsSort, TabsView},
};
use gpui::{prelude::*, *};

pub(super) const WIDTH: f32 = 240.;

impl HerdrWindow {
    pub(crate) fn open_tabs_options(
        &mut self,
        anchor: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.open_menu(window, cx) {
            self.menu.anchor = point((anchor.x - px(WIDTH - 20.)).max(px(8.)), anchor.y + px(10.));
            self.menu.page = Some(super::Page::TabsOptions);
            self.menu.selected = Some(0);
            cx.notify();
        }
    }

    fn tabs_options_count(&self) -> usize {
        if self.endpoints.len() > 1 { 5 } else { 2 }
    }

    fn select_tabs_option(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        match index {
            0 => self.sidebar_tabs_sort = TabsSort::Attention,
            1 => self.sidebar_tabs_sort = TabsSort::Manual,
            2 => self.sidebar_tabs_view = TabsView::Device,
            3 => self.sidebar_tabs_view = TabsView::All,
            4 => self.sidebar_tabs_view = TabsView::ByDevice,
            _ => return,
        }
        self.sidebar_sections_modified = true;
        self.save_chrome();
        self.dismiss_menu(window, cx);
    }

    pub(super) fn tabs_options_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        window.prevent_default();
        let count = self.tabs_options_count();
        let selected = self.menu.selected.unwrap_or(0).min(count - 1);
        match event.keystroke.key.as_str() {
            "escape" => self.dismiss_menu(window, cx),
            "enter" => self.select_tabs_option(selected, window, cx),
            "up" => self.menu.selected = Some((selected + count - 1) % count),
            "down" => self.menu.selected = Some((selected + 1) % count),
            _ => {}
        }
        cx.notify();
    }

    pub(super) fn render_tabs_options(&self, cx: &mut Context<Self>) -> Div {
        let theme = &self.theme;
        let mut panel = div();
        for (index, label, checked) in [
            (
                0,
                "Attention first",
                self.sidebar_tabs_sort == TabsSort::Attention,
            ),
            (
                1,
                "Manual order",
                self.sidebar_tabs_sort == TabsSort::Manual,
            ),
            (
                2,
                "Current device",
                self.sidebar_tabs_view == TabsView::Device,
            ),
            (3, "All devices", self.sidebar_tabs_view == TabsView::All),
            (
                4,
                "Group by device",
                self.sidebar_tabs_view == TabsView::ByDevice,
            ),
        ]
        .into_iter()
        .take(self.tabs_options_count())
        {
            if index == 0 || index == 2 {
                panel = panel.child(
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_color(rgb(theme.muted))
                        .child(if index == 0 {
                            "Sort tabs"
                        } else {
                            "Show tabs from"
                        }),
                );
            }
            panel = panel.child(
                div()
                    .id(("tabs-option", index))
                    .debug_selector(move || format!("tabs-option-{index}"))
                    .min_h(px(self.config.ui.line_height() + 12.))
                    .px(px(8.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .when(self.menu.selected == Some(index), |row| {
                        row.bg(rgb(theme.active))
                    })
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.menu.selected = Some(index);
                            cx.notify();
                        }
                    }))
                    .child(div().w(px(14.)).child(if checked { "✓" } else { "" }))
                    .child(label)
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.select_tabs_option(index, window, cx);
                    })),
            );
        }
        panel
    }
}
