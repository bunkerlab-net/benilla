// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn real(value: f32) -> Real {
    Real::new(value).unwrap()
}

fn profile() -> Profile {
    Profile {
        creature_id: 1,
        map_id: 7,
        content_revision: Revision([3; 32]),
        spawn: Point::new(0., 0., 0.).unwrap(),
        clock_origin_ms: 0,
        level: 1,
        reaction: Reaction::Defensive,
        neutral_to_all: false,
        detection: real(18.),
        aggro_rate: Real::ONE,
        combat_reach: real(1.5),
        bounding_radius: real(0.5),
        melee_leeway: real(2.66),
        run_speed: real(7.),
        home_speed: real(7.),
        mainhand_interval_ms: 2000,
        pursuit_ms: 15_000,
        leash_radius: real(30.),
        hard_leash_radius: None,
        corpse_decay_ms: 200,
        respawn_delay_seconds: 5,
        unsupported: vec![],
    }
}

fn observation(ai: &SoloCreatureAi, elapsed_ms: u32, receipts: Vec<Receipt>) -> Observation {
    Observation {
        sequence: ai.next_sequence().unwrap(),
        elapsed_ms,
        geometry_revision: Revision([4; 32]),
        target: None,
        can_initiate_attack: true,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: false,
        creature_moving_and_running: false,
        movement: None,
        receipts,
    }
}

fn death_observation(ai: &SoloCreatureAi, elapsed_ms: u32) -> Observation {
    observation(
        ai,
        elapsed_ms,
        vec![Receipt {
            id: ai.last_receipt_id() + 1,
            event: CombatEvent::Died,
        }],
    )
}

fn timing(ai: &SoloCreatureAi, o: &Observation, delay: u32) -> DeathTiming {
    DeathTiming {
        receipt_id: o.receipts.last().unwrap().id,
        at_ms: ai.now_ms + u64::from(o.elapsed_ms),
        respawn_delay_seconds: delay,
    }
}

fn update(ai: &mut SoloCreatureAi, o: Observation, timing: Option<DeathTiming>) -> Vec<Effect> {
    let token = ai.begin_receipts_with_death_timing(&o, timing).unwrap();
    ai.finish_receipts(o, token, &mut |_, _, _| Ok(MainhandResult::Unchanged))
        .unwrap()
}

fn idle_update(ai: &mut SoloCreatureAi, elapsed_ms: u32) -> Vec<Effect> {
    let o = observation(ai, elapsed_ms, vec![]);
    update(ai, o, None)
}

fn respawn_boundary() -> SoloCreatureAi {
    let mut ai = SoloCreatureAi::new_generation_owned(profile()).unwrap();
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 1);
    assert_eq!(update(&mut ai, o, Some(t)), vec![Effect::Died]);
    assert!(idle_update(&mut ai, 100).is_empty());
    assert_eq!(idle_update(&mut ai, 100), vec![Effect::CorpseRemoved]);
    assert_eq!(
        idle_update(&mut ai, 700),
        vec![Effect::Respawned {
            position: profile().spawn
        }]
    );
    ai
}

fn restore(ai: &SoloCreatureAi) -> Result<SoloCreatureAi, AiError> {
    SoloCreatureAi::restore_generation_owned(
        ai.checkpoint(),
        ai.effective_profile().clone(),
        ai.now_ms,
        ai.sequence,
        2,
        ai.generation().unwrap(),
    )
}

#[test]
fn source_death_replaces_delay_atomically_without_recapping_corpse() {
    let mut p = profile();
    p.clock_origin_ms = 1350;
    p.corpse_decay_ms = 900;
    let mut ai = SoloCreatureAi::new_generation_owned(p.clone()).unwrap();
    let mut o = death_observation(&ai, 150);
    let t = timing(&ai, &o, 7);
    assert_eq!(
        ai.source_combat_event_with_death_timing(&o, false, Some(t))
            .unwrap(),
        vec![Effect::Died]
    );
    assert_eq!(ai.now_ms, 1350);
    assert_eq!(ai.sequence, 0);
    assert_eq!(ai.death_at_ms(), Some(1500));
    assert_eq!(ai.corpse_expiration_ms(), 2400);
    assert_eq!(ai.respawn_at_ms, 8000);
    assert_eq!(ai.effective_profile().corpse_decay_ms, 900);
    assert_eq!(ai.effective_profile().respawn_delay_seconds, 7);
    o.receipts.clear();
    let token = ai.begin_source_receipts_in_phase(&o, false).unwrap();
    ai.finish_receipts(o, token, &mut |_, _, _| Ok(MainhandResult::Unchanged))
        .unwrap();
    assert_eq!(restore(&ai).unwrap(), ai);
    ai.validate_fixed_step(150).unwrap();
    assert!(SoloCreatureAi::restore_generation_owned(ai.checkpoint(), p, 1500, 1, 2, 0).is_err());
}

#[test]
fn death_timing_rejects_wrong_receipt_time_phase_missing_and_replay() {
    let mut ai = SoloCreatureAi::new_generation_owned(profile()).unwrap();
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 3);
    for invalid in [
        None,
        Some(DeathTiming { receipt_id: 2, ..t }),
        Some(DeathTiming { at_ms: 101, ..t }),
    ] {
        let before = ai.clone();
        assert!(ai
            .source_combat_event_with_death_timing(&o, true, invalid)
            .is_err());
        assert_eq!(ai, before);
    }
    let before = ai.clone();
    let mut nondeath = o.clone();
    nondeath.receipts[0].event = CombatEvent::AttackedBy { target: 2 };
    assert!(ai
        .source_combat_event_with_death_timing(&nondeath, true, Some(t))
        .is_err());
    assert_eq!(ai, before);
    ai.source_combat_event_with_death_timing(&o, true, Some(t))
        .unwrap();
    let before = ai.clone();
    assert!(ai
        .source_combat_event_with_death_timing(&o, true, Some(t))
        .is_err());
    assert_eq!(ai, before);
    let mut duplicate = o;
    duplicate.receipts[0].id += 1;
    let mut duplicate_timing = t;
    duplicate_timing.receipt_id += 1;
    assert!(ai
        .source_combat_event_with_death_timing(&duplicate, true, Some(duplicate_timing))
        .is_err());
    assert_eq!(ai, before);
}

#[test]
fn prelude_timing_and_late_receipt_failure_roll_back_together() {
    let mut ai = SoloCreatureAi::new_generation_owned(profile()).unwrap();
    let mut o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 8);
    o.receipts.push(Receipt {
        id: 2,
        event: CombatEvent::AttackedBy { target: 2 },
    });
    let before = ai.clone();
    assert!(ai.begin_receipts_with_death_timing(&o, Some(t)).is_err());
    assert_eq!(ai, before);
    o.receipts[1].event = CombatEvent::Died;
    assert!(ai
        .begin_source_receipts_with_death_timing(&o, false, Some(t))
        .is_err());
    assert_eq!(ai, before);
    o.receipts.pop();
    let token = ai
        .begin_source_receipts_with_death_timing(&o, false, Some(t))
        .unwrap();
    let prepared = ai.clone();
    assert!(ai
        .source_combat_event_with_death_timing(&o, true, Some(t))
        .is_err());
    assert_eq!(ai, prepared);
    ai.finish_receipts(o, token, &mut |_, _, _| Ok(MainhandResult::Unchanged))
        .unwrap();
    assert_eq!(ai.effective_profile().respawn_delay_seconds, 8);
    assert_eq!(restore(&ai).unwrap(), ai);
}

#[test]
fn in_phase_death_timing_retains_pre_unit_corpse_semantics() {
    let mut p = profile();
    p.corpse_decay_ms = 0;
    let mut ai = SoloCreatureAi::new_generation_owned(p).unwrap();
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 2);
    let token = ai
        .begin_source_receipts_in_phase_with_death_timing(&o, false, Some(t))
        .unwrap();
    assert_eq!(
        ai.finish_receipts(o, token, &mut |_, _, _| Ok(MainhandResult::Unchanged))
            .unwrap(),
        vec![Effect::Died, Effect::CorpseRemoved]
    );
    assert!(ai.has_pre_unit_death());
    ai.validate_fixed_step(100).unwrap();
    assert_eq!(restore(&ai).unwrap(), ai);
}

#[test]
fn death_timer_overflow_does_not_install_delay_or_consume_receipt() {
    let mut p = profile();
    p.clock_origin_ms = u64::MAX - 1000;
    p.corpse_decay_ms = 2000;
    let mut ai = SoloCreatureAi::new_generation_owned(p).unwrap();
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 9);
    let before = ai.clone();
    assert!(ai
        .source_combat_event_with_death_timing(&o, true, Some(t))
        .is_err());
    assert_eq!(ai, before);
}

#[test]
fn respawn_boundary_is_required_and_cannot_be_checkpointed_or_replayed() {
    let mut ai = SoloCreatureAi::new_generation_owned(profile()).unwrap();
    let before = ai.clone();
    assert!(ai.rebind_after_respawn(profile(), 0, 1).is_err());
    assert_eq!(ai, before);
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 1);
    update(&mut ai, o, Some(t));
    idle_update(&mut ai, 200);
    assert_eq!(ai.phase(), Phase::Despawned);
    let before = ai.clone();
    assert!(ai
        .rebind_after_respawn(ai.effective_profile().clone(), 0, 1)
        .is_err());
    assert_eq!(ai, before);

    let mut ai = respawn_boundary();
    let before = ai.clone();
    assert!(restore(&ai).is_err());
    assert!(ai.validate_fixed_step(100).is_err());
    let o = observation(&ai, 100, vec![]);
    assert!(ai.begin_receipts(&o).is_err());
    assert!(ai.update_online_status(2, Some(2), false).is_err());
    assert!(ai.detach_dead_victim(2, ai.now_ms).is_err());
    assert_eq!(ai, before);
    for (old, new) in [(1, 2), (0, 0), (0, 2)] {
        assert!(ai
            .rebind_after_respawn(ai.effective_profile().clone(), old, new)
            .is_err());
        assert_eq!(ai, before);
    }
    let p = ai.effective_profile().clone();
    ai.rebind_after_respawn(p.clone(), 0, 1).unwrap();
    let after = ai.clone();
    assert!(ai.rebind_after_respawn(p, 0, 1).is_err());
    assert_eq!(ai, after);
    assert_eq!(ai.generation(), Some(1));
    assert_eq!(restore(&ai).unwrap(), ai);
    assert!(restore(&ai)
        .unwrap()
        .rebind_after_respawn(profile(), 1, 2)
        .is_err());
    ai.validate_fixed_step(100).unwrap();
}

#[test]
fn respawn_rebind_changes_only_selected_fields_and_preserves_every_other_field() {
    let mut ai = respawn_boundary();
    let mut projected = ai.effective_profile().clone();
    projected.level = 8;
    projected.combat_reach = real(2.);
    projected.bounding_radius = real(0.8);
    projected.run_speed = real(8.);
    projected.home_speed = real(4.);
    projected.mainhand_interval_ms = 1200;
    projected.corpse_decay_ms = 60_000;
    let mut expected = ai.clone();
    expected.profile = projected.clone();
    expected.generation_lifecycle = Some(GenerationLifecycle {
        generation: 1,
        pending_respawn: None,
    });
    ai.rebind_after_respawn(projected, 0, 1).unwrap();
    assert_eq!(ai, expected);
    assert_eq!(restore(&ai).unwrap(), ai);
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 9);
    update(&mut ai, o, Some(t));
    assert_eq!(ai.corpse_expiration_ms(), 61_100);
    assert_eq!(ai.respawn_at_ms, 10_000);
    assert_eq!(restore(&ai).unwrap(), ai);
}

#[test]
fn respawn_rejects_each_stable_profile_mutation_transactionally() {
    let ai = respawn_boundary();
    let mutations: &[fn(&mut Profile)] = &[
        |p| p.creature_id += 1,
        |p| p.map_id += 1,
        |p| p.content_revision = Revision([9; 32]),
        |p| p.spawn = Point::new(1., 0., 0.).unwrap(),
        |p| p.clock_origin_ms += 1,
        |p| p.reaction = Reaction::Aggressive,
        |p| p.neutral_to_all = true,
        |p| p.detection = real(19.),
        |p| p.aggro_rate = real(2.),
        |p| p.melee_leeway = real(3.),
        |p| p.pursuit_ms += 1,
        |p| p.leash_radius = real(31.),
        |p| p.hard_leash_radius = Some(real(40.)),
        |p| p.respawn_delay_seconds += 1,
        |p| p.unsupported.push(Unsupported::Dungeon),
        |p| p.level = 0,
        |p| p.mainhand_interval_ms = 0,
        |p| p.run_speed = Real::ZERO,
    ];
    for mutation in mutations {
        let mut next = ai.clone();
        let mut projected = ai.effective_profile().clone();
        mutation(&mut projected);
        assert!(next.rebind_after_respawn(projected, 0, 1).is_err());
        assert_eq!(next, ai);
    }
}

#[test]
fn interval_rebind_preserves_previous_life_attack_timer_and_counters() {
    let mut ai = SoloCreatureAi::new_generation_owned(profile()).unwrap();
    let mut attack = observation(
        &ai,
        100,
        vec![Receipt {
            id: 1,
            event: CombatEvent::AttackedBy { target: 2 },
        }],
    );
    attack.target = Some(Target {
        id: 2,
        position: Point::new(1., 0., 0.).unwrap(),
        level: 1,
        combat_reach: real(1.5),
        bounding_radius: real(0.5),
        moving_and_running: false,
        alive: true,
        online: true,
        can_attack_in_combat: true,
        can_attack_on_sight: true,
        visible: true,
        accessible: true,
        line_of_sight: true,
    });
    assert!(update(&mut ai, attack, None).contains(&Effect::MainhandSwing { target: 2 }));
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 1);
    update(&mut ai, o, Some(t));
    idle_update(&mut ai, 100);
    idle_update(&mut ai, 100);
    idle_update(&mut ai, 600);
    assert_eq!(ai.mainhand_timer_ms(), 1700);
    assert_eq!(ai.receipt_id, 2);
    assert!(ai.command_id > 0);
    let mut projected = ai.effective_profile().clone();
    projected.mainhand_interval_ms = 500;
    let before = ai.clone();
    ai.rebind_after_respawn(projected, 0, 1).unwrap();
    assert_eq!(ai.mainhand_timer_ms(), 1700);
    assert_eq!(
        (
            ai.sequence,
            ai.now_ms,
            ai.receipt_id,
            ai.command_id,
            ai.movement_request_id
        ),
        (
            before.sequence,
            before.now_ms,
            before.receipt_id,
            before.command_id,
            before.movement_request_id
        )
    );
    assert_eq!(restore(&ai).unwrap(), ai);
}

#[test]
fn checkpoint_rejects_wrong_profile_mode_generation_version_and_pending_marker() {
    let mut ai = respawn_boundary();
    let pending_checkpoint = ai.checkpoint();
    ai.rebind_after_respawn(ai.effective_profile().clone(), 0, 1)
        .unwrap();
    let checkpoint = ai.checkpoint();
    assert!(SoloCreatureAi::restore(
        checkpoint.clone(),
        ai.profile.clone(),
        ai.now_ms,
        ai.sequence,
        2
    )
    .is_err());
    assert!(SoloCreatureAi::restore_generation_owned(
        checkpoint.clone(),
        ai.profile.clone(),
        ai.now_ms,
        ai.sequence,
        2,
        0
    )
    .is_err());
    let mut changed = ai.profile.clone();
    changed.level += 1;
    assert!(SoloCreatureAi::restore_generation_owned(
        checkpoint.clone(),
        changed,
        ai.now_ms,
        ai.sequence,
        2,
        1
    )
    .is_err());
    let baseline = serde_json::to_value(checkpoint).unwrap();
    let pending = serde_json::to_value(pending_checkpoint).unwrap();
    let mut missing_mode = baseline.clone();
    missing_mode
        .as_object_mut()
        .unwrap()
        .remove("generation_lifecycle");
    assert!(serde_json::from_value::<AiCheckpoint>(missing_mode).is_err());
    let mut missing_marker = baseline.clone();
    missing_marker["generation_lifecycle"]
        .as_object_mut()
        .unwrap()
        .remove("pending_respawn");
    assert!(serde_json::from_value::<AiCheckpoint>(missing_marker).is_err());
    let mut changed = baseline.clone();
    changed["version"] = 5.into();
    let mut missing = baseline.clone();
    missing.as_object_mut().unwrap().remove("profile");
    assert!(serde_json::from_value::<AiCheckpoint>(missing).is_err());
    let mut marker = baseline.clone();
    marker["generation_lifecycle"]["pending_respawn"] =
        pending["generation_lifecycle"]["pending_respawn"].clone();
    let mut mode = baseline;
    mode["generation_lifecycle"] = serde_json::Value::Null;
    for value in [changed, marker, mode] {
        let checkpoint = serde_json::from_value(value).unwrap();
        assert!(SoloCreatureAi::restore_generation_owned(
            checkpoint,
            ai.profile.clone(),
            ai.now_ms,
            ai.sequence,
            2,
            1
        )
        .is_err());
    }
}

#[test]
fn legacy_ai_keeps_fixed_timing_without_generation_rebind_authority() {
    let mut ai = SoloCreatureAi::new(profile()).unwrap();
    let o = death_observation(&ai, 100);
    let t = timing(&ai, &o, 1);
    let before = ai.clone();
    assert!(ai
        .source_combat_event_with_death_timing(&o, true, Some(t))
        .is_err());
    assert_eq!(ai, before);
    update(&mut ai, o, None);
    idle_update(&mut ai, 200);
    idle_update(&mut ai, 4700);
    assert_eq!(ai.phase(), Phase::Idle);
    assert_eq!(ai.generation(), None);
    assert!(ai.rebind_after_respawn(ai.profile.clone(), 0, 1).is_err());
    assert_eq!(
        SoloCreatureAi::restore(
            ai.checkpoint(),
            ai.profile.clone(),
            ai.now_ms,
            ai.sequence,
            2
        )
        .unwrap(),
        ai
    );
}

#[test]
fn generation_zero_rejects_rehashed_impossible_attack_timer_limit() {
    let p = profile();
    let ai = SoloCreatureAi::new_generation_owned(p.clone()).unwrap();
    let mut value = serde_json::to_value(ai.checkpoint()).unwrap();
    value["mainhand_timer_limit_ms"] = serde_json::json!(60_000);
    value["mainhand_ms"] = serde_json::json!(60_000);
    let snapshot = serde_json::from_value(value).unwrap();
    assert!(SoloCreatureAi::restore_generation_owned(snapshot, p, 0, 0, 2, 0).is_err());
}
