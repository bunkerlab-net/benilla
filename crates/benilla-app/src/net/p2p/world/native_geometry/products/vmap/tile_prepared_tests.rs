// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored MPQ capture -> source-bound tile residency -> unchanged public queries.
//! Residency never supplies terrain/nav evidence or turns an unavailable slot into a miss.
use super::super::{
    generated_map_tests::{assert_public_queries, PublicQueries},
    prepared::{PrepareError, PreparedCollision},
    tile_prepared::PreparedTileCollision,
};
use super::*;
use std::time::Duration;

const OTHER_TILE: TileId = TileId { x: 31, y: 31 };
const OTHER_ADT: &str = "World\\Maps\\Selected\\Selected_31_31.adt";
const EMPTY_TILE: TileId = TileId { x: 63, y: 63 };

impl PublicQueries for &PreparedTileCollision<'_, '_> {
    fn binding(&self) -> SnapshotBinding {
        PreparedTileCollision::binding(self)
    }
    fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error> {
        PreparedTileCollision::query(self, request, deadline)
    }
    fn query_height(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        PreparedTileCollision::query_height(self, request, deadline)
    }
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn requests(binding: SnapshotBinding) -> (RayRequest, HeightRequest) {
    // Exact public hit_top/hit_bottom bits from the pinned original query oracle.
    let position = [0x43e8_d540, 0x4471_aaa0, 20.0f32.to_bits()];
    (
        RayRequest {
            snapshot: binding,
            ticket: 301,
            from: position,
            to: [position[0], position[1], 0.0f32.to_bits()],
            ignore_m2: false,
        },
        HeightRequest {
            snapshot: binding,
            ticket: 302,
            position,
            max_search: 20.0f32.to_bits(),
        },
    )
}

fn assert_hits(view: &PreparedTileCollision<'_, '_>) -> (RayReply, HeightReply) {
    let (ray, height) = requests(view.binding());
    let ray_reply = view.query(ray, deadline()).unwrap();
    assert_eq!(
        ray_reply,
        RayReply {
            request: ray,
            result: StaticRay::Hit {
                distance: 15.0f32.to_bits(),
            },
        }
    );
    let height_reply = view.query_height(height, deadline()).unwrap();
    assert_eq!(
        height_reply,
        HeightReply {
            request: height,
            height: 5.0f32.to_bits(),
        }
    );
    (ray_reply, height_reply)
}

fn assert_missing(view: &PreparedTileCollision<'_, '_>) {
    let (ray, height) = requests(view.binding());
    assert_eq!(view.query(ray, deadline()), Err(Error::Missing));
    assert_eq!(view.query_height(height, deadline()), Err(Error::Missing));
}

fn assert_foreign_binding(view: &PreparedTileCollision<'_, '_>, binding: SnapshotBinding) {
    let (ray, height) = requests(binding);
    assert_eq!(view.query(ray, deadline()), Err(Error::Identity));
    assert_eq!(view.query_height(height, deadline()), Err(Error::Identity));
}

#[test]
fn tile_collision_public_capture_matches_all_original_queries_without_inventory() {
    let install = Install::new(&entries());
    let before = install.bytes();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    assert_capture(&capture);
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
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
    assert_eq!(bundle.map().tiles().len(), 1);
    assert_eq!(bundle.map().tiles()[0].coordinates(), (30, 31));
    assert_eq!(bundle.models().len(), 1);
    assert_eq!(
        actual,
        expected_products(),
        "unchanged original collision products"
    );

    let view = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    assert_eq!(view.resident_tiles(), [TILE]);
    assert_eq!(view.binding().map, 0);
    assert_eq!(view.source_version(), *bundle.map().source_binding());
    assert_eq!(view.binding().source_epoch, view.source_version());
    for hash in [
        view.binding().source_epoch,
        view.binding().content,
        view.binding().inventory,
        view.binding().policy,
    ] {
        assert_ne!(hash, [0; 32]);
    }
    view.verify_unchanged(|| false).unwrap();
    assert_hits(&view);
    assert_public_queries([(CASE, &view)], 66);

    let repeated = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    assert_eq!(repeated.binding(), view.binding());
    assert_eq!(repeated.source_version(), view.source_version());
    assert_eq!(assert_hits(&repeated), assert_hits(&view));
    let whole_map = PreparedCollision::from_source(&bundle, || false).unwrap();
    assert_eq!(whole_map.source_version(), view.source_version());
    assert_ne!(whole_map.binding(), view.binding());
    assert_foreign_binding(&view, whole_map.binding());
    let (ray, height) = requests(view.binding());
    assert_eq!(whole_map.query(ray, deadline()), Err(Error::Identity));
    assert_eq!(
        whole_map.query_height(height, deadline()),
        Err(Error::Identity)
    );
    assert_eq!(install.bytes(), before, "source consumption is read-only");
}

#[test]
fn tile_collision_unloaded_required_slots_fail_closed_but_pruned_rays_can_miss() {
    let install = Install::new(&entries());
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    // The transposed ADT pair must not silently select the loaded terrain-grid tile.
    for selected in [vec![], vec![TileId { x: 30, y: 31 }], vec![EMPTY_TILE]] {
        let view = PreparedTileCollision::from_source(&bundle, &selected, || false).unwrap();
        assert_eq!(view.resident_tiles(), selected);
        assert_missing(&view);
        let (mut ray, mut height) = requests(view.binding());
        ray.from[0] = 10_000.0f32.to_bits();
        ray.to[0] = ray.from[0];
        height.position[0] = ray.from[0];
        // Source BIH root clipping skips parallel axes, so even this horizontal
        // offset reaches the unloaded leaf while the vertical range overlaps.
        assert_eq!(view.query(ray, deadline()), Err(Error::Missing));
        assert_eq!(view.query_height(height, deadline()), Err(Error::Missing));
        // A nonparallel z clip before the root bounds can prove a genuine miss.
        ray.to[2] = 15.0f32.to_bits();
        height.max_search = 5.0f32.to_bits();
        assert_eq!(
            view.query(ray, deadline()).unwrap(),
            RayReply {
                request: ray,
                result: StaticRay::Miss,
            }
        );
        assert_eq!(
            view.query_height(height, deadline()).unwrap(),
            HeightReply {
                request: height,
                height: (-200_000.0f32).to_bits(),
            }
        );
        view.verify_unchanged(|| false).unwrap();
    }
}

#[test]
fn tile_collision_selection_is_canonical_unique_and_part_of_request_identity() {
    let install = Install::new(&entries());
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    for selected in [vec![TILE, TILE], vec![EMPTY_TILE, TILE, EMPTY_TILE]] {
        assert!(matches!(
            PreparedTileCollision::from_source(&bundle, &selected, || false),
            Err(PrepareError::Validation(Error::Duplicate))
        ));
    }
    for tile in [TileId { x: 64, y: 0 }, TileId { x: 0, y: 64 }] {
        assert!(matches!(
            PreparedTileCollision::from_source(&bundle, &[tile], || false),
            Err(PrepareError::Validation(Error::Identity))
        ));
    }
    let loaded = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    let empty = PreparedTileCollision::from_source(&bundle, &[], || false).unwrap();
    let extended =
        PreparedTileCollision::from_source(&bundle, &[EMPTY_TILE, TILE], || false).unwrap();
    let ordered =
        PreparedTileCollision::from_source(&bundle, &[TILE, EMPTY_TILE], || false).unwrap();
    assert_eq!(extended.resident_tiles(), [TILE, EMPTY_TILE]);
    assert_eq!(extended.binding(), ordered.binding());
    assert_eq!(extended.source_version(), loaded.source_version());
    assert_eq!(empty.source_version(), loaded.source_version());
    for (a, b) in [(&loaded, &empty), (&loaded, &extended), (&empty, &extended)] {
        assert_ne!(a.binding(), b.binding());
        assert_foreign_binding(a, b.binding());
        assert_foreign_binding(b, a.binding());
    }
    assert_hits(&loaded);
    assert_hits(&extended);
    assert_missing(&empty);
    let reloaded = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    assert_eq!(reloaded.binding(), loaded.binding());
    assert_eq!(assert_hits(&reloaded), assert_hits(&loaded));
    assert_missing(&empty);
}

fn two_tile_entries(shared_spawn: bool) -> Vec<Entry> {
    let mut source_entries = entries();
    let mut record = fixture("hull.modf.test-fixture");
    assert_eq!(record.len(), 64);
    if !shared_spawn {
        record[4..8].copy_from_slice(&0x89ab_cdf0u32.to_le_bytes());
        // Translate the authored placement and both source-space bounds together.
        // Source x becomes internal y, placing this second instance in ADT 31,31.
        for offset in [8, 32, 44] {
            let value = f32::from_le_bytes(record[offset..offset + 4].try_into().unwrap());
            record[offset..offset + 4].copy_from_slice(&(value + 600.0).to_le_bytes());
        }
    }
    let mut adt = chunk(b"MVER", &18u32.to_le_bytes());
    adt.extend(chunk(b"MWMO", format!("{ROOT}\0").as_bytes()));
    adt.extend(chunk(b"MODF", &record));
    source_entries.push(raw_entry(OTHER_ADT, adt));
    source_entries
}

#[test]
fn tile_collision_shared_first_wins_spawn_survives_either_resident_tile_and_reload() {
    let install = Install::new(&two_tile_entries(true));
    let before = install.bytes();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    assert_eq!(capture.placements().len(), 2);
    assert_eq!(capture.placements()[0].generated_id(), Some(1));
    assert_eq!(capture.placements()[1].generated_id(), Some(1));
    let records = capture.placement_records(profile(), || false).unwrap();
    let index = records.map_spawn_index(|| false).unwrap();
    assert_eq!(index.unique_count(), 1);
    assert_eq!(index.tile_reference_count(), 2);
    assert_eq!(
        index.tile_references().collect::<Vec<_>>(),
        [((30 << 16) | 31, 1), ((31 << 16) | 31, 1)]
    );
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(bundle.map().tiles().len(), 2);
    assert_eq!(bundle.models().len(), 1);
    assert_eq!(bundle.map().spawn_model_names().count(), 1);
    assert_eq!(
        bundle.map().tiles()[0].bytes(),
        bundle.map().tiles()[1].bytes()
    );
    assert_eq!(bundle.map().tree_bytes(), expected_products()["000.vmtree"]);
    let left = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    let right = PreparedTileCollision::from_source(&bundle, &[OTHER_TILE], || false).unwrap();
    let both = PreparedTileCollision::from_source(&bundle, &[OTHER_TILE, TILE], || false).unwrap();
    let neither = PreparedTileCollision::from_source(&bundle, &[], || false).unwrap();
    assert_eq!(both.resident_tiles(), [TILE, OTHER_TILE]);
    assert_public_queries([(CASE, &left), (CASE, &right), (CASE, &both)], 198);
    assert_missing(&neither);
    for (a, b) in [(&left, &right), (&left, &both), (&right, &both)] {
        assert_ne!(a.binding(), b.binding());
        assert_foreign_binding(a, b.binding());
        assert_foreign_binding(b, a.binding());
    }
    drop(both);
    drop(left);
    assert_hits(&right);
    let reloaded = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    assert_public_queries([(CASE, &reloaded)], 66);
    assert_missing(&neither);
    assert_eq!(install.bytes(), before);
}

#[test]
fn tile_collision_resident_shared_model_does_not_load_another_tiles_distinct_spawn() {
    let install = Install::new(&two_tile_entries(false));
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    assert_eq!(capture.placements().len(), 2);
    assert_eq!(capture.placements()[0].generated_id(), Some(1));
    assert_eq!(capture.placements()[1].generated_id(), Some(2));
    let records = capture.placement_records(profile(), || false).unwrap();
    let index = records.map_spawn_index(|| false).unwrap();
    assert_eq!(index.unique_count(), 2);
    assert_eq!(index.tile_reference_count(), 2);
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    assert_eq!(bundle.models().len(), 1);
    assert_eq!(bundle.map().spawn_model_names().count(), 2);
    let left = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
    let right = PreparedTileCollision::from_source(&bundle, &[OTHER_TILE], || false).unwrap();
    let both = PreparedTileCollision::from_source(&bundle, &[TILE, OTHER_TILE], || false).unwrap();
    assert_missing(&right);
    // Both primitives share the source's coarse BIH leaf. Even a loaded hit
    // remains unavailable if traversal also requires its unloaded leaf sibling.
    assert_missing(&left);
    assert_public_queries([(CASE, &both)], 66);
    let (mut ray, mut height) = requests(both.binding());
    ray.from[1] = (f32::from_bits(ray.from[1]) - 600.0).to_bits();
    ray.to[1] = ray.from[1];
    height.position[1] = ray.from[1];
    assert_eq!(
        both.query(ray, deadline()).unwrap(),
        RayReply {
            request: ray,
            result: StaticRay::Hit {
                distance: 15.0f32.to_bits(),
            },
        }
    );
    assert_eq!(
        both.query_height(height, deadline()).unwrap(),
        HeightReply {
            request: height,
            height: 5.0f32.to_bits(),
        }
    );
    ray.snapshot = left.binding();
    height.snapshot = left.binding();
    assert_eq!(left.query(ray, deadline()), Err(Error::Missing));
    assert_eq!(left.query_height(height, deadline()), Err(Error::Missing));
    // Reconstructing another resident set cannot mutate either older view.
    let reloaded =
        PreparedTileCollision::from_source(&bundle, &[OTHER_TILE, TILE], || false).unwrap();
    assert_eq!(reloaded.binding(), both.binding());
    assert_hits(&reloaded);
    assert_missing(&right);
    assert_eq!(left.query(ray, deadline()), Err(Error::Missing));
}

#[test]
fn tile_collision_rejects_wrong_bindings_zero_tickets_and_expired_deadlines() {
    let install = Install::new(&entries());
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    for selected in [vec![TILE], vec![]] {
        let view = PreparedTileCollision::from_source(&bundle, &selected, || false).unwrap();
        let binding = view.binding();
        let mut wrong = [binding; 5];
        wrong[0].map += 1;
        wrong[1].source_epoch[0] ^= 1;
        wrong[2].content[0] ^= 1;
        wrong[3].inventory[0] ^= 1;
        wrong[4].policy[0] ^= 1;
        for binding in wrong {
            assert_foreign_binding(&view, binding);
        }
        let (mut ray, mut height) = requests(binding);
        ray.ticket = 0;
        height.ticket = 0;
        assert_eq!(view.query(ray, deadline()), Err(Error::Identity));
        assert_eq!(view.query_height(height, deadline()), Err(Error::Identity));
        let (ray, height) = requests(binding);
        assert_eq!(view.query(ray, Instant::now()), Err(Error::Deadline));
        assert_eq!(
            view.query_height(height, Instant::now()),
            Err(Error::Deadline)
        );
    }
}

#[test]
fn tile_collision_source_changes_reject_rebinding_but_keep_historical_residency() {
    for change in ["unloaded_new_adt", "root_ambient_color", "group_bytes"] {
        let mut source_entries = entries();
        let install = Install::new(&source_entries);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let loaded = PreparedTileCollision::from_source(&bundle, &[TILE], || false).unwrap();
        let empty = PreparedTileCollision::from_source(&bundle, &[], || false).unwrap();
        let original_binding = loaded.binding();
        let original_hits = assert_hits(&loaded);
        match change {
            "unloaded_new_adt" => {
                source_entries.push(raw_entry(NEW_ADT, chunk(b"MVER", &18u32.to_le_bytes())));
            }
            "root_ambient_color" => {
                let root = source_entries
                    .iter_mut()
                    .find(|entry| entry.name == ROOT)
                    .unwrap();
                assert_eq!(&root.bytes[12..16], b"DHOM");
                root.bytes[20 + 28] ^= 1;
            }
            "group_bytes" => {
                let group = source_entries
                    .iter_mut()
                    .find(|entry| entry.name == GROUP)
                    .unwrap();
                *group.bytes.last_mut().unwrap() ^= 1;
            }
            _ => unreachable!(),
        }
        install.write(&source_entries);
        let changed = install.bytes();
        for view in [&loaded, &empty] {
            assert!(matches!(
                view.verify_unchanged(|| false),
                Err(PrepareError::Source(MapBundleError::Source(
                    SourceError::Changed
                )))
            ));
        }
        for selected in [vec![], vec![TILE], vec![EMPTY_TILE]] {
            assert!(matches!(
                PreparedTileCollision::from_source(&bundle, &selected, || false),
                Err(PrepareError::Source(MapBundleError::Source(
                    SourceError::Changed
                )))
            ));
        }
        assert_eq!(loaded.binding(), original_binding);
        assert_eq!(loaded.resident_tiles(), [TILE]);
        assert_eq!(assert_hits(&loaded), original_hits);
        assert_missing(&empty);
        if change != "group_bytes" {
            let fresh = source::prepare(&install.0, 0, || false).unwrap();
            let fresh_bundle = fresh
                .assemble_map_bundle(profile(), || false)
                .unwrap()
                .unwrap();
            assert_eq!(fresh_bundle.map().tree_bytes(), bundle.map().tree_bytes());
            assert_eq!(fresh_bundle.models()[0].bytes(), bundle.models()[0].bytes());
            let fresh_view =
                PreparedTileCollision::from_source(&fresh_bundle, &[TILE], || false).unwrap();
            assert_ne!(fresh_view.source_version(), loaded.source_version());
            assert_ne!(fresh_view.binding(), original_binding);
            assert_foreign_binding(&fresh_view, original_binding);
            assert_foreign_binding(&loaded, fresh_view.binding());
            fresh_view.verify_unchanged(|| false).unwrap();
            assert_hits(&fresh_view);
        }
        assert_eq!(
            install.bytes(),
            changed,
            "rechecks and queries never rewrite source"
        );
    }
}

fn cancelled(error: &PrepareError) -> bool {
    match error {
        PrepareError::Cancelled => true,
        PrepareError::Source(error) => bundle_cancelled(error),
        PrepareError::Validation(_) => false,
    }
}

#[test]
fn tile_collision_cancels_construction_and_freshness_for_loaded_and_empty_views() {
    let install = Install::new(&entries());
    let before = install.bytes();
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    for selected in [vec![TILE], vec![]] {
        let polls = Cell::new(0);
        let view = PreparedTileCollision::from_source(&bundle, &selected, || {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        let construction_polls = polls.replace(0);
        assert!(construction_polls > 8192);
        for cutoff in [0, 1, construction_polls / 2, construction_polls - 1] {
            polls.set(0);
            let error = PreparedTileCollision::from_source(&bundle, &selected, || {
                let n = polls.get();
                polls.set(n + 1);
                n >= cutoff
            })
            .unwrap_err();
            assert!(cancelled(&error), "construction poll {cutoff}: {error:?}");
            assert_eq!(polls.get(), cutoff + 1);
        }
        polls.set(0);
        view.verify_unchanged(|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        let freshness_polls = polls.replace(0);
        assert!(freshness_polls > 4096);
        for cutoff in [0, 1, freshness_polls / 2, freshness_polls - 1] {
            polls.set(0);
            let error = view
                .verify_unchanged(|| {
                    let n = polls.get();
                    polls.set(n + 1);
                    n >= cutoff
                })
                .unwrap_err();
            assert!(cancelled(&error), "freshness poll {cutoff}: {error:?}");
            assert_eq!(polls.get(), cutoff + 1);
        }
        view.verify_unchanged(|| false).unwrap();
        if selected.is_empty() {
            assert_missing(&view);
        } else {
            assert_hits(&view);
        }
    }
    assert_eq!(install.bytes(), before);
}

#[test]
fn tile_collision_rechecks_source_after_selection_before_publication() {
    let mut source_entries = entries();
    let install = Install::new(&source_entries);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let polls = Cell::new(0);
    bundle
        .verify_unchanged(|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
    let source_polls = polls.replace(0);
    source_entries.push(raw_entry(NEW_ADT, chunk(b"MVER", &18u32.to_le_bytes())));
    let changed = Cell::new(false);
    let error = PreparedTileCollision::from_source(&bundle, &[TILE], || {
        let n = polls.get();
        polls.set(n + 1);
        if n == source_polls + 1 {
            install.write(&source_entries);
            changed.set(true);
        }
        false
    })
    .unwrap_err();
    assert!(changed.get(), "mutation follows the initial source recheck");
    assert!(matches!(
        error,
        PrepareError::Source(MapBundleError::Source(SourceError::Changed))
    ));
}
