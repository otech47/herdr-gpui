use super::*;
use crate::usage::{Kind, Severity, WEEK, Window, countdown, shown, used_up};

#[test]
fn labels_count_down_in_the_two_coarsest_units() {
    assert_eq!(countdown(Duration::ZERO), "0m");
    assert_eq!(countdown(Duration::from_secs(47 * 60 - 30)), "47m");
    assert_eq!(countdown(Duration::from_secs(2 * 3600 + 53 * 60)), "2h 53m");
    assert_eq!(
        countdown(Duration::from_secs(4 * 86_400 + 11 * 3600 + 59)),
        "4d 11h"
    );
}

#[test]
fn pace_compares_use_with_an_even_spend() {
    let now = at(1_000_000);
    let window =
        |used, left: Duration| Window::new(Kind::Weekly, used, Some(now + left), Some(WEEK));
    let pace = window(20., WEEK / 2).pace(now).unwrap();
    assert_eq!(pace.describe(20.), "30% in reserve · Lasts until reset");
    let pace = window(50., WEEK * 3 / 4).pace(now).unwrap();
    assert_eq!(pace.runs_out, Some(WEEK / 4));
    assert_eq!(pace.describe(50.), "25% in deficit · Runs out in 1d 18h");
    assert_eq!(
        window(49.8, WEEK / 2).pace(now).unwrap().describe(49.8),
        "On pace · Lasts until reset"
    );
    assert_eq!(window(1., WEEK).pace(now), None);
    assert_eq!(window(1., WEEK / 2).pace(now + WEEK), None);
}

#[test]
fn a_gauge_row_shows_the_first_window_and_the_tightest_other() {
    let window = |kind, used| Window::new(kind, used, None, None);
    let windows = [
        window(Kind::Session, 10.),
        window(Kind::Weekly, 40.),
        window(Kind::Named("Fable".into()), 65.),
    ];
    let labels: Vec<(&str, u32)> = shown(&windows)
        .iter()
        .map(|window| (window.kind.short(), window.percent()))
        .collect();
    assert_eq!(labels, [("5h", 10), ("Fable", 65)]);
    assert_eq!(shown(&windows[..1]).len(), 1);
    assert!(shown(&[]).is_empty());
}

#[test]
fn a_window_with_nothing_left_is_used_up() {
    let window = |used| Window::new(Kind::Session, used, None, None);
    assert!(!used_up(&[window(99.4)]));
    assert!(used_up(&[window(10.), window(99.5)]));
    assert!(!used_up(&[]));
    assert_eq!(Severity::from(59.9).token(), "normal");
    assert_eq!(Severity::from(60.).token(), "warning");
    assert_eq!(Severity::from(80.).token(), "critical");
}
