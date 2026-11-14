// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical generated inputs bound to one actual loaded native allocation.
//! Descriptors are read-only diagnostics, never source freshness or support evidence.
use super::*;
use benilla_native_nav::RetainedSourceQuery;

/// No descriptor, decoded bytes or equal-byte replacement query can mint this receipt.
/// Only the generated loader can capture it after validating every input/output pair.
pub(crate) struct GeneratedQueryReceipt {
    query: RetainedSourceQuery,
    descriptor: GeneratedQueryProvenance,
}

#[derive(Debug, PartialEq)]
pub(crate) struct GeneratedQueryProvenance {
    binding: SnapshotBinding,
    configuration: Hash,
    terrain_source_epoch: String,
    collision_source_binding: Hash,
    tiles: Vec<GeneratedTileProvenance>,
}

#[derive(Debug, PartialEq)]
pub(crate) struct GeneratedTileProvenance {
    map_id: u32,
    source_grid: [u8; 2],
    request_tile: [u32; 2],
    expected_native_tile: [i32; 3],
    input_revision: Hash,
    output_revision: Hash,
    request_sha256: Hash,
    response_sha256: Hash,
    mmap_sha256: Hash,
    mmtile_sha256: Hash,
    bmin_bits: [u32; 3],
    bmax_bits: [u32; 3],
    map: MapParams,
    config: SourceConfig,
    ch_bits: u32,
    status: GenerationStatus,
}

impl GeneratedQueryReceipt {
    /// Visible only to the existing generated-query loading boundary, not consumers.
    pub(super) fn capture(
        pairs: &[(&PreparedNavigationInput<'_, '_, '_>, &GeneratedNavigation)],
        binding: SnapshotBinding,
        configuration: Hash,
        query: &SourceMapQuery,
        poll: &impl Fn() -> Result<(), GenerationError>,
    ) -> Result<Self, GenerationError> {
        let mut tiles = Vec::new();
        tiles
            .try_reserve_exact(pairs.len())
            .map_err(|_| Error::Budget)?;
        for (input, output) in pairs {
            poll()?;
            let request = &input.request;
            let encoded = request.encode().map_err(|_| GenerationError::Protocol)?;
            let request_sha256 = bytes_hash(&encoded, poll)?;
            let response = &output.response;
            // Reconstruct the exact decoded framing without another output allocation.
            let mut response_hash = Sha256::new();
            response_hash.update(b"BNAVOUT1");
            response_hash.update(
                match response.status {
                    GenerationStatus::Generated => 0_u32,
                    GenerationStatus::Empty => 1,
                    GenerationStatus::Exact625 => 2,
                }
                .to_le_bytes(),
            );
            response_hash.update((response.mmap.len() as u32).to_le_bytes());
            response_hash.update((response.mmtile.len() as u32).to_le_bytes());
            response_hash.update(response.polygon_count.to_le_bytes());
            response_hash.update(response.native_peak_bytes.to_le_bytes());
            for bytes in [&response.mmap, &response.mmtile] {
                for chunk in bytes.chunks(65536) {
                    poll()?;
                    response_hash.update(chunk);
                }
            }
            // generator.cpp sets tileX/Y from these binary32 centers and map origin.
            let expected_native_tile = [
                (((request.bmin[0] + request.bmax[0]) / 2.0 - request.map.origin[0]) / GRID_SIZE)
                    as i32,
                (((request.bmin[2] + request.bmax[2]) / 2.0 - request.map.origin[2]) / GRID_SIZE)
                    as i32,
                0,
            ];
            tiles.push(GeneratedTileProvenance {
                map_id: request.map_id,
                source_grid: [input.tile.x, input.tile.y],
                request_tile: [request.tile_x, request.tile_y],
                expected_native_tile,
                input_revision: input.revision,
                output_revision: output.revision,
                request_sha256,
                response_sha256: response_hash.finalize().into(),
                mmap_sha256: bytes_hash(&response.mmap, poll)?,
                mmtile_sha256: bytes_hash(&response.mmtile, poll)?,
                bmin_bits: request.bmin.map(f32::to_bits),
                bmax_bits: request.bmax.map(f32::to_bits),
                map: request.map.clone(),
                config: request.config.clone(),
                // Pinned generator.cpp assigns both config.cs and config.ch this value.
                ch_bits: benilla_native_nav_generator::BASE_UNIT_DIM.to_bits(),
                status: response.status,
            });
        }
        let first = pairs.first().ok_or(Error::Missing)?.0;
        poll()?;
        Ok(Self {
            query: query.retain_default(),
            descriptor: GeneratedQueryProvenance {
                binding,
                configuration,
                terrain_source_epoch: first.terrain.source_epoch().to_owned(),
                collision_source_binding: *first.source.map().source_binding(),
                tiles,
            },
        })
    }

    pub(crate) fn checked(
        &self,
        query: &SourceMapQuery,
        binding: SnapshotBinding,
        configuration: Hash,
    ) -> Result<&GeneratedQueryProvenance, Error> {
        self.query.check_query(query).map_err(|_| Error::Identity)?;
        if binding != self.descriptor.binding || configuration != self.descriptor.configuration {
            return Err(Error::Identity);
        }
        Ok(&self.descriptor)
    }
}

fn bytes_hash(
    bytes: &[u8],
    poll: &impl Fn() -> Result<(), GenerationError>,
) -> Result<Hash, GenerationError> {
    let mut hash = Sha256::new();
    for chunk in bytes.chunks(65536) {
        poll()?;
        hash.update(chunk);
    }
    Ok(hash.finalize().into())
}

impl GeneratedQueryProvenance {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    pub(crate) fn configuration_binding(&self) -> Hash {
        self.configuration
    }
    pub(crate) fn terrain_source_epoch(&self) -> &str {
        &self.terrain_source_epoch
    }
    pub(crate) fn collision_source_binding(&self) -> Hash {
        self.collision_source_binding
    }
    pub(crate) fn tiles(&self) -> &[GeneratedTileProvenance] {
        &self.tiles
    }
}

impl GeneratedTileProvenance {
    pub(crate) fn map_id(&self) -> u32 {
        self.map_id
    }
    /// GridMap coordinates used when loading this tile into SourceMapQuery.
    pub(crate) fn source_grid(&self) -> [u8; 2] {
        self.source_grid
    }
    /// Source generator tileX/tileY (the reverse of GridMap x/y).
    pub(crate) fn request_tile(&self) -> [u32; 2] {
        self.request_tile
    }
    /// Expected Detour header x/y/layer, derived from this request, not native evidence.
    pub(crate) fn expected_native_tile(&self) -> [i32; 3] {
        self.expected_native_tile
    }
    pub(crate) fn input_revision(&self) -> Hash {
        self.input_revision
    }
    pub(crate) fn output_revision(&self) -> Hash {
        self.output_revision
    }
    pub(crate) fn request_sha256(&self) -> Hash {
        self.request_sha256
    }
    pub(crate) fn response_sha256(&self) -> Hash {
        self.response_sha256
    }
    pub(crate) fn mmap_sha256(&self) -> Hash {
        self.mmap_sha256
    }
    pub(crate) fn mmtile_sha256(&self) -> Hash {
        self.mmtile_sha256
    }
    pub(crate) fn bmin_bits(&self) -> [u32; 3] {
        self.bmin_bits
    }
    pub(crate) fn bmax_bits(&self) -> [u32; 3] {
        self.bmax_bits
    }
    pub(crate) fn map_params(&self) -> &MapParams {
        &self.map
    }
    pub(crate) fn source_config(&self) -> &SourceConfig {
        &self.config
    }
    pub(crate) fn ch_bits(&self) -> u32 {
        self.ch_bits
    }
    pub(crate) fn status(&self) -> GenerationStatus {
        self.status
    }
}
