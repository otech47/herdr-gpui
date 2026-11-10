use crate::usage::model::{Balance, Severity, Unit, group};

#[test]
fn balances_read_in_their_own_unit() {
    let usd = Balance::new("Credits", 12.3, Unit::Currency("USD".into()));
    assert_eq!(usd.text(), "$12.30");
    assert_eq!(usd.clone().out_of(50.).text(), "$12.30 of $50.00");
    assert_eq!(
        Balance::new("Left", 4.5, Unit::Currency("EUR".into())).text(),
        "4.50 EUR"
    );
    assert_eq!(
        Balance::new("Points", 1_250_000., Unit::Count("points".into())).text(),
        "1,250,000 points"
    );
    assert_eq!(group(-1234), "-1,234");
    assert_eq!(Severity::from(60.), Severity::Warning);
}
