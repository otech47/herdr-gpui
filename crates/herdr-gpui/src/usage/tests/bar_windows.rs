use super::*;

#[test]
fn the_bar_shows_the_session_window_and_the_tightest_other() {
    let now = at(1_000_000);
    let window =
        |kind, used| Window::new(kind, used, Some(now + Duration::from_secs(10_380)), None);
    let claude = Report::new(
        provider("claude"),
        Account::default(),
        vec![
            window(Kind::Named("Fable".into()), 65.),
            window(Kind::Weekly, 40.),
            window(Kind::Session, 10.),
        ],
    );
    let shown: Vec<(&str, u32)> = claude
        .shown()
        .iter()
        .map(|window| (window.kind.short(), window.percent()))
        .collect();
    assert_eq!(shown, [("5h", 10), ("Fable", 65)]);
    assert_eq!(
        claude.shown()[0].describe(now),
        "Session: 10% used, resets in 2h 53m"
    );

    let codex = report(provider("codex"), 3.);
    assert_eq!(codex.shown().len(), 1);
    assert_eq!(codex.shown()[0].describe(at(0)), "Session: 3% used");
    assert!(
        Report::new(provider("codex"), Account::default(), vec![])
            .shown()
            .is_empty()
    );

    assert_eq!(Kind::Named("Total usage".into()).short(), "Total");
    assert_eq!(Kind::Named(" ".into()).short(), " ");
}
