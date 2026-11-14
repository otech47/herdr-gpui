use super::*;
use crate::browser::{Pick, Store};

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| crate::sidebar::layout_tests::full_draw(window, cx).clear(cx));
}

fn idle(cx: &mut TestAppContext) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    cx.add_window_view(|window, cx| {
        let mut view = crate::sidebar::layout_tests::fixture_window(window, cx);
        let mut shown = crate::sidebar::layout_tests::snapshot(40);
        shown.focused_workspace_id = Some("w0".into());
        shown.focused_tab_id = Some("t0".into());
        for tab in &mut shown.tabs {
            tab.agent_status = AgentStatus::Idle;
        }
        for agent in &mut shown.agents {
            agent.agent_status = AgentStatus::Idle;
        }
        view.live.snapshot = Some(Arc::new(shown));
        view
    })
}

#[gpui::test]
fn close_tab_closes_a_report_at_once_and_asks_before_an_idle_terminal(cx: &mut TestAppContext) {
    let (view, cx) = idle(cx);
    draw(cx);
    let page = cx.update(|_, cx| {
        let (scope, workspace) = view.read(cx).browser_key().unwrap();
        Store::update(cx, |store| {
            let id = store.open(scope, &workspace, None, None).unwrap();
            assert!(store.attach_to_session(id, Pick::Herdr("t0".into())));
            store.visited(id, None, Some("review"));
            id
        })
    });
    cx.update(|window, cx| view.update(cx, |view, cx| view.show_browser_tab(page, window, cx)));
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::CloseTab, window, cx);
            assert!(cx.global::<Store>().get(page).is_none());
            assert!(view.menu.close.is_none());
        })
    });
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.command(Command::CloseTab, window, cx);
            assert_eq!(view.menu.page, Some(Page::ConfirmClose));
            let close = view.menu.close.as_ref().unwrap();
            assert_eq!((close.tab.as_str(), close.error.as_ref()), ("t0", None));
        })
    });
}

#[gpui::test]
fn close_tab_asks_before_closing_a_watched_conversation(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(crate::sidebar::layout_tests::fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("watch", window, cx);
            let watched = view.active_session_tab(cx).unwrap();
            view.command(Command::CloseTab, window, cx);
            assert_eq!(view.menu.close.as_ref().unwrap().page, Some(watched));
            assert!(cx.global::<Store>().get(watched).is_some());
            view.send_close(window, cx);
            assert!(cx.global::<Store>().get(watched).is_none());
            assert!(view.menu.page.is_none());
        })
    });
}
