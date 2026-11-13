//! A tab row's hover card: the model its agent runs on and how hard it
//! thinks, once the agent's host has said.

use super::*;
use crate::agent_models::{Model, Source};
use std::{collections::HashMap, time::Duration};

fn tabs_with_models<'a>(
    panes: &[(&str, &str, &str)],
    cx: &'a mut gpui::TestAppContext,
) -> &'a mut gpui::VisualTestContext {
    let panes: HashMap<String, Model> = panes
        .iter()
        .map(|(pane, model, effort)| {
            let model = Model {
                model: (*model).into(),
                effort: (*effort).into(),
            };
            ((*pane).to_owned(), model)
        })
        .collect();
    let (_, cx) = cx.add_window_view(|window, cx| {
        let mut view = fixture_window(window, cx);
        view.config.show_sidebar_tabs = true;
        // The fixture's custom socket names no daemon the CLI can ask.
        view.endpoints[0] = crate::endpoint::Endpoint::new(
            crate::endpoint::LOCAL.into(),
            "Local".into(),
            ConnectTarget::Local,
            true,
        );
        let snapshot = Arc::make_mut(view.live.snapshot.as_mut().unwrap());
        snapshot.focused_workspace_id = Some("w0".into());
        view.agent_models
            .seed(Source::of(&ConnectTarget::Local).unwrap(), panes);
        view
    });
    cx.simulate_resize(size(px(800.), px(600.)));
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
    cx
}

fn hover(selector: &'static str, cx: &mut gpui::VisualTestContext) {
    let row = cx.debug_bounds(selector).unwrap();
    cx.simulate_mouse_move(row.center(), None, Modifiers::default());
    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();
    cx.update(|window, cx| full_draw(window, cx).clear(cx));
}

#[gpui::test]
fn resting_on_a_tab_shows_its_agent_s_model_and_effort(cx: &mut gpui::TestAppContext) {
    let cx = tabs_with_models(&[("p0", "Opus 5.5", "xhigh")], cx);
    assert!(cx.debug_bounds("agent-model-card").is_none());
    hover("row-tab-t0", cx);
    let card = cx.debug_bounds("agent-model-card").unwrap();
    let meter = cx.debug_bounds("agent-model-meter").unwrap();
    assert!(card.contains(&meter.center()));
    assert!(card.size.height < px(30.));
}

#[gpui::test]
fn a_tab_whose_model_is_unknown_shows_no_card(cx: &mut gpui::TestAppContext) {
    let cx = tabs_with_models(&[("p9", "Opus 5.5", "high")], cx);
    hover("row-tab-t0", cx);
    assert!(cx.debug_bounds("agent-model-card").is_none());
}

#[gpui::test]
fn an_effort_without_a_level_shows_no_meter(cx: &mut gpui::TestAppContext) {
    let cx = tabs_with_models(&[("p0", "Kimi K3", "")], cx);
    hover("row-tab-t0", cx);
    assert!(cx.debug_bounds("agent-model-card").is_some());
    assert!(cx.debug_bounds("agent-model-meter").is_none());
}
