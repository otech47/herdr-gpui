//! Folded sidebar sections and the dragged spaces height are remembered with
//! the rest of the chrome.
use super::*;

#[core::prelude::v1::test]
fn folded_sections_and_the_spaces_height_round_trip() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    let mut collapsed = Collapsed::default();
    collapsed.toggle(Section::Spaces);
    collapsed.toggle(Section::Agents);
    let chrome = Chrome {
        sidebar_width: Some(240.),
        collapsed,
        spaces_height: Some(180.),
        ..Chrome::default()
    };
    write_chrome(&path, chrome).unwrap();
    let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        stored["sidebar_collapsed"],
        serde_json::json!(["spaces", "agents"])
    );
    assert_eq!(stored["sidebar_spaces_height_px"], serde_json::json!(180.0));
    assert_eq!(read_chrome(&path).unwrap(), chrome);

    collapsed.toggle(Section::Spaces);
    assert!(!collapsed.is(Section::Spaces));
    assert!(collapsed.is(Section::Agents));
    write_chrome(&path, Chrome::default()).unwrap();
    assert_eq!(read_chrome(&path).unwrap(), Chrome::default());
}

#[core::prelude::v1::test]
fn damaged_sections_lose_only_themselves() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    for (collapsed, expected) in [
        (r#"["tabs", "sideways", 3, null]"#, [false, true, false]),
        (r#""spaces""#, [false, false, false]),
        ("null", [false, false, false]),
        ("{}", [false, false, false]),
        (r#"["agents", "agents", "spaces"]"#, [true, false, true]),
    ] {
        fs::write(
            &path,
            format!(
                r#"{{"sidebar_width_px": 250, "sidebar_collapsed": {collapsed}, "sidebar_spaces_height_px": "tall"}}"#
            ),
        )
        .unwrap();
        let read = read_chrome(&path).unwrap();
        assert_eq!(
            Section::ALL.map(|section| read.collapsed.is(section)),
            expected,
            "{collapsed}"
        );
        assert_eq!(read.spaces_height, None, "{collapsed}");
        assert_eq!(read.sidebar_width, Some(250.), "{collapsed}");
    }
    for damaged in ["-5", "0", "null", "1e100"] {
        fs::write(
            &path,
            format!(r#"{{"sidebar_spaces_height_px": {damaged}}}"#),
        )
        .unwrap();
        assert_eq!(read_chrome(&path).unwrap().spaces_height, None, "{damaged}");
    }
    for height in [f32::NAN, f32::INFINITY, 0., -1.] {
        write_chrome(
            &path,
            Chrome {
                spaces_height: Some(height),
                ..Chrome::default()
            },
        )
        .unwrap();
        assert_eq!(read_chrome(&path).unwrap(), Chrome::default(), "{height}");
    }
}
