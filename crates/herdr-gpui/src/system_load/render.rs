//! Drawing a host's load: a CPU sparkline and a memory meter with their
//! shares in the status bar, an icon, a small bar and a share for each after
//! a host's name, and a card with the recent CPU graph and the details shown
//! at once on hover.

use super::{HISTORY, INTERVAL, Reading};
use crate::{config::Theme, usage::Host, window::HerdrWindow};
use gpui::{prelude::*, *};
use std::time::Duration;

const BAR_WIDTH: f32 = 2.;
const BAR_GAP: f32 = 1.;
const GRAPH_HEIGHT: f32 = 11.;
const METER_WIDTH: f32 = 32.;
const ITEM_GAP: f32 = 5.;
const LINE_GAP: f32 = 10.;
/// Glyphs a share takes at most: `100%`.
const SHARE_GLYPHS: f32 = 4.;
const ICON_GAP: f32 = 3.;
const COMPACT_BAR_WIDTH: f32 = 16.;
const COMPACT_GAP: f32 = 8.;
const CARD_WIDTH: f32 = 248.;
const CARD_PADDING: f32 = 10.;
const CARD_GRAPH_HEIGHT: f32 = 36.;
const CPU_WARN: f32 = 75.;
const MEMORY_WARN: f32 = 80.;
const CRITICAL: f32 = 90.;

impl HerdrWindow {
    /// The selected host's load; nothing while hidden or before the host's
    /// first answer.
    pub(crate) fn render_system_load(&self, cx: &Context<Self>) -> Option<impl IntoElement> {
        if !self.config.show_system_load {
            return None;
        }
        let host = self.selected_host()?;
        let reading = self.system_load.get(&host)?;
        let theme = &self.theme;
        let segment = div()
            .id("system-load")
            .debug_selector(|| "system-load".into())
            .flex()
            .flex_none()
            .items_center()
            .px(px(6.))
            .h_full()
            .tooltip(tooltip(cx.weak_entity(), host, theme))
            .tooltip_show_delay(Duration::ZERO);
        if reading.latest().is_none() {
            // A host that never answered says so; one still starting shows nothing.
            reading.error()?;
            return Some(
                segment.child(
                    div()
                        .text_color(rgb(theme.muted))
                        .child("CPU and memory unavailable"),
                ),
            );
        }
        Some(segment.child(line(reading, theme)))
    }
}

/// A card with `host`'s recent CPU graph, cores, load and memory. It reads
/// the window's latest sample on every frame, so it keeps moving while shown.
pub(crate) fn tooltip(
    window: WeakEntity<HerdrWindow>,
    host: Host,
    theme: &Theme,
) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let theme = theme.clone();
    move |_, cx| {
        cx.new(|_| Card {
            window: window.clone(),
            host: host.clone(),
            theme: theme.clone(),
        })
        .into()
    }
}

struct Card {
    window: WeakEntity<HerdrWindow>,
    host: Host,
    theme: Theme,
}

impl Render for Card {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = &self.theme;
        let name = match &self.host {
            Host::Local => "This machine".to_owned(),
            Host::Ssh(target) => target.clone(),
        };
        let card = div()
            .debug_selector(|| "system-load-card".into())
            .w(px(CARD_WIDTH))
            .p(px(CARD_PADDING))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(crate::config::corners::CONTROL))
            .shadow_md()
            .text_size(px(12.))
            .text_color(rgb(theme.foreground))
            .bg(rgb(theme.surface))
            .child(div().font_weight(FontWeight::SEMIBOLD).child(name));
        let Some(window) = self.window.upgrade() else {
            return card;
        };
        let Some(reading) = window.read(cx).system_load.get(&self.host) else {
            return card;
        };
        let muted = |text: String| div().text_color(rgb(theme.muted)).child(text);
        let stale = reading.error().is_some();
        let card = match reading.latest() {
            Some(sample) => {
                let mut about = Vec::new();
                if let Some(cores) = sample.cores {
                    about.push(format!("{cores} cores"));
                }
                if let Some([one, five, fifteen]) = sample.load {
                    about.push(format!("load {one:.2} {five:.2} {fifteen:.2}"));
                }
                let span = INTERVAL.as_secs() * HISTORY as u64;
                let inner = CARD_WIDTH - 2. * CARD_PADDING;
                let history = HISTORY as f32;
                let bar_width = (inner - (history - 1.) * BAR_GAP) / history;
                let cpu = div()
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(heading(
                        "icons/cpu.svg",
                        "CPU",
                        sample
                            .cpu
                            .map(|value| share(value, CPU_WARN, stale, theme, None)),
                        theme,
                    ))
                    .child(sparkline(
                        reading.history(),
                        theme,
                        bar_width,
                        CARD_GRAPH_HEIGHT,
                    ))
                    .child(
                        div()
                            .flex()
                            .justify_between()
                            .child(muted(about.join(" · ")))
                            .child(muted(format!("last {span} s"))),
                    );
                let memory = sample.memory.map(|memory| {
                    div()
                        .flex()
                        .flex_col()
                        .gap(px(4.))
                        .child(heading(
                            "icons/memory.svg",
                            "Memory",
                            Some(share(memory.percent(), MEMORY_WARN, stale, theme, None)),
                            theme,
                        ))
                        .child(bar(memory.percent(), MEMORY_WARN, inner, 6., theme))
                        .child(muted(format!(
                            "{} of {}",
                            gigabytes(memory.used),
                            gigabytes(memory.total)
                        )))
                });
                card.child(cpu).children(memory)
            }
            None => card.child(muted("CPU and memory…".to_owned())),
        };
        card.children(reading.error().map(|error| muted(error.to_owned())))
    }
}

fn heading(icon: &'static str, name: &'static str, share: Option<Div>, theme: &Theme) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(ICON_GAP + 2.))
        .child(icon_view(icon, 12., theme))
        .child(div().flex_1().child(name))
        .children(share)
}

fn icon_view(path: &'static str, size: f32, theme: &Theme) -> Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_none()
        .text_color(rgb(theme.muted))
}

/// `CPU ▁▃▂▅ 12%  MEM ━━─ 61%`.
fn line(reading: &Reading, theme: &Theme) -> Div {
    let Some(sample) = reading.latest() else {
        return div().text_color(rgb(theme.muted)).child("CPU and memory…");
    };
    // The numbers are the last good ones while a refresh fails.
    let stale = reading.error().is_some();
    let label = |name: &'static str| div().flex_none().text_color(rgb(theme.muted)).child(name);
    let mut cpu = div()
        .debug_selector(|| "system-load-cpu".into())
        .flex()
        .flex_none()
        .items_center()
        .gap(px(ITEM_GAP))
        .child(label("CPU"))
        .child(sparkline(reading.history(), theme, BAR_WIDTH, GRAPH_HEIGHT));
    if let Some(value) = sample.cpu {
        cpu = cpu.child(share(value, CPU_WARN, stale, theme, None));
    }
    let memory = sample.memory.map(|memory| {
        div()
            .debug_selector(|| "system-load-memory".into())
            .flex()
            .flex_none()
            .items_center()
            .gap(px(ITEM_GAP))
            .child(label("MEM"))
            .child(bar(memory.percent(), MEMORY_WARN, METER_WIDTH, 5., theme))
            .child(share(memory.percent(), MEMORY_WARN, stale, theme, None))
    });
    div()
        .flex()
        .items_center()
        .gap(px(LINE_GAP))
        .overflow_hidden()
        .child(cpu)
        .children(memory)
}

/// The view and its width. The width depends only on `glyph` and `icon`,
/// so every host row gives its name the same room.
pub(crate) fn compact(reading: &Reading, theme: &Theme, glyph: f32, icon: f32) -> (f32, Div) {
    let item_width = icon + ICON_GAP + COMPACT_BAR_WIDTH + ICON_GAP + glyph * SHARE_GLYPHS;
    let width = 2. * item_width + COMPACT_GAP;
    let row = div()
        .debug_selector(|| "system-load-compact".into())
        .w(px(width))
        .flex()
        .flex_none()
        .justify_end()
        .items_center()
        .gap(px(COMPACT_GAP));
    let Some(sample) = reading.latest() else {
        return (width, row);
    };
    let stale = reading.error().is_some();
    let item = |path: &'static str, value: f32, warn: f32| {
        div()
            .w(px(item_width))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(ICON_GAP))
            .child(icon_view(path, icon, theme))
            .child(bar(value, warn, COMPACT_BAR_WIDTH, 4., theme))
            .child(share(value, warn, stale, theme, Some(glyph)))
    };
    let cpu = sample.cpu.map(|cpu| item("icons/cpu.svg", cpu, CPU_WARN));
    let memory = sample
        .memory
        .map(|memory| item("icons/memory.svg", memory.percent(), MEMORY_WARN));
    (width, row.children(cpu).children(memory))
}

fn share(value: f32, warn: f32, stale: bool, theme: &Theme, glyph: Option<f32>) -> Div {
    div()
        .flex_none()
        .when_some(glyph, |text, glyph| {
            text.w(px(glyph * SHARE_GLYPHS)).flex().justify_end()
        })
        .when(glyph.is_none(), |text| {
            text.min_w(px(28.)).flex().justify_end()
        })
        .text_color(rgb(if stale {
            theme.muted
        } else {
            severity(value, warn, theme, theme.foreground)
        }))
        .child(format!("{value:.0}%"))
}

fn severity(value: f32, warn: f32, theme: &Theme, normal: u32) -> u32 {
    if value >= CRITICAL {
        theme.ink(theme.palette[1])
    } else if value >= warn {
        theme.ink(theme.palette[3])
    } else {
        normal
    }
}

/// Oldest on the left; empty slots wait on the left until history fills.
fn sparkline(
    history: impl ExactSizeIterator<Item = f32>,
    theme: &Theme,
    bar_width: f32,
    height: f32,
) -> Div {
    let empty = HISTORY.saturating_sub(history.len());
    let bars = std::iter::repeat_n(None, empty)
        .chain(history.map(Some))
        .map(|value| {
            let bar = div().w(px(bar_width)).flex_none().rounded(px(0.5));
            match value {
                Some(value) => bar.h(px((height * value / 100.).max(1.))).bg(rgb(severity(
                    value,
                    CPU_WARN,
                    theme,
                    theme.muted,
                ))),
                None => bar.h(px(1.)).bg(rgb(theme.active)),
            }
        });
    div()
        .flex()
        .flex_none()
        .items_end()
        .gap(px(BAR_GAP))
        .h(px(height))
        .children(bars)
}

fn bar(value: f32, warn: f32, width: f32, height: f32, theme: &Theme) -> Div {
    div()
        .w(px(width))
        .h(px(height))
        .flex_none()
        .rounded_full()
        .overflow_hidden()
        .bg(rgb(theme.active))
        .child(
            div()
                .h_full()
                .w(px(width * value.clamp(0., 100.) / 100.))
                .rounded_full()
                .bg(rgb(severity(value, warn, theme, theme.muted))),
        )
}

/// Binary gigabytes, as Activity Monitor and `free -h` count them.
pub(super) fn gigabytes(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / f64::from(1u32 << 30))
}
