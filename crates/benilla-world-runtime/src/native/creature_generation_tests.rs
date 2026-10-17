// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored raw rows and real synthetic-host lifecycle transitions. These tests
//! establish neither database capture completeness nor native-content admission.
use super::{synthetic_creature_model_fixture as models, synthetic_fixture as f, *};
use benilla_sim::creature_init as init;
use std::collections::BTreeSet;

mod equipment;

fn source(spawn: u64, minimum: u8, maximum: u8) -> CreatureGenerationSource {
    let mut input = models::input();
    input.template.extra_flags = 2;
    input.template.minimum_level = minimum;
    input.template.maximum_level = maximum;
    input.template.corpse_delay_seconds = 5;
    input.template.regenerate_stats = 2;
    input.template.displays[0].probability = 1;
    input.class_levels = init::Rows::Complete(
        (minimum..=maximum)
            .map(|level| init::ClassLevelRow {
                class: 1,
                level,
                base_health: 60 + u32::from(level) * 17,
                base_mana: 0,
                base_armor: 70 + u32::from(level) * 11,
                base_melee_attack_power: f::float(50.0 + f32::from(level) * 10.0),
                base_ranged_attack_power: f::float(70.0),
                base_damage: f::float(3.0 + f32::from(level)),
                attributes: [19, 23, 21, 22, 17],
            })
            .collect(),
    );
    let init::Rows::Complete(rows) = &mut input.spawn_template else {
        unreachable!()
    };
    rows[0].current_health = 1;
    CreatureGenerationSource {
        spawn_id: spawn,
        spawn_entry: input.template.entry,
        requested_spawn_template_entry: 0,
        respawn_delay_min_seconds: 2,
        respawn_delay_max_seconds: 3,
        input,
    }
}

fn install(mut p: Profile, sources: Vec<CreatureGenerationSource>, order: Vec<u64>) -> Profile {
    // Static fixture metadata is deliberately compatible with the raw rows. The
    // low-level exemplar is not the world's live selection or an RNG receipt.
    for source in &sources {
        let plan = init::prepare(&source.input).unwrap();
        let model = plan.model_roll_range().map(|range| range.0);
        let gender = plan.gender_roll_range(model).unwrap().map(|range| range.1);
        let level = plan.level_roll_range().map(|range| range.0);
        let value = plan
            .resolve(init::SelectionRolls {
                model,
                gender,
                level,
            })
            .unwrap();
        let npc = p
            .npcs
            .iter_mut()
            .find(|npc| npc.ai.creature_id == source.spawn_id)
            .unwrap();
        npc.entry = value.entry;
        npc.display_id = value.model.display_id;
        npc.faction_template = value.faction;
        npc.max_health = value.stats.resources.max_health;
        npc.ai.level = value.level;
        npc.ai.run_speed = ai::Real::from_bits(value.model.run_speed.bits()).unwrap();
        npc.ai.home_speed = npc.ai.run_speed;
        npc.ai.combat_reach = ai::Real::from_bits(value.model.combat_reach.bits()).unwrap();
        npc.ai.bounding_radius = ai::Real::from_bits(value.model.bounding_radius.bits()).unwrap();
        npc.ai.mainhand_interval_ms = value.mainhand_interval_ms;
        if let Some(combat) = &mut npc.combat {
            combat.minimum_damage = value.stats.damage.minimum;
            combat.maximum_damage = value.stats.damage.maximum;
            combat.no_crushing_blows = value.no_crushing_blows;
            combat.creature_is_critter = value.creature_is_critter;
        }
    }
    p.initialize_creature_generations(init::Rows::Complete(sources), order)
        .unwrap();
    p
}
fn profile(source: CreatureGenerationSource) -> Profile {
    install(f::initialized_profile(), vec![source], vec![2])
}
fn world(p: Profile, seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(p, [8; 16], f::player(), seed).unwrap()
}
fn owner(w: &NativeWorld, spawn: u64) -> &CreatureGenerationOwner {
    w.state.npcs[&spawn].source_generation.as_ref().unwrap()
}
fn step(w: &mut NativeWorld, authority: &mut impl Authority) {
    w.step(w.state.tick + 1, None, authority).unwrap();
}
fn restore(w: &NativeWorld) -> NativeWorld {
    NativeWorld::restore_synthetic_component_test(
        w.profile.clone(),
        Checkpoint::decode(&w.checkpoint().unwrap().encode().unwrap()).unwrap(),
    )
    .unwrap()
}
fn kill(w: &mut NativeWorld, authority: &mut impl Authority) {
    let command = f::cast(w, 60001);
    w.step(w.state.tick + 1, Some(command), authority).unwrap();
    for _ in 0..100 {
        if w.state.npcs[&2].health == 0 {
            assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
            return;
        }
        step(w, authority);
    }
    panic!("authored lethal spell did not reach the target");
}
fn respawn(w: &mut NativeWorld, authority: &mut impl Authority) {
    let old = w.state.npcs[&2].id;
    for _ in 0..100 {
        step(w, authority);
        if w.state.npcs[&2].id != old {
            assert_eq!(w.state.npcs[&2].id.generation, old.generation + 1);
            return;
        }
    }
    panic!("ordinary dead update did not respawn the authored creature");
}
fn assert_selection(w: &NativeWorld, spawn: u64) {
    let n = &w.state.npcs[&spawn];
    let owned = owner(w, spawn);
    let life = owned.history().last().unwrap();
    let source = w
        .profile
        .creature_generations
        .as_ref()
        .unwrap()
        .source(spawn)
        .unwrap();
    let mut rolls = init::SelectionRolls {
        model: None,
        gender: None,
        level: None,
    };
    for draw in life.creation_draws() {
        match draw.purpose {
            DrawPurpose::CreatureModel { npc } => {
                assert_eq!(npc, n.id);
                rolls.model = Some(draw.value.try_into().unwrap());
            }
            DrawPurpose::CreatureGender { npc } => {
                assert_eq!(npc, n.id);
                rolls.gender = Some(draw.value.try_into().unwrap());
            }
            DrawPurpose::CreatureLevel { npc } => {
                assert_eq!(npc, n.id);
                rolls.level = Some(draw.value.try_into().unwrap());
            }
            DrawPurpose::CreatureInitialRespawnDelay { npc } => {
                assert_eq!(npc, n.id);
                assert_eq!(npc.generation, 0);
            }
            _ => panic!("unrelated draw in creature selection"),
        }
    }
    let (expected, model) = init::prepare(&source.input)
        .unwrap()
        .resolve_with_model_receipt(rolls)
        .unwrap();
    assert_eq!(owned.npc_id(), n.id);
    assert_eq!(owned.result(), &expected);
    assert_eq!(owned.model_selection(), &model);
    assert_eq!(n.initialized_creature(), Some(&expected));
    let p = n.effective_profile(&w.profile).unwrap();
    assert_eq!(p.ai.level, expected.level);
    assert_eq!(p.max_health, expected.stats.resources.max_health);
    assert_eq!(p.display_id, expected.model.display_id);
    assert_eq!(p.ai.run_speed.bits(), expected.model.run_speed.bits());
    assert_eq!(p.ai.combat_reach.bits(), expected.model.combat_reach.bits());
    assert_eq!(p.ai.mainhand_interval_ms, expected.mainhand_interval_ms);
    assert_eq!(
        p.combat.as_ref().unwrap().minimum_damage,
        expected.stats.damage.minimum
    );
    assert_eq!(
        p.combat.as_ref().unwrap().maximum_damage,
        expected.stats.damage.maximum
    );
    assert_eq!(p.ai.corpse_decay_ms, owned.corpse_delay_seconds() * 1000);
    assert_eq!(p.ai.respawn_delay_seconds, owned.respawn_delay_seconds());
    assert_eq!(n.ai.effective_profile(), &p.ai);
    assert_eq!(n.ai.generation(), Some(n.id.generation));
}
fn resign(value: serde_json::Value) -> Checkpoint {
    let mut cp: Checkpoint = serde_json::from_value(value).unwrap();
    cp.digest = crate::digest(b"benilla-native-state-v39\0", &cp.state);
    cp.checksum = crate::digest(
        b"benilla-native-checkpoint-v39\0",
        &(&cp.state, cp.digest, cp.history_tip),
    );
    cp
}
fn rejects(w: &NativeWorld, value: serde_json::Value) {
    assert!(
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), resign(value)).is_err()
    );
}

#[test]
fn independent_seeds_select_variable_levels_with_one_consistent_derived_projection() {
    let p = profile(source(2, 1, 3));
    let mut by_level = BTreeMap::new();
    for seed in 0..64 {
        let w = world(p.clone(), seed);
        assert_selection(&w, 2);
        assert_eq!(w.state.npcs[&2].health, 1);
        assert_eq!(restore(&w).state_digest(), w.state_digest());
        by_level.entry(owner(&w, 2).result().level).or_insert(w);
        if by_level.len() == 3 {
            break;
        }
    }
    assert_eq!(
        by_level.len(),
        3,
        "bounded authored seed search must cover all levels"
    );
    let first = owner(&by_level[&1], 2).result();
    let third = owner(&by_level[&3], 2).result();
    assert_ne!(
        first.stats.resources.max_health,
        third.stats.resources.max_health
    );
    assert_ne!(first.stats.armor, third.stats.armor);
    assert_ne!(first.stats.damage, third.stats.damage);
    assert_eq!(by_level[&1].profile_digest(), by_level[&3].profile_digest());
    assert_ne!(by_level[&1].state_digest(), by_level[&3].state_digest());
}

#[test]
fn equal_level_omits_level_but_singleton_model_and_equal_respawn_still_draw() {
    let mut source = source(2, 2, 2);
    source.respawn_delay_max_seconds = 2;
    let init::Rows::Complete(rows) = &mut source.input.models else {
        unreachable!()
    };
    rows.iter_mut().for_each(|row| row.other_gender_id = 0);
    let w = world(profile(source), 7);
    let draws = owner(&w, 2).genesis_draws();
    let id = w.state.npcs[&2].id;
    assert_eq!(draws.len(), 2);
    assert_eq!(draws[0].purpose, DrawPurpose::CreatureModel { npc: id });
    assert_eq!((draws[0].low, draws[0].high, draws[0].value), (0, 0, 0));
    assert_eq!(
        draws[1].purpose,
        DrawPurpose::CreatureInitialRespawnDelay { npc: id }
    );
    assert_eq!((draws[1].low, draws[1].high, draws[1].value), (2, 2, 2));
    assert_eq!(w.state.rng_counter, 2);
    assert_selection(&w, 2);
}

#[test]
fn gender_request_depends_on_selected_root_and_spawn_override_omits_only_model() {
    let mut source = source(2, 1, 1);
    source.input.template.displays[1] = init::DisplayChoice {
        id: 34,
        probability: 1,
    };
    let init::Rows::Complete(rows) = &mut source.input.models else {
        unreachable!()
    };
    rows.iter_mut()
        .find(|row| row.id == 34)
        .unwrap()
        .other_gender_id = 0;
    let p = profile(source.clone());
    let mut roots = BTreeSet::new();
    for seed in 0..64 {
        let w = world(p.clone(), seed);
        let owned = owner(&w, 2);
        let root = owned.model_selection().original_display_id();
        roots.insert(root);
        let genders = owned
            .genesis_draws()
            .iter()
            .filter(|d| matches!(d.purpose, DrawPurpose::CreatureGender { .. }))
            .count();
        assert_eq!(genders, usize::from(root == 31));
        assert_selection(&w, 2);
        if roots.len() == 2 {
            break;
        }
    }
    assert_eq!(roots, BTreeSet::from([31, 34]));
    let init::Rows::Complete(rows) = &mut source.input.spawn_template else {
        unreachable!()
    };
    rows[0].model_id = 31;
    let w = world(profile(source), 7);
    assert_eq!(owner(&w, 2).genesis_draws().len(), 2);
    assert!(matches!(
        owner(&w, 2).genesis_draws()[0].purpose,
        DrawPurpose::CreatureGender { .. }
    ));
    assert!(owner(&w, 2).genesis_draws().iter().all(|draw| !matches!(
        draw.purpose,
        DrawPurpose::CreatureModel { .. } | DrawPurpose::CreatureLevel { .. }
    )));
}

#[test]
fn actual_death_and_respawn_redraw_on_the_shared_stream_and_preserve_ai_clocks() {
    let mut w = world(profile(source(2, 1, 3)), 7);
    let start = w.checkpoint().unwrap();
    let original = owner(&w, 2).result().clone();
    let original_id = w.state.npcs[&2].id;
    let initial_cap = owner(&w, 2).corpse_delay_seconds();
    assert_eq!(
        initial_cap,
        (owner(&w, 2).respawn_delay_seconds() * 9 / 10).min(5)
    );
    let mut authority = f::KernelAuthority::default();
    kill(&mut w, &mut authority);
    let death_tick = w.state.tick;
    let death_ms = w.state.now_ms;
    let death_ai = serde_json::to_value(w.state.npcs[&2].ai.checkpoint()).unwrap();
    let death_owner = owner(&w, 2);
    let death_draw = death_owner.history()[0].death_draw().unwrap();
    assert_eq!(
        death_draw.purpose,
        DrawPurpose::CreatureDeathRespawnDelay { npc: original_id }
    );
    assert_eq!(death_owner.current_death_tick(), Some(death_tick));
    assert_eq!(death_owner.result(), &original);
    assert_eq!(death_owner.corpse_delay_seconds(), initial_cap);
    assert_eq!(
        death_ai["respawn_at_ms"].as_u64().unwrap(),
        (death_ms / 1000 + u64::from(death_owner.respawn_delay_seconds())) * 1000
    );
    assert_eq!(
        death_ai["corpse_at_ms"].as_u64().unwrap(),
        death_ms + u64::from(initial_cap) * 1000
    );
    assert_eq!(
        w.last_tick()
            .unwrap()
            .draws
            .iter()
            .filter(|d| matches!(d.purpose, DrawPurpose::CreatureDeathRespawnDelay { .. }))
            .count(),
        1
    );
    let regen_at_death = w.state.npcs[&2].regen_elapsed_ms;
    let mut resumed = restore(&w);
    respawn(&mut w, &mut authority);
    while resumed.state.tick < w.state.tick {
        step(&mut resumed, &mut f::KernelAuthority::default());
    }
    assert_eq!(resumed.state_digest(), w.state_digest());
    let reborn = &w.state.npcs[&2];
    assert_eq!(reborn.health, 1);
    assert_eq!(reborn.regen_elapsed_ms, regen_at_death);
    assert_eq!(
        reborn.ai.last_receipt_id(),
        death_ai["receipt_id"].as_u64().unwrap()
    );
    let ai = serde_json::to_value(reborn.ai.checkpoint()).unwrap();
    assert_eq!(ai["sequence"].as_u64().unwrap(), w.state.tick);
    assert_eq!(ai["now_ms"].as_u64().unwrap(), w.state.now_ms);
    assert!(ai["command_id"].as_u64().unwrap() >= death_ai["command_id"].as_u64().unwrap());
    let owned = owner(&w, 2);
    assert_eq!(owned.history().len(), 2);
    assert_eq!(owned.historical(original_id).unwrap().result(), &original);
    assert_eq!(owned.corpse_delay_seconds(), 5);
    assert!(owned.history()[1]
        .creation_draws()
        .iter()
        .all(|d| !matches!(
            d.purpose,
            DrawPurpose::CreatureInitialRespawnDelay { .. }
                | DrawPurpose::CreatureDeathRespawnDelay { .. }
        )));
    assert_selection(&w, 2);
    assert!(w.last_tick().unwrap().events.iter().any(
        |e| matches!(e, Event::Npc { id, effect: ai::Effect::Respawned { .. } } if *id == reborn.id)
    ));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let mut stale = f::cast(&w, 60001);
    stale.input.command = Command::Cast {
        ability: 60001,
        target: original_id,
    };
    let record = w
        .step(w.state.tick + 1, Some(stale), &mut authority)
        .unwrap();
    assert!(record
        .events
        .contains(&Event::Rejected(Rejection::UnknownTarget)));
}

#[test]
fn player_first_entry_prefix_and_authored_spawn_order_survive_replay_and_compaction() {
    let mut base = f::initialized_profile();
    let mut second = base.npcs[0].clone();
    second.ai.creature_id = 7;
    base.npcs.push(second);
    let (mut p, input, _, seal) =
        synthetic_initial_cast_fixture::prepare(base, f::initialized_starter_input());
    p.initialize_initial_cast_source(&input, seal).unwrap();
    let p = install(p, vec![source(2, 1, 3), source(7, 1, 3)], vec![7, 2]);
    let mut w = world(p, 7);
    let player = w.state.initial_cast_genesis.clone();
    assert_eq!(player.end_counter(), 1);
    assert_eq!(player.draws()[0].first_counter, 0);
    assert_eq!(player.draws()[0].value, 5);
    let mut cursor = player.end_counter();
    for spawn in [7, 2] {
        for draw in owner(&w, spawn).genesis_draws() {
            assert_eq!(draw.first_counter, cursor);
            cursor += u64::from(draw.consumed);
        }
    }
    assert_eq!(w.state.rng_counter, cursor);
    assert!(w.records().is_empty());
    let initial = w.checkpoint().unwrap();
    kill(&mut w, &mut f::KernelAuthority::default());
    assert_eq!(
        w.records()
            .iter()
            .flat_map(|r| &r.draws)
            .next()
            .unwrap()
            .first_counter,
        cursor
    );
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let before = owner(&w, 2).checkpoint();
    let cp = w.checkpoint().unwrap();
    let tip = w.journal_tip;
    w.checkpoint_committed(&cp).unwrap();
    assert!(w.records().is_empty());
    let mut resumed = restore(&w);
    assert_eq!(resumed.state.initial_cast_genesis, player);
    assert_eq!(owner(&resumed, 2).checkpoint(), before);
    step(&mut resumed, &mut f::KernelAuthority::default());
    assert_eq!(resumed.last_tick().unwrap().previous_record, tip);
}

#[test]
fn rehashed_owner_roll_source_counter_generation_and_death_mutations_reject() {
    let mut w = world(profile(source(2, 1, 3)), 7);
    kill(&mut w, &mut f::KernelAuthority::default());
    respawn(&mut w, &mut f::KernelAuthority::default());
    let base = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let mutations = [
        (
            "/state/npcs/2/source_generation/policy",
            serde_json::json!("cmangos-8ec338a1-native-creature-generation-v1"),
        ),
        (
            "/state/npcs/2/source_generation/source/0",
            serde_json::json!(255),
        ),
        (
            "/state/npcs/2/source_generation/spawn",
            serde_json::json!(3),
        ),
        (
            "/state/npcs/2/source_generation/generations/1/npc/generation",
            serde_json::json!(3),
        ),
        (
            "/state/npcs/2/source_generation/generations/1/first_counter",
            serde_json::json!(0),
        ),
        (
            "/state/npcs/2/source_generation/generations/1/end_counter",
            serde_json::json!(0),
        ),
        (
            "/state/npcs/2/source_generation/generations/1/created_tick",
            serde_json::json!(1),
        ),
        (
            "/state/npcs/2/source_generation/generations/0/draws/0/consumed",
            serde_json::json!(2),
        ),
        (
            "/state/npcs/2/source_generation/generations/0/draws/0/value",
            serde_json::json!(1),
        ),
        (
            "/state/npcs/2/source_generation/generations/0/draws/2/high",
            serde_json::json!(4),
        ),
        (
            "/state/npcs/2/source_generation/generations/0/death/tick",
            serde_json::json!(0),
        ),
        (
            "/state/npcs/2/source_generation/generations/0/death/draw/first_counter",
            serde_json::json!(0),
        ),
        ("/state/npcs/2/id/generation", serde_json::json!(0)),
        ("/state/rng_counter", serde_json::json!(0)),
    ];
    for (path, replacement) in mutations {
        let mut v = base.clone();
        let actual = v.pointer_mut(path).unwrap();
        assert_ne!(*actual, replacement, "mutation must change {path}");
        *actual = replacement;
        rejects(&w, v);
    }
    let mut missing_death = base.clone();
    missing_death["state"]["npcs"]["2"]["source_generation"]["generations"][0]["death"] =
        serde_json::Value::Null;
    rejects(&w, missing_death);
    let mut extra_result = base;
    extra_result["state"]["npcs"]["2"]["source_generation"]["generations"][1]["initialized"] =
        serde_json::to_value(owner(&w, 2).result()).unwrap();
    assert!(
        serde_json::from_value::<Checkpoint>(extra_result).is_err(),
        "caller-authored stats are never a checkpoint owner"
    );
}

#[test]
fn missing_owner_is_not_legacy_and_rebound_changed_raw_ranges_still_reject() {
    let w = world(profile(source(2, 1, 3)), 7);
    let base = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    let mut missing = base.clone();
    missing["state"]["npcs"]["2"]
        .as_object_mut()
        .unwrap()
        .remove("source_generation");
    assert!(serde_json::from_value::<Checkpoint>(missing).is_err());
    let mut absent = base.clone();
    absent["state"]["npcs"]["2"]["source_generation"] = serde_json::Value::Null;
    rejects(&w, absent);
    let mut p = w.profile.clone();
    p.creature_generations = None;
    assert!(NativeWorld::restore_synthetic_component_test(p, w.checkpoint().unwrap()).is_err());

    let changed = source(2, 1, 4);
    let source_identity = changed.identity();
    let changed_profile = profile(changed);
    let mut rebound = base;
    let digest = serde_json::to_value(changed_profile.digest()).unwrap();
    rebound["state"]["profile"] = digest.clone();
    rebound["state"]["initial_cast_genesis"]["profile"] = digest.clone();
    rebound["state"]["npcs"]["2"]["source_generation"]["profile"] = digest;
    rebound["state"]["npcs"]["2"]["source_generation"]["source"] =
        serde_json::to_value(source_identity).unwrap();
    assert!(
        NativeWorld::restore_synthetic_component_test(changed_profile, resign(rebound)).is_err()
    );
}

#[derive(Default)]
struct LateFailure {
    inner: f::KernelAuthority,
    fail_spawn: Option<u64>,
    seen: Vec<u64>,
}
impl Authority for LateFailure {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Some(id) = request.binding.npc {
            self.seen.push(id.spawn);
            if self.fail_spawn == Some(id.spawn) {
                return Err(NativeError::AuthorityUnavailable(
                    "authored late NPC failure",
                ));
            }
        }
        self.inner.resolve(request)
    }
}
#[test]
fn late_second_npc_failure_rolls_back_real_respawn_selection_history_rng_and_journal() {
    let mut p = f::initialized_profile();
    let mut second = p.npcs[0].clone();
    second.ai.creature_id = 7;
    second.ai.reaction = ai::Reaction::Defensive;
    p.npcs.push(second);
    let mut w = world(
        install(p, vec![source(2, 1, 3), source(7, 1, 3)], vec![2, 7]),
        7,
    );
    kill(&mut w, &mut LateFailure::default());
    for _ in 0..100 {
        let mut probe = w.clone();
        step(&mut probe, &mut LateFailure::default());
        if probe.state.npcs[&2].id.generation == 1 {
            let before = w.checkpoint().unwrap();
            let records = w.records().to_vec();
            let mut authority = LateFailure {
                fail_spawn: Some(7),
                ..Default::default()
            };
            assert!(matches!(
                w.step(w.state.tick + 1, None, &mut authority),
                Err(NativeError::AuthorityUnavailable(
                    "authored late NPC failure"
                ))
            ));
            assert!(authority.seen.contains(&2));
            assert!(authority.seen.contains(&7));
            assert_eq!(w.checkpoint().unwrap(), before);
            assert_eq!(w.records(), records);
            assert_eq!(owner(&w, 2).history().len(), 1);
            assert!(probe.state.rng_counter > w.state.rng_counter);
            step(&mut w, &mut LateFailure::default());
            assert_eq!(w.state_digest(), probe.state_digest());
            assert_eq!(w.records(), probe.records());
            return;
        }
        w = probe;
    }
    panic!("never reached the actual respawn transaction");
}

#[test]
fn old_checkpoints_and_records_cannot_synthesize_generation_owners() {
    let mut w = world(profile(source(2, 1, 3)), 7);
    let cp = w.checkpoint().unwrap();
    for version in [37, 38] {
        let mut old = serde_json::to_value(&cp).unwrap();
        old["version"] = version.into();
        old["state"]["version"] = version.into();
        rejects(&w, old);
    }
    step(&mut w, &mut f::KernelAuthority::default());
    for version in [37, 38] {
        let mut records = w.records().to_vec();
        records[0].version = version;
        let mut resumed =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), cp.clone()).unwrap();
        assert!(resumed.replay(&records).is_err());
        assert_eq!(resumed.checkpoint().unwrap(), cp);
    }
}

#[test]
fn missing_raw_collections_and_equipment_remain_closed_and_transactional() {
    for case in 0..4 {
        let mut raw = source(2, 1, 3);
        match case {
            0 => raw.input.class_levels = init::Rows::Missing,
            1 => raw.input.external_features = init::Rows::Missing,
            2 => raw.input.template.equipment_template_id = 1,
            _ => {
                let init::Rows::Complete(rows) = &mut raw.input.spawn_template else {
                    unreachable!()
                };
                rows[0].equipment_id = 1;
            }
        }
        let mut p = f::initialized_profile();
        let before = p.clone();
        assert!(p
            .initialize_creature_generations(init::Rows::Complete(vec![raw]), vec![2])
            .is_err());
        assert_eq!(p, before);
    }
}

#[test]
fn current_health_override_clamps_to_each_selected_life_on_actual_respawn() {
    let mut raw = source(2, 1, 3);
    let init::Rows::Complete(rows) = &mut raw.input.class_levels else {
        unreachable!()
    };
    for row in rows {
        row.base_health = u32::from(row.level) + 1;
    }
    let init::Rows::Complete(rows) = &mut raw.input.spawn_template else {
        unreachable!()
    };
    rows[0].current_health = 1_000_000;
    let p = profile(raw);
    let mut different_life_observed = false;
    for seed in 0..16 {
        let mut w = world(p.clone(), seed);
        let level = owner(&w, 2).result().level;
        assert_eq!(
            w.state.npcs[&2].health,
            owner(&w, 2).result().stats.resources.max_health
        );
        kill(&mut w, &mut f::KernelAuthority::default());
        respawn(&mut w, &mut f::KernelAuthority::default());
        assert_eq!(
            w.state.npcs[&2].health,
            owner(&w, 2).result().stats.resources.max_health
        );
        assert_selection(&w, 2);
        assert_eq!(restore(&w).state_digest(), w.state_digest());
        if owner(&w, 2).result().level != level {
            different_life_observed = true;
            break;
        }
    }
    assert!(
        different_life_observed,
        "actual respawn must perform a fresh variable-level selection"
    );
}

#[derive(Default)]
struct MotionAuthority {
    source: synthetic_motion_fixture::SyntheticMotionAuthority,
    kernel: f::KernelAuthority,
}
impl Authority for MotionAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::NativeMelee { .. }) {
            self.kernel.resolve(request)
        } else {
            self.source.resolve(request)
        }
    }
}
#[test]
fn source_motion_lethal_callback_and_respawn_keep_owned_selection_and_replay() {
    let mut p = profile(source(2, 1, 3));
    let mut motion = synthetic_motion_fixture::genesis(&p, 2);
    motion.npcs.get_mut(&2).unwrap().npc_policy.walk_speed = f::real(2.5);
    p.source_motion = Some(motion);
    p.validate().unwrap();
    let mut w = world(p, 7);
    let start = w.checkpoint().unwrap();
    let mut authority = MotionAuthority::default();
    authority.source.critical_chance = Some(0);
    authority.source.resist_chance = Some(0);
    kill(&mut w, &mut authority);
    assert!(w.state.npcs[&2].source_motion.is_some());
    assert_eq!(owner(&w, 2).current_death_tick(), Some(w.state.tick));
    let saved = restore(&w);
    assert_eq!(saved.state_digest(), w.state_digest());
    respawn(&mut w, &mut authority);
    assert_eq!(w.state.npcs[&2].id.generation, 1);
    assert_selection(&w, 2);
    assert!(w.records().iter().any(|record| {
        record
            .replies
            .iter()
            .any(|reply| matches!(reply.request.query, Query::SourceMotion(_)))
    }));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}

fn loot_setup() -> (
    Profile,
    benilla_sim::starter_init::StarterInput,
    QuestProgressionInput,
) {
    let (mut p, input, quests) = synthetic_loot_fixture::setup();
    // This immutable source has no script reaction writer.
    p.npcs
        .iter_mut()
        .for_each(|npc| npc.ai.reaction = ai::Reaction::Defensive);
    let first = source(2, 1, 3);
    let mut giver = source(3, 1, 3);
    giver.spawn_entry = 60003;
    giver.input.template.entry = 60003;
    giver.input.template.npc_flags = 2;
    (install(p, vec![first, giver], vec![2, 3]), input, quests)
}

#[test]
fn actual_body_loot_precedes_death_delay_and_recomputed_overlaps_fail_after_rehash() {
    let (mut p, input, quests) = loot_setup();
    p.initialize_quest_progression(&input, quests).unwrap();
    let mut w = world(p, 7);
    let initial = w.checkpoint().unwrap();
    let prefix = w.state.rng_counter;
    kill(
        &mut w,
        &mut synthetic_loot_fixture::LootAuthority::default(),
    );
    let corpse = w.state.npcs[&2].id;
    let receipt = w.state.npcs[&2].corpse_loot.as_ref().unwrap();
    let death = owner(&w, 2).history()[0].death_draw().unwrap();
    assert!(receipt.first_counter >= prefix);
    assert!(receipt.first_counter < receipt.end_counter);
    assert_eq!(receipt.end_counter, death.first_counter);
    assert_eq!(w.last_tick().unwrap().draws.last().unwrap(), death);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());

    // Both forgeries use genuine deterministic loot samples for their new spans.
    // One steals a death-delay word; the other steals the creature-genesis prefix.
    for first_counter in [
        death.first_counter,
        w.state.initial_cast_genesis.end_counter(),
    ] {
        let mut forged = w.clone();
        let genesis = w
            .profile
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap();
        let row = &genesis.creatures[&w.profile.npc(2).unwrap().entry];
        let mut probe = w.state.clone();
        probe.rng_counter = first_counter;
        let mut draws = vec![];
        let generated = genesis
            .catalog
            .generate(
                row.loot_id,
                row.minimum_money,
                row.maximum_money,
                &genesis.rules,
                &mut loot::RandomHost {
                    state: &mut probe,
                    corpse,
                    draws: &mut draws,
                },
            )
            .unwrap();
        assert!(probe.rng_counter > first_counter);
        let loot = forged
            .state
            .npcs
            .get_mut(&2)
            .unwrap()
            .corpse_loot
            .as_mut()
            .unwrap();
        loot.first_counter = first_counter;
        loot.end_counter = probe.rng_counter;
        loot.generated = generated;
        loot.draws = draws;
        forged.state.rng_counter = forged.state.rng_counter.max(probe.rng_counter);
        assert!(creature_generation_binding::validate_prefix(
            &forged.profile,
            &forged.state,
            forged.state.initial_cast_genesis.end_counter()
        )
        .is_err());
        let mut value = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
        value["state"] = serde_json::to_value(forged.snapshot_of(&forged.state)).unwrap();
        rejects(&w, value);
    }
}

#[test]
fn selected_quest_flags_and_kill_classification_conflicts_reject_transactionally() {
    for case in 0..5 {
        let (mut p, input, mut quests) = loot_setup();
        let before = p.clone();
        match case {
            0 => quests.npc_rows[0].npc_flags = 0,
            1 => quests.npc_rows[0].unit_flags = 1,
            2 => quests.kill.creatures[0].is_critter = true,
            3 => quests.kill.creatures[0].is_elite = true,
            _ => quests.kill.creatures[0].player_controlled = true,
        }
        assert!(p.interactions.is_none());
        assert!(
            p.initialize_quest_progression(&input, quests).is_err(),
            "cross-catalog conflict {case}"
        );
        assert_eq!(p, before);
    }
}

#[test]
fn removed_owner_native_melee_uses_selected_level_and_final_damage_range() {
    use benilla_sim::npc_melee as melee;
    let mut raw = source(2, 1, 3);
    let init::Rows::Complete(rows) = &mut raw.input.spawn_template else {
        unreachable!()
    };
    rows[0].current_health = 0;
    raw.input.template.extra_flags = 0;
    let mut p = f::initialized_profile();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    let p = install(p, vec![raw], vec![2]);
    let mut w = (0..64)
        .map(|seed| world(p.clone(), seed))
        .find(|w| owner(w, 2).result().level == 3)
        .unwrap();
    let initial = w.checkpoint().unwrap();
    for _ in 0..100 {
        let selected = w.state.npcs[&2].effective_profile(&w.profile).unwrap();
        let combat = w.state.combat.as_ref().unwrap();
        let defender = melee::MageDefense {
            level: w.state.level(&w.profile),
            agility: combat.stats.attributes[1] as u32,
            defense_skill: combat.skills[&combat::DEFENSE_SKILL].current_value(),
            armor: combat.stats.resistances[0],
            standing: true,
            casting_non_melee: false,
            has_channeled_spell: false,
            has_auto_repeat_spell: false,
        };
        let range = |npc: &NpcProfile| {
            melee::prepare(melee::MeleeInput {
                attacker: npc.combat.as_ref().unwrap().attacker(npc.ai.level),
                defender,
                health: w.state.health,
                front_facing: true,
            })
            .unwrap()
            .damage_range()
        };
        let expected = range(&selected);
        let stale = range(w.profile.npc(2).unwrap());
        assert_ne!(
            expected, stale,
            "fixture must distinguish selected damage from static placeholders"
        );
        step(&mut w, &mut f::KernelAuthority::default());
        if let Some(draw) = w
            .last_tick()
            .unwrap()
            .draws
            .iter()
            .find(|draw| matches!(draw.purpose, DrawPurpose::MeleeDamage { .. }))
        {
            assert_eq!(
                (draw.low, draw.high),
                (i64::from(expected.0), i64::from(expected.1))
            );
            assert!(w
                .last_tick()
                .unwrap()
                .events
                .iter()
                .any(|event| matches!(event, Event::Melee { .. })));
            let mut replay =
                NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
            replay.replay(w.records()).unwrap();
            assert_eq!(replay.state_digest(), w.state_digest());
            return;
        }
    }
    panic!("aggressive selected owner never performed a native melee callback");
}

#[test]
fn historical_target_projection_is_exact_life_and_rejects_dead_or_unborn_intervals() {
    let mut w = world(profile(source(2, 1, 3)), 7);
    let previous = w.state.npcs[&2].id;
    let previous_result = owner(&w, 2).result().clone();
    kill(&mut w, &mut f::KernelAuthority::default());
    let death_ms = w.state.now_ms;
    respawn(&mut w, &mut f::KernelAuthority::default());
    let current = w.state.npcs[&2].id;
    let birth_ms = w.state.now_ms;
    let npc = &w.state.npcs[&2];
    let old = npc.historical_profile(&w.profile, previous).unwrap();
    assert_eq!(old.ai.level, previous_result.level);
    assert_eq!(old.max_health, previous_result.stats.resources.max_health);
    assert_eq!(
        npc.historical_initialized_creature(previous),
        Some(&previous_result)
    );
    let historical =
        |id, at| interactions::spell_authority::historical_target_profile(&w.profile, npc, id, at);
    assert_eq!(historical(previous, death_ms).unwrap(), old);
    assert_eq!(
        historical(previous, w.profile.clock_origin_ms).unwrap(),
        old
    );
    assert_eq!(
        historical(current, birth_ms).unwrap(),
        npc.effective_profile(&w.profile).unwrap()
    );
    assert!(historical(previous, death_ms + u64::from(w.profile.tick_ms)).is_err());
    assert!(historical(current, birth_ms - u64::from(w.profile.tick_ms)).is_err());
    assert!(historical(current, birth_ms + 1).is_err());
    assert!(historical(
        crate::NpcId {
            spawn: 7,
            generation: 0
        },
        birth_ms
    )
    .is_err());
    assert!(historical(
        crate::NpcId {
            spawn: 2,
            generation: current.generation + 1
        },
        birth_ms
    )
    .is_err());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
}

fn complete_scene_sources(p: &Profile) -> Vec<CreatureGenerationSource> {
    p.npcs
        .iter()
        .map(|npc| {
            let mut raw = source(npc.ai.creature_id, 1, 1);
            raw.input.template.extra_flags = match npc.ai.reaction {
                ai::Reaction::Defensive => 2,
                ai::Reaction::Aggressive => 0,
                ai::Reaction::Passive => panic!("source fixture has no passive reaction writer"),
            };
            raw.spawn_entry = npc.entry;
            raw.input.template.entry = npc.entry;
            raw.input.template.faction = npc.faction_template;
            raw.input.template.creature_type = 7; // Exact authored source-spell target row.
            raw.input.template.mainhand_interval_ms = npc.ai.mainhand_interval_ms;
            raw.input.template.npc_flags = u32::from(npc.ai.creature_id == 3) * 2;
            let shift = u32::try_from(npc.ai.creature_id - 2).unwrap() * 100;
            for display in &mut raw.input.template.displays {
                if display.id != 0 {
                    display.id += shift;
                }
            }
            let init::Rows::Complete(rows) = &mut raw.input.models else {
                unreachable!()
            };
            for row in rows {
                row.id += shift;
                if row.other_gender_id != 0 {
                    row.other_gender_id += shift;
                }
                row.bounding_radius = f::float(npc.ai.bounding_radius.as_f32());
                row.combat_reach = f::float(npc.ai.combat_reach.as_f32());
                row.speed_run = f::float(npc.ai.run_speed.as_f32() / 7.0);
                row.speed_walk = f::float(
                    p.npc_motion(npc.ai.creature_id)
                        .unwrap()
                        .npc_policy
                        .walk_speed
                        .as_f32()
                        / 2.5,
                );
            }
            let init::Rows::Complete(rows) = &mut raw.input.displays else {
                unreachable!()
            };
            for row in rows {
                row.id += shift;
            }
            let init::Rows::Complete(rows) = &mut raw.input.spawn_template else {
                unreachable!()
            };
            rows[0].spawn_flags = 1; // Exact authored capability source row.
            raw
        })
        .collect()
}

#[test]
fn complete_source_motion_spell_loot_pipeline_draws_death_delay_only_after_body_loot() {
    let (p, initial, mut quests) = synthetic_distinct_quest_fixture::setup();
    let sources = complete_scene_sources(&p);
    let mut p = install(p, sources, vec![2, 3]);
    let loot = quests.loot.as_mut().unwrap();
    loot.captures[0].rows[0].chance_or_quest_chance_bits = 75.0_f32.to_bits();
    loot.captures[0].rows[0].max_count = 8;
    loot.creatures
        .iter_mut()
        .find(|row| row.entry == 60001)
        .unwrap()
        .maximum_money = 9;
    p.initialize_quest_progression(&initial, quests).unwrap();
    let mut w = world(p, 7);
    let start = w.checkpoint().unwrap();
    let prefix = w.state.rng_counter;
    let mut authority = synthetic_distinct_quest_fixture::spatial_only();
    for _ in 0..100 {
        if w.state.npcs[&2].health == 0 {
            break;
        }
        let command = w.state.casts.is_empty().then(|| f::cast(&w, 133));
        w.step(w.state.tick + 1, command, &mut authority).unwrap();
    }
    assert_eq!(w.state.npcs[&2].health, 0);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
    let loot = w.state.npcs[&2].corpse_loot.as_ref().unwrap();
    let death = owner(&w, 2).history()[0].death_draw().unwrap();
    assert!(loot.first_counter >= prefix);
    assert!(loot.end_counter > loot.first_counter);
    assert_eq!(loot.end_counter, death.first_counter);
    let record = w.last_tick().unwrap();
    let death_index = record.draws.iter().position(|draw| draw == death).unwrap();
    assert!(record.draws[..death_index].iter().any(|draw| matches!(
        draw.purpose,
        DrawPurpose::Loot { .. } | DrawPurpose::LootChance { .. } | DrawPurpose::LootShuffle { .. }
    )));
    assert!(record.draws[death_index + 1..].iter().all(|draw| !matches!(
        draw.purpose,
        DrawPurpose::Loot { .. } | DrawPurpose::LootChance { .. } | DrawPurpose::LootShuffle { .. }
    )));
    assert!(authority
        .queries
        .iter()
        .any(|request| matches!(request.query, Query::SourceSpell(_))));
    assert!(authority
        .base
        .queries
        .iter()
        .any(|request| matches!(request.query, Query::SourceMotion(_))));
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    respawn(&mut w, &mut authority);
    assert_eq!(owner(&w, 2).history().len(), 2);
    assert_selection(&w, 2);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
}

#[test]
fn selected_event_ai_keeps_historical_life_and_rejects_recomputed_counter_overlap() {
    use benilla_sim::event_ai as text_source;
    let p = synthetic_event_ai_fixture::profile();
    let mut sources = complete_scene_sources(&p);
    for raw in &mut sources {
        raw.input.template.maximum_level = 3;
        raw.input.template.npc_flags = 0;
        raw.input.class_levels = source(raw.spawn_id, 1, 3).input.class_levels;
    }
    let mut w = world(install(p, sources, vec![2, 3]), 7);
    let start = w.checkpoint().unwrap();
    let previous = w.state.npcs[&2].id;
    let mut authority = synthetic_event_ai_fixture::Authority::default();
    for _ in 0..20 {
        step(&mut w, &mut authority);
        if !w.state.event_ai.as_ref().unwrap().history.is_empty() {
            break;
        }
    }
    assert!(w.state.event_ai.as_ref().unwrap().history.iter().any(|event| matches!(event, event_ai::EventAiHistory::EnterCombat(e) if e.callback.npc == previous && !e.actions.is_empty())));
    kill(&mut w, &mut authority);
    let death_tick = w.state.tick;
    let death_counter = owner(&w, 2).history()[0]
        .death_draw()
        .unwrap()
        .first_counter;
    respawn(&mut w, &mut authority);
    assert_selection(&w, 2);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let cp = w.checkpoint().unwrap();
    w.checkpoint_committed(&cp).unwrap();
    assert_eq!(restore(&w).state_digest(), w.state_digest());

    for first_counter in [w.state.initial_cast_genesis.end_counter(), death_counter] {
        let mut forged = w.clone();
        let selected = w.profile.event_ai().unwrap();
        let event = selected.catalog().events(2).unwrap()[0];
        let gender = owner(&w, 2)
            .historical(previous)
            .unwrap()
            .result()
            .model
            .gender;
        let evaluation = forged
            .state
            .event_ai
            .as_mut()
            .unwrap()
            .history
            .iter_mut()
            .find_map(|history| match history {
                event_ai::EventAiHistory::EnterCombat(e) if e.callback.npc == previous => Some(e),
                _ => None,
            })
            .unwrap();
        let action = &mut evaluation.actions[0];
        let mut holder = text_source::AggroTextHolder::new(event);
        holder.begin_enter_combat();
        holder
            .consume_chance(rng::full_u32(w.state.world, w.state.seed, first_counter))
            .unwrap();
        let selection = holder
            .consume_action(rng::full_u32(
                w.state.world,
                w.state.seed,
                first_counter + 1,
            ))
            .unwrap();
        action.chance_counter = first_counter;
        action.action_counter = Some(first_counter + 1);
        action.broadcast_id = Some(selection.text_id);
        action.text_slot = Some(selection.text_slot);
        action.speech = Some(
            EventAiState::speech(
                selected,
                &evaluation.callback,
                evaluation.occurrence,
                selection,
                Some(gender),
            )
            .unwrap(),
        );
        let mut anchors = BTreeMap::from([
            (2, event_ai::EventAiCallbackAnchor::default()),
            (3, event_ai::EventAiCallbackAnchor::default()),
        ]);
        for history in &forged.state.event_ai.as_ref().unwrap().history {
            if let event_ai::EventAiHistory::EnterCombat(e) = history {
                anchors
                    .get_mut(&e.callback.npc.spawn)
                    .unwrap()
                    .append(e)
                    .unwrap();
            }
        }
        for (spawn, anchor) in anchors {
            forged
                .state
                .npcs
                .get_mut(&spawn)
                .unwrap()
                .events
                .event_ai_anchor = Some(anchor);
        }
        assert!(creature_generation_binding::validate_prefix(
            &forged.profile,
            &forged.state,
            forged.state.initial_cast_genesis.end_counter()
        )
        .is_err());
        let mut value = serde_json::to_value(&cp).unwrap();
        value["state"] = serde_json::to_value(forged.snapshot_of(&forged.state)).unwrap();
        rejects(&w, value);
    }

    for mutate_respawn in [false, true] {
        let mut forged = w.clone();
        if mutate_respawn {
            let event_ai::EventAiHistory::Respawn { tick, .. } = forged.state.event_ai.as_mut().unwrap().history.iter_mut().find(|history| matches!(history, event_ai::EventAiHistory::Respawn { replacement, .. } if replacement.spawn == 2)).unwrap() else { unreachable!() };
            *tick -= 1;
        } else {
            let evaluation = forged
                .state
                .event_ai
                .as_mut()
                .unwrap()
                .history
                .iter_mut()
                .find_map(|history| match history {
                    event_ai::EventAiHistory::EnterCombat(e) if e.callback.npc == previous => {
                        Some(e)
                    }
                    _ => None,
                })
                .unwrap();
            evaluation.callback.tick = death_tick + 1;
            evaluation.callback.now_ms =
                w.profile.clock_origin_ms + (death_tick + 1) * u64::from(w.profile.tick_ms);
        }
        assert!(EventAiState::validate_optional(&forged.profile, &forged.state).is_err());
        let mut value = serde_json::to_value(&cp).unwrap();
        value["state"] = serde_json::to_value(forged.snapshot_of(&forged.state)).unwrap();
        rejects(&w, value);
    }
}

#[test]
fn selected_generation_player_slow_combination_keeps_explicit_slot_history_gate() {
    let mut p = synthetic_slow_fixture::profile();
    let before = p.clone();
    assert_eq!(
        p.initialize_creature_generations(init::Rows::Complete(vec![source(2, 1, 3)]), vec![2]),
        Err(NativeError::UnsupportedMechanic(
            "generation-owned player slow slot residue history"
        ))
    );
    assert_eq!(p, before);
}

#[test]
fn old_life_removal_ai_proves_corpse_expiry_before_later_respawn_after_compaction() {
    let mut w = world(profile(source(2, 1, 3)), 7);
    let mut authority = f::KernelAuthority::default();
    kill(&mut w, &mut authority);
    respawn(&mut w, &mut authority);
    assert_eq!(owner(&w, 2).corpse_delay_seconds(), 5);
    kill(&mut w, &mut authority);
    let death_tick = w.state.tick;
    let death_ms = w.state.now_ms;
    let respawn_due_ms = (death_ms / 1000 + u64::from(owner(&w, 2).respawn_delay_seconds())) * 1000;
    let corpse_due_ms = death_ms + 5000;
    assert!(respawn_due_ms < corpse_due_ms);
    respawn(&mut w, &mut authority);
    assert_eq!(w.state.npcs[&2].id.generation, 2);
    assert!(w.state.now_ms > corpse_due_ms);
    assert_eq!(
        owner(&w, 2).history()[1].corpse_removed_tick().unwrap() * u64::from(w.profile.tick_ms),
        corpse_due_ms
    );
    let checkpoint = w.checkpoint().unwrap();
    w.checkpoint_committed(&checkpoint).unwrap();
    assert!(w.records().is_empty());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let base = serde_json::to_value(&checkpoint).unwrap();

    let early_tick = death_tick + 1;
    let early_ms = w.profile.clock_origin_ms + early_tick * u64::from(w.profile.tick_ms);
    let mut early_removal = base.clone();
    let history = &mut early_removal["state"]["npcs"]["2"]["source_generation"]["generations"][1];
    history["corpse_removed_tick"] = early_tick.into();
    let ai = &mut history["corpse_removal_ai"];
    ai["now_ms"] = early_ms.into();
    ai["sequence"] = early_tick.into();
    ai["despawned_at_ms"] = early_ms.into();
    assert!(ai["corpse_at_ms"].as_u64().unwrap() > early_ms);
    rejects(&w, early_removal.clone());

    // Even matching the forged removal and replacement times cannot shorten the
    // retained entry corpse delay. The current AI has already cleared this life.
    let early_birth_tick =
        (respawn_due_ms - w.profile.clock_origin_ms) / u64::from(w.profile.tick_ms);
    assert!(early_birth_tick > early_tick);
    early_removal["state"]["npcs"]["2"]["source_generation"]["generations"][2]["created_tick"] =
        early_birth_tick.into();
    rejects(&w, early_removal);

    let mut missing_proof = base.clone();
    missing_proof["state"]["npcs"]["2"]["source_generation"]["generations"][1]
        ["corpse_removal_ai"] = serde_json::Value::Null;
    rejects(&w, missing_proof);
    for field in ["corpse_removal_ai", "corpse_timer_calls"] {
        let mut missing = base.clone();
        missing["state"]["npcs"]["2"]["source_generation"]["generations"][1]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(
            serde_json::from_value::<Checkpoint>(missing).is_err(),
            "required historical field {field}"
        );
    }
}

#[test]
fn actual_loot_timer_chain_binds_duplicate_ai_metadata_and_prior_removal_boundary() {
    let (mut p, input, mut quests) = loot_setup();
    let loot = quests.loot.as_mut().unwrap();
    loot.captures[0].rows[0].chance_or_quest_chance_bits = 100f32.to_bits();
    loot.captures[0].rows[0].min_count_or_reference = 1;
    loot.captures[0].rows[0].max_count = 1;
    for row in &mut loot.creatures {
        row.minimum_money = 0;
        row.maximum_money = 0;
        row.corpse_acceleration_decay_ms = 3000;
    }
    p.initialize_quest_progression(&input, quests).unwrap();
    let mut w = world(p, 7);
    let initial = w.checkpoint().unwrap();
    let mut authority = synthetic_loot_fixture::LootAuthority::default();
    kill(&mut w, &mut authority);
    let corpse = w.state.npcs[&2].id;
    let original_deadline = w.state.npcs[&2].ai.corpse_expiration_ms();
    let tick_ms = u64::from(w.profile.tick_ms);
    while w.state.now_ms + tick_ms < original_deadline {
        step(&mut w, &mut authority);
    }
    // Player callbacks run before this NPC's update: the exact prior removal
    // boundary is reachable and extends the real corpse rather than losing it.
    let command = synthetic_quest_fixture::command(&w, Command::OpenLoot { corpse });
    w.step(w.state.tick + 1, Some(command), &mut authority)
        .unwrap();
    assert_eq!(w.state.now_ms, original_deadline);
    assert_eq!(w.state.npcs[&2].ai.phase(), ai::Phase::Corpse);
    assert_eq!(
        w.state.npcs[&2].ai.corpse_expiration_ms(),
        original_deadline + 3000
    );
    assert!(w.state.npcs[&2].corpse_loot.as_ref().unwrap().opened);
    for command in [
        Command::LootItem { corpse, slot: 0 },
        Command::ReleaseLoot { corpse },
    ] {
        let input = synthetic_quest_fixture::command(&w, command);
        w.step(w.state.tick + 1, Some(input), &mut authority)
            .unwrap();
    }
    assert!(w.state.npcs[&2].corpse_loot.as_ref().unwrap().looted);
    assert_eq!(owner(&w, 2).history()[0].corpse_timer_calls().len(), 2);
    assert_eq!(w.state.npcs[&2].ai.corpse_timer_calls().len(), 1);
    assert_eq!(
        w.state.npcs[&2].ai.corpse_timer_previous_ms(),
        original_deadline
    );
    // The release is a real callback even though min(previous, until) leaves
    // the deadline unchanged; its time differs from the last changing call.
    assert_eq!(
        w.state.npcs[&2].ai.corpse_timer_last_ms(),
        original_deadline + 2 * tick_ms
    );
    step(&mut w, &mut authority);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let current = serde_json::to_value(w.checkpoint().unwrap()).unwrap();

    for (field, changed) in [
        ("corpse_timer_previous_ms", original_deadline + tick_ms),
        ("corpse_timer_last_ms", w.state.now_ms),
    ] {
        let mut forged = current.clone();
        forged["state"]["npcs"]["2"]["ai"][field] = changed.into();
        // These duplicate values satisfy the isolated AI checkpoint arithmetic.
        // The immutable full callback history must reject each independently.
        let ai = ai::SoloCreatureAi::restore_generation_owned(
            serde_json::from_value(forged["state"]["npcs"]["2"]["ai"].clone()).unwrap(),
            w.state.npcs[&2].ai.effective_profile().clone(),
            w.state.now_ms,
            w.state.tick,
            SOLO_UNIT,
            corpse.generation,
        )
        .unwrap();
        assert!(owner(&w, 2)
            .validate_corpse_timer(&ai, w.profile.tick_ms, w.state.now_ms)
            .is_err());
        rejects(&w, forged);
    }

    // The forged inspecting call is one fixed step after the old corpse would
    // already have been removed. All duplicate timestamps and the extended
    // final deadline agree, so arithmetic consistency alone cannot admit it.
    let mut late = current.clone();
    let late_call = original_deadline + tick_ms;
    late["state"]["npcs"]["2"]["source_generation"]["generations"][0]["corpse_timer_calls"][0]
        ["at_ms"] = late_call.into();
    late["state"]["npcs"]["2"]["ai"]["corpse_timer_calls"][0]["at_ms"] = late_call.into();
    late["state"]["npcs"]["2"]["ai"]["corpse_at_ms"] = (late_call + 3000).into();
    ai::SoloCreatureAi::restore_generation_owned(
        serde_json::from_value(late["state"]["npcs"]["2"]["ai"].clone()).unwrap(),
        w.state.npcs[&2].ai.effective_profile().clone(),
        w.state.now_ms,
        w.state.tick,
        SOLO_UNIT,
        corpse.generation,
    )
    .unwrap();
    rejects(&w, late);

    respawn(&mut w, &mut authority);
    assert_eq!(w.state.npcs[&2].id.generation, 1);
    assert_eq!(owner(&w, 2).history()[0].corpse_timer_calls().len(), 2);
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    let checkpoint = w.checkpoint().unwrap();
    w.checkpoint_committed(&checkpoint).unwrap();
    assert!(w.records().is_empty());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    for (field, changed) in [
        ("corpse_timer_previous_ms", original_deadline + tick_ms),
        ("corpse_timer_last_ms", original_deadline + 3 * tick_ms),
    ] {
        let mut forged = serde_json::to_value(&checkpoint).unwrap();
        forged["state"]["npcs"]["2"]["source_generation"]["generations"][0]["corpse_removal_ai"]
            [field] = changed.into();
        rejects(&w, forged);
    }
}

#[test]
fn generation_extra_flags_bind_reaction_and_reject_unowned_melee_controls() {
    let p = profile(source(2, 1, 1));
    assert!(p.interactions.is_none());
    let mut wrong_reaction = p.clone();
    wrong_reaction.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    assert_eq!(wrong_reaction.validate(), Err(NativeError::InvalidProfile));
    for flag in [0x0002_0000, 0x0008_0000, 0x8000_0000] {
        let mut raw = source(2, 1, 1);
        raw.input.template.extra_flags |= flag;
        let before = p.clone();
        let mut changed = p.clone();
        assert_eq!(
            changed.initialize_creature_generations(init::Rows::Complete(vec![raw]), vec![2]),
            Err(NativeError::UnsupportedMechanic(
                "unowned creature generation extra flags"
            ))
        );
        assert_eq!(changed, before);
    }
}
