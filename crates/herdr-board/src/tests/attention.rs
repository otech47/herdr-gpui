use super::*;

fn sighting(tab: &str, rank: u8, held: bool) -> Sighting {
    Sighting {
        device: "local".into(),
        tab: tab.into(),
        rank,
        held,
    }
}

#[test]
fn marks_rank_from_waiting_on_an_answer_down_to_unknown() {
    let ranked: Vec<u8> = [
        Mark::Status(AgentStatus::Blocked),
        Mark::Limited,
        Mark::Status(AgentStatus::Working),
        Mark::Status(AgentStatus::Done),
        Mark::Monitoring,
        Mark::Status(AgentStatus::Idle),
        Mark::Status(AgentStatus::Unknown),
    ]
    .map(attention_rank)
    .to_vec();
    assert_eq!(ranked, [0, 1, 2, 3, 4, 5, 6]);
    for (index, mark) in Mark::ALL.into_iter().enumerate() {
        assert_eq!(mark.index(), index);
    }
}

#[test]
fn a_tab_takes_a_later_tick_each_time_its_rank_changes() {
    let mut ledger = AttentionLedger::default();
    assert_eq!(ledger.place("local", "a", 5), Place { rank: 5, since: 0 });
    ledger.settle([sighting("a", 2, false), sighting("b", 2, false)]);
    assert_eq!(ledger.place("local", "a", 9), Place { rank: 2, since: 0 });

    ledger.settle([sighting("a", 0, false), sighting("b", 2, false)]);
    ledger.settle([sighting("a", 0, false), sighting("b", 0, false)]);
    assert_eq!(ledger.place("local", "a", 9), Place { rank: 0, since: 1 });
    assert_eq!(ledger.place("local", "b", 9), Place { rank: 0, since: 2 });
}

#[test]
fn a_held_tab_keeps_its_place_and_an_unsighted_one_drops_out() {
    let mut ledger = AttentionLedger::default();
    ledger.settle([sighting("a", 2, false), sighting("b", 2, false)]);
    ledger.settle([sighting("a", 0, true)]);
    assert_eq!(ledger.place("local", "a", 9), Place { rank: 2, since: 0 });
    assert_eq!(ledger.place("local", "b", 9), Place { rank: 9, since: 0 });
    // Once released it moves to the rank its mark has.
    ledger.settle([sighting("a", 0, false)]);
    assert_eq!(ledger.place("local", "a", 9), Place { rank: 0, since: 1 });
}

#[test]
fn a_dropped_order_holds_within_its_rank() {
    let mut ledger = AttentionLedger::default();
    ledger.reorder(
        2,
        ["c", "a", "b"].map(|tab| ("local".to_owned(), tab.to_owned())),
    );
    let since = |tab| ledger.place("local", tab, 2).since;
    assert!(since("c") > since("a") && since("a") > since("b"));
}
