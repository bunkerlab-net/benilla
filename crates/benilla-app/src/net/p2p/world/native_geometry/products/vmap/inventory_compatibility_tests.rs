// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine client-chain inventory against independently captured terrain/VMap inputs.
//! Hash/length and exact-owner evidence only; no navigation or corridor authority.
use super::super::super::source_height::{CompositionError, PreparedSourceHeight};
use super::super::prepared::PrepareError;
use super::*;
use benilla_formats::{Chain, NativeM2MapInventory};
use benilla_native_terrain::{self as terrain_source, PreparedTerrain};
use std::time::Duration;

const WDT: &str = "World\\Maps\\Selected\\Selected.wdt";
const AREA: &str = "DBFilesClient\\AreaTable.dbc";
const INACTIVE: &str = "World\\Props\\Inactive.m2";
const TRANSPOSED_ADT: &str = "World\\Maps\\Selected\\Selected_31_30.adt";
const SELECTED: terrain_source::Tile = terrain_source::Tile { x: 31, y: 30 };

fn terrain_fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-terrain/tools/fixtures")
            .join(name),
    )
    .unwrap()
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn put_word(bytes: &mut [u8], at: usize, value: u32) {
    bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
}

fn strict_placed_adt() -> Vec<u8> {
    let mut adt = terrain_fixture("flat.adt.test-fixture");
    // The last physical MCNK has no later cell offsets to relocate. Extend it
    // with the exact MODF reference and update its actual MCIN size entry.
    let last = (0..256)
        .map(|slot| word(&adt, 92 + slot * 16) as usize)
        .max()
        .unwrap();
    let old_size = word(&adt, last + 4);
    assert_eq!(last + 8 + old_size as usize, adt.len());
    put_word(&mut adt, last + 8 + 0x38, 1);
    put_word(&mut adt, last + 8 + 0x20, old_size + 8);
    let references = chunk(b"MCRF", &0u32.to_le_bytes());
    let size = old_size + references.len() as u32;
    put_word(&mut adt, last + 4, size);
    let slot = (0..256)
        .find(|slot| word(&adt, 92 + slot * 16) as usize == last)
        .unwrap();
    put_word(&mut adt, 92 + slot * 16 + 4, size + 8);
    adt.extend(references);

    let mut placement = fixture("hull.modf.test-fixture");
    // Vanilla raw static closure excludes the later-client MODF scale field.
    placement[62..64].fill(0);
    for (field, tag, payload) in [
        (20, b"MWMO", format!("{ROOT}\0").into_bytes()),
        (24, b"MWID", 0u32.to_le_bytes().to_vec()),
        (32, b"MODF", placement),
    ] {
        let offset = u32::try_from(adt.len() - 20).unwrap();
        put_word(&mut adt, 20 + field, offset);
        adt.extend(chunk(tag, &payload));
    }
    let parsed = benilla_formats::adt_to_tile_mesh(&adt).unwrap();
    assert!(parsed.native_static_coverage_safe);
    assert_eq!(parsed.chunks.len(), 256);
    assert_eq!(parsed.wmos.len(), 1);
    assert!(parsed.doodads.is_empty());
    adt
}

fn compatible_entries() -> Vec<Entry> {
    let mut entries = source_height_tests::shared_entries();
    let mut root = fixture("hull.root.test-fixture");
    let mut group = fixture("hull_000.group.test-fixture");
    // MOGI supplies the strict renderer's complete declared-group metadata.
    let mut info = [0; 32];
    info[..28].copy_from_slice(&group[28..56]);
    root.extend(chunk(b"MOGI", &info));

    // Set zero is empty; set one owns a hull-less, inactive MODD dependency.
    // Both consumers must still read its full raw M2, despite emitting no model.
    put_word(&mut root, 20 + 20, 1);
    put_word(&mut root, 20 + 24, 2);
    root.extend(chunk(b"MODN", format!("{INACTIVE}\0").as_bytes()));
    let mut doodad = [0; 40];
    put_word(&mut doodad, 28, 1.0f32.to_bits());
    put_word(&mut doodad, 32, 1.0f32.to_bits());
    root.extend(chunk(b"MODD", &doodad));
    let mut sets = [0; 64];
    put_word(&mut sets, 32 + 24, 1);
    root.extend(chunk(b"MODS", &sets));
    let reference = chunk(b"MODR", &0u16.to_le_bytes());
    let group_size = word(&group, 16) + reference.len() as u32;
    put_word(&mut group, 16, group_size);
    group.extend(reference);
    let mut inactive = vec![0; 0x144];
    inactive[..4].copy_from_slice(b"MD20");
    put_word(&mut inactive, 4, 256);
    for (name, bytes) in [(ADT, strict_placed_adt()), (ROOT, root), (GROUP, group)] {
        *entries.iter_mut().find(|entry| entry.name == name).unwrap() = raw_entry(name, bytes);
    }
    entries.push(raw_entry(INACTIVE, inactive));
    entries
}

fn raw_inventory(install: &Install) -> NativeM2MapInventory {
    NativeM2MapInventory::load(&mut Chain::open(&install.0).unwrap(), "Selected")
        .expect("independent strict renderer inventory acceptance")
}

fn terrain(install: &Install) -> PreparedTerrain {
    terrain_source::prepare(&install.0, 0, &[SELECTED]).unwrap()
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn assert_dependency(error: CompositionError) {
    assert!(
        matches!(error, CompositionError::Validation(Error::Dependency)),
        "{error:?}"
    );
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

fn assert_full_partition(bundle: &AssembledSourceMap<'_>, inventory: &NativeM2MapInventory) {
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
                (Some(raw), source::SourceOutcome::Present { sha256, .. }) => {
                    assert_eq!(raw.as_slice(), unhex(sha256));
                    assert_eq!((x, y), (30, 31));
                }
                (None, source::SourceOutcome::Absent) => {}
                _ => panic!("wrong coordinate outcome {x},{y}"),
            }
        }
    }
}

#[test]
fn inventory_compatibility_accepts_genuine_closure_and_retains_only_its_exact_owner() {
    let install = Install::new(&compatible_entries());
    let before = install.bytes();
    let inventory = raw_inventory(&install);
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    assert_full_partition(&bundle, &inventory);
    let owner = inventory.source_provenance().unwrap();
    assert_eq!(owner.receipts().len(), 5);
    assert!(owner
        .receipts()
        .all(|receipt| receipt.winning_archive() == "model.MPQ"));
    assert_eq!(inventory.wmo_placements().count(), 1);
    let (_, wmo) = inventory.wmo_placements().next().unwrap();
    assert!(wmo
        .collision()
        .root()
        .selected_doodads(0)
        .unwrap()
        .is_empty());
    assert_eq!(wmo.collision().root().selected_doodads(1).unwrap(), [0]);
    assert!(wmo.doodad(0).unwrap().matches(None));
    assert_eq!(
        bundle.models().len(),
        1,
        "inactive hull-less M2 emits no model"
    );
    for receipt in owner.receipts() {
        let matched = bundle
            .source_receipts()
            .find(|captured| captured.name().eq_ignore_ascii_case(receipt.member()))
            .unwrap();
        let source::SourceOutcome::Present {
            winning_archive,
            sha256,
            bytes,
        } = matched.outcome()
        else {
            panic!("every raw dependency must be captured");
        };
        assert_eq!(receipt.winning_archive(), winning_archive);
        assert_eq!(receipt.sha256().as_slice(), unhex(sha256));
        assert_eq!(receipt.bytes(), bytes as u64);
    }
    let proof = view
        .match_raw_inventory(&inventory, deadline(), || false)
        .unwrap();
    assert_ne!(proof.revision(), [0; 32]);
    let retained_owner = owner.clone();
    assert!(owner.same_owner(&retained_owner));
    proof
        .verify_current_owner(&retained_owner, deadline(), || false)
        .unwrap();
    let replacement = raw_inventory(&install);
    let replacement_owner = replacement.source_provenance().unwrap();
    assert_eq!(inventory.source_digest(), replacement.source_digest());
    assert_eq!(owner.receipt_digest(), replacement_owner.receipt_digest());
    assert!(!owner.same_owner(replacement_owner));
    assert!(matches!(
        proof.verify_current_owner(replacement_owner, deadline(), || false),
        Err(CompositionError::Validation(Error::Identity))
    ));
    let replacement_proof = view
        .match_raw_inventory(&replacement, deadline(), || false)
        .unwrap();
    assert_eq!(
        proof.revision(),
        replacement_proof.revision(),
        "revision is not owner identity"
    );
    replacement_proof
        .verify_current_owner(replacement_owner, deadline(), || false)
        .unwrap();
    drop(inventory);
    proof
        .verify_current_owner(&retained_owner, deadline(), || false)
        .unwrap();
    assert_eq!(
        install.bytes(),
        before,
        "capture and matching are read-only"
    );
}

#[test]
fn inventory_compatibility_rejects_equal_bytes_from_different_fresh_winners() {
    let entries = compatible_entries();
    let raw_install = Install::new(&entries);
    let inventory = raw_inventory(&raw_install);
    for name in [WDT, ADT, ROOT, GROUP, INACTIVE] {
        let source_install = Install::new(&entries);
        let bytes = entries
            .iter()
            .find(|entry| entry.name == name)
            .unwrap()
            .bytes
            .clone();
        fs::write(
            source_install.0.join("patch.MPQ"),
            archive(&[raw_entry(name, bytes)]),
        )
        .unwrap();
        let own_inventory = raw_inventory(&source_install);
        assert_eq!(inventory.source_digest(), own_inventory.source_digest());
        assert_ne!(
            inventory.source_provenance().unwrap().receipt_digest(),
            own_inventory.source_provenance().unwrap().receipt_digest()
        );
        let terrain = terrain(&source_install);
        let capture = source::prepare(&source_install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        terrain.verify_unchanged().unwrap();
        bundle.verify_unchanged(|| false).unwrap();
        view.match_raw_inventory(&own_inventory, deadline(), || false)
            .unwrap();
        assert_dependency(
            view.match_raw_inventory(&inventory, deadline(), || false)
                .err()
                .expect(name),
        );
    }
}

fn change_unused(entries: &mut [Entry], name: &str) {
    let bytes = &mut entries
        .iter_mut()
        .find(|entry| entry.name == name)
        .unwrap()
        .bytes;
    let at = match name {
        AREA => 24,
        WDT => 60 + (31 * 64 + 30) * 8 + 4,
        ADT => {
            // MCNK area is consumed by terrain; this separate unused MHDR word is not.
            20 + 60
        }
        ROOT => 48,        // MOHD ambient color
        GROUP => 20 + 64,  // unused MOGP header word
        INACTIVE => 0x140, // unused raw M2 header field, including for hull-less models
        _ => panic!("unexpected source member"),
    };
    bytes[at] ^= 1;
}

#[test]
fn inventory_compatibility_rejects_raw_unused_root_group_and_inactive_model_mismatches() {
    let raw_install = Install::new(&compatible_entries());
    let inventory = raw_inventory(&raw_install);
    for name in [ROOT, GROUP, INACTIVE] {
        let mut entries = compatible_entries();
        change_unused(&mut entries, name);
        let source_install = Install::new(&entries);
        let own_inventory = raw_inventory(&source_install);
        assert_ne!(inventory.source_digest(), own_inventory.source_digest());
        let terrain = terrain(&source_install);
        let capture = source::prepare(&source_install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        terrain.verify_unchanged().unwrap();
        bundle.verify_unchanged(|| false).unwrap();
        view.match_raw_inventory(&own_inventory, deadline(), || false)
            .unwrap();
        assert_dependency(
            view.match_raw_inventory(&inventory, deadline(), || false)
                .err()
                .expect(name),
        );
    }
}

#[test]
fn inventory_compatibility_rejects_extra_present_or_dummy_coordinates() {
    for name in [TRANSPOSED_ADT, NEW_ADT] {
        for bytes in [terrain_fixture("flat.adt.test-fixture"), vec![], vec![0]] {
            let mut entries = compatible_entries();
            let dummy = bytes.len() <= 1;
            entries.push(raw_entry(name, bytes));
            let install = Install::new(&entries);
            let inventory = raw_inventory(&install);
            assert!(inventory.tile_digest((31, 30)).is_none());
            assert!(inventory.tile_digest((63, 63)).is_none());
            let terrain = terrain(&install);
            let capture = source::prepare(&install.0, 0, || false).unwrap();
            let bundle = capture
                .assemble_map_bundle(profile(), || false)
                .unwrap()
                .unwrap();
            let outcome = bundle
                .source_receipts()
                .find(|receipt| receipt.name() == name)
                .unwrap()
                .outcome();
            assert_eq!(
                matches!(outcome, source::SourceOutcome::Dummy { .. }),
                dummy
            );
            assert!(dummy || matches!(outcome, source::SourceOutcome::Present { .. }));
            let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
            view.verify_unchanged(|| false).unwrap();
            assert_dependency(
                view.match_raw_inventory(&inventory, deadline(), || false)
                    .err()
                    .expect("extra coordinate cannot disappear"),
            );
        }
    }
}

#[test]
fn inventory_compatibility_rejects_missing_or_dummy_wdt_present_dependency() {
    for missing in [true, false] {
        let mut entries = compatible_entries();
        let wdt = &mut entries
            .iter_mut()
            .find(|entry| entry.name == WDT)
            .unwrap()
            .bytes;
        put_word(wdt, 60 + (63 * 64 + 63) * 8, 1);
        entries.push(raw_entry(NEW_ADT, terrain_fixture("flat.adt.test-fixture")));
        let raw_install = Install::new(&entries);
        let inventory = raw_inventory(&raw_install);
        assert!(inventory.tile_digest((63, 63)).is_some());
        entries.retain(|entry| entry.name != NEW_ADT);
        if !missing {
            entries.push(raw_entry(NEW_ADT, vec![0]));
        }
        let source_install = Install::new(&entries);
        assert!(NativeM2MapInventory::load(
            &mut Chain::open(&source_install.0).unwrap(),
            "Selected"
        )
        .is_err());
        let terrain = terrain(&source_install);
        let capture = source::prepare(&source_install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        view.verify_unchanged(|| false).unwrap();
        assert_dependency(
            view.match_raw_inventory(&inventory, deadline(), || false)
                .err()
                .expect("required raw ADT has no matching positive receipt"),
        );
    }
}

#[test]
fn inventory_compatibility_rechecks_mutation_before_acquisition_and_after_proof() {
    for name in [AREA, WDT, ADT, ROOT, GROUP, INACTIVE] {
        let mut entries = compatible_entries();
        let install = Install::new(&entries);
        let inventory = raw_inventory(&install);
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let proof = view
            .match_raw_inventory(&inventory, deadline(), || false)
            .unwrap();
        change_unused(&mut entries, name);
        install.write(&entries);
        let after = install.bytes();
        let assert_stale = |error| {
            assert!(
                matches!(
                    error,
                    CompositionError::TerrainSource
                        | CompositionError::Collision(PrepareError::Source(
                            MapBundleError::Source(SourceError::Changed)
                        ))
                ),
                "{name}: {error:?}"
            );
        };
        assert_stale(
            view.match_raw_inventory(&inventory, deadline(), || false)
                .err()
                .expect("stale capture before acquisition"),
        );
        assert_stale(
            proof
                .verify_current_owner(inventory.source_provenance().unwrap(), deadline(), || false)
                .unwrap_err(),
        );
        assert_eq!(
            install.bytes(),
            after,
            "freshness checks never rewrite inputs"
        );
    }
}

#[test]
fn inventory_compatibility_rechecks_sources_after_metadata_comparison_begins() {
    for name in [AREA, GROUP] {
        let mut entries = compatible_entries();
        let install = Install::new(&entries);
        let inventory = raw_inventory(&install);
        let terrain = terrain(&install);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
        let freshness_polls = Cell::new(0);
        view.verify_unchanged(|| {
            freshness_polls.set(freshness_polls.get() + 1);
            false
        })
        .unwrap();
        change_unused(&mut entries, name);
        // Acquisition polls once before provenance, once before freshness and
        // once afterward. The next poll begins the immutable receipt comparison.
        let mutate_at = freshness_polls.get() + 4;
        let calls = Cell::new(0);
        let error = view
            .match_raw_inventory(&inventory, deadline(), || {
                calls.set(calls.get() + 1);
                if calls.get() == mutate_at {
                    install.write(&entries);
                }
                false
            })
            .err()
            .expect("trailing source freshness must precede publication");
        assert!(calls.get() > mutate_at);
        match name {
            AREA => assert!(matches!(error, CompositionError::TerrainSource)),
            GROUP => assert!(matches!(
                error,
                CompositionError::Collision(PrepareError::Source(MapBundleError::Source(
                    SourceError::Changed
                )))
            )),
            _ => unreachable!(),
        }
    }
}

#[test]
fn inventory_compatibility_cancellation_and_expiry_never_publish_or_reuse_proof() {
    let install = Install::new(&compatible_entries());
    let inventory = raw_inventory(&install);
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let calls = Cell::new(0);
    let proof = view
        .match_raw_inventory(&inventory, deadline(), || {
            calls.set(calls.get() + 1);
            false
        })
        .unwrap();
    assert!(calls.get() > 4096);
    for stop in [1, calls.get() / 2, calls.get()] {
        let polls = Cell::new(0);
        assert_cancelled(
            view.match_raw_inventory(&inventory, deadline(), || {
                polls.set(polls.get() + 1);
                polls.get() >= stop
            })
            .err()
            .expect("cancelled compatibility cannot publish"),
        );
    }
    assert!(matches!(
        view.match_raw_inventory(&inventory, Instant::now(), || false),
        Err(CompositionError::Validation(Error::Deadline))
    ));
    let owner = inventory.source_provenance().unwrap();
    calls.set(0);
    proof
        .verify_current_owner(owner, deadline(), || {
            calls.set(calls.get() + 1);
            false
        })
        .unwrap();
    for stop in [1, calls.get() / 2, calls.get()] {
        let polls = Cell::new(0);
        assert_cancelled(
            proof
                .verify_current_owner(owner, deadline(), || {
                    polls.set(polls.get() + 1);
                    polls.get() >= stop
                })
                .unwrap_err(),
        );
    }
    assert!(matches!(
        proof.verify_current_owner(owner, Instant::now(), || false),
        Err(CompositionError::Validation(Error::Deadline))
    ));
    proof
        .verify_current_owner(owner, deadline(), || false)
        .unwrap();
}
