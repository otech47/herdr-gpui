use super::*;

#[gpui::test]
fn watched_reports_arrive_without_switching_and_closed_pages_stay_closed(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("watch", window, cx);
            let parent = view.active_session_tab(cx).unwrap();
            let source = view.agent_sessions.as_ref().unwrap().rows[0].host.clone();
            let observation = Observation {
                state: Activity::Watch,
                output: "[review](http://host.tail123.ts.net:8788/report.html)".into(),
            };
            view.session_tabs[&parent]
                .sender
                .send(Update::Watched(Ok((source.clone(), observation.clone()))))
                .unwrap();
            view.poll_session_tabs(window, cx);
            assert_eq!(
                view.group_pick(view.active_group().unwrap()),
                Some(Pick::Page(parent))
            );
            let (scope, workspace) = view.browser_key().unwrap();
            let child = cx
                .global::<Store>()
                .session_pages(&scope, &workspace, &Pick::Page(parent))
                .next()
                .unwrap()
                .id;
            view.close_browser_tab(child, window, cx);
            view.session_tabs[&parent]
                .sender
                .send(Update::Watched(Ok((source, observation))))
                .unwrap();
            view.poll_session_tabs(window, cx);
            assert_eq!(
                cx.global::<Store>()
                    .session_pages(&scope, &workspace, &Pick::Page(parent))
                    .count(),
                0
            );
            let Some(Location::AgentSession { session }) =
                &cx.global::<Store>().get(parent).unwrap().location
            else {
                panic!("watch lost its session");
            };
            let bytes = serde_json::to_vec(session).unwrap();
            let restored: SavedSession = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(restored.reports.len(), 1);
        })
    });
}

#[gpui::test]
fn continuing_a_watch_waits_for_its_pane_then_moves_its_pages(cx: &mut TestAppContext) {
    let (view, cx) = cx.add_window_view(fixture_window);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.preview_agent_sessions("ready", window, cx);
            let parent = view.active_session_tab(cx).unwrap();
            let source = view.agent_sessions.as_ref().unwrap().rows[0].host.clone();
            let (scope, workspace) = view.browser_key().unwrap();
            let page = Store::update(cx, |store| {
                let id = store.open(scope.clone(), &workspace, None, None).unwrap();
                assert!(store.attach_to_session(id, Pick::Page(parent)));
                id
            });
            view.session_tabs[&parent]
                .sender
                .send(Update::Opened(Ok((
                    source,
                    OpenResult {
                        opened: Destination {
                            pane_id: "new-pane".into(),
                        },
                    },
                ))))
                .unwrap();
            view.poll_session_tabs(window, cx);
            assert!(cx.global::<Store>().get(parent).is_some());
            assert!(!view.session_tabs[&parent].ready());
            let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
            snapshot
                .panes
                .push(herdr_client::protocol::ClientShellPane {
                    pane_id: "new-pane".into(),
                    workspace_id: workspace.clone(),
                    tab_id: "t0".into(),
                    label: None,
                    cwd: None,
                    foreground_cwd: None,
                    focused: true,
                    right_click_passthrough: false,
                });
            view.poll_session_tabs(window, cx);
            let store = cx.global::<Store>();
            assert!(store.get(parent).is_none());
            let page = store.get(page).unwrap();
            assert_eq!(page.session, Some(Pick::Herdr("t0".into())));
            assert_eq!(page.origin.as_deref(), Some("new-pane"));
            assert_eq!(page.scope, scope);
        })
    });
}
