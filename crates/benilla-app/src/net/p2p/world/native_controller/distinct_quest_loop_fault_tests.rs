// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn inject(h: &Harness) {
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored distinct quest save fault",
    )
    .unwrap();
}
fn flush_fault(
    h: &Harness,
    c: &mut NativeController,
    controls: &Controls,
    out: Pump,
) -> Vec<SessionEvent> {
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(c.pending.is_none());
    let checkpoint = c.world.checkpoint().unwrap();
    let state = c.world.state().clone();
    let generation = c.store.generation();
    let calls = controls.calls.load(Ordering::SeqCst);
    let event_count = c.outbox.as_ref().unwrap().events.len();
    assert!(event_count > 0);
    assert!(c.outbox.as_ref().unwrap().saved.is_none());
    assert!(c.retry_tick().is_err());
    assert!(c.retry_checkpoint().is_err());
    let paused = c.pump(Duration::from_secs(600));
    assert_eq!(paused.ticks, 0);
    assert!(paused.events.is_empty());
    assert!(matches!(
        c.receive(ClientCommand::QuestQuery {
            quest: loop_fixture::QUEST
        }),
        Err(Error::Paused)
    ));
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    let saved = c.retry_checkpoint().unwrap();
    assert_eq!(saved.tick, state.tick);
    assert_eq!(c.store.generation(), generation + 1);
    assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(c.world.state(), &state);
    assert_eq!(controls.calls.load(Ordering::SeqCst), calls);
    assert!(matches!(
        c.receive(ClientCommand::QuestQuery {
            quest: loop_fixture::QUEST
        }),
        Err(Error::Paused)
    ));
    let published = c.pump(Duration::ZERO);
    assert_eq!(published.ticks, 0);
    assert_eq!(published.events.len(), event_count);
    assert!(published.pause.is_none());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(
        controls.calls.load(Ordering::SeqCst),
        calls,
        "checkpoint retry must not replay gameplay or authority"
    );
    assert_eq!(c.world.state(), &state);
    published.events
}

fn collection_reward_checkpoint_faults(quest_only: bool) {
    let h = Harness::new();
    let data = if quest_only {
        quest_only::profile(1)
    } else {
        loop_fixture::profile()
    };
    let controls = Controls::default();
    let mut c = enter(&h, data.clone(), false, controls.clone());
    c.initial_events().unwrap();
    accept(&mut c, loop_fixture::GIVER);
    begin_cast(&mut c, loop_fixture::HOSTILE);
    let mut lethal = None;
    for _ in 0..100 {
        let mut expected = c.world.clone();
        expected
            .step(
                c.tick() + 1,
                None,
                &mut loop_fixture::CompleteAuthority::default(),
            )
            .unwrap();
        if expected.state().npcs[&loop_fixture::HOSTILE].health == 0 {
            lethal = Some(expected);
            break;
        }
        step(&mut c);
    }
    let expected = lethal.expect("authored selected source spell reaches lethal impact");
    let before_generation = c.store.generation();
    inject(&h);
    let out = c.pump(c.tick_duration());
    assert_eq!(c.world.state(), expected.state());
    assert_eq!(c.store.generation(), before_generation);
    assert_eq!(progress(&c).credited_generations.len(), 1);
    assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 0);
    let events = flush_fault(&h, &mut c, &controls, out);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::XpGain(x) if x.kill))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::QuestObjectiveKill {
                    quest_id: 70001,
                    count: 1,
                    ..
                }
            ))
            .count(),
        usize::from(!quest_only)
    );
    c = reopen(&h, c, &data, &controls);

    open_loot(&mut c, loop_fixture::HOSTILE);
    c.receive(ClientCommand::LootMoney).unwrap();
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    inject(&h);
    let before_generation = c.store.generation();
    let out = c.pump(c.tick_duration());
    assert_eq!(c.store.generation(), before_generation);
    assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 1);
    assert_eq!(progress(&c).money, 14);
    let events = flush_fault(&h, &mut c, &controls, out);
    assert_eq!(events.iter().filter(|e| matches!(e, SessionEvent::ItemPushResult(p) if p.item_entry == loop_fixture::ITEM && p.count == 1)).count(), 1);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootClearMoney))
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::QuestObjectiveItem {
                    item_id: 70001,
                    count: 1
                }
            ))
            .count(),
        1
    );
    c = reopen(&h, c, &data, &controls);

    choose_reward(&mut c, loop_fixture::GIVER);
    inject(&h);
    let before_generation = c.store.generation();
    let out = c.pump(c.tick_duration());
    assert_eq!(c.store.generation(), before_generation);
    assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 0);
    assert_eq!(progress(&c).money, 21);
    assert!(
        progress(&c)
            .quests
            .progress(loop_fixture::QUEST)
            .unwrap()
            .rewarded
    );
    let events = flush_fault(&h, &mut c, &controls, out);
    assert_eq!(
        events
            .iter()
            .filter(
                |e| matches!(e, SessionEvent::QuestComplete(q) if q.quest_id == loop_fixture::QUEST)
            )
            .count(),
        1
    );
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::XpGain(x) if !x.kill))
            .count(),
        1
    );
    c = reopen(&h, c, &data, &controls);
    choose_reward(&mut c, loop_fixture::GIVER);
    no_transient_progress(&step(&mut c).events);
    assert_eq!(progress(&c).money, 21);
    assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 0);
    assert_source_hosts(&controls);
    c.shutdown().unwrap();
}

#[test]
fn distinct_source_quest_kill_collection_reward_checkpoint_faults_publish_once_without_reexecution()
{
    collection_reward_checkpoint_faults(false);
    collection_reward_checkpoint_faults(true);
}

#[test]
fn distinct_source_quest_late_second_owner_failure_rolls_back_acceptance_and_all_owned_state() {
    for (hostile, giver) in [(2, 3), (30, 20)] {
        // B is deliberately last in the source-selected update schedule, even when its ID is lower.
        let data = loop_fixture::profile_with_spawns(hostile, giver, [hostile, giver]);
        let h = Harness::new();
        let controls = Controls::default();
        let mut c = enter(&h, data.clone(), false, controls.clone());
        c.initial_events().unwrap();
        step(&mut c);
        c.receive(ClientCommand::QuestgiverAccept {
            npc: guid(&c, giver),
            quest: loop_fixture::QUEST,
        })
        .unwrap();
        let checkpoint = c.world.checkpoint().unwrap();
        let before = c.world.state().clone();
        let generation = c.store.generation();
        let mut expected = c.world.clone();
        controls.fail_owner.store(giver, Ordering::SeqCst);
        let failed = c.pump(c.tick_duration());
        assert_eq!(failed.ticks, 0);
        assert!(failed.events.is_empty());
        assert!(matches!(failed.pause.unwrap().kind, PauseKind::Authority));
        assert_eq!(c.world.checkpoint().unwrap(), checkpoint);
        assert_eq!(c.world.state(), &before); // RNG, queues, credit, training, inventory and command sequence.
        assert_eq!(c.store.generation(), generation);
        assert!(progress(&c).quests.progress(loop_fixture::QUEST).is_none());
        let pending = c.pending.as_ref().unwrap().inputs.clone();
        assert_eq!(pending.len(), 1);
        let record = expected
            .step_batch(
                c.tick() + 1,
                pending.clone(),
                &mut loop_fixture::CompleteAuthority::default(),
            )
            .unwrap()
            .clone();
        assert!(expected
            .state()
            .quest_progression
            .as_ref()
            .unwrap()
            .quests
            .progress(loop_fixture::QUEST)
            .is_some());
        assert_ne!(
            expected.state().npcs[&hostile].source_motion,
            before.npcs[&hostile].source_motion
        );
        let mut replay = c.world.clone();
        replay.replay(&[record]).unwrap();
        assert_eq!(replay.state(), expected.state());
        controls.fail_owner.store(0, Ordering::SeqCst);
        c.retry_tick().unwrap();
        assert_eq!(c.pending.as_ref().unwrap().inputs, pending);
        let out = c.pump(Duration::ZERO);
        assert_eq!(out.ticks, 1);
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert!(!out.events.is_empty());
        assert_eq!(c.world.state(), expected.state());
        assert_eq!(c.world.state().rng_counter, expected.state().rng_counter);
        assert!(c.pending.is_none());
        assert!(c.pump(Duration::ZERO).events.is_empty());
        c = reopen(&h, c, &data, &controls);
        c.shutdown().unwrap();
    }
}
