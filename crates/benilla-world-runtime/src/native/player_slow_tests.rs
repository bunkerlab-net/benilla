// SPDX-License-Identifier: GPL-3.0-or-later
use super::{
    synthetic_fixture as f, synthetic_motion_fixture as motion_fixture,
    synthetic_slow_fixture as fixture, *,
};
use benilla_sim::player_slow::{MoveType, SpeedRates};
fn effect(
    tick: u64,
    spell_id: u32,
    phase: fixture::SyntheticSlowPhase,
) -> fixture::SyntheticSlowEffect {
    fixture::SyntheticSlowEffect {
        tick,
        spell_id,
        caster: crate::NpcId {
            spawn: 2,
            generation: 0,
        },
        phase,
    }
}
fn world(profile: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap()
}
fn authority() -> motion_fixture::SyntheticMotionAuthority {
    motion_fixture::SyntheticMotionAuthority {
        visits: Some(false),
        ..Default::default()
    }
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state().tick + 1, None, a).unwrap();
}
fn slow(w: &NativeWorld) -> &PlayerSlowState {
    w.state().player_slow.as_ref().unwrap()
}
fn events(w: &NativeWorld) -> Vec<PlayerSlowEvent> {
    w.last_tick()
        .unwrap()
        .events
        .iter()
        .filter_map(|e| match e {
            Event::PlayerSlow(e) => Some(e.clone()),
            _ => None,
        })
        .collect()
}
fn reopen(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
#[test]
fn actual_tick_creation_skip_late_creation_expiry_and_restore_follow_map_time() {
    for phase in [
        fixture::SyntheticSlowPhase::BeforePlayerUpdate,
        fixture::SyntheticSlowPhase::AfterPlayerUpdate,
    ] {
        let mut w = world(fixture::with_effects(vec![effect(1, 61004, phase)]));
        let start = w.checkpoint().unwrap();
        let mut a = authority();
        step(&mut w, &mut a);
        let holder = slow(&w).holders().next().unwrap();
        assert_eq!(holder.remaining_ms, 400);
        assert_eq!(
            holder.skip_update,
            phase == fixture::SyntheticSlowPhase::AfterPlayerUpdate
        );
        assert_eq!(holder.caster, w.state().npcs[&2].id);
        assert_eq!(holder.caster_level, w.profile().npcs[0].ai.level);
        assert_eq!(holder.amount, -40);
        assert_eq!(slow(&w).speeds(), SpeedRates::from_amounts([-40]).unwrap());
        let mut resumed = reopen(&w);
        let mut b = authority();
        for duration in [300, 200, 100] {
            step(&mut w, &mut a);
            step(&mut resumed, &mut b);
            assert_eq!(slow(&w).holders().next().unwrap().remaining_ms, duration);
            assert!(!slow(&w).holders().next().unwrap().skip_update);
            assert_eq!(w.state_digest(), resumed.state_digest());
        }
        step(&mut w, &mut a);
        step(&mut resumed, &mut b);
        assert_eq!(w.state_digest(), resumed.state_digest());
        assert!(slow(&w).holders().next().is_none());
        assert_eq!(slow(&w).speeds(), SpeedRates::normal());
        assert!(matches!(
            events(&w).last(),
            Some(PlayerSlowEvent::Removed {
                reason: PlayerSlowRemoval::Expire,
                ..
            })
        ));
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), start).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
    }
}
#[test]
fn same_caster_replacement_removes_then_reapplies_not_timer_refresh() {
    use fixture::SyntheticSlowPhase::BeforePlayerUpdate as Before;
    let mut w = world(fixture::with_effects(vec![
        effect(1, 61004, Before),
        effect(2, 61004, Before),
    ]));
    let mut a = authority();
    step(&mut w, &mut a);
    let old = slow(&w).holders().next().unwrap().clone();
    step(&mut w, &mut a);
    let new = slow(&w).holders().next().unwrap();
    assert_eq!(new.id, old.id + 1);
    assert_eq!(new.slot, old.slot);
    assert_eq!(new.remaining_ms, 400);
    let e = events(&w);
    assert_eq!(e.len(), 8);
    assert!(
        matches!(&e[0],PlayerSlowEvent::SpeedChanged{kind:MoveType::Run,rate,..} if *rate==ai::Real::ONE)
    );
    assert!(matches!(
        &e[1],
        PlayerSlowEvent::SpeedChanged {
            kind: MoveType::RunBack,
            ..
        }
    ));
    assert!(matches!(
        &e[2],
        PlayerSlowEvent::SpeedChanged {
            kind: MoveType::Swim,
            ..
        }
    ));
    assert!(
        matches!(&e[3],PlayerSlowEvent::Removed{holder,reason:PlayerSlowRemoval::Stack,..} if holder.id==old.id)
    );
    assert!(matches!(&e[4],PlayerSlowEvent::Applied{holder} if holder.id==new.id));
    assert!(matches!(
        &e[5],
        PlayerSlowEvent::SpeedChanged {
            kind: MoveType::Run,
            ..
        }
    ));
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}
#[test]
fn simultaneous_holders_use_strongest_negative_and_expire_in_source_order() {
    use fixture::SyntheticSlowPhase::BeforePlayerUpdate as Before;
    let mut w = world(fixture::with_effects(vec![
        effect(1, 61004, Before),
        effect(1, 61005, Before),
    ]));
    let mut a = authority();
    step(&mut w, &mut a);
    assert_eq!(slow(&w).holders().count(), 2);
    assert_eq!(slow(&w).speeds(), SpeedRates::from_amounts([-60]).unwrap());
    for _ in 0..4 {
        step(&mut w, &mut a);
    }
    assert_eq!(slow(&w).holders().count(), 1);
    assert!(events(&w)
        .iter()
        .all(|e| !matches!(e, PlayerSlowEvent::SpeedChanged { .. })));
    assert!(
        matches!(events(&w).last(),Some(PlayerSlowEvent::Removed{holder,reason:PlayerSlowRemoval::Expire,..}) if holder.spell_id==61004)
    );
    for _ in 0..2 {
        step(&mut w, &mut a);
    }
    assert_eq!(slow(&w).speeds(), SpeedRates::normal());
}
#[test]
fn caster_death_does_not_remove_holder_and_captured_identity_survives_reload() {
    let mut w = fixture::world();
    let mut a = authority();
    step(&mut w, &mut a);
    let original = slow(&w).holders().next().unwrap().caster;
    // Fixture-only staged death uses the actual NPC lifecycle on the next runtime update.
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    step(&mut w, &mut a);
    assert_eq!(w.state().npcs[&2].ai.phase(), ai::Phase::Corpse);
    assert_eq!(slow(&w).holders().next().unwrap().caster, original);
    assert_eq!(slow(&w).holders().next().unwrap().remaining_ms, 300);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}
#[test]
fn failed_authority_rolls_back_applied_holder_events_clock_and_allocator() {
    let mut w = fixture::world();
    let before = w.state_digest();
    let mut a = authority();
    a.missing = true;
    assert!(w.step(1, None, &mut a).is_err());
    assert_eq!(before, w.state_digest());
    assert!(w.records().is_empty());
    a.missing = false;
    step(&mut w, &mut a);
    assert_eq!(slow(&w).holders().next().unwrap().id, 1 << 32);
    assert_eq!(
        events(&w)
            .iter()
            .filter(|e| matches!(e, PlayerSlowEvent::Applied { .. }))
            .count(),
        1
    );
}
#[test]
fn different_caster_same_spell_is_explicit_unsupported_in_private_state_seam() {
    // Motion currently admits one NPC. Direct state staging isolates this unavailable branch
    // without relaxing that production gate or claiming a second NPC runtime callback.
    let mut w = fixture::world();
    step(&mut w, &mut authority());
    let mut next = slow(&w).clone();
    let mut events = vec![];
    assert_eq!(
        next.apply(
            w.profile(),
            w.state().world,
            w.state().player,
            crate::NpcId {
                spawn: 3,
                generation: 0
            },
            w.profile().npcs[0].ai.level,
            w.state().now_ms,
            false,
            61004,
            &mut events
        ),
        Err(NativeError::UnsupportedMechanic(
            "same spell from different slow-aura casters"
        ))
    );
    assert_eq!(&next, slow(&w));
    assert!(events.is_empty());
}
struct MeleeAuthority {
    motion: motion_fixture::SyntheticMotionAuthority,
    melee: f::KernelAuthority,
}
impl Authority for MeleeAuthority {
    fn resolve(&mut self, r: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(r.query, Query::NativeMelee { .. }) {
            self.melee.resolve(r)
        } else {
            self.motion.resolve(r)
        }
    }
}
#[test]
fn actual_lethal_melee_removes_active_holders_with_callback_owned_source_unit_level() {
    use fixture::SyntheticSlowPhase::BeforePlayerUpdate as Before;
    for previous_life in [false, true] {
        let mut p = f::initialized_profile();
        p.npcs[0].ai.spawn = f::point(1.0);
        p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
        p.npcs[0].ai.corpse_decay_ms = 100;
        p.npcs[0].ai.respawn_delay_seconds = 1;
        p.npcs[0].combat.as_mut().unwrap().minimum_damage = f::float(10000.0);
        p.npcs[0].combat.as_mut().unwrap().maximum_damage = f::float(10000.0);
        p.source_motion = Some(motion_fixture::genesis(&p, 2));
        p.player_slow = Some(
            PlayerSlowGenesis::from_sources(
                vec![player_slow::source::synthetic_source(61004, -40, 6000)],
                player_slow::host::synthetic_host(&p),
            )
            .unwrap()
            .with_fixture_effects(vec![effect(1, 61004, Before)]),
        );
        p.configure_player_death(
            benilla_sim::player_death::DeathConfig {
                map_instanceable: false,
                pve_reclaim_delay_enabled: true,
            },
            benilla_sim::player_death::DeathSupport::checked(
                &benilla_sim::player_death::REQUIRED_ABSENCES.map(|d| (d, Some(false))),
            )
            .unwrap(),
        )
        .unwrap();
        let mut w = world(p);
        let mut a = MeleeAuthority {
            motion: authority(),
            melee: f::KernelAuthority::default(),
        };
        step(&mut w, &mut a);
        if previous_life {
            // Authored prior caster death exercises the actual corpse/DEAD/respawn lifecycle.
            w.state.npcs.get_mut(&2).unwrap().health = 0;
            while w.state().npcs[&2].id.generation == 0 {
                assert!(w.state().tick < 25);
                step(&mut w, &mut a);
            }
            assert_eq!(slow(&w).holders().next().unwrap().caster.generation, 0);
            assert_eq!(w.state().npcs[&2].id.generation, 1);
        }
        a.motion.visits = Some(true);
        // A source relocation notification starts ordinary aggressive combat.
        w.state
            .npcs
            .get_mut(&2)
            .unwrap()
            .events
            .schedule_notify(100, true)
            .unwrap();
        while w.state().health > 0 && w.state().tick < 35 {
            step(&mut w, &mut a);
        }
        assert_eq!(w.state().health, 0);
        assert!(slow(&w).holders().next().is_none());
        assert_eq!(slow(&w).speeds(), SpeedRates::normal());
        assert_eq!(slow(&w).slot_levels()[0], w.profile().npcs[0].ai.level);
        let events = &w.last_tick().unwrap().events;
        let removal=events.iter().position(|e| matches!(e,Event::Death(benilla_sim::player_death::DeathAction::RemoveAuraByDeath{holder_id,..}) if *holder_id>=1<<32)).unwrap();
        let speed=events.iter().position(|e| matches!(e,Event::PlayerSlow(PlayerSlowEvent::SpeedChanged{rate,..}) if *rate==ai::Real::ONE)).unwrap();
        let clear = events
            .iter()
            .position(|e| {
                matches!(
                    e,
                    Event::PlayerSlow(PlayerSlowEvent::Removed {
                        reason: PlayerSlowRemoval::Death,
                        ..
                    })
                )
            })
            .unwrap();
        assert!(removal < speed && speed < clear);
        assert_eq!(reopen(&w).state_digest(), w.state_digest());
    }
}
#[test]
fn movement_queries_use_incoming_walk_backwards_and_live_slow_budget() {
    let mut w = fixture::world();
    let mut a = authority();
    step(&mut w, &mut a);
    for (bits, mode) in [
        (0x101, MoveType::Walk),
        (2, MoveType::RunBack),
        (1, MoveType::Run),
    ] {
        let expected = slow(&w).speeds().speed(mode).unwrap();
        let input = OwnedInput {
            authenticated_owner: w.state().player.owner,
            input: Input {
                world: w.state().world,
                player: w.state().player,
                tick: w.state().tick + 1,
                sequence: w.state().next_sequence,
                command: Command::MovementPose {
                    destination: f::point(w.state().position.x.as_f32() + 0.01),
                    orientation: Orientation::ZERO,
                    movement_flags: benilla_sim::creature_motion::GroundMovementFlags::from_bits(
                        bits,
                    )
                    .unwrap(),
                },
            },
        };
        w.step(w.state().tick + 1, Some(input), &mut a).unwrap();
        assert!(w.last_tick().unwrap().replies.iter().any(|r|matches!(r.request.query,Query::PlayerMovement{speed:Some(speed),..} if speed==expected)));
    }
}
fn repaired(value: serde_json::Value) -> Checkpoint {
    let mut checkpoint: Checkpoint = serde_json::from_value(value).unwrap();
    checkpoint.digest = crate::digest(b"benilla-native-state-v64\0", &checkpoint.state);
    checkpoint.checksum = crate::digest(
        b"benilla-native-checkpoint-v64\0",
        &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
    );
    checkpoint
}
#[test]
fn repaired_checksum_cannot_forge_amount_duration_caster_binding_slot_or_rates() {
    let mut w = fixture::world();
    step(&mut w, &mut authority());
    let original = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    assert_eq!(
        NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            repaired(original.clone())
        )
        .unwrap()
        .state_digest(),
        w.state_digest()
    );
    for (field, value) in [
        ("amount", serde_json::json!(-80)),
        ("remaining_ms", serde_json::json!(300)),
        ("caster_level", serde_json::json!(2)),
        ("slot", serde_json::json!(12)),
        ("skip_update", serde_json::json!(true)),
        ("id", serde_json::json!(1)),
    ] {
        let mut bad = original.clone();
        bad["state"]["player_slow"]["holders"][0][field] = value;
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile().clone(), repaired(bad))
                .is_err(),
            "{field}"
        );
    }
    let mut bad = original.clone();
    bad["state"]["player_slow"]["speeds"]["run"] = serde_json::json!(1.0_f32.to_bits());
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), repaired(bad)).is_err()
    );
    let mut old = original;
    old["version"] = serde_json::json!(13);
    assert!(Checkpoint::decode(&serde_json::to_vec(&old).unwrap()).is_err());
}

#[test]
fn fixture_effect_budget_is_bounded_before_frame_and_allocation_is_restorable() {
    use fixture::SyntheticSlowPhase::BeforePlayerUpdate as Before;
    let effects = vec![effect(1, 61004, Before); MAX_INPUTS_PER_TICK];
    let mut w = world(fixture::with_effects(effects));
    step(&mut w, &mut authority());
    assert_eq!(slow(&w).holders().next().unwrap().id, (1 << 32) + 63);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
    let p = fixture::with_effects(vec![effect(1, 61004, Before); MAX_INPUTS_PER_TICK + 1]);
    assert_eq!(p.validate(), Err(NativeError::TooLarge));
}
#[test]
fn historical_caster_generation_and_source_unit_level_survive_ordinary_respawn() {
    use fixture::SyntheticSlowPhase::BeforePlayerUpdate as Before;
    let mut p = fixture::with_effects(vec![]);
    p.npcs[0].ai.corpse_decay_ms = 100;
    p.npcs[0].ai.respawn_delay_seconds = 1;
    let mut host = player_slow::host::synthetic_host(&p);
    host.max_player_level = 59;
    p.player_slow = Some(
        PlayerSlowGenesis::from_sources(
            vec![player_slow::source::synthetic_source(61004, -40, 3000)],
            host,
        )
        .unwrap()
        .with_fixture_effects(vec![effect(1, 61004, Before)]),
    );
    let mut w = world(p);
    let mut a = authority();
    step(&mut w, &mut a);
    let original = slow(&w).holders().next().unwrap().caster;
    w.state.npcs.get_mut(&2).unwrap().health = 0;
    while w.state().npcs[&2].id.generation == 0 {
        assert!(w.state().tick < 25);
        step(&mut w, &mut a);
        assert_eq!(reopen(&w).state_digest(), w.state_digest());
    }
    assert_eq!(slow(&w).holders().next().unwrap().caster, original);
    while slow(&w).holders().next().is_some() {
        step(&mut w, &mut a);
    }
    assert_eq!(slow(&w).slot_levels()[0], w.profile().npcs[0].ai.level);
    assert!(matches!(
        events(&w).last(),
        Some(PlayerSlowEvent::Removed { slot_level: 1, .. })
    ));
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}

#[test]
fn truly_absent_unit_metadata_uses_explicit_config_without_granting_application_authority() {
    let mut w = fixture::world();
    step(&mut w, &mut authority());
    let id = slow(&w).holders().next().unwrap().id;
    // Permanent Unit removal is not an admitted world lifecycle. This private lookup test
    // isolates the source fallback, rather than conflating ordinary DEAD/respawn with deletion.
    assert_eq!(
        slow(&w)
            .removal_level(id, w.profile(), &BTreeMap::new())
            .unwrap(),
        63
    );
}

#[test]
fn untouched_and_unreachable_slot_residue_cannot_be_forged_with_repaired_hashes() {
    let mut w = fixture::world();
    let initial = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    assert_eq!(
        NativeWorld::restore_synthetic_component_test(
            w.profile().clone(),
            repaired(initial.clone())
        )
        .unwrap()
        .state_digest(),
        w.state_digest()
    );
    let mut bad = initial;
    bad["state"]["player_slow"]["slot_levels"][0] = serde_json::json!(1);
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), repaired(bad)).is_err()
    );
    step(&mut w, &mut authority());
    let mut bad = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    bad["state"]["player_slow"]["holders"][0]["slot"] = serde_json::json!(47);
    bad["state"]["player_slow"]["slot_levels"][0] = serde_json::json!(0);
    bad["state"]["player_slow"]["slot_levels"][15] = serde_json::json!(1);
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), repaired(bad)).is_err()
    );
}

#[test]
fn respawn_retains_holder_history_but_only_current_generation_can_apply_new_effects() {
    use fixture::SyntheticSlowPhase::BeforePlayerUpdate as Before;
    for stale in [false, true] {
        let mut p = fixture::with_effects(vec![]);
        p.npcs[0].ai.corpse_decay_ms = 100;
        p.npcs[0].ai.respawn_delay_seconds = 1;
        let replacement = fixture::SyntheticSlowEffect {
            caster: crate::NpcId {
                spawn: 2,
                generation: u64::from(!stale),
            },
            ..effect(30, 61004, Before)
        };
        p.player_slow = Some(
            PlayerSlowGenesis::from_sources(
                vec![player_slow::source::synthetic_source(61004, -40, 6000)],
                player_slow::host::synthetic_host(&p),
            )
            .unwrap()
            .with_fixture_effects(vec![effect(1, 61004, Before), replacement]),
        );
        let mut w = world(p);
        let mut a = authority();
        step(&mut w, &mut a);
        w.state.npcs.get_mut(&2).unwrap().health = 0;
        while w.state().tick < 29 {
            step(&mut w, &mut a);
        }
        assert_eq!(w.state().npcs[&2].id.generation, 1);
        assert_eq!(slow(&w).holders().next().unwrap().caster.generation, 0);
        let before = w.state_digest();
        if stale {
            assert_eq!(w.step(30, None, &mut a), Err(NativeError::UnknownTarget));
            assert_eq!(before, w.state_digest());
        } else {
            step(&mut w, &mut a);
            assert_eq!(slow(&w).holders().next().unwrap().caster.generation, 1);
            assert!(events(&w).iter().any(|e| matches!(
                e,
                PlayerSlowEvent::Removed {
                    reason: PlayerSlowRemoval::Stack,
                    slot_level: 1,
                    ..
                }
            )));
            assert_eq!(reopen(&w).state_digest(), w.state_digest());
        }
    }
}
