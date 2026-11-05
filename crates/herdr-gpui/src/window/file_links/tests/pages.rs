use super::*;
use crate::browser::{Location, Store};

#[gpui::test]
fn markdown_click_opens_a_page_in_its_source_session(cx: &mut gpui::TestAppContext) {
    if !crate::browser::EMBEDDED {
        return;
    }
    let root = tempfile::Builder::new()
        .prefix("herdr-document-")
        .tempdir()
        .unwrap();
    let page = root.path().join("notes.md");
    std::fs::write(&page, "# Notes").unwrap();
    let (view, cx) = window("read ./notes.md", root.path(), cx);
    let position = at(&view, cx, 9);
    let source = view.read_with(cx, |view, _| {
        let pane = &view.live.snapshot.as_ref().unwrap().panes[0].pane_id;
        view.page_source_for_pane(pane).unwrap()
    });
    cx.simulate_click(position, Modifiers::secondary_key());
    cx.run_until_parked();
    assert!(cx.opened_url().is_none());
    cx.update(|_, cx| {
        Store::update(cx, |store| {
            let tabs: Vec<_> = store
                .session_pages(&source.scope, &source.workspace, &source.session)
                .collect();
            assert_eq!(tabs.len(), 1);
            assert_eq!(tabs[0].origin, source.pane);
            let Some(Location::Local { file }) = &tabs[0].location else {
                panic!("missing document")
            };
            assert_eq!(file.path(), real_path(&page).unwrap());
        })
    });
}

#[gpui::test]
fn remote_markdown_click_keeps_the_path_even_when_daemon_resolves_it(
    cx: &mut gpui::TestAppContext,
) {
    if !crate::browser::EMBEDDED || !cfg!(unix) {
        return;
    }
    let (view, cx) = window(
        "read /home/user/.agent-scratch/reply-draft.md",
        Path::new("/home/user"),
        cx,
    );
    let position = at(&view, cx, 9);
    view.update(cx, |view, _| {
        view.endpoints[view.selected_endpoint].connection.target = ConnectTarget::Ssh {
            target: "unused".into(),
            session: "default".into(),
        };
        view.live.supports_link_activate = true;
        assert!(view.local_link_at(position).is_some());
        let pressed = view
            .terminal_link_press(position, Modifiers::secondary_key())
            .unwrap();
        assert_eq!(
            pressed.file.unwrap().path,
            "/home/user/.agent-scratch/reply-draft.md"
        );
        assert!(pressed.cell.is_none());
        assert!(
            view.terminal_link_press(position, Modifiers::default())
                .is_none()
        );
    });
}
