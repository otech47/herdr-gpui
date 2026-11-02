#![allow(clippy::unwrap_used)]
use super::*;
use crate::sidebar::layout_tests::fixture_window;
use gpui::{EntityInputHandler, TestAppContext, px, size};

#[gpui::test]
fn watching_ignores_typing_paste_and_interrupt_keys_even_when_ready(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(700.), px(640.)));
    for mode in ["watch", "ready"] {
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
                view.preview_agent_sessions(mode, window, cx);
            })
        });
        cx.update(|window, cx| {
            window.draw(cx).clear(cx);
        });
        for key in [
            "a",
            "enter",
            "ctrl-c",
            "ctrl-d",
            "cmd-v",
            "shift-tab",
            "up",
            "down",
        ] {
            cx.simulate_keystrokes(key);
        }
        cx.update(|window, cx| {
            view.update(cx, |view, cx| {
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
            })
        });
        view.read_with(cx, |view, _| {
            assert_eq!(view.menu.page, Some(Page::AgentSessions));
            assert!(view.menu.input.is_none());
            let picker = view.menu.agent_sessions.as_ref().unwrap();
            assert!(!picker.opening);
            assert!(picker.watch.is_some());
            assert!(picker.receiver.try_recv().is_err());
        });
    }
}

#[gpui::test]
fn session_picker_fits_narrow_windows_and_filters_all_context(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.simulate_resize(size(px(520.), px(640.)));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("list", window, cx);
            let picker = view.menu.agent_sessions.as_mut().unwrap();
            picker.filter("mini jarvis mobile");
            assert_eq!(picker.filtered.len(), 1);
        })
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let row = cx.debug_bounds("agent-session-0").unwrap();
    assert!(row.left() >= px(0.) && row.right() <= px(520.));
    cx.simulate_keystrokes("enter");
    view.read_with(cx, |view, _| {
        let picker = view.menu.agent_sessions.as_ref().unwrap();
        assert!(picker.watch.is_some());
        assert!(!picker.opening);
    });
}

#[gpui::test]
fn watch_errors_remove_permission_to_continue(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("ready", window, cx);
            let picker = view.menu.agent_sessions.as_mut().unwrap();
            picker
                .sender
                .send(Reply::Watched(Err(Error::Refused("offline".into()))))
                .unwrap();
            view.poll_agent_sessions(window, cx);
            view.continue_agent_session(cx);
            let picker = view.menu.agent_sessions.as_ref().unwrap();
            assert!(!picker.opening);
            assert!(picker.watch.as_ref().unwrap().observation.is_none());
        })
    });
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
            modified: 0.,
        },
    };
    let sources = [local, remote];
    let (owner, observation) = observe(&row, &sources, |source| {
        Ok(Observation {
            state: if source.id == "remote" {
                Activity::Watch
            } else {
                Activity::Ready
            },
            output: "".into(),
        })
    })
    .unwrap();
    assert_eq!(owner.id, "remote");
    assert!(!observation.state.can_open());
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
