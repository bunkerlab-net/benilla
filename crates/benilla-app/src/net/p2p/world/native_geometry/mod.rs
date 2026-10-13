// SPDX-License-Identifier: GPL-3.0-or-later
//! Local-only geometry service. This is deliberately inert until a supported native
//! session is installed. See README.md for the restricted domain and remaining gates.
#![expect(
    dead_code,
    reason = "native production session admission remains closed"
)]
mod core;
pub(crate) use self::core::{Body, Failure, LocalAuthority, Session};
use self::core::{Corridor, Mailbox};
use avian3d::prelude::{Collider, ColliderAabb, PhysicsSystems, SpatialQuery};
use benilla_assets::coords::wow_to_bevy;
use benilla_world::{
    collision::WorldCollision,
    schedule::WorldLive,
    terrain_stream::{CoverageError, NativeTerrainCoverage},
};
use bevy::{diagnostic::FrameCount, prelude::*, transform::TransformSystems};

/// Installed as a NonSend resource only by future audited native entry. Replacing/dropping
/// it closes the old queue; no renderer or explorer session silently becomes authority.
struct Service {
    mailbox: Mailbox,
    budget: core::FrameBudget,
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct GeometryQueries;

pub(crate) fn install(world: &mut World, session: Session) -> Result<LocalAuthority, Failure> {
    let (authority, mailbox) = core::mailbox(session)?;
    world.insert_non_send_resource(Service {
        mailbox,
        budget: default(),
    });
    Ok(authority)
}

pub(crate) fn uninstall(world: &mut World) {
    world.remove_non_send_resource::<Service>();
}

pub(crate) fn plugin(app: &mut App) {
    // PhysicsPlugins::default uses FixedPostUpdate. Avian updates its collider trees during
    // StepSimulation and writes positions back before Last. Update's streamer runs afterward;
    // a later fixed step observes its attached colliders, rather than querying a stale tree.
    app.configure_sets(
        FixedPostUpdate,
        GeometryQueries
            .after(PhysicsSystems::Last)
            .after(TransformSystems::Propagate),
    );
    app.add_systems(
        FixedPostUpdate,
        process
            .in_set(GeometryQueries)
            .run_if(|world: &World| world.contains_non_send::<Service>()),
    );
}

fn process(
    mut service: Option<NonSendMut<Service>>,
    frame: Res<FrameCount>,
    live: Res<WorldLive>,
    terrain: NativeTerrainCoverage,
    collision: WorldCollision,
    spatial: SpatialQuery,
) {
    let Some(service) = service.as_mut() else {
        return;
    };
    let allowance = service.budget.remaining(frame.0);
    if allowance == 0 {
        return;
    }
    let used = service.mailbox.serve(allowance, |request, body| {
        if !live.0 {
            return Err(Failure::MissingGeometry);
        }
        let mut corridor = LocalCorridor {
            terrain: &terrain,
            collision: &collision,
            spatial: &spatial,
            map: request.map_id,
            revision: request.geometry_revision.0,
        };
        core::solve(request, body, &mut corridor)
    });
    service.budget.spend(used);
}

struct LocalCorridor<'a, 'w, 's> {
    terrain: &'a NativeTerrainCoverage<'w, 's>,
    collision: &'a WorldCollision<'w, 's>,
    spatial: &'a SpatialQuery<'w, 's>,
    map: u32,
    revision: [u8; 32],
}
impl Corridor for LocalCorridor<'_, '_, '_> {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure> {
        // Positive raw geometry proof precedes every no-hit interpretation. No endpoint ray
        // interpolation or floating-point tolerance is used as continuous ground evidence.
        let proof = self
            .terrain
            .prove(self.map, from, to, body.radius.as_f32())
            .map_err(coverage_failure)?;
        if core::geometry_revision(proof.map, proof.wdt_source_digest, &proof.tiles)
            != self.revision
        {
            return Err(Failure::Binding);
        }
        let feet = wow_to_bevy(from);
        let movement = wow_to_bevy(to) - feet;
        let height = body.height.as_f32();
        // Additional colliders have no source/provenance support in this initial subset.
        // A stationary overlap also rejects: a zero-length body cast must not bless it.
        let end = wow_to_bevy(to);
        let radius3 = Vec3::new(body.radius.as_f32(), 0.0, body.radius.as_f32());
        let bounds = ColliderAabb {
            min: feet.min(end) - radius3,
            max: feet.max(end) + radius3 + Vec3::Y * height,
        };
        let (mut found_root, mut only_terrain) = (false, true);
        self.spatial
            .aabb_intersections_with_aabb_callback(bounds, |id| {
                found_root |= id == proof.terrain_root;
                only_terrain &= id == proof.terrain_root;
                only_terrain
            });
        if !only_terrain {
            return Err(Failure::UnsupportedNavigation);
        }
        if !found_root {
            return Err(Failure::MissingGeometry);
        }
        let radius = body.radius.as_f32();
        // Strict enclosing box, not a claim that native creatures use the client mover's shape.
        let shape = Collider::cuboid(radius * 2.0, height, radius * 2.0);
        if self
            .collision
            .cast_body(&shape, feet + Vec3::Y * (height * 0.5), movement, 0.0)
            .is_some()
        {
            return Err(Failure::Obstacle);
        }
        if let Ok(dir) = Dir3::new(movement) {
            // Pinned WorldObject::IsWithinLOS raises origin/target by collision height. The
            // flat placement-free subset makes M2/WMO source mask differences inapplicable.
            // This body-layer two-sided ray remains a local restriction, not server LOS parity.
            if self
                .spatial
                .cast_ray(
                    feet + Vec3::Y * height,
                    dir,
                    movement.length(),
                    true,
                    &WorldCollision::body_filter(),
                )
                .is_some()
            {
                return Err(Failure::Obstacle);
            }
        }
        Ok(())
    }
}
fn coverage_failure(error: CoverageError) -> Failure {
    // Unsupported is different from not-yet-loaded, but neither is an unreachable path.
    match error {
        CoverageError::InvalidInput | CoverageError::MalformedTerrain => Failure::Invalid,
        CoverageError::WrongMap => Failure::Binding,
        CoverageError::UnsupportedMap
        | CoverageError::UnsupportedSource
        | CoverageError::UnsupportedPlacements => Failure::UnsupportedNavigation,
        CoverageError::UnsupportedTerrain | CoverageError::OutsideSupportedChunk => {
            Failure::UnsupportedGround
        }
        CoverageError::UnloadedMap
        | CoverageError::UnloadedTile
        | CoverageError::UnfurnishedTile
        | CoverageError::ColliderUnavailable => Failure::MissingGeometry,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn net_plugin_registers_the_same_inert_geometry_plugin() {
        assert!(include_str!("../../../../net.rs").contains("p2p::native_geometry_plugin(app);"));
        assert!(include_str!("../../../p2p.rs")
            .contains("use native_geometry::plugin as native_geometry_plugin;"));
        let mut app = App::new();
        plugin(&mut app);
        assert!(!app.world().contains_non_send::<Service>());
        app.world_mut().run_schedule(FixedPostUpdate);
    }
    #[derive(Resource, Default)]
    struct Order(Vec<u8>);
    #[test]
    fn production_geometry_registration_runs_after_physics_and_transform_producers() {
        let mut app = App::new();
        plugin(&mut app);
        app.init_resource::<Order>();
        // Insert consumer first so incidental registration order cannot make this pass.
        app.add_systems(
            FixedPostUpdate,
            (|mut o: ResMut<Order>| o.0.push(3)).in_set(GeometryQueries),
        );
        app.add_systems(
            FixedPostUpdate,
            (|mut o: ResMut<Order>| o.0.push(1)).in_set(PhysicsSystems::Last),
        );
        app.configure_sets(
            FixedPostUpdate,
            PhysicsSystems::Last.before(TransformSystems::Propagate),
        );
        app.add_systems(
            FixedPostUpdate,
            (|mut o: ResMut<Order>| o.0.push(2)).in_set(TransformSystems::Propagate),
        );
        app.world_mut().run_schedule(FixedPostUpdate);
        assert_eq!(app.world().resource::<Order>().0, [1, 2, 3]);
    }
}
