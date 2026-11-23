use super::*;
use crate::{
    agent_models::{Report, Source},
    browser::{Pick, TabId},
    menu::Page,
};
use herdr_client::ConnectTarget;
use std::collections::HashMap;

const OLD: &str = "http://localhost:8788/scratch/old-plan.html";
const NEW: &str = "http://localhost:8788/scratch/layouts.html";

fn report(url: &str, title: &str, at: u64) -> Report {
    Report {
        url: url.into(),
        title: title.into(),
        at,
    }
}

/// The fixture window on the local daemon, whose agent in pane `p0` of tab
/// `t0` linked `reports`, newest first.
fn linked(
    cx: &mut gpui::TestAppContext,
    reports: Vec<Report>,
) -> (Entity<HerdrWindow>, &mut VisualTestContext) {
    let (view, cx) = window(cx);
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.endpoints[0] = crate::endpoint::Endpoint::new(
                crate::endpoint::LOCAL.into(),
                "Local".into(),
                ConnectTarget::Local,
                true,
            );
            view.agent_models.seed(
                Source::of(&ConnectTarget::Local).unwrap(),
                HashMap::new(),
                HashMap::from([("p0".to_owned(), reports)]),
            );
        })
    });
    draw(cx);
    (view, cx)
}

fn two() -> Vec<Report> {
    vec![
        report(NEW, "Report list layouts", 200),
        report(OLD, "", 100),
    ]
}

fn press(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).unwrap();
    cx.simulate_click(bounds.center(), gpui::Modifiers::none());
    draw(cx);
}

fn session_pages(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Vec<(TabId, String)> {
    cx.update(|_, cx| {
        let (scope, workspace) = view.read(cx).browser_key().unwrap();
        let session = Pick::Herdr("t0".into());
        cx.try_global::<Store>()
            .into_iter()
            .flat_map(|store| store.session_pages(&scope, &workspace, &session))
            .filter_map(|tab| match &tab.location {
                Some(Location::Web { url }) => Some((tab.id, url.as_str().to_owned())),
                _ => None,
            })
            .collect()
    })
}

fn active_pick(view: &Entity<HerdrWindow>, cx: &mut VisualTestContext) -> Option<Pick> {
    cx.update(|_, cx| {
        let view = view.read(cx);
        view.group_pick(view.active_group().unwrap())
    })
}

#[gpui::test]
fn the_strip_has_a_reports_button_only_for_a_session_that_linked_one(
    cx: &mut gpui::TestAppContext,
) {
    let (view, cx) = linked(cx, Vec::new());
    assert!(cx.debug_bounds("session-pages").is_some());
    assert!(cx.debug_bounds("session-reports").is_none());
    cx.update(|_, cx| {
        view.update(cx, |view, _| {
            view.agent_models.seed(
                Source::of(&ConnectTarget::Local).unwrap(),
                HashMap::new(),
                HashMap::from([("p0".to_owned(), two())]),
            );
        })
    });
    draw(cx);
    let strip = cx.debug_bounds("session-pages").unwrap();
    let button = cx.debug_bounds("session-reports").unwrap();
    assert!(button.right() <= strip.right() + gpui::px(1.));
    assert!(button.left() >= cx.debug_bounds("session-terminal").unwrap().right());
    // Tab `t1` holds no agent, so nothing was linked from it.
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.choose_herdr_tab(view.active_group().unwrap(), "t1", window, cx);
        })
    });
    draw(cx);
    assert!(cx.debug_bounds("session-reports").is_none());
}

#[gpui::test]
fn the_button_stays_in_view_when_the_strip_overflows(cx: &mut gpui::TestAppContext) {
    let (view, cx) = linked(cx, two());
    cx.update(|_, cx| {
        let (scope, workspace) = view.read(cx).browser_key().unwrap();
        Store::update(cx, |store| {
            for index in 0..24 {
                let id = store.open(scope.clone(), &workspace, None, None).unwrap();
                assert!(store.attach_to_session(id, Pick::Herdr("t0".into())));
                store.visited(id, None, Some(&format!("a long report title {index}")));
            }
        })
    });
    draw(cx);
    let strip = cx.debug_bounds("session-pages").unwrap();
    let button = cx.debug_bounds("session-reports").unwrap();
    assert!(button.left() >= strip.left() && button.right() <= strip.right() + gpui::px(1.));
}

#[gpui::test]
fn the_list_holds_every_report_and_marks_the_open_ones(cx: &mut gpui::TestAppContext) {
    let (view, cx) = linked(cx, two());
    cx.update(|_, cx| {
        let (scope, workspace) = view.read(cx).browser_key().unwrap();
        Store::update(cx, |store| {
            let id = store.open(scope, &workspace, Some(url(OLD)), None).unwrap();
            assert!(store.attach_to_session(id, Pick::Herdr("t0".into())));
        })
    });
    draw(cx);
    let listed = cx.update(|_, cx| {
        view.read(cx)
            .session_reports(&Pick::Herdr("t0".into()), cx)
            .into_iter()
            .map(|entry| (entry.title.to_string(), entry.open))
            .collect::<Vec<_>>()
    });
    assert_eq!(
        listed,
        [
            ("Report list layouts".to_owned(), false),
            ("old-plan.html".to_owned(), true)
        ]
    );
    press(cx, "session-reports");
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, Some(Page::Reports)));
    let first = cx.debug_bounds("session-report-0").unwrap();
    let second = cx.debug_bounds("session-report-1").unwrap();
    assert!(first.bottom() <= second.top() + gpui::px(1.));
    assert!(cx.debug_bounds("session-report-2").is_none());
    cx.simulate_keystrokes("escape");
    draw(cx);
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, None));
    assert!(cx.debug_bounds("session-report-0").is_none());
}

#[gpui::test]
fn picking_a_report_shows_it_as_a_page_of_its_session(cx: &mut gpui::TestAppContext) {
    let (view, cx) = linked(cx, two());
    assert!(session_pages(&view, cx).is_empty());
    press(cx, "session-reports");
    press(cx, "session-report-1");
    let pages = session_pages(&view, cx);
    assert_eq!(pages.len(), 1);
    assert_eq!(pages[0].1, OLD);
    assert_eq!(active_pick(&view, cx), Some(Pick::Page(pages[0].0)));
    cx.update(|_, cx| assert_eq!(view.read(cx).menu.page, None));
    assert!(cx.debug_bounds("session-page-0").is_some());

    // The same report again goes to the page it already has.
    press(cx, "session-terminal");
    assert_eq!(active_pick(&view, cx), Some(Pick::Herdr("t0".into())));
    press(cx, "session-reports");
    cx.simulate_keystrokes("down down enter");
    draw(cx);
    assert_eq!(session_pages(&view, cx), pages);
    assert_eq!(active_pick(&view, cx), Some(Pick::Page(pages[0].0)));

    // The button is still there while a page of the session is in view.
    press(cx, "session-reports");
    cx.simulate_keystrokes("down enter");
    draw(cx);
    let pages = session_pages(&view, cx);
    assert_eq!(pages.len(), 2);
    assert_eq!(pages[1].1, NEW);
    assert_eq!(active_pick(&view, cx), Some(Pick::Page(pages[1].0)));
}

#[gpui::test]
fn only_addresses_the_app_shows_as_reports_are_listed(cx: &mut gpui::TestAppContext) {
    let (view, cx) = linked(
        cx,
        vec![
            report("http://localhost:8788/scratch/data.json", "", 5),
            report("https://example.com/page.html", "", 4),
            report("not an address", "", 3),
            report(OLD, "  Old\n plan  ", 2),
            report(OLD, "Old plan", 1),
        ],
    );
    let listed = cx.update(|_, cx| {
        view.read(cx)
            .session_reports(&Pick::Herdr("t0".into()), cx)
            .into_iter()
            .map(|entry| (entry.url.as_str().to_owned(), entry.title.to_string()))
            .collect::<Vec<_>>()
    });
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].0, OLD);
}

#[gpui::test]
fn only_the_sessions_on_screen_have_their_reports_read(cx: &mut gpui::TestAppContext) {
    let (view, cx) = linked(cx, two());
    let viewed = |cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            view.update(cx, |view, cx| {
                view.view_session_reports(cx);
                view.agent_models
                    .viewed(&ConnectTarget::Local)
                    .unwrap_or_default()
            })
        })
    };
    assert_eq!(viewed(cx), ["p0"]);
    cx.update(|window, cx| {
        view.update(cx, |view, cx| {
            view.choose_herdr_tab(view.active_group().unwrap(), "t1", window, cx);
        })
    });
    draw(cx);
    assert!(viewed(cx).is_empty());
}
