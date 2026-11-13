//! The card every window shows before the app is relaunched: a countdown and
//! a Cancel button. An agent asks for it through the control socket and
//! relaunches only when the countdown runs out.
use crate::config::{Theme, corners};
use gpui::{prelude::*, *};
use std::time::{Duration, Instant};

pub(crate) const MAX_SECONDS: u64 = 30;
/// "Restarting now" clears itself when no relaunch follows.
const RESTARTING_FOR: Duration = Duration::from_secs(20);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Idle,
    Counting {
        until: Instant,
        left: u64,
    },
    Cancelled,
    Restarting {
        until: Instant,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Outcome {
    Proceed,
    Cancelled,
}

impl State {
    fn counting(seconds: u64, now: Instant) -> Self {
        let seconds = seconds.clamp(1, MAX_SECONDS);
        Self::Counting {
            until: now + Duration::from_secs(seconds),
            left: seconds,
        }
    }

    fn next(self, now: Instant) -> (Self, Option<Outcome>) {
        match self {
            Self::Counting { until, .. } if now >= until => (
                Self::Restarting {
                    until: now + RESTARTING_FOR,
                },
                Some(Outcome::Proceed),
            ),
            Self::Counting { until, .. } => {
                let left = (until - now).as_secs_f64().ceil() as u64;
                (Self::Counting { until, left }, None)
            }
            Self::Cancelled => (Self::Idle, Some(Outcome::Cancelled)),
            Self::Restarting { until } if now >= until => (Self::Idle, None),
            state => (state, None),
        }
    }
}

#[derive(Default)]
pub(crate) struct Notice {
    state: State,
    reason: Option<SharedString>,
}

impl Global for Notice {}

impl Notice {
    /// Starts a countdown, or leaves one already running as it is, so two
    /// callers asking at once share it.
    pub(crate) fn start(seconds: u64, reason: Option<&str>, cx: &mut App) {
        let notice = cx.default_global::<Self>();
        if matches!(notice.state, State::Counting { .. }) {
            return;
        }
        notice.state = State::counting(seconds, Instant::now());
        notice.reason = reason
            .map(|reason| crate::notifications::safe_text(reason, 160))
            .filter(|reason| !reason.is_empty())
            .map(Into::into);
        cx.refresh_windows();
    }

    fn cancel(cx: &mut App) {
        let notice = cx.default_global::<Self>();
        if matches!(notice.state, State::Counting { .. }) {
            notice.state = State::Cancelled;
            cx.refresh_windows();
        }
    }

    pub(crate) fn tick(cx: &mut App) -> Option<Outcome> {
        let state = cx.try_global::<Self>()?.state;
        let (next, outcome) = state.next(Instant::now());
        if next != state {
            cx.global_mut::<Self>().state = next;
            cx.refresh_windows();
        }
        outcome
    }

    pub(crate) fn render(theme: &Theme, size: f32, cx: &App) -> Option<AnyElement> {
        let notice = cx.try_global::<Self>()?;
        let (title, counting) = match notice.state {
            State::Counting { left, .. } => (format!("Restarting Herdr in {left}"), true),
            State::Restarting { .. } => ("Restarting Herdr now".to_owned(), false),
            State::Idle | State::Cancelled => return None,
        };
        let primary = theme.primary();
        let card = div()
            .id("relaunch-notice")
            .debug_selector(|| "relaunch-notice".into())
            .occlude()
            .w(px(340.))
            .p(px(20.))
            .rounded(px(corners::PANEL))
            .border_1()
            .border_color(rgb(theme.ink(primary)))
            .bg(rgb(theme.surface))
            .shadow_lg()
            .text_color(rgb(theme.foreground))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(10.))
            .child(
                div()
                    .text_size(px(size * 1.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(title),
            )
            .when_some(notice.reason.clone(), |card, reason| {
                card.child(div().text_center().child(reason))
            })
            .child(
                div()
                    .text_color(rgb(theme.muted))
                    .child("Terminals and agents keep running."),
            )
            .when(counting, |card| {
                card.child(
                    div()
                        .id("relaunch-cancel")
                        .debug_selector(|| "relaunch-cancel".into())
                        .mt(px(6.))
                        .w_full()
                        .h(px(40.))
                        .rounded(px(corners::CONTROL))
                        .bg(rgb(primary))
                        .text_color(rgb(theme.text_on(primary)))
                        .font_weight(FontWeight::SEMIBOLD)
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .hover(|button| button.opacity(0.85))
                        .child("Cancel")
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_click(|_, _, cx| Self::cancel(cx)),
                )
            });
        Some(
            div()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .child(card)
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, Instant, MAX_SECONDS, Notice, Outcome, RESTARTING_FOR, State};
    use gpui::{Modifiers, px, size};

    #[test]
    fn a_countdown_counts_whole_seconds_then_proceeds() {
        let start = Instant::now();
        let state = State::counting(5, start);
        let (state, outcome) = state.next(start + Duration::from_millis(1200));
        assert_eq!((state.left(), outcome), (Some(4), None));
        let (state, outcome) = state.next(start + Duration::from_secs(5));
        assert!(matches!(state, State::Restarting { .. }));
        assert_eq!(outcome, Some(Outcome::Proceed));
        let (state, outcome) = state.next(start + Duration::from_secs(5) + RESTARTING_FOR);
        assert_eq!((state, outcome), (State::Idle, None));
    }

    #[test]
    fn a_cancelled_countdown_reports_once_and_clears() {
        let (state, outcome) = State::Cancelled.next(Instant::now());
        assert_eq!((state, outcome), (State::Idle, Some(Outcome::Cancelled)));
        assert_eq!(State::Idle.next(Instant::now()), (State::Idle, None));
    }

    #[test]
    fn a_countdown_lasts_between_one_and_thirty_seconds() {
        let now = Instant::now();
        assert_eq!(State::counting(0, now).left(), Some(1));
        assert_eq!(State::counting(600, now).left(), Some(MAX_SECONDS));
    }

    #[gpui::test]
    fn cancel_on_the_card_stops_the_countdown(cx: &mut gpui::TestAppContext) {
        let (_, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
        cx.simulate_resize(size(px(800.), px(600.)));
        cx.update(|window, cx| {
            Notice::start(5, Some("Installs the model card."), cx);
            window.draw(cx).clear(cx);
        });
        assert!(cx.debug_bounds("relaunch-notice").is_some());
        let Some(cancel) = cx.debug_bounds("relaunch-cancel") else {
            panic!("the card has no cancel button");
        };
        cx.simulate_click(cancel.center(), Modifiers::default());
        assert_eq!(
            cx.update(|_, cx| Notice::tick(cx)),
            Some(Outcome::Cancelled)
        );
        cx.update(|window, cx| window.draw(cx).clear(cx));
        assert!(cx.debug_bounds("relaunch-notice").is_none());
    }

    impl State {
        fn left(self) -> Option<u64> {
            match self {
                Self::Counting { left, .. } => Some(left),
                _ => None,
            }
        }
    }
}
