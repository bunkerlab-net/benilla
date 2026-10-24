// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored addon query closures on real synthetic-host generation transitions.
//! These tests establish neither public database completeness nor native admission.
use super::*;

fn addon(sheath_state: u32) -> init::Addon {
    init::Addon {
        mount: 0,
        stand_state: 0,
        sheath_state,
        emote: 0,
        move_flags: 0,
        auras: vec![],
    }
}

fn addon_source(
    spawn: u64,
    spawn_sheath: Option<u32>,
    template_sheath: Option<u32>,
) -> CreatureGenerationSource {
    let mut raw = source(spawn, 1, 3);
    raw.input.spawn_addons = init::Rows::Complete(
        spawn_sheath
            .into_iter()
            .map(|sheath| init::SpawnAddonRow {
                guid: spawn,
                addon: addon(sheath),
            })
            .collect(),
    );
    raw.input.template_addons = init::Rows::Complete(
        template_sheath
            .into_iter()
            .map(|sheath| init::TemplateAddonRow {
                entry: raw.input.template.entry,
                addon: addon(sheath),
            })
            .collect(),
    );
    raw
}

fn spawn_addons(raw: &mut CreatureGenerationSource) -> &mut Vec<init::SpawnAddonRow> {
    let init::Rows::Complete(rows) = &mut raw.input.spawn_addons else {
        panic!("authored complete GUID addon query")
    };
    rows
}

fn template_addons(raw: &mut CreatureGenerationSource) -> &mut Vec<init::TemplateAddonRow> {
    let init::Rows::Complete(rows) = &mut raw.input.template_addons else {
        panic!("authored complete template addon query")
    };
    rows
}

fn assert_sheath(value: &init::InitializedCreature, sheath: u32) {
    assert_eq!(value.unit_bytes_2, 0x1000 | sheath);
}

#[test]
fn addon_precedence_overwrites_only_sheath_without_an_extra_creation_or_lifecycle_draw() {
    for sheath in 0..=2 {
        let baseline_source = addon_source(2, None, None);
        let mut baseline = world(profile(baseline_source.clone()), 7);
        assert_sheath(owner(&baseline, 2).result(), 1);
        let initial = owner(&baseline, 2).result().clone();
        let initial_draws = owner(&baseline, 2).genesis_draws().to_vec();
        let initial_counter = baseline.state.rng_counter;
        kill(&mut baseline, &mut f::KernelAuthority::default());
        respawn(&mut baseline, &mut f::KernelAuthority::default());
        let baseline_draws = baseline
            .records()
            .iter()
            .flat_map(|record| &record.draws)
            .cloned()
            .collect::<Vec<_>>();
        for (spawn, template) in [
            (None, Some(sheath)),
            (Some(sheath), None),
            (Some(sheath), Some((sheath + 1) % 3)),
        ] {
            let raw = addon_source(2, spawn, template);
            assert_ne!(raw.identity(), baseline_source.identity());
            let mut w = world(profile(raw), 7);
            let mut expected = initial.clone();
            expected.unit_bytes_2 = 0x1000 | sheath;
            assert_eq!(owner(&w, 2).result(), &expected);
            assert_eq!(owner(&w, 2).genesis_draws(), initial_draws);
            assert_eq!(w.state.rng_counter, initial_counter);
            kill(&mut w, &mut f::KernelAuthority::default());
            respawn(&mut w, &mut f::KernelAuthority::default());
            assert_sheath(owner(&w, 2).result(), sheath);
            assert_eq!(w.state.tick, baseline.state.tick);
            assert_eq!(w.state.rng_counter, baseline.state.rng_counter);
            assert_eq!(
                w.records()
                    .iter()
                    .flat_map(|record| &record.draws)
                    .cloned()
                    .collect::<Vec<_>>(),
                baseline_draws
            );
            assert_eq!(
                owner(&w, 2).history()[1].creation_draws(),
                owner(&baseline, 2).history()[1].creation_draws()
            );
            assert_selection(&w, 2);
        }
    }
}

#[test]
fn every_addon_sheath_survives_death_respawn_historical_replay_and_checkpoint_compaction() {
    for sheath in 0..=2 {
        let p = profile(addon_source(2, Some(sheath), Some((sheath + 1) % 3)));
        let mut saw_distinct_level = false;
        for seed in 0..16 {
            let mut w = world(p.clone(), seed);
            let initial = w.checkpoint().unwrap();
            let old_id = w.state.npcs[&2].id;
            let old = owner(&w, 2).result().clone();
            assert_sheath(&old, sheath);
            let mut authority = f::KernelAuthority::default();
            kill(&mut w, &mut authority);
            assert_eq!(owner(&w, 2).result(), &old);
            assert_eq!(owner(&w, 2).current_death_tick(), Some(w.state.tick));
            let mut corpse_resume = restore(&w);
            respawn(&mut w, &mut authority);
            while corpse_resume.state.tick < w.state.tick {
                step(&mut corpse_resume, &mut authority);
            }
            assert_eq!(corpse_resume.state_digest(), w.state_digest());
            let current = owner(&w, 2).result().clone();
            assert_sheath(&current, sheath);
            assert_eq!(owner(&w, 2).history().len(), 2);
            assert_eq!(owner(&w, 2).historical(old_id).unwrap().result(), &old);
            assert_eq!(
                w.state.npcs[&2].historical_initialized_creature(old_id),
                Some(&old)
            );
            assert_eq!(w.state.npcs[&2].initialized_creature(), Some(&current));
            assert!(owner(&w, 2)
                .historical(old_id)
                .unwrap()
                .corpse_removed_tick()
                .is_some());
            assert_selection(&w, 2);
            let mut replay =
                NativeWorld::restore_synthetic_component_test(p.clone(), initial).unwrap();
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
            assert_eq!(reopened.profile_digest(), w.profile_digest());
            if current.level != old.level {
                assert_ne!(current.stats.damage, old.stats.damage);
                saw_distinct_level = true;
                break;
            }
        }
        assert!(
            saw_distinct_level,
            "sheath {sheath} retains distinct historical-life selections"
        );
    }
}

#[test]
fn equal_result_addon_source_substitution_cannot_reuse_stale_owner_or_replay_evidence() {
    let raw = addon_source(2, Some(1), Some(2));
    let mut w = world(profile(raw.clone()), 7);
    kill(&mut w, &mut f::KernelAuthority::default());
    respawn(&mut w, &mut f::KernelAuthority::default());
    let original = w.checkpoint().unwrap();
    for case in 0..5 {
        let mut changed = raw.clone();
        match case {
            0 => spawn_addons(&mut changed)[0].addon.move_flags = u32::MAX,
            1 => template_addons(&mut changed)[0].addon.move_flags = 0x100,
            2 => template_addons(&mut changed)[0].addon.sheath_state = 0,
            3 => template_addons(&mut changed).clear(),
            _ => {
                spawn_addons(&mut changed).clear();
                template_addons(&mut changed)[0].addon.sheath_state = 1;
            }
        }
        assert_ne!(changed.identity(), raw.identity());
        let p = profile(changed);
        assert_ne!(p.digest(), w.profile_digest());
        let mut fresh = world(p.clone(), 7);
        assert_eq!(
            owner(&fresh, 2).result(),
            owner(&world(profile(raw.clone()), 7), 2).result()
        );
        assert_eq!(
            owner(&fresh, 2).genesis_draws(),
            owner(&w, 2).genesis_draws()
        );
        let before = fresh.checkpoint().unwrap();
        assert!(
            fresh.replay(w.records()).is_err(),
            "source substitution case {case}"
        );
        assert_eq!(fresh.checkpoint().unwrap(), before);
        assert!(fresh.records().is_empty());
        assert!(
            NativeWorld::restore_synthetic_component_test(p.clone(), original.clone()).is_err()
        );
        let mut forged = serde_json::to_value(&original).unwrap();
        let digest = serde_json::to_value(p.digest()).unwrap();
        forged["state"]["profile"] = digest.clone();
        forged["state"]["initial_cast_genesis"]["profile"] = digest.clone();
        forged["state"]["npcs"]["2"]["source_generation"]["profile"] = digest;
        // Re-signing outer hashes does not replace the retained raw-source owner.
        assert!(
            NativeWorld::restore_synthetic_component_test(p, resign(forged)).is_err(),
            "rehashed source substitution case {case}"
        );
    }
}

#[test]
fn addon_historical_draw_and_result_injection_reject_after_respawn() {
    let mut w = world(profile(addon_source(2, Some(0), Some(2))), 7);
    kill(&mut w, &mut f::KernelAuthority::default());
    respawn(&mut w, &mut f::KernelAuthority::default());
    let base = serde_json::to_value(w.checkpoint().unwrap()).unwrap();
    for life in 0..=1 {
        let mut changed = base.clone();
        let draw = &mut changed["state"]["npcs"]["2"]["source_generation"]["generations"][life]
            ["draws"][0];
        draw["consumed"] = (draw["consumed"].as_u64().unwrap() + 1).into();
        rejects(&w, changed);
        let mut injected = base.clone();
        injected["state"]["npcs"]["2"]["source_generation"]["generations"][life]["unit_bytes_2"] =
            0x1002.into();
        assert!(
            serde_json::from_value::<Checkpoint>(injected).is_err(),
            "historical result fields cannot replace source ownership"
        );
    }
    let mut missing_source = base;
    missing_source["state"]["npcs"]["2"]["source_generation"]["source"] =
        serde_json::to_value([0_u8; 32]).unwrap();
    rejects(&w, missing_source);
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
fn guid_addons_are_spawn_owned_while_template_query_closure_is_shared() {
    let first = addon_source(2, Some(0), Some(2));
    let second = addon_source(7, Some(1), Some(2));
    let good = two_spawn_profile(first.clone(), second.clone());
    let mut w = world(good.clone(), 7);
    assert_sheath(owner(&w, 2).result(), 0);
    assert_sheath(owner(&w, 7).result(), 1);
    assert_ne!(first.identity(), second.identity());
    kill(&mut w, &mut f::KernelAuthority::default());
    respawn(&mut w, &mut f::KernelAuthority::default());
    assert_eq!(w.state.npcs[&2].id.generation, 1);
    assert_eq!(w.state.npcs[&7].id.generation, 0);
    assert_sheath(owner(&w, 2).result(), 0);
    assert_sheath(owner(&w, 7).result(), 1);
    assert_eq!(restore(&w).state_digest(), w.state_digest());
    for case in 0..5 {
        let mut conflicting = second.clone();
        match case {
            0 => template_addons(&mut conflicting)[0].addon.sheath_state = 0,
            1 => template_addons(&mut conflicting)[0].addon.move_flags = 1,
            2 => template_addons(&mut conflicting).clear(),
            3 => spawn_addons(&mut conflicting)[0].guid = 2,
            _ => {
                conflicting.input.spawn_id = 2;
                spawn_addons(&mut conflicting)[0].guid = 2;
                init::prepare(&conflicting.input).unwrap();
            }
        }
        let mut p = good.clone();
        let before = p.clone();
        assert!(
            p.initialize_creature_generations(
                init::Rows::Complete(vec![first.clone(), conflicting]),
                vec![2, 7]
            )
            .is_err(),
            "ownership case {case}"
        );
        assert_eq!(p, before);
    }
    let mut absent_first = first;
    template_addons(&mut absent_first).clear();
    assert!(CreatureGenerationCatalog::validate_sources(
        init::Rows::Complete(vec![absent_first, second]),
        vec![2, 7]
    )
    .is_err());
}

#[test]
fn incomplete_ambiguous_or_unsupported_addons_reject_catalog_replacement_atomically() {
    let raw = addon_source(2, Some(0), Some(2));
    let good = profile(raw.clone());
    for case in 0..18 {
        let mut changed = raw.clone();
        match case {
            0 => changed.input.spawn_addons = init::Rows::Missing,
            1 => changed.input.template_addons = init::Rows::Missing,
            2 => {
                let row = spawn_addons(&mut changed)[0].clone();
                spawn_addons(&mut changed).push(row);
            }
            3 => {
                let row = template_addons(&mut changed)[0].clone();
                template_addons(&mut changed).push(row);
            }
            4 => spawn_addons(&mut changed)[0].guid = 7,
            5 => template_addons(&mut changed)[0].entry += 1,
            6 => changed.input.spawn_id = 7,
            7 => spawn_addons(&mut changed)[0].addon.mount = 1,
            8 => spawn_addons(&mut changed)[0].addon.stand_state = 1,
            9 => spawn_addons(&mut changed)[0].addon.emote = 1,
            10 => spawn_addons(&mut changed)[0].addon.auras = vec![1],
            11 => spawn_addons(&mut changed)[0].addon.sheath_state = 3,
            12 => template_addons(&mut changed)[0].addon.mount = 1,
            13 => template_addons(&mut changed)[0].addon.stand_state = 1,
            14 => template_addons(&mut changed)[0].addon.emote = 1,
            15 => template_addons(&mut changed)[0].addon.auras = vec![1],
            16 => template_addons(&mut changed)[0].addon.sheath_state = 3,
            _ => {
                changed.input.external_features =
                    init::Rows::Complete(vec![init::ExternalFeature::Addon])
            }
        }
        let mut p = good.clone();
        let before = p.clone();
        assert!(
            p.initialize_creature_generations(init::Rows::Complete(vec![changed]), vec![2])
                .is_err(),
            "addon validation case {case}"
        );
        assert_eq!(p, before);
    }
    let empty = addon_source(2, None, None);
    assert_sheath(owner(&world(profile(empty.clone()), 7), 2).result(), 1);
    for missing_spawn in [false, true] {
        let mut missing = empty.clone();
        if missing_spawn {
            missing.input.spawn_addons = init::Rows::Missing;
        } else {
            missing.input.template_addons = init::Rows::Missing;
        }
        assert_ne!(missing.identity(), empty.identity());
        assert!(CreatureGenerationCatalog::validate_sources(
            init::Rows::Complete(vec![missing]),
            vec![2]
        )
        .is_err());
    }
}

#[test]
fn addon_sheath_is_stable_through_real_engage_evade_home_and_replay() {
    use benilla_sim::creature_motion as motion;
    for sheath in 0..=2 {
        let mut p = synthetic_motion_fixture::profile();
        p.npcs[0].ai.hard_leash_radius = Some(f::real(5.0));
        let mut sources = complete_scene_sources(&p);
        sources[0].input.spawn_addons = init::Rows::Complete(vec![init::SpawnAddonRow {
            guid: 2,
            addon: addon(sheath),
        }]);
        sources[0].input.template_addons = init::Rows::Complete(vec![init::TemplateAddonRow {
            entry: sources[0].spawn_entry,
            addon: addon((sheath + 1) % 3),
        }]);
        p.source_motion
            .as_mut()
            .unwrap()
            .npcs
            .get_mut(&2)
            .unwrap()
            .npc_policy
            .home
            .addon_closure = HomeAddonClosure::GenerationSource {
            source_identity: sources[0].identity(),
        };
        let mut w = world(install(p, sources, vec![2]), 7);
        let initial = w.checkpoint().unwrap();
        let id = w.state.npcs[&2].id;
        let selected = owner(&w, 2).result().clone();
        let history = owner(&w, 2).history().to_vec();
        assert_sheath(&selected, sheath);
        let mut authority = synthetic_motion_fixture::SyntheticMotionAuthority::default();
        let mut engaged = false;
        let mut evaded = false;
        let mut returned = false;
        for _ in 0..100 {
            step(&mut w, &mut authority);
            assert_eq!(w.state.npcs[&2].id, id);
            assert_eq!(w.state.npcs[&2].initialized_creature(), Some(&selected));
            assert_eq!(owner(&w, 2).history(), history);
            assert_eq!(owner(&w, 2).historical(id).unwrap().result(), &selected);
            for event in &w.last_tick().unwrap().events {
                match event {
                    Event::Npc {
                        id: npc,
                        effect: ai::Effect::CombatStarted { .. },
                    } if *npc == id => engaged = true,
                    Event::Npc {
                        id: npc,
                        effect: ai::Effect::EvadeStarted(_),
                    } if *npc == id => {
                        assert!(engaged);
                        evaded = true;
                    }
                    Event::NpcMotion {
                        event: motion::Event::HomeReached { .. },
                        ..
                    } => {
                        assert!(engaged && evaded);
                        returned = true;
                    }
                    _ => {}
                }
            }
            if returned {
                break;
            }
        }
        assert!(
            engaged && evaded && returned,
            "sheath {sheath} must execute combat, evade and home callbacks"
        );
        assert_eq!(restore(&w).state_digest(), w.state_digest());
        let mut replay =
            NativeWorld::restore_synthetic_component_test(w.profile.clone(), initial).unwrap();
        replay.replay(w.records()).unwrap();
        assert_eq!(replay.state_digest(), w.state_digest());
        assert_eq!(owner(&replay, 2).history(), history);
    }
}

#[test]
fn independent_capability_no_addon_closure_rejects_addon_owners_in_both_installation_orders() {
    let p = synthetic_motion_fixture::profile();
    let empty_sources = complete_scene_sources(&p);
    let mut base = install(p, empty_sources.clone(), vec![2]);
    base.creature_generations = None;
    for capabilities_first in [false, true] {
        let mut empty = base.clone();
        if capabilities_first {
            empty
                .initialize_creature_capabilities(
                    synthetic_motion_fixture::creature_capability_sources(&empty),
                )
                .unwrap();
        }
        empty
            .initialize_creature_generations(init::Rows::Complete(empty_sources.clone()), vec![2])
            .unwrap();
        if !capabilities_first {
            empty
                .initialize_creature_capabilities(
                    synthetic_motion_fixture::creature_capability_sources(&empty),
                )
                .unwrap();
        }
        let w = world(empty, 7);
        assert_sheath(owner(&w, 2).result(), 1);
        assert_eq!(restore(&w).state_digest(), w.state_digest());
        for (spawn, template) in [(Some(0), None), (None, Some(1)), (Some(0), Some(2))] {
            let mut sources = empty_sources.clone();
            if let Some(sheath) = spawn {
                sources[0].input.spawn_addons = init::Rows::Complete(vec![init::SpawnAddonRow {
                    guid: 2,
                    addon: addon(sheath),
                }]);
            }
            if let Some(sheath) = template {
                sources[0].input.template_addons =
                    init::Rows::Complete(vec![init::TemplateAddonRow {
                        entry: sources[0].spawn_entry,
                        addon: addon(sheath),
                    }]);
            }
            let mut p = base.clone();
            let addon_identity = sources[0].identity();
            if capabilities_first {
                p.initialize_creature_capabilities(
                    synthetic_motion_fixture::creature_capability_sources(&p),
                )
                .unwrap();
                p.source_motion
                    .as_mut()
                    .unwrap()
                    .npcs
                    .get_mut(&2)
                    .unwrap()
                    .npc_policy
                    .home
                    .addon_closure = HomeAddonClosure::GenerationSource {
                    source_identity: addon_identity,
                };
                let before = p.clone();
                assert!(p
                    .initialize_creature_generations(init::Rows::Complete(sources), vec![2])
                    .is_err());
                assert_eq!(p, before);
            } else {
                p.source_motion
                    .as_mut()
                    .unwrap()
                    .npcs
                    .get_mut(&2)
                    .unwrap()
                    .npc_policy
                    .home
                    .addon_closure = HomeAddonClosure::GenerationSource {
                    source_identity: addon_identity,
                };
                p.initialize_creature_generations(init::Rows::Complete(sources), vec![2])
                    .unwrap();
                let before = p.clone();
                assert!(p
                    .initialize_creature_capabilities(
                        synthetic_motion_fixture::creature_capability_sources(&p)
                    )
                    .is_err());
                assert_eq!(p, before);
            }
        }
    }
}
