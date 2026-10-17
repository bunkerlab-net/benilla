//! Complete static-map source closure with bounded spatial buckets. This does not certify
//! non-render participants, server masks or an independently changing asset-source epoch.
use super::static_mesh::ExteriorIndex;
use super::CoverageError;
use benilla_assets::coords::{bevy_to_wow, placement_rotation, wmo_doodad_local, wow_to_bevy};
use benilla_formats::{NativeM2MapInventory, NativeWmoMapModel, WmoInstance};
use bevy::prelude::*;

/// Installing/replacing this immutable source snapshot is a compiler/controller responsibility.
#[derive(Resource)]
pub struct NativeStaticMapIndex {
    source: NativeM2MapInventory,
    exterior: ExteriorIndex,
}
impl NativeStaticMapIndex {
    pub fn from_inventory(source: NativeM2MapInventory) -> Result<Self, CoverageError> {
        let mut exterior = ExteriorIndex::default();
        for (d, c) in source.placements() {
            let transform = Transform {
                translation: wow_to_bevy(d.position),
                rotation: placement_rotation(d.rotation),
                scale: Vec3::splat(d.scale),
            };
            let mut bounds = Bounds::default();
            bounds.include(c.hull(), transform)?;
            bounds.insert(&mut exterior, d.unique_id, c.source_digest())?;
        }
        for (w, c) in source.wmo_placements() {
            let transform = Transform {
                translation: wow_to_bevy(w.position),
                rotation: placement_rotation(w.rotation),
                scale: Vec3::ONE,
            };
            let mut bounds = Bounds::default();
            bounds.include(c.collision().walking(), transform)?;
            bounds.include(c.collision().camera(), transform)?;
            for di in c
                .collision()
                .root()
                .selected_doodads(w.doodad_set)
                .ok_or(CoverageError::UnsupportedSource)?
            {
                let d = &c.collision().root().doodads()[di];
                let local = wmo_doodad_local(d.position, d.orientation, d.scale);
                bounds.include(
                    c.doodad(di).ok_or(CoverageError::UnsupportedSource)?.hull(),
                    transform.mul_transform(local),
                )?;
            }
            bounds.insert(&mut exterior, w.unique_id, c.source_digest())?;
        }
        Ok(Self { source, exterior })
    }
    pub(super) fn wmo(&self, id: u32) -> Option<(&WmoInstance, &NativeWmoMapModel)> {
        self.source.wmo(id)
    }
    pub(super) fn verify(
        &self,
        directory: &str,
        wdt: [u8; 32],
        tiles: &[((i32, i32), [u8; 32]); 9],
        models: &[(u32, [u8; 32])],
        from: [f32; 3],
        to: [f32; 3],
        radius: f32,
    ) -> Result<[u8; 32], CoverageError> {
        if self.source.directory() != directory
            || self.source.wdt_digest() != wdt
            || tiles
                .iter()
                .any(|(tile, digest)| self.source.tile_digest(*tile) != Some(*digest))
        {
            return Err(CoverageError::UnsupportedSource);
        }
        if !self.exterior.covers(from, to, radius, models) {
            return Err(CoverageError::ColliderUnavailable);
        }
        Ok(self.source.source_digest())
    }
}
#[derive(Default)]
struct Bounds(Option<(Vec3, Vec3)>);
impl Bounds {
    fn include(
        &mut self,
        hull: &benilla_formats::CollisionMesh,
        transform: Transform,
    ) -> Result<(), CoverageError> {
        if hull.indices.is_empty() {
            return Ok(());
        }
        // Same arithmetic as the producer, including composed MODD transforms. An authored
        // MODF/MOGI box is never trusted to bound geometry outside its declaring ADT tile.
        for &p in &hull.positions {
            let p = transform.transform_point(wow_to_bevy(p));
            if !p.is_finite() {
                return Err(CoverageError::UnsupportedSource);
            }
            let (min, max) = self.0.get_or_insert((p, p));
            *min = min.min(p);
            *max = max.max(p);
        }
        Ok(())
    }
    fn insert(
        self,
        index: &mut ExteriorIndex,
        id: u32,
        digest: [u8; 32],
    ) -> Result<(), CoverageError> {
        if let Some((min, max)) = self.0 {
            let (a, b) = (bevy_to_wow(min), bevy_to_wow(max));
            if !index.insert(
                id,
                digest,
                [a[0].min(b[0]), a[1].min(b[1])],
                [a[0].max(b[0]), a[1].max(b[1])],
            ) {
                return Err(CoverageError::UnsupportedPlacements);
            }
        }
        Ok(())
    }
}
