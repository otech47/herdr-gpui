//! The `[[links]]` buttons after the sidebar toggle.
use crate::HerdrWindow;
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(super) fn titlebar_leading(&self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .flex_none()
            .items_center()
            .h_full()
            .child(self.sidebar_toggle(cx))
            .children(
                self.config
                    .links
                    .iter()
                    .enumerate()
                    .map(|(index, link)| self.link_button(index, link)),
            )
            .into_any_element()
    }

    fn link_button(&self, index: usize, link: &crate::config::Link) -> AnyElement {
        let (foreground, surface) = (self.theme.foreground, self.theme.surface);
        let (label, url) = (link.label.clone(), link.url.clone());
        div()
            .id(("titlebar-link", index))
            .debug_selector(move || format!("titlebar-link-{index}"))
            .flex_none()
            .size(px(28.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(crate::config::corners::CONTROL))
            .cursor_pointer()
            .hover(|style| style.bg(rgb(self.theme.active)))
            .child(
                svg()
                    .path(SharedString::from(link.icon.clone()))
                    .size(px(16.))
                    .text_color(rgb(foreground)),
            )
            .tooltip(move |_, cx| {
                cx.new(|_| crate::usage::Hint {
                    text: label.clone().into(),
                    foreground,
                    surface,
                })
                .into()
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(move |_, _, cx| {
                cx.stop_propagation();
                cx.open_url(&url);
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use gpui::{Modifiers, TestAppContext, px, size};

    #[gpui::test]
    fn a_link_button_follows_the_sidebar_toggle_and_opens_its_page(cx: &mut TestAppContext) {
        let (view, cx) = cx.add_window_view(|window, cx| {
            let mut view = crate::titlebar::tests::header_window(window, cx);
            view.config.links = vec![crate::config::Link {
                label: "otech".into(),
                url: "http://127.0.0.1:7477".into(),
                icon: "icons/pulse.svg".into(),
            }];
            view
        });
        for width in [1200., 360.] {
            cx.simulate_resize(size(px(width), px(600.)));
            cx.update(|window, cx| {
                window.refresh();
                window.draw(cx).clear(cx);
            });
            let toggle = cx.debug_bounds("toggle-sidebar").unwrap();
            let link = cx.debug_bounds("titlebar-link-0").unwrap();
            assert_eq!(toggle.origin.x, px(super::super::LEADING));
            assert!(link.left() >= toggle.right());
            assert_eq!(link.size, size(px(28.), px(28.)));
            cx.simulate_click(link.center(), Modifiers::default());
            assert_eq!(cx.opened_url(), Some("http://127.0.0.1:7477".into()));
            assert!(view.read_with(cx, |view, _| view.sidebar_visible));
        }
    }
}
