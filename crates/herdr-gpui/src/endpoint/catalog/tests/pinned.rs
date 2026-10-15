use super::*;

#[test]
fn a_pinned_window_ignores_the_shared_selection_but_still_writes_a_click() {
    let update = |selection| CatalogUpdate {
        hosts: vec![host("a", true)],
        selection,
    };
    let mut catalog = Catalog::new(&ConnectTarget::Local);
    catalog.pin();
    catalog.accept(&update(Some(Some("a".into()))));
    assert!(catalog.initialized);
    assert!(!catalog.restore_pending);
    assert_eq!(catalog.desired.as_deref(), Some("a"));
    catalog.choose("ssh:a");
    assert_eq!(catalog.queued_write, Some(Some("a".into())));
    assert!(!catalog.restore_pending);
    let mut late = Catalog::new(&ConnectTarget::Local);
    late.accept(&update(Some(Some("a".into()))));
    assert!(late.restore_pending);
    late.pin();
    assert!(!late.restore_pending);
}
