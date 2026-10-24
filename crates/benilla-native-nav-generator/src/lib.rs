// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure bounded wire codec. This library contains no generator FFI or native linkage.
use std::fmt;

pub const SOURCE_CONFIG_JSON: &[u8] = include_bytes!("../source_config/config.json");
pub const SOURCE_OFFMESH: &[u8] = include_bytes!("../source_config/offmesh.txt");

pub const GRID_SIZE: f32 = 533.333_3;
pub const BASE_UNIT_DIM: f32 = 0.266_666_6;
pub const MAX_REQUEST_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_RESPONSE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_VERTICES: usize = 500_000;
pub const MAX_TRIANGLES: usize = 1_000_000;
pub const DEFAULT_ALLOCATION_BUDGET: u64 = 128 * 1024 * 1024;
pub const MAX_ALLOCATION_BUDGET: u64 = 256 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct MapParams {
    pub origin: [f32; 3],
    pub tile_width: f32,
    pub tile_height: f32,
    pub max_tiles: u32,
    pub max_polys: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SourceConfig {
    pub border_size: u32,
    pub walkable_slope_angle: f32,
    pub walkable_height: u32,
    pub walkable_climb: u32,
    pub walkable_radius: u32,
    pub max_edge_len: u32,
    pub max_simplification_error: f32,
    /// Source JSON value before MapBuilder squares it.
    pub min_region_area: u32,
    /// Source JSON value before MapBuilder squares it.
    pub merge_region_area: u32,
    pub detail_sample_dist: f32,
    pub detail_sample_max_error: f32,
    pub liquid_flag_merge_threshold: f32,
}
impl Default for SourceConfig {
    fn default() -> Self {
        Self {
            border_size: 5,
            walkable_slope_angle: 60.0,
            walkable_height: 6,
            walkable_climb: 4,
            walkable_radius: 2,
            max_edge_len: 81,
            max_simplification_error: 1.8,
            min_region_area: 60,
            merge_region_area: 50,
            detail_sample_dist: BASE_UNIT_DIM * 16.0,
            detail_sample_max_error: BASE_UNIT_DIM,
            liquid_flag_merge_threshold: 0.0,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TriangleMesh {
    pub vertices: Vec<[f32; 3]>,
    pub triangles: Vec<[u32; 3]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationRequest {
    pub map_id: u32,
    pub tile_x: u32,
    pub tile_y: u32,
    pub uses_liquids: bool,
    pub allocation_budget_bytes: u64,
    pub map: MapParams,
    pub bmin: [f32; 3],
    pub bmax: [f32; 3],
    pub config: SourceConfig,
    pub solid: TriangleMesh,
    pub liquid: TriangleMesh,
    pub liquid_areas: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProtocolError(pub &'static str);
impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for ProtocolError {}

impl GenerationRequest {
    pub fn encode(&self) -> Result<Vec<u8>, ProtocolError> {
        if self.tile_x > 63 || self.tile_y > 63 || self.map_id > 999 {
            return Err(ProtocolError(
                "map/tile identity outside supported source domain",
            ));
        }
        if self.allocation_budget_bytes == 0 || self.allocation_budget_bytes > MAX_ALLOCATION_BUDGET
        {
            return Err(ProtocolError("native allocation budget outside bounds"));
        }
        if self.liquid_areas.len() != self.liquid.triangles.len()
            || self.liquid_areas.iter().any(|&v| v != 8 && v != 9)
            || (!self.uses_liquids && !self.liquid.triangles.is_empty())
        {
            return Err(ProtocolError("liquid flags do not match liquid triangles"));
        }
        let mut len = 148usize;
        for mesh in [&self.solid, &self.liquid] {
            if mesh.vertices.len() > MAX_VERTICES || mesh.triangles.len() > MAX_TRIANGLES {
                return Err(ProtocolError("mesh count exceeds limit"));
            }
            if mesh
                .vertices
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 100_000.0)
                || mesh
                    .triangles
                    .iter()
                    .flatten()
                    .any(|&i| i as usize >= mesh.vertices.len())
            {
                return Err(ProtocolError("invalid mesh vertex or triangle index"));
            }
            len = len
                .checked_add((mesh.vertices.len() + mesh.triangles.len()) * 12)
                .ok_or(ProtocolError("request size overflow"))?;
        }
        len = len
            .checked_add(self.liquid_areas.len())
            .ok_or(ProtocolError("request size overflow"))?;
        if len > MAX_REQUEST_BYTES {
            return Err(ProtocolError("request exceeds byte limit"));
        }
        let mut out = Vec::with_capacity(len);
        out.extend_from_slice(b"BNAVIN01");
        fn u(out: &mut Vec<u8>, value: u32) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        fn f(out: &mut Vec<u8>, value: f32) {
            out.extend_from_slice(&value.to_le_bytes());
        }
        for value in [
            self.map_id,
            self.tile_x,
            self.tile_y,
            u32::from(self.uses_liquids),
        ] {
            u(&mut out, value);
        }
        out.extend_from_slice(&self.allocation_budget_bytes.to_le_bytes());
        for value in self.map.origin {
            f(&mut out, value);
        }
        f(&mut out, self.map.tile_width);
        f(&mut out, self.map.tile_height);
        u(&mut out, self.map.max_tiles);
        u(&mut out, self.map.max_polys);
        for value in self.bmin.into_iter().chain(self.bmax) {
            f(&mut out, value);
        }
        let c = &self.config;
        u(&mut out, c.border_size);
        f(&mut out, c.walkable_slope_angle);
        for value in [
            c.walkable_height,
            c.walkable_climb,
            c.walkable_radius,
            c.max_edge_len,
        ] {
            u(&mut out, value);
        }
        f(&mut out, c.max_simplification_error);
        u(&mut out, c.min_region_area);
        u(&mut out, c.merge_region_area);
        f(&mut out, c.detail_sample_dist);
        f(&mut out, c.detail_sample_max_error);
        f(&mut out, c.liquid_flag_merge_threshold);
        for mesh in [&self.solid, &self.liquid] {
            u(&mut out, mesh.vertices.len() as u32);
            u(&mut out, mesh.triangles.len() as u32);
            for &value in mesh.vertices.iter().flatten() {
                f(&mut out, value);
            }
            for &value in mesh.triangles.iter().flatten() {
                u(&mut out, value);
            }
        }
        out.extend_from_slice(&self.liquid_areas);
        debug_assert_eq!(out.len(), len);
        Ok(out)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationStatus {
    Generated,
    Empty,
    Exact625,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationResponse {
    pub status: GenerationStatus,
    pub mmap: Vec<u8>,
    pub mmtile: Vec<u8>,
    pub polygon_count: u32,
    /// Recast/Detour payload plus tracking-header peak, not process RSS.
    pub native_peak_bytes: u64,
}
impl GenerationResponse {
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 32 || bytes.len() > MAX_RESPONSE_BYTES || &bytes[..8] != b"BNAVOUT1" {
            return Err(ProtocolError("invalid response framing"));
        }
        let u = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let status = match u(8) {
            0 => GenerationStatus::Generated,
            1 => GenerationStatus::Empty,
            2 => GenerationStatus::Exact625,
            _ => return Err(ProtocolError("unknown generation status")),
        };
        let mmap_len = u(12) as usize;
        let mmtile_len = u(16) as usize;
        let polygon_count = u(20);
        let native_peak_bytes = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
        if mmap_len != 28
            || 32usize
                .checked_add(mmap_len)
                .and_then(|n| n.checked_add(mmtile_len))
                != Some(bytes.len())
            || native_peak_bytes > MAX_ALLOCATION_BUDGET
            || (status == GenerationStatus::Generated
                && (mmtile_len < 120 || polygon_count == 0 || polygon_count == 625))
            || (status != GenerationStatus::Generated && mmtile_len != 0)
            || (status == GenerationStatus::Exact625 && polygon_count != 625)
        {
            return Err(ProtocolError("inconsistent generation response"));
        }
        Ok(Self {
            status,
            mmap: bytes[32..60].to_vec(),
            mmtile: bytes[60..].to_vec(),
            polygon_count,
            native_peak_bytes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn framing_rejects_untrusted_response() {
        for bytes in [vec![], vec![0; 32], b"BNAVOUT1".to_vec()] {
            assert!(GenerationResponse::decode(&bytes).is_err());
        }
    }
    #[test]
    fn source_constants_are_exact_binary32() {
        assert_eq!(GRID_SIZE.to_bits(), 0x4405_5555);
        assert_eq!(BASE_UNIT_DIM.to_bits(), 0.2666666f32.to_bits());
        assert_eq!(
            SourceConfig::default().detail_sample_dist.to_bits(),
            (0.2666666f32 * 16.0).to_bits()
        );
    }
}
