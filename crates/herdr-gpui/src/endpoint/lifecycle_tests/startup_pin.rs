use super::*;
use crate::{endpoint::tests::host, window::Startup};

#[gpui::test]
fn a_window_opened_for_a_space_stays_off_the_last_chosen_device(cx: &mut gpui::TestAppContext) {
    let (fixture, cx) = cx.add_window_view(|window, cx| {
        Fixture(cx.new(|cx| crate::sidebar::layout_tests::fixture_window(window, cx)))
    });
    let view = fixture.update(cx, |fixture, _| fixture.0.clone());
    let (remote, _server) = connected_endpoint("ssh:saved");
    view.update(cx, |view, cx| {
        view.open_for(Startup::Show {
            workspace: "w0".into(),
            tab: None,
            device: None,
        });
        view.endpoints.push(remote);
        view.catalog.accept(&catalog::CatalogUpdate {
            hosts: vec![host("saved", true)],
            selection: Some(Some("saved".into())),
        });
        assert!(view.catalog_loaded());
        for _ in 0..3 {
            view.restore_selection(cx);
            assert_eq!(view.selected_endpoint, 0);
            assert!(!view.catalog.restore_pending);
        }
        assert!(view.select_endpoint("ssh:saved", cx));
        assert_eq!(view.selected_endpoint, 1);
        assert_eq!(view.catalog.desired.as_deref(), Some("saved"));
    });
}
