// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored client-shaped source -> public capture -> bundle -> unchanged consumers.
//! The pinned original extractor/assembler/query runner supplies the independent oracle.
//! Terrain/MMap inventory scaffolding is authored test data, never gameplay admission.
use super::super::{tests as product_tests, InventorySpec, ProductSpec, TileId};
use super::*;
use benilla_native_vmap::SourceModelName;
use benilla_native_vmap_source::{
    self as source, AssembledSourceMap, DoodadProjectionProfile, MapBundleError, MapProductError,
    PlacementRecordError, PreparedDiscovery, SourceError, SpawnBoundsError,
};
use std::{
    cell::Cell,
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

#[path = "captured_map_archive.rs"]
mod archive;
use archive::{archive, raw_entry, Entry};

const CASE: &str = "captured_wmo";
const TILE: TileId = TileId { x: 31, y: 30 };
const ROOT: &str = "World\\Wmo\\Hull.wmo";
const GROUP: &str = "World\\Wmo\\Hull_000.wmo";
const ADT: &str = "World\\Maps\\Selected\\Selected_30_31.adt";
const NEW_ADT: &str = "World\\Maps\\Selected\\Selected_63_63.adt";
const PRODUCTS: &str = include_str!(
    "../../../../../../../../benilla-native-vmap-source/tools/fixtures/map-products.tsv"
);

struct Install(PathBuf);
impl Install {
    fn new(entries: &[Entry]) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "native-captured-map-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        let install = Self(path);
        install.write(entries);
        install
    }
    fn write(&self, entries: &[Entry]) {
        fs::write(self.0.join("model.MPQ"), archive(entries)).unwrap();
    }
    fn bytes(&self) -> BTreeMap<String, Vec<u8>> {
        fs::read_dir(&self.0)
            .unwrap()
            .map(|entry| {
                let entry = entry.unwrap();
                assert!(entry.file_type().unwrap().is_file(), "no source outputs");
                (
                    entry.file_name().into_string().unwrap(),
                    fs::read(entry.path()).unwrap(),
                )
            })
            .collect()
    }
}
impl Drop for Install {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn fixture(name: &str) -> Vec<u8> {
    fs::read(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-geometry-tests/tools/fixtures/captured_wmo_source")
            .join(name),
    )
    .unwrap()
}
fn chunk(tag: &[u8; 4], payload: &[u8]) -> Vec<u8> {
    let mut bytes: Vec<_> = tag.iter().rev().copied().collect();
    bytes.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    bytes.extend_from_slice(payload);
    bytes
}
fn dbc(fields: u32, row: &[u8], strings: &[u8]) -> Vec<u8> {
    assert_eq!(row.len(), fields as usize * 4);
    let mut bytes = b"WDBC".to_vec();
    for value in [1, fields, fields * 4, strings.len() as u32] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    bytes.extend_from_slice(row);
    bytes.extend_from_slice(strings);
    bytes
}
fn entries() -> Vec<Entry> {
    let mut map = [0; 168];
    map[4..8].copy_from_slice(&1u32.to_le_bytes());
    let mut adt = chunk(b"MVER", &18u32.to_le_bytes());
    adt.extend(chunk(b"MWMO", format!("{ROOT}\0").as_bytes()));
    adt.extend(chunk(b"MODF", &fixture("hull.modf.test-fixture")));
    vec![
        raw_entry("DBFilesClient\\LiquidType.dbc", dbc(4, &[0; 16], &[0])),
        raw_entry("DBFilesClient\\Map.dbc", dbc(42, &map, b"\0Selected\0")),
        raw_entry(
            "World\\Maps\\Selected\\Selected.wdt",
            chunk(b"MVER", &18u32.to_le_bytes()),
        ),
        // Ordinary valid member enumeration only, without a tokenizer oracle.
        raw_entry("(listfile)", format!("{ROOT}\r\n").into_bytes()),
        raw_entry(ROOT, fixture("hull.root.test-fixture")),
        raw_entry(GROUP, fixture("hull_000.group.test-fixture")),
        raw_entry(ADT, adt),
    ]
}
fn profile() -> DoodadProjectionProfile {
    DoodadProjectionProfile::from_policy_id("g3d-portable-libm-0.2.16-v1").unwrap()
}
fn unhex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    value
        .as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn expected_products() -> BTreeMap<String, Vec<u8>> {
    let rows: Vec<_> = PRODUCTS
        .lines()
        .filter(|line| line.split('\t').next() == Some(CASE))
        .collect();
    assert_eq!(rows.len(), 1);
    let fields: Vec<_> = rows[0].split('\t').collect();
    assert_eq!(fields.len(), 5);
    assert_eq!(unhex(fields[1]), fixture("dir_bin"));
    let raw: BTreeMap<String, String> = serde_json::from_str(fields[2]).unwrap();
    assert_eq!(raw.len(), 1);
    assert_eq!(unhex(&raw["Hull.wmo"]), fixture("hull.raw.vmap"));
    let products: BTreeMap<String, String> = serde_json::from_str(fields[3]).unwrap();
    products
        .into_iter()
        .map(|(name, bytes)| (name, unhex(&bytes)))
        .collect()
}
fn assert_capture(prepared: &PreparedDiscovery) {
    assert_eq!(prepared.map_id(), 0);
    let summary = prepared.summary();
    assert_eq!(summary.source_pin, source::SOURCE_PIN);
    assert_eq!(summary.source_epoch, prepared.source_epoch());
    assert_ne!(unhex(&prepared.source_epoch()), [0; 32]);
    assert_eq!(unhex(&prepared.source_epoch()).len(), 32);
    assert_eq!(summary.state, source::MapDiscoveryState::Scanned);
    assert_eq!((summary.archive_count, summary.global_root_events), (1, 1));
    assert_eq!(
        (
            summary.adt_attempts,
            summary.adts_present,
            summary.adts_absent
        ),
        (4096, 1, 4095)
    );
    assert_eq!(
        (
            summary.products,
            summary.wmo_raw_products,
            summary.placements
        ),
        (1, 1, 1)
    );
    assert_eq!(
        (summary.emitting_placements, summary.encounter_id_count),
        (1, 1)
    );
    assert_eq!(
        (summary.wmo_liquid_groups, summary.doodad_candidates),
        (0, 0)
    );
    assert!(prepared.doodad_candidates().is_empty());
    let products = prepared.products();
    assert_eq!(products.len(), 1);
    let product = &products[0];
    assert_eq!(product.kind(), source::ModelKind::Wmo);
    assert_eq!(product.source_member(), ROOT);
    assert_eq!(product.extracted_name(), "Hull.wmo");
    assert_eq!(product.wmo_group_counts(), Some((1, 1)));
    assert!(product.source_has_collision());
    assert_eq!(product.raw_vmap().unwrap(), fixture("hull.raw.vmap"));
    assert_eq!(prepared.placements().len(), 1);
    let placement = &prepared.placements()[0];
    assert_eq!(placement.generated_id(), Some(1));
    assert_eq!(placement.tile(), source::Tile { x: 30, y: 31 });
    assert_eq!(placement.product_index(), 0);
    assert_eq!(placement.disposition(), source::PlacementDisposition::Emits);
    let source::PlacementInput::Wmo(input) = placement.input() else {
        panic!("captured WMO required");
    };
    assert_eq!((input.name_index, input.unique_id), (0, 0x89ab_cdef));
    assert_eq!(input.position, [16100.0, 4.0, 16600.0]);
    assert_eq!(input.rotation, [0.0; 3]);
    assert_eq!(
        input.bounds,
        [[16098.0, 1.0, 16599.0], [16105.0, 10.0, 16604.0]]
    );
    assert_eq!(
        (input.flags, input.doodad_set, input.name_set, input.scale),
        (0, 0, 11, 1024)
    );
}
fn inventory(
    bundle: &AssembledSourceMap<'_>,
    source_epoch: &str,
) -> (InventorySpec, BTreeMap<String, Vec<u8>>) {
    // Revalidate the public source-bound object immediately before consumption.
    bundle.verify_unchanged(|| false).unwrap();
    assert_eq!(bundle.map().map_id(), 0);
    assert_eq!(bundle.map().tiles().len(), 1);
    assert_eq!(bundle.map().tiles()[0].coordinates(), (30, 31));
    assert_eq!(
        bundle.map().spawn_model_names().collect::<Vec<_>>(),
        [b"Hull.wmo"]
    );
    assert_eq!(bundle.models().len(), 1);
    let model = &bundle.models()[0];
    let name = model.source_identity().unwrap();
    assert_eq!(name, SourceModelName::new(b"Hull.wmo").unwrap());
    assert_eq!(model.source_name(), b"Hull.wmo");
    assert_eq!(model.effective_source_basename(), b"Hull.wmo.vmo");
    assert_eq!(
        name.effective_source_basename().unwrap(),
        model.effective_source_basename()
    );
    let actual = BTreeMap::from([
        ("000.vmtree".to_owned(), bundle.map().tree_bytes().to_vec()),
        (
            "000_30_31.vmtile".to_owned(),
            bundle.map().tiles()[0].bytes().to_vec(),
        ),
        ("Hull.wmo.vmo".to_owned(), model.bytes().to_vec()),
    ]);
    assert_eq!(actual, expected_products(), "complete original products");
    let (mut spec, mut files) = product_tests::fixture();
    spec.binding.source_epoch = unhex(source_epoch).try_into().unwrap();
    spec.tiles = vec![TILE];
    spec.products.retain(|product| {
        matches!(product.id, ProductId::MmapParams)
            || matches!(product.id, ProductId::Terrain(tile) | ProductId::MmapTile(tile) if tile == TILE)
    });
    files.retain(|name, _| {
        spec.products
            .iter()
            .any(|p| p.id.filename(0).unwrap() == *name)
    });
    let model_id = ProductId::VmapSourceModel(name.clone());
    for (id, bytes, dependencies) in [
        (ProductId::VmapTree, bundle.map().tree_bytes(), vec![]),
        (
            ProductId::VmapTile(TILE),
            bundle.map().tiles()[0].bytes(),
            vec![ProductId::VmapTree, model_id.clone()],
        ),
        (model_id.clone(), model.bytes(), vec![]),
    ] {
        spec.products.push(ProductSpec {
            id: id.clone(),
            bytes: bytes.len() as u64,
            sha256: Sha256::digest(bytes).into(),
            dependencies,
        });
        assert!(files
            .insert(id.filename(0).unwrap(), bytes.to_vec())
            .is_none());
    }
    let mut digest = Sha256::new();
    digest.update(b"benilla-source-model-name-v1\0");
    digest.update(8u32.to_le_bytes());
    digest.update(b"Hull.wmo");
    assert_eq!(
        model_id.filename(0).unwrap(),
        format!("vmaps/source-{:x}.vmo", digest.finalize())
    );
    assert!(!files.contains_key("vmaps/Hull.wmo.vmo"));
    assert_eq!(spec.products.len(), 6);
    for product in &spec.products {
        let bytes = &files[&product.id.filename(0).unwrap()];
        assert_eq!(product.bytes, bytes.len() as u64);
        assert_eq!(product.sha256, <[u8; 32]>::from(Sha256::digest(bytes)));
    }
    (spec, files)
}

#[test]
fn captured_map_public_source_bundle_matches_original_bytes_and_queries() {
    let install = Install::new(&entries());
    let before = install.bytes();
    let prepared = source::prepare(&install.0, 0, || false).unwrap();
    assert_capture(&prepared);
    prepared.verify_unchanged(|| false).unwrap();
    let records = prepared.placement_records(profile(), || false).unwrap();
    assert_eq!(records.record_count(), 1);
    assert_eq!(records.bytes(), fixture("dir_bin"));
    let summary = prepared.summary();
    let mut digest = Sha256::new();
    for policy in [
        records.policy_id(),
        source::SOURCE_PIN,
        summary.acquisition_policy,
        summary.listfile_policy,
        summary.liquid_header_policy,
        summary.writer_policy,
        summary.encounter_id_policy,
        summary.doodad_reference_order_policy,
        profile().policy_id(),
        &summary.source_epoch,
    ] {
        digest.update(policy.as_bytes());
        digest.update([0]);
    }
    digest.update(records.bytes());
    assert_eq!(
        *records.preprocessing_binding(),
        <[u8; 32]>::from(digest.finalize())
    );
    let bundle = prepared
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(
        bundle.map().source_binding(),
        records.preprocessing_binding()
    );
    let (spec, files) = inventory(&bundle, &prepared.source_epoch());
    let inventoried = product_tests::load(spec, &files).unwrap();
    for model in bundle.models() {
        let id = ProductId::VmapSourceModel(model.source_identity().unwrap());
        assert_eq!(
            inventoried.inventoried_bytes(0, &id).unwrap(),
            model.bytes()
        );
    }
    for id in [ProductId::VmapTree, ProductId::VmapTile(TILE)] {
        assert_eq!(
            inventoried.inventoried_bytes(0, &id).unwrap(),
            files[&id.filename(0).unwrap()]
        );
    }
    assert!(matches!(
        inventoried.inventoried_bytes(0, &ProductId::VmapTile(TileId { x: 30, y: 31 })),
        Err(Error::Missing)
    ));
    let snapshot = inventoried.load_static_vmap().unwrap();
    snapshot.require_complete_dry_map().unwrap();
    assert_eq!(
        snapshot.binding().source_epoch.as_slice(),
        unhex(&prepared.source_epoch())
    );
    assert_eq!((snapshot.models.len(), snapshot.spawns.len()), (1, 1));
    assert_eq!(snapshot.models[0].root_id, 5875);
    assert_eq!(snapshot.models[0].groups.len(), 1);
    assert_eq!(snapshot.models[0].groups[0].id, 23);
    let spawn = snapshot.spawns[0].as_ref().unwrap();
    assert_eq!((spawn.id, spawn.adt, spawn.flags), (1, 11, 4));
    assert_eq!(spawn.classification(), ModelClassification::Wmo);
    assert_eq!(spawn.position, [16600.0, 16100.0, 4.0]);
    super::generated_map_tests::assert_public_queries([(CASE, snapshot)], 66);
    bundle.verify_unchanged(|| false).unwrap();
    let repeated = source::prepare(&install.0, 0, || false).unwrap();
    assert_eq!(repeated.source_epoch(), prepared.source_epoch());
    assert_eq!(
        install.bytes(),
        before,
        "capture, assembly and consumption are read-only"
    );
}

#[test]
fn captured_map_recheck_rejects_changed_source_and_new_previously_absent_adt() {
    for new_adt in [false, true] {
        let mut entries = entries();
        let install = Install::new(&entries);
        let prepared = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = prepared
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        bundle.verify_unchanged(|| false).unwrap();
        if new_adt {
            entries.push(raw_entry(NEW_ADT, chunk(b"MVER", &18u32.to_le_bytes())));
        } else {
            let group = entries
                .iter_mut()
                .find(|entry| entry.name == GROUP)
                .unwrap();
            *group.bytes.last_mut().unwrap() ^= 1;
        }
        install.write(&entries);
        assert!(matches!(
            prepared.verify_unchanged(|| false),
            Err(source::Error::Source(SourceError::Changed))
        ));
        assert!(matches!(
            bundle.verify_unchanged(|| false),
            Err(MapBundleError::Source(SourceError::Changed))
        ));
        assert!(matches!(
            prepared.assemble_map_bundle(profile(), || false),
            Err(MapBundleError::Map(MapProductError::Bounds(
                SpawnBoundsError::Preparation(PlacementRecordError::Source(SourceError::Changed))
            )))
        ));
        assert_eq!(
            prepared.products()[0].raw_vmap().unwrap(),
            fixture("hull.raw.vmap")
        );
        if new_adt {
            let fresh = source::prepare(&install.0, 0, || false).unwrap();
            assert_eq!(fresh.summary().adts_present, 2);
            assert_ne!(fresh.source_epoch(), prepared.source_epoch());
        }
    }
}

#[test]
fn captured_map_missing_group_fails_before_a_bundle_can_exist() {
    let mut entries = entries();
    entries.retain(|entry| entry.name != GROUP);
    let install = Install::new(&entries);
    let before = install.bytes();
    let error = source::prepare(&install.0, 0, || false).unwrap_err();
    assert!(matches!(
        error.category(),
        source::Error::RequiredMemberAbsent
    ));
    assert_eq!(
        error.diagnostic().stage,
        Some(source::PreparationStage::GlobalWmoRoot)
    );
    assert_eq!(
        error.diagnostic().progress.unwrap().complete_raw_products,
        0
    );
    assert_eq!(install.bytes(), before);
}

#[test]
fn captured_map_missing_model_and_exact_name_mismatch_fail_closed() {
    let install = Install::new(&entries());
    let prepared = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = prepared
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let model = ProductId::VmapSourceModel(bundle.models()[0].source_identity().unwrap());
    let (spec, mut files) = inventory(&bundle, &prepared.source_epoch());
    files.remove(&model.filename(0).unwrap());
    assert!(matches!(
        product_tests::load(spec, &files),
        Err(Error::Missing)
    ));

    let (mut spec, mut files) = inventory(&bundle, &prepared.source_epoch());
    product_tests::mutate(&mut spec, &mut files, &ProductId::VmapTile(TILE), |bytes| {
        let length = u32::from_le_bytes(bytes[74..78].try_into().unwrap()) as usize;
        assert_eq!(&bytes[78..78 + length], b"Hull.wmo");
        bytes.insert(78 + length, 0);
        bytes[74..78].copy_from_slice(&((length + 1) as u32).to_le_bytes());
    });
    assert!(
        matches!(product_tests::load(spec, &files), Err(Error::Dependency)),
        "terminal NUL changes the exact captured dependency identity"
    );
}

#[test]
fn captured_map_altered_spawn_and_tree_bounds_fail_closed() {
    let install = Install::new(&entries());
    let prepared = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = prepared
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    for (id, offset, value) in [
        // Exclude the captured model's x=-1 vertex after placement.
        (ProductId::VmapTile(TILE), 50, 16600.0f32),
        // Exclude the complete captured spawn's high x bound from the tree.
        (ProductId::VmapTree, 25, 16603.0f32),
    ] {
        let (mut spec, mut files) = inventory(&bundle, &prepared.source_epoch());
        product_tests::mutate(&mut spec, &mut files, &id, |bytes| {
            bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        });
        let inventoried = product_tests::load(spec, &files).unwrap();
        assert!(
            matches!(inventoried.load_static_vmap(), Err(Error::Format)),
            "{id:?}"
        );
    }
}

fn bundle_cancelled(error: &MapBundleError) -> bool {
    matches!(
        error,
        MapBundleError::Cancelled
            | MapBundleError::Source(SourceError::Cancelled)
            | MapBundleError::Map(
                MapProductError::Cancelled
                    | MapProductError::Source(SourceError::Cancelled)
                    | MapProductError::Index(source::MapSpawnIndexError::Cancelled)
                    | MapProductError::Bih(benilla_native_vmap::BihBuildError::Cancelled)
                    | MapProductError::Bounds(
                        SpawnBoundsError::Cancelled
                            | SpawnBoundsError::Source(SourceError::Cancelled)
                            | SpawnBoundsError::Index(source::MapSpawnIndexError::Cancelled)
                            | SpawnBoundsError::Preparation(
                                PlacementRecordError::Cancelled
                                    | PlacementRecordError::Source(SourceError::Cancelled)
                            )
                    )
            )
    )
}

#[test]
fn captured_map_cancellation_at_entry_and_during_prepare_assembly_and_recheck() {
    let install = Install::new(&entries());
    let before = install.bytes();
    let count = Cell::new(0);
    let prepared = source::prepare(&install.0, 0, || {
        count.set(count.get() + 1);
        false
    })
    .unwrap();
    assert!(count.get() > 4096);
    for cutoff in [0, count.get() / 2, count.get() - 1] {
        let polls = Cell::new(0);
        let error = source::prepare(&install.0, 0, || {
            let n = polls.get();
            polls.set(n + 1);
            n >= cutoff
        })
        .unwrap_err();
        assert!(
            matches!(
                error.category(),
                source::Error::Cancelled | source::Error::Source(SourceError::Cancelled)
            ),
            "{cutoff}: {error:?}"
        );
    }
    count.set(0);
    let bundle = prepared
        .assemble_map_bundle(profile(), || {
            count.set(count.get() + 1);
            false
        })
        .unwrap()
        .unwrap();
    assert!(count.get() > 4096);
    for cutoff in [0, count.get() / 2, count.get() - 1] {
        let polls = Cell::new(0);
        let error = prepared
            .assemble_map_bundle(profile(), || {
                let n = polls.get();
                polls.set(n + 1);
                n >= cutoff
            })
            .unwrap_err();
        assert!(bundle_cancelled(&error), "{cutoff}: {error:?}");
    }
    count.set(0);
    bundle
        .verify_unchanged(|| {
            count.set(count.get() + 1);
            false
        })
        .unwrap();
    assert!(count.get() > 4096);
    for cutoff in [0, count.get() / 2, count.get() - 1] {
        let polls = Cell::new(0);
        let error = bundle
            .verify_unchanged(|| {
                let n = polls.get();
                polls.set(n + 1);
                n >= cutoff
            })
            .unwrap_err();
        assert!(bundle_cancelled(&error), "{cutoff}: {error:?}");
    }
    bundle.verify_unchanged(|| false).unwrap();
    assert_eq!(install.bytes(), before);
}
