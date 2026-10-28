// SPDX-License-Identifier: GPL-3.0-or-later
use super::{synthetic_fixture as f, synthetic_spell_fixture as fixture, *};
use ai::movement::Check;
fn world(p: Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), 7).unwrap()
}
fn step(w: &mut NativeWorld, a: &mut impl Authority) {
    w.step(w.state.tick + 1, None, a).unwrap();
}
fn cast(w: &mut NativeWorld, a: &mut impl Authority) {
    let input = f::cast(w, 133);
    w.step(w.state.tick + 1, Some(input), a).unwrap();
}
fn semantics(record: &TickRecord) -> Vec<(SourceSpellPhase, SpellFacts)> {
    record
        .replies
        .iter()
        .filter_map(|r| match (&r.request.query, &r.facts) {
            (
                Query::SourceSpell(SourceSpellQuery::Semantics(q)),
                Facts::SourceSpell(SourceSpellFacts::Semantics(f)),
            ) => Some((q.phase, f.clone())),
            _ => None,
        })
        .collect()
}
#[test]
fn source_spells_recompute_phases_keep_draw_order_and_replay_scheduler_history() {
    let mut w = world(fixture::profile());
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::spatial_only();
    cast(&mut w, &mut a);
    assert_eq!(
        semantics(w.last_tick().unwrap())[0].0,
        SourceSpellPhase::Prepare
    );
    for _ in 0..40 {
        step(&mut w, &mut a);
    }
    let records = w.records();
    let phases: Vec<_> = records.iter().flat_map(semantics).map(|(p, _)| p).collect();
    assert!(phases.contains(&SourceSpellPhase::ReleaseCheck));
    assert!(phases.contains(&SourceSpellPhase::ReleaseHit));
    assert!(phases.contains(&SourceSpellPhase::Impact));
    let launch = records
        .iter()
        .find(|r| {
            semantics(r)
                .iter()
                .any(|(p, _)| *p == SourceSpellPhase::ReleaseHit)
        })
        .unwrap();
    let hit = semantics(launch)
        .into_iter()
        .find(|(p, _)| *p == SourceSpellPhase::ReleaseHit)
        .unwrap()
        .1;
    assert_eq!(hit.resist_chance, 400);
    assert!(hit.critical_chance > 0);
    assert_eq!(launch.draws[0].purpose, DrawPurpose::Hit);
    assert!(!a.queries.iter().any(|r| matches!(
        r.query,
        Query::Spell { .. } | Query::SourceSpell(SourceSpellQuery::Semantics(_))
    )));
    for record in records {
        for (ordinal, reply) in record.replies.iter().enumerate() {
            let q = match &reply.request.query {
                Query::SourceSpell(
                    SourceSpellQuery::Semantics(q)
                    | SourceSpellQuery::LineOfSight(q)
                    | SourceSpellQuery::Facing(q)
                    | SourceSpellQuery::ImpactVisibility(q),
                ) => q,
                Query::SourceSpell(SourceSpellQuery::Range { context, .. }) => context,
                _ => continue,
            };
            assert_eq!(
                q.witness.moment,
                InteractionMoment::Frame {
                    action_ordinal: ordinal as u32
                }
            );
            assert_ne!(q.history, [0; 32]);
            if matches!(
                q.phase,
                SourceSpellPhase::ReleaseCheck
                    | SourceSpellPhase::ReleaseHit
                    | SourceSpellPhase::Impact
            ) {
                let scheduler = q.scheduler.as_ref().unwrap();
                assert_eq!(
                    scheduler.queue.clock_ms,
                    record.tick * u64::from(w.profile.tick_ms)
                );
                assert_eq!(scheduler.due.now_ms, scheduler.queue.clock_ms);
                assert_eq!(
                    scheduler.due.kind,
                    benilla_sim::event_processor::EventKind::SpellUpdate { cast_id: q.cast_id }
                );
                assert!(scheduler.callback_ordinal > 0);
                assert!(scheduler
                    .queue
                    .detached
                    .iter()
                    .any(|e| e.event_id == scheduler.due.event_id));
            } else {
                assert!(q.scheduler.is_none());
            }
        }
    }
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), w.checkpoint().unwrap());
    let mut reopened = NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap();
    step(&mut w, &mut a);
    step(&mut reopened, &mut fixture::spatial_only());
    assert_eq!(w.state_digest(), reopened.state_digest());
}
#[test]
fn cant_crit_raw_flag_is_not_overwritten_by_current_combat_stat() {
    let (mut p, mut initial, mut source) = fixture::inputs();
    source.spells[0].words[8] = 0x2000_0000;
    initial.spells[0].words[8] = 0x2000_0000;
    p.source_motion
        .as_mut()
        .unwrap()
        .spell_hits
        .get_mut(&133)
        .unwrap()
        .attributes[2] = 0x2000_0000;
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
    let mut w = world(p);
    let mut a = fixture::spatial_only();
    assert!(w.state.combat.as_ref().unwrap().stats.crit_percent.bits() > 0);
    cast(&mut w, &mut a);
    for _ in 0..3 {
        step(&mut w, &mut a);
    }
    assert!(w
        .records()
        .iter()
        .flat_map(semantics)
        .any(|(p, f)| p == SourceSpellPhase::ReleaseHit && f.critical_chance == 0));
    assert!(!w
        .records()
        .iter()
        .flat_map(|r| &r.draws)
        .any(|d| matches!(d.purpose, DrawPurpose::Critical)));
}
#[test]
fn source_spell_spatial_failures_preserve_error_precedence_and_short_circuit() {
    for (los, range, front, expected, kinds) in [
        (
            Check::Failed,
            SpellRangeResult::TooFar,
            Check::Failed,
            0x2a,
            1,
        ),
        (
            Check::Passed,
            SpellRangeResult::TooFar,
            Check::Failed,
            0x59,
            2,
        ),
        (
            Check::Passed,
            SpellRangeResult::TooClose,
            Check::Failed,
            0x76,
            2,
        ),
        (
            Check::Passed,
            SpellRangeResult::Within,
            Check::Failed,
            0x7c,
            3,
        ),
    ] {
        let (mut p, initial, mut source) = fixture::inputs();
        p.npcs[0].ai.spawn = f::point(16.0);
        match range {
            SpellRangeResult::TooFar => source.spells[0].range.maximum = f::float(1.0),
            SpellRangeResult::TooClose => source.spells[0].range.minimum = f::float(20.0),
            SpellRangeResult::Within => {}
            SpellRangeResult::Unknown => unreachable!(),
        }
        p.initialize_spell_authority(&initial, source).unwrap();
        let mut w = world(p);
        let mut a = fixture::spatial_only();
        a.line_of_sight = Some(los);
        a.facing = Some(front);
        cast(&mut w, &mut a);
        assert_eq!(
            semantics(w.last_tick().unwrap())[0].1.other_failure,
            Some(expected)
        );
        assert!(w.last_tick().unwrap().draws.is_empty());
        assert_eq!(
            w.last_tick()
                .unwrap()
                .replies
                .iter()
                .filter(|r| matches!(r.request.query, Query::SourceSpell(ref q)
                    if !matches!(q, SourceSpellQuery::Semantics(_))))
                .count(),
            kinds
        );
        assert!(w.state.casts.is_empty());
    }
}
#[test]
fn late_missing_spatial_or_receipt_rolls_back_cast_rng_scheduler_sequence_and_record() {
    for kind in 0..6 {
        let mut w = world(fixture::profile());
        let mut a = fixture::spatial_only();
        cast(&mut w, &mut a);
        step(&mut w, &mut a);
        match kind {
            0 => a.line_of_sight = Some(Check::Unknown),
            1 => a.fail_range_phase = Some(SourceSpellPhase::ReleaseCheck),
            2 => a.facing = Some(Check::Unknown),
            3 => a.stale = true,
            4 => a.wrong_kind = true,
            _ => a.fail_phase = Some(SourceSpellPhase::ReleaseHit),
        };
        let before = w.checkpoint().unwrap();
        let count = w.records().len();
        assert!(
            w.step(w.state.tick + 1, None, &mut a).is_err(),
            "kind {kind}"
        );
        assert_eq!(w.checkpoint().unwrap(), before);
        assert_eq!(w.records().len(), count);
        step(&mut w, &mut fixture::spatial_only());
    }
}
#[test]
fn unknown_modified_immune_high_level_or_wrong_damage_class_sources_never_select() {
    for kind in 0..17 {
        let (mut p, initial, mut source) = fixture::inputs();
        let before = p.digest();
        match kind {
            0 => source.npcs[0].resistances[1] = 1,
            1 => source.npcs[0].school_immune_mask = 4,
            2 => source.npcs[0].mechanic_immune_mask = 1,
            3 => source.npcs[0].addon_auras = SpellSourceRows::Unknown,
            4 => source.npcs[0].addon_auras = SpellSourceRows::Complete(vec![9876]),
            5 => source.npcs[0].rank = 3,
            6 => source.npcs[0].subtype = 2,
            7 => source.spells[0].words[164] = 2,
            8 => source.spells[0].words[9] = 0x0004_0000,
            9 => source.spells[0].hooks.spell_proc_event = SpellSourceRows::Unknown,
            10 => source.spells[0].hooks.spell_affect = SpellSourceRows::Complete(vec![133]),
            11 => source.modifier_hooks.spell_bonus_data = SpellSourceRows::Unknown,
            12 => source.spells[0].words[61] = 77,
            13 => source.spells[0].words[79] = 1,
            14 => source.spells[0].server_attributes = 1,
            15 => source.spells[0].range.flags = 1,
            _ => source.npcs[0].event_scripts = SpellSourceRows::Complete(vec![1]),
        }
        assert!(
            p.initialize_spell_authority(&initial, source).is_err(),
            "kind {kind}"
        );
        assert_eq!(p.digest(), before);
    }
    let (mut p, initial, source) = fixture::inputs();
    p.npcs[0].ai.level = 21;
    assert!(p.initialize_spell_authority(&initial, source).is_err());
}

fn progression_profile() -> Profile {
    progression_profile_levels(3)
}
fn progression_profile_levels(levels: u32) -> Profile {
    use benilla_sim::quest_lifecycle as q;
    let (mut p, mut initial, mut data) =
        synthetic_quest_fixture::setup_with_levels(levels, 60, true, 0x3f800000);
    fixture::rekey(&mut p, &mut initial);
    p.npcs.truncate(1);
    p.npcs[0].max_health = 3000;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.npcs[0].ai.neutral_to_all = true;
    p.npcs[0].faction_template = 2;
    data.quests[0].special_flags = 0;
    data.quests[0].required_creature = [0; 4];
    data.quests[0].required_creature_count = [0; 4];
    let relation = q::CreatureQuestRelation {
        creature: 60001,
        quest: 70001,
    };
    data.relations = q::QuestRelations {
        starters: vec![relation.clone()],
        finishers: vec![relation],
    };
    data.npc_rows = vec![QuestNpcRow {
        entry: 60001,
        npc_flags: 2,
        unit_flags: 0,
    }];
    data.kill.creatures.retain(|r| r.entry == 60001);
    let source = fixture::source(&p);
    for s in &source.spells {
        initial
            .spells
            .iter_mut()
            .find(|r| r.id == s.words[0])
            .unwrap()
            .words = s.words.clone().try_into().unwrap();
    }
    data.abilities = p
        .abilities
        .iter()
        .cloned()
        .map(|ability| AbilityRecipe {
            ability,
            cast_time: Some(CastTimeRow {
                id: 901,
                base_ms: 200,
                per_level_ms: 0,
                minimum_ms: 0,
            }),
        })
        .collect();
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    p.initialize_quest_progression(&initial, data).unwrap();
    p.source_motion = Some(synthetic_motion_fixture::genesis(&p, 2));
    let mut relations = synthetic_combat_fixture::source_rows(&p);
    relations.npcs[0].npc_flags = 2;
    for (i, row) in relations.factions.iter_mut().enumerate() {
        row.faction = 96001 + i as u32;
        row.reputation_list_id = Some(-1);
        row.group_mask = 0;
        row.friend_group_mask = 0;
        row.enemy_group_mask = 0;
    }
    p.initialize_interactions(relations).unwrap();
    p.initialize_combat_authority(vec![NpcCombatAuthoritySource {
        entry: 60001,
        extra_flags: 0,
        static_flags1: 0,
        static_flags2: 0,
        static_flags3: 0,
        static_flags4: 0,
    }])
    .unwrap();
    p.initialize_spell_authority(&initial, source).unwrap();
    p
}
#[test]
fn actual_reward_changes_current_level_and_crit_before_release_not_prepared_history() {
    let mut w = world(progression_profile());
    let genesis = w.checkpoint().unwrap();
    let mut a = fixture::spatial_only();
    let giver = w.state.npcs[&2].id;
    let accept = synthetic_quest_fixture::command(
        &w,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    w.step(1, Some(accept), &mut a).unwrap();
    let initial_crit = w.state.combat.as_ref().unwrap().stats.crit_percent;
    cast(&mut w, &mut a);
    let prepared = w.state.casts[&1].binding.clone();
    assert_eq!(prepared.level, 1);
    let reward = synthetic_quest_fixture::command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver,
        },
    );
    w.step(w.state.tick + 1, Some(reward), &mut a).unwrap();
    assert_eq!(w.player_level(), 2);
    assert_eq!(w.state.casts[&1].binding, prepared);
    let current_crit = w.state.combat.as_ref().unwrap().stats.crit_percent;
    assert_ne!(initial_crit, current_crit);
    let checkpoint = w.checkpoint().unwrap();
    let mut resumed =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint).unwrap();
    step(&mut w, &mut a);
    step(&mut resumed, &mut fixture::spatial_only());
    assert_eq!(w.state_digest(), resumed.state_digest());
    let release = semantics(w.last_tick().unwrap())
        .into_iter()
        .find(|(phase, _)| *phase == SourceSpellPhase::ReleaseHit)
        .unwrap()
        .1;
    assert_eq!(release.resist_chance, 300);
    assert_eq!(
        release.critical_chance,
        spell::combat_chance(current_crit).unwrap()
    );
    assert_eq!(w.state.casts[&1].binding, prepared);
    assert_eq!(w.state.casts[&1].release_binding.as_ref().unwrap().level, 2);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}
#[test]
fn impact_visibility_is_rechecked_after_release_and_missing_evidence_rolls_back() {
    let mut w = world(fixture::profile());
    let mut a = fixture::spatial_only();
    cast(&mut w, &mut a);
    while matches!(
        w.state.casts[&1].execution.state(),
        spell::CastState::Casting { .. }
    ) {
        step(&mut w, &mut a);
    }
    let launch = w.state.casts[&1].execution.launch_context().unwrap();
    assert!(launch.rolls.hit > launch.resist_chance);
    let spell::CastState::Traveling { impact_at_ms } = w.state.casts[&1].execution.state() else {
        panic!()
    };
    while w.state.now_ms + u64::from(w.profile.tick_ms) < impact_at_ms {
        step(&mut w, &mut a);
    }
    let before = w.checkpoint().unwrap();
    a.impact_visibility = Some(Check::Unknown);
    assert!(w.step(w.state.tick + 1, None, &mut a).is_err());
    assert_eq!(w.checkpoint().unwrap(), before);
    a.impact_visibility = Some(Check::Failed);
    step(&mut w, &mut a);
    assert!(w.state.casts.is_empty());
    assert!(w.state.npcs[&2].auras.is_empty());
    assert!(semantics(w.last_tick().unwrap())
        .iter()
        .any(|(p, f)| *p == SourceSpellPhase::Impact && !f.allowed && !f.immune));
    assert!(!w.last_tick().unwrap().draws.iter().any(|d| matches!(
        d.purpose,
        DrawPurpose::Periodic { .. } | DrawPurpose::Hit | DrawPurpose::Critical
    )));
}
#[test]
fn stale_or_forged_derived_source_facts_and_phase_history_fail_replay() {
    let mut w = world(fixture::profile());
    let genesis = w.checkpoint().unwrap();
    let mut a = fixture::spatial_only();
    cast(&mut w, &mut a);
    step(&mut w, &mut a);
    step(&mut w, &mut a);
    for kind in 0..5 {
        let mut records = w.records().to_vec();
        let reply=records[2].replies.iter_mut().find(|r|matches!(&r.request.query,Query::SourceSpell(SourceSpellQuery::Semantics(q)) if q.phase==SourceSpellPhase::ReleaseHit)).unwrap();
        match kind {
            0 => {
                if let Facts::SourceSpell(SourceSpellFacts::Semantics(f)) = &mut reply.facts {
                    f.resist_chance = 0
                }
            }
            1 => {
                if let Query::SourceSpell(SourceSpellQuery::Semantics(q)) = &mut reply.request.query
                {
                    q.history[0] ^= 1
                }
            }
            2 => {
                if let Query::SourceSpell(SourceSpellQuery::Semantics(q)) = &mut reply.request.query
                {
                    q.witness.draft[0] ^= 1
                }
            }
            3 => {
                if let Query::SourceSpell(SourceSpellQuery::Semantics(q)) = &mut reply.request.query
                {
                    q.scheduler.as_mut().unwrap().queue.clock_ms += 1;
                }
            }
            _ => {
                if let Query::SourceSpell(SourceSpellQuery::Semantics(q)) = &mut reply.request.query
                {
                    q.scheduler.as_mut().unwrap().callback_ordinal += 1;
                }
            }
        };
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), genesis.clone())
                .unwrap();
        assert!(replay.replay(&records).is_err());
        assert_eq!(replay.state.tick, 0);
    }
}

#[test]
fn live_view_hit_and_crit_reducer_matches_350_hash_pinned_cpp_probes() {
    // White-box numeric view probes cover the bounded matrix independently of
    // constructor/training; the reward test above exercises a real level change.
    let mut worlds = Vec::new();
    for disabled in [false, true] {
        let (mut p, mut initial, mut source) = fixture::inputs();
        if disabled {
            source.spells[0].words[8] = 0x2000_0000;
            initial.spells[0].words[8] = 0x2000_0000;
            p.source_motion
                .as_mut()
                .unwrap()
                .spell_hits
                .get_mut(&133)
                .unwrap()
                .attributes[2] = 0x2000_0000;
        }
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        p.initialize_spell_authority(&initial, source).unwrap();
        worlds.push(world(p));
    }
    use sha2::Digest as _;
    let probes = include_str!("fixtures/spell_authority_cmangos.tsv").replace("\r\n", "\n");
    assert_eq!(
        format!("{:x}", sha2::Sha256::digest(probes.as_bytes())),
        "bf857ab3c925117d5fb55fe32a161b4b873ded7deccf981eab3f0d822edc93d5"
    );
    let mut count = 0;
    for line in probes.lines() {
        let v: Vec<u32> = line
            .split_whitespace()
            .map(|w| w.parse().unwrap())
            .collect();
        let w = &mut worlds[v[3] as usize];
        w.profile.player.level = v[0] as u8;
        w.profile.npcs[0].ai.level = v[1] as u8;
        w.state.combat.as_mut().unwrap().stats.crit_percent =
            spell::SpellFloat::from_bits(v[2]).unwrap();
        let view = interactions::spell_authority::CurrentSpell::new(
            &w.profile,
            &w.state,
            w.state.npcs[&2].id,
            133,
        )
        .unwrap();
        assert_eq!(
            view.launch_chances().unwrap(),
            (v[4] as u16, v[5] as u16),
            "{line}"
        );
        count += 1;
    }
    assert_eq!(count, 350);
}
#[test]
fn resisted_release_consumes_only_hit_die_and_skips_delayed_visibility() {
    let p = fixture::profile();
    let mut found = false;
    for seed in 0..200 {
        let mut w =
            NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), seed).unwrap();
        let mut a = fixture::spatial_only();
        cast(&mut w, &mut a);
        step(&mut w, &mut a);
        step(&mut w, &mut a);
        let launch = w.state.casts[&1].execution.launch_context().unwrap();
        if launch.rolls.hit > launch.resist_chance {
            continue;
        }
        let draws: Vec<_> = w
            .last_tick()
            .unwrap()
            .draws
            .iter()
            .filter(|d| d.cast_id == 1)
            .map(|d| d.purpose)
            .collect();
        assert_eq!(draws, vec![DrawPurpose::Hit]);
        a.impact_visibility = Some(Check::Unknown);
        for _ in 0..20 {
            step(&mut w, &mut a);
            if w.state.casts.is_empty() {
                break;
            }
        }
        assert!(w.state.casts.is_empty());
        assert!(w.state.npcs[&2].auras.is_empty());
        assert!(!a.queries.iter().any(|r| matches!(
            r.query,
            Query::SourceSpell(SourceSpellQuery::ImpactVisibility(_))
        )));
        found = true;
        break;
    }
    assert!(found);
}
#[test]
fn source_contract_rejects_published_format_ten_and_source_profile_substitution() {
    let mut w = world(fixture::profile());
    let initial = w.checkpoint().unwrap();
    let mut a = fixture::spatial_only();
    cast(&mut w, &mut a);
    let mut old = serde_json::to_value(&initial).unwrap();
    old["version"] = 10.into();
    old["state"]["version"] = 10.into();
    let old: Checkpoint = serde_json::from_value(old).unwrap();
    assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
    assert!(NativeWorld::restore_synthetic_component_test(w.profile.clone(), old).is_err());
    let mut records = w.records().to_vec();
    records[0].version = 10;
    assert!(decode_records(&encode_records(&records).unwrap()).is_err());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial.clone()).unwrap();
    assert!(replay.replay(&records).is_err());
    assert_eq!(replay.checkpoint().unwrap(), initial);
    let (mut p, initial_input, mut source) = fixture::inputs();
    source.spells[0].facing_flags = 0;
    p.initialize_spell_authority(&initial_input, source)
        .unwrap();
    assert!(NativeWorld::restore_synthetic_component_test(p, initial).is_err());
    assert_eq!(NATIVE_FORMAT_VERSION, 64);
}

fn rehash(mut value: serde_json::Value) -> Checkpoint {
    let state: persistence::StateCheckpoint =
        serde_json::from_value(value["state"].clone()).unwrap();
    let digest = crate::digest(b"benilla-native-state-v64\0", &state);
    value["digest"] = serde_json::to_value(digest).unwrap();
    let tip: crate::Digest = serde_json::from_value(value["history_tip"].clone()).unwrap();
    value["checksum"] = serde_json::to_value(crate::digest(
        b"benilla-native-checkpoint-v64\0",
        &(&state, digest, tip),
    ))
    .unwrap();
    serde_json::from_value(value).unwrap()
}
#[test]
fn historical_launch_chances_survive_later_level_change_and_refuse_rehashed_forgery() {
    let mut w = world(progression_profile());
    let mut a = fixture::spatial_only();
    let giver = w.state.npcs[&2].id;
    let accept = synthetic_quest_fixture::command(
        &w,
        Command::AcceptQuest {
            quest: 70001,
            giver,
        },
    );
    w.step(1, Some(accept), &mut a).unwrap();
    cast(&mut w, &mut a);
    while matches!(
        w.state.casts[&1].execution.state(),
        spell::CastState::Casting { .. }
    ) {
        step(&mut w, &mut a);
    }
    let launch = w.state.casts[&1].execution.launch_context().unwrap();
    assert_eq!(w.state.casts[&1].release_binding.as_ref().unwrap().level, 1);
    let old_crit = w.state.combat.as_ref().unwrap().stats.crit_percent;
    let reward = synthetic_quest_fixture::command(
        &w,
        Command::TurnInQuest {
            quest: 70001,
            giver,
        },
    );
    w.step(w.state.tick + 1, Some(reward), &mut a).unwrap();
    assert_eq!(w.player_level(), 2);
    assert_ne!(
        old_crit,
        w.state.combat.as_ref().unwrap().stats.crit_percent
    );
    let checkpoint = w.checkpoint().unwrap();
    NativeWorld::restore_synthetic_component_test(w.profile.clone(), checkpoint.clone()).unwrap();
    for (field, value) in [("resist_chance", 0), ("critical_chance", 10000)] {
        let mut altered = serde_json::to_value(&checkpoint).unwrap();
        altered["state"]["casts"]["1"]["execution"]["launch"][field] = value.into();
        assert!(
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), rehash(altered))
                .is_err(),
            "{field}"
        );
    }
    assert_eq!(
        w.state.casts[&1].execution.launch_context().unwrap(),
        launch
    );
}
#[test]
fn singleton_domain_retains_nonempty_rank_group_rows_but_refuses_unknown_and_second_id() {
    let (mut p, initial, mut source) = fixture::inputs();
    source.spells[0].hooks.spell_group_membership =
        SpellSourceRows::Complete(vec![SpellGroupSource {
            id: 1,
            rule: 1,
            name: "authored group".into(),
            spell_ids: vec![133, 143],
        }]);
    source.spells[0].hooks.spell_rank_chain = SpellSourceRows::Complete(vec![
        SpellRankSource {
            spell_id: 133,
            previous_spell: 0,
            first_spell: 133,
            rank: 1,
            required_spell: 0,
        },
        SpellRankSource {
            spell_id: 143,
            previous_spell: 133,
            first_spell: 133,
            rank: 2,
            required_spell: 0,
        },
    ]);
    assert!(p
        .initialize_spell_authority(&initial, source.clone())
        .is_err());
    source.modifier_spell_ids = SpellSourceRows::Complete(vec![133, 143]);
    p.initialize_spell_authority(&initial, source.clone())
        .unwrap();
    let mut w = world(p.clone());
    cast(&mut w, &mut fixture::spatial_only());
    let mut other = w.state.casts[&1].binding.clone();
    other.ability.id = 143;
    if let AbilityAdmission::Available(spec) = &mut other.ability.admission {
        spec.spell_id = 143;
    }
    assert!(interactions::spell_authority::validate_historical_binding(&p, &other).is_err());
    source.spells[0].hooks.spell_rank_chain = SpellSourceRows::Unknown;
    assert!(p.initialize_spell_authority(&initial, source).is_err());
    let (mut p, initial, mut source) = fixture::inputs();
    let mut other = source.spells[0].clone();
    other.words[0] = 143;
    source.spells.push(other);
    assert!(p.initialize_spell_authority(&initial, source).is_err());
}
#[test]
fn instant_and_unhistoried_mutable_stats_are_explicit_admission_failures() {
    let (mut p, initial, mut source) = fixture::inputs();
    if let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission {
        spec.cast_ms = 0;
    }
    source.spells[0].cast_time.base_ms = 0;
    assert!(matches!(
        p.initialize_spell_authority(&initial, source),
        Err(NativeError::UnsupportedMechanic(
            "instant spell entry lifecycle"
        ))
    ));
    let (mut p, mut initial, source) = fixture::inputs();
    initial.items[0].max_durability = 1;
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    assert!(matches!(
        p.initialize_spell_authority(&initial, source),
        Err(NativeError::UnsupportedMechanic(
            "mutable spell stats require progression history"
        ))
    ));
}

#[test]
fn initial_cast_interrupt_aura_flags_and_raw_cost_mismatch_never_select() {
    for flags in [4, 0x10000] {
        let (mut p, mut initial, source) = fixture::inputs();
        let mut passive = initial.spells[0].clone();
        passive.id = 60100;
        passive.words = [0; 173];
        passive.words[0] = passive.id;
        passive.words[6] = 0x40;
        passive.words[22] = flags;
        passive.words[58] = u32::MAX;
        passive.words[61] = 6;
        passive.words[76] = 4;
        passive.words[82] = 1;
        passive.words[91] = 29;
        passive.words[106] = 3;
        initial.spells.push(passive);
        if let benilla_sim::starter_init::Rows::Complete(rows) = &mut initial.default_spells {
            rows.push(60100);
        }
        // The raw initializer accepts this stat passive. Bind the changed
        // source first, so the selected host reaches its aura-removal gate.
        benilla_sim::starter_init::initialize_mage(&initial).unwrap();
        assert!(matches!(
            p.initialize_spell_authority(&initial, source.clone()),
            Err(NativeError::InvalidProfile)
        ));
        let original = p.clone();
        assert_eq!(
            p.initialize_player(&initial, f::float(100.0), 1),
            Err(NativeError::UnsupportedMechanic(
                "source motion interruptible initial player auras"
            ))
        );
        assert_eq!(p, original);
        // A separately authored player-only fixture isolates the spell-selector
        // guard from the source-motion/combat hosts' earlier rejection.
        p = f::initialized_profile();
        fixture::rekey(&mut p, &mut initial);
        p.initialize_player(&initial, f::float(100.0), 1).unwrap();
        assert_eq!(
            p.initialize_spell_authority(&initial, source),
            Err(NativeError::UnsupportedMechanic(
                "initial spell aura-interrupt lifecycle"
            ))
        );
    }
    let (mut p, mut initial, mut source) = fixture::inputs();
    source.spells[0].words[32] += 1;
    initial.spells[0].words[32] += 1;
    p.initialize_player(&initial, f::float(100.0), 1).unwrap();
    assert!(p.initialize_spell_authority(&initial, source).is_err());
}

#[test]
fn source_authority_level_sixty_cast_release_and_aura_reload() {
    let mut p = progression_profile_levels(60);
    p.admin = Some(AdminPolicy {
        world: [8; 16],
        world_owner: f::player().owner,
        allowlist: Default::default(),
        revision: 7,
    });
    let mut w = world(p);
    let mut a = fixture::spatial_only();
    while w.player_level() < 60 {
        let mut key = [0; 16];
        key[0] = w.player_level();
        let request = AdminRequest {
            key,
            world: w.state.world,
            player: w.state.player,
            policy_revision: 7,
            operation: AdminOperation::GainOneLevel,
        };
        let input = synthetic_quest_fixture::command(&w, Command::Admin(request));
        w.step(w.state.tick + 1, Some(input), &mut a).unwrap();
        let checkpoint = w.checkpoint().unwrap();
        w.checkpoint_committed(&checkpoint).unwrap();
    }
    cast(&mut w, &mut a);
    assert_eq!(w.state.casts[&1].binding.level, 60);
    let reopen = |w: &NativeWorld| {
        NativeWorld::restore_synthetic_component_test(
            w.profile.clone(),
            Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
        )
        .unwrap()
    };
    w = reopen(&w);
    let mut saw_release = false;
    let mut saw_aura = false;
    for _ in 0..40 {
        step(&mut w, &mut a);
        if w.state.casts.values().any(|c| c.release_binding.is_some()) {
            saw_release = true;
            w = reopen(&w);
        }
        if !w.state.npcs[&2].aura_bindings.is_empty() {
            assert!(w.state.npcs[&2]
                .aura_bindings
                .values()
                .all(|b| b.level == 60));
            saw_aura = true;
            w = reopen(&w);
            break;
        }
    }
    assert!(saw_release && saw_aura);
    assert_eq!(reopen(&w).state_digest(), w.state_digest());
}
