// SPDX-License-Identifier: GPL-3.0-or-later
//! Refusal probes on the actual first multi-polygon straight callback. Diagnostic
//! points locate an obstacle only; every attempt reruns fresh native/source work.
use super::{
    core::{Body, Failure},
    products::{
        source_height::{
            inventory::InventoryCompatibility, CompositionError, PreparedSourceHeight,
        },
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::{ResidentLedger, StraightSegmentPhase},
};
use avian3d::prelude::{Collider, CollisionLayers, RigidBody};
use benilla_assets::{coords::wow_to_bevy, AdtTile};
use benilla_sim::creature_ai::Point;
use benilla_world::terrain_stream::NativeStaticMapIndex;
use benilla_world_runtime::native::{AuthorityRequest, Facts, MotionQuery, Query};
use bevy::prelude::{Assets, Transform, Vec3};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    time::{Duration, Instant},
};

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

#[derive(Default)]
struct Refusals {
    attempts: usize,
    emitted: usize,
}
impl Refusals {
    #[allow(clippy::too_many_arguments)]
    fn attempt(
        &mut self,
        ledger: &mut ResidentLedger,
        geometry: &mut InstalledFixture,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        request: &AuthorityRequest,
        until: Instant,
        cancelled: impl Fn() -> bool,
        on_segment: impl FnMut(StraightSegmentPhase, usize),
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        let before = ledger.diagnostic();
        let constructors = provider.retained_constructor_count();
        geometry.advance_physics();
        let result = ledger.certify_straight_segments_observed(
            geometry.app.world_mut(),
            compatibility,
            provider,
            request,
            401,
            until,
            cancelled,
            on_segment,
            after,
        );
        self.attempts += 1;
        self.emitted += usize::from(result.is_ok());
        assert_eq!(
            ledger.diagnostic(),
            before,
            "no refused polyline publishes any candidate or partial Facts"
        );
        assert_eq!(provider.retained_constructor_count(), constructors);
        result
    }
}

fn expect(
    result: Result<Facts, ResidentFilterError>,
    expected: impl FnOnce(&ResidentFilterError) -> bool,
) {
    match result {
        Err(error) => assert!(expected(&error), "wrong straight refusal stage: {error:?}"),
        Ok(_) => panic!("a straight refusal emitted successful Facts"),
    }
}

struct RestoreSource<'a> {
    install: &'a input::Install,
    original: Vec<(String, Vec<u8>)>,
    before: BTreeMap<String, Vec<u8>>,
}
impl<'a> RestoreSource<'a> {
    fn new(install: &'a input::Install) -> Self {
        Self {
            install,
            original: input::navigation_entries(),
            before: install.bytes(),
        }
    }
    fn change_wdt(&self) {
        let mut changed = self.original.clone();
        changed
            .iter_mut()
            .find(|(name, _)| name == input::WDT)
            .unwrap()
            .1[20] ^= 1;
        self.install.write(&changed);
        assert_ne!(self.install.bytes(), self.before);
    }
}
impl Drop for RestoreSource<'_> {
    fn drop(&mut self) {
        self.install.write(&self.original);
        assert_eq!(self.install.bytes(), self.before);
    }
}

fn to_bevy(point: Point) -> Vec3 {
    wow_to_bevy([point.x.as_f32(), point.y.as_f32(), point.z.as_f32()])
}

/// A tiny actual collider lies inside a later swept body rectangle and outside
/// EVERY earlier one. This proves the expected failure cannot occur on segment0.
fn later_obstacle(points: &[Point], body: Body) -> (usize, Vec3, Vec3) {
    let dimensions = Vec3::new(0.125, body.height.as_f32() * 0.5, 0.125);
    let half = dimensions * 0.5;
    let radius = Vec3::new(body.radius.as_f32(), 0., body.radius.as_f32());
    let vertical = Vec3::Y * body.height.as_f32();
    for index in 1..points.len() - 1 {
        let from = to_bevy(points[index]);
        let to = to_bevy(points[index + 1]);
        let center = (from + to) * 0.5 + vertical * 0.5;
        let obstacle_min = center - half;
        let obstacle_max = center + half;
        let intersects = |pair: &[Point]| {
            let from = to_bevy(pair[0]);
            let to = to_bevy(pair[1]);
            let min = from.min(to) - radius;
            let max = from.max(to) + radius + vertical;
            min.cmple(obstacle_max).all() && max.cmpge(obstacle_min).all()
        };
        if points.windows(2).take(index).all(|pair| !intersects(pair)) {
            assert!(intersects(&points[index..=index + 1]));
            return (index, center, dimensions);
        }
    }
    panic!("actual native polyline has no isolated later segment; do not alter poses or clearance to manufacture one")
}

fn expected_prefix(segments: usize) -> Vec<(StraightSegmentPhase, usize)> {
    (0..segments)
        .flat_map(|index| {
            [
                (StraightSegmentPhase::Corridor, index),
                (StraightSegmentPhase::SourceLos, index),
            ]
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
pub(super) fn probe(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    install: &input::Install,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("polyline refusal probe requires the actual fourth Route callback")
    };
    assert_eq!(route.request.prior_cache.as_ref().unwrap().revision, 3);
    let original = ledger.diagnostic();
    let original_source = install.bytes();
    assert_eq!(ledger.counts(), (1, 1));
    let diagnostic = ledger
        .probe_straight_geometry_for_test(provider, request, 401, deadline(), || false)
        .unwrap();
    assert_eq!(ledger.diagnostic(), original);
    assert!(diagnostic.points.len() > 2 && diagnostic.points.len() <= 32);
    assert!(!diagnostic.trace.is_empty());
    assert_eq!(diagnostic.points.first(), Some(&route.request.origin));
    assert_eq!(diagnostic.points.last(), Some(&route.request.goal));
    let segment_count = diagnostic.points.len() - 1;
    let (blocked_segment, obstacle_center, obstacle_size) =
        later_obstacle(&diagnostic.points, diagnostic.body);
    let mut refused = Refusals::default();
    let segments = RefCell::new(Vec::new());
    let completed = Cell::new(0);

    // All segments share the same mandatory 3x3 tile guard. A missing tile is
    // therefore an honest FIRST-segment refusal, not a later-segment claim.
    let removed: Vec<_> = {
        let mut assets = geometry.app.world_mut().resource_mut::<Assets<AdtTile>>();
        let ids: Vec<_> = assets.iter().map(|(id, _)| id).collect();
        assert!(!ids.is_empty());
        ids.into_iter()
            .map(|id| (id, assets.remove(id).unwrap()))
            .collect()
    };
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || false,
        |phase, index| segments.borrow_mut().push((phase, index)),
        || completed.set(completed.get() + 1),
    );
    {
        let mut assets = geometry.app.world_mut().resource_mut::<Assets<AdtTile>>();
        for (id, tile) in removed {
            assets.insert(id, tile).unwrap();
        }
    }
    geometry.advance_physics();
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Context(Failure::MissingGeometry)
        )
    });
    assert!(segments.borrow().is_empty());
    assert_eq!(completed.get(), 0);

    let obstacle = geometry
        .app
        .world_mut()
        .spawn((
            Collider::cuboid(obstacle_size.x, obstacle_size.y, obstacle_size.z),
            RigidBody::Static,
            CollisionLayers::DEFAULT,
            Transform::from_translation(obstacle_center),
        ))
        .id();
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || false,
        |phase, index| segments.borrow_mut().push((phase, index)),
        || completed.set(completed.get() + 1),
    );
    assert!(geometry.app.world_mut().despawn(obstacle));
    geometry.advance_physics();
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Context(Failure::UnsupportedNavigation)
        )
    });
    assert_eq!(
        *segments.borrow(),
        expected_prefix(blocked_segment),
        "every earlier corridor and independent M2-inclusive source LOS genuinely passed"
    );
    assert_eq!(completed.get(), 0);
    segments.borrow_mut().clear();

    let replacement = input::raw_inventory(install);
    let replacement_compatibility = height
        .match_raw_inventory(&replacement, deadline(), || false)
        .unwrap();
    assert_eq!(
        replacement.source_digest(),
        compatibility.owner().inventory_digest()
    );
    assert_eq!(
        replacement_compatibility.revision(),
        compatibility.revision()
    );
    assert_eq!(
        replacement_compatibility.owner().receipt_digest(),
        compatibility.owner().receipt_digest()
    );
    assert!(!replacement_compatibility
        .owner()
        .same_owner(compatibility.owner()));
    let original_index = geometry
        .app
        .world_mut()
        .remove_resource::<NativeStaticMapIndex>()
        .unwrap();
    geometry
        .app
        .insert_resource(NativeStaticMapIndex::from_inventory(replacement).unwrap());
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || false,
        |phase, index| segments.borrow_mut().push((phase, index)),
        || completed.set(completed.get() + 1),
    );
    geometry.app.insert_resource(original_index);
    geometry.advance_physics();
    expect(result, |error| {
        matches!(error, ResidentFilterError::Context(Failure::Binding))
    });
    assert!(segments.borrow().is_empty());
    assert_eq!(completed.get(), 0);

    let cancelled = Cell::new(false);
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || cancelled.get(),
        |phase, index| segments.borrow_mut().push((phase, index)),
        || {
            completed.set(completed.get() + 1);
            cancelled.set(true);
        },
    );
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Source(CompositionError::Cancelled)
        )
    });
    assert_eq!(*segments.borrow(), expected_prefix(segment_count));
    assert_eq!(completed.get(), 1);
    segments.borrow_mut().clear();

    let restore = RestoreSource::new(install);
    let until = deadline();
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        until,
        || false,
        |phase, index| segments.borrow_mut().push((phase, index)),
        || {
            completed.set(completed.get() + 1);
            restore.change_wdt();
        },
    );
    drop(restore);
    expect(result, |error| {
        matches!(
            error,
            ResidentFilterError::Source(CompositionError::TerrainSource)
        )
    });
    assert!(Instant::now() < until, "expiry cannot mask source mutation");
    assert_eq!(*segments.borrow(), expected_prefix(segment_count));
    assert_eq!(completed.get(), 2);
    segments.borrow_mut().clear();

    let original_deadline = deadline();
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        original_deadline,
        || false,
        |phase, index| segments.borrow_mut().push((phase, index)),
        || {
            assert!(
                Instant::now() < original_deadline,
                "every segment completed under the original lease"
            );
            completed.set(completed.get() + 1);
            while Instant::now() < original_deadline {
                std::thread::sleep(original_deadline.saturating_duration_since(Instant::now()));
            }
        },
    );
    expect(result, |error| {
        matches!(error, ResidentFilterError::Context(Failure::Deadline))
    });
    assert_eq!(*segments.borrow(), expected_prefix(segment_count));
    assert_eq!(completed.get(), 3);
    assert_eq!(refused.attempts, 6);
    assert_eq!(refused.emitted, 0);
    assert_eq!(ledger.diagnostic(), original);
    assert_eq!(install.bytes(), original_source);
    // This fixture has no M2 placements. Successful independent rays include M2
    // by policy, but do not establish a genuine M2-hit refusal or later tile loss.
    eprintln!("resident straight polyline refusals: points={} segments={segment_count} blocked-segment={blocked_segment} attempts=6 emitted=0 after-all-segments=3 committed-cache3=1 staged-cache4=0; missing-coverage=first-segment M2-hit=not-exercised", diagnostic.points.len());
}
