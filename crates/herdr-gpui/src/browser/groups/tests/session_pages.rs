use super::*;

#[test]
fn each_session_remembers_its_page_across_a_restart() {
    let (mut layout, mut ids, _) = layout();
    layout.remember_session_page(herdr("t1"), Some(TabId::test(4)));
    layout.remember_session_page(herdr("t2"), Some(TabId::test(5)));
    let saved = layout.saved().unwrap();
    assert!(saved.valid());
    let bytes = serde_json::to_vec(&saved).unwrap();
    let saved: SavedLayout = serde_json::from_slice(&bytes).unwrap();
    let mut restored = Layout::restore(&saved, &mut ids);
    assert_eq!(restored.session_page(&herdr("t1")), Some(TabId::test(4)));
    assert_eq!(restored.session_page(&herdr("t2")), Some(TabId::test(5)));
    restored.remember_session_page(herdr("t1"), None);
    assert_eq!(restored.session_page(&herdr("t1")), None);
    assert_eq!(restored.session_page(&herdr("t2")), Some(TabId::test(5)));
    restored.forget_session_pages(|_, _| false);
    assert!(restored.saved().is_none());
}
