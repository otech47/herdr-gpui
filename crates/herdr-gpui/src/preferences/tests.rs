#![allow(clippy::unwrap_used)]
use super::*;

#[core::prelude::v1::test]
fn font_size_input_accepts_only_whole_values_in_range() {
    for (input, expected) in [
        ("8", Some(8.)),
        ("48", Some(48.)),
        (" 24 ", Some(24.)),
        ("7", None),
        ("49", None),
        ("14.5", None),
        ("-8", None),
        ("+12", None),
        ("12px", None),
        ("", None),
        ("999999", None),
    ] {
        assert_eq!(parse_font_size(input), expected, "{input:?}");
    }
}
