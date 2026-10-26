use super::*;

// Under `/tmp`, so the address fits the fixture's sixty-column row.
#[cfg(unix)]
#[gpui::test]
fn a_link_modifier_click_opens_a_printed_file_url(cx: &mut gpui::TestAppContext) {
    let root = tempfile::tempdir_in("/tmp").unwrap();
    let page = root.path().join("report.html");
    std::fs::write(&page, "<p>hi</p>").unwrap();
    let url = url::Url::from_file_path(&page).unwrap();
    let text = format!("report: {url} done");
    let (view, cx) = window(&text, root.path(), cx);
    let position = at(&view, cx, 12);

    view.read_with(cx, |view, _| {
        assert!(view.terminal_link_at(position).is_none());
        assert!(!view.terminal_link_hovered(position, Modifiers::default()));
        assert!(view.terminal_link_hovered(position, Modifiers::secondary_key()));
    });
    cx.simulate_mouse_move(position, None, Modifiers::secondary_key());
    view.read_with(cx, |view, _| {
        let link = view.hovered_local_link().unwrap();
        let columns = 8..8 + u16::try_from(url.as_str().len()).unwrap();
        assert_eq!((link.row, link.columns.clone()), (0, columns));
    });

    cx.simulate_click(position, Modifiers::secondary_key());
    cx.run_until_parked();
    let real = real_path(&page).unwrap();
    let expected = url::Url::from_file_path(real).unwrap();
    assert_eq!(cx.opened_url().as_deref(), Some(expected.as_str()));
}
