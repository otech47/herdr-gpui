#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::{EntityInputHandler, TestAppContext, px, size};

#[gpui::test]
fn watch_is_a_persistent_tab_and_ignores_input_even_when_ready(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(900.), px(700.)));
    for mode in ["watch", "ready"] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| view.preview_agent_sessions(mode, window, cx))
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        for key in ["a", "enter", "ctrl-c", "ctrl-d", "cmd-v", "up", "down"] {
            cx.simulate_keystrokes(key);
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                assert!(view.menu.page.is_none());
                let id = view.active_session_tab(cx).unwrap();
                assert_eq!(view.session_tabs.len(), 1);
                assert!(!view.session_tabs[&id].opening);
                EntityInputHandler::replace_text_in_range(
                    view,
                    None,
                    "accidental paste",
                    window,
                    cx,
                );
                EntityInputHandler::replace_and_mark_text_in_range(
                    view,
                    None,
                    "composition",
                    None,
                    window,
                    cx,
                );
                view.send(
                    herdr_client::protocol::ClientPaneInputEvent::TextCommit(
                        "accidental input".into(),
                    ),
                    cx,
                );
                assert!(view.marked.is_empty());
                #[cfg(unix)]
                assert_eq!(view.pending_input.len(), 0);
            })
        });
    }
}

#[gpui::test]
fn picker_keeps_search_and_watch_when_closed_and_reopened(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(800.), px(650.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("watch", window, cx);
            let id = view.active_session_tab(cx).unwrap();
            view.open_agent_sessions(window, cx);
            let picker = view.agent_sessions.as_mut().unwrap();
            picker.search.update(cx, |search, cx| {
                search.set_text_selected("mini jarvis mobile", cx)
            });
            picker.filter("mini jarvis mobile");
            assert_eq!(picker.filtered.len(), 1);
            view.dismiss_menu(window, cx);
            assert_eq!(view.active_session_tab(cx), Some(id));
            view.open_agent_sessions(window, cx);
            assert_eq!(
                view.agent_sessions.as_ref().unwrap().search.read(cx).text(),
                "mini jarvis mobile"
            );
            assert_eq!(view.agent_sessions.as_ref().unwrap().rows.len(), 4);
            view.open_agent_session(0, window, cx);
            assert_eq!(view.active_session_tab(cx), Some(id));
            assert_eq!(view.session_tabs.len(), 1);
        })
    });
}

#[gpui::test]
fn ready_watch_fits_narrow_window_with_action_and_readonly_status(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(650.), px(640.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.sidebar_visible = false;
            view.preview_agent_sessions("ready", window, cx);
        })
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let action = cx.debug_bounds("session-continue").unwrap();
    let status = cx.debug_bounds("session-watch-status").unwrap();
    assert!(action.left() >= px(0.) && action.right() <= px(650.));
    assert!(status.bottom() < px(640.));
}

#[test]
fn running_or_unverified_activity_cannot_open_a_tab() {
    assert!(!Activity::Watch.can_open());
    assert!(!Activity::Unknown.can_open());
    assert!(!Activity::Missing.can_open());
    assert!(Activity::Ready.can_open());
    assert!(Activity::Attached.can_open());
}

#[test]
fn synced_sessions_follow_the_running_host_and_refuse_unverified_peers() {
    let host = crate::teleport::host_for(&ConnectTarget::Local).unwrap();
    let local = Source {
        id: "local".into(),
        label: "This Mac".into(),
        generation: 0,
        host: host.clone(),
        cli: vec![],
    };
    let remote = Source {
        id: "remote".into(),
        label: "Mini".into(),
        generation: 0,
        host,
        cli: vec![],
    };
    let row = Row {
        host: local.clone(),
        entry: Entry {
            agent: "claude".into(),
            session: "same-session".into(),
            title: "Shared transcript".into(),
            cwd: "/project".into(),
            source: "Saved session".into(),
            modified: 0,
            running: false,
        },
    };
    let sources = [local, remote];
    for local in [Activity::Ready, Activity::Attached, Activity::Unknown] {
        let (owner, observation) = observe(&row, &sources, |source| {
            Ok(Observation {
                state: if source.id == "remote" {
                    Activity::Watch
                } else {
                    local
                },
                output: "".into(),
            })
        })
        .unwrap();
        assert_eq!(owner.id, "remote");
        assert!(!observation.state.can_open());
    }
    let (owner, observation) = observe(&row, &sources, |source| {
        if source.id == "local" {
            Err(Error::Refused("offline".into()))
        } else {
            Ok(Observation {
                state: Activity::Watch,
                output: "Still working.".into(),
            })
        }
    })
    .unwrap();
    assert_eq!(owner.id, "remote");
    assert!(!observation.state.can_open());
    assert_eq!(observation.output, "Still working.");
    let result = observe(&row, &sources, |source| {
        if source.id == "remote" {
            Err(Error::Refused("offline".into()))
        } else {
            Ok(Observation {
                state: Activity::Ready,
                output: "".into(),
            })
        }
    });
    assert!(matches!(result, Err(Error::Refused(_))));
}
