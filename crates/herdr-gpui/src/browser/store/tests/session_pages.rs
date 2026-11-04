use super::*;

#[test]
fn session_pages_keep_device_and_workspace_ownership_when_restored() {
    let mut store = Store::default();
    let local = Scope::endpoint("local");
    let remote = Scope::endpoint("ssh:other");
    let session = Pick::Herdr("t1".into());
    let a = store
        .open(
            local.clone(),
            "w1",
            url("https://a.test/report.html"),
            Some("p1".into()),
        )
        .unwrap();
    let b = store
        .open(
            remote.clone(),
            "w1",
            url("https://b.test/report.html"),
            Some("p1".into()),
        )
        .unwrap();
    assert!(store.attach_to_session(a, session.clone()));
    assert!(store.attach_to_session(b, session.clone()));
    assert_eq!(
        store
            .session_pages(&local, "w1", &session)
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        [a]
    );
    assert_eq!(
        store
            .session_pages(&remote, "w1", &session)
            .map(|t| t.id)
            .collect::<Vec<_>>(),
        [b]
    );
    let bytes = serde_json::to_vec(&Saved {
        tabs: store.tabs.clone(),
    })
    .unwrap();
    let restored = Store::with_tabs(parse(&bytes).unwrap(), None);
    assert_eq!(restored.tabs, store.tabs);
    store.detach_session(&local, "w1", &session);
    assert!(store.get(a).unwrap().session.is_none());
    assert_eq!(store.get(b).unwrap().session.as_ref(), Some(&session));
}

#[test]
fn reports_cannot_be_parents_and_legacy_tabs_still_load() {
    let mut store = Store::default();
    let scope = Scope::endpoint("local");
    let a = store
        .open(scope.clone(), "w1", url("https://a.test/"), None)
        .unwrap();
    let b = store.open(scope, "w1", None, None).unwrap();
    assert!(!store.attach_to_session(b, Pick::Page(a)));
    assert!(!store.attach_to_session(b, Pick::Page(b)));
    assert!(!store.attach_to_session(b, Pick::Herdr(String::new())));
    let old = br#"{"tabs":[{"id":0,"scope":"local","workspace_id":"w1","location":null,"title":"New Tab"}]}"#;
    assert!(parse(old).unwrap()[0].session.is_none());
}
