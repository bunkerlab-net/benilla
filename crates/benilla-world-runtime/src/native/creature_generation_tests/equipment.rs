// SPDX-License-Identifier: GPL-3.0-or-later
//! Reviewed public equipment 6/item 5276 metadata with authored surrounding
//! classlevel/model/display rows. No installed assets or native-content admission.
use super::*;

const DISARMED: u32 = 0x0020_0000;

fn staff() -> init::ItemTemplateRow {
    // classic-db ec4f596146be6467ea93c57397858e329e2db852 ordinary final row.
    init::ItemTemplateRow {
        entry: 5276,
        display_id: 5010,
        class: 2,
        subclass: 10,
        material: 2,
        inventory_type: 17,
        sheath: 2,
        delay_ms: 0,
        damage: [init::ItemDamage {
            minimum: f::float(0.0),
            maximum: f::float(0.0),
            school: 0,
        }; 5],
    }
}

fn equipped_source(
    spawn: u64,
    minimum: u8,
    maximum: u8,
    disarmed: bool,
) -> CreatureGenerationSource {
    let mut raw = source(spawn, minimum, maximum);
    raw.input.template.equipment_template_id = 6;
    raw.input.template.extra_flags = 2;
    raw.input.template.mainhand_interval_ms = 2000;
    raw.input.template.unit_flags = if disarmed { DISARMED } else { 0 };
    raw.input.equipment_templates = init::Rows::Complete(vec![init::EquipmentTemplateRow {
        entry: 6,
        item_entries: [5276, 0, 0],
    }]);
    raw.input.item_templates = init::Rows::Complete(vec![staff()]);
    raw
}

fn equipment_rows(raw: &mut CreatureGenerationSource) -> &mut Vec<init::EquipmentTemplateRow> {
    let init::Rows::Complete(rows) = &mut raw.input.equipment_templates else {
        panic!("authored complete equipment rows")
    };
    rows
}

fn item_rows(raw: &mut CreatureGenerationSource) -> &mut Vec<init::ItemTemplateRow> {
    let init::Rows::Complete(rows) = &mut raw.input.item_templates else {
        panic!("authored complete item rows")
    };
    rows
}

fn spawn_row(raw: &mut CreatureGenerationSource) -> &mut init::SpawnTemplate {
    let init::Rows::Complete(rows) = &mut raw.input.spawn_template else {
        panic!("authored resolved spawn row")
    };
    &mut rows[0]
}

fn equipped_profile(raw: CreatureGenerationSource) -> Profile {
    let mut p = f::initialized_profile();
    p.npcs[0].ai.reaction = if raw.input.template.extra_flags & 2 != 0 {
        ai::Reaction::Defensive
    } else {
        ai::Reaction::Aggressive
    };
    let init::Rows::Complete(rows) = &raw.input.spawn_template else {
        unreachable!()
    };
    p.npcs[0].health_regeneration.unit_flags = if rows[0].unit_flags == -1 {
        raw.input.template.unit_flags
    } else {
        rows[0].unit_flags as u32
    };
    install(p, vec![raw], vec![2])
}

fn attack_profile(mut raw: CreatureGenerationSource) -> Profile {
    raw.input.template.extra_flags &= !2;
    let mut p = equipped_profile(raw);
    // This timer/RNG probe is stationary; a hostfloat straight-line fixture is
    // not a source-accurate fractional movement oracle.
    p.npcs[0].ai.spawn = p.player.start;
    p
}

fn assert_staff(value: &init::InitializedCreature) {
    assert_eq!(value.equipment.equipment_template_id, 6);
    assert_eq!(
        value.equipment.mainhand,
        Some(init::InitialVirtualItem {
            item_entry: 5276,
            display_id: 5010,
            info: [0x1102_0a02, 2],
            has_weapon: true,
        })
    );
    assert_eq!(value.unit_bytes_2, 0x1001);
    assert_eq!(value.mainhand_interval_ms, 2000);
}

fn first_melee(w: &mut NativeWorld) -> Draw {
    for _ in 0..100 {
        step(w, &mut f::KernelAuthority::default());
        let record = w.last_tick().unwrap();
        if let Some(index) = record
            .draws
            .iter()
            .position(|draw| matches!(draw.purpose, DrawPurpose::MeleeDamage { .. }))
        {
            assert!(matches!(
                record.draws[index + 1].purpose,
                DrawPurpose::MeleeOutcome { .. }
            ));
            assert!(record
                .events
                .iter()
                .any(|event| matches!(event, Event::Melee { .. })));
            assert!(record
                .replies
                .iter()
                .any(|reply| matches!(reply.request.query, Query::NativeMelee { .. })));
            return record.draws[index].clone();
        }
    }
    panic!("source-owned creature never executed its ordinary mainhand callback")
}

fn assert_same_gameplay_records(a: &NativeWorld, b: &NativeWorld) {
    assert_eq!(a.records().len(), b.records().len());
    for (left, right) in a.records().iter().zip(b.records()) {
        assert_eq!(left.tick, right.tick);
        assert_eq!(left.draws, right.draws);
        assert_eq!(left.events.len(), right.events.len());
        for (left, right) in left.events.iter().zip(&right.events) {
            match (left, right) {
                (
                    Event::Combat(CombatEvent::ActionCommitted {
                        action: a,
                        binding: ab,
                        tip: at,
                    }),
                    Event::Combat(CombatEvent::ActionCommitted {
                        action: b,
                        binding: bb,
                        tip: bt,
                    }),
                ) => {
                    // Same gameplay still authenticates different retained source profiles.
                    assert_eq!(a, b);
                    assert_ne!(ab, bb);
                    assert_ne!(at, bt);
                }
                _ => assert_eq!(left, right),
            }
        }
    }
}

#[test]
fn public_staff_uses_creature_stats_and_timing_without_consuming_an_equipment_draw() {
    let armed = equipped_source(2, 1, 3, false);
    let mut empty = armed.clone();
    spawn_row(&mut empty).equipment_id = 0;
    let mut changed = armed.clone();
    let item = &mut item_rows(&mut changed)[0];
    item.delay_ms = 59_999;
    item.damage[0].minimum = f::float(900.25);
    item.damage[0].maximum = f::float(1999.5);
    item.damage[4].school = 6;
    item.damage[4].maximum = f::float(88.0);
    assert_ne!(armed.identity(), changed.identity());
    let mut a = world(attack_profile(armed), 7);
    let mut b = world(attack_profile(empty), 7);
    let mut c = world(attack_profile(changed), 7);
    assert_staff(owner(&a, 2).result());
    assert_staff(owner(&c, 2).result());
    assert_eq!(owner(&b, 2).result().equipment.mainhand, None);
    assert_eq!(owner(&b, 2).result().equipment.equipment_template_id, 0);
    assert_eq!(owner(&a, 2).result().stats, owner(&b, 2).result().stats);
    assert_eq!(owner(&a, 2).result(), owner(&c, 2).result());
    assert_ne!(a.profile_digest(), c.profile_digest());
    assert_eq!(owner(&a, 2).genesis_draws(), owner(&b, 2).genesis_draws());
    assert_eq!(owner(&a, 2).genesis_draws(), owner(&c, 2).genesis_draws());
    assert_eq!(a.state.rng_counter, b.state.rng_counter);
    assert_eq!(a.state.rng_counter, c.state.rng_counter);
    let first = first_melee(&mut a);
    assert_eq!(first_melee(&mut b), first);
    assert_eq!(first_melee(&mut c), first);
    assert_eq!(a.state.tick, b.state.tick);
    assert_eq!(a.state.tick, c.state.tick);
    let first_at_ms = a.state.now_ms;
    let second = first_melee(&mut a);
    assert_eq!(a.state.now_ms - first_at_ms, 2000);
    assert_eq!(first_melee(&mut b), second);
    assert_eq!(first_melee(&mut c), second);
    assert_eq!(a.state.now_ms, b.state.now_ms);
    assert_eq!(a.state.now_ms, c.state.now_ms);
    assert_same_gameplay_records(&a, &b);
    assert_same_gameplay_records(&a, &c);
    assert_eq!(a.state.health, b.state.health);
    assert_eq!(a.state.health, c.state.health);
    assert_eq!(a.state.rng_counter, b.state.rng_counter);
    assert_eq!(a.state.rng_counter, c.state.rng_counter);
}

fn fractional_damage_source(disarmed: bool) -> CreatureGenerationSource {
    let mut raw = equipped_source(2, 1, 1, disarmed);
    raw.input.template.damage_variance = f::float(0.5);
    let init::Rows::Complete(rows) = &mut raw.input.class_levels else {
        unreachable!()
    };
    rows[0].base_damage = f::float(12.0);
    rows[0].base_melee_attack_power = f::float(0.0);
    raw
}

#[test]
fn static_disarm_halves_binary32_before_the_damage_draw_and_still_executes_attacks() {
    let armed = fractional_damage_source(false);
    let disarmed = fractional_damage_source(true);
    let mut non_postdivision = false;
    for seed in 0..32 {
        let mut a = world(attack_profile(armed.clone()), seed);
        let mut d = world(attack_profile(disarmed.clone()), seed);
        let full = owner(&a, 2).result();
        let half = owner(&d, 2).result();
        assert_staff(full);
        assert_staff(half);
        assert_eq!(half.unit_flags, DISARMED);
        assert_eq!(full.stats.damage.minimum, f::float(9.0));
        assert_eq!(full.stats.damage.maximum, f::float(15.0));
        assert_eq!(half.stats.damage.minimum, f::float(4.5));
        assert_eq!(half.stats.damage.maximum, f::float(7.5));
        let mut expected = full.stats;
        expected.damage.minimum = f::float(4.5);
        expected.damage.maximum = f::float(7.5);
        assert_eq!(half.stats, expected);
        assert_eq!(owner(&a, 2).genesis_draws(), owner(&d, 2).genesis_draws());
        let full_draw = first_melee(&mut a);
        let half_draw = first_melee(&mut d);
        assert_eq!((full_draw.low, full_draw.high), (9, 15));
        assert_eq!((half_draw.low, half_draw.high), (4, 7));
        assert_eq!(full_draw.purpose, half_draw.purpose);
        assert_eq!(full_draw.first_counter, half_draw.first_counter);
        assert_eq!(full_draw.consumed, half_draw.consumed);
        assert_eq!(a.state.tick, d.state.tick);
        assert_eq!(a.state.rng_counter, d.state.rng_counter);
        assert_eq!(
            a.last_tick()
                .unwrap()
                .draws
                .iter()
                .map(|draw| draw.purpose)
                .collect::<Vec<_>>(),
            d.last_tick()
                .unwrap()
                .draws
                .iter()
                .map(|draw| draw.purpose)
                .collect::<Vec<_>>()
        );
        assert_eq!(restore(&d).state_digest(), d.state_digest());
        if half_draw.value != full_draw.value / 2 {
            non_postdivision = true;
            break;
        }
    }
    assert!(
        non_postdivision,
        "uniform pre-roll halving differs from integer post-roll division"
    );
}

#[test]
fn effective_spawn_flags_and_weapon_class_decide_disarm_without_erasing_equipment() {
    let full = fractional_damage_source(false);
    let half = fractional_damage_source(true);
    let mut added = full;
    spawn_row(&mut added).unit_flags = i64::from(DISARMED);
    let mut cleared = half.clone();
    spawn_row(&mut cleared).unit_flags = 0;
    let mut empty = half.clone();
    spawn_row(&mut empty).equipment_id = 0;
    let mut shield = half;
    let item = &mut item_rows(&mut shield)[0];
    item.class = 4;
    item.subclass = 6;
    item.inventory_type = 14;
    for (raw, halves, equipped) in [
        (added, true, true),
        (cleared, false, true),
        (empty, false, false),
        (shield, false, true),
    ] {
        let w = world(equipped_profile(raw), 7);
        let selected = owner(&w, 2).result();
        assert_eq!(selected.equipment.mainhand.is_some(), equipped);
        assert_eq!(
            selected.stats.damage.minimum,
            f::float(if halves { 4.5 } else { 9.0 })
        );
        assert_eq!(
            selected.stats.damage.maximum,
            f::float(if halves { 7.5 } else { 15.0 })
        );
        assert_eq!(selected.mainhand_interval_ms, 2000);
        assert_eq!(restore(&w).state_digest(), w.state_digest());
    }
}

#[test]
fn equipped_death_respawn_historical_projection_replay_and_compaction_keep_each_life() {
    let p = equipped_profile(equipped_source(2, 1, 3, true));
    let mut changed_level = false;
    for seed in 0..16 {
        let mut w = world(p.clone(), seed);
        let initial = w.checkpoint().unwrap();
        let old_id = w.state.npcs[&2].id;
        let old = owner(&w, 2).result().clone();
        assert_staff(&old);
        let mut authority = f::KernelAuthority::default();
        kill(&mut w, &mut authority);
        assert_eq!(owner(&w, 2).result(), &old);
        assert_eq!(owner(&w, 2).current_death_tick(), Some(w.state.tick));
        assert_eq!(
            owner(&w, 2).history()[0].death_draw().unwrap().purpose,
            DrawPurpose::CreatureDeathRespawnDelay { npc: old_id }
        );
        let mut corpse_resume = restore(&w);
        respawn(&mut w, &mut authority);
        while corpse_resume.state.tick < w.state.tick {
            step(&mut corpse_resume, &mut authority);
        }
        assert_eq!(corpse_resume.state_digest(), w.state_digest());
        let current = owner(&w, 2).result().clone();
        assert_staff(&current);
        assert_eq!(current.unit_flags, DISARMED);
        assert_selection(&w, 2);
        let old_life = owner(&w, 2).historical(old_id).unwrap();
        assert_eq!(old_life.result(), &old);
        assert!(old_life.corpse_removed_tick().is_some());
        assert_eq!(
            w.state.npcs[&2].historical_initialized_creature(old_id),
            Some(&old)
        );
        assert_eq!(w.state.npcs[&2].initialized_creature(), Some(&current));
        assert!(w.records().iter().flat_map(|record| &record.events).any(|event|
            matches!(event, Event::Npc { id, effect: ai::Effect::CorpseRemoved } if *id == old_id)));
        assert!(owner(&w, 2).history()[1]
            .creation_draws()
            .iter()
            .all(|draw| matches!(
                draw.purpose,
                DrawPurpose::CreatureModel { .. }
                    | DrawPurpose::CreatureGender { .. }
                    | DrawPurpose::CreatureLevel { .. }
            )));
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
        assert_eq!(owner(&replay, 2).history(), owner(&w, 2).history());
        let checkpoint = w.checkpoint().unwrap();
        let history = owner(&w, 2).history().to_vec();
        w.checkpoint_committed(&checkpoint).unwrap();
        assert!(w.records().is_empty());
        let reopened = restore(&w);
        assert_eq!(owner(&reopened, 2).history(), history);
        assert_eq!(reopened.state_digest(), w.state_digest());
        if current.level != old.level {
            assert_ne!(current.stats.damage, old.stats.damage);
            changed_level = true;
            break;
        }
    }
    assert!(
        changed_level,
        "current equipment must coexist with distinct historical-life stats"
    );
}

#[test]
fn changed_raw_equipment_cannot_reuse_stale_owner_evidence_after_outer_rehashing() {
    let raw = equipped_source(2, 1, 3, false);
    let mut w = world(equipped_profile(raw.clone()), 7);
    step(&mut w, &mut f::KernelAuthority::default());
    let original = w.checkpoint().unwrap();
    for case in 0..8 {
        let mut changed = raw.clone();
        match case {
            0 => item_rows(&mut changed)[0].display_id += 1,
            1 => item_rows(&mut changed)[0].subclass = 4,
            2 => item_rows(&mut changed)[0].material = -1,
            3 => item_rows(&mut changed)[0].sheath = 3,
            4 => item_rows(&mut changed)[0].delay_ms = 777,
            5 => item_rows(&mut changed)[0].damage[0].maximum = f::float(99.0),
            6 => equipment_rows(&mut changed)[0].item_entries = [0; 3],
            _ => changed.input.template.unit_flags = DISARMED,
        }
        assert_ne!(changed.identity(), raw.identity());
        let p = equipped_profile(changed);
        let mut forged = serde_json::to_value(&original).unwrap();
        let digest = serde_json::to_value(p.digest()).unwrap();
        forged["state"]["profile"] = digest.clone();
        forged["state"]["initial_cast_genesis"]["profile"] = digest.clone();
        forged["state"]["npcs"]["2"]["source_generation"]["profile"] = digest;
        // The stale owner source digest is not repaired by valid outer hashes.
        // Fully reconstructed source evidence is not a capture-authenticity claim.
        assert!(
            NativeWorld::restore_synthetic_component_test(p.clone(), resign(forged)).is_err(),
            "metadata case {case}"
        );
        let mut fresh = world(p, 7);
        let before = fresh.checkpoint().unwrap();
        assert!(fresh.replay(w.records()).is_err());
        assert_eq!(fresh.checkpoint().unwrap(), before);
        assert!(fresh.records().is_empty());
    }
    let mut stale_policy = serde_json::to_value(&original).unwrap();
    stale_policy["state"]["npcs"]["2"]["source_generation"]["policy"] =
        "cmangos-8ec338a1-native-creature-generation-v1".into();
    rejects(&w, stale_policy);
    let mut forged_result = serde_json::to_value(original).unwrap();
    forged_result["state"]["npcs"]["2"]["source_generation"]["generations"][0]["equipment"] =
        serde_json::to_value(owner(&w, 2).result().equipment).unwrap();
    assert!(serde_json::from_value::<Checkpoint>(forged_result).is_err());
}

fn two_spawn_profile(first: CreatureGenerationSource, second: CreatureGenerationSource) -> Profile {
    let mut p = f::initialized_profile();
    let mut npc = p.npcs[0].clone();
    npc.ai.creature_id = 7;
    npc.ai.reaction = ai::Reaction::Defensive;
    p.npcs.push(npc);
    install(p, vec![first, second], vec![2, 7])
}

#[test]
fn shared_selected_and_unused_equipment_rows_must_agree_across_spawns_transactionally() {
    let mut first = equipped_source(2, 1, 3, false);
    let mut unused = staff();
    unused.entry = 59999;
    item_rows(&mut first).push(unused);
    equipment_rows(&mut first).push(init::EquipmentTemplateRow {
        entry: 99,
        item_entries: [59999, 0, 0],
    });
    let mut second = first.clone();
    second.spawn_id = 7;
    let good = two_spawn_profile(first.clone(), second.clone());
    let w = world(good.clone(), 7);
    assert_staff(owner(&w, 2).result());
    assert_staff(owner(&w, 7).result());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    for case in 0..8 {
        let mut conflicting = second.clone();
        match case {
            0 => equipment_rows(&mut conflicting)[0].item_entries[0] = 59999,
            1 => item_rows(&mut conflicting)[0].display_id += 1,
            2 => item_rows(&mut conflicting)[0].subclass = 4,
            3 => item_rows(&mut conflicting)[0].material = -1,
            4 => item_rows(&mut conflicting)[0].delay_ms = 100,
            5 => item_rows(&mut conflicting)[0].damage[1].maximum = f::float(1.0),
            6 => equipment_rows(&mut conflicting)[1].item_entries[0] = 5276,
            _ => item_rows(&mut conflicting)[1].sheath = 3,
        }
        init::prepare(&conflicting.input).unwrap();
        let mut p = good.clone();
        let before = p.clone();
        assert!(
            p.initialize_creature_generations(
                init::Rows::Complete(vec![first.clone(), conflicting]),
                vec![2, 7]
            )
            .is_err(),
            "cross-spawn case {case}"
        );
        assert_eq!(p, before);
    }
}

#[test]
fn missing_equipment_closure_and_unsupported_writers_reject_without_replacing_the_catalog() {
    let raw = equipped_source(2, 1, 3, false);
    let good = equipped_profile(raw.clone());
    for case in 0..16 {
        let mut changed = raw.clone();
        match case {
            0 => changed.input.equipment_templates = init::Rows::Missing,
            1 => changed.input.item_templates = init::Rows::Missing,
            2 => equipment_rows(&mut changed).clear(),
            3 => item_rows(&mut changed).clear(),
            4 => changed.input.template.extra_flags |= 0x0080_0000,
            5 => equipment_rows(&mut changed)[0].item_entries[1] = 5276,
            6 => equipment_rows(&mut changed)[0].item_entries[2] = 5276,
            7 => {
                changed.input.external_features =
                    init::Rows::Complete(vec![init::ExternalFeature::EquipmentOrLootWeapon])
            }
            8 => {
                changed.input.external_features =
                    init::Rows::Complete(vec![init::ExternalFeature::InitialAuraOrUnitModifier])
            }
            9 => {
                changed.input.external_features =
                    init::Rows::Complete(vec![init::ExternalFeature::GameEventOrPoolSubstitution])
            }
            10 => {
                changed.input.external_features =
                    init::Rows::Complete(vec![init::ExternalFeature::ScriptOrAiSubstitution])
            }
            11 => {
                changed.input.external_features =
                    init::Rows::Complete(vec![init::ExternalFeature::Addon])
            }
            12 => spawn_row(&mut changed).equipment_id = -2,
            13 => item_rows(&mut changed).push(staff()),
            14 => item_rows(&mut changed)[0].inventory_type = 1,
            _ => {
                let duplicate = equipment_rows(&mut changed)[0];
                equipment_rows(&mut changed).push(duplicate);
            }
        }
        let mut p = good.clone();
        let before = p.clone();
        assert!(
            p.initialize_creature_generations(init::Rows::Complete(vec![changed]), vec![2])
                .is_err(),
            "unsupported case {case}"
        );
        assert_eq!(p, before);
    }
    let mut complete_empty = raw;
    complete_empty.input.template.equipment_template_id = 0;
    equipment_rows(&mut complete_empty).clear();
    item_rows(&mut complete_empty).clear();
    let w = world(equipped_profile(complete_empty.clone()), 7);
    assert_eq!(owner(&w, 2).result().equipment.mainhand, None);
    for equipment_missing in [false, true] {
        let mut missing = complete_empty.clone();
        if equipment_missing {
            missing.input.equipment_templates = init::Rows::Missing;
        } else {
            missing.input.item_templates = init::Rows::Missing;
        }
        assert_ne!(missing.identity(), complete_empty.identity());
        assert!(CreatureGenerationCatalog::validate_sources(
            init::Rows::Complete(vec![missing]),
            vec![2]
        )
        .is_err());
    }
}

#[test]
fn late_npc_failure_rolls_back_equipped_respawn_history_rng_and_journal() {
    let mut w = world(
        two_spawn_profile(
            equipped_source(2, 1, 3, false),
            equipped_source(7, 1, 3, false),
        ),
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
            assert_staff(owner(&w, 2).result());
            step(&mut w, &mut LateFailure::default());
            assert_eq!(w.state_digest(), probe.state_digest());
            assert_eq!(w.records(), probe.records());
            assert_staff(owner(&w, 2).result());
            return;
        }
        w = probe;
    }
    panic!("equipped creature never reached its real respawn transaction")
}

#[test]
fn aggregate_catalog_budget_counts_equipment_rows_before_constructing_owners() {
    let mut raw = equipped_source(2, 1, 1, false);
    raw.input.equipment_templates = init::Rows::Complete(
        (1..=4000)
            .map(|entry| init::EquipmentTemplateRow {
                entry,
                item_entries: if entry == 6 { [5276, 0, 0] } else { [0; 3] },
            })
            .collect(),
    );
    init::prepare(&raw.input).unwrap();
    let sources = (2..19)
        .map(|spawn| {
            let mut source = raw.clone();
            source.spawn_id = spawn;
            source
        })
        .collect();
    assert!(matches!(
        CreatureGenerationCatalog::validate_sources(
            init::Rows::Complete(sources),
            (2..19).collect()
        ),
        Err(NativeError::TooLarge)
    ));
}

#[test]
fn static_disarm_executes_through_the_selected_source_combat_host_and_replays() {
    let mut p = synthetic_combat_fixture::profile();
    // Bind all source components before opening the world.
    p.interactions = None;
    p.npcs[0].health_regeneration.unit_flags = DISARMED;
    p.npcs[0].ai.mainhand_interval_ms = 2000;
    let mut raw = complete_scene_sources(&p).remove(0);
    raw.input.template.extra_flags = 0;
    raw.input.template.unit_flags = DISARMED;
    raw.input.template.equipment_template_id = 6;
    raw.input.template.damage_variance = f::float(0.5);
    raw.input.equipment_templates = init::Rows::Complete(vec![init::EquipmentTemplateRow {
        entry: 6,
        item_entries: [5276, 0, 0],
    }]);
    raw.input.item_templates = init::Rows::Complete(vec![staff()]);
    let init::Rows::Complete(rows) = &mut raw.input.class_levels else {
        unreachable!()
    };
    rows[0].base_damage = f::float(12.0);
    rows[0].base_melee_attack_power = f::float(0.0);
    let mut p = install(p, vec![raw], vec![2]);
    synthetic_combat_fixture::install(&mut p);
    assert!(p.interactions.as_ref().unwrap().has_combat_authority());
    let mut w = world(p, 7);
    let start = w.checkpoint().unwrap();
    let health = w.state.health;
    assert_staff(owner(&w, 2).result());
    assert_eq!(owner(&w, 2).result().unit_flags, DISARMED);
    assert_eq!(owner(&w, 2).result().stats.damage.minimum, f::float(4.5));
    assert_eq!(owner(&w, 2).result().stats.damage.maximum, f::float(7.5));
    let mut authority = synthetic_combat_fixture::SpatialOnly::default();
    let mut hit = false;
    for _ in 0..200 {
        step(&mut w, &mut authority);
        for draw in &w.last_tick().unwrap().draws {
            if matches!(draw.purpose, DrawPurpose::MeleeDamage { .. }) {
                assert_eq!((draw.low, draw.high), (4, 7));
            }
        }
        if w.last_tick()
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(event, Event::Melee { effective, .. } if *effective > 0))
        {
            hit = true;
            break;
        }
    }
    assert!(
        hit,
        "static disarm must not suppress ordinary source-owned attacks"
    );
    assert!(w.state.health < health);
    assert!(authority.derived.iter().any(|reply| matches!(&reply.facts,
        Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcSemantics(facts)))
            if facts.can_initiate_attack && facts.can_attack_in_combat)));
    assert!(authority.derived.iter().any(|reply| matches!(&reply.facts,
        Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeDamage(facts)))
            if facts.player_can_attack_npc && facts.player_can_enter_combat && facts.npc_can_enter_combat)));
    assert!(authority.derived.iter().any(|reply| matches!(&reply.facts,
        Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeCombatEntry(facts)))
            if facts.player_can_enter_combat && facts.npc_can_enter_combat)));
    assert!(authority.queries.iter().all(|request| !matches!(
        request.query,
        Query::Npc { .. }
            | Query::NativeMelee { .. }
            | Query::SourceMotion(MotionQuery::Combat(
                CombatQuery::NpcSemantics(_)
                    | CombatQuery::MeleeDamage(_)
                    | CombatQuery::MeleeCombatEntry(_)
            ))
    )));
    let mut replay =
        NativeWorld::restore_synthetic_component_test(w.profile.clone(), start).unwrap();
    replay.replay(w.records()).unwrap();
    assert_eq!(replay.state_digest(), w.state_digest());
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    let checkpoint = w.checkpoint().unwrap();
    w.checkpoint_committed(&checkpoint).unwrap();
    let reopened = restore(&w);
    assert_eq!(reopened.state_digest(), w.state_digest());
    assert_staff(owner(&reopened, 2).result());
    assert_eq!(owner(&reopened, 2).result().unit_flags, DISARMED);
}
