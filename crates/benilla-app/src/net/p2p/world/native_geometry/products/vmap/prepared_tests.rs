// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine authored MPQ -> capture -> bundle -> historical collision-only view.
//! No inventory helper or terrain/nav evidence enters this construction path.
use super::super::{
    generated_map_tests::{assert_public_queries, PublicQueries},
    prepared::{PrepareError, PreparedCollision},
};
use super::*;
use std::time::Duration;

impl PublicQueries for &PreparedCollision<'_, '_> {
    fn binding(&self) -> SnapshotBinding {
        PreparedCollision::binding(self)
    }
    fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error> {
        PreparedCollision::query(self, request, deadline)
    }
    fn query_height(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        PreparedCollision::query_height(self, request, deadline)
    }
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn requests(binding: SnapshotBinding) -> (RayRequest, HeightRequest) {
    // Exact public hit_top/hit_bottom coordinates from the pinned original oracle.
    let position = [0x43e8_d540, 0x4471_aaa0, 20.0f32.to_bits()];
    (
        RayRequest {
            snapshot: binding,
            ticket: 101,
            from: position,
            to: [position[0], position[1], 0.0f32.to_bits()],
            ignore_m2: false,
        },
        HeightRequest {
            snapshot: binding,
            ticket: 102,
            position,
            max_search: 20.0f32.to_bits(),
        },
    )
}

fn assert_hits(view: &PreparedCollision<'_, '_>) -> (RayReply, HeightReply) {
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

// Called only by the existing inventory test. The prepared path itself never
// receives that inventory and cannot convert its distinct view into one.
pub(super) fn assert_distinct_inventory_binding(
    bundle: &AssembledSourceMap<'_>,
    inventory_snapshot: &VmapSnapshot,
) {
    let view = PreparedCollision::from_source(bundle, || false).unwrap();
    let collision = view.binding();
    let inventory = inventory_snapshot.binding();
    assert_eq!(collision.map, inventory.map);
    assert_eq!(view.source_version(), *bundle.map().source_binding());
    assert_eq!(collision.source_epoch, view.source_version());
    assert_ne!(collision.source_epoch, inventory.source_epoch);
    assert_ne!(collision.content, inventory.content);
    assert_ne!(collision.inventory, inventory.inventory);
    assert_ne!(collision.policy, inventory.policy);
    assert_ne!(
        collision.policy,
        super::super::dry_policy(collision.inventory)
    );
    let (ray, height) = requests(inventory);
    assert_eq!(view.query(ray, deadline()), Err(Error::Identity));
    assert_eq!(view.query_height(height, deadline()), Err(Error::Identity));
    let (ray, height) = requests(collision);
    assert_eq!(
        inventory_snapshot.query(ray, deadline()),
        Err(Error::Identity)
    );
    assert_eq!(
        inventory_snapshot.query_height(height, deadline()),
        Err(Error::Identity)
    );
    assert_hits(&view);
}

#[test]
fn prepared_collision_public_capture_matches_all_original_queries_without_inventory() {
    let install = Install::new(&entries());
    let before = install.bytes();
    assert_eq!(
        before.keys().map(String::as_str).collect::<Vec<_>>(),
        ["model.MPQ"]
    );
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
        "original collision products only"
    );
    let view = PreparedCollision::from_source(&bundle, || false).unwrap();
    let binding = view.binding();
    assert_eq!(binding.map, 0);
    assert_eq!(binding.source_epoch, view.source_version());
    assert_eq!(view.source_version(), *bundle.map().source_binding());
    assert_eq!(
        view.source_version(),
        *capture
            .placement_records(profile(), || false)
            .unwrap()
            .preprocessing_binding()
    );
    assert_ne!(
        view.source_version().as_slice(),
        unhex(&capture.source_epoch())
    );
    for hash in [
        binding.source_epoch,
        binding.content,
        binding.inventory,
        binding.policy,
    ] {
        assert_ne!(hash, [0; 32]);
    }
    view.verify_unchanged(|| false).unwrap();
    assert_hits(&view);
    assert_public_queries([(CASE, &view)], 66);
    let repeated = PreparedCollision::from_source(&bundle, || false).unwrap();
    assert_eq!(repeated.binding(), binding);
    assert_eq!(repeated.source_version(), view.source_version());
    assert_eq!(assert_hits(&repeated), assert_hits(&view));
    assert_eq!(install.bytes(), before, "source consumption is read-only");
}

#[test]
fn prepared_collision_rejects_every_wrong_binding_field_zero_tickets_and_deadlines() {
    let install = Install::new(&entries());
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let view = PreparedCollision::from_source(&bundle, || false).unwrap();
    assert_hits(&view);
    let binding = view.binding();
    let mut wrong = [binding; 5];
    wrong[0].map += 1;
    wrong[1].source_epoch[0] ^= 1;
    wrong[2].content[0] ^= 1;
    wrong[3].inventory[0] ^= 1;
    wrong[4].policy[0] ^= 1;
    for binding in wrong {
        let (ray, height) = requests(binding);
        assert_eq!(view.query(ray, deadline()), Err(Error::Identity));
        assert_eq!(view.query_height(height, deadline()), Err(Error::Identity));
    }
    let (mut ray, mut height) = requests(binding);
    ray.ticket = 0;
    height.ticket = 0;
    assert_eq!(view.query(ray, deadline()), Err(Error::Identity));
    assert_eq!(view.query_height(height, deadline()), Err(Error::Identity));
    for miss in [false, true] {
        let (mut ray, mut height) = requests(binding);
        if miss {
            ray.to[2] = 15.0f32.to_bits();
            height.max_search = 5.0f32.to_bits();
            assert_eq!(view.query(ray, deadline()).unwrap().result, StaticRay::Miss);
            assert_eq!(
                view.query_height(height, deadline()).unwrap().height,
                (-200_000.0f32).to_bits()
            );
        }
        assert_eq!(view.query(ray, Instant::now()), Err(Error::Deadline));
        assert_eq!(
            view.query_height(height, Instant::now()),
            Err(Error::Deadline)
        );
    }
    assert_hits(&view);
}

#[test]
fn prepared_collision_source_changes_fail_freshness_but_preserve_historical_queries() {
    for change in ["group_bytes", "root_ambient_color", "new_adt"] {
        let mut source_entries = entries();
        let install = Install::new(&source_entries);
        let capture = source::prepare(&install.0, 0, || false).unwrap();
        let bundle = capture
            .assemble_map_bundle(profile(), || false)
            .unwrap()
            .unwrap();
        let view = PreparedCollision::from_source(&bundle, || false).unwrap();
        view.verify_unchanged(|| false).unwrap();
        let original_binding = view.binding();
        let original_version = view.source_version();
        let original_hits = assert_hits(&view);
        match change {
            "new_adt" => {
                source_entries.push(raw_entry(NEW_ADT, chunk(b"MVER", &18u32.to_le_bytes())));
            }
            "root_ambient_color" => {
                let root = source_entries
                    .iter_mut()
                    .find(|entry| entry.name == ROOT)
                    .unwrap();
                assert_eq!(&root.bytes[12..16], b"DHOM");
                // MOHD ambient color is read source data, ignored by collision extraction.
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
        assert!(matches!(
            view.verify_unchanged(|| false),
            Err(PrepareError::Source(MapBundleError::Source(
                SourceError::Changed
            )))
        ));
        assert!(matches!(
            PreparedCollision::from_source(&bundle, || false),
            Err(PrepareError::Source(MapBundleError::Source(
                SourceError::Changed
            )))
        ));
        assert_eq!(view.binding(), original_binding);
        assert_eq!(view.source_version(), original_version);
        assert_eq!(assert_hits(&view), original_hits);
        assert_public_queries([(CASE, &view)], 66);
        if change != "group_bytes" {
            let fresh = source::prepare(&install.0, 0, || false).unwrap();
            assert_ne!(fresh.source_epoch(), capture.source_epoch());
            assert_eq!(
                fresh.products()[0].raw_vmap().unwrap(),
                capture.products()[0].raw_vmap().unwrap()
            );
            let fresh_bundle = fresh
                .assemble_map_bundle(profile(), || false)
                .unwrap()
                .unwrap();
            let fresh_view = PreparedCollision::from_source(&fresh_bundle, || false).unwrap();
            assert_ne!(fresh_view.source_version(), original_version);
            assert_ne!(
                fresh_view.binding().source_epoch,
                original_binding.source_epoch
            );
            assert_ne!(fresh_view.binding().inventory, original_binding.inventory);
            assert_ne!(fresh_view.binding().policy, original_binding.policy);
            assert_eq!(fresh_bundle.map().tree_bytes(), bundle.map().tree_bytes());
            assert_eq!(fresh_bundle.map().tiles().len(), bundle.map().tiles().len());
            assert_eq!(
                fresh_bundle.map().tiles()[0].bytes(),
                bundle.map().tiles()[0].bytes()
            );
            assert_eq!(fresh_bundle.models()[0].bytes(), bundle.models()[0].bytes());
            assert_hits(&fresh_view);
            fresh_view.verify_unchanged(|| false).unwrap();
            for (target, foreign) in [
                (&view, fresh_view.binding()),
                (&fresh_view, original_binding),
            ] {
                let (ray, height) = requests(foreign);
                assert_eq!(target.query(ray, deadline()), Err(Error::Identity));
                assert_eq!(
                    target.query_height(height, deadline()),
                    Err(Error::Identity)
                );
            }
        }
        assert_eq!(
            install.bytes(),
            changed,
            "queries and rechecks never rewrite the install"
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
fn prepared_collision_cancels_at_observed_construction_and_freshness_stages() {
    let install = Install::new(&entries());
    let before = install.bytes();
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
    assert!(source_polls > 4096);
    let view = PreparedCollision::from_source(&bundle, || {
        polls.set(polls.get() + 1);
        false
    })
    .unwrap();
    let construction_polls = polls.replace(0);
    assert!(construction_polls > 2 * source_polls + 2);
    // Exercise the entry, first recheck, collision-row work, final recheck and
    // final publication guard, using the poll counts actually observed above.
    let mut direct = 0;
    let mut wrapped = 0;
    for cutoff in [
        0,
        1,
        source_polls / 2,
        source_polls,
        source_polls + 1,
        construction_polls / 2,
        construction_polls - source_polls - 2,
        construction_polls - source_polls,
        construction_polls - 2,
        construction_polls - 1,
    ] {
        polls.set(0);
        let error = PreparedCollision::from_source(&bundle, || {
            let n = polls.get();
            polls.set(n + 1);
            n >= cutoff
        })
        .unwrap_err();
        assert!(cancelled(&error), "construction poll {cutoff}: {error:?}");
        assert_eq!(polls.get(), cutoff + 1, "construction poll {cutoff}");
        direct += usize::from(matches!(error, PrepareError::Cancelled));
        wrapped += usize::from(matches!(error, PrepareError::Source(_)));
    }
    assert!(
        direct >= 3,
        "construction must poll outside source rechecks"
    );
    assert!(
        wrapped >= 3,
        "construction must honor source-recheck cancellation"
    );
    polls.set(0);
    view.verify_unchanged(|| {
        polls.set(polls.get() + 1);
        false
    })
    .unwrap();
    let freshness_polls = polls.replace(0);
    assert_eq!(freshness_polls, source_polls + 2);
    for cutoff in [
        0,
        1,
        freshness_polls / 2,
        freshness_polls - 2,
        freshness_polls - 1,
    ] {
        polls.set(0);
        let error = view
            .verify_unchanged(|| {
                let n = polls.get();
                polls.set(n + 1);
                n >= cutoff
            })
            .unwrap_err();
        assert!(cancelled(&error), "freshness poll {cutoff}: {error:?}");
        assert_eq!(polls.get(), cutoff + 1, "freshness poll {cutoff}");
    }
    view.verify_unchanged(|| false).unwrap();
    assert_hits(&view);
    assert_eq!(install.bytes(), before);
}

#[test]
fn prepared_collision_rechecks_source_after_decoding_before_publication() {
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
    let error = PreparedCollision::from_source(&bundle, || {
        let n = polls.get();
        polls.set(n + 1);
        if n == source_polls + 1 {
            // The initial source recheck has completed; owned product decoding
            // must not let a changed install cross the final publication guard.
            install.write(&source_entries);
            changed.set(true);
        }
        false
    })
    .unwrap_err();
    assert!(
        changed.get(),
        "mutation must occur after the initial recheck"
    );
    assert!(matches!(
        error,
        PrepareError::Source(MapBundleError::Source(SourceError::Changed))
    ));
    assert!(polls.get() > source_polls + 2);
    assert_eq!(
        capture.products()[0].raw_vmap().unwrap(),
        fixture("hull.raw.vmap")
    );
}
