// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical scalar height from compatible genuine terrain/collision consumed inputs.
//! No complete dry inventory, navigation, dynamic-world or gameplay admission.
use super::{
    height,
    liquid::world_tile,
    terrain::TerrainTile,
    vmap::{self, prepared::PrepareError, tile_prepared::PreparedTileCollision},
    Error, Hash, PointHeight, TileId,
};
use benilla_native_terrain::PreparedTerrain;
use benilla_native_vmap_source::AssembledSourceMap;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Instant};
mod environment;
mod initialization;
pub(crate) use initialization::ResidentInitializationConfiguration;
pub(crate) mod inventory;
mod point_liquid;
pub(crate) use point_liquid::{PointNoWaterReply, PointNoWaterRequest};

const POLICY: &[u8] = b"benilla-compatible-consumed-source-static-height-observation-v1";
const TERRAIN_POLICY: &str = "benilla-chain-and-cmangos-ad-fixed-five-exact-winner-v3";
const COLLISION_POLICY: &str = "cmangos-vmap-numeric-1-through-99-reverse-first-hit-v1";
const MAX_RECEIPTS: usize = 32_768;
#[derive(Debug)]
pub(crate) enum CompositionError {
    Cancelled,
    TerrainSource,
    Collision(PrepareError),
    Validation(Error),
}
impl From<Error> for CompositionError {
    fn from(value: Error) -> Self {
        Self::Validation(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub map: u32,
    pub revision: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeightRequest {
    pub binding: Binding,
    pub ticket: u64,
    pub position: [u32; 3],
    pub max_search: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct HeightReply {
    pub request: HeightRequest,
    pub height: u32,
}
pub(crate) struct PreparedSourceHeight<'terrain, 'bundle, 'source> {
    terrain: &'terrain PreparedTerrain,
    collision: PreparedTileCollision<'bundle, 'source>,
    tiles: BTreeMap<TileId, TerrainTile>,
    binding: Binding,
}
impl<'terrain, 'bundle, 'source> PreparedSourceHeight<'terrain, 'bundle, 'source> {
    /// Fixed source Chase LOS geometry primitive. It carries no model-height or
    /// caller-gate authority; the resident provider validates those separately.
    pub(crate) fn chase_ray(
        &self,
        ticket: u64,
        from: [u32; 3],
        to: [u32; 3],
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<vmap::RayReply, CompositionError> {
        let mut budget = vmap::QueryBudget::new(deadline);
        self.source_ray_with_budget(ticket, from, to, &mut budget, cancelled)
    }

    /// Fixed ignore-M2=false ray with one caller-owned cumulative work budget.
    /// This is a geometry observation, not a Chase or NearPoint caller certificate.
    pub(crate) fn source_ray_with_budget(
        &self,
        ticket: u64,
        from: [u32; 3],
        to: [u32; 3],
        budget: &mut vmap::QueryBudget,
        cancelled: impl Fn() -> bool,
    ) -> Result<vmap::RayReply, CompositionError> {
        check(&cancelled)?;
        budget.complete()?;
        self.verify_unchanged(&cancelled)?;
        check(&cancelled)?;
        let request = vmap::RayRequest {
            snapshot: self.collision.binding(),
            ticket,
            from,
            to,
            ignore_m2: false,
        };
        let reply = self.collision.query_with_budget(request, budget)?;
        if reply.request != request {
            return Err(Error::Identity.into());
        }
        self.verify_unchanged(&cancelled)?;
        check(&cancelled)?;
        budget.complete()?;
        Ok(reply)
    }

    pub(crate) fn from_source(
        terrain: &'terrain PreparedTerrain,
        source: &'bundle AssembledSourceMap<'source>,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, CompositionError> {
        check(&cancelled)?;
        terrain
            .verify_unchanged()
            .map_err(|_| CompositionError::TerrainSource)?;
        source
            .verify_unchanged(&cancelled)
            .map_err(|e| CompositionError::Collision(PrepareError::Source(e)))?;
        if terrain.map_id() != source.map().map_id() {
            return Err(Error::Identity.into());
        }
        compatible_receipts(terrain, source, &cancelled)?;
        let mut tiles = BTreeMap::new();
        let mut selected = Vec::new();
        selected.try_reserve_exact(9).map_err(|_| Error::Budget)?;
        let mut digest = Sha256::new();
        digest.update(POLICY);
        digest.update(terrain.map_id().to_le_bytes());
        digest.update(terrain.source_epoch().as_bytes());
        for (tile, bytes) in terrain.tiles() {
            check(&cancelled)?;
            if selected.len() >= 9 {
                return Err(Error::Budget.into());
            }
            let id = TileId {
                x: tile.x,
                y: tile.y,
            };
            let decoded = TerrainTile::decode(id, bytes)?;
            if tiles.insert(id, decoded).is_some() {
                return Err(Error::Duplicate.into());
            }
            selected.push(id);
            digest.update([id.x, id.y]);
            digest.update((bytes.len() as u64).to_le_bytes());
            for chunk in bytes.chunks(64 * 1024) {
                check(&cancelled)?;
                digest.update(chunk);
            }
        }
        if selected.is_empty() {
            return Err(Error::Missing.into());
        }
        let collision = PreparedTileCollision::from_source(source, &selected, &cancelled)
            .map_err(CompositionError::Collision)?;
        digest.update(collision.binding().inventory);
        digest.update(collision.source_version());
        let result = Self {
            terrain,
            collision,
            tiles,
            binding: Binding {
                map: terrain.map_id(),
                revision: digest.finalize().into(),
            },
        };
        result.verify_unchanged(&cancelled)?;
        check(&cancelled)?;
        Ok(result)
    }
    pub(crate) fn generated_query_source_epoch(&self) -> Hash {
        super::nav_generation::query_source_epoch(
            self.terrain.source_epoch(),
            self.collision.source_version(),
        )
    }
    pub(crate) fn binding(&self) -> Binding {
        self.binding
    }
    pub(crate) fn verify_unchanged(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), CompositionError> {
        check(&cancelled)?;
        self.terrain
            .verify_unchanged()
            .map_err(|_| CompositionError::TerrainSource)?;
        self.collision
            .verify_unchanged(&cancelled)
            .map_err(CompositionError::Collision)?;
        check(&cancelled)
    }
    pub(crate) fn query(
        &self,
        request: HeightRequest,
        deadline: Instant,
    ) -> Result<HeightReply, Error> {
        let mut budget = vmap::QueryBudget::new(deadline);
        self.query_with_budget(request, &mut budget)
    }
    pub(crate) fn query_with_budget(
        &self,
        request: HeightRequest,
        budget: &mut vmap::QueryBudget,
    ) -> Result<HeightReply, Error> {
        if request.binding != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let p = request.position.map(f32::from_bits);
        let search = f32::from_bits(request.max_search);
        if p.iter().any(|v| !v.is_finite()) || !search.is_finite() {
            return Err(Error::Format);
        }
        if p.iter().any(|v| v.abs() > 100_000.0) || search.abs() > 10_000.0 {
            return Err(Error::Unsupported);
        }
        budget.complete()?;
        // Fixed terrain lookup/interpolation shares the traversal allowance.
        budget.take()?;
        let tile = self
            .tiles
            .get(&world_tile(p[0], p[1])?)
            .ok_or(Error::Missing)?;
        let terrain = match tile.point_height(p[0], p[1])? {
            PointHeight::Height(h) => h,
            PointHeight::Hole => -200_000.0,
        };
        let value = height::select_height(terrain, p[2], search, |z, distance| {
            self.collision
                .height_with_budget([p[0], p[1], z], distance, budget)
        })?;
        budget.complete()?;
        Ok(HeightReply {
            request,
            height: value.to_bits(),
        })
    }
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), CompositionError> {
    if cancelled() {
        Err(CompositionError::Cancelled)
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Outcome<'a> {
    Present(&'a str, &'a str, usize),
    Absent,
    Dummy(&'a str, usize),
}
pub(super) fn compatible_receipts(
    terrain: &PreparedTerrain,
    source: &AssembledSourceMap<'_>,
    cancelled: &impl Fn() -> bool,
) -> Result<(), CompositionError> {
    if terrain.source_policy() != TERRAIN_POLICY
        || source.source_policy() != COLLISION_POLICY
        || benilla_native_terrain::SOURCE_PIN != benilla_native_vmap_source::SOURCE_PIN
    {
        return Err(Error::Unsupported.into());
    }
    let mut collision = BTreeMap::new();
    let mut count = 0usize;
    for r in source.source_receipts() {
        check(cancelled)?;
        count += 1;
        if count > MAX_RECEIPTS {
            return Err(Error::Budget.into());
        }
        let outcome = match r.outcome() {
            benilla_native_vmap_source::SourceOutcome::Present {
                winning_archive,
                sha256,
                bytes,
            } => Outcome::Present(winning_archive, sha256, bytes),
            benilla_native_vmap_source::SourceOutcome::Absent => Outcome::Absent,
            benilla_native_vmap_source::SourceOutcome::Dummy {
                winning_archive,
                bytes,
            } => Outcome::Dummy(winning_archive, bytes),
        };
        if collision
            .insert(r.name(), outcome)
            .is_some_and(|old| old != outcome)
        {
            return Err(Error::Dependency.into());
        }
    }
    let mut required = Vec::new();
    required.try_reserve_exact(11).map_err(|_| Error::Budget)?;
    required.push("DBFilesClient\\Map.dbc".to_owned());
    let directory = terrain.map_directory();
    required.push(format!("World\\Maps\\{directory}\\{directory}.wdt"));
    for (tile, _) in terrain.tiles() {
        if required.len() >= 11 {
            return Err(Error::Budget.into());
        }
        required.push(format!(
            "World\\Maps\\{directory}\\{directory}_{}_{}.adt",
            tile.y, tile.x
        ));
    }
    let mut required_seen = [false; 11];
    count = 0;
    for r in terrain.source_receipts() {
        check(cancelled)?;
        count += 1;
        if count > MAX_RECEIPTS {
            return Err(Error::Budget.into());
        }
        let outcome = match r.outcome() {
            benilla_native_terrain::SourceOutcome::Present {
                winning_archive,
                sha256,
                bytes,
            } => Outcome::Present(winning_archive, sha256, bytes),
            benilla_native_terrain::SourceOutcome::Absent => Outcome::Absent,
        };
        if let Some(other) = collision.get(r.name()) {
            if *other != outcome {
                return Err(Error::Dependency.into());
            }
        }
        if let Some(index) = required.iter().position(|name| name == r.name()) {
            if !matches!(outcome, Outcome::Present(..)) || collision.get(r.name()) != Some(&outcome)
            {
                return Err(Error::Missing.into());
            }
            required_seen[index] = true;
        }
    }
    if required_seen[..required.len()].iter().any(|seen| !*seen) {
        return Err(Error::Missing.into());
    }
    check(cancelled)
}
