// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure bounded conversion of captured VMAPs05 bytes to original VMO framing.
use crate::{BihBounds, BihBuildError, SourceBih};
use std::cell::Cell;
const MAX_BYTES: usize = 128 * 1024 * 1024;
const MAX_GROUPS: usize = 4096;
const MAX_ELEMENTS: usize = 1_048_576;
const MAX_POLLS: usize = 65_536;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelWriteError {
    Invalid,
    Limit,
    Allocation,
    Cancelled,
    Bih(BihBuildError),
}
impl std::fmt::Display for ModelWriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "vmap.model_write.{self:?}")
    }
}
impl std::error::Error for ModelWriteError {}
/// Convert one complete captured raw model. No filesystem writes or admission.
pub fn assemble_raw_model(
    raw: &[u8],
    cancelled: impl Fn() -> bool,
) -> Result<Vec<u8>, ModelWriteError> {
    with_poll_limit(raw, cancelled, MAX_POLLS)
}
fn with_poll_limit(
    raw: &[u8],
    cancelled: impl Fn() -> bool,
    max_polls: usize,
) -> Result<Vec<u8>, ModelWriteError> {
    if raw.len() > MAX_BYTES {
        return Err(ModelWriteError::Limit);
    }
    let polls = Cell::new(0usize);
    let stopped = || {
        polls.set(polls.get().saturating_add(1));
        polls.get() > max_polls || cancelled()
    };
    let result = assemble(raw, &stopped);
    if polls.get() > max_polls {
        Err(ModelWriteError::Limit)
    } else {
        result
    }
}
fn check(cancelled: &impl Fn() -> bool) -> Result<(), ModelWriteError> {
    if cancelled() {
        Err(ModelWriteError::Cancelled)
    } else {
        Ok(())
    }
}
fn assemble(raw: &[u8], cancelled: &impl Fn() -> bool) -> Result<Vec<u8>, ModelWriteError> {
    check(cancelled)?;
    let mut r = Reader { bytes: raw, pos: 0 };
    if r.take(8)? != b"VMAPs05\0" {
        return Err(ModelWriteError::Invalid);
    }
    let _ignored_vectors = r.word()?;
    let groups = r.count(MAX_GROUPS)?;
    let root = r.word()?;
    let mut out = Output(Vec::new());
    out.add(b"VMAP_7.0WMOD")?;
    out.word(8)?;
    out.word(root)?;
    let mut group_bounds = Vec::new();
    group_bounds
        .try_reserve_exact(groups)
        .map_err(|_| ModelWriteError::Allocation)?;
    if groups != 0 {
        out.add(b"GMOD")?;
        out.word(groups as u32)?;
    }
    let mut elements = 0usize;
    for _ in 0..groups {
        check(cancelled)?;
        let flags = r.word()?;
        let id = r.word()?;
        let bounds_bytes = r.take(24)?;
        let bounds = bbox(bounds_bytes)?;
        group_bounds.push(bounds);
        let liquid_flags = r.word()?;
        if liquid_flags & !1 != 0 {
            return Err(ModelWriteError::Invalid);
        }
        let batches = r.array(b"GRP ", 4, MAX_ELEMENTS)?; // source ignores branch data
        charge(&mut elements, batches.len() / 4)?;
        let indices = r.array(b"INDX", 2, MAX_ELEMENTS * 3)?;
        if indices.len() % 6 != 0 {
            return Err(ModelWriteError::Invalid);
        }
        let vertices = r.array(b"VERT", 12, MAX_ELEMENTS)?;
        charge(&mut elements, indices.len() / 6)?;
        charge(&mut elements, vertices.len() / 12)?;
        for (i, v) in vertices.as_chunks::<12>().0.iter().enumerate() {
            if i % 1024 == 0 {
                check(cancelled)?;
            }
            vector(v)?;
        }
        let liquid = if liquid_flags != 0 {
            Some(liquid(&mut r, &mut elements, cancelled)?)
        } else {
            None
        };
        out.add(bounds_bytes)?;
        out.word(flags)?;
        out.word(id)?;
        out.add(b"VERT")?;
        out.word((4 + vertices.len()) as u32)?;
        out.word((vertices.len() / 12) as u32)?;
        // The source writer ends empty groups here, including their liquid.
        if vertices.is_empty() {
            if !indices.is_empty() {
                return Err(ModelWriteError::Invalid);
            }
            continue;
        }
        out.add(vertices)?;
        let triangles = indices.len() / 6;
        out.add(b"TRIM")?;
        out.word((4 + triangles * 12) as u32)?;
        out.word(triangles as u32)?;
        let mut boxes = Vec::new();
        boxes
            .try_reserve_exact(triangles)
            .map_err(|_| ModelWriteError::Allocation)?;
        for (i, triangle) in indices.as_chunks::<6>().0.iter().enumerate() {
            if i % 1024 == 0 {
                check(cancelled)?;
            }
            let mut b = [[0.0; 3]; 2];
            for n in 0..3 {
                let idx =
                    u16::from_le_bytes(triangle[n * 2..n * 2 + 2].try_into().unwrap()) as usize;
                let v = vector(
                    vertices
                        .get(idx * 12..idx * 12 + 12)
                        .ok_or(ModelWriteError::Invalid)?,
                )?;
                out.word(idx as u32)?;
                if n == 0 {
                    b = [v, v];
                } else {
                    for (a, value) in v.into_iter().enumerate() {
                        b[0][a] = if b[0][a] < value { b[0][a] } else { value };
                        b[1][a] = if b[1][a] > value { b[1][a] } else { value };
                    }
                }
            }
            boxes.push(b);
        }
        out.add(b"MBIH")?;
        tree(&mut out, &boxes, 3, cancelled)?;
        out.add(b"LIQU")?;
        if let Some(liquid) = liquid {
            // GetFileSize omits the serialized u32 type, a source format quirk.
            out.word((20 + liquid.heights.len() + liquid.tiles.len()) as u32)?;
            out.add(&liquid.header[8..28])?;
            let kind = i16::from_le_bytes(liquid.header[28..30].try_into().unwrap()) as i32 as u32;
            out.word(kind)?;
            out.add(liquid.heights)?;
            out.add(liquid.tiles)?;
        } else {
            out.word(0)?;
        }
    }
    if r.pos != raw.len() {
        return Err(ModelWriteError::Invalid);
    }
    if groups != 0 {
        out.add(b"GBIH")?;
        tree(&mut out, &group_bounds, 1, cancelled)?;
    }
    check(cancelled)?;
    Ok(out.0)
}
fn tree(
    out: &mut Output,
    boxes: &[BihBounds],
    leaf: u32,
    cancelled: &impl Fn() -> bool,
) -> Result<(), ModelWriteError> {
    let tree = SourceBih::build(boxes, leaf, cancelled).map_err(map_bih)?;
    let bytes = tree.serialize(cancelled).map_err(map_bih)?;
    out.add(&bytes)
}
fn map_bih(e: BihBuildError) -> ModelWriteError {
    if e == BihBuildError::Cancelled {
        ModelWriteError::Cancelled
    } else {
        ModelWriteError::Bih(e)
    }
}
fn charge(total: &mut usize, n: usize) -> Result<(), ModelWriteError> {
    *total = total.checked_add(n).ok_or(ModelWriteError::Limit)?;
    if *total > MAX_ELEMENTS {
        Err(ModelWriteError::Limit)
    } else {
        Ok(())
    }
}
struct Liquid<'a> {
    header: &'a [u8],
    heights: &'a [u8],
    tiles: &'a [u8],
}
fn liquid<'a>(
    r: &mut Reader<'a>,
    elements: &mut usize,
    cancelled: &impl Fn() -> bool,
) -> Result<Liquid<'a>, ModelWriteError> {
    if r.take(4)? != b"LIQU" {
        return Err(ModelWriteError::Invalid);
    }
    let advertised = r.word()? as usize;
    let header = r.take(32)?;
    let d: [usize; 4] = std::array::from_fn(|i| {
        u32::from_le_bytes(header[i * 4..i * 4 + 4].try_into().unwrap()) as usize
    });
    if d.iter().any(|&v| v == 0 || v > MAX_ELEMENTS) || d[0] != d[2] + 1 || d[1] != d[3] + 1 {
        return Err(ModelWriteError::Invalid);
    }
    let height_count = d[0].checked_mul(d[1]).ok_or(ModelWriteError::Limit)?;
    let tile_count = d[2].checked_mul(d[3]).ok_or(ModelWriteError::Limit)?;
    charge(elements, height_count)?;
    charge(elements, tile_count)?;
    if advertised != 32 + height_count * 8 + tile_count {
        return Err(ModelWriteError::Invalid);
    }
    vector(&header[16..28])?;
    let heights = r.take(height_count * 4)?;
    let tiles = r.take(tile_count)?;
    for (i, h) in heights.as_chunks::<4>().0.iter().enumerate() {
        if i % 1024 == 0 {
            check(cancelled)?;
        }
        if !f32::from_le_bytes(*h).is_finite() {
            return Err(ModelWriteError::Invalid);
        }
    }
    Ok(Liquid {
        header,
        heights,
        tiles,
    })
}
fn vector(bytes: &[u8]) -> Result<[f32; 3], ModelWriteError> {
    let v =
        std::array::from_fn(|i| f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()));
    if v.iter().all(|x| x.is_finite()) {
        Ok(v)
    } else {
        Err(ModelWriteError::Invalid)
    }
}
fn bbox(bytes: &[u8]) -> Result<BihBounds, ModelWriteError> {
    let b = [vector(&bytes[..12])?, vector(&bytes[12..])?];
    if (0..3).any(|i| b[0][i] > b[1][i]) {
        Err(ModelWriteError::Invalid)
    } else {
        Ok(b)
    }
}
struct Reader<'a> {
    bytes: &'a [u8],
    pos: usize,
}
impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], ModelWriteError> {
        let end = self.pos.checked_add(n).ok_or(ModelWriteError::Limit)?;
        let b = self
            .bytes
            .get(self.pos..end)
            .ok_or(ModelWriteError::Invalid)?;
        self.pos = end;
        Ok(b)
    }
    fn word(&mut self) -> Result<u32, ModelWriteError> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn count(&mut self, max: usize) -> Result<usize, ModelWriteError> {
        let n = self.word()? as usize;
        if n > max {
            Err(ModelWriteError::Limit)
        } else {
            Ok(n)
        }
    }
    fn array(
        &mut self,
        tag: &[u8; 4],
        width: usize,
        max: usize,
    ) -> Result<&'a [u8], ModelWriteError> {
        if self.take(4)? != tag {
            return Err(ModelWriteError::Invalid);
        }
        let size = self.word()? as usize;
        let n = self.count(max)?;
        if size != 4 + n * width {
            return Err(ModelWriteError::Invalid);
        }
        self.take(n * width)
    }
}
struct Output(Vec<u8>);
impl Output {
    fn word(&mut self, w: u32) -> Result<(), ModelWriteError> {
        self.add(&w.to_le_bytes())
    }
    fn add(&mut self, b: &[u8]) -> Result<(), ModelWriteError> {
        let end = self
            .0
            .len()
            .checked_add(b.len())
            .filter(|n| *n <= MAX_BYTES)
            .ok_or(ModelWriteError::Limit)?;
        if end > self.0.capacity() {
            let target = end.max(self.0.capacity().saturating_mul(2)).min(MAX_BYTES);
            self.0
                .try_reserve_exact(target - self.0.len())
                .map_err(|_| ModelWriteError::Allocation)?;
            if self.0.capacity() > MAX_BYTES {
                return Err(ModelWriteError::Allocation);
            }
        }
        self.0.extend_from_slice(b);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixtures() -> std::path::PathBuf {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../benilla-native-geometry-tests/tools/fixtures")
    }
    #[test]
    fn complete_original_model_products_match() {
        let base = fixtures();
        let pairs = [
            (
                "m2_vmap_source/authored-m2.raw.vmap",
                "m2_vmap_source/products/authored.m2.vmo",
            ),
            (
                "m2_vmap_source/signed-zero-m2.raw.vmap",
                "m2_vmap_source/products/signed-zero.m2.vmo",
            ),
            (
                "wmo_vmap_source/authored-wmo.raw.vmap",
                "wmo_vmap_source/products/Authored.wmo.vmo",
            ),
        ];
        for (raw, product) in pairs {
            let input = std::fs::read(base.join(raw)).unwrap();
            assert_eq!(
                assemble_raw_model(&input, || false).unwrap(),
                std::fs::read(base.join(product)).unwrap(),
                "{raw}"
            );
        }
        for entry in std::fs::read_dir(base.join("wmo_liquid_source/raw")).unwrap() {
            let entry = entry.unwrap();
            let name = entry
                .file_name()
                .to_str()
                .unwrap()
                .replace(".raw.vmap", ".wmo.vmo");
            let input = std::fs::read(entry.path()).unwrap();
            assert_eq!(
                assemble_raw_model(&input, || false).unwrap(),
                std::fs::read(base.join("wmo_liquid_source/products").join(&name)).unwrap(),
                "{name}"
            );
        }
    }
    #[test]
    fn original_raw_loader_edge_fixtures_match() {
        fn unhex(s: &str) -> Vec<u8> {
            s.as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u8::from_str_radix(std::str::from_utf8(b).unwrap(), 16).unwrap())
                .collect()
        }
        let mut count = 0;
        for line in include_str!("../tools/fixtures/source-model.tsv")
            .lines()
            .filter(|s| !s.starts_with('#'))
        {
            let fields: Vec<_> = line.split('\t').collect();
            assert_eq!(
                assemble_raw_model(&unhex(fields[1]), || false).unwrap(),
                unhex(fields[2]),
                "{}",
                fields[0]
            );
            count += 1;
        }
        assert_eq!(count, 7);
    }
    #[test]
    fn cancellation_is_checked_through_serialization() {
        let raw = std::fs::read(fixtures().join("wmo_liquid_source/raw/Water.raw.vmap")).unwrap();
        let polls = Cell::new(0);
        assemble_raw_model(&raw, || {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        for cutoff in 0..polls.get() {
            let current = Cell::new(0);
            assert_eq!(
                assemble_raw_model(&raw, || {
                    let n = current.get();
                    current.set(n + 1);
                    n >= cutoff
                })
                .unwrap_err(),
                ModelWriteError::Cancelled
            );
        }
    }
    #[test]
    fn malformed_models_fail_without_partial_output() {
        let raw = std::fs::read(fixtures().join("m2_vmap_source/authored-m2.raw.vmap")).unwrap();
        for n in 0..raw.len() {
            assert!(assemble_raw_model(&raw[..n], || false).is_err());
        }
        let mut trailing = raw.clone();
        trailing.push(0);
        assert!(assemble_raw_model(&trailing, || false).is_err());
        let mut empty = b"VMAPs05\0".to_vec();
        empty.extend_from_slice(&[0; 12]);
        assert_eq!(
            assemble_raw_model(&empty, || false).unwrap(),
            b"VMAP_7.0WMOD\x08\0\0\0\0\0\0\0"
        );
    }
}

#[cfg(test)]
mod safety_tests {
    use super::*;
    #[test]
    fn caps_are_checked_and_shared_across_builder_serialization() {
        let raw=include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_vmap_source/authored-wmo.raw.vmap");
        let polls = Cell::new(0usize);
        assemble_raw_model(raw, || {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        assert_eq!(
            with_poll_limit(raw, || false, polls.get() - 1).unwrap_err(),
            ModelWriteError::Limit
        );
        assert_eq!(
            with_poll_limit(raw, || false, polls.get()).unwrap(),
            assemble_raw_model(raw, || false).unwrap()
        );
        let mut total = MAX_ELEMENTS;
        assert_eq!(charge(&mut total, 1), Err(ModelWriteError::Limit));
        total = usize::MAX;
        assert_eq!(charge(&mut total, 1), Err(ModelWriteError::Limit));
        let mut r = Reader {
            bytes: &u32::MAX.to_le_bytes(),
            pos: 0,
        };
        assert_eq!(r.count(MAX_GROUPS), Err(ModelWriteError::Limit));
    }
    #[test]
    fn invalid_indices_bounds_flags_and_chunk_sizes_fail() {
        let raw=include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/m2_vmap_source/authored-m2.raw.vmap");
        let index = raw.windows(4).position(|v| v == b"INDX").unwrap();
        let mut changed = raw.to_vec();
        changed[index + 12..index + 14].copy_from_slice(&u16::MAX.to_le_bytes());
        assert_eq!(
            assemble_raw_model(&changed, || false),
            Err(ModelWriteError::Invalid)
        );
        for (offset, value) in [(28, f32::NAN.to_bits()), (28, 1f32.to_bits()), (52, 2)] {
            let mut changed = raw.to_vec();
            changed[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            assert_eq!(
                assemble_raw_model(&changed, || false),
                Err(ModelWriteError::Invalid)
            );
        }
        for tag in [b"GRP ", b"INDX", b"VERT"] {
            let offset = raw.windows(4).position(|v| v == tag).unwrap() + 4;
            let mut changed = raw.to_vec();
            changed[offset..offset + 4].copy_from_slice(&0u32.to_le_bytes());
            assert_eq!(
                assemble_raw_model(&changed, || false),
                Err(ModelWriteError::Invalid)
            );
        }
        let mut changed = raw.to_vec();
        let count = u32::from_le_bytes(raw[index + 8..index + 12].try_into().unwrap());
        changed[index + 8..index + 12].copy_from_slice(&(count - 1).to_le_bytes());
        changed[index + 4..index + 8].copy_from_slice(&(4 + (count - 1) * 2).to_le_bytes());
        changed.drain(index + 12 + (count as usize - 1) * 2..index + 12 + count as usize * 2);
        assert_eq!(
            assemble_raw_model(&changed, || false),
            Err(ModelWriteError::Invalid)
        );
    }
    #[test]
    fn ignored_liquid_padding_does_not_change_model() {
        let raw=include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_liquid_source/raw/Water.raw.vmap");
        let at = raw.windows(4).position(|v| v == b"LIQU").unwrap();
        let mut changed = raw.to_vec();
        changed[at + 38..at + 40].copy_from_slice(&[255, 127]);
        assert_eq!(
            assemble_raw_model(&changed, || false).unwrap(),
            assemble_raw_model(raw, || false).unwrap()
        );
    }
    #[test]
    fn malformed_liquid_dimensions_size_and_values_reject() {
        let raw=include_bytes!("../../benilla-native-geometry-tests/tools/fixtures/wmo_liquid_source/raw/Water.raw.vmap");
        let at = raw.windows(4).position(|v| v == b"LIQU").unwrap();
        for (offset, bits) in [
            (at + 4, 0),
            (at + 8, 0),
            (at + 8, 4),
            (at + 16, u32::MAX),
            (at + 24, f32::NAN.to_bits()),
            (at + 40, f32::INFINITY.to_bits()),
        ] {
            let mut changed = raw.to_vec();
            changed[offset..offset + 4].copy_from_slice(&bits.to_le_bytes());
            assert!(assemble_raw_model(&changed, || false).is_err(), "{offset}");
        }
    }
}
