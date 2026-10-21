#![allow(clippy::expect_used, clippy::unwrap_used)]

use super::{Monitoring, Tab, Watched, Watchers, parse, runs};
use crate::{Error, listening_ports::Daemon, usage::Host};
use herdr_client::ConnectTarget;

fn tab(id: &str) -> Tab {
    Tab {
        id: id.into(),
        socket: String::new(),
    }
}

fn under(name: &str) -> Watched {
    Watched {
        under: vec![name.into()],
        disowned: false,
    }
}

fn seeded(watchers: Watchers) -> Monitoring {
    let mut monitoring = Monitoring::default();
    monitoring.seed(Host::Local, watchers);
    monitoring
}

#[test]
fn a_shell_under_the_agent_keeps_its_tab_watched() {
    // Neither the pane's login shell nor the agent's background shell shows
    // an environment; the agent and its servers do. MCP servers and the
    // caffeinate helper are not shells.
    let watchers = parse(
        "P 1 0 launchd\n\
         P 100 1 herdr\n\
         P 200 100 bash\n\
         P 300 200 claude\n\
         P 301 300 npm\n\
         P 302 301 node\n\
         P 303 300 bash\n\
         P 304 303 cargo\n\
         P 305 300 caffeinate\n\
         P 400 100 bash\n\
         P 500 400 codex\n\
         P 501 500 node\n\
         P 600 100 bash\n\
         P 700 600 kimi\n\
         P 701 700 sh\n\
         E 300 w1:t1\n\
         E 301 w1:t1\n\
         E 302 w1:t1\n\
         E 305 w1:t1\n\
         E 500 w1:t2\n\
         E 501 w1:t2\n\
         E 700 w1:t3\n\
         E 701 w1:t3\n",
    )
    .unwrap();
    assert_eq!(watchers.get(&tab("w1:t1")), Some(&under("claude")));
    assert_eq!(watchers.get(&tab("w1:t3")), Some(&under("kimi")));
    assert_eq!(watchers.len(), 2);
    let monitoring = seeded(watchers);
    let daemon = Daemon::from(&ConnectTarget::Local);
    assert!(monitoring.watching(&daemon, "w1:t1", "claude"));
    assert!(!monitoring.watching(&daemon, "w1:t1", "codex"));
    assert!(!monitoring.watching(&daemon, "w1:t2", "codex"));
}

#[test]
fn a_shell_above_the_agent_is_not_its_work() {
    // nix develop puts a wrapper shell between the pane and the agent, and
    // a tab's shells are placed by their ancestors, never by a descendant.
    let watchers = parse(
        "P 1 0 launchd\n\
         P 200 1 bash\n\
         P 210 200 nix\n\
         P 220 210 bash\n\
         P 230 220 claude\n\
         P 231 230 node\n\
         P 240 1 bash\n\
         P 250 240 codex\n\
         E 210 w1:t1\n\
         E 230 w1:t1\n\
         E 231 w1:t1\n\
         E 250 w1:t2\n",
    )
    .unwrap();
    assert_eq!(watchers.get(&tab("w1:t1")), Some(&under("nix")));
    assert_eq!(watchers.len(), 1);
    let monitoring = seeded(watchers);
    assert!(!monitoring.watching(&Daemon::from(&ConnectTarget::Local), "w1:t1", "claude"));
}

#[test]
fn a_disowned_timer_keeps_its_tab_watched_but_a_server_does_not() {
    let watchers = parse(
        "P 1 0 launchd\n\
         P 300 1 Python\n\
         P 400 1 node\n\
         P 500 1 bash\n\
         E 300 w1:t1\n\
         E 400 w1:t2\n\
         E 500 w1:t3\n",
    )
    .unwrap();
    assert!(watchers[&tab("w1:t1")].disowned);
    assert_eq!(watchers.get(&tab("w1:t2")), None);
    assert!(watchers[&tab("w1:t3")].disowned);
    let monitoring = seeded(watchers);
    let daemon = Daemon::from(&ConnectTarget::Local);
    assert!(monitoring.watching(&daemon, "w1:t1", "claude"));
    assert!(!monitoring.watching(&daemon, "w1:t2", "claude"));
    assert!(monitoring.watching(&daemon, "w1:t3", "codex"));
}

#[test]
fn another_daemon_s_tabs_are_not_claimed() {
    let watchers = parse(
        "P 1 0 launchd\n\
         P 10 1 claude\n\
         P 11 10 bash\n\
         E 10 w1:t1 /home/u/.config/herdr/sessions/other/herdr.sock\n\
         E 11 w1:t1 /home/u/.config/herdr/sessions/other/herdr.sock\n",
    )
    .unwrap();
    let mut monitoring = Monitoring::default();
    monitoring.seed(Host::Ssh("devbox".into()), watchers);
    let remote = |session: &str| {
        Daemon::from(&ConnectTarget::Ssh {
            target: "devbox".into(),
            session: session.into(),
        })
    };
    assert!(monitoring.watching(&remote("other"), "w1:t1", "claude"));
    assert!(!monitoring.watching(&remote("default"), "w1:t1", "claude"));
    assert!(!monitoring.watching(&remote("other"), "w1:t2", "claude"));
    assert!(!monitoring.watching(&Daemon::from(&ConnectTarget::Local), "w1:t1", "claude"));
}

#[test]
fn the_agent_is_matched_by_the_daemon_s_label() {
    assert!(runs("claude", "claude"));
    assert!(runs("claude", "claude-code"));
    assert!(runs("cursor", "agent"));
    assert!(runs("cursor", "cursor-agent"));
    assert!(runs("codex", "codex.exe"));
    assert!(runs("kimi", "kimi"));
    assert!(!runs("claude", "claudette"));
    assert!(!runs("claude", "nix"));
    assert!(!runs("codex", "claude"));
    assert!(!runs("claude", "agent"));
}

#[test]
fn unusable_lines_are_skipped_and_a_bare_host_is_an_error() {
    assert!(matches!(parse("N\n"), Err(Error::MonitoringTool)));
    let watchers = parse(
        "P x 1 bash\n\
         P 2\n\
         E 2\n\
         P 3 1 bash\n\
         E 3 w1:t1\n\
         P 4 1 bash\n\
         E 4 w1:t1=bad\n\
         P 5 1 bash\n\
         E 5 \u{7f}w1\n\
         rubbish\n",
    )
    .unwrap();
    assert_eq!(watchers.len(), 1);
    assert!(watchers[&tab("w1:t1")].disowned);
}

#[test]
fn a_process_tree_with_a_loop_still_parses() {
    let watchers = parse(
        "P 10 11 claude\n\
         P 11 10 bash\n\
         E 10 w1:t1\n",
    )
    .unwrap();
    assert_eq!(watchers.get(&tab("w1:t1")), Some(&under("claude")));
}

#[test]
fn a_shell_of_another_tab_s_pane_stays_with_its_own_tab() {
    let watchers = parse(
        "P 1 0 launchd\n\
         P 10 1 claude\n\
         P 11 10 bash\n\
         P 20 11 codex\n\
         P 21 20 bash\n\
         E 10 w1:t1\n\
         E 20 w1:t2\n",
    )
    .unwrap();
    assert_eq!(watchers.get(&tab("w1:t1")), Some(&under("claude")));
    assert_eq!(watchers.get(&tab("w1:t2")), Some(&under("codex")));
}
