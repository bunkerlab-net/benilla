// SPDX-License-Identifier: GPL-3.0-or-later
//! Genuine selected-tile extraction inventory to a bounded separate native generator.
//! Inventory-relative neighbor absence is explicit; it is not whole-install coverage.
#[cfg(test)]
mod byte_assert;
#[cfg(test)]
pub(crate) use byte_assert::assert_navigation_bytes;
mod process;
mod query_provider;
#[cfg(test)]
pub(crate) use query_provider::assert_source_change_after_query_load;
pub(super) use query_provider::{load_query_set, source_epoch as query_source_epoch};
pub(super) mod terrain_mesh;
use super::{
    source_height,
    vmap::{prepared::PrepareError, tile_prepared::PreparedTileCollision},
    Error, Hash, TerrainTile, TileId,
};
use benilla_native_nav_generator::{
    GenerationRequest, GenerationResponse, GenerationStatus, MapParams, SourceConfig, TriangleMesh,
    DEFAULT_ALLOCATION_BUDGET, GRID_SIZE,
};
use benilla_native_terrain::PreparedTerrain;
use benilla_native_vmap_source::AssembledSourceMap;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path, time::Instant};
const POLICY: &[u8] = b"benilla-source-extraction-inventory-navigation-v2-stable-bv-v1";
#[derive(Debug)]
pub(crate) enum GenerationError {
    Cancelled,
    Deadline,
    Io,
    Protocol,
    Helper,
    Validation(Error),
    Composition(source_height::CompositionError),
    Collision(PrepareError),
    Native(benilla_native_nav::Error),
}
impl From<Error> for GenerationError {
    fn from(value: Error) -> Self {
        Self::Validation(value)
    }
}
/// Exact successful extraction products establish this inventory, never WDT flags.
/// Expanding the inventory changes identity, may change origin, and requires regeneration.
pub(crate) struct PreparedNavigationInput<'terrain, 'bundle, 'source> {
    terrain: &'terrain PreparedTerrain,
    source: &'bundle AssembledSourceMap<'source>,
    tile: TileId,
    revision: Hash,
    request: GenerationRequest,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct GenerationSummary {
    pub status: GenerationStatus,
    pub polygon_count: u32,
    pub output_bytes: usize,
    pub native_peak_bytes: u64,
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct InputSummary {
    pub tiles: usize,
    pub solid_vertices: usize,
    pub solid_triangles: usize,
    pub liquid_vertices: usize,
    pub liquid_triangles: usize,
    pub request_bytes: usize,
    pub revision: Hash,
}
pub(crate) struct GeneratedNavigation {
    input_revision: Hash,
    revision: Hash,
    tile: TileId,
    response: GenerationResponse,
}
impl<'terrain, 'bundle, 'source> PreparedNavigationInput<'terrain, 'bundle, 'source> {
    pub(crate) fn from_source(
        terrain: &'terrain PreparedTerrain,
        source: &'bundle AssembledSourceMap<'source>,
        tile: TileId,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, GenerationError> {
        check(&cancelled)?;
        tile.validate()?;
        terrain.verify_unchanged().map_err(|_| {
            GenerationError::Composition(source_height::CompositionError::TerrainSource)
        })?;
        source
            .verify_unchanged(&cancelled)
            .map_err(|e| GenerationError::Collision(PrepareError::Source(e)))?;
        if terrain.map_id() != source.map().map_id() {
            return Err(Error::Identity.into());
        }
        source_height::compatible_receipts(terrain, source, &cancelled)
            .map_err(GenerationError::Composition)?;
        let mut union = BTreeSet::new();
        let mut digest = Sha256::new();
        digest.update(POLICY);
        digest.update(terrain.source_epoch().as_bytes());
        digest.update(source.map().source_binding());
        digest.update(terrain.map_id().to_le_bytes());
        digest.update([tile.x, tile.y]);
        for (id, bytes) in terrain.tiles() {
            check(&cancelled)?;
            TerrainTile::decode(TileId { x: id.x, y: id.y }, bytes)?;
            union.insert((id.y, id.x));
            digest.update([id.x, id.y]);
            digest.update((bytes.len() as u64).to_le_bytes());
            for chunk in bytes.chunks(65536) {
                check(&cancelled)?;
                digest.update(chunk);
            }
        }
        for product in source.map().tiles() {
            check(&cancelled)?;
            let (x, y) = product.coordinates();
            if x >= 64 || y >= 64 {
                return Err(Error::Unsupported.into());
            }
            union.insert((x as u8, y as u8));
        }
        if union.is_empty() || union.len() > 4096 {
            return Err(Error::Budget.into());
        }
        if !union.contains(&(tile.y, tile.x)) {
            return Err(Error::Missing.into());
        }
        let mut max_x = 0;
        let mut max_y = 0;
        for &(x, y) in &union {
            digest.update([x, y]);
            max_x = max_x.max(x);
            max_y = max_y.max(y);
        }
        let map = MapParams {
            origin: [
                (32 - i32::from(max_x)) as f32 * GRID_SIZE - GRID_SIZE,
                f32::MIN_POSITIVE,
                (32 - i32::from(max_y)) as f32 * GRID_SIZE - GRID_SIZE,
            ],
            tile_width: GRID_SIZE,
            tile_height: GRID_SIZE,
            max_tiles: union.len() as u32,
            max_polys: 0x8000_0000,
        };
        let mut mesh = terrain_mesh::Mesh::default();
        let main = terrain
            .tiles()
            .find(|(id, _)| id.x == tile.x && id.y == tile.y);
        let mut loaded = false;
        if let Some((_, bytes)) = main {
            loaded = terrain_mesh::append_map(
                &mut mesh,
                tile.y,
                tile.x,
                bytes,
                terrain_mesh::Portion::Entire,
                &cancelled,
            )?;
        }
        digest.update([u8::from(main.is_some())]);
        // Exact TerrainBuilder loadMap order; absent means absent in this inventory.
        for (x, y, portion) in [
            (
                i16::from(tile.y) + 1,
                i16::from(tile.x),
                terrain_mesh::Portion::Left,
            ),
            (
                i16::from(tile.y) - 1,
                i16::from(tile.x),
                terrain_mesh::Portion::Right,
            ),
            (
                i16::from(tile.y),
                i16::from(tile.x) + 1,
                terrain_mesh::Portion::Top,
            ),
            (
                i16::from(tile.y),
                i16::from(tile.x) - 1,
                terrain_mesh::Portion::Bottom,
            ),
        ] {
            check(&cancelled)?;
            let neighbor = terrain
                .tiles()
                .find(|(id, _)| i16::from(id.y) == x && i16::from(id.x) == y);
            digest.update(x.to_le_bytes());
            digest.update(y.to_le_bytes());
            digest.update([u8::from(neighbor.is_some())]);
            if loaded {
                if let Some((_, bytes)) = neighbor {
                    terrain_mesh::append_map(
                        &mut mesh, x as u8, y as u8, bytes, portion, &cancelled,
                    )?;
                }
            }
        }
        let collision = PreparedTileCollision::from_source(source, &[tile], &cancelled)
            .map_err(GenerationError::Collision)?;
        collision.append_navigation_mesh(&mut mesh, &cancelled)?;
        clean(
            &mut mesh.solid_vertices,
            &mut mesh.solid_triangles,
            &cancelled,
        )?;
        clean(
            &mut mesh.liquid_vertices,
            &mut mesh.liquid_triangles,
            &cancelled,
        )?;
        let mut low = [0.0; 3];
        let mut high = [0.0; 3];
        let mut first = true;
        // Source concatenates liquids before solids before rcCalcBounds.
        for point in mesh.liquid_vertices.iter().chain(&mesh.solid_vertices) {
            check(&cancelled)?;
            if first {
                low = *point;
                high = *point;
                first = false;
            } else {
                for i in 0..3 {
                    if point[i] < low[i] {
                        low[i] = point[i];
                    }
                    if point[i] > high[i] {
                        high[i] = point[i];
                    }
                }
            }
        }
        if first {
            low[1] = f32::MIN_POSITIVE;
            high[1] = f32::MAX;
        }
        high[0] = (32 - i32::from(tile.y)) as f32 * GRID_SIZE;
        high[2] = (32 - i32::from(tile.x)) as f32 * GRID_SIZE;
        low[0] = high[0] - GRID_SIZE;
        low[2] = high[2] - GRID_SIZE;
        let config = source_config(terrain.map_id(), tile.y, tile.x)?;
        digest.update(benilla_native_nav_generator::SOURCE_CONFIG_JSON);
        digest.update(benilla_native_nav_generator::SOURCE_OFFMESH);
        let request = GenerationRequest {
            map_id: terrain.map_id(),
            tile_x: u32::from(tile.y),
            tile_y: u32::from(tile.x),
            uses_liquids: true,
            allocation_budget_bytes: DEFAULT_ALLOCATION_BUDGET,
            map,
            bmin: low,
            bmax: high,
            config,
            solid: TriangleMesh {
                vertices: mesh.solid_vertices,
                triangles: mesh.solid_triangles,
            },
            liquid: TriangleMesh {
                vertices: mesh.liquid_vertices,
                triangles: mesh.liquid_triangles,
            },
            liquid_areas: mesh.liquid_areas,
        };
        let result = Self {
            terrain,
            source,
            tile,
            revision: digest.finalize().into(),
            request,
        };
        result.verify_unchanged(&cancelled)?;
        Ok(result)
    }
    pub(crate) fn summary(&self) -> Result<InputSummary, GenerationError> {
        Ok(InputSummary {
            tiles: self.terrain.tiles().count(),
            solid_vertices: self.request.solid.vertices.len(),
            solid_triangles: self.request.solid.triangles.len(),
            liquid_vertices: self.request.liquid.vertices.len(),
            liquid_triangles: self.request.liquid.triangles.len(),
            request_bytes: self
                .request
                .encode()
                .map_err(|_| GenerationError::Protocol)?
                .len(),
            revision: self.revision,
        })
    }
    /// Test-only comparison of raw generation products. This never constructs an
    /// admitted navigation view; the public generation path still loads the reader.
    #[cfg(test)]
    pub(crate) fn assert_original_products_for_test(
        &self,
        helper: &Path,
        request_sha256: &str,
        original_map: &[u8],
        original_tile: &[u8],
    ) {
        self.verify_unchanged(|| false).unwrap();
        let request = self.request.encode().unwrap();
        // The independent oracle uses a 256 MiB native allocation allowance;
        // production uses 128 MiB. Normalize only that resource field for comparison.
        let mut normalized = request.clone();
        normalized[24..32].copy_from_slice(&(256u64 * 1024 * 1024).to_le_bytes());
        assert_eq!(format!("{:x}", Sha256::digest(&normalized)), request_sha256);
        let response = process::run(
            helper,
            request,
            Instant::now() + std::time::Duration::from_secs(60),
            &|| false,
        )
        .unwrap();
        validate_response(&self.request, &response).unwrap();
        assert_navigation_bytes(&response.mmap, original_map, "original MMap");
        assert_navigation_bytes(&response.mmtile, original_tile, "original MMTile");
        self.verify_unchanged(|| false).unwrap();
    }
    pub(crate) fn verify_unchanged(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<(), GenerationError> {
        check(&cancelled)?;
        self.terrain.verify_unchanged().map_err(|_| {
            GenerationError::Composition(source_height::CompositionError::TerrainSource)
        })?;
        self.source
            .verify_unchanged(&cancelled)
            .map_err(|e| GenerationError::Collision(PrepareError::Source(e)))?;
        check(&cancelled)
    }
    pub(crate) fn generate(
        &self,
        helper: &Path,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<GeneratedNavigation, GenerationError> {
        self.generate_observed(helper, deadline, cancelled, |_| {})
    }
    /// The observer receives only bounded metadata after framing, exact requested map-parameter binding,
    /// and consistent count/liquid metadata; it does not establish production validation or source freshness.
    pub(crate) fn generate_observed(
        &self,
        helper: &Path,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        mut observed: impl FnMut(GenerationSummary),
    ) -> Result<GeneratedNavigation, GenerationError> {
        self.verify_unchanged(&cancelled)?;
        if Instant::now() >= deadline {
            return Err(GenerationError::Deadline);
        }
        let request = self
            .request
            .encode()
            .map_err(|_| GenerationError::Protocol)?;
        let response = process::run(helper, request, deadline, &cancelled)?;
        validate_response(&self.request, &response)?;
        observed(summary(&response));
        // The independently validated production Detour reader remains mandatory.
        if response.status == GenerationStatus::Generated {
            benilla_native_nav::Mesh::load(
                &response.mmap,
                &[benilla_native_nav::TileInput {
                    grid: [self.tile.x, self.tile.y],
                    bytes: &response.mmtile,
                }],
            )
            .map_err(GenerationError::Native)?;
        }
        self.verify_unchanged(&cancelled)?;
        if Instant::now() >= deadline {
            return Err(GenerationError::Deadline);
        }
        let mut digest = Sha256::new();
        digest.update(self.revision);
        digest.update(&response.mmap);
        digest.update(&response.mmtile);
        Ok(GeneratedNavigation {
            input_revision: self.revision,
            revision: digest.finalize().into(),
            tile: self.tile,
            response,
        })
    }
}
fn summary(response: &GenerationResponse) -> GenerationSummary {
    GenerationSummary {
        status: response.status,
        polygon_count: response.polygon_count,
        output_bytes: response.mmap.len() + response.mmtile.len(),
        native_peak_bytes: response.native_peak_bytes,
    }
}
impl GeneratedNavigation {
    pub(crate) fn summary(&self) -> GenerationSummary {
        summary(&self.response)
    }

    pub(crate) fn products(&self) -> (&[u8], &[u8]) {
        (&self.response.mmap, &self.response.mmtile)
    }
    pub(crate) fn revision(&self) -> Hash {
        self.revision
    }
    pub(crate) fn load(&self) -> Result<benilla_native_nav::Mesh, GenerationError> {
        if self.response.status != GenerationStatus::Generated {
            return Err(Error::Missing.into());
        }
        benilla_native_nav::Mesh::load(
            &self.response.mmap,
            &[benilla_native_nav::TileInput {
                grid: [self.tile.x, self.tile.y],
                bytes: &self.response.mmtile,
            }],
        )
        .map_err(GenerationError::Native)
    }
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), GenerationError> {
    if cancelled() {
        Err(GenerationError::Cancelled)
    } else {
        Ok(())
    }
}
fn clean(
    vertices: &mut Vec<[f32; 3]>,
    triangles: &mut [[u32; 3]],
    cancelled: &impl Fn() -> bool,
) -> Result<(), Error> {
    let mut indices = Vec::new();
    indices
        .try_reserve_exact(vertices.len())
        .map_err(|_| Error::Budget)?;
    for _ in 0..vertices.len() {
        if cancelled() {
            return Err(Error::Deadline);
        }
        indices.push(u32::MAX);
    }
    for triangle in triangles.iter() {
        if cancelled() {
            return Err(Error::Deadline);
        }
        for &index in triangle {
            *indices.get_mut(index as usize).ok_or(Error::Format)? = 0;
        }
    }
    let mut next = 0;
    for (i, index) in indices.iter_mut().enumerate() {
        if cancelled() {
            return Err(Error::Deadline);
        }
        if *index == 0 {
            vertices[next] = vertices[i];
            *index = next as u32;
            next += 1;
        }
    }
    vertices.truncate(next);
    for triangle in triangles {
        if cancelled() {
            return Err(Error::Deadline);
        }
        for index in triangle {
            *index = indices[*index as usize];
        }
    }
    Ok(())
}
fn source_config(map: u32, x: u8, y: u8) -> Result<SourceConfig, GenerationError> {
    let offmesh = std::str::from_utf8(benilla_native_nav_generator::SOURCE_OFFMESH)
        .map_err(|_| GenerationError::Protocol)?;
    for line in offmesh.lines() {
        let mut fields = line.split_whitespace();
        if fields.next() == Some(map.to_string().as_str())
            && fields.next() == Some(format!("{x},{y}").as_str())
        {
            return Err(Error::Unsupported.into());
        }
    }
    let json: serde_json::Value =
        serde_json::from_slice(benilla_native_nav_generator::SOURCE_CONFIG_JSON)
            .map_err(|_| GenerationError::Protocol)?;
    let map_config = &json[map.to_string()];
    let tile_config = &map_config[format!("{x}{y}")];
    let mut config = SourceConfig::default();
    for source in [map_config, tile_config] {
        macro_rules! number {
            ($name:literal,$field:ident,$ty:ty) => {
                if let Some(value) = source.get($name) {
                    let number = value.as_f64().ok_or(GenerationError::Protocol)?;
                    if !number.is_finite() || number < 0.0 || number > u32::MAX as f64 {
                        return Err(GenerationError::Protocol);
                    }
                    config.$field = number as $ty;
                }
            };
        }
        number!("borderSize", border_size, u32);
        number!("walkableSlopeAngle", walkable_slope_angle, f32);
        number!("walkableHeight", walkable_height, u32);
        number!("walkableClimb", walkable_climb, u32);
        number!("walkableRadius", walkable_radius, u32);
        number!("maxEdgeLen", max_edge_len, u32);
        number!("maxSimplificationError", max_simplification_error, f32);
        number!("minRegionArea", min_region_area, u32);
        number!("mergeRegionArea", merge_region_area, u32);
        number!("detailSampleDist", detail_sample_dist, f32);
        number!("detailSampleMaxError", detail_sample_max_error, f32);
        number!("liquidFlagMergeThreshold", liquid_flag_merge_threshold, f32);
    }
    Ok(config)
}

fn validate_response(
    request: &GenerationRequest,
    response: &GenerationResponse,
) -> Result<(), GenerationError> {
    let mut expected = [0u8; 28];
    for (index, bits) in request
        .map
        .origin
        .iter()
        .map(|v| v.to_bits())
        .chain([
            request.map.tile_width.to_bits(),
            request.map.tile_height.to_bits(),
            request.map.max_tiles,
            request.map.max_polys,
        ])
        .enumerate()
    {
        expected[index * 4..index * 4 + 4].copy_from_slice(&bits.to_le_bytes());
    }
    if response.mmap != expected || response.native_peak_bytes > request.allocation_budget_bytes {
        return Err(GenerationError::Protocol);
    }
    if response.status == GenerationStatus::Generated {
        if response.mmtile.len() < 48 {
            return Err(GenerationError::Protocol);
        }
        let word =
            |offset| u32::from_le_bytes(response.mmtile[offset..offset + 4].try_into().unwrap());
        if word(16) != u32::from(request.uses_liquids) || word(44) != response.polygon_count {
            return Err(GenerationError::Protocol);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_config_defaults_overrides_and_offmesh_are_explicit() {
        let config = source_config(0, 36, 53).unwrap();
        assert_eq!(config.walkable_climb, 7);
        assert_eq!(config.detail_sample_dist, 0.5);
        assert_eq!(config.max_simplification_error, 1.0);
        assert_eq!(source_config(0, 30, 31).unwrap(), SourceConfig::default());
        assert_eq!(source_config(562, 30, 31).unwrap().walkable_radius, 0);
        assert!(matches!(
            source_config(572, 28, 29),
            Err(GenerationError::Validation(Error::Unsupported))
        ));
    }
    #[test]
    fn clean_keeps_source_sorted_original_indices_not_first_triangle_encounter() {
        let mut vertices = vec![
            [1.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
        ];
        let mut triangles = vec![[3, 1, 3]];
        clean(&mut vertices, &mut triangles, &|| false).unwrap();
        assert_eq!(vertices, vec![[2.0, 0.0, 0.0], [4.0, 0.0, 0.0]]);
        assert_eq!(triangles, [[1, 0, 1]]);
    }
    #[test]
    fn child_response_must_match_requested_map_even_when_skipped() {
        let request = GenerationRequest {
            map_id: 0,
            tile_x: 30,
            tile_y: 31,
            uses_liquids: true,
            allocation_budget_bytes: DEFAULT_ALLOCATION_BUDGET,
            map: MapParams {
                origin: [0.0, f32::MIN_POSITIVE, 0.0],
                tile_width: GRID_SIZE,
                tile_height: GRID_SIZE,
                max_tiles: 1,
                max_polys: 0x80000000,
            },
            bmin: [0.0; 3],
            bmax: [1.0; 3],
            config: SourceConfig::default(),
            solid: TriangleMesh::default(),
            liquid: TriangleMesh::default(),
            liquid_areas: vec![],
        };
        let mut bytes = Vec::new();
        for bits in request.map.origin.map(f32::to_bits).into_iter().chain([
            GRID_SIZE.to_bits(),
            GRID_SIZE.to_bits(),
            1,
            0x80000000,
        ]) {
            bytes.extend(bits.to_le_bytes());
        }
        let mut response = GenerationResponse {
            status: GenerationStatus::Empty,
            mmap: bytes,
            mmtile: vec![],
            polygon_count: 0,
            native_peak_bytes: 0,
        };
        validate_response(&request, &response).unwrap();
        response.native_peak_bytes = request.allocation_budget_bytes + 1;
        assert!(matches!(
            validate_response(&request, &response),
            Err(GenerationError::Protocol)
        ));
        response.native_peak_bytes = 0;
        for offset in [0, 4, 8, 12, 16, 20, 24] {
            let original = response.mmap[offset];
            response.mmap[offset] ^= 1;
            assert!(matches!(
                validate_response(&request, &response),
                Err(GenerationError::Protocol)
            ));
            response.mmap[offset] = original;
        }
        response.status = GenerationStatus::Generated;
        response.mmtile = vec![0; 48];
        response.polygon_count = 1;
        assert!(matches!(
            validate_response(&request, &response),
            Err(GenerationError::Protocol)
        ));
        response.mmtile[16..20].copy_from_slice(&1u32.to_le_bytes());
        response.mmtile[44..48].copy_from_slice(&1u32.to_le_bytes());
        validate_response(&request, &response).unwrap();
        response.polygon_count = 2;
        assert!(matches!(
            validate_response(&request, &response),
            Err(GenerationError::Protocol)
        ));
    }
}
