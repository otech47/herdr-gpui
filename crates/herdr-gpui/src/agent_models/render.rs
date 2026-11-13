//! The hover card: one line with the model, a rising five-step effort meter
//! and the effort's own word, under an optional heading for rows too narrow
//! to show their title.

use super::Model;
use crate::config::Theme;
use gpui::{prelude::*, *};

const STEPS: u8 = 5;
const STEP_WIDTH: f32 = 2.;
const STEP_GAP: f32 = 1.5;
/// Heights from the lowest step to the highest, so the meter reads as a level.
const STEP_HEIGHTS: [f32; STEPS as usize] = [4., 5.5, 7., 8.5, 10.];

/// How many steps of [`STEPS`] an effort fills; None for a word no agent
/// is known to use, which is shown without a meter.
pub(super) fn level(effort: &str) -> Option<u8> {
    match effort {
        "none" | "off" | "minimal" => Some(0),
        "low" => Some(1),
        "medium" => Some(2),
        "high" => Some(3),
        "xhigh" => Some(4),
        "max" => Some(5),
        _ => None,
    }
}

struct Card {
    heading: Option<SharedString>,
    model: SharedString,
    effort: SharedString,
    level: Option<u8>,
    foreground: u32,
    muted: u32,
    surface: u32,
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let (foreground, muted) = (self.foreground, self.muted);
        let meter = self.level.map(|level| {
            div()
                .debug_selector(|| "agent-model-meter".into())
                .flex()
                .flex_none()
                .items_end()
                .gap(px(STEP_GAP))
                .h(px(STEP_HEIGHTS[STEPS as usize - 1]))
                .children(STEP_HEIGHTS.iter().enumerate().map(move |(step, height)| {
                    div().w(px(STEP_WIDTH)).h(px(*height)).rounded(px(0.5)).bg(
                        if (step as u8) < level {
                            rgb(foreground)
                        } else {
                            rgba((muted << 8) | 0x50)
                        },
                    )
                }))
        });
        let line = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                div()
                    .text_color(rgb(foreground))
                    .font_weight(FontWeight::MEDIUM)
                    .child(self.model.clone()),
            )
            .children(meter)
            .when(!self.effort.is_empty(), |line| {
                line.child(div().text_color(rgb(muted)).child(self.effort.clone()))
            });
        div()
            .debug_selector(|| "agent-model-card".into())
            .flex()
            .flex_col()
            .gap(px(2.))
            .px(px(7.))
            .py(px(3.))
            .rounded(px(crate::config::corners::CONTROL))
            .shadow_md()
            .border_1()
            .border_color(rgba((muted << 8) | 0x30))
            .bg(rgb(self.surface))
            .text_size(px(11.))
            .line_height(px(15.))
            .children(
                self.heading
                    .clone()
                    .map(|heading| div().text_color(rgb(foreground)).child(heading)),
            )
            .child(line)
    }
}

pub(crate) fn tooltip(
    heading: Option<SharedString>,
    model: &Model,
    theme: &Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let model_name = SharedString::from(model.model.clone());
    let effort = SharedString::from(model.effort.clone());
    let level = level(&model.effort);
    let (foreground, muted, surface) = (theme.foreground, theme.muted, theme.surface);
    move |_, cx| {
        cx.new(|_| Card {
            heading: heading.clone(),
            model: model_name.clone(),
            effort: effort.clone(),
            level,
            foreground,
            muted,
            surface,
        })
        .into()
    }
}
