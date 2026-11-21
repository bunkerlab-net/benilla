// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine one-tile resources exercise fresh grid creation and scoped ownership.
use super::super::super::super::{
    nav_generation::fresh_grid::{FreshSingleGrid, GridConfiguration, GridStage},
    source_height::preparation_counts::{self, Counts},
};
use super::*;
use benilla_sim::resident_map::compute_cell_pair;

fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn fresh_single_grid_owns_position_slot_cells_and_query_scope() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let entries = ridge_entries();
    let install = Install::new(&entries);
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

    // The actual pinned oracle's Detour start is YZX; use its source XY bits.
    let expected: serde_json::Value =
        serde_json::from_slice(&oracle("ridge_wmo/expected.json")).unwrap();
    let values: Vec<u32> = expected["output"]["paths"][0]["start_bits"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| u32::from_str_radix(v.as_str().unwrap(), 16).unwrap())
        .collect();
    let position = [values[2], values[0]];
    let cell = compute_cell_pair(position[0], position[1]).unwrap();
    assert_eq!(63 - cell.grid_x(), u16::from(TILE.x));
    assert_eq!(63 - cell.grid_y(), u16::from(TILE.y));
    let wrong_position = [position[1], position[0]];
    let wrong_cell = compute_cell_pair(wrong_position[0], wrong_position[1]).unwrap();
    assert_ne!(
        (wrong_cell.grid_x(), wrong_cell.grid_y()),
        (cell.grid_x(), cell.grid_y())
    );
    let p = crossing_profile();
    let configuration = GridConfiguration {
        expiry_ms: 123_456,
        unload_enabled: false,
        vmap: positive_tile_load_tests::ENABLED,
    };
    let empty_counts = Counts::default();
    let loaded_counts = Counts {
        height_preparations: 1,
        terrain_tiles: 1,
        collision_preparations: 1,
    };

    // No caller tile label or default config can authorize grid creation.
    for (at, config) in [
        (position, None),
        (wrong_position, Some(configuration)),
        ([f32::NAN.to_bits(), position[1]], Some(configuration)),
    ] {
        let mut stages = Vec::new();
        let (result, counts) = preparation_counts::measure(|| {
            FreshSingleGrid::from_generated_observed(
                &input,
                &output,
                &p,
                at,
                config,
                until(),
                || false,
                |stage| stages.push(stage),
            )
        });
        assert!(result.is_err());
        assert!(stages.is_empty());
        assert_eq!(counts, empty_counts);
    }
    let mut disabled = configuration;
    disabled.vmap.height_enabled = false;
    let (result, counts) = preparation_counts::measure(|| {
        FreshSingleGrid::from_generated(
            &input,
            &output,
            &p,
            position,
            Some(disabled),
            until(),
            || false,
        )
    });
    assert!(result.is_err());
    assert_eq!(counts, empty_counts);
    assert!(FreshSingleGrid::from_generated(
        &input,
        &output,
        &p,
        position,
        Some(configuration),
        Instant::now(),
        || false,
    )
    .is_err());
    assert!(FreshSingleGrid::from_generated(
        &input,
        &output,
        &p,
        position,
        Some(configuration),
        until(),
        || true,
    )
    .is_err());

    let ordered = [
        GridStage::Allocated,
        GridStage::Stored,
        GridStage::Linked,
        GridStage::Idle,
        GridStage::TerrainLoaded,
    ];
    // Every interrupted staged construction returns no usable owner. The last
    // case exercises cancellation after the actual terrain/query allocation.
    for (index, stop) in ordered.iter().copied().enumerate() {
        let cancelled = std::cell::Cell::new(false);
        let mut stages = Vec::new();
        let (result, counts) = preparation_counts::measure(|| {
            FreshSingleGrid::from_generated_observed(
                &input,
                &output,
                &p,
                position,
                Some(configuration),
                until(),
                || cancelled.get(),
                |stage| {
                    stages.push(stage);
                    if stage == stop {
                        cancelled.set(true);
                    }
                },
            )
        });
        assert!(result.is_err());
        assert_eq!(stages, ordered[..=index]);
        assert_eq!(
            counts,
            if stop == GridStage::TerrainLoaded {
                loaded_counts
            } else {
                empty_counts
            }
        );
    }

    let mut stages = Vec::new();
    let (result, counts) = preparation_counts::measure(|| {
        FreshSingleGrid::from_generated_observed(
            &input,
            &output,
            &p,
            position,
            Some(configuration),
            until(),
            || false,
            |stage| stages.push(stage),
        )
    });
    let grid = result.unwrap();
    let height_allocation = std::ptr::from_ref(grid.height());
    assert_eq!(stages, ordered);
    assert_eq!(counts, loaded_counts);
    assert_eq!(
        grid.grid_id(),
        u32::from(cell.grid_x()) * 64 + u32::from(cell.grid_y())
    );
    assert_eq!(grid.linked_count(), 1);
    assert_eq!(grid.cell_count(), 16 * 16);
    assert!(grid.cells_empty());
    assert!(!grid.object_data_loaded());
    assert_eq!(grid.active_locks(), 0);
    assert!(grid.explicit_lock());
    assert_eq!(grid.timer_ms(), configuration.expiry_ms);
    assert_eq!(grid.configuration(), configuration);
    assert_eq!(grid.vmap_configuration(), configuration.vmap);
    assert_eq!(grid.tile(), TILE);
    assert_eq!(grid.references(), 1);
    assert_eq!(grid.load_attempts(), 1);
    assert_eq!(grid.slot().map_id, p.map_id);
    assert_eq!(
        grid.slot().instance_id,
        p.pathfinder_configuration()
            .unwrap()
            .selected()
            .source_instance_id
    );

    let (warm, counts) = preparation_counts::measure(|| grid.ensure(position, until(), || false));
    assert_eq!(warm.unwrap(), cell);
    assert_eq!(std::ptr::from_ref(grid.height()), height_allocation);
    assert_eq!(counts, empty_counts);
    assert_eq!(grid.references(), 1);
    assert_eq!(grid.load_attempts(), 1);
    assert!(grid.ensure(wrong_position, until(), || false).is_err());
    assert!(grid.ensure(position, until(), || true).is_err());
    assert!(grid.ensure(position, Instant::now(), || false).is_err());
    let mut changed = ridge_entries();
    changed.iter_mut().find(|e| e.name == ADT).unwrap().bytes[0] ^= 1;
    install.write(&changed);
    assert!(grid.ensure(position, until(), || false).is_err());
    assert_eq!(grid.references(), 1);
    assert_eq!(grid.load_attempts(), 1);
    install.write(&entries);
    grid.ensure(position, until(), || false).unwrap();

    // The callback sees the exact still-owned node/link, height and allocation.
    // Its return value can retain a query, but is not a grid-registration token.
    let ((query, retained), counts) = preparation_counts::measure(|| {
        grid.with_query(|live, query| {
            assert_eq!(std::ptr::from_ref(live.height()), height_allocation);
            assert_eq!(live.linked_count(), 1);
            assert_eq!(live.cell_count(), 256);
            assert!(live.cells_empty());
            assert!(!live.object_data_loaded());
            assert_eq!(live.references(), 1);
            live.ensure(position, until(), || false).unwrap();
            live.height().verify_unchanged(|| false).unwrap();
            live.check_query(&query).unwrap();
            let retained = query.query().retain_default();
            retained.check_query(query.query()).unwrap();
            (query, retained)
        })
        .unwrap()
    });
    assert_eq!(counts, empty_counts);
    retained.check_query(query.query()).unwrap();

    let create = |config| {
        FreshSingleGrid::from_generated(
            &input,
            &output,
            &p,
            position,
            Some(config),
            until(),
            || false,
        )
        .unwrap()
    };
    let mut unload_enabled = configuration;
    unload_enabled.unload_enabled = true;
    let refused = create(unload_enabled)
        .with_query(|live, replacement| {
            assert!(!live.explicit_lock());
            live.check_query(&replacement).unwrap();
            assert!(live.check_query(&query).is_err());
            Err::<(), _>("consumer declined")
        })
        .unwrap();
    assert_eq!(refused, Err("consumer declined"));
    let panicked = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        create(configuration)
            .with_query::<()>(|live, query| {
                live.check_query(&query).unwrap();
                assert_eq!(live.linked_count(), 1);
                panic!("intentional consumer unwind");
            })
            .unwrap();
    }));
    assert!(panicked.is_err());
    // A separately constructed retry remains usable after either host callback
    // failure. This makes no claim about source unload or garbage collection.
    create(configuration)
        .with_query(|live, retry| {
            live.check_query(&retry).unwrap();
            assert!(retained.check_query(retry.query()).is_err());
            live.ensure(position, until(), || false).unwrap();
        })
        .unwrap();
    assert_eq!(install.bytes(), before);
}
