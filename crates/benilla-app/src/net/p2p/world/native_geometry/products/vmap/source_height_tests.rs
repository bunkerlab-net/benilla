// SPDX-License-Identifier: GPL-3.0-or-later
//! Real authored MPQ captures, shared raw receipts, and historical scalar height.
//! No caller-supplied inventory, serialized receipt, navigation, or admission proof.
use super::super::super::source_height::{
    CompositionError, HeightReply as SourceHeightReply, HeightRequest as SourceHeightRequest,
    PreparedSourceHeight,
};
use super::super::{prepared::PrepareError, tile_prepared::PreparedTileCollision};
use super::*;
use benilla_native_terrain::{self as terrain_source, PreparedTerrain};
use std::time::Duration;

const MAP: &str = "DBFilesClient\\Map.dbc";
const AREA: &str = "DBFilesClient\\AreaTable.dbc";
const LIQUID: &str = "DBFilesClient\\LiquidType.dbc";
const WDT: &str = "World\\Maps\\Selected\\Selected.wdt";
const SELECTED: terrain_source::Tile = terrain_source::Tile { x: 31, y: 30 };
const HISTORY_ADT: &str = "World\\Maps\\Selected\\Selected_29_31.adt";
const HIT_XY: [u32; 2] = [0x43e8_d540, 0x4471_aaa0];
const MISS_XY: [u32; 2] = [0x43e7_d540, 0x4470_aaa0];

fn terrain_fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures")
            .join(name),
    )
    .unwrap()
}

pub(super) fn shared_entries() -> Vec<Entry> {
    let mut entries = entries();
    let mut wdt = terrain_fixture("source.wdt.test-fixture");
    assert_eq!(&wdt[52..56], b"NIAM");
    wdt[60..60 + 64 * 64 * 8].fill(0);
    let selected = 60 + (usize::from(SELECTED.x) * 64 + usize::from(SELECTED.y)) * 8;
    wdt[selected..selected + 4].copy_from_slice(&1u32.to_le_bytes());

    let adt = placed_terrain("flat.adt.test-fixture");
    for (name, bytes) in [
        (WDT, wdt),
        (ADT, adt),
        (LIQUID, terrain_fixture("LiquidType.dbc.test-fixture")),
    ] {
        *entries.iter_mut().find(|entry| entry.name == name).unwrap() = raw_entry(name, bytes);
    }
    entries.push(raw_entry(
        AREA,
        terrain_fixture("AreaTable.dbc.test-fixture"),
    ));
    entries
}

fn placed_terrain(fixture_name: &str) -> Vec<u8> {
    let mut adt = terrain_fixture(fixture_name);
    // Preserve every MCIN/cell byte. Terrain additionally validates the MHDR's
    // relative-to-byte-20 references to these source placement chunks.
    for (field, tag, payload) in [
        (20, b"MWMO", format!("{ROOT}\0").into_bytes()),
        (32, b"MODF", fixture("hull.modf.test-fixture")),
    ] {
        let offset = u32::try_from(adt.len() - 20).unwrap();
        adt[20 + field..24 + field].copy_from_slice(&offset.to_le_bytes());
        adt.extend(chunk(tag, &payload));
    }
    adt
}

fn terrain(install: &Install) -> PreparedTerrain {
    terrain_source::prepare(&install.0, 0, &[SELECTED]).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn request(view: &PreparedSourceHeight<'_, '_, '_>, z: f32, search: f32) -> SourceHeightRequest {
    SourceHeightRequest {
        binding: view.binding(),
        ticket: 401,
        position: [HIT_XY[0], HIT_XY[1], z.to_bits()],
        max_search: search.to_bits(),
    }
}

fn assert_height(view: &PreparedSourceHeight<'_, '_, '_>, z: f32, search: f32, expected: f32) {
    let request = request(view, z, search);
    assert_eq!(
        view.query(request, deadline()),
        Ok(SourceHeightReply {
            request,
            height: expected.to_bits(),
        }),
        "z={z:?}, search={search:?}"
    );
}

fn assert_original_products(terrain: &PreparedTerrain, bundle: &AssembledSourceMap<'_>) {
    let products: Vec<_> = terrain.tiles().collect();
    assert_eq!(products.len(), 1);
    assert_eq!(products[0].0, SELECTED);
    assert_eq!(products[0].1, terrain_fixture("flat.map"));
    assert_original_collision(bundle);
}

fn assert_original_collision(bundle: &AssembledSourceMap<'_>) {
    assert_eq!(bundle.map().map_id(), 0);
    assert_eq!(bundle.map().tiles().len(), 1);
    assert_eq!(bundle.map().tiles()[0].coordinates(), (30, 31));
    assert_eq!(bundle.models().len(), 1);
    let actual = BTreeMap::from([
        ("000.vmtree".to_owned(), bundle.map().tree_bytes().to_vec()),
        (
            "000_30_31.vmtile".to_owned(),
            bundle.map().tiles()[0].bytes().to_vec(),
        ),
        (
            "Hull.wmo.vmo".to_owned(),
            bundle.models()[0].bytes().to_vec(),
        ),
    ]);
    assert_eq!(
        actual,
        expected_products(),
        "unchanged original WMO products"
    );
}

fn assert_shared_receipts(terrain: &PreparedTerrain, bundle: &AssembledSourceMap<'_>) {
    // LiquidType is also shared; matching only the three required selection
    // inputs would overlook another genuinely consumed overlapping member.
    for name in [MAP, WDT, ADT, LIQUID] {
        let terrain_source::SourceOutcome::Present {
            winning_archive: terrain_winner,
            sha256: terrain_hash,
            bytes: terrain_size,
        } = terrain
            .source_receipts()
            .find(|receipt| receipt.name() == name)
            .unwrap()
            .outcome()
        else {
            panic!("terrain member must be present: {name}");
        };
        let source::SourceOutcome::Present {
            winning_archive: collision_winner,
            sha256: collision_hash,
            bytes: collision_size,
        } = bundle
            .source_receipts()
            .find(|receipt| receipt.name() == name)
            .unwrap()
            .outcome()
        else {
            panic!("collision member must be present: {name}");
        };
        assert_eq!(
            (terrain_winner, terrain_hash, terrain_size),
            (collision_winner, collision_hash, collision_size),
            "{name}"
        );
        assert_eq!(terrain_winner, "model.MPQ");
    }
    assert_eq!(terrain.source_receipts().count(), 5);
    assert!(bundle.source_receipts().count() > 4096);
    assert!(terrain
        .source_receipts()
        .any(|receipt| receipt.name() == AREA));
    assert!(!bundle
        .source_receipts()
        .any(|receipt| receipt.name() == AREA));
    assert!(bundle
        .source_receipts()
        .any(|receipt| receipt.name() == ROOT));
    assert!(!terrain
        .source_receipts()
        .any(|receipt| receipt.name() == ROOT));
}

#[test]
fn source_height_genuine_shared_captures_keep_different_consumed_set_epochs() {
    let install = Install::new(&shared_entries());
    let before = install.bytes();
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_original_products(&terrain, &bundle);
    assert_shared_receipts(&terrain, &bundle);
    assert_ne!(terrain.source_epoch(), capture.source_epoch());
    assert_eq!(terrain.map_directory(), "Selected");
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    assert_eq!(view.binding().map, 0);
    assert_ne!(view.binding().revision, [0; 32]);
    view.verify_unchanged(|| false).unwrap();
    let repeated = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    assert_eq!(repeated.binding(), view.binding());
    assert_height(&view, 20.0, 50.0, 7.25);
    assert_height(&repeated, 0.0, 50.0, 5.0);
    assert_eq!(install.bytes(), before, "both capture paths are read-only");

    // Physical directory identity and unconsumed bytes grant no authority and
    // are not required to agree. Exact shared consumed winners/bytes suffice.
    let mut other_entries = shared_entries();
    other_entries.push(raw_entry("World\\Unused.test-fixture", vec![1; 8]));
    let other = Install::new(&other_entries);
    let other_before = other.bytes();
    let other_capture = source::prepare(&other.0, 0, || false).unwrap();
    let other_bundle = other_capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let compatible = PreparedSourceHeight::from_source(&terrain, &other_bundle, || false).unwrap();
    assert_eq!(compatible.binding(), view.binding());
    assert_height(&compatible, 0.0, 50.0, 5.0);
    assert_eq!(other.bytes(), other_before);
}

#[test]
fn source_height_uses_original_static_selection_with_real_terrain_and_wmo() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    // Original ConvertADT gives flat 7.25. The original public query oracle's
    // hit_top/hit_bottom ray intersects this exact WMO at 5. GetHeightStatic's
    // existing complete-source oracle establishes the selector/call schedule.
    // These fixed results exercise that schedule against actual captured bytes.
    let below_map = f32::from_bits(7.25f32.to_bits() - 1);
    let above_map = f32::from_bits(7.25f32.to_bits() + 1);
    for (z, search, expected) in [
        (18.0, 50.0, 7.25), // z+2 is original hit_top; higher terrain wins
        (18.0, 0.0, 7.25),  // extended first range misses 5; long fallback hits
        (below_map, 50.0, 5.0),
        (7.25, 50.0, 7.25), // strict z<map comparison
        (above_map, 50.0, 7.25),
        (4.0, 50.0, 5.0),
        (-2.0, 50.0, 5.0),  // z+2 is hit_bottom; near-terrain retry hits
        (-40.0, 50.0, 5.0), // two downward misses, then genuine upward ray
        (18.0, -50.0, 7.25),
        (-2.0, -20.0, 5.0), // initial negative-range upward hit
    ] {
        assert_height(&view, z, search, expected);
    }
    for z in [18.0, -2.0, -40.0] {
        let mut query = request(&view, z, 50.0);
        query.position[..2].copy_from_slice(&MISS_XY);
        assert_eq!(
            view.query(query, deadline()).unwrap().height,
            7.25f32.to_bits(),
            "original miss_top/miss_bottom point retains terrain"
        );
    }
}

fn assert_cancelled(error: CompositionError) {
    assert!(
        matches!(
            error,
            CompositionError::Cancelled
                | CompositionError::Collision(PrepareError::Cancelled)
                | CompositionError::Collision(PrepareError::Source(
                    MapBundleError::Cancelled | MapBundleError::Source(SourceError::Cancelled)
                ))
        ),
        "{error:?}"
    );
}

#[test]
fn source_height_rejects_wrong_bindings_invalid_queries_deadlines_and_cancellation() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let polls = Cell::new(0usize);
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || {
        polls.set(polls.get() + 1);
        false
    })
    .unwrap();
    assert!(polls.get() > 10);
    for stop in [1, polls.get() / 2, polls.get()] {
        let calls = Cell::new(0);
        let result = PreparedSourceHeight::from_source(&terrain, &bundle, || {
            calls.set(calls.get() + 1);
            calls.get() >= stop
        });
        assert_cancelled(
            result
                .err()
                .expect("cancelled preparation cannot publish a view"),
        );
    }
    assert_cancelled(view.verify_unchanged(|| true).unwrap_err());
    let query = request(&view, 18.0, 50.0);
    for field in 0..3 {
        let mut wrong = query;
        match field {
            0 => wrong.binding.map += 1,
            1 => wrong.binding.revision[0] ^= 1,
            _ => wrong.ticket = 0,
        }
        assert_eq!(view.query(wrong, deadline()), Err(Error::Identity));
    }
    assert_eq!(view.query(query, Instant::now()), Err(Error::Deadline));
    for axis in 0..4 {
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut bad = query;
            if axis == 3 {
                bad.max_search = invalid.to_bits();
            } else {
                bad.position[axis] = invalid.to_bits();
            }
            assert_eq!(view.query(bad, deadline()), Err(Error::Format));
        }
    }
    for (axis, value) in [(0, 100_001.0f32), (1, -100_001.0), (2, 100_001.0)] {
        let mut bad = query;
        bad.position[axis] = value.to_bits();
        assert_eq!(view.query(bad, deadline()), Err(Error::Unsupported));
    }
    for search in [-10_001.0f32, 10_001.0] {
        let mut bad = query;
        bad.max_search = search.to_bits();
        assert_eq!(view.query(bad, deadline()), Err(Error::Unsupported));
    }
    let mut missing = query;
    missing.position.swap(0, 1); // ADT filename order is not terrain-grid order
    assert_eq!(view.query(missing, deadline()), Err(Error::Missing));
    // Input z is bounded, but the original unclamped extension exceeds the
    // bounded collision profile. No fabricated terrain fallback is returned.
    for z in [20_000.0, 99_999.0] {
        assert_eq!(
            view.query(request(&view, z, 50.0), deadline()),
            Err(Error::Unsupported)
        );
    }
    view.verify_unchanged(|| false).unwrap();
    assert_height(&view, 0.0, 50.0, 5.0);
}

fn change_shared_byte(entries: &mut [Entry], name: &str) {
    let bytes = &mut entries
        .iter_mut()
        .find(|entry| entry.name == name)
        .unwrap()
        .bytes;
    let offset = match name {
        MAP => 28,    // unused Map.dbc field; directory and map ID are unchanged
        LIQUID => 24, // unused LiquidType string field
        WDT => 60 + (usize::from(SELECTED.x) * 64 + usize::from(SELECTED.y)) * 8 + 4,
        ADT => {
            let first_cell = u32::from_le_bytes(bytes[92..96].try_into().unwrap()) as usize;
            first_cell + 8 + 4 // MCNK ix metadata; MCIN owns terrain placement
        }
        _ => panic!("unexpected shared member"),
    };
    bytes[offset] ^= 1;
}

#[test]
fn source_height_rejects_each_shared_byte_mismatch_between_fresh_genuine_captures() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    for name in [MAP, WDT, ADT, LIQUID] {
        let mut changed = shared_entries();
        change_shared_byte(&mut changed, name);
        let other = Install::new(&changed);
        let other_terrain = self::terrain(&other);
        let capture = source::prepare(&other.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        // Both independently genuine captures are fresh; even their generated
        // products agree. Only the exact overlapping consumed raw bytes differ.
        assert_original_products(&other_terrain, &bundle);
        terrain.verify_unchanged().unwrap();
        bundle.verify_unchanged(|| false).unwrap();
        PreparedSourceHeight::from_source(&other_terrain, &bundle, || false).unwrap();
        assert!(
            matches!(
                PreparedSourceHeight::from_source(&terrain, &bundle, || false),
                Err(CompositionError::Validation(Error::Dependency))
            ),
            "mismatched shared member: {name}"
        );
    }
}

#[test]
fn source_height_rejects_equal_shared_bytes_from_a_different_archive_winner() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    for name in [MAP, WDT, ADT] {
        let entries = shared_entries();
        let other = Install::new(&entries);
        let bytes = entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap()
            .bytes
            .clone();
        fs::write(
            other.0.join("patch.MPQ"),
            archive(&[raw_entry(name, bytes)]),
        )
        .unwrap();
        let other_terrain = self::terrain(&other);
        let capture = source::prepare(&other.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        assert_original_products(&other_terrain, &bundle);
        let source::SourceOutcome::Present {
            winning_archive, ..
        } = bundle
            .source_receipts()
            .find(|receipt| receipt.name() == name)
            .unwrap()
            .outcome()
        else {
            panic!("overridden source member must remain present");
        };
        assert_eq!(winning_archive, "patch.MPQ");
        terrain.verify_unchanged().unwrap();
        bundle.verify_unchanged(|| false).unwrap();
        PreparedSourceHeight::from_source(&other_terrain, &bundle, || false).unwrap();
        assert!(
            matches!(
                PreparedSourceHeight::from_source(&terrain, &bundle, || false),
                Err(CompositionError::Validation(Error::Dependency))
            ),
            "mismatched shared winner: {name}"
        );
    }
}

#[test]
fn source_height_rejects_absent_selected_adt_and_wrong_map_in_genuine_bundles() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let mut moved = shared_entries();
    moved
        .iter_mut()
        .find(|entry| entry.name == ADT)
        .unwrap()
        .name = NEW_ADT.to_owned();
    let other = Install::new(&moved);
    let capture = source::prepare(&other.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert!(matches!(
        bundle
            .source_receipts()
            .find(|receipt| receipt.name() == ADT)
            .unwrap()
            .outcome(),
        source::SourceOutcome::Absent
    ));
    bundle.verify_unchanged(|| false).unwrap();
    assert!(matches!(
        PreparedSourceHeight::from_source(&terrain, &bundle, || false),
        Err(CompositionError::Validation(Error::Dependency))
    ));

    let mut different_map = shared_entries();
    different_map
        .iter_mut()
        .find(|entry| entry.name == MAP)
        .unwrap()
        .bytes[20..24]
        .copy_from_slice(&1u32.to_le_bytes());
    let other = Install::new(&different_map);
    let capture = source::prepare(&other.0, 1, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(bundle.map().map_id(), 1);
    bundle.verify_unchanged(|| false).unwrap();
    assert!(matches!(
        PreparedSourceHeight::from_source(&terrain, &bundle, || false),
        Err(CompositionError::Validation(Error::Identity))
    ));
}

fn history_entries(predecessor_present: bool) -> Vec<Entry> {
    let mut entries = shared_entries();
    *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() = raw_entry(
        ADT,
        placed_terrain("liquid_history61_positive.adt.test-fixture"),
    );
    let wdt = &mut entries
        .iter_mut()
        .find(|entry| entry.name == WDT)
        .unwrap()
        .bytes;
    // This preceding output tile participates only in terrain's history closure,
    // never its selected output/residency set. VMap discovery also observes it.
    let predecessor = 60 + (usize::from(SELECTED.x) * 64 + usize::from(SELECTED.y - 1)) * 8;
    wdt[predecessor..predecessor + 4].copy_from_slice(&1u32.to_le_bytes());
    if predecessor_present {
        entries.push(raw_entry(
            HISTORY_ADT,
            terrain_fixture("liquid_water_flat.adt.test-fixture"),
        ));
    }
    entries
}

#[test]
fn source_height_compares_overlapping_history_bytes_and_absence_from_genuine_captures() {
    for mismatch in ["bytes", "presence"] {
        let predecessor_present = mismatch == "bytes";
        let install = Install::new(&history_entries(predecessor_present));
        let terrain = terrain(&install);
        let expected = if predecessor_present {
            "liquid_history61_positive.map"
        } else {
            "liquid_history61_initial.map"
        };
        assert_eq!(terrain.tiles().count(), 1, "history does not become output");
        assert_eq!(terrain.tiles().next().unwrap().1, terrain_fixture(expected));
        assert_eq!(terrain.source_receipts().count(), 6);
        let history_receipt = terrain
            .source_receipts()
            .find(|receipt| receipt.name() == HISTORY_ADT)
            .unwrap();
        assert_eq!(
            matches!(
                history_receipt.outcome(),
                terrain_source::SourceOutcome::Absent
            ),
            !predecessor_present
        );
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        assert_original_collision(&bundle);
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        assert_height(&view, 0.0, 50.0, 5.0);

        let mut changed = history_entries(true);
        if predecessor_present {
            let bytes = &mut changed
                .iter_mut()
                .find(|entry| entry.name == HISTORY_ADT)
                .unwrap()
                .bytes;
            let first_cell = u32::from_le_bytes(bytes[92..96].try_into().unwrap()) as usize;
            bytes[first_cell + 8 + 4] ^= 1; // ignored MCNK coordinate, exact history digest changes
        }
        let other = Install::new(&changed);
        let other_terrain = self::terrain(&other);
        assert_eq!(
            other_terrain.tiles().next().unwrap().1,
            terrain_fixture("liquid_history61_positive.map")
        );
        let other_capture = source::prepare(&other.0, 0, || false).unwrap();
        let other_bundle = other_capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        assert_original_collision(&other_bundle);
        assert!(matches!(
            other_bundle
                .source_receipts()
                .find(|receipt| receipt.name() == HISTORY_ADT)
                .unwrap()
                .outcome(),
            source::SourceOutcome::Present { .. }
        ));
        // The selected Map/WDT/ADT bytes are identical across these installs.
        // Freshness cannot substitute for comparing their overlapping history.
        terrain.verify_unchanged().unwrap();
        other_bundle.verify_unchanged(|| false).unwrap();
        PreparedSourceHeight::from_source(&other_terrain, &other_bundle, || false).unwrap();
        assert!(
            matches!(
                PreparedSourceHeight::from_source(&terrain, &other_bundle, || false),
                Err(CompositionError::Validation(Error::Dependency))
            ),
            "history {mismatch} mismatch cannot be composed"
        );
        view.verify_unchanged(|| false).unwrap();
        assert_height(&view, 0.0, 50.0, 5.0);
    }
}

#[test]
fn source_height_missing_collision_on_a_real_fallback_never_becomes_terrain_height() {
    let mut entries = shared_entries();
    // The covered terrain tile has no placement. The genuine map contains the
    // WMO only in a different residency tile, preserving its original bounds.
    *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() =
        raw_entry(ADT, terrain_fixture("flat.adt.test-fixture"));
    let mut other_adt = chunk(b"MVER", &18u32.to_le_bytes());
    other_adt.extend(chunk(b"MWMO", format!("{ROOT}\0").as_bytes()));
    other_adt.extend(chunk(b"MODF", &fixture("hull.modf.test-fixture")));
    entries.push(raw_entry(
        "World\\Maps\\Selected\\Selected_31_31.adt",
        other_adt,
    ));
    let install = Install::new(&entries);
    let terrain = terrain(&install);
    assert_eq!(
        terrain.tiles().next().unwrap().1,
        terrain_fixture("flat.map")
    );
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(bundle.map().tiles().len(), 1);
    assert_eq!(bundle.map().tiles()[0].coordinates(), (31, 31));
    let collision = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    // The exact two downward rays from z+2=0 prune below original WMO bounds.
    // They are genuine misses despite unloaded geometry. Only the near-terrain
    // retry from 7.25+2 reaches the unavailable leaf and must remain Missing.
    let mut probe = HeightRequest {
        snapshot: collision.binding(),
        ticket: 402,
        position: [HIT_XY[0], HIT_XY[1], 0.0f32.to_bits()],
        max_search: 50.0f32.to_bits(),
    };
    for range in [50.0f32, 10_000.0] {
        probe.max_search = range.to_bits();
        assert_eq!(
            collision.query_height(probe, deadline()).unwrap().height,
            (-200_000.0f32).to_bits()
        );
    }
    probe.position[2] = 9.25f32.to_bits();
    probe.max_search = 10.0f32.to_bits();
    assert_eq!(
        collision.query_height(probe, deadline()),
        Err(Error::Missing)
    );
    assert_eq!(
        view.query(request(&view, -2.0, 50.0), deadline()),
        Err(Error::Missing)
    );
    view.verify_unchanged(|| false).unwrap();
}

#[test]
fn source_height_recheck_rejects_mutation_but_queries_keep_the_captured_historical_view() {
    for changed_member in [AREA, ROOT, ADT] {
        let mut entries = shared_entries();
        let install = Install::new(&entries);
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let binding = view.binding();
        let query = request(&view, 0.0, 50.0);
        let reply = view.query(query, deadline()).unwrap();
        match changed_member {
            AREA => {
                // Unused AreaTable field changes only terrain's consumed set.
                entries
                    .iter_mut()
                    .find(|entry| entry.name == AREA)
                    .unwrap()
                    .bytes[24] ^= 1;
            }
            ROOT => {
                // Original WMO root ambient color is captured but not emitted.
                entries
                    .iter_mut()
                    .find(|entry| entry.name == ROOT)
                    .unwrap()
                    .bytes[48] ^= 1;
            }
            ADT => change_shared_byte(&mut entries, ADT),
            _ => unreachable!(),
        }
        install.write(&entries);
        let changed = install.bytes();
        let assert_changed = |error| match changed_member {
            ROOT => assert!(matches!(
                error,
                CompositionError::Collision(PrepareError::Source(MapBundleError::Source(
                    SourceError::Changed
                )))
            )),
            _ => assert!(matches!(error, CompositionError::TerrainSource)),
        };
        assert_changed(view.verify_unchanged(|| false).unwrap_err());
        assert_changed(
            PreparedSourceHeight::from_source(&terrain, &bundle, || false)
                .err()
                .expect("stale owners cannot reconstruct a new historical view"),
        );
        assert_eq!(view.binding(), binding);
        assert_eq!(view.query(query, deadline()), Ok(reply));
        let fresh_terrain = self::terrain(&install);
        let fresh_capture = source::prepare(&install.0, 0, || false).unwrap();
        let fresh_bundle = fresh_capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        assert_original_products(&fresh_terrain, &fresh_bundle);
        let fresh =
            PreparedSourceHeight::from_source(&fresh_terrain, &fresh_bundle, || false).unwrap();
        assert_ne!(
            fresh.binding(),
            binding,
            "every consumed input binds revision"
        );
        assert_eq!(fresh.query(query, deadline()), Err(Error::Identity));
        assert_height(&fresh, 0.0, 50.0, 5.0);
        assert_eq!(view.query(query, deadline()), Ok(reply));
        assert_eq!(
            install.bytes(),
            changed,
            "rechecks and queries are read-only"
        );
    }
}

#[test]
fn source_height_shared_budget_charges_terrain_before_missing_lookup() {
    let install = Install::new(&shared_entries());
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    for missing in [false, true] {
        let mut query = request(&view, 20.0, 50.0);
        if missing {
            query.position.swap(0, 1);
        }
        let mut budget = QueryBudget::new(deadline());
        budget.restrict_work_for_test(0);
        assert_eq!(
            view.query_with_budget(query, &mut budget),
            Err(Error::Budget)
        );
        assert_eq!(budget.remaining_work_for_test(), 0);
        let ample = view.query(query, deadline());
        if missing {
            assert_eq!(ample, Err(Error::Missing));
        } else {
            ample.unwrap();
        }
        // Request validation still precedes the new terrain debit.
        query.ticket = 0;
        assert_eq!(
            view.query_with_budget(query, &mut budget),
            Err(Error::Identity)
        );
    }
}

#[test]
fn resident_swimming_height50_distinguishes_ground10_on_actual_upward_retry() {
    let mut entries = shared_entries();
    let mut adt = placed_terrain("flat.adt.test-fixture");
    // Authored flat terrain above the existing floor: the upward retry must use
    // the caller's actual range, not Ground's ten-yard range.
    let mut offset = 0;
    let mut cells = 0;
    while offset + 8 <= adt.len() {
        let size = u32::from_le_bytes(adt[offset + 4..offset + 8].try_into().unwrap()) as usize;
        if &adt[offset..offset + 4] == b"KNCM" {
            adt[offset + 8 + 0x70..offset + 8 + 0x74].copy_from_slice(&100.0f32.to_le_bytes());
            cells += 1;
        }
        offset += 8 + size;
    }
    assert_eq!(cells, 256);
    let original_adt = adt.clone();
    *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() = raw_entry(ADT, adt);
    let install = Install::new(&entries);
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    assert_height(&view, -40.0, 50.0, 5.0);
    assert_height(&view, -40.0, 10.0, 100.0);

    // A second authored floor one yard above makes the nested observation differ
    // from the initial upward hit. It cannot replace the caller's outer groundZ.
    let mut adt = original_adt;
    let modf = adt.len() - 64 - 8;
    assert_eq!(&adt[modf..modf + 4], b"FDOM");
    let mut second = adt[modf + 8..].to_vec();
    let id = u32::from_le_bytes(second[4..8].try_into().unwrap());
    second[4..8].copy_from_slice(&(id + 1).to_le_bytes());
    for offset in [12, 36, 48] {
        let value = f32::from_le_bytes(second[offset..offset + 4].try_into().unwrap());
        second[offset..offset + 4].copy_from_slice(&(value + 1.0).to_le_bytes());
    }
    adt[modf + 4..modf + 8].copy_from_slice(&128u32.to_le_bytes());
    adt.extend(second);
    *entries.iter_mut().find(|entry| entry.name == ADT).unwrap() = raw_entry(ADT, adt);
    let layered = Install::new(&entries);
    let terrain = terrain_source::prepare(&layered.0, 0, &[SELECTED]).unwrap();
    let capture = source::prepare(&layered.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    assert_height(&view, -40.0, 50.0, 5.0);
    let liquid = view
        .point_no_water(
            super::super::super::source_height::PointNoWaterRequest {
                binding: view.binding(),
                ticket: 402,
                position: [HIT_XY[0], HIT_XY[1], 5.0f32.to_bits()],
                mask: 15,
            },
            deadline(),
            || false,
        )
        .unwrap();
    assert_eq!(liquid.initial_static_height(), 6.0f32.to_bits());
}
