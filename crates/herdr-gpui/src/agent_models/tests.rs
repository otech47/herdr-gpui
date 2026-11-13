#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{AgentModels, Answer, Model, Reading, Source, render::level};
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
    assert!(reading.apply(Ok(panes.clone())));
    assert!(!reading.apply(Ok(panes.clone())));
    assert!(!reading.apply(Err(Error::AgentModelsRefused("RuntimeError".into()))));
    assert_eq!(reading.panes, panes);
    assert!(reading.error.is_some());
    assert!(!reading.apply(Ok(panes)));
    assert!(reading.error.is_none());
}

#[test]
fn panes_are_looked_up_on_their_own_daemon_and_forgotten_when_dropped() {
    let mut models = AgentModels::default();
    models.seed(
        Source::of(&ConnectTarget::Local).unwrap(),
        HashMap::from([("w1:p1".to_owned(), model("Opus 5.5", "high"))]),
    );
    models.seed(
        Source::of(&remote("default")).unwrap(),
        HashMap::from([("w1:p1".to_owned(), model("GPT-6 Astra", "xhigh"))]),
    );
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
