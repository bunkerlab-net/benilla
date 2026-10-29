// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual prepared resources exercise the isolated positive Load-stage owner.
use super::super::super::super::{
    nav_generation::{
        positive_load::{LoadStage, PositiveTileLoad, VmapLoadConfiguration},
        GeneratedNavigation,
    },
    source_height::preparation_counts::{self, Counts},
};
use super::*;

pub(super) const ENABLED: VmapLoadConfiguration = VmapLoadConfiguration {
    line_of_sight_enabled: true,
    height_enabled: true,
};

fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn positive_tile_load_owns_cold_allocations_and_warm_references() {
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
    let p = crossing_profile();

    // Unknown and unsupported settings are refused before decoding any owner.
    for configuration in [
        None,
        Some(VmapLoadConfiguration {
            line_of_sight_enabled: false,
            height_enabled: false,
        }),
        Some(VmapLoadConfiguration {
            line_of_sight_enabled: false,
            height_enabled: true,
        }),
        Some(VmapLoadConfiguration {
            line_of_sight_enabled: true,
            height_enabled: false,
        }),
    ] {
        let (result, counts) = preparation_counts::measure(|| {
            PositiveTileLoad::from_generated(&input, &output, &p, configuration, until(), || false)
        });
        assert!(result.is_err());
        assert_eq!(counts, Counts::default());
    }
    let (result, counts) = preparation_counts::measure(|| {
        PositiveTileLoad::from_generated(&input, &output, &p, Some(ENABLED), until(), || true)
    });
    assert!(result.is_err());
    assert_eq!(counts, Counts::default());
    let (result, counts) = preparation_counts::measure(|| {
        PositiveTileLoad::from_generated(&input, &output, &p, Some(ENABLED), Instant::now(), || {
            false
        })
    });
    assert!(result.is_err());
    assert_eq!(counts, Counts::default());

    // Cancellation after actual allocations publishes no owner or partial state.
    // Each fresh retry below starts its own isolated allocation stage.
    for stop in [LoadStage::TerrainAndVmapLoaded, LoadStage::MmapLoaded] {
        let cancelled = std::cell::Cell::new(false);
        let mut observed = Vec::new();
        let (result, counts) = preparation_counts::measure(|| {
            PositiveTileLoad::from_generated_observed(
                &input,
                &output,
                &p,
                Some(ENABLED),
                until(),
                || cancelled.get(),
                |stage| {
                    observed.push(stage);
                    if stage == stop {
                        cancelled.set(true);
                    }
                },
            )
        });
        assert!(result.is_err());
        assert_eq!(observed.first(), Some(&LoadStage::Referenced));
        assert_eq!(observed.last(), Some(&stop));
        assert!(!observed.contains(&LoadStage::FullyLoaded));
        assert!(!observed.contains(&LoadStage::Attempted));
        assert_eq!(
            counts,
            Counts {
                height_preparations: 1,
                terrain_tiles: 1,
                collision_preparations: 1,
            }
        );
    }

    // The measured calls are this owner's allocations, not generation's earlier
    // independent temporary decodes/native validation.
    let mut observed = Vec::new();
    let (result, counts) = preparation_counts::measure(|| {
        PositiveTileLoad::from_generated_observed(
            &input,
            &output,
            &p,
            Some(ENABLED),
            until(),
            || false,
            |stage| observed.push(stage),
        )
    });
    assert_eq!(
        observed,
        [
            LoadStage::Referenced,
            LoadStage::TerrainAndVmapLoaded,
            LoadStage::MmapLoaded,
            LoadStage::FullyLoaded,
            LoadStage::Attempted,
        ]
    );
    let mut owner = result.unwrap();
    assert_eq!(
        counts,
        Counts {
            height_preparations: 1,
            terrain_tiles: 1,
            collision_preparations: 1,
        }
    );
    assert_eq!(owner.references(), 1);
    assert_eq!(owner.load_attempts(), 1);
    assert_eq!(owner.configuration(), ENABLED);
    assert_eq!(owner.tile(), TILE);
    assert_eq!(owner.height().binding().map, p.map_id);
    let allocation = owner.query().query().retain_default();
    let binding = owner.query().binding();
    let (warm, counts) =
        preparation_counts::measure(|| owner.acquire(Some(ENABLED), until(), || false));
    warm.unwrap();
    assert_eq!(counts, Counts::default());
    assert_eq!(owner.references(), 2);
    assert_eq!(owner.load_attempts(), 1);
    assert_eq!(owner.query().binding(), binding);
    allocation.check_query(owner.query().query()).unwrap();

    // Rejected host acquisitions do not alter the completed source-stage owner.
    assert!(owner.acquire(None, until(), || false).is_err());
    assert!(owner.acquire(Some(ENABLED), until(), || true).is_err());
    assert!(owner
        .acquire(Some(ENABLED), Instant::now(), || false)
        .is_err());
    let mut changed = ridge_entries();
    changed.iter_mut().find(|e| e.name == ADT).unwrap().bytes[0] ^= 1;
    install.write(&changed);
    assert!(owner.acquire(Some(ENABLED), until(), || false).is_err());
    assert_eq!(owner.references(), 2);
    assert_eq!(owner.load_attempts(), 1);
    allocation.check_query(owner.query().query()).unwrap();
    install.write(&entries);
    owner.acquire(Some(ENABLED), until(), || false).unwrap();
    assert_eq!(owner.references(), 3);
    let (height, query) = owner.into_parts();
    allocation.check_query(query.query()).unwrap();
    height.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}

pub(super) fn assert_inventory_refusals(
    selected: &PreparedNavigationInput<'_, '_, '_>,
    selected_output: &GeneratedNavigation,
    expanded: &PreparedNavigationInput<'_, '_, '_>,
    expanded_output: &GeneratedNavigation,
    skipped_input: &PreparedNavigationInput<'_, '_, '_>,
    skipped_output: &GeneratedNavigation,
    profile: &benilla_world_runtime::native::Profile,
) {
    // One generated tile cannot certify the expanded two-tile terrain inventory.
    assert!(PositiveTileLoad::from_generated(
        expanded,
        expanded_output,
        profile,
        Some(ENABLED),
        until(),
        || false,
    )
    .is_err());
    // Equal tile labels do not make an output from another prepared input valid.
    assert!(PositiveTileLoad::from_generated(
        selected,
        expanded_output,
        profile,
        Some(ENABLED),
        until(),
        || false,
    )
    .is_err());
    assert!(PositiveTileLoad::from_generated(
        skipped_input,
        selected_output,
        profile,
        Some(ENABLED),
        until(),
        || false,
    )
    .is_err());
    assert!(PositiveTileLoad::from_generated(
        skipped_input,
        skipped_output,
        profile,
        Some(ENABLED),
        until(),
        || false,
    )
    .is_err());
}
