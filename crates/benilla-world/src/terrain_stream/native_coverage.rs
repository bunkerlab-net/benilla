//! Conservative positive ADT coverage for the opt-in native simulation. Display residency is
//! not evidence: a failed placement can retire as spawned, and point probes cannot prove a
//! continuous corridor. Only a source-certified empty guard neighborhood and complete flat fan
//! geometry qualify. Physics-query freshness and obstacle sweeps remain the caller's duty.

use avian3d::prelude::{
    Collider, ColliderDisabled, CollisionLayers, Position, RigidBody, RigidBodyDisabled, Rotation,
    Sensor,
};
use benilla_assets::coords::wow_to_bevy;
use benilla_assets::{AdtTile, MapCatalogRes, WdtIndex};
use benilla_formats::{world_to_tile, ChunkMesh};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::{PendingCollider, TerrainStreamer, TileState};
use crate::collision::{GroundDecalSurface, PickOccluder};
use crate::world_map::CurrentMap;

mod planar;
pub use planar::CoverageError;

/// Source identity for the exact neighborhood examined by [`NativeTerrainCoverage::prove`].
/// Callers bind their geometry policy and these raw-byte digests into their revision; a caller's
/// requested revision is not evidence of which geometry actually loaded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CoverageProof {
    pub map: u32,
    /// Exact raw map index; a renderer-filtered WDT cannot certify the terrain-only subset.
    pub wdt_source_digest: [u8; 32],
    /// Exact supporting collider; callers reject other intersecting geometry as unsupported.
    pub terrain_root: Entity,
    /// Stable row-major order: increasing tile y, then increasing tile x, over the 3×3 guard.
    pub tiles: [((i32, i32), [u8; 32]); 9],
}

type RootData = (
    &'static Collider,
    &'static RigidBody,
    &'static Transform,
    &'static GlobalTransform,
    &'static Position,
    &'static Rotation,
    Option<&'static CollisionLayers>,
);

type RootFilter = (
    With<GroundDecalSurface>,
    With<PickOccluder>,
    Without<PendingCollider>,
    Without<ColliderDisabled>,
    Without<RigidBodyDisabled>,
    Without<Sensor>,
    Without<ChildOf>,
);

/// Main-thread, read-only proof over currently resident terrain assets and their live colliders.
/// Missing resources, model references (failed loads included), liquid source records, malformed
/// chunks, slopes, holes, seams and height mismatches fail closed. This is deliberately not a
/// general navigation mesh, WMO resolver, liquid query, or a replacement for an obstacle sweep.
#[derive(SystemParam)]
pub struct NativeTerrainCoverage<'w, 's> {
    current_map: Option<Res<'w, CurrentMap>>,
    catalog: Option<Res<'w, MapCatalogRes>>,
    streamer: Option<Res<'w, TerrainStreamer>>,
    tiles: Option<Res<'w, Assets<AdtTile>>>,
    wdts: Option<Res<'w, Assets<WdtIndex>>>,
    roots: Query<'w, 's, RootData, RootFilter>,
}

impl NativeTerrainCoverage<'_, '_> {
    /// Prove the complete swept XY rectangle is supported by one flat, dry, fully triangulated
    /// native ADT chunk. Positions are WoW feet coordinates; both Z values must equal every
    /// supporting vertex's Z exactly. A zero radius is allowed for a bare segment.
    ///
    /// The rectangle must lie strictly within one chunk, in a loaded/furnished tile whose 3×3
    /// guard is also source-certified, placement-free and furnished with live terrain colliders.
    /// No geometry outside that neighborhood is certified. No point sampling or readiness flag
    /// substitutes for this proof; the caller must independently prove a fresh physics sweep.
    pub fn prove(
        &self,
        map_id: u32,
        from: [f32; 3],
        to: [f32; 3],
        radius: f32,
    ) -> Result<CoverageProof, CoverageError> {
        if !from.into_iter().chain(to).all(f32::is_finite) || !radius.is_finite() || radius < 0.0 {
            return Err(CoverageError::InvalidInput);
        }
        let current_map = self
            .current_map
            .as_ref()
            .ok_or(CoverageError::UnloadedMap)?;
        let catalog = self.catalog.as_ref().ok_or(CoverageError::UnloadedMap)?;
        let streamer = self.streamer.as_ref().ok_or(CoverageError::UnloadedMap)?;
        let tiles = self.tiles.as_ref().ok_or(CoverageError::UnloadedMap)?;
        let directory = catalog.0.directory(map_id).ok_or(CoverageError::WrongMap)?;
        if current_map.0 != map_id || streamer.map_dir.as_deref() != Some(directory) {
            return Err(CoverageError::WrongMap);
        }
        let wdt = self
            .wdts
            .as_ref()
            .and_then(|wdts| streamer.wdt.as_ref().and_then(|h| wdts.get(h)))
            .ok_or(CoverageError::UnloadedMap)?;
        if streamer.wdt_ungated
            || streamer.global_wmo
            || wdt.global_wmo().is_some()
            || !wdt.native_terrain_coverage_safe()
        {
            return Err(CoverageError::UnsupportedMap);
        }
        let (tx, ty) = world_to_tile(from[0], from[1]);
        // A full guard is mandatory. world_to_tile clamps, so the pure geometry proof below also
        // checks actual canonical coordinates rather than treating an off-map value as resident.
        if !(1..63).contains(&tx) || !(1..63).contains(&ty) {
            return Err(CoverageError::OutsideSupportedChunk);
        }
        let (tx, ty) = (tx as i32, ty as i32);
        let mut proof = CoverageProof {
            map: map_id,
            wdt_source_digest: wdt.native_source_digest(),
            terrain_root: Entity::PLACEHOLDER,
            tiles: [((0, 0), [0; 32]); 9],
        };
        for (slot, (x, y)) in ((ty - 1)..=(ty + 1))
            .flat_map(|y| ((tx - 1)..=(tx + 1)).map(move |x| (x, y)))
            .enumerate()
        {
            if !wdt.has_tile(x as u32, y as u32) {
                return Err(CoverageError::UnloadedTile);
            }
            let state = streamer
                .tiles
                .get(&(x, y))
                .ok_or(CoverageError::UnloadedTile)?;
            let adt = tiles
                .get(&state.handle)
                .ok_or(CoverageError::UnloadedTile)?;
            if !state.furnished {
                return Err(CoverageError::UnfurnishedTile);
            }
            if !adt.native_coverage_safe {
                return Err(CoverageError::UnsupportedSource);
            }
            // Do not ask `Placement::spawned` or `WorldLoadProgress::presentable`: failed loads
            // deliberately retire from those display backlogs. Any original reference rejects.
            if !state.placements.is_empty() || !adt.doodads.is_empty() || !adt.wmos.is_empty() {
                return Err(CoverageError::UnsupportedPlacements);
            }
            if adt.chunks.len() != 256 {
                return Err(CoverageError::MalformedTerrain);
            }
            if adt.chunks.iter().any(|chunk| !chunk.liquids.is_empty()) {
                return Err(CoverageError::UnsupportedTerrain);
            }
            let collider = self.live_collider(state)?;
            if (x, y) == (tx, ty) {
                proof.terrain_root = state.entity.ok_or(CoverageError::ColliderUnavailable)?;
                planar::prove_planar_tile(&adt.chunks, (tx as u32, ty as u32), from, to, radius)?;
                if !collider_matches_tile(collider, &adt.chunks) {
                    return Err(CoverageError::ColliderUnavailable);
                }
            }
            proof.tiles[slot] = ((x, y), adt.native_source_digest);
        }
        Ok(proof)
    }

    fn live_collider(&self, state: &TileState) -> Result<&Collider, CoverageError> {
        let root = state.entity.ok_or(CoverageError::ColliderUnavailable)?;
        let (collider, body, transform, global, position, rotation, layers) = self
            .roots
            .get(root)
            .map_err(|_| CoverageError::ColliderUnavailable)?;
        if *body != RigidBody::Static
            || *transform != Transform::IDENTITY
            || *global != GlobalTransform::IDENTITY
            || position.0 != Vec3::ZERO
            || rotation.0 != Quat::IDENTITY
            || layers.is_some_and(|layers| *layers != CollisionLayers::DEFAULT)
        {
            return Err(CoverageError::ColliderUnavailable);
        }
        Ok(collider)
    }
}

/// Presence alone cannot tie a collider to a hot-reloaded/replaced asset. Compare the active
/// scaled trimesh to the exact bounded vertex/index stream `terrain_collider_data` supplies.
fn collider_matches_tile(collider: &Collider, chunks: &[ChunkMesh]) -> bool {
    let Some(mesh) = collider.shape_scaled().as_trimesh() else {
        return false;
    };
    let vertex_count: usize = chunks.iter().map(|c| c.positions.len()).sum();
    let triangle_count: usize = chunks.iter().map(|c| c.indices.len() / 3).sum();
    if mesh.vertices().len() != vertex_count || mesh.indices().len() != triangle_count {
        return false;
    }
    let mut vertex_offset = 0usize;
    let mut triangle_offset = 0usize;
    for chunk in chunks {
        for (offset, position) in chunk.positions.iter().enumerate() {
            if mesh.vertices()[vertex_offset + offset] != wow_to_bevy(*position) {
                return false;
            }
        }
        for (offset, triangle) in chunk.indices.chunks_exact(3).enumerate() {
            let base = vertex_offset as u32;
            if mesh.indices()[triangle_offset + offset]
                != [base + triangle[0], base + triangle[1], base + triangle[2]]
            {
                return false;
            }
        }
        vertex_offset += chunk.positions.len();
        triangle_offset += chunk.indices.len() / 3;
    }
    true
}
