// SPDX-License-Identifier: GPL-3.0-or-later
//! Core lifecycle boundaries; the native host integration has separate tests.
use super::*;

fn source() -> CreatureGenerationSource {
    CreatureGenerationSource {
        spawn_id: 2,
        spawn_entry: 60001,
        requested_spawn_template_entry: 0,
        respawn_delay_min_seconds: 0,
        respawn_delay_max_seconds: 0,
        input: synthetic_creature_model_fixture::input(),
    }
}

#[test]
fn zero_db_delay_draws_and_same_tick_corpse_removal_remain_distinct() {
    let mut source = source();
    if let init::Rows::Complete(spawns) = &mut source.input.spawn_template {
        spawns[0].model_id = 31;
    }
    if let init::Rows::Complete(models) = &mut source.input.models {
        for model in models {
            model.other_gender_id = 0;
        }
    }
    let catalog =
        CreatureGenerationCatalog::validate_sources(init::Rows::Complete(vec![source]), vec![2])
            .unwrap();
    let world = [7; 16];
    let profile = [8; 32];
    let first = crate::NpcId {
        spawn: 2,
        generation: 0,
    };
    let mut cursor = 17;
    let (owner, draws) = catalog
        .initial(world, profile, first, 42, &mut cursor)
        .unwrap();
    assert_eq!(draws.len(), 1);
    assert_eq!(
        draws[0].purpose,
        DrawPurpose::CreatureInitialRespawnDelay { npc: first }
    );
    assert_eq!((draws[0].low, draws[0].high, draws[0].value), (0, 0, 0));
    assert_eq!(cursor, 18);
    assert_eq!(owner.corpse_delay_seconds(), 0);

    let (dead, death_draws) = owner
        .redraw_death_delay(&catalog, 1, 42, &mut cursor)
        .unwrap();
    assert_eq!(death_draws.len(), 1);
    assert_eq!(
        death_draws[0].purpose,
        DrawPurpose::CreatureDeathRespawnDelay { npc: first }
    );
    assert_eq!(cursor, 19);
    let mut base = synthetic_fixture::profile().npcs.remove(0);
    base.ai.creature_id = 2;
    base.ai.clock_origin_ms = 0;
    base.faction_template = 17;
    let mut ai =
        ai::SoloCreatureAi::new_generation_owned(owner.project_profile(&base).unwrap().ai).unwrap();
    let observation = ai::Observation {
        sequence: 1,
        elapsed_ms: 50,
        geometry_revision: ai::movement::Revision([9; 32]),
        target: None,
        can_initiate_attack: true,
        casting_non_melee: false,
        stationary_melee_refresh_eligible: false,
        creature_moving_and_running: false,
        movement: None,
        receipts: vec![ai::Receipt {
            id: 1,
            event: ai::CombatEvent::Died,
        }],
    };
    let token = ai
        .begin_source_receipts_in_phase_with_death_timing(
            &observation,
            false,
            Some(ai::DeathTiming {
                receipt_id: 1,
                at_ms: 50,
                respawn_delay_seconds: 0,
            }),
        )
        .unwrap();
    let effects = ai
        .finish_receipts(observation, token, &mut |_, _, _| {
            Ok(ai::MainhandResult::Unchanged)
        })
        .unwrap();
    assert!(effects.contains(&ai::Effect::CorpseRemoved));
    let removed = dead.record_corpse_removed(1, &ai).unwrap();
    assert!(removed.record_corpse_removed(1, &ai).is_err());
    let next = crate::NpcId {
        spawn: 2,
        generation: 1,
    };
    let before = cursor;
    assert!(removed.respawn(&catalog, next, 1, 42, &mut cursor).is_err());
    assert_eq!(cursor, before);
    let (revived, draws) = removed.respawn(&catalog, next, 2, 42, &mut cursor).unwrap();
    assert!(draws.is_empty());
    assert_eq!(cursor, before);
    assert_eq!(revived.corpse_delay_seconds(), 83);
    assert_eq!(revived.historical(first).unwrap().death_tick(), Some(1));
    assert_eq!(
        revived.historical(first).unwrap().corpse_removed_tick(),
        Some(1)
    );
    let restored = CreatureGenerationOwner::restore(
        &catalog,
        revived.checkpoint(),
        world,
        profile,
        next,
        42,
        2,
        cursor,
        50,
        0,
        &base,
    )
    .unwrap();
    assert_eq!(restored, revived);

    let mut missing_removal = revived.checkpoint();
    missing_removal.generations[0].corpse_removed_tick = None;
    assert!(CreatureGenerationOwner::restore(
        &catalog,
        missing_removal,
        world,
        profile,
        next,
        42,
        2,
        cursor,
        50,
        0,
        &base,
    )
    .is_err());
}

#[test]
fn zero_probability_tail_is_retained_without_requesting_unreachable_bucket() {
    let mut source = source();
    source.input.template.displays[1] = init::DisplayChoice {
        id: 34,
        probability: 0,
    };
    let catalog = CreatureGenerationCatalog::validate_sources(
        init::Rows::Complete(vec![source.clone()]),
        vec![2],
    )
    .unwrap();
    assert_eq!(catalog.source(2), Some(&source));
}

#[test]
fn every_level_must_prepare_before_catalog_can_supply_randomness() {
    let mut source = source();
    source.input.template.maximum_level = 2;
    assert!(CreatureGenerationCatalog::validate_sources(
        init::Rows::Complete(vec![source]),
        vec![2],
    )
    .is_err());
}
