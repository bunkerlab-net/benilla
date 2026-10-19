// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned TileAssembler placement bounds, before model/map BIH construction.
use crate::{DoodadProjectionProfile, PlacementRecords, PreparedDiscovery};
use std::{collections::HashMap, fmt};

pub const ASSEMBLED_BOUNDS_POLICY: &str = "cmangos-vertex-bounds-source-pi-v1";
pub const MAX_BOUND_VERTEX_VISITS: usize = 16_777_216;
#[derive(Debug)]
pub enum SpawnBoundsError {
    Cancelled,
    Allocation,
    Limit,
    Record,
    Model,
    Numeric,
    Preparation(crate::PlacementRecordError),
    Index(crate::MapSpawnIndexError),
    Source(crate::SourceError),
}
impl fmt::Display for SpawnBoundsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "vmap.bounds_cancelled",
            Self::Allocation => "vmap.bounds_allocation",
            Self::Limit => "vmap.bounds_limit",
            Self::Record => "vmap.bounds_record",
            Self::Model => "vmap.bounds_model",
            Self::Numeric => "vmap.bounds_numeric",
            Self::Preparation(_) => "vmap.bounds_preparation",
            Self::Index(_) => "vmap.bounds_index",
            Self::Source(_) => "vmap.bounds_source",
        })
    }
}
impl std::error::Error for SpawnBoundsError {}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AssembledSpawnBound {
    pub generated_id: u32,
    pub flags: u32,
    pub bounds: [[f32; 3]; 2],
}
/// Owns the captured stream and ascending-ID bounds. No BIH or gameplay admission.
pub struct AssembledSpawnBounds {
    records: PlacementRecords,
    bounds: Vec<AssembledSpawnBound>,
    visits: usize,
}
impl fmt::Debug for AssembledSpawnBounds {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssembledSpawnBounds")
            .field("spawns", &self.bounds.len())
            .field("vertex_visits", &self.visits)
            .finish_non_exhaustive()
    }
}
impl AssembledSpawnBounds {
    pub fn records(&self) -> &PlacementRecords {
        &self.records
    }
    pub fn bounds(&self) -> &[AssembledSpawnBound] {
        &self.bounds
    }
    pub fn vertex_visits(&self) -> usize {
        self.visits
    }
    /// Build the source map BIH in ascending unique-ID order. Tile references
    /// still use these input slots, never the resulting object permutation.
    pub fn map_bih(
        &self,
        cancelled: impl Fn() -> bool,
    ) -> Result<benilla_native_vmap::SourceBih, benilla_native_vmap::BihBuildError> {
        if cancelled() {
            return Err(benilla_native_vmap::BihBuildError::Cancelled);
        }
        let mut boxes = Vec::new();
        boxes
            .try_reserve_exact(self.bounds.len())
            .map_err(|_| benilla_native_vmap::BihBuildError::Allocation)?;
        for (i, b) in self.bounds.iter().enumerate() {
            if i % 1024 == 0 && cancelled() {
                return Err(benilla_native_vmap::BihBuildError::Cancelled);
            }
            boxes.push(b.bounds);
        }
        benilla_native_vmap::SourceBih::build(&boxes, 3, cancelled)
    }
    pub fn policy_id(&self) -> &'static str {
        ASSEMBLED_BOUNDS_POLICY
    }
}
impl PreparedDiscovery {
    pub fn assembled_spawn_bounds(
        &self,
        profile: DoodadProjectionProfile,
        cancelled: impl Fn() -> bool,
    ) -> Result<AssembledSpawnBounds, SpawnBoundsError> {
        let records = self
            .placement_records(profile, &cancelled)
            .map_err(SpawnBoundsError::Preparation)?;
        let index = records
            .map_spawn_index(&cancelled)
            .map_err(SpawnBoundsError::Index)?;
        let mut products = HashMap::new();
        products
            .try_reserve(self.products.len())
            .map_err(|_| SpawnBoundsError::Allocation)?;
        for product in &self.products {
            check(&cancelled)?;
            products.insert(product.extracted_name.as_bytes(), product);
        }
        let mut bounds = Vec::new();
        bounds
            .try_reserve_exact(index.unique_count())
            .map_err(|_| SpawnBoundsError::Allocation)?;
        let mut visits = 0usize;
        for (id, record) in index.unique_records() {
            check(&cancelled)?;
            let mut flags = word(record, 12)?;
            let position = vector(record, 22)?;
            let rotation = vector(record, 34)?;
            let scale = float(record, 46)?;
            let bound = if flags & 1 != 0 {
                let name_at = if flags & 4 != 0 { 74 } else { 50 };
                let count = word(record, name_at)? as usize;
                let name = record
                    .get(name_at + 4..name_at + 4 + count)
                    .ok_or(SpawnBoundsError::Record)?;
                // Original ModelSpawn retains a trailing null from shortened MDX names;
                // fopen's c_str consumes the prefix, while source bytes remain unchanged.
                let name = name.strip_suffix(&[0]).unwrap_or(name);
                let product = products.get(name).ok_or(SpawnBoundsError::Model)?;
                let crate::Collision::M2(raw) = &product.collision else {
                    return Err(SpawnBoundsError::Model);
                };
                flags |= 4;
                m2_bounds(raw, position, rotation, scale, &mut visits, &cancelled)?
            } else {
                if flags & 4 == 0 {
                    return Err(SpawnBoundsError::Record);
                };
                supplied_wmo_bounds(record, flags)?
            };
            bounds.push(AssembledSpawnBound {
                generated_id: id,
                flags,
                bounds: bound,
            });
        }
        drop(index);
        self.source
            .verify_unchanged(&cancelled)
            .map_err(SpawnBoundsError::Source)?;
        check(&cancelled)?;
        Ok(AssembledSpawnBounds {
            records,
            bounds,
            visits,
        })
    }
}
fn supplied_wmo_bounds(record: &[u8], flags: u32) -> Result<[[f32; 3]; 2], SpawnBoundsError> {
    let mut b = [vector(record, 50)?, vector(record, 62)?];
    if flags & 2 != 0 {
        let offset = f32::from_bits(0x4685_5555);
        for v in &mut b {
            v[0] += offset;
            v[1] += offset;
            v[2] += 0.0;
        }
    }
    if b.iter().flatten().any(|v| !v.is_finite()) {
        return Err(SpawnBoundsError::Numeric);
    };
    Ok(b)
}
fn check(cancelled: &dyn Fn() -> bool) -> Result<(), SpawnBoundsError> {
    if cancelled() {
        Err(SpawnBoundsError::Cancelled)
    } else {
        Ok(())
    }
}
fn word(b: &[u8], i: usize) -> Result<u32, SpawnBoundsError> {
    Ok(u32::from_le_bytes(
        b.get(i..i.checked_add(4).ok_or(SpawnBoundsError::Record)?)
            .ok_or(SpawnBoundsError::Record)?
            .try_into()
            .map_err(|_| SpawnBoundsError::Record)?,
    ))
}
fn float(b: &[u8], i: usize) -> Result<f32, SpawnBoundsError> {
    let v = f32::from_bits(word(b, i)?);
    if v.is_finite() {
        Ok(v)
    } else {
        Err(SpawnBoundsError::Numeric)
    }
}
fn vector(b: &[u8], i: usize) -> Result<[f32; 3], SpawnBoundsError> {
    Ok([float(b, i)?, float(b, i + 4)?, float(b, i + 8)?])
}
type Matrix = [[f32; 3]; 3];
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] * b[0] + a[1] * b[1]) + a[2] * b[2]
}
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|r| std::array::from_fn(|c| dot(a[r], [b[0][c], b[1][c], b[2][c]])))
}
fn rotation(angles: [f32; 3]) -> Result<Matrix, SpawnBoundsError> {
    let trig = |angle: f32| -> Result<[f32; 2], SpawnBoundsError> {
        if !angle.is_finite() || !(-360.0..=360.0).contains(&angle) {
            return Err(SpawnBoundsError::Numeric);
        };
        let radians = (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(angle) / 180.0) as f32;
        benilla_sim::source_trig::sin_cos_radians_bits_native(radians.to_bits())
            .map(|v| v.map(f32::from_bits))
            .map_err(|_| SpawnBoundsError::Numeric)
    };
    let [s, c] = trig(angles[1])?;
    let z = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
    let [s, c] = trig(angles[0])?;
    let y = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
    let [s, c] = trig(angles[2])?;
    let x = [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]];
    Ok(multiply(z, multiply(y, x)))
}
fn merge_bound(bounds: &mut [[f32; 3]; 2], value: [f32; 3]) {
    for axis in 0..3 {
        // G3D AABox uses std::min/max(new, old), so ties select new.
        bounds[0][axis] = if bounds[0][axis] < value[axis] {
            bounds[0][axis]
        } else {
            value[axis]
        };
        bounds[1][axis] = if value[axis] < bounds[1][axis] {
            bounds[1][axis]
        } else {
            value[axis]
        };
    }
}
// Only privately captured extractor-produced single-group M2 raw products enter
// this path. Nonempty validated indices require at least one in-range vertex;
// the original assembler's arbitrary multi-group/empty-model cases are excluded.
fn m2_bounds(
    raw: &[u8],
    position: [f32; 3],
    angles: [f32; 3],
    scale: f32,
    visits: &mut usize,
    cancelled: &dyn Fn() -> bool,
) -> Result<[[f32; 3]; 2], SpawnBoundsError> {
    if raw.get(..8) != Some(b"VMAPs05\0") || word(raw, 12)? != 1 || raw.get(72..76) != Some(b"INDX")
    {
        return Err(SpawnBoundsError::Model);
    };
    let indices = word(raw, 80)? as usize;
    let vertex_at = 84usize
        .checked_add(indices.checked_mul(2).ok_or(SpawnBoundsError::Model)?)
        .ok_or(SpawnBoundsError::Model)?;
    if raw.get(vertex_at..vertex_at + 4) != Some(b"VERT") {
        return Err(SpawnBoundsError::Model);
    };
    let count = word(raw, vertex_at + 8)? as usize;
    if count == 0 || count > benilla_native_vmap::MAX_BOUNDING_VERTICES as usize {
        return Err(SpawnBoundsError::Model);
    };
    *visits = visits
        .checked_add(count)
        .filter(|v| *v <= MAX_BOUND_VERTEX_VISITS)
        .ok_or(SpawnBoundsError::Limit)?;
    let vertices = raw
        .get(vertex_at + 12..)
        .filter(|v| v.len() == count * 12)
        .ok_or(SpawnBoundsError::Model)?;
    if !scale.is_finite() || position.iter().any(|v| !v.is_finite()) {
        return Err(SpawnBoundsError::Numeric);
    };
    let matrix = rotation(angles)?;
    let mut bounds = [[0.0; 3]; 2];
    for (i, vertex) in vertices.as_chunks::<12>().0.iter().enumerate() {
        if i % 256 == 0 {
            check(cancelled)?;
        }
        let scaled = vector(vertex, 0)?.map(|v| v * scale);
        if scaled.iter().any(|v| !v.is_finite()) {
            return Err(SpawnBoundsError::Numeric);
        }
        let mut value = [0.0; 3];
        for (axis, row) in matrix.into_iter().enumerate() {
            let products = std::array::from_fn::<_, 3, _>(|i| row[i] * scaled[i]);
            let partial = products[0] + products[1];
            let total = partial + products[2];
            if products.iter().any(|v| !v.is_finite()) || !partial.is_finite() || !total.is_finite()
            {
                return Err(SpawnBoundsError::Numeric);
            }
            value[axis] = total;
        }
        if value.iter().any(|v| !v.is_finite()) {
            return Err(SpawnBoundsError::Numeric);
        };
        if i == 0 {
            bounds = [value, value];
        } else {
            merge_bound(&mut bounds, value);
        }
    }
    for b in &mut bounds {
        for axis in 0..3 {
            b[axis] += position[axis];
            if !b[axis].is_finite() {
                return Err(SpawnBoundsError::Numeric);
            }
        }
    }
    Ok(bounds)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(name: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source")
                .join(name),
        )
        .unwrap()
    }
    #[test]
    fn transformed_vertex_bounds_match_original_assembler_bit_patterns() {
        let tile = fixture("products/000_32_32.vmtile");
        for (start, name) in [
            (12, "authored-m2.raw.vmap"),
            (93, "signed-zero-m2.raw.vmap"),
            (177, "authored-m2.raw.vmap"),
        ] {
            let raw = fixture(name);
            let mut visits = 0;
            let actual = m2_bounds(
                &raw,
                vector(&tile, start + 10).unwrap(),
                vector(&tile, start + 22).unwrap(),
                float(&tile, start + 34).unwrap(),
                &mut visits,
                &|| false,
            )
            .unwrap();
            let expected = [
                vector(&tile, start + 38).unwrap(),
                vector(&tile, start + 50).unwrap(),
            ];
            assert_eq!(
                actual.map(|v| v.map(f32::to_bits)),
                expected.map(|v| v.map(f32::to_bits)),
                "record={start}"
            );
            assert_eq!(visits, 25);
        }
    }
    #[test]
    fn vertex_budget_cancel_and_numeric_envelope_fail_explicitly() {
        let raw = fixture("authored-m2.raw.vmap");
        assert!(matches!(
            m2_bounds(
                &raw,
                [0.0; 3],
                [0.0; 3],
                1.0,
                &mut { MAX_BOUND_VERTEX_VISITS },
                &|| false
            ),
            Err(SpawnBoundsError::Limit)
        ));
        assert!(matches!(
            m2_bounds(&raw, [0.0; 3], [0.0; 3], 1.0, &mut 0, &|| true),
            Err(SpawnBoundsError::Cancelled)
        ));
        for (position, angles, scale) in [
            ([f32::MAX; 3], [0.0; 3], f32::MAX),
            ([0.0; 3], [361.0, 0.0, 0.0], 1.0),
            ([0.0; 3], [0.0; 3], f32::NAN),
        ] {
            assert!(matches!(
                m2_bounds(&raw, position, angles, scale, &mut 0, &|| false),
                Err(SpawnBoundsError::Numeric)
            ));
        }
        for scale in [0.0, -0.0, -1.0] {
            let b = m2_bounds(&raw, [0.0; 3], [0.0; 3], scale, &mut 0, &|| false).unwrap();
            assert!(b.iter().flatten().all(|v| v.is_finite()));
        }
    }
    #[test]
    fn ordinary_wmo_preserves_original_bounds_and_global_shift_preserves_source_zero_rule() {
        let tile=std::fs::read(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/products/000_32_32.vmtile")).unwrap();
        for start in [12, 94] {
            let mut record = vec![0; 12];
            record.extend_from_slice(&tile[start..start + 62]);
            let expected = [
                vector(&tile, start + 38).unwrap(),
                vector(&tile, start + 50).unwrap(),
            ];
            assert_eq!(
                supplied_wmo_bounds(&record, 4)
                    .unwrap()
                    .map(|v| v.map(f32::to_bits)),
                expected.map(|v| v.map(f32::to_bits))
            );
        }
        let mut record = vec![0; 74];
        for at in [50, 54, 58, 62, 66, 70] {
            record[at..at + 4].copy_from_slice(&(-0.0f32).to_le_bytes());
        }
        let shifted = supplied_wmo_bounds(&record, 6).unwrap();
        assert_eq!(
            shifted.map(|v| v.map(f32::to_bits)),
            [[0x46855555, 0x46855555, 0]; 2]
        );
    }
    #[test]
    fn newest_equal_vertex_controls_signed_zero_before_position_addition() {
        for (old, new) in [(0.0f32, -0.0f32), (-0.0, 0.0)] {
            let mut bounds = [[old; 3]; 2];
            merge_bound(&mut bounds, [new; 3]);
            for endpoint in &mut bounds {
                for component in endpoint {
                    *component += -0.0;
                }
            }
            assert_eq!(bounds.map(|v| v.map(f32::to_bits)), [[new.to_bits(); 3]; 2]);
        }
    }
    #[test]
    fn scale_dot_sum_and_translation_overflows_are_separate_rejections() {
        let original = fixture("authored-m2.raw.vmap");
        for (vertex, angle, scale, position) in [
            ([3.0e38f32, 0.0, 0.0], [0.0; 3], 2.0, [0.0; 3]),
            ([3.0e38, 3.0e38, 0.0], [0.0, 45.0, 0.0], 1.0, [0.0; 3]),
            ([3.0e38, 0.0, 0.0], [0.0; 3], 1.0, [3.0e38, 0.0, 0.0]),
        ] {
            let mut raw = original.clone();
            for chunk in raw[288..].as_chunks_mut::<12>().0.iter_mut() {
                for (axis, value) in vertex.into_iter().enumerate() {
                    chunk[axis * 4..axis * 4 + 4].copy_from_slice(&value.to_le_bytes());
                }
            }
            assert!(matches!(
                m2_bounds(&raw, position, angle, scale, &mut 0, &|| false),
                Err(SpawnBoundsError::Numeric)
            ));
        }
    }
}
