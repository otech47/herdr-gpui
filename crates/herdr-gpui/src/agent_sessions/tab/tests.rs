#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::TestAppContext;

#[gpui::test]
fn running_then_ready_requires_a_click_and_errors_keep_output(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("watch", window, cx);
            let id = view.active_session_tab(cx).unwrap();
            let source = view.agent_sessions.as_ref().unwrap().rows[0].host.clone();
            let tab = view.session_tabs.get_mut(&id).unwrap();
            tab.sender
                .send(Update::Watched(Ok((
                    source,
                    Observation {
                        state: Activity::Ready,
                        output: "Finished checks.".into(),
                    },
                ))))
                .unwrap();
            view.poll_session_tabs(window, cx);
            let tab = &view.session_tabs[&id];
            assert!(tab.ready());
            assert!(!tab.opening);
            assert!(!tab.auto_continue);
            tab.sender
                .send(Update::Watched(Err(Error::Refused("offline".into()))))
                .unwrap();
            view.poll_session_tabs(window, cx);
            let tab = &view.session_tabs[&id];
            assert!(!tab.ready());
            assert_eq!(tab.output, "Finished checks.");
            view.continue_session_tab(id, cx);
            assert!(!view.session_tabs[&id].opening);
        })
    });
}

#[gpui::test]
fn restored_tabs_never_resume_automatically_and_closed_tabs_stop_polling(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("watch", window, cx);
            let id = view.active_session_tab(cx).unwrap();
            let location = cx
                .global::<Store>()
                .get(id)
                .unwrap()
                .location
                .clone()
                .unwrap();
            let bytes = serde_json::to_vec(&location).unwrap();
            assert_eq!(
                serde_json::from_slice::<Location>(&bytes).unwrap(),
                location
            );
            let cancelled = view.session_tabs[&id].cancel.clone();
            view.session_tabs.clear();
            assert!(cancelled.load(Ordering::Relaxed));
            for endpoint in &mut view.endpoints {
                endpoint.enabled = false;
            }
            view.poll_session_tabs(window, cx);
            assert_eq!(view.session_tabs.len(), 1);
            let tab = &view.session_tabs[&id];
            assert!(!tab.auto_continue);
            assert!(!tab.opening);
            let cancelled = tab.cancel.clone();
            view.close_browser_tab(id, window, cx);
            view.poll_session_tabs(window, cx);
            assert!(view.session_tabs.is_empty());
            assert!(cancelled.load(Ordering::Relaxed));
        })
    });
}
