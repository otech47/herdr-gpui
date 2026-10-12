use super::*;
use crate::usage::model::{Balance, Unit};

#[test]
fn the_bar_shows_the_share_used_of_the_tightest_window_else_a_balance() {
    let mut report = report(provider("claude"), 10.);
    report
        .windows
        .push(Window::new(Kind::Weekly, 40.4, None, None));
    assert_eq!(report.summary().as_deref(), Some("40%"));
    report.windows.clear();
    assert_eq!(report.summary(), None);
    report
        .balances
        .push(Balance::new("Credits", 12.3, Unit::Currency("USD".into())));
    assert_eq!(report.summary().as_deref(), Some("$12.30"));
}
