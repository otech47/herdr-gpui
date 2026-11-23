#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{AgentModels, Answer, Model, Pass, Reading, Report, Source, render::level};
use crate::{Error, usage::Host};
use herdr_client::ConnectTarget;
use std::{collections::HashMap, path::PathBuf};

fn model(name: &str, effort: &str) -> Model {
    Model {
        model: name.into(),
        effort: effort.into(),
    }
}

fn remote(session: &str) -> ConnectTarget {
    ConnectTarget::Ssh {
        target: "studio".into(),
        session: session.into(),
    }
}

#[test]
fn each_daemon_is_asked_through_the_cli_that_names_it() {
    let cli = |target: &ConnectTarget| Source::of(target).map(|source| source.cli().join(" "));
    assert_eq!(cli(&ConnectTarget::Local).as_deref(), Some("herdr"));
    let named = ConnectTarget::Session {
        name: "work".into(),
        development: false,
    };
    assert_eq!(cli(&named).as_deref(), Some("herdr --session work"));
    assert_eq!(
        cli(&remote("default")).as_deref(),
        Some("herdr --session default")
    );
    assert_eq!(
        Source::of(&remote("default")).unwrap().host,
        Host::Ssh("studio".into())
    );
    let development = ConnectTarget::Session {
        name: "work".into(),
        development: true,
    };
    assert_eq!(cli(&development), None);
    assert_eq!(
        cli(&ConnectTarget::Socket(PathBuf::from("/tmp/x.sock"))),
        None
    );
}

#[test]
fn the_helper_answer_decodes() {
    let answer: Answer = serde_json::from_str(
        r#"{"panes": {"w1:p2": {"model": "Opus 5.5", "effort": "xhigh"},
                      "w1:p3": {"model": "Kimi K3"}},
            "known": {"claude:abc": {"path": "/x", "size": 1, "mtime": 2, "model": "m", "effort": ""}}}"#,
    )
    .unwrap();
    assert_eq!(answer.panes["w1:p2"], model("Opus 5.5", "xhigh"));
    assert_eq!(answer.panes["w1:p3"], model("Kimi K3", ""));
    assert!(answer.known["claude:abc"].is_object());
    assert!(answer.reports.is_empty());
    let answer: Answer = serde_json::from_str(
        r#"{"panes": {}, "known": {},
            "reports": {"w1:p2": [{"url": "http://localhost:8788/a.html", "title": "A", "at": 7},
                                  {"url": "http://localhost:8788/b.md"}]}}"#,
    )
    .unwrap();
    assert_eq!(
        answer.reports["w1:p2"],
        [report("a.html", "A", 7), report("b.md", "", 0)]
    );
}

fn report(name: &str, title: &str, at: u64) -> Report {
    Report {
        url: format!("http://localhost:8788/{name}"),
        title: title.into(),
        at,
    }
}

fn pass(panes: &super::Panes, asked: &[&str], reports: &[(&str, Vec<Report>)]) -> Pass {
    Pass {
        panes: panes.clone(),
        asked: asked.iter().map(|pane| (*pane).to_owned()).collect(),
        reports: reports
            .iter()
            .map(|(pane, reports)| ((*pane).to_owned(), reports.clone()))
            .collect(),
    }
}

#[test]
fn reports_stay_known_until_their_pane_is_asked_about_again() {
    let mut reading = Reading::default();
    let panes = HashMap::new();
    let first = vec![report("a.html", "A", 7)];
    assert!(reading.apply(Ok(pass(&panes, &["w1:p1"], &[("w1:p1", first.clone())]))));
    assert!(!reading.apply(Ok(pass(&panes, &["w1:p1"], &[("w1:p1", first.clone())]))));
    // Another tab is on screen: the first one's list is kept for the way back.
    assert!(!reading.apply(Ok(pass(&panes, &["w1:p2"], &[("w1:p2", Vec::new())]))));
    assert_eq!(reading.reports["w1:p1"], first);
    assert!(!reading.reports.contains_key("w1:p2"));
    // The pane now holds an agent that linked nothing, or no agent at all.
    assert!(reading.apply(Ok(pass(&panes, &["w1:p1"], &[]))));
    assert!(reading.reports.is_empty());
}

#[test]
fn every_effort_an_agent_uses_fills_its_own_step() {
    let levels: Vec<_> = ["minimal", "low", "medium", "high", "xhigh", "max"]
        .iter()
        .map(|effort| level(effort))
        .collect();
    assert_eq!(
        levels,
        [Some(0), Some(1), Some(2), Some(3), Some(4), Some(5)]
    );
    assert_eq!(level(""), None);
    assert_eq!(level("auto"), None);
}

#[test]
fn only_a_different_answer_changes_what_is_shown() {
    let mut reading = Reading::default();
    let panes = HashMap::from([("w1:p1".to_owned(), model("Opus 5.5", "high"))]);
    assert!(reading.apply(Ok(pass(&panes, &[], &[]))));
    assert!(!reading.apply(Ok(pass(&panes, &[], &[]))));
    assert!(!reading.apply(Err(Error::AgentModelsRefused("RuntimeError".into()))));
    assert_eq!(reading.panes, panes);
    assert!(reading.error.is_some());
    assert!(!reading.apply(Ok(pass(&panes, &[], &[]))));
    assert!(reading.error.is_none());
}

#[test]
fn panes_are_looked_up_on_their_own_daemon_and_forgotten_when_dropped() {
    let mut models = AgentModels::default();
    models.seed(
        Source::of(&ConnectTarget::Local).unwrap(),
        HashMap::from([("w1:p1".to_owned(), model("Opus 5.5", "high"))]),
        HashMap::from([("w1:p1".to_owned(), vec![report("a.html", "A", 7)])]),
    );
    models.seed(
        Source::of(&remote("default")).unwrap(),
        HashMap::from([("w1:p1".to_owned(), model("GPT-6 Astra", "xhigh"))]),
        HashMap::new(),
    );
    assert_eq!(
        models.reports(&ConnectTarget::Local, "w1:p1"),
        [report("a.html", "A", 7)]
    );
    assert!(models.reports(&remote("default"), "w1:p1").is_empty());
    assert_eq!(
        models.get(&ConnectTarget::Local, "w1:p1"),
        Some(&model("Opus 5.5", "high"))
    );
    assert_eq!(
        models.get(&remote("default"), "w1:p1"),
        Some(&model("GPT-6 Astra", "xhigh"))
    );
    assert_eq!(models.get(&remote("other"), "w1:p1"), None);
    assert_eq!(models.get(&ConnectTarget::Local, "w1:p2"), None);
    assert!(models.poll(Vec::new()));
    assert_eq!(models.get(&ConnectTarget::Local, "w1:p1"), None);
}
