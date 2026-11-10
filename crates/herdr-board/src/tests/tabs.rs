use super::*;

#[test]
fn a_tab_is_named_by_its_own_label_then_its_agent_then_its_directory() {
    let mut snapshot = device(
        &[("w1", "tools")],
        &[
            ("t1", "w1", "working"),
            ("t2", "w1", "idle"),
            ("t3", "w1", "idle"),
        ],
        &[("t1", "claude", "  fix the build  "), ("t2", "codex", " ")],
    );
    let title =
        |snapshot: &ClientShellSnapshot, index: usize| tab_title(&snapshot.tabs[index], snapshot);
    assert_eq!(title(&snapshot, 0), "fix the build");
    // A blank terminal title leaves the agent's name.
    assert_eq!(title(&snapshot, 1), "codex");
    // A shell tab takes its directory's name, the foreground one first.
    assert_eq!(title(&snapshot, 2), "t3");
    snapshot.panes[2].foreground_cwd = Some("/work/deeper/inner".into());
    assert_eq!(title(&snapshot, 2), "inner");
    snapshot.panes.clear();
    assert_eq!(title(&snapshot, 2), "tab 3");
    // A renamed tab keeps its name over everything else.
    snapshot.tabs[0].custom_label = true;
    snapshot.tabs[0].label = "release".into();
    assert_eq!(title(&snapshot, 0), "release");
}

#[test]
fn a_list_shows_the_shown_space_and_across_devices_its_namesakes() {
    let local = device(
        &[("w1", "tools"), ("w2", "fedi")],
        &[
            ("t1", "w1", "idle"),
            ("t2", "w2", "idle"),
            ("t3", "w1", "idle"),
        ],
        &[],
    );
    let mini = device(
        &[("m1", "fedi"), ("m2", "tools")],
        &[("t1", "m2", "idle"), ("t9", "m1", "idle")],
        &[],
    );
    let mut sources = [
        source("local", "Local", &local),
        source("ssh:mini", "Mini", &mini),
    ];
    assert_eq!(
        ids(&listed_tabs(&sources, 0, false)),
        [(0, "t1"), (0, "t3")]
    );
    assert_eq!(
        ids(&listed_tabs(&sources, 0, true)),
        [(0, "t1"), (0, "t3"), (1, "t1")]
    );
    // Rows keep device order, whichever device is selected.
    assert_eq!(ids(&listed_tabs(&sources, 1, true)), [(0, "t2"), (1, "t9")]);

    for hide in [
        |source: &mut Source<'_>| source.enabled = false,
        |source: &mut Source<'_>| source.visible = false,
        |source: &mut Source<'_>| source.snapshot = None,
    ] {
        let mut hidden = sources;
        hide(&mut hidden[1]);
        assert_eq!(ids(&listed_tabs(&hidden, 0, true)), [(0, "t1"), (0, "t3")]);
    }
    sources[0].snapshot = None;
    assert!(listed_tabs(&sources, 0, true).is_empty());
    assert!(listed_tabs(&sources, 7, true).is_empty());
}

#[test]
fn the_attention_sort_orders_by_place_and_can_keep_devices_together() {
    let local = device(
        &[("w1", "tools")],
        &[
            ("a", "w1", "idle"),
            ("b", "w1", "working"),
            ("c", "w1", "blocked"),
        ],
        &[],
    );
    let mini = device(
        &[("m1", "tools")],
        &[("d", "m1", "blocked"), ("e", "m1", "done")],
        &[],
    );
    let sources = [
        source("local", "Local", &local),
        source("ssh:mini", "Mini", &mini),
    ];
    let mut ledger = AttentionLedger::default();
    settle(&sources, &mut ledger, daemon_mark, |_, _| false);
    let rank = |row: &ListedTab<'_>| attention_rank(Mark::Status(row.tab.agent_status));

    let mut listed = listed_tabs(&sources, 0, true);
    sort_by_attention(&mut listed, &sources, &ledger, false, rank);
    // Rows of one rank keep the order they were listed in.
    assert_eq!(
        ids(&listed),
        [(0, "c"), (1, "d"), (0, "b"), (1, "e"), (0, "a")]
    );
    sort_by_attention(&mut listed, &sources, &ledger, true, rank);
    assert_eq!(
        ids(&listed),
        [(0, "c"), (0, "b"), (0, "a"), (1, "d"), (1, "e")]
    );

    // The tab that reached a rank last leads it.
    let mut later = local.clone();
    later.tabs[0].agent_status = AgentStatus::Blocked;
    let sources = [
        source("local", "Local", &later),
        source("ssh:mini", "Mini", &mini),
    ];
    settle(&sources, &mut ledger, daemon_mark, |_, _| false);
    let mut listed = listed_tabs(&sources, 0, true);
    sort_by_attention(&mut listed, &sources, &ledger, false, rank);
    assert_eq!(ids(&listed)[..3], [(0, "a"), (0, "c"), (1, "d")]);
}

#[test]
fn blocked_tabs_outside_the_list_wait_elsewhere_latest_first() {
    let local = device(
        &[("w1", "tools"), ("w2", "fedi")],
        &[
            ("t1", "w1", "blocked"),
            ("t2", "w2", "working"),
            ("t3", "w2", "working"),
        ],
        &[],
    );
    let mini = device(&[("m1", "worksteps")], &[("t4", "m1", "working")], &[]);
    let mut ledger = AttentionLedger::default();
    let mut block = |local: &ClientShellSnapshot, mini: &ClientShellSnapshot| {
        let sources = [
            source("local", "Local", local),
            source("ssh:mini", "Mini", mini),
        ];
        settle(&sources, &mut ledger, daemon_mark, |_, _| false);
    };
    block(&local, &mini);
    let mut local = local;
    let mut mini = mini;
    local.tabs[2].agent_status = AgentStatus::Blocked;
    block(&local, &mini);
    mini.tabs[0].agent_status = AgentStatus::Blocked;
    block(&local, &mini);

    let mut sources = [
        source("local", "Local", &local),
        source("ssh:mini", "Mini", &mini),
    ];
    let listed = listed_tabs(&sources, 0, false);
    assert_eq!(ids(&listed), [(0, "t1")]);
    // The listed blocked tab is not repeated; the rest come latest first.
    assert_eq!(
        ids(&waiting_elsewhere(&sources, &listed, &ledger)),
        [(1, "t4"), (0, "t3")]
    );
    assert_eq!(
        ids(&waiting_elsewhere(&sources, &[], &ledger)),
        [(1, "t4"), (0, "t3"), (0, "t1")]
    );
    // A hidden device still reports; a disconnected or disabled one does not.
    sources[1].visible = false;
    assert_eq!(waiting_elsewhere(&sources, &listed, &ledger).len(), 2);
    sources[1].connected = false;
    assert_eq!(
        ids(&waiting_elsewhere(&sources, &listed, &ledger)),
        [(0, "t3")]
    );
    sources[1].connected = true;
    sources[1].enabled = false;
    assert_eq!(
        ids(&waiting_elsewhere(&sources, &listed, &ledger)),
        [(0, "t3")]
    );
}

#[test]
fn a_waiting_tag_names_the_space_and_another_device_cut_to_sixteen() {
    assert_eq!(waiting_place("tools", None), "tools");
    assert_eq!(waiting_place("tools", Some("Mini")), "tools \u{b7} Mini");
    assert_eq!(
        waiting_place("a-space-with-a-long-name", Some("sixteen-chars-ok")),
        "a-space-with-a-\u{2026} \u{b7} sixteen-chars-ok"
    );
}
