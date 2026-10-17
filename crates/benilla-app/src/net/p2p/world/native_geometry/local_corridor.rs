// SPDX-License-Identifier: GPL-3.0-or-later
//! Concrete production terrain membership, body clearance and LOS checks.
//! Shared verbatim by the app service and the thin authored corridor harness.
use super::{core, Body, Corridor, Failure};
use avian3d::prelude::{Collider, ColliderAabb, ShapeCastConfig, SpatialQuery};
use benilla_assets::coords::wow_to_bevy;
use benilla_world::{
    collision::WorldCollision,
    terrain_stream::{CoverageError, NativeTerrainCoverage},
};
use bevy::prelude::*;

pub(super) struct LocalCorridor<'a, 'w, 's> {
    pub(super) terrain: &'a NativeTerrainCoverage<'w, 's>,
    pub(super) collision: &'a WorldCollision<'w, 's>,
    pub(super) spatial: &'a SpatialQuery<'w, 's>,
    pub(super) map: u32,
    pub(super) revision: [u8; 32],
}
impl Corridor for LocalCorridor<'_, '_, '_> {
    fn certify(&mut self, from: [f32; 3], to: [f32; 3], body: Body) -> Result<(), Failure> {
        // Positive raw geometry proof precedes every no-hit interpretation. No endpoint ray
        // interpolation or floating-point tolerance is used as continuous ground evidence.
        let proof = self
            .terrain
            .prove(self.map, from, to, body.radius.as_f32())
            .map_err(coverage_failure)?;
        if core::geometry_revision(
            proof.map,
            proof.wdt_source_digest,
            &proof.tiles,
            &proof.models,
            proof.inventory_digest,
        ) != self.revision
        {
            return Err(Failure::Binding);
        }
        let feet = wow_to_bevy(from);
        let movement = wow_to_bevy(to) - feet;
        let height = body.height.as_f32();
        // Every intersecting collider must be the supporting terrain or an exact source-bound
        // hull. Uncatalogued dynamic/unsupported geometry is never interpreted as clear space.
        let end = wow_to_bevy(to);
        let radius3 = Vec3::new(body.radius.as_f32(), 0.0, body.radius.as_f32());
        let bounds = ColliderAabb {
            min: feet.min(end) - radius3,
            max: feet.max(end) + radius3 + Vec3::Y * height,
        };
        let (mut found_root, mut only_proven) = (false, true);
        self.spatial
            .aabb_intersections_with_aabb_callback(bounds, |id| {
                found_root |= id == proof.terrain_root;
                only_proven &= id == proof.terrain_root || proof.static_roots.contains(&id);
                only_proven
            });
        if !only_proven {
            return Err(Failure::UnsupportedNavigation);
        }
        if !found_root {
            #[cfg(test)]
            eprintln!("native geometry corridor failure: stage=terrain collider membership error=missing terrain root");
            return Err(Failure::MissingGeometry);
        }
        let radius = body.radius.as_f32();
        // Strict enclosing box, not a claim that native creatures use the client mover's shape.
        let shape = Collider::cuboid(radius * 2.0, height, radius * 2.0);
        // Supporting flat terrain is already proved continuously. Exclude only that root
        // from two-sided static hull checks so floor contact is not mistaken for an obstacle.
        // Trimeshes are hollow: conservatively reject an endpoint body whose AABB intersects
        // any individual source hull AABB, rather than declaring an enclosed body unblocked.
        for endpoint in [feet, end] {
            let min = endpoint - radius3;
            let max = endpoint + radius3 + Vec3::Y * height;
            if proof
                .static_bounds
                .iter()
                .any(|(a, b)| min.cmple(*b).all() && max.cmpge(*a).all())
            {
                return Err(Failure::Obstacle);
            }
        }
        let hull_filter =
            WorldCollision::body_filter().with_excluded_entities([proof.terrain_root]);
        let center = feet + Vec3::Y * (height * 0.5);
        for at in [center, end + Vec3::Y * (height * 0.5)] {
            let mut intersects = false;
            self.spatial.shape_intersections_callback(
                &shape,
                at,
                Quat::IDENTITY,
                &hull_filter,
                |_| {
                    intersects = true;
                    false
                },
            );
            if intersects {
                return Err(Failure::Obstacle);
            }
        }
        if let Ok(direction) = Dir3::new(movement) {
            if self
                .spatial
                .cast_shape(
                    &shape,
                    center,
                    Quat::IDENTITY,
                    direction,
                    &ShapeCastConfig::from_max_distance(movement.length()),
                    &hull_filter,
                )
                .is_some()
            {
                return Err(Failure::Obstacle);
            }
        }
        if self
            .collision
            .cast_body(&shape, feet + Vec3::Y * (height * 0.5), movement, 0.0)
            .is_some()
        {
            return Err(Failure::Obstacle);
        }
        if let Ok(dir) = Dir3::new(movement) {
            // Pinned WorldObject::IsWithinLOS raises origin/target by collision height. The
            // local static domain unions walking and camera faces, including camera-only
            // WMO DETAIL faces. This is not server MOD_M2/ignoreM2 or WMO face-mask parity.
            if self
                .spatial
                .cast_ray(
                    feet + Vec3::Y * height,
                    dir,
                    movement.length(),
                    true,
                    &WorldCollision::conservative_los_filter(),
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
    #[cfg(test)]
    eprintln!("native geometry corridor failure: stage=raw coverage error={error:?}");
    // Unsupported is different from not-yet-loaded, but neither is an unreachable path.
    match error {
        CoverageError::InvalidInput | CoverageError::MalformedTerrain => Failure::Invalid,
        CoverageError::WrongMap => Failure::Binding,
        CoverageError::UnsupportedMap
        | CoverageError::UnsupportedSource
        | CoverageError::UnsupportedPlacements => Failure::UnsupportedNavigation,
        CoverageError::UnsupportedTerrain | CoverageError::OutsideSupportedTile => {
            Failure::UnsupportedGround
        }
        CoverageError::UnloadedMap
        | CoverageError::UnloadedTile
        | CoverageError::UnfurnishedTile
        | CoverageError::ColliderUnavailable => Failure::MissingGeometry,
    }
}
