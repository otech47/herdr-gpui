//! The tabs list's sort is remembered with the rest of the chrome.
use super::*;

#[core::prelude::v1::test]
fn the_tabs_sort_round_trips_and_an_unknown_one_is_manual() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    for sort in [TabsSort::Manual, TabsSort::Attention] {
        let chrome = Chrome {
            tabs_sort: sort,
            ..Chrome::default()
        };
        write_chrome(&path, chrome).unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored["sidebar_tabs_sort"], serde_json::json!(sort.name()));
        assert_eq!(read_chrome(&path).unwrap(), chrome);
    }
    for damaged in [r#""priority""#, "3", "null", "[]"] {
        fs::write(
            &path,
            format!(r#"{{"sidebar_width_px": 250, "sidebar_tabs_sort": {damaged}}}"#),
        )
        .unwrap();
        let read = read_chrome(&path).unwrap();
        assert_eq!(read.tabs_sort, TabsSort::Manual, "{damaged}");
        assert_eq!(read.sidebar_width, Some(250.));
    }
    fs::write(&path, r#"{"sidebar_width_px": 250}"#).unwrap();
    assert_eq!(read_chrome(&path).unwrap().tabs_sort, TabsSort::Manual);
    assert_eq!(TabsSort::Manual.next(), TabsSort::Attention);
    assert_eq!(TabsSort::Attention.next(), TabsSort::Manual);
}
