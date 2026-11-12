//! The agents list: how it is sorted, and how each agent's place and status
//! are labelled. Status comes from the daemon's snapshot, never from guessing
//! at terminal output.

use super::{STATUS_DOT_UNKNOWN, STATUS_WIDTH, label_text, line_height};
use crate::{
    HerdrWindow,
    config::{FontConfig, Theme},
    herdr_settings::{IndicatorStyle, Settings},
    listening_ports::Daemon,
    usage::Host,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellAgent};

pub(crate) use herdr_board::{Mark, agent_name, state_label};
pub(super) use herdr_board::{
    agent_labels, agent_names, agent_place, sorted_agents, status_symbol, status_text,
};

impl HerdrWindow {
    /// Limited while the agent waits with a window of its provider on its
    /// host used up. A Claude session also has to have stopped on the limit
    /// itself, since another model can carry on past a model's own limit.
    /// Monitoring while an idle agent has a shell running under it.
    pub(crate) fn mark(
        &self,
        endpoint: usize,
        agent: Option<&ClientShellAgent>,
        status: AgentStatus,
    ) -> Mark {
        let Some((agent, endpoint)) = agent.zip(self.endpoints.get(endpoint)) else {
            return Mark::Status(status);
        };
        let Some(label) = agent.agent.as_deref() else {
            return Mark::Status(status);
        };
        let target = &endpoint.connection.target;
        let waiting = !matches!(status, AgentStatus::Blocked | AgentStatus::Working);
        let stopped = label != "claude"
            || self
                .monitoring
                .stopped(&Daemon::from(target), &agent.tab_id);
        if waiting && stopped && self.usage.limited(&Host::from(target), label) {
            Mark::Limited
        } else if status == AgentStatus::Idle
            && self
                .monitoring
                .watching(&Daemon::from(target), &agent.tab_id, label)
        {
            Mark::Monitoring
        } else {
            Mark::Status(status)
        }
    }
}

pub(super) fn agents_sort(window: &HerdrWindow, cx: &mut Context<HerdrWindow>) -> Stateful<Div> {
    let theme = &window.theme;
    let view = window
        .live
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.agent_view_label.clone());
    let label = view
        .clone()
        .unwrap_or_else(|| window.agent_sort.to_string());
    div()
        .id("agents-sort")
        .debug_selector(|| "agents-sort".into())
        .flex_none()
        .min_w_0()
        .truncate()
        .text_color(rgb(theme.muted))
        .when(view.is_none(), |sort| {
            sort.cursor_pointer()
                .hover(|style| style.text_color(rgb(theme.foreground)))
                .on_click(cx.listener(|this, _, _, cx| {
                    cx.stop_propagation();
                    this.agent_sort = this.agent_sort.toggled();
                    this.agent_sort_modified = true;
                    this.save_chrome();
                    cx.notify();
                }))
        })
        .child(label_text(&label))
}

/// Prepared, comparable props for the cached sidebar, never loaded during render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Indicators {
    pub(super) style: IndicatorStyle,
    colors: [u32; 7],
}

impl Indicators {
    pub(crate) fn new(settings: Option<&Settings>, light: bool, theme: &Theme) -> Self {
        Self {
            style: settings.map_or(IndicatorStyle::Dots, |settings| settings.indicators),
            colors: Mark::ALL.map(|mark| {
                settings.map_or_else(
                    || status_style(mark, theme).2,
                    |settings| theme.ink(settings.mark_color(mark, light)),
                )
            }),
        }
    }

    pub(super) fn color(self, mark: impl Into<Mark>) -> u32 {
        self.colors[mark.into().index()]
    }

    pub(crate) fn width(self, font: &FontConfig) -> f32 {
        match self.style {
            IndicatorStyle::Dots => STATUS_WIDTH,
            IndicatorStyle::Symbols => font.size.ceil().max(STATUS_WIDTH),
        }
    }
}

pub(crate) fn status_indicator(
    mark: impl Into<Mark>,
    font: &FontConfig,
    indicators: Indicators,
) -> Div {
    let mark = mark.into();
    status_mark(mark, font, indicators, indicators.color(mark), false)
}

/// Configured tokens can override color and weight without changing indicator style.
pub(super) fn status_mark(
    mark: impl Into<Mark>,
    font: &FontConfig,
    indicators: Indicators,
    color: u32,
    bold: bool,
) -> Div {
    let mark = mark.into();
    // Upstream dots: working/blocked/done filled, idle hollow, unknown a small dot.
    let (diameter, filled) = match mark {
        Mark::Status(AgentStatus::Unknown) => (STATUS_DOT_UNKNOWN, true),
        Mark::Status(AgentStatus::Idle) | Mark::Monitoring => (STATUS_WIDTH, false),
        _ => (STATUS_WIDTH, true),
    };
    let symbol = indicators.style == IndicatorStyle::Symbols;
    let height = if symbol {
        line_height(font)
    } else {
        STATUS_WIDTH
    };
    div()
        .w(px(indicators.width(font)))
        .h(px(height))
        .mt(px((line_height(font) - height) / 2.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .when(symbol, |slot| {
            slot.overflow_hidden()
                .text_size(px(font.size))
                .line_height(px(line_height(font)))
                .text_color(rgb(color))
                .when(bold, |slot| slot.font_weight(FontWeight::BOLD))
                .child(status_symbol(mark))
        })
        .when(!symbol, |slot| {
            slot.child(
                div()
                    .size(px(if bold { STATUS_WIDTH } else { diameter }))
                    .rounded_full()
                    .border_1()
                    .when(bold, |dot| dot.border_2())
                    .border_color(rgb(color))
                    .when(filled, |dot| dot.bg(rgb(color))),
            )
        })
}

/// Upstream draws status from its own palette, defaulting to Catppuccin Mocha,
/// and never from the terminal's ANSI colors. Matching those literals keeps a
/// dot the same color in both clients whatever terminal theme is loaded, where
/// ANSI slots would drift: Xcode Dark paints its cyan purple. Mocha's pastels
/// vanish on light chrome, so [`Theme::ink`] darkens them there, keeping hue.
/// Limited takes Mocha's overlay1 gray.
pub(super) fn status_style(mark: impl Into<Mark>, theme: &Theme) -> (f32, bool, u32) {
    let (diameter, filled, color) = match mark.into() {
        Mark::Status(AgentStatus::Working) => (STATUS_WIDTH, true, 0xf9e2af),
        Mark::Status(AgentStatus::Blocked) => (STATUS_WIDTH, true, 0xf38ba8),
        Mark::Status(AgentStatus::Done) => (STATUS_WIDTH, true, 0x94e2d5),
        Mark::Status(AgentStatus::Idle) => (STATUS_WIDTH, false, 0xa6e3a1),
        Mark::Status(AgentStatus::Unknown) => (STATUS_DOT_UNKNOWN, true, 0x6c7086),
        Mark::Limited => (STATUS_WIDTH, true, 0x7f849c),
        Mark::Monitoring => (STATUS_WIDTH, false, 0x89b4fa),
    };
    (diameter, filled, theme.ink(color))
}

/// The agent's icon color: its own hue, or the limited gray once its plan is used up.
pub(super) fn icon_tint(
    icon: crate::icons::AgentIcon,
    text: u32,
    mark: Mark,
    indicators: Indicators,
    theme: &Theme,
) -> u32 {
    if mark == Mark::Limited {
        indicators.color(mark)
    } else {
        icon.color(text, theme)
    }
}

#[cfg(test)]
mod tests {
    use super::{Indicators, Mark, status_indicator};
    use crate::{config::FontConfig, herdr_settings::IndicatorStyle};
    use gpui::{Styled, rgb};
    use herdr_client::protocol::AgentStatus;

    #[test]
    fn shared_palette_props_keep_style_and_follow_chrome_contrast() -> anyhow::Result<()> {
        use crate::{config::Theme, contrast::Contrast, herdr_settings::Settings};
        use anyhow::Context as _;
        let settings = Settings::parse_text(
            "[ui]\nstatus_indicators = 'symbols'\n[theme.custom]\nyellow = '#123456'\n",
        )?;
        for light in [false, true] {
            for name in ["Default", "Catppuccin Latte"] {
                let theme = Theme::builtin(name)
                    .context("builtin theme")?
                    .with_contrast(Contrast::High);
                let indicators = Indicators::new(Some(&settings), light, &theme);
                assert_eq!(indicators, Indicators::new(Some(&settings), light, &theme));
                assert_eq!(indicators.style, IndicatorStyle::Symbols);
                for mark in [
                    Mark::Status(AgentStatus::Unknown),
                    Mark::Status(AgentStatus::Idle),
                    Mark::Status(AgentStatus::Working),
                    Mark::Status(AgentStatus::Done),
                    Mark::Status(AgentStatus::Blocked),
                    Mark::Limited,
                ] {
                    assert_eq!(
                        indicators.color(mark),
                        theme.ink(settings.mark_color(mark, light))
                    );
                    for background in [theme.background, theme.surface, theme.active] {
                        assert!(
                            crate::contrast::ratio(indicators.color(mark), background)
                                >= Contrast::High.mark_ratio()
                        );
                    }
                }
            }
        }
        Ok(())
    }

    #[test]
    fn prepared_custom_palette_keeps_each_authoritative_status_color() {
        let font = FontConfig {
            family: "Menlo".into(),
            size: 16.,
            fallbacks: None,
        };
        for style in [IndicatorStyle::Dots, IndicatorStyle::Symbols] {
            let indicators = Indicators {
                style,
                colors: [
                    0x112233, 0x223344, 0x334455, 0x445566, 0x556677, 0x667788, 0x778899,
                ],
            };
            for (mark, expected) in [
                (Mark::Status(AgentStatus::Unknown), 0x112233),
                (Mark::Status(AgentStatus::Idle), 0x223344),
                (Mark::Status(AgentStatus::Working), 0x334455),
                (Mark::Status(AgentStatus::Done), 0x445566),
                (Mark::Status(AgentStatus::Blocked), 0x556677),
                (Mark::Limited, 0x667788),
                (Mark::Monitoring, 0x778899),
            ] {
                assert_eq!(indicators.color(mark), expected);
                if style == IndicatorStyle::Symbols {
                    let mut slot = status_indicator(mark, &font, indicators);
                    assert_eq!(slot.text_style().color, Some(rgb(expected).into()));
                }
            }
        }
    }
}
