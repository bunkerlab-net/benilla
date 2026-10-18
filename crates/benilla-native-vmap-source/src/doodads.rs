// SPDX-License-Identifier: GPL-3.0-or-later
//! Pure bounded WMO doodad inputs, separate from the raw collision writer.
//!
//! Source: CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
//! contrib/vmap_extractor/vmapextract/{wmo.cpp,wmo.h,vmapexport.cpp}.
//! WMORoot::open copies MODN Paths and each extraction path before modifying
//! the archive buffer's basename. These copies must retain their original bytes.
//! Repeated MODN chunks replace Paths but still perform every extraction in
//! source order; successful-name offset membership belongs to the coordinator.
//! MODS, MODD, and MODR likewise replace their arrays rather than append to them.
//!
//! The stricter envelope requires complete chunks/records, MVER 17, bounded
//! counts and names, finite raw transforms, and nonoverflowing MODS ranges.
//! Unreferenced MODD name offsets remain raw: only a caller that dereferences
//! a retained offset must check it against the final copied Paths. No set union,
//! group-skip decision, unordered-set enumeration, G3D transform, generated ID,
//! output collision test, assembly, or gameplay authority is produced here.

use benilla_native_vmap::{MAX_WMO_CHUNKS, MAX_WMO_FILE_BYTES, MAX_WMO_GROUPS};
use std::fmt;

const MAX_DOODAD_NAMES: usize = 100_000;
const MAX_DOODAD_NAME_BYTES: usize = 511;
const MAX_DOODAD_SETS: usize = 65_536;
const MAX_DOODAD_SPAWNS: usize = 100_000;
const MAX_DOODAD_REFERENCES: usize = 1_048_576;
const MOGP_HEADER_BYTES: usize = 68;

/// Original MODS bytes and numeric range; this is not a selected-set witness.
#[derive(Clone, PartialEq, Eq)]
pub struct DoodadSet {
    /// All 20 source bytes, without assuming a terminating NUL or encoding.
    pub name: [u8; 20],
    pub start_index: u32,
    pub count: u32,
    pub padding: [u8; 4],
}

impl fmt::Debug for DoodadSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DoodadSet")
            .field("start_index", &self.start_index)
            .field("count", &self.count)
            .finish_non_exhaustive()
    }
}

/// Raw MODD record, before any placement composition or G3D conversion.
#[derive(Debug, Clone, PartialEq)]
pub struct DoodadSpawn {
    /// The source's 24-bit field, not the complete leading uint32 word.
    pub name_index: u32,
    /// Retained for fidelity; the source NameIndex bitfield ignores these bits.
    pub ignored_name_high_byte: u8,
    pub position: [f32; 3],
    /// Source Quaternion storage order X, Y, Z, W; not normalized or permuted.
    pub rotation: [f32; 4],
    pub scale: f32,
    pub color: u32,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DoodadName {
    pub(crate) name_index: u32,
    pub(crate) path: Vec<u8>,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct DoodadExtractionEvent {
    /// Zero-based root chunk ordinal, including MVER and unconsumed chunks.
    pub(crate) chunk_ordinal: usize,
    /// Byte offset within this event's MODN chunk, not an ordinal name index.
    pub(crate) name_index: u32,
    /// The local source copy passed to ExtractSingleModel before normalization.
    pub(crate) path: Vec<u8>,
}

#[derive(Default, Clone, PartialEq)]
pub(crate) struct RootDoodads {
    pub(crate) group_count: u32,
    /// Exact final MODN copy, including every NUL and empty entry.
    pub(crate) paths: Vec<u8>,
    pub(crate) names: Vec<DoodadName>,
    pub(crate) sets: Vec<DoodadSet>,
    pub(crate) spawns: Vec<DoodadSpawn>,
    pub(crate) events: Vec<DoodadExtractionEvent>,
}

impl RootDoodads {
    /// Resolve a source-used offset in final Paths. Offsets from an earlier
    /// MODN can land inside a final name; the source then reads that suffix.
    pub(crate) fn path_at(&self, name_index: u32) -> Result<&[u8], DoodadError> {
        let tail = self
            .paths
            .get(name_index as usize..)
            .ok_or(DoodadError("vmap.doodad_name_offset"))?;
        let length = tail
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(DoodadError("vmap.doodad_name_offset"))?;
        Ok(&tail[..length])
    }
}

/// Fixed diagnostics only; no path, set-name, or asset bytes are formatted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DoodadError(&'static str);

impl DoodadError {
    pub fn code(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for DoodadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

impl std::error::Error for DoodadError {}

pub(crate) fn parse_root_doodads(bytes: &[u8]) -> Result<RootDoodads, DoodadError> {
    file_limit(bytes.len())?;
    let mut result = RootDoodads::default();
    let mut chunks = Chunks::new(bytes);
    let mut chunk_count = 0;
    let mut version = false;
    let mut header = false;
    let mut set_records = 0;
    let mut spawn_records = 0;
    while let Some(chunk) = chunks.next(&mut chunk_count)? {
        let ordinal = chunk_count - 1;
        leading_version(&chunk, ordinal)?;
        match &chunk.tag {
            b"MVER" => parse_version(chunk.payload, &mut version)?,
            b"MOHD" => {
                if header {
                    return Err(DoodadError("vmap.doodad_duplicate_header"));
                }
                if chunk.payload.len() != 64 {
                    return Err(DoodadError("vmap.doodad_root_header"));
                }
                result.group_count = word(chunk.payload, 4);
                if result.group_count > MAX_WMO_GROUPS {
                    return Err(DoodadError("vmap.doodad_group_count"));
                }
                header = true;
            }
            b"MODN" => parse_names(chunk.payload, ordinal, &mut result)?,
            b"MODS" => {
                let count = record_count(chunk.payload, 32, "vmap.doodad_set_record")?;
                charge(&mut set_records, count, MAX_DOODAD_SETS)?;
                result.sets = parse_sets(chunk.payload, count)?;
            }
            b"MODD" => {
                let count = record_count(chunk.payload, 40, "vmap.doodad_spawn_record")?;
                charge(&mut spawn_records, count, MAX_DOODAD_SPAWNS)?;
                result.spawns = parse_spawns(chunk.payload, count)?;
            }
            b"MLIQ" => return Err(DoodadError("vmap.doodad_unsupported_liquid")),
            b"MOGP" => return Err(DoodadError("vmap.doodad_unexpected_chunk")),
            _ => {}
        }
    }
    if !version || !header {
        return Err(DoodadError("vmap.doodad_missing_root_header"));
    }
    Ok(result)
}

/// Final source MODR array, with order and duplicates preserved. The caller must
/// use the independent raw writer's retained-group result before accepting any
/// reference; malformed or missing spawn indices are not resolved in this layer.
pub(crate) fn parse_group_doodad_references(bytes: &[u8]) -> Result<Vec<u16>, DoodadError> {
    file_limit(bytes.len())?;
    let mut chunks = Chunks::new(bytes);
    let mut chunk_count = 0;
    let mut version = false;
    let mut header = false;
    let mut references = Vec::new();
    let mut reference_records = 0;
    while let Some(chunk) = chunks.next(&mut chunk_count)? {
        leading_version(&chunk, chunk_count - 1)?;
        match &chunk.tag {
            b"MVER" => parse_version(chunk.payload, &mut version)?,
            b"MOGP" => {
                if header {
                    return Err(DoodadError("vmap.doodad_duplicate_header"));
                }
                if chunk.payload.len() < MOGP_HEADER_BYTES {
                    return Err(DoodadError("vmap.doodad_group_header"));
                }
                header = true;
                let mut children = Chunks::new(&chunk.payload[MOGP_HEADER_BYTES..]);
                while let Some(child) = children.next(&mut chunk_count)? {
                    consume_group_chunk(child, &mut references, &mut reference_records)?;
                }
            }
            _ => consume_group_chunk(chunk, &mut references, &mut reference_records)?,
        }
    }
    if !version || !header {
        return Err(DoodadError("vmap.doodad_missing_group_header"));
    }
    Ok(references)
}

fn consume_group_chunk(
    chunk: Chunk<'_>,
    references: &mut Vec<u16>,
    reference_records: &mut usize,
) -> Result<(), DoodadError> {
    match &chunk.tag {
        b"MODR" => {
            let count = record_count(chunk.payload, 2, "vmap.doodad_reference_record")?;
            charge(reference_records, count, MAX_DOODAD_REFERENCES)?;
            let mut next = Vec::new();
            reserve(&mut next, count)?;
            next.extend(
                chunk
                    .payload
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|&raw| u16::from_le_bytes(raw)),
            );
            *references = next;
            Ok(())
        }
        // Geometry/liquid validity belongs to the complete raw writer, which
        // acquisition invokes before this metadata-only reference projection.
        b"MLIQ" => Ok(()),
        b"MVER" | b"MOHD" | b"MOGP" => Err(DoodadError("vmap.doodad_unexpected_chunk")),
        _ => Ok(()),
    }
}

fn parse_names(
    bytes: &[u8],
    chunk_ordinal: usize,
    result: &mut RootDoodads,
) -> Result<(), DoodadError> {
    let mut names = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        if result.events.len() >= MAX_DOODAD_NAMES {
            return Err(DoodadError("vmap.doodad_name_count"));
        }
        let tail = &bytes[at..];
        let length = tail
            .iter()
            .position(|&byte| byte == 0)
            .ok_or(DoodadError("vmap.doodad_name_nul"))?;
        if length > MAX_DOODAD_NAME_BYTES {
            return Err(DoodadError("vmap.doodad_name_length"));
        }
        let name_index = u32::try_from(at).map_err(|_| DoodadError("vmap.doodad_name_offset"))?;
        reserve(&mut result.events, 1)?;
        reserve(&mut names, 1)?;
        result.events.push(DoodadExtractionEvent {
            chunk_ordinal,
            name_index,
            path: copy_bytes(&tail[..length])?,
        });
        names.push(DoodadName {
            name_index,
            path: copy_bytes(&tail[..length])?,
        });
        at += length + 1;
    }
    result.paths = copy_bytes(bytes)?;
    result.names = names;
    Ok(())
}

fn parse_sets(bytes: &[u8], count: usize) -> Result<Vec<DoodadSet>, DoodadError> {
    let mut sets = Vec::new();
    reserve(&mut sets, count)?;
    for raw in bytes.as_chunks::<32>().0 {
        let start_index = word(raw, 20);
        let count = word(raw, 24);
        start_index
            .checked_add(count)
            .ok_or(DoodadError("vmap.doodad_set_range"))?;
        sets.push(DoodadSet {
            name: raw[..20].try_into().expect("whole MODS record"),
            start_index,
            count,
            padding: raw[28..32].try_into().expect("whole MODS record"),
        });
    }
    Ok(sets)
}

fn parse_spawns(bytes: &[u8], count: usize) -> Result<Vec<DoodadSpawn>, DoodadError> {
    let mut spawns = Vec::new();
    reserve(&mut spawns, count)?;
    for raw in bytes.as_chunks::<40>().0 {
        let mut transform = [0.0; 8];
        for (index, value) in transform.iter_mut().enumerate() {
            *value = f32::from_bits(word(raw, 4 + index * 4));
            if !value.is_finite() {
                return Err(DoodadError("vmap.doodad_nonfinite_transform"));
            }
        }
        let name_word = word(raw, 0);
        spawns.push(DoodadSpawn {
            name_index: name_word & 0x00ff_ffff,
            ignored_name_high_byte: (name_word >> 24) as u8,
            position: transform[..3].try_into().expect("three position values"),
            rotation: transform[3..7].try_into().expect("four rotation values"),
            scale: transform[7],
            color: word(raw, 36),
        });
    }
    Ok(spawns)
}

struct Chunk<'a> {
    tag: [u8; 4],
    payload: &'a [u8],
}

struct Chunks<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Chunks<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    fn next(&mut self, count: &mut usize) -> Result<Option<Chunk<'a>>, DoodadError> {
        if self.at == self.bytes.len() {
            return Ok(None);
        }
        if *count >= MAX_WMO_CHUNKS {
            return Err(DoodadError("vmap.doodad_chunk_count"));
        }
        *count += 1;
        let rest = &self.bytes[self.at..];
        if rest.len() < 8 {
            return Err(DoodadError("vmap.doodad_chunk_header"));
        }
        let mut tag: [u8; 4] = rest[..4].try_into().expect("complete chunk header");
        tag.reverse();
        let end = self
            .at
            .checked_add(8)
            .and_then(|start| start.checked_add(word(rest, 4) as usize))
            .filter(|&end| end <= self.bytes.len())
            .ok_or(DoodadError("vmap.doodad_chunk_payload"))?;
        let payload = &self.bytes[self.at + 8..end];
        self.at = end;
        Ok(Some(Chunk { tag, payload }))
    }
}

fn leading_version(chunk: &Chunk<'_>, ordinal: usize) -> Result<(), DoodadError> {
    if ordinal == 0 && chunk.tag != *b"MVER" {
        return Err(DoodadError("vmap.doodad_missing_version"));
    }
    Ok(())
}

fn parse_version(bytes: &[u8], seen: &mut bool) -> Result<(), DoodadError> {
    if *seen {
        return Err(DoodadError("vmap.doodad_duplicate_version"));
    }
    if bytes.len() != 4 || word(bytes, 0) != 17 {
        return Err(DoodadError("vmap.doodad_version"));
    }
    *seen = true;
    Ok(())
}

fn file_limit(bytes: usize) -> Result<(), DoodadError> {
    if bytes > MAX_WMO_FILE_BYTES {
        return Err(DoodadError("vmap.doodad_file_bytes"));
    }
    Ok(())
}

fn record_count(bytes: &[u8], stride: usize, code: &'static str) -> Result<usize, DoodadError> {
    if !bytes.len().is_multiple_of(stride) {
        return Err(DoodadError(code));
    }
    Ok(bytes.len() / stride)
}

fn charge(total: &mut usize, count: usize, maximum: usize) -> Result<(), DoodadError> {
    *total = total
        .checked_add(count)
        .filter(|&next| next <= maximum)
        .ok_or(DoodadError("vmap.doodad_record_limit"))?;
    Ok(())
}

fn reserve<T>(items: &mut Vec<T>, additional: usize) -> Result<(), DoodadError> {
    items
        .try_reserve(additional)
        .map_err(|_| DoodadError("vmap.doodad_allocation"))
}

fn copy_bytes(bytes: &[u8]) -> Result<Vec<u8>, DoodadError> {
    let mut result = Vec::new();
    reserve(&mut result, bytes.len())?;
    result.extend_from_slice(bytes);
    Ok(result)
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().expect("bounded word"))
}

#[cfg(test)]
#[path = "doodads_tests.rs"]
mod tests;
