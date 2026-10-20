//! The agents list: how it is sorted, and how each agent's place and status
//! are labelled. Status comes from the daemon's snapshot, never from guessing
//! at terminal output.

use super::{STATUS_DOT_UNKNOWN, STATUS_WIDTH, first_text, label_text, line_height};
use crate::{
    HerdrWindow,
    config::{FontConfig, Theme},
    herdr_settings::{IndicatorStyle, Settings},
    usage::Host,
};
use gpui::{prelude::*, *};
use herdr_client::protocol::{AgentStatus, ClientShellAgent, ClientShellSnapshot};

/// The daemon's status, or that the agent's plan is used up, which the daemon
/// cannot see and this client reads from the host's usage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mark {
    Status(AgentStatus),
    Limited,
}

impl From<AgentStatus> for Mark {
    fn from(status: AgentStatus) -> Self {
        Self::Status(status)
    }
}

impl Mark {
    fn index(self) -> usize {
        match self {
            Self::Status(AgentStatus::Unknown) => 0,
            Self::Status(AgentStatus::Idle) => 1,
            Self::Status(AgentStatus::Working) => 2,
            Self::Status(AgentStatus::Done) => 3,
            Self::Status(AgentStatus::Blocked) => 4,
            Self::Limited => 5,
        }
    }
}

impl HerdrWindow {
    /// Limited once a window of the agent's provider on its host is used up,
    /// unless the agent is blocked, which still needs an answer.
    pub(crate) fn mark(
        &self,
        endpoint: usize,
        agent: Option<&ClientShellAgent>,
        status: AgentStatus,
    ) -> Mark {
        let limited = status != AgentStatus::Blocked
            && agent
                .and_then(|agent| agent.agent.as_deref())
                .zip(self.endpoints.get(endpoint))
                .is_some_and(|(agent, endpoint)| {
                    self.usage
                        .limited(&Host::from(&endpoint.connection.target), agent)
                });
        if limited {
            Mark::Limited
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

/// Attention first, then the most recent change, as upstream orders it.
pub(super) fn status_priority(status: AgentStatus) -> u8 {
    match status {
        AgentStatus::Blocked => 4,
        AgentStatus::Done => 3,
        AgentStatus::Working => 2,
        AgentStatus::Idle => 1,
        AgentStatus::Unknown => 0,
    }
}

/// The agents of one endpoint in the order the panel paints them.
///
/// A plugin's agent view (`agent.view.set`) is authoritative while the daemon
/// names it: the panel shows exactly the panes in `agent_order`, in that
/// order, so agents its filter excluded stay hidden and an empty order means
/// none match. Upstream's client keys this on the label, not on the order
/// being non-empty, and drops ids the snapshot has no agent for. Without a
/// view the local sort applies.
pub(super) fn sorted_agents(
    snapshot: &ClientShellSnapshot,
    sort: crate::preferences::AgentSort,
) -> Vec<&ClientShellAgent> {
    if snapshot.agent_view_label.is_some() {
        return snapshot
            .agent_order
            .iter()
            .filter_map(|pane_id| {
                snapshot
                    .agents
                    .iter()
                    .find(|agent| agent.pane_id == *pane_id)
            })
            .collect();
    }
    let mut ordered: Vec<_> = snapshot.agents.iter().collect();
    if sort == crate::preferences::AgentSort::Priority {
        ordered.sort_by_key(|agent| {
            (
                std::cmp::Reverse(status_priority(agent.agent_status)),
                std::cmp::Reverse(agent.state_change_seq),
            )
        });
    }
    ordered
}

/// What an agent is called wherever it is listed.
pub(crate) fn agent_name(agent: &ClientShellAgent) -> &str {
    first_text(agent_names(agent), "agent")
}

pub(super) fn agent_names(agent: &ClientShellAgent) -> [Option<&str>; 4] {
    [
        agent.display_agent.as_deref(),
        agent.name.as_deref(),
        agent.agent.as_deref(),
        agent.title.as_deref(),
    ]
}

/// Where an agent runs: its workspace, and its tab when that earns a place,
/// which upstream decides by the workspace having several tabs or the user
/// naming it. `None` once the agent's workspace has gone.
pub(super) fn agent_place<'a>(
    agent: &ClientShellAgent,
    snapshot: &'a ClientShellSnapshot,
) -> Option<(&'a str, Option<&'a str>)> {
    let workspace = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.workspace_id == agent.workspace_id)?;
    let tabs = snapshot
        .tabs
        .iter()
        .filter(|tab| tab.workspace_id == agent.workspace_id)
        .count();
    let tab = snapshot
        .tabs
        .iter()
        .find(|tab| tab.tab_id == agent.tab_id)
        .filter(|tab| tabs > 1 || tab.custom_label)
        .map(|tab| tab.label.as_str());
    Some((workspace.label.as_str(), tab))
}

/// Upstream's default agent rows: host, workspace and tab on the first line,
/// the agent itself on the second. A pane whose workspace has gone leaves the
/// agent to name the row.
pub(super) fn agent_labels<'a>(
    name: &'a str,
    place: Option<(&'a str, Option<&'a str>)>,
    host: Option<&'a str>,
) -> (Vec<(&'a str, bool)>, &'a str) {
    let Some((workspace, tab)) = place else {
        return (vec![(name, true)], "");
    };
    // Only the workspace carries the row's weight: upstream paints the host and
    // tab around it in its secondary color.
    let segments = [(host, false), (Some(workspace), true), (tab, false)]
        .into_iter()
        .filter_map(|(text, primary)| Some((text?, primary)))
        .filter(|(text, _)| !text.is_empty())
        .collect();
    (segments, name)
}

/// Prepared, comparable props for the cached sidebar, never loaded during render.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Indicators {
    pub(super) style: IndicatorStyle,
    colors: [u32; 6],
}

impl Indicators {
    pub(crate) fn new(settings: Option<&Settings>, light: bool, theme: &Theme) -> Self {
        Self {
            style: settings.map_or(IndicatorStyle::Dots, |settings| settings.indicators),
            colors: [
                Mark::Status(AgentStatus::Unknown),
                Mark::Status(AgentStatus::Idle),
                Mark::Status(AgentStatus::Working),
                Mark::Status(AgentStatus::Done),
                Mark::Status(AgentStatus::Blocked),
                Mark::Limited,
            ]
            .map(|mark| {
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

pub(super) fn status_symbol(mark: impl Into<Mark>) -> &'static str {
    match mark.into() {
        Mark::Status(AgentStatus::Working) => "\u{25d0}",
        Mark::Status(AgentStatus::Blocked) => "\u{d7}",
        Mark::Status(AgentStatus::Done) => "\u{2713}",
        Mark::Status(AgentStatus::Idle) => "\u{25cb}",
        Mark::Status(AgentStatus::Unknown) => "\u{b7}",
        Mark::Limited => "\u{2298}",
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
        Mark::Status(AgentStatus::Idle) => (STATUS_WIDTH, false),
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

/// The word the daemon's `state_text` token shows for a status when its
/// sidebar config asks for it. Lowercase, matching the daemon's status names
/// and what the terminal client prints.
pub(super) fn status_text(mark: impl Into<Mark>) -> &'static str {
    match mark.into() {
        Mark::Status(AgentStatus::Working) => "working",
        Mark::Status(AgentStatus::Blocked) => "blocked",
        Mark::Status(AgentStatus::Done) => "done",
        Mark::Status(AgentStatus::Idle) => "idle",
        Mark::Status(AgentStatus::Unknown) => "unknown",
        Mark::Limited => "limited",
    }
}

/// Herdr's own cap on a state label, so a longer one came from a peer that
/// ignored it and is cut where the terminal client would have cut it.
const STATE_LABEL_LIMIT: usize = 80;

/// The words an agent's integration chose for its current status, as Herdr's
/// `state_labels` metadata, else `fallback`. Labels are untrusted display
/// text: bounded and stripped of controls and bidi overrides.
pub(crate) fn state_label<'a>(
    agent: &ClientShellAgent,
    fallback: &'a str,
) -> std::borrow::Cow<'a, str> {
    let key = status_text(agent.agent_status);
    agent
        .state_labels
        .iter()
        .find(|(state, _)| state == key)
        .map(|(_, label)| crate::notifications::safe_text(label, STATE_LABEL_LIMIT))
        .map(|label| label.trim().to_owned())
        .filter(|label| !label.is_empty())
        .map_or(
            std::borrow::Cow::Borrowed(fallback),
            std::borrow::Cow::Owned,
        )
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
    use super::{Indicators, Mark, sorted_agents, state_label, status_indicator};
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
                colors: [0x112233, 0x223344, 0x334455, 0x445566, 0x556677, 0x667788],
            };
            for (mark, expected) in [
                (Mark::Status(AgentStatus::Unknown), 0x112233),
                (Mark::Status(AgentStatus::Idle), 0x223344),
                (Mark::Status(AgentStatus::Working), 0x334455),
                (Mark::Status(AgentStatus::Done), 0x445566),
                (Mark::Status(AgentStatus::Blocked), 0x556677),
                (Mark::Limited, 0x667788),
            ] {
                assert_eq!(indicators.color(mark), expected);
                if style == IndicatorStyle::Symbols {
                    let mut slot = status_indicator(mark, &font, indicators);
                    assert_eq!(slot.text_style().color, Some(rgb(expected).into()));
                }
            }
        }
    }

    /// An integration's label names the agent's current status; a label for
    /// another status, a blank one, or none leaves the fallback. Labels are
    /// untrusted: controls and bidi overrides go, and length is bounded.
    #[test]
    fn state_labels_name_the_current_status_as_safe_bounded_text() {
        let agent = |status, labels: &[(&str, &str)]| herdr_client::protocol::ClientShellAgent {
            pane_id: "p".into(),
            workspace_id: "w".into(),
            tab_id: "t".into(),
            name: None,
            display_agent: None,
            agent: None,
            title: None,
            terminal_title: None,
            terminal_title_stripped: None,
            agent_status: status,
            state_change_seq: 0,
            state_labels: labels
                .iter()
                .map(|(state, label)| ((*state).into(), (*label).into()))
                .collect(),
            tokens: Vec::new(),
            focused: false,
        };
        let labels = [("working", "deep in the mines"), ("blocked", "  ")];
        for (status, fallback, expected) in [
            (AgentStatus::Working, "working", "deep in the mines"),
            (AgentStatus::Blocked, "blocked", "blocked"),
            (AgentStatus::Done, "done", "done"),
            (AgentStatus::Unknown, "", ""),
        ] {
            assert_eq!(state_label(&agent(status, &labels), fallback), expected);
        }
        let hostile = agent(
            AgentStatus::Working,
            &[("working", "\u{1b}[31mred\u{202e}dlrow\n")],
        );
        assert_eq!(state_label(&hostile, "working"), "[31mreddlrow");
        let long = "x".repeat(10_000);
        let long = agent(AgentStatus::Idle, &[("idle", &long)]);
        assert_eq!(state_label(&long, "idle").chars().count(), 80);
    }

    fn ordered(
        snapshot: &herdr_client::protocol::ClientShellSnapshot,
        sort: crate::preferences::AgentSort,
    ) -> Vec<&str> {
        sorted_agents(snapshot, sort)
            .into_iter()
            .map(|agent| agent.pane_id.as_str())
            .collect()
    }

    #[test]
    fn a_plugin_view_order_replaces_the_local_sort_and_filters() {
        use crate::preferences::AgentSort;
        let mut snapshot = crate::sidebar::layout_tests::snapshot(2);
        snapshot.agents[1].agent_status = AgentStatus::Blocked;
        // Without a view the order is local, even when the daemon sent one.
        snapshot.agent_order = vec!["p1".into()];
        assert_eq!(ordered(&snapshot, AgentSort::Grouped), ["p0", "p1"]);
        assert_eq!(ordered(&snapshot, AgentSort::Priority), ["p1", "p0"]);

        snapshot.agent_view_label = Some("review".into());
        snapshot.agent_order = vec!["p1".into(), "gone".into(), "p0".into()];
        for sort in [AgentSort::Grouped, AgentSort::Priority] {
            // A pane the snapshot no longer lists as an agent is skipped.
            assert_eq!(ordered(&snapshot, sort), ["p1", "p0"]);
        }
        snapshot.agents[1].agent_status = AgentStatus::Idle;
        snapshot.agent_order = vec!["p0".into()];
        assert_eq!(ordered(&snapshot, AgentSort::Priority), ["p0"]);
        snapshot.agent_order.clear();
        assert!(ordered(&snapshot, AgentSort::Grouped).is_empty());
    }
}
