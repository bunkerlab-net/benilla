//! Positive raw ADT/M2/WMO coverage for the opt-in native simulation. Display residency alone
//! cannot prove collision. A full-map static source inventory excludes unloaded exterior hulls;
//! the local guard proves loaded placement/collider completeness and flat continuous support.
//! Physics-query freshness and obstacle checks remain the caller's duty.

use avian3d::prelude::{
    Collider, ColliderDisabled, CollisionLayers, Position, RigidBody, RigidBodyDisabled, Rotation,
    Sensor,
};
use benilla_assets::coords::{placement_rotation, wmo_doodad_local, wow_to_bevy};
use benilla_assets::{AdtTile, M2Model, MapCatalogRes, WdtIndex, WmoModel};
use benilla_formats::{world_to_tile, ChunkMesh};
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use super::{ModelHandle, PendingCollider, Placements, TerrainStreamer, TileState};
use crate::collision::{GroundDecalSurface, PickOccluder};
use crate::world_map::CurrentMap;

mod index;
mod planar;
mod static_mesh;
pub use index::NativeStaticMapIndex;
pub use planar::CoverageError;
pub(super) use static_mesh::NativeHullId;

/// Source identity for the exact neighborhood examined by [`NativeTerrainCoverage::prove`].
/// Callers bind their geometry policy and these raw-byte digests into their revision; a caller's
/// requested revision is not evidence of which geometry actually loaded.
#[derive(Clone, Debug, PartialEq)]
pub struct CoverageProof {
    pub map: u32,
    /// Exact raw map index; a renderer-filtered WDT cannot certify the terrain-only subset.
    pub wdt_source_digest: [u8; 32],
    /// Exact supporting collider; callers reject other intersecting geometry as unsupported.
    pub terrain_root: Entity,
    /// Complete loaded placement dependency identities in ascending placement-ID order.
    pub models: Vec<(u32, [u8; 32])>,
    /// Exact live hulls checked against source placement transforms and source collision arrays.
    pub static_roots: Vec<Entity>,
    /// Per-placement hull bounds; conservative solid containment exclusion at endpoints.
    pub static_bounds: Vec<(Vec3, Vec3)>,
    /// Complete map-source inventory, including unloaded exterior placements.
    pub inventory_digest: [u8; 32],
    /// Stable row-major order: increasing tile y, then increasing tile x, over the 3×3 guard.
    pub tiles: [((i32, i32), [u8; 32]); 9],
}

type ExpectedHulls = BTreeMap<NativeHullId, (Vec<Vec3>, Vec<[u32; 3]>, [u8; 32])>;

type RootData = (
    &'static Collider,
    &'static RigidBody,
    &'static Transform,
    &'static GlobalTransform,
    &'static Position,
    &'static Rotation,
    Option<&'static CollisionLayers>,
);

/// Written only by the actual streamer collider producer. Membership is evidence only after
/// every source, transform and the complete live triangle stream are checked again.
#[derive(Component)]
pub(super) struct StaticColliderSources(pub(super) Vec<NativeHullId>);

type StaticFilter = (
    With<StaticColliderSources>,
    Without<PendingCollider>,
    Without<ColliderDisabled>,
    Without<RigidBodyDisabled>,
    Without<Sensor>,
    Without<ChildOf>,
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
/// Missing resources/models/hulls, unsupported or liquid source records, malformed chunks,
/// intersecting slopes/holes, tile boundaries and height mismatches fail closed. This is not a
/// general navigation mesh, liquid query, or a replacement for an obstacle sweep.
#[derive(SystemParam)]
pub struct NativeTerrainCoverage<'w, 's> {
    current_map: Option<Res<'w, CurrentMap>>,
    catalog: Option<Res<'w, MapCatalogRes>>,
    streamer: Option<Res<'w, TerrainStreamer>>,
    tiles: Option<Res<'w, Assets<AdtTile>>>,
    wdts: Option<Res<'w, Assets<WdtIndex>>>,
    roots: Query<'w, 's, RootData, RootFilter>,
    hulls: Query<'w, 's, RootData, StaticFilter>,
    sources: Query<'w, 's, &'static StaticColliderSources>,
    m2s: Option<Res<'w, Assets<M2Model>>>,
    wmos: Option<Res<'w, Assets<WmoModel>>>,
    placements: Option<Res<'w, Placements>>,
    inventory: Option<Res<'w, NativeStaticMapIndex>>,
}

impl NativeTerrainCoverage<'_, '_> {
    /// Prove the complete swept XY rectangle is supported by flat, dry, fully triangulated
    /// native ADT cells. Positions are WoW feet coordinates; both Z values must equal every
    /// touched cell vertex's Z exactly. Chunk seams are allowed only at this same height.
    /// A zero radius is allowed for a bare segment.
    ///
    /// The rectangle must lie strictly within one loaded/furnished tile whose 3×3
    /// guard is source-certified, source-complete and furnished with exact live static colliders. A full
    /// map inventory must also exclude unloaded exterior hulls from the corridor.
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
            return Err(CoverageError::OutsideSupportedTile);
        }
        let (tx, ty) = (tx as i32, ty as i32);
        let mut proof = CoverageProof {
            map: map_id,
            wdt_source_digest: wdt.native_source_digest(),
            terrain_root: Entity::PLACEHOLDER,
            models: Vec::new(),
            static_roots: Vec::new(),
            static_bounds: Vec::new(),
            inventory_digest: [0; 32],
            tiles: [((0, 0), [0; 32]); 9],
        };
        let mut expected = BTreeMap::new();
        let mut model_sources = BTreeMap::new();
        let mut hull_size = (0usize, 0usize);
        let mut hull_entities = BTreeSet::new();
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
            if !adt.native_static_coverage_safe {
                return Err(CoverageError::UnsupportedSource);
            }
            self.collect_placements(
                state,
                adt,
                &mut expected,
                &mut hull_entities,
                &mut hull_size,
                &mut model_sources,
            )?;
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
        proof.models = model_sources.into_iter().collect();
        self.prove_hulls(&expected, &hull_entities, &mut proof)?;
        proof.inventory_digest = self
            .inventory
            .as_ref()
            .ok_or(CoverageError::UnsupportedSource)?
            .verify(
                directory,
                proof.wdt_source_digest,
                &proof.tiles,
                &proof.models,
                from,
                to,
                radius,
            )?;
        Ok(proof)
    }

    fn collect_placements(
        &self,
        state: &TileState,
        adt: &AdtTile,
        expected: &mut ExpectedHulls,
        hull_entities: &mut BTreeSet<Entity>,
        hull_size: &mut (usize, usize),
        model_sources: &mut BTreeMap<u32, [u8; 32]>,
    ) -> Result<(), CoverageError> {
        let ids: BTreeSet<_> = adt
            .doodads
            .iter()
            .map(|d| d.unique_id)
            .chain(adt.wmos.iter().map(|w| w.unique_id))
            .collect();
        if ids.len() != adt.doodads.len() + adt.wmos.len()
            || ids != state.placements.iter().copied().collect()
            || state.placements.len() != ids.len()
        {
            return Err(CoverageError::UnsupportedPlacements);
        }
        if ids.is_empty() {
            return Ok(());
        }
        let placements = self
            .placements
            .as_ref()
            .ok_or(CoverageError::ColliderUnavailable)?;
        let m2s = self
            .m2s
            .as_ref()
            .ok_or(CoverageError::ColliderUnavailable)?;
        let streamer = self.streamer.as_ref().ok_or(CoverageError::UnloadedMap)?;
        for d in &adt.doodads {
            let p = placements
                .by_id
                .get(&d.unique_id)
                .ok_or(CoverageError::ColliderUnavailable)?;
            let ModelHandle::M2(handle) = &p.model else {
                return Err(CoverageError::UnsupportedPlacements);
            };
            let transform = Transform {
                translation: wow_to_bevy(d.position),
                rotation: placement_rotation(d.rotation),
                scale: Vec3::splat(d.scale),
            };
            if p.transform != transform
                || !p.spawned
                || p.refs == 0
                || handle
                    .path()
                    .is_none_or(|path| path.to_string() != benilla_assets::m2_url(&d.model))
            {
                return Err(CoverageError::ColliderUnavailable);
            }
            let model = m2s.get(handle).ok_or(CoverageError::ColliderUnavailable)?;
            let source = model
                .native_collision
                .as_ref()
                .ok_or(CoverageError::UnsupportedSource)?;
            if !source.matches(model.collision.as_ref()) {
                return Err(CoverageError::UnsupportedSource);
            }
            if let Some((_, _, digest)) = expected.get(&NativeHullId::M2(d.unique_id)) {
                if *digest != source.source_digest() {
                    return Err(CoverageError::UnsupportedPlacements);
                }
            } else {
                hull_size.0 += source.hull().positions.len();
                hull_size.1 += source.hull().indices.len() / 3;
                if expected.len() >= 4096
                    || hull_size.0 > static_mesh::MAX_HULL_VERTICES
                    || hull_size.1 > static_mesh::MAX_HULL_TRIANGLES
                {
                    return Err(CoverageError::UnsupportedPlacements);
                }
                let (vertices, triangles) =
                    super::collider::placement_collider_data(model.collision.as_ref(), &transform)
                        .unwrap_or_default();
                expected.insert(
                    NativeHullId::M2(d.unique_id),
                    (vertices, triangles, source.source_digest()),
                );
                model_sources.insert(d.unique_id, source.source_digest());
            }
            // A weld can belong to a previously resident neighbor. Its complete membership
            // must still be in this raw guard manifest; no missing owner becomes clear space.
            let owner = streamer
                .tiles
                .get(&p.owner)
                .ok_or(CoverageError::ColliderUnavailable)?;
            if owner.welds.len() > 4096 || p.entities.len() > 65_536 {
                return Err(CoverageError::UnsupportedPlacements);
            }
            hull_entities.extend(owner.welds.iter().copied());
            if hull_entities.len() > 4096 {
                return Err(CoverageError::UnsupportedPlacements);
            }
            for &entity in &p.entities {
                if self.sources.get(entity).is_ok() {
                    hull_entities.insert(entity);
                    if hull_entities.len() > 4096 {
                        return Err(CoverageError::UnsupportedPlacements);
                    }
                }
            }
        }
        self.collect_wmos(adt, expected, hull_entities, hull_size, model_sources)?;
        Ok(())
    }

    fn collect_wmos(
        &self,
        adt: &AdtTile,
        expected: &mut ExpectedHulls,
        entities: &mut BTreeSet<Entity>,
        size: &mut (usize, usize),
        models: &mut BTreeMap<u32, [u8; 32]>,
    ) -> Result<(), CoverageError> {
        if adt.wmos.is_empty() {
            return Ok(());
        }
        let placements = self
            .placements
            .as_ref()
            .ok_or(CoverageError::ColliderUnavailable)?;
        let wmos = self
            .wmos
            .as_ref()
            .ok_or(CoverageError::ColliderUnavailable)?;
        let m2s = self
            .m2s
            .as_ref()
            .ok_or(CoverageError::ColliderUnavailable)?;
        let inventory = self
            .inventory
            .as_ref()
            .ok_or(CoverageError::UnsupportedSource)?;
        for w in &adt.wmos {
            let p = placements
                .by_id
                .get(&w.unique_id)
                .ok_or(CoverageError::ColliderUnavailable)?;
            let ModelHandle::Wmo(handle) = &p.model else {
                return Err(CoverageError::UnsupportedPlacements);
            };
            let transform = Transform {
                translation: wow_to_bevy(w.position),
                rotation: placement_rotation(w.rotation),
                scale: Vec3::ONE,
            };
            if p.transform != transform
                || !p.spawned
                || p.refs == 0
                || p.doodad_set != w.doodad_set
                || p.name_set != w.name_set
                || handle
                    .path()
                    .is_none_or(|path| path.to_string() != benilla_assets::wmo_url(&w.model))
            {
                return Err(CoverageError::ColliderUnavailable);
            }
            let model = wmos.get(handle).ok_or(CoverageError::ColliderUnavailable)?;
            let source = model
                .native_collision
                .as_ref()
                .ok_or(CoverageError::UnsupportedSource)?;
            let (_, complete) = inventory
                .wmo(w.unique_id)
                .ok_or(CoverageError::UnsupportedSource)?;
            if source.source_digest() != complete.collision().source_digest()
                || !source.matches(model.collision.as_ref(), model.collision_camera.as_ref())
            {
                return Err(CoverageError::UnsupportedSource);
            }
            let selected = source
                .root()
                .selected_doodads(w.doodad_set)
                .ok_or(CoverageError::UnsupportedPlacements)?;
            if p.doodads.len() != selected.len()
                || p.doodads.len() > 4096
                || p.entities.len() > 65_536
            {
                return Err(CoverageError::UnsupportedPlacements);
            }
            insert_hull(
                expected,
                size,
                NativeHullId::WmoWalk(w.unique_id),
                source.walking(),
                transform,
                source.source_digest(),
            )?;
            insert_hull(
                expected,
                size,
                NativeHullId::WmoCamera(w.unique_id),
                source.camera(),
                transform,
                source.source_digest(),
            )?;
            for (actual, di) in p.doodads.iter().zip(selected) {
                let d = &source.root().doodads()[di];
                let transform =
                    transform.mul_transform(wmo_doodad_local(d.position, d.orientation, d.scale));
                if !actual.spawned
                    || actual.source_index as usize != di
                    || actual.transform != transform
                    || actual
                        .handle
                        .path()
                        .is_none_or(|path| path.to_string() != benilla_assets::m2_url(&d.model))
                {
                    return Err(CoverageError::ColliderUnavailable);
                }
                // A failed load can set spawned=true in the renderer. The actual asset and
                // strict source witness are mandatory, even for authored hull-less doodads.
                let m = m2s
                    .get(&actual.handle)
                    .ok_or(CoverageError::ColliderUnavailable)?;
                let c = m
                    .native_collision
                    .as_ref()
                    .ok_or(CoverageError::UnsupportedSource)?;
                if complete
                    .doodad(di)
                    .is_none_or(|expected| expected.source_digest() != c.source_digest())
                    || !c.matches(m.collision.as_ref())
                {
                    return Err(CoverageError::UnsupportedSource);
                }
                insert_hull(
                    expected,
                    size,
                    NativeHullId::WmoDoodad(w.unique_id, di as u32),
                    c.hull(),
                    transform,
                    c.source_digest(),
                )?;
            }
            if models
                .insert(w.unique_id, complete.source_digest())
                .is_some_and(|old| old != complete.source_digest())
            {
                return Err(CoverageError::UnsupportedSource);
            }
            for &entity in &p.entities {
                if self.sources.get(entity).is_ok() {
                    entities.insert(entity);
                }
                if entities.len() > 4096 {
                    return Err(CoverageError::UnsupportedPlacements);
                }
            }
        }
        Ok(())
    }

    fn prove_hulls(
        &self,
        expected: &ExpectedHulls,
        entities: &BTreeSet<Entity>,
        proof: &mut CoverageProof,
    ) -> Result<(), CoverageError> {
        let mut found = BTreeSet::new();
        let (mut total_vertices, mut total_triangles) = (0usize, 0usize);
        for (vertices, triangles, _) in expected.values() {
            total_vertices += vertices.len();
            total_triangles += triangles.len();
            if total_vertices > static_mesh::MAX_HULL_VERTICES
                || total_triangles > static_mesh::MAX_HULL_TRIANGLES
            {
                return Err(CoverageError::UnsupportedPlacements);
            }
            if !triangles.is_empty() {
                let min = vertices
                    .iter()
                    .copied()
                    .fold(Vec3::splat(f32::INFINITY), Vec3::min);
                let max = vertices
                    .iter()
                    .copied()
                    .fold(Vec3::splat(f32::NEG_INFINITY), Vec3::max);
                proof.static_bounds.push((min, max));
            }
        }
        for &entity in entities {
            let sources = self
                .sources
                .get(entity)
                .map_err(|_| CoverageError::ColliderUnavailable)?;
            if sources.0.is_empty() || sources.0.len() > 128 {
                return Err(CoverageError::UnsupportedPlacements);
            }
            let (collider, body, transform, global, position, rotation, layers) = self
                .hulls
                .get(entity)
                .map_err(|_| CoverageError::ColliderUnavailable)?;
            let wanted = match sources.0.first() {
                Some(NativeHullId::WmoWalk(_)) => crate::collision::walk_layers(),
                Some(NativeHullId::WmoCamera(_)) => crate::collision::camera_layers(),
                _ => CollisionLayers::DEFAULT,
            };
            if sources.0.iter().any(|id| match id {
                NativeHullId::WmoWalk(_) => wanted != crate::collision::walk_layers(),
                NativeHullId::WmoCamera(_) => wanted != crate::collision::camera_layers(),
                _ => wanted != CollisionLayers::DEFAULT,
            }) || !identity_static(body, transform, global, position, rotation, None)
                || layers.copied().unwrap_or_default() != wanted
            {
                return Err(CoverageError::ColliderUnavailable);
            }
            let (mut vertices, mut triangles) = (Vec::new(), Vec::new());
            for id in &sources.0 {
                if !found.insert(*id) {
                    return Err(CoverageError::ColliderUnavailable);
                }
                let (v, t, _) = expected
                    .get(id)
                    .ok_or(CoverageError::UnsupportedPlacements)?;
                if t.is_empty() {
                    return Err(CoverageError::ColliderUnavailable);
                }
                let base = vertices.len() as u32;
                vertices.extend(v.iter().map(|v| v.to_array()));
                triangles.extend(t.iter().map(|t| t.map(|i| i + base)));
            }
            let Some(mesh) = collider.shape_scaled().as_trimesh() else {
                return Err(CoverageError::ColliderUnavailable);
            };
            if !static_mesh::matches(
                &vertices,
                &triangles,
                mesh.vertices().iter().map(|v| v.to_array()),
                mesh.indices(),
            ) {
                return Err(CoverageError::ColliderUnavailable);
            }
            proof.static_roots.push(entity);
        }
        if expected
            .iter()
            .any(|(id, (_, triangles, _))| !triangles.is_empty() && !found.contains(id))
        {
            return Err(CoverageError::ColliderUnavailable);
        }
        Ok(())
    }

    fn live_collider(&self, state: &TileState) -> Result<&Collider, CoverageError> {
        let root = state.entity.ok_or(CoverageError::ColliderUnavailable)?;
        let (collider, body, transform, global, position, rotation, layers) = self
            .roots
            .get(root)
            .map_err(|_| CoverageError::ColliderUnavailable)?;
        if !identity_static(body, transform, global, position, rotation, layers) {
            return Err(CoverageError::ColliderUnavailable);
        }
        Ok(collider)
    }
}

fn insert_hull(
    expected: &mut ExpectedHulls,
    size: &mut (usize, usize),
    id: NativeHullId,
    hull: &benilla_formats::CollisionMesh,
    transform: Transform,
    digest: [u8; 32],
) -> Result<(), CoverageError> {
    if let Some((_, _, prior)) = expected.get(&id) {
        return if *prior == digest {
            Ok(())
        } else {
            Err(CoverageError::UnsupportedSource)
        };
    }
    size.0 += hull.positions.len();
    size.1 += hull.indices.len() / 3;
    if expected.len() >= 4096
        || size.0 > static_mesh::MAX_HULL_VERTICES
        || size.1 > static_mesh::MAX_HULL_TRIANGLES
    {
        return Err(CoverageError::UnsupportedPlacements);
    }
    let (vertices, triangles) =
        super::collider::placement_collider_data(Some(hull), &transform).unwrap_or_default();
    if vertices.iter().any(|p| !p.is_finite()) {
        return Err(CoverageError::UnsupportedSource);
    }
    expected.insert(id, (vertices, triangles, digest));
    Ok(())
}

fn identity_static(
    body: &RigidBody,
    transform: &Transform,
    global: &GlobalTransform,
    position: &Position,
    rotation: &Rotation,
    layers: Option<&CollisionLayers>,
) -> bool {
    *body == RigidBody::Static
        && *transform == Transform::IDENTITY
        && *global == GlobalTransform::IDENTITY
        && position.0 == Vec3::ZERO
        && rotation.0 == Quat::IDENTITY
        && layers.is_none_or(|layers| *layers == CollisionLayers::DEFAULT)
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
        let vertices = &mesh.vertices()[vertex_offset..vertex_offset + chunk.positions.len()];
        for (vertex, position) in vertices.iter().zip(&chunk.positions) {
            if *vertex != wow_to_bevy(*position) {
                return false;
            }
        }
        let source_triangles = chunk.indices.as_chunks::<3>().0;
        let triangles = &mesh.indices()[triangle_offset..triangle_offset + source_triangles.len()];
        let base = vertex_offset as u32;
        for (actual, triangle) in triangles.iter().zip(source_triangles) {
            if *actual != [base + triangle[0], base + triangle[1], base + triangle[2]] {
                return false;
            }
        }
        vertex_offset += chunk.positions.len();
        triangle_offset += chunk.indices.len() / 3;
    }
    true
}
