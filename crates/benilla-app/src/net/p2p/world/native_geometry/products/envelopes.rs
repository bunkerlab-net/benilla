// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned native serialization framing only. No BIH traversal, native allocation,
//! Detour polygon/reference validation or VMap collision semantics are performed.
use super::{u32_at, Error, ProductId};
use std::collections::BTreeSet;

const MAX_ELEMENTS: usize = 262_144;

pub(super) fn validate(id: &ProductId, bytes: &[u8]) -> Result<BTreeSet<ProductId>, Error> {
    validate_profile(id, bytes, false, false)
}

pub(super) fn validate_with_vmap_liquid(
    id: &ProductId,
    bytes: &[u8],
) -> Result<BTreeSet<ProductId>, Error> {
    validate_profile(id, bytes, true, false)
}

pub(super) fn validate_source_names(
    id: &ProductId,
    bytes: &[u8],
    allow_liquid: bool,
) -> Result<BTreeSet<ProductId>, Error> {
    validate_profile(id, bytes, allow_liquid, true)
}

fn validate_profile(
    id: &ProductId,
    bytes: &[u8],
    allow_vmap_liquid: bool,
    source_names: bool,
) -> Result<BTreeSet<ProductId>, Error> {
    let mut models = BTreeSet::new();
    let mut c = Cursor { bytes, at: 0 };
    match id {
        ProductId::Terrain(_) => return Err(Error::Identity),
        ProductId::MmapParams => {
            if bytes.len() != 28 {
                return Err(Error::Framing);
            }
            c.floats(3)?;
            if c.float()? <= 0.0 || c.float()? <= 0.0 {
                return Err(Error::Format);
            }
            let tiles = c.count()?;
            // MoveMapGen uses 1 << DT_POLY_BITS (31), stored as the raw int bits.
            // This is not a positive signed allocation count in the 64-bit-ref profile.
            let polys = c.word()?;
            if tiles == 0 || tiles > 4096 || polys != 0x8000_0000 {
                return Err(Error::Format);
            }
        }
        ProductId::MmapTile(_) => mmap_tile(&mut c)?,
        ProductId::VmapTree => {
            c.magic(b"VMAP_7.0")?;
            if c.take(1)? != [1] {
                return Err(Error::Unsupported);
            } // tiled exterior only
            c.magic(b"NODE")?;
            c.bih()?;
            c.magic(b"GOBJ")?; // tiled maps contain no global model spawns
        }
        ProductId::VmapTile(_) => {
            c.magic(b"VMAP_7.0")?;
            let count = c.count()?;
            if count > 4096 {
                return Err(Error::Budget);
            }
            for _ in 0..count {
                let flags = c.word()?;
                // MOD_M2, MOD_HAS_BOUND. Worldspawns are outside the tiled exterior profile.
                if flags & !5 != 0 {
                    return Err(Error::Unsupported);
                }
                c.take(2)?; // adtId
                c.word()?; // ID
                c.floats(6)?; // position and rotation
                if c.float()? <= 0.0 {
                    return Err(Error::Format);
                }
                if flags & 4 != 0 {
                    c.bounds()?;
                }
                let length = c.count()?;
                let model = ProductId::from_vmap_source_name(c.take(length)?, source_names)?;
                models.insert(model);
                c.word()?; // referenced BIH value; validity requires the future loaded tree
            }
        }
        ProductId::VmapModel(_) | ProductId::VmapSourceModel(_) => {
            c.magic(b"VMAP_7.0")?;
            c.magic(b"WMOD")?;
            if c.word()? != 8 {
                return Err(Error::Framing);
            }
            c.word()?; // RootWMOID
            if c.at < bytes.len() {
                c.magic(b"GMOD")?;
                let groups = c.count()?;
                if groups == 0 || groups > 4096 {
                    return Err(Error::Budget);
                }
                for _ in 0..groups {
                    vmap_group(&mut c, allow_vmap_liquid)?;
                }
                c.magic(b"GBIH")?;
                c.bih()?;
            }
        }
    }
    if c.at != bytes.len() {
        return Err(Error::Framing);
    }
    Ok(models)
}

fn vmap_group(c: &mut Cursor<'_>, allow_vmap_liquid: bool) -> Result<(), Error> {
    c.bounds()?;
    // These source model flags/IDs are deliberately opaque: no collision mask is certified.
    c.word()?;
    c.word()?;
    c.magic(b"VERT")?;
    let length = c.word()? as usize;
    let vertices = c.count()?;
    if length != 4 + 12 * vertices {
        return Err(Error::Framing);
    }
    if vertices == 0 {
        return Ok(());
    }
    c.floats(vertices * 3)?;
    c.magic(b"TRIM")?;
    let length = c.word()? as usize;
    let triangles = c.count()?;
    if length != 4 + 12 * triangles {
        return Err(Error::Framing);
    }
    for _ in 0..triangles * 3 {
        if c.word()? as usize >= vertices {
            return Err(Error::Format);
        }
    }
    c.magic(b"MBIH")?;
    c.bih()?;
    c.magic(b"LIQU")?;
    let length = c.word()? as usize;
    if length != 0 {
        if !allow_vmap_liquid {
            return Err(Error::Unsupported);
        }
        let width = c.word()?;
        let height = c.word()?;
        let (heights, flags) = liquid_dimensions(width, height, length)?;
        // The source size omits the serialized type word; require actual payload.
        let required = 16 + heights * 4 + flags;
        if c.bytes.len() - c.at < required {
            return Err(Error::Framing);
        }
        c.floats(3)?;
        c.word()?; // raw liquid type is preserved, including unknown IDs
        c.floats(heights)?;
        c.take(flags)?;
    }
    Ok(())
}

/// WorldModel.cpp::GetFileSize omits iType although writeToFile writes it.
/// Counts and arithmetic are bounded before any allocation or array iteration.
pub(super) fn liquid_dimensions(
    width: u32,
    height: u32,
    declared: usize,
) -> Result<(usize, usize), Error> {
    let width = width as usize;
    let height = height as usize;
    let heights = width
        .checked_add(1)
        .and_then(|x| height.checked_add(1).and_then(|y| x.checked_mul(y)))
        .ok_or(Error::Budget)?;
    let flags = width.checked_mul(height).ok_or(Error::Budget)?;
    if heights > MAX_ELEMENTS || flags > MAX_ELEMENTS {
        return Err(Error::Budget);
    }
    if declared != 20 + heights * 4 + flags {
        return Err(Error::Framing);
    }
    Ok((heights, flags))
}

fn mmap_tile(c: &mut Cursor<'_>) -> Result<(), Error> {
    if c.word()? != 0x4d4d4150 || c.word()? != 7 || c.word()? != 8 {
        return Err(Error::Format);
    }
    let size = c.word()? as usize;
    if c.word()? > 1 {
        return Err(Error::Unsupported);
    }
    if size != c.bytes.len() - c.at || size < 100 {
        return Err(Error::Framing);
    }
    // DT_NAVMESH_MAGIC = 'D'<<24 | 'N'<<16 | 'A'<<8 | 'V', serialized LE.
    if c.word()? != 0x444e4156 || c.word()? != 7 {
        return Err(Error::Format);
    }
    c.word()?; // Detour X/Y are origin-relative; NOT the source grid filename pair.
    c.word()?;
    if c.word()? != 0 {
        return Err(Error::Unsupported);
    } // tile layer
    c.word()?; // userId
    let polygons = c.count()?;
    let vertices = c.count()?;
    let links = c.count()?;
    let details = c.count()?;
    let detail_vertices = c.count()?;
    let detail_triangles = c.count()?;
    let bv_nodes = c.count()?;
    let offmesh = c.count()?;
    let offmesh_base = c.count()?;
    if offmesh != 0 {
        return Err(Error::Unsupported);
    }
    if polygons == 0
        || !(3..65535).contains(&vertices)
        || details != polygons
        || offmesh_base != polygons
    {
        return Err(Error::Format);
    }
    // Body dimensions and BV quantization are data, not a resolved owner context.
    for _ in 0..3 {
        if c.float()? < 0.0 {
            return Err(Error::Format);
        }
    }
    c.bounds()?;
    if c.float()? <= 0.0 {
        return Err(Error::Format);
    }
    // Exact pinned DT_POLYREF64 structure widths and dtAlign4 sections.
    // Payloads remain opaque; an in-bounds byte layout is not safe native load evidence.
    for (count, width) in [
        (vertices, 12),
        (polygons, 32),
        (links, 16),
        (details, 12),
        (detail_vertices, 12),
        (detail_triangles, 4),
        (bv_nodes, 16),
    ] {
        c.take(count.checked_mul(width).ok_or(Error::Budget)?)?;
    }
    Ok(())
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, length: usize) -> Result<&'a [u8], Error> {
        let end = self.at.checked_add(length).ok_or(Error::Budget)?;
        let result = self.bytes.get(self.at..end).ok_or(Error::Framing)?;
        self.at = end;
        Ok(result)
    }
    fn magic(&mut self, magic: &[u8]) -> Result<(), Error> {
        if self.take(magic.len())? != magic {
            return Err(Error::Format);
        }
        Ok(())
    }
    fn word(&mut self) -> Result<u32, Error> {
        u32_at(self.take(4)?, 0)
    }
    fn count(&mut self) -> Result<usize, Error> {
        let count = self.word()? as usize;
        if count > MAX_ELEMENTS {
            return Err(Error::Budget);
        }
        Ok(count)
    }
    fn float(&mut self) -> Result<f32, Error> {
        let value = f32::from_bits(self.word()?);
        if !value.is_finite() {
            return Err(Error::Format);
        }
        Ok(value)
    }
    fn floats(&mut self, count: usize) -> Result<(), Error> {
        // Check remaining size before spending work on a malicious declared count.
        let bytes = self.take(count.checked_mul(4).ok_or(Error::Budget)?)?;
        for word in bytes.as_chunks::<4>().0 {
            if !f32::from_bits(u32_at(word, 0)?).is_finite() {
                return Err(Error::Format);
            }
        }
        Ok(())
    }
    fn bounds(&mut self) -> Result<(), Error> {
        let low = [self.float()?, self.float()?, self.float()?];
        let high = [self.float()?, self.float()?, self.float()?];
        if low.into_iter().zip(high).any(|(a, b)| a > b) {
            return Err(Error::Format);
        }
        Ok(())
    }
    fn bih(&mut self) -> Result<(), Error> {
        self.bounds()?;
        let tree_words = self.count()?;
        if tree_words % 3 != 0 {
            return Err(Error::Framing);
        }
        self.take(tree_words * 4)?;
        let objects = self.count()?;
        self.take(objects * 4)?;
        Ok(())
    }
}
