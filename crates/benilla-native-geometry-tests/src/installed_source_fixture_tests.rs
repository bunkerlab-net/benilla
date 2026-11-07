// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine source preparation and strict canonical raw parsing of the shared guard.
use crate::{
    planar,
    products::{
        source_height::{CompositionError, HeightReply, HeightRequest, PreparedSourceHeight},
        vmap::{
            installed_source_fixture as fixture, tile_prepared::PreparedTileCollision, QueryBudget,
            RayReply, RayRequest, StaticRay,
        },
        Error, TileId,
    },
};
use benilla_formats::{Chain, NativeM2MapInventory};
use benilla_native_vmap_source::{self as source, AssembledSourceMap};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn assert_full_partition(bundle: &AssembledSourceMap<'_>, inventory: &NativeM2MapInventory) {
    let expected: BTreeSet<_> = fixture::guard_tiles()
        .into_iter()
        .chain([(30, 31)])
        .collect();
    let mut present = 0;
    let mut absent = 0;
    for x in 0..64 {
        for y in 0..64 {
            let name = format!("World\\Maps\\Selected\\Selected_{x}_{y}.adt");
            let outcomes: Vec<_> = bundle
                .source_receipts()
                .filter(|receipt| receipt.name() == name)
                .map(|receipt| receipt.outcome())
                .collect();
            assert_eq!(outcomes.len(), 1, "exact coordinate {x},{y}");
            match (inventory.tile_digest((x, y)), outcomes[0]) {
                (
                    Some(raw),
                    source::SourceOutcome::Present {
                        sha256,
                        winning_archive,
                        ..
                    },
                ) => {
                    let hex: String = raw.iter().map(|byte| format!("{byte:02x}")).collect();
                    assert_eq!(hex, sha256);
                    assert_eq!(winning_archive, "model.MPQ");
                    assert!(expected.contains(&(x as u32, y as u32)));
                    present += 1;
                }
                (None, source::SourceOutcome::Absent) => absent += 1,
                _ => panic!("wrong coordinate outcome {x},{y}"),
            }
        }
    }
    assert_eq!((present, absent), (10, 4086));
}

#[test]
fn installed_source_fixture_prepares_real_sources_and_nine_canonical_raw_guards() {
    let install = fixture::install();
    let before = install.bytes();
    let inventory = fixture::raw_inventory(&install);
    assert_eq!(inventory.directory(), fixture::MAP_DIRECTORY);
    assert_eq!(inventory.placements().count(), 0);
    assert_eq!(inventory.wmo_placements().count(), 1);
    assert_eq!(inventory.source_provenance().unwrap().receipts().len(), 14);
    let mut chain = Chain::open(install.path()).unwrap();
    for (x, y) in fixture::guard_tiles() {
        let name = format!("World\\Maps\\Selected\\Selected_{x}_{y}.adt");
        let bytes = chain.read_file(&name).unwrap();
        let digest: [u8; 32] = Sha256::digest(&bytes).into();
        assert_eq!(inventory.tile_digest((x as i32, y as i32)), Some(digest));
        let tile = benilla_formats::adt_to_tile_mesh(&bytes).unwrap();
        assert!(tile.native_static_coverage_safe && tile.native_coverage_safe);
        assert_eq!(tile.chunks.len(), 256);
        assert!(tile.doodads.is_empty() && tile.wmos.is_empty());
        assert!(tile.chunks.iter().all(|chunk| {
            chunk.holes == 0
                && chunk.liquids.is_empty()
                && chunk.positions.iter().all(|p| p[2] == fixture::HEIGHT)
        }));
        let base = |tile| ((32. - f64::from(tile)) * (1600. / 3.)) as f32;
        let from = [base(y) - 10., base(x) - 10., fixture::HEIGHT];
        let to = [from[0] - 4., from[1], fixture::HEIGHT];
        assert_eq!(
            planar::prove_planar_tile(&tile.chunks, (x, y), from, to, 0.5),
            Ok(()),
            "all 256 live chunks must have canonical vertices and topology: {x},{y}"
        );
        let mut malformed = tile.chunks;
        malformed.last_mut().unwrap().positions[144][0] += 0.125;
        assert_eq!(
            planar::prove_planar_tile(&malformed, (x, y), from, to, 0.5),
            Err(planar::CoverageError::MalformedTerrain),
            "a distant final vertex is still validated"
        );
    }
    let selected = fixture::guard_source_tiles();
    let terrain = benilla_native_terrain::prepare(install.path(), 0, &selected).unwrap();
    assert_eq!(terrain.tiles().count(), 9);
    assert_eq!(
        terrain
            .tiles()
            .map(|(tile, _)| tile)
            .collect::<BTreeSet<_>>(),
        selected.into_iter().collect()
    );
    let capture = source::prepare(install.path(), 0, || false).unwrap();
    let bounds = capture
        .assembled_spawn_bounds(fixture::profile(), || false)
        .unwrap();
    assert_eq!(bounds.bounds().len(), 1);
    assert_eq!(
        bounds.bounds()[0].bounds,
        [[16599., 16098., 1.], [16604., 16105., 10.]]
    );
    let (from, to) = fixture::support();
    assert_eq!(
        benilla_formats::world_to_tile(from[0], from[1]),
        fixture::CENTER
    );
    let middle = 0.5_f32 * 64. * 533.333_3_f32;
    for point in [from, to] {
        let internal = [middle - point[0], middle - point[1], point[2]];
        assert!((0..2).all(|axis| internal[axis] - 0.5 > bounds.bounds()[0].bounds[1][axis]));
        assert!(point[2] + 2. < bounds.bounds()[0].bounds[0][2]);
    }
    let bundle = capture
        .assemble_map_bundle(fixture::profile(), || false)
        .unwrap()
        .unwrap();
    assert_full_partition(&bundle, &inventory);
    assert_eq!(
        bundle.models().len(),
        1,
        "inactive hull-less MODD remains a raw dependency"
    );
    assert_eq!(bundle.map().tiles().len(), 1);
    assert_eq!(bundle.map().tiles()[0].coordinates(), (30, 31));
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let compatibility = view
        .match_raw_inventory(&inventory, deadline(), || false)
        .unwrap();
    let owner = inventory.source_provenance().unwrap();
    compatibility
        .verify_current_owner(owner, deadline(), || false)
        .unwrap();
    for (i, point) in [from, to].into_iter().enumerate() {
        let request = HeightRequest {
            binding: view.binding(),
            ticket: 1100 + i as u64,
            position: point.map(f32::to_bits),
            max_search: 10.0_f32.to_bits(),
        };
        assert_eq!(
            view.query(request, deadline()),
            Ok(HeightReply {
                request,
                height: fixture::HEIGHT.to_bits()
            })
        );
        // Both the first ray and the selector's mandatory 10,000-unit retry
        // start below the exterior BIH. Parallel XY alone would not prune it.
        let mut crossing = request;
        crossing.position[2] = 20.0_f32.to_bits();
        assert_eq!(view.query(crossing, deadline()), Err(Error::Missing));
    }
    let collision = PreparedTileCollision::from_source(
        &bundle,
        &fixture::guard_source_tiles()
            .into_iter()
            .map(|tile| TileId {
                x: tile.x,
                y: tile.y,
            })
            .collect::<Vec<_>>(),
        || false,
    )
    .unwrap();
    assert_eq!(collision.resident_tiles().len(), 9);
    let eye = |point: [f32; 3]| [point[0], point[1], point[2] + 2.].map(f32::to_bits);
    let request = RayRequest {
        snapshot: collision.binding(),
        ticket: 1110,
        from: eye(from),
        to: eye(to),
        ignore_m2: false,
    };
    let mut budget = QueryBudget::new(deadline());
    let remaining = budget.remaining_work_for_test();
    assert_eq!(
        view.source_ray_with_budget(
            request.ticket,
            request.from,
            request.to,
            &mut budget,
            || false
        )
        .unwrap(),
        RayReply {
            request,
            result: StaticRay::Miss
        }
    );
    assert!(budget.remaining_work_for_test() < remaining);
    let relevant_from = [0x43e8_d540, 0x4471_aaa0, 20.0_f32.to_bits()];
    let relevant_to = [relevant_from[0], relevant_from[1], 0.0_f32.to_bits()];
    assert!(matches!(
        view.source_ray_with_budget(
            1111,
            relevant_from,
            relevant_to,
            &mut QueryBudget::new(deadline()),
            || false
        ),
        Err(CompositionError::Validation(Error::Missing))
    ));
    compatibility
        .verify_current_owner(owner, deadline(), || false)
        .unwrap();
    let replacement = fixture::raw_inventory(&install);
    assert_eq!(replacement.source_digest(), inventory.source_digest());
    assert!(matches!(
        compatibility.verify_current_owner(
            replacement.source_provenance().unwrap(),
            deadline(),
            || false
        ),
        Err(CompositionError::Validation(Error::Identity))
    ));
    assert_eq!(
        install.bytes(),
        before,
        "all preparation and observations are read-only"
    );
}

#[test]
fn installed_source_fixture_moved_exterior_remains_a_strict_source_dependency() {
    let entries = fixture::overlapping_entries();
    let install = fixture::Install::new(&entries);
    let inventory = fixture::raw_inventory(&install);
    let (placement, _) = inventory.wmo_placements().next().unwrap();
    assert_eq!(placement.position, [-4278., -4276., fixture::HEIGHT - 1.]);
    assert_eq!(placement.rotation, [0.; 3]);
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &fixture::guard_source_tiles()).unwrap();
    let capture = source::prepare(install.path(), 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(fixture::profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    view.match_raw_inventory(&inventory, deadline(), || false)
        .unwrap();
    let request = HeightRequest {
        binding: view.binding(),
        ticket: 1112,
        position: fixture::support().0.map(f32::to_bits),
        max_search: 10.0_f32.to_bits(),
    };
    assert_eq!(view.query(request, deadline()), Err(Error::Missing));
    let mut changed = entries;
    changed
        .iter_mut()
        .find(|(name, _)| name == fixture::WDT)
        .unwrap()
        .1[64] ^= 1;
    install.write(&changed);
    assert!(view.verify_unchanged(|| false).is_err());
}
