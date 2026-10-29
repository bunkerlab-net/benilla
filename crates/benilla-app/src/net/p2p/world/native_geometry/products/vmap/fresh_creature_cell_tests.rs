// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual cold constructor objects in real component cells, not live registration.
use super::super::super::super::{
    nav_generation::fresh_grid::{FreshSingleGrid, GridConfiguration},
    source_height::preparation_counts::{self, Counts},
};
use super::*;
use benilla_sim::{creature_ai as ai, resident_map::compute_cell_pair};
use benilla_world_runtime::native::{
    synthetic_combat_fixture as combat, synthetic_creature_model_fixture as models,
    synthetic_fixture as f, synthetic_map_classification_fixture as maps,
    synthetic_motion_fixture as motion, synthetic_two_owner_fixture as two_owner,
    CreatureCapabilityCatalog, EffectiveDisabledMMapSet, NpcCombatAuthoritySource, Profile,
    SelectedMMapConfiguration, SourceVisitorCatalog,
};

fn until() -> Instant {
    Instant::now() + Duration::from_secs(20)
}

// Author two database candidates before any source-owner/catalog is sealed.
// The second can deliberately be outside this isolated single-grid domain.
fn creature_profile(start: [f32; 3], second: [f32; 3]) -> Profile {
    let mut p = f::initialized_profile();
    p.map_id = 0;
    p.npcs[0].ai.map_id = 0;
    p.npcs[0].ai.spawn = ai::Point::new(start[0], start[1], start[2]).unwrap();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.npcs[0].faction_template = 2;
    p.player.start = ai::Point::new(start[0], start[1] + 8.0, start[2]).unwrap();
    p.source_motion = Some(motion::genesis(&p, 2));
    let mut other = p.npcs[0].clone();
    other.ai.creature_id = 3;
    other.entry += 1;
    other.ai.spawn = ai::Point::new(second[0], second[1], second[2]).unwrap();
    let motion = p.source_motion.as_mut().unwrap();
    motion.npcs.insert(3, motion.npcs[&2].clone());
    p.npcs.push(other);
    // Match the existing two-owner fixture's dependency staging. Multi-owner
    // motion requires an explicitly selected complete authored visitor scene.
    let source_motion = p.source_motion.take();
    let mut interactions = combat::source_rows(&p);
    // complete_scene_sources authors the second template with NPC flag 2.
    interactions
        .npcs
        .iter_mut()
        .find(|row| row.entry == p.npcs[1].entry)
        .unwrap()
        .npc_flags = 2;
    p.initialize_interactions(interactions).unwrap();
    p.source_motion = source_motion;
    p.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(motion::creature_capability_sources(&p))
            .unwrap(),
    );
    p.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(two_owner::visitor_input(&p, vec![2, 3])).unwrap(),
    );
    p.initialize_combat_authority(
        p.npcs
            .iter()
            .map(|npc| NpcCombatAuthoritySource {
                entry: npc.entry,
                extra_flags: 0,
                static_flags1: 0,
                static_flags2: 0,
                static_flags3: 0,
                static_flags4: 0,
            })
            .collect(),
    )
    .unwrap();
    p = models::with_complete_scene_generations(p);
    let catalog = maps::catalog(p.map_id, 0);
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled: true,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        &catalog,
    )
    .unwrap();
    p.initialize_map_classification(&catalog).unwrap();
    p.validate().unwrap();
    p
}

#[test]
fn fresh_creature_cells_retain_actual_constructor_objects_and_rollback() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let install = Install::new(&ridge_entries());
    let before = install.bytes();
    let terrain = benilla_native_terrain::prepare(
        &install.0,
        0,
        &[benilla_native_terrain::Tile { x: 31, y: 30 }],
    )
    .unwrap();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let input = PreparedNavigationInput::from_source(&terrain, &bundle, TILE, || false).unwrap();
    let output = input
        .generate(&helper(), Instant::now() + Duration::from_secs(60), || {
            false
        })
        .unwrap();
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let bits: Vec<u32> = expected["output"]["paths"][0]["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u32::from_str_radix(v.as_str().unwrap(), 16).unwrap())
        .collect();
    let start = [
        f32::from_bits(bits[2]),
        f32::from_bits(bits[0]),
        f32::from_bits(bits[1]),
    ];
    let position = [bits[2], bits[0]];
    let second = [start[0] + 2.0, start[1], start[2]];
    let cell = compute_cell_pair(position[0], position[1]).unwrap();
    assert_eq!(
        cell,
        compute_cell_pair(second[0].to_bits(), second[1].to_bits()).unwrap()
    );
    let p = creature_profile(start, second);
    assert_eq!(
        p.creature_generations.as_ref().unwrap().cold_entry_order(),
        &[2, 3]
    );
    let config = GridConfiguration {
        expiry_ms: 123_456,
        unload_enabled: false,
        vmap: positive_tile_load_tests::ENABLED,
    };
    let make_grid = |p: &Profile| {
        FreshSingleGrid::from_generated(&input, &output, p, position, Some(config), until(), || {
            false
        })
        .unwrap()
    };
    let world = [7; 16];
    let mut authority = f::SyntheticAuthority::default();

    // Successful scope retains the exact grid node, height, query and two fresh
    // generation-zero objects. Insert-front order is source GridReference order.
    let grid = make_grid(&p);
    let height = std::ptr::from_ref(grid.height());
    let (result, counts) = preparation_counts::measure(|| {
        grid.with_initial_creatures(
            p.clone(),
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |grid, native, query| {
                assert_eq!(std::ptr::from_ref(grid.height()), height);
                grid.check_query(&query).unwrap();
                assert_eq!(grid.initial_creature_count(), 2);
                assert!(!grid.cells_empty());
                assert!(!grid.object_data_loaded());
                assert_eq!(grid.linked_count(), 1);
                assert_eq!(grid.cell_count(), 256);
                assert_eq!(grid.references(), 1);
                assert_eq!(
                    grid.initial_creatures()
                        .map(|a| a.object().npc().spawn)
                        .collect::<Vec<_>>(),
                    vec![3, 2]
                );
                for attached in grid.initial_creatures() {
                    let object = attached.object();
                    assert_eq!(object.world(), world);
                    assert_eq!(object.profile_identity(), p.digest());
                    assert_eq!(
                        object.source_identity(),
                        object.generation().source_identity()
                    );
                    assert_eq!(object.npc().generation, 0);
                    assert_eq!(object.generation().history().len(), 1);
                    assert_eq!(object.cell(), cell);
                    assert_eq!(attached.current_cell(), cell);
                    assert_eq!(grid.initial_cell_for(object.npc()), Some(cell));
                    assert_eq!(object.map_id(), 0);
                    assert_eq!(object.instance_id(), 0);
                    assert!(!object.is_active());
                    assert_eq!(object.viewer_count(), 0);
                    assert_eq!(object.viewpoint_grid(), None);
                    assert_eq!(native.state().npcs[&object.npc().spawn].id, object.npc());
                    // Source AddToWorld registration-prefix candidates retain
                    // this exact allocation, not copies or live GetUnit results.
                    let identity = object.identity();
                    let typed = grid.initial_guid_candidate(identity.guid()).unwrap();
                    assert!(std::ptr::eq(typed, attached));
                    let database: Vec<_> = grid
                        .initial_database_candidates(0xf130, identity.database_guid())
                        .collect();
                    assert_eq!(database.len(), 1);
                    assert!(std::ptr::eq(database[0], attached));
                    // Same low database GUID with a different type is not a hit.
                    assert!(grid
                        .initial_database_candidates(0, identity.database_guid())
                        .next()
                        .is_none());
                    assert!(grid
                        .initial_guid_candidate(identity.guid() ^ (1_u64 << 48))
                        .is_none());
                }
                assert!(grid.initial_guid_candidate(0).is_none());
                assert!(grid.initial_database_candidates(0xf130, 0).next().is_none());
                // Return only ordinary runtime data; no grid or CurrentCell certificate.
                native.checkpoint().unwrap()
            },
        )
    });
    let checkpoint = result.unwrap();
    assert_eq!(counts, Counts::default());

    // All refusal paths consume their grid and never call the final consumer.
    let consumed = std::cell::Cell::new(false);
    let mut mismatch = p.clone();
    mismatch.geometry_revision[0] ^= 1;
    assert!(make_grid(&p)
        .with_initial_creatures(
            mismatch,
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |_, _, _| consumed.set(true)
        )
        .is_err());
    assert!(make_grid(&p)
        .with_initial_creatures(
            p.clone(),
            [0; 16],
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |_, _, _| consumed.set(true)
        )
        .is_err());
    assert!(make_grid(&p)
        .with_initial_creatures(
            p.clone(),
            world,
            f::player(),
            42,
            &mut authority,
            Instant::now(),
            || false,
            |_, _, _| consumed.set(true)
        )
        .is_err());
    assert!(make_grid(&p)
        .with_initial_creatures(
            p.clone(),
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || true,
            |_, _, _| consumed.set(true)
        )
        .is_err());

    let stale = make_grid(&p);
    let mut changed = ridge_entries();
    changed.iter_mut().find(|e| e.name == ADT).unwrap().bytes[0] ^= 1;
    install.write(&changed);
    assert!(stale
        .with_initial_creatures(
            p.clone(),
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |_, _, _| consumed.set(true)
        )
        .is_err());
    install.write(&ridge_entries());

    // First source constructor is in-grid, second is genuinely outside it.
    // Neither caller-supplied IDs nor pose snapshots can mint these objects.
    let outside = [start[1], start[0], start[2]];
    let outside_cell = compute_cell_pair(outside[0].to_bits(), outside[1].to_bits()).unwrap();
    assert_ne!(
        (cell.grid_x(), cell.grid_y()),
        (outside_cell.grid_x(), outside_cell.grid_y())
    );
    let wrong_grid = creature_profile(start, outside);
    assert_eq!(
        wrong_grid
            .creature_generations
            .as_ref()
            .unwrap()
            .cold_entry_order(),
        &[2, 3]
    );
    assert!(make_grid(&wrong_grid)
        .with_initial_creatures(
            wrong_grid,
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |_, _, _| consumed.set(true)
        )
        .is_err());
    assert!(!consumed.get());

    // A consumer panic unwinds the entire already-populated grid, too. No
    // catch-and-reuse operation can expose either staged constructor object.
    let panic_grid = make_grid(&p);
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = panic_grid.with_initial_creatures::<()>(
            p.clone(),
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |grid, _, query| {
                assert_eq!(grid.initial_creature_count(), 2);
                grid.check_query(&query).unwrap();
                panic!("intentional initial-cell consumer unwind");
            },
        );
    }));
    assert!(panicked.is_err());

    // A separate complete construction remains deterministic after failed scopes.
    let retry = make_grid(&p)
        .with_initial_creatures(
            p,
            world,
            f::player(),
            42,
            &mut authority,
            until(),
            || false,
            |grid, native, query| {
                assert_eq!(grid.initial_creature_count(), 2);
                grid.check_query(&query).unwrap();
                native.checkpoint().unwrap()
            },
        )
        .unwrap();
    assert_eq!(retry, checkpoint);
    assert_eq!(install.bytes(), before);
}
