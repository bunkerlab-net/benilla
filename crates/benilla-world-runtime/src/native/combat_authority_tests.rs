// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_combat_fixture as fixture, synthetic_fixture as f, *};
use ai::movement::Check;
fn world(p: Profile, seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), seed).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) -> Result<(), NativeError> {
    w.step(w.state.tick + 1, None, a).map(|_| ())
}
fn melee_request(reply: &AuthorityReply) -> Option<&CombatRequest> {
    match &reply.request.query {
        Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeDamage(q))) => Some(q),
        _ => None,
    }
}
#[test]
fn source_combat_uses_current_notify_post_movement_ai_and_post_health_then_replays() {
    let mut w = world(fixture::profile(), 7);
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SpatialOnly::default();
    step(&mut w, &mut a).unwrap();
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    let initial_semantics = a
        .derived
        .iter()
        .filter_map(|r| match &r.facts {
            Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcSemantics(f))) => Some(f),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!initial_semantics.is_empty());
    assert!(initial_semantics.iter().all(|f| !f.can_initiate_attack));
    for _ in 0..5 {
        step(&mut w, &mut a).unwrap();
    }
    assert!(a.derived.iter().any(|r| melee_request(r).is_some()));
    for record in w.records() {
        let mut front = None;
        for (ordinal, reply) in record.replies.iter().enumerate() {
            let Query::SourceMotion(MotionQuery::Combat(query)) = &reply.request.query else {
                continue;
            };
            let request = match query {
                CombatQuery::NpcSemantics(q)
                | CombatQuery::NpcVisibility(q)
                | CombatQuery::NpcAccessibility(q)
                | CombatQuery::NpcLineOfSight(q)
                | CombatQuery::MeleeFront(q)
                | CombatQuery::MeleeDamage(q)
                | CombatQuery::MeleeCombatEntry(q) => q,
            };
            assert_eq!(
                request.witness.frame.moment,
                InteractionMoment::Frame {
                    action_ordinal: ordinal as u32
                }
            );
            if let CombatQuery::MeleeFront(q) = query {
                assert_eq!(q.phase, CombatPhase::MainhandFront);
                assert_eq!(q.creature_phase, ai::Phase::Combat);
                assert_eq!(q.creature_ai_target, Some(SOLO_UNIT));
                front = Some(q);
            }
            if let CombatQuery::MeleeDamage(q) = query {
                let before = front.unwrap();
                assert_eq!(q.phase, CombatPhase::SurvivingMeleeDamage);
                assert!(q.player_health > 0 && q.player_health < before.player_health);
                assert_eq!(q.creature_pose, before.creature_pose);
                assert_eq!(q.action, before.action);
                assert_ne!(q.witness.frame.draft, before.witness.frame.draft);
                assert_eq!(q.witness.working_npc, before.witness.working_npc);
            }
        }
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.checkpoint().unwrap(), replay.checkpoint().unwrap());
    let cp = Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap();
    let mut reopened =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp).unwrap();
    step(&mut reopened, &mut fixture::SpatialOnly::default()).unwrap();
    step(&mut w, &mut a).unwrap();
    assert_eq!(w.state_digest(), reopened.state_digest());
}
#[test]
fn separate_spatial_unknown_stale_and_wrong_kind_roll_back_the_entire_frame() {
    for kind in 0..6 {
        let mut w = world(fixture::profile(), 7);
        let mut a = fixture::SpatialOnly::default();
        step(&mut w, &mut a).unwrap();
        let before = w.checkpoint().unwrap();
        match kind {
            0 => a.visibility = Some(Check::Unknown),
            1 => a.accessibility = Some(Check::Unknown),
            2 => a.line_of_sight = Some(Check::Unknown),
            3 => a.front = Some(Check::Unknown),
            4 => a.stale_spatial = true,
            _ => a.wrong_spatial_kind = true,
        }
        assert!(step(&mut w, &mut a).is_err(), "kind {kind}");
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records().len(), 1);
    }
    for kind in 0..3 {
        let mut w = world(fixture::profile(), 7);
        let mut a = fixture::SpatialOnly::default();
        match kind {
            0 => a.visibility = Some(Check::Failed),
            1 => a.accessibility = Some(Check::Failed),
            _ => a.line_of_sight = Some(Check::Failed),
        }
        for _ in 0..3 {
            step(&mut w, &mut a).unwrap();
        }
        assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
        assert!(w.records().iter().all(|r| r.draws.is_empty()));
    }
}
#[test]
fn late_post_health_derived_receipt_failure_rolls_back_rng_input_and_clocks() {
    let mut w = world(fixture::profile(), 7);
    let mut a = fixture::SpatialOnly::default();
    step(&mut w, &mut a).unwrap();
    let mut expected = w.clone();
    let before = w.checkpoint().unwrap();
    a.fail_post_damage = true;
    assert_eq!(
        step(&mut w, &mut a),
        Err(NativeError::AuthorityUnavailable(
            "authored post-health receipt fault"
        ))
    );
    assert_eq!(w.checkpoint().unwrap(), before);
    assert!(a.derived.iter().any(|r| melee_request(r).is_some()));
    a.fail_post_damage = false;
    step(&mut w, &mut a).unwrap();
    step(&mut expected, &mut fixture::SpatialOnly::default()).unwrap();
    assert_eq!(w.last_tick(), expected.last_tick());
    assert_eq!(w.state_digest(), expected.state_digest());
}
#[test]
fn lethal_mainhand_skips_surviving_damage_predicate_and_replays_death() {
    use benilla_sim::player_death as d;
    let mut p = fixture::profile();
    p.npcs[0].combat.as_mut().unwrap().minimum_damage = f::float(10000.0);
    p.npcs[0].combat.as_mut().unwrap().maximum_damage = f::float(10000.0);
    p.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|d| (d, Some(false)))).unwrap(),
    )
    .unwrap();
    let mut w = world(p, 7);
    let mut replay = w.clone();
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..5 {
        if w.state.health == 0 {
            break;
        }
        step(&mut w, &mut a).unwrap();
    }
    assert_eq!(w.state.health, 0);
    assert!(a.queries.iter().any(|r| matches!(
        r.query,
        Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(_)))
    )));
    assert!(a.derived.iter().all(|r| melee_request(r).is_none()));
    assert!(a.derived.iter().any(|r| matches!(&r.facts,
        Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeCombatEntry(f)))
            if f.player_can_enter_combat && f.npc_can_enter_combat)));

    assert!(a.derived.iter().any(|r| matches!(&r.request.query, Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeCombatEntry(q))) if q.player_health == 0 && q.phase == CombatPhase::AfterMeleeDamage)));

    assert!(w
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, Event::Death(_))));
    replay.replay(w.records()).unwrap();
    assert_eq!(w.state_digest(), replay.state_digest());
}
#[test]
fn explicit_source_selection_and_raw_flags_are_strictly_profile_bound() {
    let mut p = fixture::profile();
    let before = p.digest();
    for rows in [
        vec![],
        vec![NpcCombatAuthoritySource {
            entry: 999,
            extra_flags: 0,
            static_flags1: 0,
            static_flags3: 0,
            static_flags4: 0,
            static_flags2: 0,
        }],
        vec![NpcCombatAuthoritySource {
            entry: p.npcs[0].entry,
            extra_flags: 1,
            static_flags1: 0,
            static_flags3: 0,
            static_flags4: 0,
            static_flags2: 0,
        }],
        vec![NpcCombatAuthoritySource {
            entry: p.npcs[0].entry,
            extra_flags: 0,
            static_flags1: 0,
            static_flags3: 0,
            static_flags4: 0,
            static_flags2: 1,
        }],
    ] {
        assert!(p.initialize_combat_authority(rows).is_err());
        assert_eq!(p.digest(), before);
    }
    let w = world(p.clone(), 7);
    let cp = w.checkpoint().unwrap();
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: p.npcs[0].entry,
        extra_flags: 0x80000,
        static_flags1: 0,
        static_flags3: 0,
        static_flags4: 0,
        static_flags2: 0x100,
    }])
    .unwrap();
    assert_ne!(p.digest(), before);
    assert!(NativeWorld::restore_synthetic_component_test(p.clone(), cp).is_err());
    p.source_motion = None;
    assert!(NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).is_err());
    let mut p = f::initialized_profile();
    assert!(p.initialize_combat_authority(vec![]).is_err());
    let mut p = fixture::profile();
    p.npcs[0].health_regeneration.creature_type_flags = 0x8000_0000;
    assert!(p.initialize_interactions(fixture::source_rows(&p)).is_err());
    for field in ["control", "script", "unit_flags"] {
        let mut p = fixture::profile();
        let before = p.digest();
        let mut rows = fixture::source_rows(&p);
        match field {
            "control" => rows.npcs[0].control.charmer_guid = 1,
            "script" => rows.audit.npc_aura_or_script_hooks = InteractionEvidence::Unknown,
            _ => rows.npcs[0].unit_flags = 0x8000_0000,
        }
        assert!(p.initialize_interactions(rows).is_err());
        assert_eq!(p.digest(), before);
    }
}

#[test]
fn asymmetric_relations_do_not_substitute_npc_sight_for_victim_can_attack() {
    let mut p = fixture::profile();
    let mut rows = fixture::source_rows(&p);
    rows.factions[0].enemy_group_mask = 0;
    rows.factions[0].friend_group_mask |= 2;
    p.initialize_interactions(rows).unwrap();
    assert!(
        p.source_combat_authority(),
        "interaction update must preserve explicit host selection"
    );
    let mut w = world(p, 7);
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..3 {
        step(&mut w, &mut a).unwrap();
    }
    assert!(a.derived.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcSemantics(f))) if f.can_attack_in_combat && f.can_attack_on_sight)));
    assert!(a.derived.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeDamage(f))) if !f.player_can_attack_npc && f.player_can_enter_combat && f.npc_can_enter_combat)));
}

#[test]
fn friendly_at_war_runtime_retaliates_without_becoming_an_on_sight_enemy() {
    use benilla_sim::reputation as rep;
    use synthetic_reputation_fixture as rep_fixture;
    let (mut p, input, mut data) = rep_fixture::setup();
    p.npcs.retain(|n| n.entry == 60003);
    data.kill.creatures.retain(|n| n.entry == 60003);
    p.npcs[0].faction_template = 2;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let original = rep_fixture::catalog();
    let mut rows = [
        rep_fixture::PRIMARY,
        rep_fixture::SPILLOVER,
        rep_fixture::OTHER,
    ]
    .map(|id| original.row(id).unwrap().unwrap().clone());
    rows[0].faction.base_values[0] = 3000;
    data.reputation = Some(
        rep::Catalog::new(
            rows.into(),
            vec![],
            original.config(),
            rep::Audit {
                general_reputation_auras: rep::Evidence::Absent,
                faction_reputation_auras: rep::Evidence::Absent,
                forced_reactions: rep::Evidence::Absent,
                reputation_scripts: rep::Evidence::Absent,
            },
        )
        .unwrap(),
    );
    p.initialize_quest_progression(&input, data).unwrap();
    p.source_motion = Some(synthetic_motion_fixture::genesis(&p));
    let mut rows = fixture::source_rows(&p);
    rows.npcs[0].npc_flags = 2;
    rows.factions[1].faction = rep_fixture::PRIMARY;
    rows.factions[1].reputation_list_id = Some(3);
    // Tracked reputation makes Unit::IsNeutralToAll false even when the raw
    // template alone has no friendly/enemy masks or explicit enemy rows.
    rows.factions[1].enemy_group_mask = 0;
    rows.factions[1].friend_group_mask = 0;
    p.initialize_interactions(rows).unwrap();
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: 60003,
        extra_flags: 0,
        static_flags1: 0,
        static_flags3: 0,
        static_flags4: 0,
        static_flags2: 0,
    }])
    .unwrap();
    let mut w = world(p, 7);
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..3 {
        step(&mut w, &mut a).unwrap();
    }
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Idle);
    assert!(a.derived.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcSemantics(f))) if f.can_attack_in_combat && !f.can_attack_on_sight)));
    let command = OwnedInput {
        authenticated_owner: w.state.player.owner,
        input: Input {
            world: w.state.world,
            player: w.state.player,
            tick: w.state.tick + 1,
            sequence: w.state.next_sequence,
            command: Command::Cast {
                ability: 60001,
                target: w.state.npcs[&3].id,
            },
        },
    };
    w.step(w.state.tick + 1, Some(command), &mut a).unwrap();
    assert_eq!(
        w.state.casts.len(),
        1,
        "{:?}",
        w.last_tick().unwrap().events
    );
    // Source projectile travel applies its minimum distance; wait for the
    // persisted release deadline through the actual fixed-step dispatcher.
    while matches!(
        w.state.casts[&1].execution.state(),
        spell::CastState::Casting { .. }
    ) {
        step(&mut w, &mut a).unwrap();
    }
    let spell::CastState::Traveling { impact_at_ms } = w.state.casts[&1].execution.state() else {
        panic!("authored cast did not launch")
    };
    assert!(impact_at_ms > w.state.now_ms);
    while w.state.now_ms < impact_at_ms {
        assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Idle);
        assert!(!a.derived.iter().any(|r| matches!(&r.request.query,
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(q)))
            if q.phase == CombatPhase::CombatReceipt)));
        assert!(!w.last_tick().unwrap().events.iter().any(|e| matches!(
            e,
            Event::Spell {
                event: spell::SpellEvent::DirectDamage { .. },
                ..
            }
        )));
        step(&mut w, &mut a).unwrap();
    }
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(e,
        Event::Spell { event: spell::SpellEvent::DirectDamage { effective, .. }, .. } if *effective > 0)));
    assert!(w.last_tick().unwrap().events.iter().any(|e| matches!(
        e,
        Event::Npc {
            effect: ai::Effect::CombatStarted { target: SOLO_UNIT },
            ..
        }
    )));
    assert!(a.derived.iter().any(|r| matches!((&r.request.query, &r.facts),
        (Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(q))),
         Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcSemantics(f))))
        if q.phase == CombatPhase::CombatReceipt && f.can_attack_in_combat && !f.can_attack_on_sight)));
    assert_eq!(w.state.npcs[&3].ai.phase(), ai::Phase::Combat);
    assert!(a.derived.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeDamage(f))) if f.player_can_attack_npc)));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(w.state_digest(), replay.state_digest());
}

#[test]
fn moving_mainhand_uses_live_ai_and_facing_not_the_pre_movement_npc_snapshot() {
    let mut p = fixture::profile();
    p.npcs[0].ai.spawn = f::point(8.0);
    let mut w = world(p, 7);
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..30 {
        step(&mut w, &mut a).unwrap();
        if w.last_tick()
            .unwrap()
            .events
            .iter()
            .any(|e| matches!(e, Event::Melee { .. }))
        {
            break;
        }
    }
    let record = w.last_tick().unwrap();
    let after_movement = record
        .replies
        .iter()
        .find_map(|r| match &r.request.query {
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::NpcSemantics(q)))
                if q.phase == CombatPhase::AfterMovement =>
            {
                Some(q)
            }
            _ => None,
        })
        .unwrap();
    let front = record
        .replies
        .iter()
        .find_map(|r| match &r.request.query {
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(q))) => Some(q),
            _ => None,
        })
        .unwrap();
    assert!(front.creature_pose.position.x.as_f32() < 8.0);
    assert_eq!(
        front.creature_pose.position,
        after_movement.creature_pose.position
    );
    assert_ne!(
        front.witness.working_npc,
        after_movement.witness.working_npc
    );
    assert_eq!(front.creature_ai_target, Some(SOLO_UNIT));
}

#[test]
fn miss_survival_and_lethal_keep_distinct_source_predicate_order() {
    let mut w = world(fixture::profile(), 7);
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..60 {
        step(&mut w, &mut a).unwrap();
    }
    let mut saw_miss = false;
    let mut saw_damage = false;
    for record in w.records() {
        let Some((damage, missed)) = record.events.iter().find_map(|e| match e {
            Event::Melee {
                effective, outcome, ..
            } => Some((*effective, outcome.hit_info & 0x10 != 0)),
            _ => None,
        }) else {
            continue;
        };
        let phases = record
            .replies
            .iter()
            .filter_map(|r| match &r.request.query {
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(_))) => Some(0),
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeDamage(_))) => Some(1),
                Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeCombatEntry(_))) => {
                    Some(2)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if damage == 0 {
            saw_miss |= missed;
            assert_eq!(phases, [0, 2]);
        } else {
            saw_damage = true;
            assert_eq!(phases, [0, 1, 2]);
        }
    }
    assert!(saw_miss && saw_damage);
}

#[test]
fn captured_force_capability_is_not_a_generic_attack_or_home_override() {
    use benilla_sim::classic_targets::unit_flags;
    for force in [false, true] {
        let mut p = fixture::profile();
        p.npcs[0].health_regeneration.unit_flags = unit_flags::UNINTERACTIBLE;
        p.initialize_interactions(fixture::source_rows(&p)).unwrap();
        p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
            entry: p.npcs[0].entry,
            extra_flags: if force { 0x80000 } else { 0 },
            static_flags1: 0,
            static_flags3: 0,
            static_flags4: 0,
            static_flags2: 0,
        }])
        .unwrap();
        let mut w = world(p, 7);
        let mut a = fixture::SpatialOnly::default();
        for _ in 0..3 {
            step(&mut w, &mut a).unwrap();
        }
        assert_eq!(w.state.npcs[&2].ai.phase() == ai::Phase::Combat, force);
        if force {
            assert!(a.derived.iter().any(|r| matches!(&r.facts, Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeDamage(f))) if !f.player_can_attack_npc)));
        }
    }
    let mut p = fixture::profile();
    p.npcs[0].ai.spawn = f::point(8.0);
    p.npcs[0].ai.hard_leash_radius = Some(f::real(0.1));
    let mut w = world(p, 7);
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..20 {
        step(&mut w, &mut a).unwrap();
        if w.state.npcs[&2].ai.phase() == ai::Phase::ReturningHome {
            break;
        }
    }
    let npc = w.state.npcs.remove(&2).unwrap();
    assert_eq!(npc.ai.phase(), ai::Phase::ReturningHome);
    let view = interactions::WorkingNpc::from_npc(&npc).unwrap();
    let facts = view.combat_entry(&w.profile).unwrap();
    assert!(facts.player_can_enter_combat);
    assert!(!facts.npc_can_enter_combat);
}

#[test]
fn replay_does_not_accept_forged_semantics_or_stale_working_npc_witness() {
    let mut w = world(fixture::profile(), 7);
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..3 {
        step(&mut w, &mut a).unwrap();
    }
    for attack in 0..5 {
        let mut records = w.records().to_vec();
        let reply = records[1]
            .replies
            .iter_mut()
            .find(|r| {
                matches!(
                    r.request.query,
                    Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeDamage(_)))
                )
            })
            .unwrap();
        if attack == 0 {
            let Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeDamage(facts))) =
                &mut reply.facts
            else {
                unreachable!()
            };
            facts.player_can_attack_npc = !facts.player_can_attack_npc;
        } else {
            let Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeDamage(q))) =
                &mut reply.request.query
            else {
                unreachable!()
            };
            match attack {
                1 => q.witness.working_npc[0] ^= 1,
                2 => q.creature_pose.position = f::point(100.0),
                3 => q.player_health += 1,
                _ => q.phase = CombatPhase::MainhandFront,
            }
        }
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone())
                .unwrap();
        assert!(replay.replay(&records).is_err(), "attack {attack}");
        // Replay stages the complete record batch; a later forged observation
        // cannot commit even the valid first tick.
        assert_eq!(replay.checkpoint().unwrap(), initial);
    }
}

#[test]
fn overlapping_source_faction_and_static_flags_must_match_ai_combat_and_motion() {
    let baseline = fixture::profile();
    for mismatch in 0..10 {
        let mut p = baseline.clone();
        match mismatch {
            0 => p.npcs[0].ai.neutral_to_all = true,
            1 => p.npcs[0].combat.as_mut().unwrap().no_crushing_blows = true,
            2 => {
                p.source_motion
                    .as_mut()
                    .unwrap()
                    .npc_policy
                    .settings
                    .no_wounded_slowdown = true
            }
            3 => {
                p.source_motion
                    .as_mut()
                    .unwrap()
                    .npc_policy
                    .settings
                    .backpedal_enabled = false
            }
            7 => p.npcs[0].ai.reaction = ai::Reaction::Defensive,
            8 => p.source_motion.as_mut().unwrap().npc_can_enter_combat = false,
            9 => p.source_motion.as_mut().unwrap().player_can_enter_combat = false,
            _ => {}
        }
        if (4..7).contains(&mismatch) {
            let mut raw = NpcCombatAuthoritySource {
                entry: p.npcs[0].entry,
                extra_flags: 0,
                static_flags1: 0,
                static_flags2: 0,
                static_flags3: 0,
                static_flags4: 0,
            };
            match mismatch {
                4 => raw.static_flags1 = 0x100000,
                5 => raw.static_flags3 = 0x2000000,
                _ => raw.static_flags4 = 0x10000,
            }
            assert!(p.initialize_combat_authority(vec![raw]).is_err());
        } else {
            assert!(p.validate().is_err(), "mismatch {mismatch}");
            let saved = world(baseline.clone(), 7).checkpoint().unwrap();
            assert!(NativeWorld::restore_synthetic_component_test(p, saved).is_err());
        }
    }
    for bits in [0x10, 0x40] {
        let mut p = baseline.clone();
        assert!(p
            .initialize_combat_authority(vec![NpcCombatAuthoritySource {
                entry: p.npcs[0].entry,
                extra_flags: 0,
                static_flags1: 0,
                static_flags2: bits,
                static_flags3: 0,
                static_flags4: 0,
            }])
            .is_err());
    }
    let mut p = baseline.clone();
    let mut rows = fixture::source_rows(&p);
    rows.factions[1].enemy_group_mask = 0;
    rows.factions[1].friend_group_mask = 0;
    assert!(p.initialize_interactions(rows).is_err());
    let mut p = baseline.clone();
    let mut rows = fixture::source_rows(&p);
    rows.factions[1].faction = 0;
    rows.factions[1].reputation_list_id = None;
    assert!(p.initialize_interactions(rows.clone()).is_err());
    assert_eq!(p.digest(), baseline.digest());
    p.npcs[0].ai.neutral_to_all = true;
    p.initialize_interactions(rows).unwrap();
    let mut w = world(p, 7);
    let mut a = fixture::SpatialOnly::default();
    for _ in 0..3 {
        step(&mut w, &mut a).unwrap();
    }
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Idle);
    let mut p = baseline;
    p.npcs[0].combat.as_mut().unwrap().no_crushing_blows = true;
    p.source_motion
        .as_mut()
        .unwrap()
        .npc_policy
        .settings
        .no_wounded_slowdown = true;
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: p.npcs[0].entry,
        extra_flags: 0,
        static_flags1: 0,
        static_flags2: 0x50,
        static_flags3: 0,
        static_flags4: 0,
    }])
    .unwrap();
    let w = world(p, 7);
    assert!(NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        w.checkpoint().unwrap()
    )
    .is_ok());
}

#[test]
fn native_ten_rejects_published_nine_checkpoint_and_records_without_migration() {
    let mut w = world(fixture::profile(), 7);
    let initial = w.checkpoint().unwrap();
    let mut old = serde_json::to_value(&initial).unwrap();
    old["version"] = 9.into();
    old["state"]["version"] = 9.into();
    old["state"]["policy"] = "benilla-native-source-unit-state-v9".into();
    let state: persistence::StateCheckpoint = serde_json::from_value(old["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v9\0", &state);
    old["digest"] = serde_json::to_value(digest).unwrap();
    let tip: crate::Digest = serde_json::from_value(old["history_tip"].clone()).unwrap();
    old["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v9\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    let old: Checkpoint = serde_json::from_value(old).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), old).is_err());
    step(&mut w, &mut fixture::SpatialOnly::default()).unwrap();
    let mut records = w.records().to_vec();
    records[0].version = 9;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone()).unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), initial);
    assert_eq!(NATIVE_FORMAT_VERSION, 22);
    assert_eq!(crate::FORMAT_VERSION, 2);
}
