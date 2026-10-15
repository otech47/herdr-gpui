//! The tabs list's view is remembered with the rest of the chrome.
use super::*;

#[core::prelude::v1::test]
fn the_tabs_view_round_trips_and_an_unknown_one_is_the_default() {
    let directory = TestDirectory::new();
    let path = directory.0.join("preferences.json");
    for view in [TabsView::Device, TabsView::All, TabsView::ByDevice] {
        let chrome = Chrome {
            tabs_view: view,
            ..Chrome::default()
        };
        write_chrome(&path, chrome).unwrap();
        let stored: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(stored["sidebar_tabs_view"], serde_json::json!(view.name()));
        assert_eq!(read_chrome(&path).unwrap(), chrome);
    }
    for damaged in [r#""grouped""#, "3", "null", "[]"] {
        fs::write(
            &path,
            format!(r#"{{"sidebar_width_px": 250, "sidebar_tabs_view": {damaged}}}"#),
        )
        .unwrap();
        let read = read_chrome(&path).unwrap();
        assert_eq!(read.tabs_view, TabsView::Device, "{damaged}");
        assert_eq!(read.sidebar_width, Some(250.));
    }
    fs::write(&path, r#"{"sidebar_width_px": 250}"#).unwrap();
    assert_eq!(read_chrome(&path).unwrap().tabs_view, TabsView::Device);
    assert_eq!(TabsView::Device.next(), TabsView::All);
    assert_eq!(TabsView::All.next(), TabsView::ByDevice);
    assert_eq!(TabsView::ByDevice.next(), TabsView::Device);
}
