use crate::{HerdrWindow, config::corners};
use gpui::{prelude::*, *};

impl HerdrWindow {
    pub(crate) fn render_new_tab(&self, cx: &mut Context<Self>) -> Div {
        let (Some(prompt), Some(new_tab)) = (&self.menu.new_tab, &self.config.new_tab) else {
            return div();
        };
        let theme = &self.theme;
        let chip = |id: (&'static str, usize), key: String, label: String, on: bool| {
            div()
                .id(id)
                .debug_selector(move || format!("{}-{}", id.0, id.1))
                .flex()
                .gap(px(5.))
                .px(px(8.))
                .py(px(3.))
                .rounded(px(corners::CONTROL))
                .border_1()
                .border_color(rgb(if on { theme.foreground } else { theme.active }))
                .when(on, |chip| chip.bg(rgb(theme.active)))
                .cursor_pointer()
                .hover(|chip| chip.bg(rgb(theme.active)))
                .when(!key.is_empty(), |chip| {
                    chip.child(div().text_color(rgb(theme.muted)).child(key))
                })
                .child(label)
        };
        let row = |name: &'static str| {
            div().flex().flex_wrap().items_center().gap(px(6.)).child(
                div()
                    .w(px(52.))
                    .text_color(rgb(theme.subtext()))
                    .child(name),
            )
        };
        let choices = self.launcher_choices();
        let devices = row("device").children(choices.iter().enumerate().map(|(index, choice)| {
            let key = if index < 9 {
                (index + 1).to_string()
            } else {
                String::new()
            };
            chip(
                ("new-tab-device", index),
                key,
                choice.label.clone(),
                index == prompt.device,
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.pick_new_tab(|prompt| prompt.device = index, cx)
            }))
        }));
        let agents =
            row("agent").children(new_tab.agents.iter().enumerate().map(|(index, agent)| {
                chip(
                    ("new-tab-agent", index),
                    agent.key.to_string(),
                    agent.label.clone(),
                    index == prompt.agent,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pick_new_tab(|prompt| prompt.agent = index, cx)
                }))
            }));
        let spaces = row("space")
            .children(prompt.spaces.iter().enumerate().map(|(index, space)| {
                chip(
                    ("new-tab-space", index),
                    String::new(),
                    space.label.clone(),
                    index == prompt.space,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.pick_new_tab(|prompt| prompt.space = index, cx)
                }))
            }))
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("\u{2190} \u{2192}"),
            );
        let summary = format!(
            "{} on {} in {}",
            new_tab
                .agents
                .get(prompt.agent)
                .map_or("", |agent| agent.label.as_str()),
            choices
                .get(prompt.device)
                .map_or("", |choice| choice.label.as_str()),
            prompt
                .spaces
                .get(prompt.space)
                .map_or("", |space| space.label.as_str()),
        );
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
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(self.config.ui.size * 1.35))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child("New tab"),
            )
            .child(devices)
            .child(agents)
            .child(spaces)
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(8.))
                    .child(
                        div()
                            .debug_selector(|| "new-tab-summary".into())
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(summary),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.))
                            .child(button("new-tab-cancel", "Cancel", false).on_click(
                                cx.listener(|this, _, window, cx| this.dismiss_menu(window, cx)),
                            ))
                            .child(button("new-tab-open", "Open", true).on_click(
                                cx.listener(|this, _, window, cx| this.confirm_new_tab(window, cx)),
                            )),
                    ),
            )
    }
}
