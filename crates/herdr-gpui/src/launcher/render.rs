//! The device prompt: the launcher's label, one row per device with its
//! number, and Cancel and Run buttons, styled like the close confirmation.

use crate::{HerdrWindow, config::corners};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn render_launcher(&self, cx: &mut Context<Self>) -> Div {
        let Some(prompt) = &self.menu.launcher else {
            return div();
        };
        let Some(launcher) = self.config.launchers.get(prompt.launcher) else {
            return div();
        };
        let theme = &self.theme;
        let selected = prompt.selected;
        let choices = self.launcher_choices();
        let button = |id: &'static str, label: &str, highlighted: bool| {
            div()
                .id(id)
                .debug_selector(move || id.into())
                .px(px(12.))
                .py(px(6.))
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(if highlighted {
                    theme.foreground
                } else {
                    theme.active
                }))
                .when(highlighted, |button| button.bg(rgb(theme.active)))
                .cursor_pointer()
                .hover(|button| button.bg(rgb(theme.active)))
                .child(label.to_owned())
        };
        div()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(
                div()
                    .text_size(px(self.config.ui.size * 1.35))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(launcher.label.clone()),
            )
            .child(
                div()
                    .text_color(rgb(theme.subtext()))
                    .child("Run on which device?"),
            )
            .child(div().flex().flex_col().gap(px(2.)).children(
                choices.into_iter().enumerate().map(|(index, choice)| {
                    let current = index == selected;
                    div()
                        .id(("launcher-choice", index))
                        .debug_selector(move || format!("launcher-choice-{index}"))
                        .flex()
                        .items_center()
                        .gap(px(10.))
                        .px(px(10.))
                        .py(px(6.))
                        .rounded(px(corners::CONTROL))
                        .cursor_pointer()
                        .when(current, |row| row.bg(rgb(theme.active)))
                        .hover(|row| row.bg(rgb(theme.active)))
                        .child(
                            div()
                                .w(px(14.))
                                .text_color(rgb(theme.muted))
                                .child(if index < 9 {
                                    (index + 1).to_string()
                                } else {
                                    String::new()
                                }),
                        )
                        .child(div().flex_1().child(choice.label))
                        .child(
                            div()
                                .text_color(rgb(theme.muted))
                                .child(choice.host.unwrap_or_else(|| "this computer".into())),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.confirm_launcher(index, window, cx)
                        }))
                }),
            ))
            .child(
                div()
                    .flex()
                    .justify_end()
                    .gap(px(8.))
                    .child(
                        button("launcher-cancel", "Cancel", false).on_click(
                            cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                        ),
                    )
                    .child(button("launcher-run", "Run", true).on_click(cx.listener(
                        move |this, _, window, cx| this.confirm_launcher(selected, window, cx),
                    ))),
            )
    }
}
