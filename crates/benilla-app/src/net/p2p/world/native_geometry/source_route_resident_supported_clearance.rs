// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual fifth-callback negatives for the generated flat support policy. The
//! diagnostic points locate mutations only; every attempt reruns source/native work.
use super::{
    core::{Body, Failure},
    products::{
        source_height::{inventory::InventoryCompatibility, CompositionError},
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::{
        support_prism_bounds_for_test, ResidentLedger, StraightSegmentPhase,
    },
};
use avian3d::prelude::{Collider, CollisionLayers, RigidBody};
use benilla_assets::{coords::wow_to_bevy, AdtTile};
use benilla_formats::{ChunkMesh, LiquidKind, LiquidMesh};
use benilla_sim::creature_ai::Point;
use benilla_world::terrain_stream::{CoverageError, NativeTerrainCoverage};
use benilla_world_runtime::native::{AuthorityRequest, Facts, MotionQuery, Query};
use bevy::{ecs::system::SystemState, prelude::*};
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
            501,
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
            "a refused support path stages no partial candidate or Facts"
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
        Err(error) => assert!(
            expected(&error),
            "wrong generated-support refusal stage: {error:?}"
        ),
        Ok(_) => panic!("a generated-support refusal emitted Facts"),
    }
}

fn xyz(point: Point) -> [f32; 3] {
    [point.x.as_f32(), point.y.as_f32(), point.z.as_f32()]
}

fn projected(pair: &[Point], plane: f32) -> ([f32; 3], [f32; 3]) {
    let mut from = xyz(pair[0]);
    let mut to = xyz(pair[1]);
    from[2] = plane;
    to[2] = plane;
    (from, to)
}

fn raw_coverage(
    geometry: &mut InstalledFixture,
    map: u32,
    pair: &[Point],
    body: Body,
    plane: f32,
) -> Result<(), CoverageError> {
    let mut state = SystemState::<NativeTerrainCoverage>::new(geometry.app.world_mut());
    let terrain = state.get(geometry.app.world());
    let (from, to) = projected(pair, plane);
    terrain
        .prove(map, from, to, body.radius.as_f32())
        .map(|_| ())
}

#[derive(Clone, Copy, Debug)]
enum Mutation {
    Hole,
    Liquid,
    Step,
}

/// Select an actual canonical cell intersected inside the first segment. A
/// changed fan remains structurally valid, so its error is not malformed topology.
fn mutate_first_footprint(
    geometry: &mut InstalledFixture,
    points: &[Point],
    kind: Mutation,
) -> (AssetId<AdtTile>, usize, ChunkMesh) {
    let from = xyz(points[0]);
    let to = xyz(points[1]);
    let middle = [(from[0] + to[0]) * 0.5, (from[1] + to[1]) * 0.5];
    let mut assets = geometry.app.world_mut().resource_mut::<Assets<AdtTile>>();
    let mut found = None;
    for (id, tile) in assets.iter_mut() {
        for (index, chunk) in tile.chunks.iter_mut().enumerate() {
            for row in 0..8usize {
                for col in 0..8usize {
                    let tl = row * 17 + col;
                    let br = tl + 18;
                    let nw = chunk.positions[tl];
                    let se = chunk.positions[br];
                    if !(se[0] < middle[0]
                        && middle[0] < nw[0]
                        && se[1] < middle[1]
                        && middle[1] < nw[1])
                    {
                        continue;
                    }
                    assert!(
                        found.is_none(),
                        "one exact canonical cell owns the interior point"
                    );
                    let original = chunk.clone();
                    match kind {
                        Mutation::Hole => {
                            chunk.holes |= 1 << ((row >> 1) * 4 + (col >> 1));
                            chunk.indices.clear();
                            for r in 0..8u32 {
                                for c in 0..8u32 {
                                    if chunk.holes & (1 << ((r >> 1) * 4 + (c >> 1))) != 0 {
                                        continue;
                                    }
                                    let tl = r * 17 + c;
                                    let (tr, bl, br, center) =
                                        (tl + 1, tl + 17, tl + 18, r * 17 + 9 + c);
                                    chunk.indices.extend([
                                        center, tl, bl, center, bl, br, center, br, tr, center, tr,
                                        tl,
                                    ]);
                                }
                            }
                        }
                        Mutation::Liquid => {
                            let mut corners =
                                vec![nw, chunk.positions[tl + 1], chunk.positions[tl + 17], se];
                            for corner in &mut corners {
                                corner[2] += 0.125;
                            }
                            chunk.liquids.push(LiquidMesh {
                                grid: [2, 2],
                                wet: vec![true],
                                shared: vec![false],
                                positions: corners,
                                uvs: vec![[0., 0.], [1., 0.], [0., 1.], [1., 1.]],
                                depths: vec![1.; 4],
                                indices: vec![0, 2, 1, 1, 2, 3],
                                sound_nibble: 0,
                                material_id: None,
                                kind: LiquidKind::Still,
                            });
                        }
                        Mutation::Step => chunk.positions[row * 17 + 9 + col][2] += 0.125,
                    }
                    found = Some((id, index, original));
                }
            }
        }
    }
    found.expect("the unchanged actual first segment crosses a loaded raw cell")
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

/// Both boxes are outside segment zero's actual checked enclosure, not merely
/// its thin native rectangle. The low box is wholly below the native feet; the
/// upper box overlaps the real body, not only the enclosing prism's margin.
fn obstacles(points: &[Point], body: Body, plane: f32) -> [(Vec3, Vec3); 2] {
    assert_eq!(points.len(), 4);
    let from = wow_to_bevy(xyz(points[1]));
    let to = wow_to_bevy(xyz(points[2]));
    assert_eq!(from.y, to.y);
    let q = from.y;
    let gap = q - plane;
    assert!(gap > 0.);
    let middle = (from + to) * 0.5;
    let low_size = Vec3::new(0.125, gap * 0.25, 0.125);
    let low = Vec3::new(middle.x, plane + gap * 0.5, middle.z);
    assert!(low.y - low_size.y * 0.5 > plane && low.y + low_size.y * 0.5 < q);
    let high_size = Vec3::splat(0.125);
    let high = middle + Vec3::Y * (body.height.as_f32() * 0.75);
    assert!(high.y - high_size.y * 0.5 > q);
    assert!(high.y + high_size.y * 0.5 < q + body.height.as_f32());
    let radius = Vec3::new(body.radius.as_f32(), 0., body.radius.as_f32());
    let (earlier_min, earlier_max) =
        support_prism_bounds_for_test(xyz(points[0]), xyz(points[1]), body, plane, q).unwrap();
    let (current_min, current_max) =
        support_prism_bounds_for_test(xyz(points[1]), xyz(points[2]), body, plane, q).unwrap();
    for (center, size) in [(low, low_size), (high, high_size)] {
        let obstacle_min = center - size * 0.5;
        let obstacle_max = center + size * 0.5;
        assert!(
            obstacle_max.x < earlier_min.x
                || obstacle_min.x > earlier_max.x
                || obstacle_max.z < earlier_min.z
                || obstacle_min.z > earlier_max.z,
            "the refusal cannot be attributed to the earlier checked stationary prism"
        );
        assert!(current_min.cmplt(obstacle_min).all());
        assert!(current_max.cmpgt(obstacle_max).all());
        // Independently stay within the actual body's midpoint XY footprint,
        // rather than relying on the proof square's deliberately larger area.
        let segment_min = middle - radius;
        let segment_max = middle + radius;
        assert!(
            obstacle_min.x > segment_min.x
                && obstacle_max.x < segment_max.x
                && obstacle_min.z > segment_min.z
                && obstacle_max.z < segment_max.z
        );
    }
    eprintln!("resident generated-support collider placement: earlier-checked-prism={earlier_min:?}..{earlier_max:?} current-checked-prism={current_min:?}..{current_max:?} low-gap={low:?} upper-body={high:?}");
    [(low, low_size), (high, high_size)]
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

#[allow(clippy::too_many_arguments)]
pub(super) fn probe(
    ledger: &mut ResidentLedger,
    geometry: &mut InstalledFixture,
    install: &input::Install,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        panic!("support refusal probe requires the actual fifth Route callback")
    };
    assert_eq!(request.binding.tick, 128);
    assert_eq!(route.request.binding.query_id, 142);
    assert_eq!(route.request.prior_cache.as_ref().unwrap().revision, 4);
    let original = ledger.diagnostic();
    let original_source = install.bytes();
    let diagnostic = ledger
        .probe_straight_geometry_for_test(provider, request, 501, deadline(), || false)
        .unwrap();
    assert_eq!(ledger.diagnostic(), original);
    assert_eq!(diagnostic.points.len(), 4);
    assert_eq!(diagnostic.points.first(), Some(&route.request.origin));
    assert_eq!(diagnostic.points.last(), Some(&route.request.goal));
    assert!(!diagnostic.trace.is_empty());
    let plane = diagnostic.points[0].z.as_f32();
    assert_eq!(plane.to_bits(), 3259498496);
    assert_eq!(diagnostic.points[1].z.bits(), 3259428591);
    assert_eq!(diagnostic.points[2].z.bits(), 3259428591);
    assert_eq!(diagnostic.points[3].z.bits(), plane.to_bits());
    let map = compatibility.source_binding().0.map;
    for pair in diagnostic.points.windows(2) {
        raw_coverage(geometry, map, pair, diagnostic.body, plane).unwrap();
    }
    let mut refused = Refusals::default();
    let phases = RefCell::new(vec![]);
    let completed = Cell::new(0);
    for kind in [Mutation::Hole, Mutation::Liquid, Mutation::Step] {
        let (id, index, original_chunk) =
            mutate_first_footprint(geometry, &diagnostic.points, kind);
        assert_eq!(
            raw_coverage(
                geometry,
                map,
                &diagnostic.points[..2],
                diagnostic.body,
                plane
            ),
            Err(CoverageError::UnsupportedTerrain),
            "{kind:?} reaches continuous raw support before any collider comparison"
        );
        let result = refused.attempt(
            ledger,
            geometry,
            compatibility,
            provider,
            request,
            deadline(),
            || false,
            |phase, index| phases.borrow_mut().push((phase, index)),
            || completed.set(completed.get() + 1),
        );
        geometry
            .app
            .world_mut()
            .resource_mut::<Assets<AdtTile>>()
            .get_mut(id)
            .unwrap()
            .chunks[index] = original_chunk;
        geometry.advance_physics();
        expect(result, |error| {
            matches!(
                error,
                ResidentFilterError::Context(Failure::MissingGeometry)
            )
        });
        assert!(phases.borrow().is_empty());
        assert_eq!(completed.get(), 0);
        for pair in diagnostic.points.windows(2) {
            raw_coverage(geometry, map, pair, diagnostic.body, plane).unwrap();
        }
        eprintln!("resident generated-support raw refusal: kind={kind:?} raw=UnsupportedTerrain segment=0 emitted=0 restored=true");
    }
    for (kind, (center, size)) in ["low-gap", "upper-body"].into_iter().zip(obstacles(
        &diagnostic.points,
        diagnostic.body,
        plane,
    )) {
        let entity = geometry
            .app
            .world_mut()
            .spawn((
                Collider::cuboid(size.x, size.y, size.z),
                RigidBody::Static,
                CollisionLayers::DEFAULT,
                Transform::from_translation(center),
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
            |phase, index| phases.borrow_mut().push((phase, index)),
            || completed.set(completed.get() + 1),
        );
        assert!(geometry.app.world_mut().despawn(entity));
        geometry.advance_physics();
        expect(result, |error| {
            matches!(
                error,
                ResidentFilterError::Context(Failure::UnsupportedNavigation)
            )
        });
        assert_eq!(*phases.borrow(), expected_prefix(1));
        assert_eq!(completed.get(), 0);
        phases.borrow_mut().clear();
        eprintln!("resident generated-support collider refusal: kind={kind} segment=1 earlier-corridor-and-source-los=passed unknown-collider-membership=UnsupportedNavigation emitted=0 removed=true");
    }
    let segment_count = diagnostic.points.len() - 1;
    let cancelled = Cell::new(false);
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        deadline(),
        || cancelled.get(),
        |phase, index| phases.borrow_mut().push((phase, index)),
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
    assert_eq!(*phases.borrow(), expected_prefix(segment_count));
    assert_eq!(completed.get(), 1);
    phases.borrow_mut().clear();
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
        |phase, index| phases.borrow_mut().push((phase, index)),
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
    assert!(Instant::now() < until, "source mutation is not expiry");
    assert_eq!(*phases.borrow(), expected_prefix(segment_count));
    assert_eq!(completed.get(), 2);
    phases.borrow_mut().clear();
    let until = deadline();
    let result = refused.attempt(
        ledger,
        geometry,
        compatibility,
        provider,
        request,
        until,
        || false,
        |phase, index| phases.borrow_mut().push((phase, index)),
        || {
            assert!(
                Instant::now() < until,
                "all segments finished under the original lease"
            );
            completed.set(completed.get() + 1);
            while Instant::now() < until {
                std::thread::sleep(until.saturating_duration_since(Instant::now()));
            }
        },
    );
    expect(result, |error| {
        matches!(error, ResidentFilterError::Context(Failure::Deadline))
    });
    assert_eq!(*phases.borrow(), expected_prefix(segment_count));
    assert_eq!(completed.get(), 3);
    assert_eq!((refused.attempts, refused.emitted), (8, 0));
    assert_eq!(ledger.diagnostic(), original);
    assert_eq!(install.bytes(), original_source);
    eprintln!("resident generated-support fifth refusals: points=4 segments=3 attempts=8 emitted=0 raw-footprint=hole,liquid,step prism=low-gap,upper-body after-all-segments=cancel,source,expiry committed-cache4=1 staged-cache5=0; genuine-source-M2-hit=not-exercised; fresh actual certification follows");
}
