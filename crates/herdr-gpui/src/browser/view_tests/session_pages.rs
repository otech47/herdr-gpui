use super::*;
#[cfg(unix)]
use crate::browser::Shown;
use crate::browser::{Pick, TabId};

fn add_page(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, session: Pick) -> TabId {
    cx.update(|_, cx| {
        let (scope, workspace) = view.read(cx).browser_key().unwrap();
        Store::update(cx, |store| {
            let id = store.open(scope, &workspace, None, None).unwrap();
            assert!(store.attach_to_session(id, session));
            store.visited(id, None, Some("navigation review"));
            id
        })
    })
}

fn pick(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Pick> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.group_pick(view.active_group().unwrap())
    })
}

fn choose(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext, tab: &str) {
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.choose_herdr_tab(view.active_group().unwrap(), tab, window, cx);
        })
    });
    draw(cx);
}

#[gpui::test]
fn session_pages_restore_the_last_page_and_an_explicit_terminal_choice(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = window(cx);
    draw(cx);
    let page = add_page(&view, cx, Pick::Herdr("t0".into()));
    cx.update(|window, cx| view.update(cx, |view, cx| view.show_browser_tab(page, window, cx)));
    draw(cx);
    assert!(cx.debug_bounds("session-pages").is_some());
    assert!(cx.debug_bounds("session-page-0").is_some());
    assert!(cx.debug_bounds("browser-tab-0").is_none());
    choose(&view, cx, "t1");
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t1".into())));
    assert!(cx.debug_bounds("session-pages").is_some());
    assert!(cx.debug_bounds("session-page-0").is_none());
    choose(&view, cx, "t0");
    assert_eq!(pick(&view, cx), Some(Pick::Page(page)));
    let terminal = cx.debug_bounds("session-terminal").unwrap();
    cx.simulate_click(terminal.center(), gpui::Modifiers::none());
    draw(cx);
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t0".into())));
    choose(&view, cx, "t1");
    choose(&view, cx, "t0");
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t0".into())));
}

#[gpui::test]
fn closing_a_session_page_never_selects_another_sessions_report(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let mine = add_page(&view, cx, Pick::Herdr("t0".into()));
    let other = add_page(&view, cx, Pick::Herdr("t1".into()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_browser_tab(mine, window, cx);
            view.close_browser_tab(mine, window, cx);
        })
    });
    draw(cx);
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t0".into())));
    assert!(cx.update(|_, cx| cx.global::<Store>().get(other).is_some()));
}

#[gpui::test]
fn the_first_report_does_not_resize_the_terminal(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let before = cx.debug_bounds("terminal").unwrap();
    add_page(&view, cx, Pick::Herdr("t0".into()));
    draw(cx);
    assert_eq!(cx.debug_bounds("terminal").unwrap(), before);
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t0".into())));
}

#[gpui::test]
fn closing_the_source_session_keeps_its_report_accessible(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let page = add_page(&view, cx, Pick::Herdr("t0".into()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.show_browser_tab(page, window, cx);
            Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                .tabs
                .retain(|tab| tab.tab_id != "t0");
            view.forget_closed_herdr_tabs(cx);
            assert!(cx.global::<Store>().get(page).unwrap().session.is_none());
        })
    });
    draw(cx);
    assert_eq!(pick(&view, cx), Some(Pick::Page(page)));
    assert!(cx.debug_bounds("browser-tab-0").is_some());
}

#[gpui::test]
#[cfg(unix)]
fn device_names_never_bind_a_remote_report_to_local_matching_ids(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            let mut remote = crate::endpoint::Endpoint::new(
                "ssh:remote".into(),
                "Mini".into(),
                herdr_client::ConnectTarget::Ssh {
                    target: "unused".into(),
                    session: "default".into(),
                },
                true,
            );
            remote.live = view.live.clone();
            Arc::make_mut(remote.live.snapshot.as_mut().unwrap())
                .panes
                .push(herdr_client::protocol::ClientShellPane {
                    pane_id: "p1".into(),
                    workspace_id: "w0".into(),
                    tab_id: "t1".into(),
                    label: None,
                    cwd: None,
                    foreground_cwd: None,
                    focused: false,
                    right_click_passthrough: false,
                });
            view.endpoints.push(remote);
            let target = crate::control::Target {
                daemon: None,
                workspace: Some("w0"),
                pane: Some("p1"),
                device: Some("Mini"),
            };
            assert!(
                view.open_requested_browser_tab(
                    &target,
                    true,
                    &url("https://example.test/report.html"),
                    false,
                    window,
                    cx
                )
                .is_some()
            );
            let tab = cx.global::<Store>().get(TabId::test(0)).unwrap();
            assert_eq!(tab.scope, crate::browser::Scope::endpoint("ssh:remote"));
            assert_eq!(tab.session, Some(Pick::Herdr("t1".into())));
            view.endpoints[0].label = "Mini".into();
            assert!(
                view.open_requested_browser_tab(
                    &target,
                    true,
                    &url("https://example.test/second.html"),
                    false,
                    window,
                    cx
                )
                .is_none()
            );
        })
    });
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t0".into())));
}

#[gpui::test]
fn session_pages_remain_available_with_the_global_tab_strip_hidden(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    let page = add_page(&view, cx, Pick::Herdr("t0".into()));
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.config.show_tab_strip = false;
            view.show_browser_tab(page, window, cx);
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("session-pages").is_some());
    assert!(cx.debug_bounds("session-terminal").is_some());
    let bar = cx.debug_bounds("session-pages").unwrap();
    let browser = cx.debug_bounds("browser").unwrap();
    assert!(bar.bottom() <= browser.top() + gpui::px(1.));
}

#[gpui::test]
#[cfg(unix)]
fn a_background_page_request_attaches_to_its_pane_without_switching(cx: &mut gpui::TestAppContext) {
    let (view, cx) = window(cx);
    draw(cx);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            Arc::make_mut(view.live.snapshot.as_mut().unwrap())
                .panes
                .push(herdr_client::protocol::ClientShellPane {
                    pane_id: "p1".into(),
                    workspace_id: "w0".into(),
                    tab_id: "t1".into(),
                    label: None,
                    cwd: None,
                    foreground_cwd: None,
                    focused: false,
                    right_click_passthrough: false,
                });
            let target = crate::control::Target {
                daemon: None,
                workspace: Some("w0"),
                pane: Some("p1"),
                device: None,
            };
            assert!(matches!(
                view.open_requested_browser_tab(
                    &target,
                    true,
                    &url("https://example.test/report.html"),
                    false,
                    window,
                    cx
                ),
                Some(crate::control::Placed::Opened { .. })
            ));
        })
    });
    draw(cx);
    assert_eq!(pick(&view, cx), Some(Pick::Herdr("t0".into())));
    cx.update(|_, cx| {
        let view = view.read(cx);
        assert_eq!(
            view.group_shown(view.active_group().unwrap(), cx),
            Shown::Terminal
        );
        assert_eq!(
            cx.global::<Store>().get(TabId::test(0)).unwrap().session,
            Some(Pick::Herdr("t1".into()))
        );
    });
}
