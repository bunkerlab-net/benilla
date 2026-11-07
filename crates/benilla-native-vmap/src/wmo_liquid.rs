//! Validated MLIQ arrays and the pinned source's LIQU field projection.

use super::{check_finite, exact_size, put_word, word, WmoError, WmoFile};

/// Maximum source MLIQ vertices per chunk, including reset/skipped chunks.
pub const MAX_WMO_LIQUID_VERTICES: usize = 1_048_576;
/// Maximum source MLIQ tile flags per chunk, including reset/skipped chunks.
pub const MAX_WMO_LIQUID_TILES: usize = 1_048_576;

/// Versioned deterministic normalization of source ABI tail padding only.
///
/// Deviates: the pinned writer copies two uninitialized bytes after its signed
/// 16-bit type. Independent extractor/assembler layout and padding-invariance
/// probes establish that offsets 30–31 are unused padding in the 32-byte header.
/// This profile writes zero there; it does not claim parity for undefined bytes.
pub const WMO_LIQUID_HEADER_ZERO_PADDING_V1: &str = "wmo-liquid-header-zero-padding-v1";

const SOURCE_HEADER_BYTES: usize = 30;
const RAW_HEADER_BYTES: usize = 32;
const SOURCE_VERTEX_BYTES: usize = 8;
const MAX_ROOT_MEMBER_BYTES: usize = 511;

pub(super) fn validate_root_member(member: &str) -> Result<(), WmoError> {
    // Match acquisition's safe archive-member envelope without changing case.
    let bytes = member.as_bytes();
    if bytes.len() > MAX_ROOT_MEMBER_BYTES
        || !bytes.ends_with(b".wmo")
        || !bytes.iter().all(|&b| {
            b.is_ascii_alphanumeric()
                || matches!(b, b'\\' | b'_' | b'-' | b'.' | b' ' | b'(' | b')')
        })
        || bytes.split(|&b| b == b'\\').any(|part| {
            part.is_empty()
                || part == b"."
                || part == b".."
                || part.first() == Some(&b' ')
                || matches!(part.last(), Some(b' ' | b'.'))
        })
        || bytes
            .rsplit(|&b| b == b'\\')
            .next()
            .unwrap_or_default()
            .len()
            <= 4
    {
        return Err(WmoError::InvalidSourceRootMember);
    }
    Ok(())
}

pub(super) struct Liquid<'a> {
    header: &'a [u8],
    vertices: &'a [u8],
    tiles: &'a [u8],
    advertised_bytes: u32,
}

impl<'a> Liquid<'a> {
    pub(super) fn parse(bytes: &'a [u8], group: u32) -> Result<Self, WmoError> {
        let file = WmoFile::Group(group);
        if bytes.len() < SOURCE_HEADER_BYTES {
            return Err(WmoError::InvalidChunkSize {
                file,
                chunk: *b"MLIQ",
                bytes: bytes.len(),
            });
        }
        let dimensions: [i32; 4] = std::array::from_fn(|i| {
            i32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
        });
        let [xverts, yverts, xtiles, ytiles] = dimensions;
        if dimensions.iter().any(|&n| n <= 0)
            || xtiles.checked_add(1) != Some(xverts)
            || ytiles.checked_add(1) != Some(yverts)
        {
            return Err(WmoError::InvalidLiquidDimensions {
                group,
                vertices: [xverts, yverts],
                tiles: [xtiles, ytiles],
            });
        }
        // Bound every signed source product before host conversions or slicing.
        let vertex_count = xverts
            .checked_mul(yverts)
            .ok_or(WmoError::LiquidSizeOverflow { group })?;
        let tile_count = xtiles
            .checked_mul(ytiles)
            .ok_or(WmoError::LiquidSizeOverflow { group })?;
        let vertex_bytes = vertex_count
            .checked_mul(SOURCE_VERTEX_BYTES as i32)
            .ok_or(WmoError::LiquidSizeOverflow { group })?;
        let advertised_bytes = vertex_bytes
            .checked_add(tile_count)
            .and_then(|n| n.checked_add(RAW_HEADER_BYTES as i32))
            .ok_or(WmoError::LiquidSizeOverflow { group })?;
        let vertex_count = vertex_count as usize;
        let tile_count = tile_count as usize;
        for (count, maximum) in [
            (vertex_count, MAX_WMO_LIQUID_VERTICES),
            (tile_count, MAX_WMO_LIQUID_TILES),
        ] {
            if count > maximum {
                return Err(WmoError::CountLimit {
                    file,
                    chunk: *b"MLIQ",
                    count,
                    maximum,
                });
            }
        }
        let vertex_bytes = vertex_bytes as usize;
        let expected = SOURCE_HEADER_BYTES
            .checked_add(vertex_bytes)
            .and_then(|n| n.checked_add(tile_count))
            .ok_or(WmoError::LiquidSizeOverflow { group })?;
        exact_size(bytes, file, *b"MLIQ", expected)?;
        check_finite(&bytes[16..28], file, *b"MLIQ")?;
        let vertices = &bytes[SOURCE_HEADER_BYTES..SOURCE_HEADER_BYTES + vertex_bytes];
        for (ordinal, raw) in vertices
            .as_chunks::<SOURCE_VERTEX_BYTES>()
            .0
            .iter()
            .enumerate()
        {
            if !f32::from_bits(word(raw, 4)).is_finite() {
                return Err(WmoError::NonFiniteLiquidHeight { group, ordinal });
            }
        }
        Ok(Self {
            header: &bytes[..SOURCE_HEADER_BYTES],
            vertices,
            tiles: &bytes[SOURCE_HEADER_BYTES + vertex_bytes..],
            advertised_bytes: advertised_bytes as u32,
        })
    }

    pub(super) fn output_size(&self) -> usize {
        8 + RAW_HEADER_BYTES + self.vertices.len() / SOURCE_VERTEX_BYTES * 4 + self.tiles.len()
    }

    pub(super) fn write(
        &self,
        output: &mut Vec<u8>,
        root_flags: u32,
        group_type: u32,
        group_flags: u32,
        root_member: &str,
    ) {
        output.extend_from_slice(b"LIQU");
        // Upstream advertises 8 bytes per source vertex but emits only heights.
        put_word(output, self.advertised_bytes);
        output.extend_from_slice(&self.header[..28]);
        let entry = liquid_entry(root_flags, group_type, group_flags, root_member, self.tiles);
        // Pinned ABI narrows to signed short. Preserve its low 16 bits; assembler
        // later sign-extends that short to uint32 rather than zero-extending it.
        output.extend_from_slice(&(entry as u16).to_le_bytes());
        output.extend_from_slice(&[0, 0]);
        for vertex in self.vertices.as_chunks::<SOURCE_VERTEX_BYTES>().0 {
            output.extend_from_slice(&vertex[4..8]);
        }
        output.extend_from_slice(self.tiles);
    }
}

fn liquid_entry(
    root_flags: u32,
    group_type: u32,
    group_flags: u32,
    root_member: &str,
    tiles: &[u8],
) -> u32 {
    let mut entry = if root_flags & 4 != 0 {
        group_type
    } else if group_type == 15 {
        0
    } else {
        // Source liquidType is uint32; addition wraps before fallback selection.
        group_type.wrapping_add(1)
    };
    if entry == 0 {
        entry = tiles
            .iter()
            .find(|&&tile| tile & 0x0f != 15)
            .map_or(0, |&tile| u32::from(tile & 0x0f) + 1);
    }
    if entry != 0 && entry < 21 {
        entry = match (entry as u8 - 1) & 3 {
            0 => u32::from(group_flags & 0x80000 != 0) + 1,
            1 => 2,
            2 => 3,
            3 if root_member.contains("Stratholme_raid") => 21,
            3 => 4,
            _ => unreachable!(),
        };
    }
    entry
}
