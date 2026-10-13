use crate::{NavigationTarget, sidebar::layout_tests::snapshot, window::startup::shown_navigation};

#[test]
fn a_shown_target_navigates_to_its_tab_when_the_snapshot_lists_it() {
    let mut snapshot = snapshot(2);
    snapshot.focused_workspace_id = Some("w0".into());
    snapshot.focused_tab_id = Some("t0".into());
    assert_eq!(
        shown_navigation(&snapshot, "w1".into(), None),
        Some(NavigationTarget::Workspace("w1".into()))
    );
    assert_eq!(shown_navigation(&snapshot, "w0".into(), None), None);
    assert_eq!(shown_navigation(&snapshot, "w9".into(), None), None);
    assert_eq!(
        shown_navigation(&snapshot, "w0".into(), Some("t1".into())),
        Some(NavigationTarget::Tab("t1".into()))
    );
    assert_eq!(
        shown_navigation(&snapshot, "w0".into(), Some("t0".into())),
        None
    );
    assert_eq!(
        shown_navigation(&snapshot, "w1".into(), Some("t1".into())),
        Some(NavigationTarget::Workspace("w1".into()))
    );
    assert_eq!(
        shown_navigation(&snapshot, "w1".into(), Some("t9".into())),
        Some(NavigationTarget::Workspace("w1".into()))
    );
}
