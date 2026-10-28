// SPDX-License-Identifier: GPL-3.0-or-later
// Authored probes of pinned QuestHandler::HandleQuestLogRemoveQuest and
// Player::SetQuestStatus/AddQuest. No native content or source-item admission.
use super::*;

fn abandon(c: &Catalog, s: &mut State, h: &mut Ledger, slot: u8) -> Vec<Event> {
    run(c, s, h, Command::Abandon { slot })
}
fn assert_roundtrip(c: &Catalog, s: &State) {
    assert_eq!(
        State::restore(c, s.snapshot()).unwrap().snapshot(),
        s.snapshot()
    );
}

#[test]
fn invalid_slots_are_silent_and_empty_valid_slot_only_clears_slot() {
    let c = catalog(vec![row(1, 0, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    let host = h.clone();
    for slot in [20, 25, 255] {
        let view = s.view();
        assert!(abandon(&c, &mut s, &mut h, slot).is_empty());
        assert_eq!(s.view(), view);
        assert_eq!(h, host);
    }
    for slot in [0, 19] {
        let view = s.view();
        assert_eq!(
            abandon(&c, &mut s, &mut h, slot),
            vec![Event::LogSet {
                slot: usize::from(slot),
                quest: None
            }]
        );
        assert_eq!(s.view(), view);
    }
    assert_roundtrip(&c, &s);
}

#[test]
fn occupied_abandon_preserves_history_and_publishes_none_before_slot_clear() {
    for complete in [false, true] {
        let c = catalog(vec![row(1, 0, 0), row(2, 1, 0)]);
        let mut s = State::new(&c);
        let mut h = Ledger::new();
        accept(&c, &mut s, &mut h, 1);
        accept(&c, &mut s, &mut h, 2);
        kill(&c, &mut s, &mut h);
        if complete {
            kill(&c, &mut s, &mut h);
        }
        let previous = s.progress(1).unwrap().clone();
        let unrelated = s.progress(2).unwrap().clone();
        let host = h.clone();
        let events = abandon(&c, &mut s, &mut h, 0);
        let [Event::WorldObjects { view }, Event::LogSet {
            slot: 0,
            quest: None,
        }] = events.as_slice()
        else {
            panic!("wrong abandon order: {events:?}")
        };
        assert_eq!(view.log[0], Some(1));
        assert_eq!(view.records[&1].status, Status::None);
        assert_eq!(view.records[&1].creatures, previous.creatures);
        assert_eq!(view.records[&1].items, previous.items);
        assert!(!view.records[&1].rewarded);
        assert_eq!(s.snapshot().log[0], None);
        assert_eq!(s.progress(1).unwrap(), &view.records[&1]);
        assert_eq!(s.progress(2), Some(&unrelated));
        assert_eq!(h, host);
        assert_roundtrip(&c, &s);
        assert_eq!(
            abandon(&c, &mut s, &mut h, 0),
            vec![Event::LogSet {
                slot: 0,
                quest: None
            }]
        );
        assert_eq!(s.progress(1).unwrap().creatures, previous.creatures);
    }
}

#[test]
fn reaccept_uses_first_free_slot_and_resets_prior_kill_credit() {
    let c = catalog(vec![row(1, 0, 0), row(2, 1, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    accept(&c, &mut s, &mut h, 2);
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    abandon(&c, &mut s, &mut h, 0);
    assert!(can_take(&c, &s, &h, 1).is_ok());
    let events = run(
        &c,
        &mut s,
        &mut h,
        Command::Accept {
            quest: 1,
            giver: 900,
        },
    );
    assert!(events.contains(&Event::LogSet {
        slot: 0,
        quest: Some(1)
    }));
    assert_eq!(s.progress(1).unwrap().creatures, [0; 4]);
    assert_eq!(s.progress(1).unwrap().status, Status::Incomplete);
    assert!(!s.progress(1).unwrap().rewarded);
    assert_eq!(s.snapshot().log[1], Some(2));
    kill(&c, &mut s, &mut h);
    assert_eq!(s.progress(1).unwrap().creatures[0], 1);
    assert_eq!(s.progress(1).unwrap().status, Status::Incomplete);
    assert_roundtrip(&c, &s);
}

#[test]
fn deliver_abandon_keeps_ordinary_items_and_reaccept_recounts_inventory() {
    let c = catalog(vec![row(1, 1, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    h.carried.insert(200, 3);
    accept(&c, &mut s, &mut h, 1);
    assert_eq!(s.progress(1).unwrap().status, Status::Complete);
    let host = h.clone();
    abandon(&c, &mut s, &mut h, 0);
    assert_eq!(h, host);
    assert_eq!(s.progress(1).unwrap().items[0], 2);
    accept(&c, &mut s, &mut h, 1);
    assert_eq!(s.progress(1).unwrap().status, Status::Complete);
    abandon(&c, &mut s, &mut h, 0);
    run(
        &c,
        &mut s,
        &mut h,
        Command::RemoveItems {
            item: 200,
            count: 3,
        },
    );
    assert_eq!(s.progress(1).unwrap().status, Status::None);
    assert_eq!(s.progress(1).unwrap().items[0], 2);
    accept(&c, &mut s, &mut h, 1);
    assert_eq!(s.progress(1).unwrap().items, [0; 4]);
    assert_eq!(s.progress(1).unwrap().status, Status::Incomplete);
    assert_roundtrip(&c, &s);
}

#[test]
fn rewarded_history_cannot_be_erased_by_abandoning_its_empty_former_slot() {
    let c = catalog(vec![row(1, 0, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    reward(&c, &mut s, &mut h, 1);
    let history = s.progress(1).unwrap().clone();
    let host = h.clone();
    abandon(&c, &mut s, &mut h, 0);
    assert_eq!(s.progress(1), Some(&history));
    assert!(history.rewarded);
    assert_eq!(h, host);
    assert_eq!(
        can_take(&c, &s, &h, 1),
        Err(Error::Rejected(Gate::AlreadyTaken))
    );
    assert_roundtrip(&c, &s);
}

#[test]
fn none_history_restore_rejects_logged_or_rewarded_forgery_and_sequence_replay() {
    let c = catalog(vec![row(1, 0, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    let sequence = s.next_sequence();
    abandon(&c, &mut s, &mut h, 0);
    assert!(reduce(&c, &s, &h, sequence, Command::Abandon { slot: 0 }).is_err());
    let mut bad = s.snapshot();
    bad.log[0] = Some(1);
    assert!(State::restore(&c, bad).is_err());
    let mut bad = s.snapshot();
    bad.records.get_mut(&1).unwrap().rewarded = true;
    assert!(State::restore(&c, bad).is_err());
    let mut bad = s.snapshot();
    bad.records.get_mut(&1).unwrap().status = Status::Incomplete;
    assert!(State::restore(&c, bad).is_err());
    assert_roundtrip(&c, &s);
}

#[test]
fn source_item_and_timer_dependencies_stay_unadmitted() {
    for fault in 0..5 {
        let mut r = row(1, 0, 0);
        match fault {
            0 => r.excluded.source_item = 200,
            1 => r.excluded.source_item_count = 1,
            2 => r.excluded.source_spell = 8326,
            3 => r.excluded.limit_time = 60,
            _ => r.special_flags |= 0x40, // QUEST_SPECIAL_FLAG_TIMED
        }
        let rel = relations(std::slice::from_ref(&r));
        assert!(matches!(
            Catalog::new(vec![r], rel, audit()),
            Err(Error::Unsupported(_))
        ));
    }
}

#[test]
fn abandoned_next_chain_history_does_not_block_but_rewarded_complete_still_does() {
    let mut predecessor = row(1, 0, 0);
    predecessor.next_in_chain = 2;
    let c = catalog(vec![predecessor, row(2, 0, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 2);
    assert_eq!(
        can_take(&c, &s, &h, 1),
        Err(Error::Rejected(Gate::NextChain))
    );
    abandon(&c, &mut s, &mut h, 0);
    assert_eq!(s.progress(2).unwrap().status, Status::None);
    assert!(can_take(&c, &s, &h, 1).is_ok());
    assert_roundtrip(&c, &s);
    accept(&c, &mut s, &mut h, 2);
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    reward(&c, &mut s, &mut h, 2);
    assert!(s.progress(2).unwrap().rewarded);
    assert_eq!(s.progress(2).unwrap().status, Status::Complete);
    // SatisfyQuestNextChain examines Complete even when no log slot remains.
    assert_eq!(
        can_take(&c, &s, &h, 1),
        Err(Error::Rejected(Gate::NextChain))
    );
    assert_roundtrip(&c, &s);
}

#[test]
fn abandoned_negative_prerequisite_is_not_current() {
    let mut dependent = row(2, 0, 0);
    dependent.previous_quest = -1;
    let c = catalog(vec![row(1, 0, 0), dependent]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    assert!(can_take(&c, &s, &h, 2).is_ok());
    abandon(&c, &mut s, &mut h, 0);
    assert_eq!(
        can_take(&c, &s, &h, 2),
        Err(Error::Rejected(Gate::Previous))
    );
    accept(&c, &mut s, &mut h, 1);
    assert!(can_take(&c, &s, &h, 2).is_ok());
    assert_roundtrip(&c, &s);
}

#[test]
fn abandoned_previous_chain_is_not_current_and_reaccept_restores_the_gate() {
    let mut predecessor = row(1, 0, 0);
    predecessor.next_in_chain = 2;
    let c = catalog(vec![predecessor, row(2, 0, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    assert_eq!(
        can_take(&c, &s, &h, 2),
        Err(Error::Rejected(Gate::PreviousChain))
    );
    abandon(&c, &mut s, &mut h, 0);
    assert!(can_take(&c, &s, &h, 2).is_ok());
    accept(&c, &mut s, &mut h, 1);
    assert_eq!(
        can_take(&c, &s, &h, 2),
        Err(Error::Rejected(Gate::PreviousChain))
    );
    assert_roundtrip(&c, &s);
}
