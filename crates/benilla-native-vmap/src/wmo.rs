//! Bounded projection of the pinned WMO root/group raw geometry writer.

use crate::wmo_error::{WmoError, WmoFile};
use crate::{put_word, vertex, word};

#[path = "wmo_liquid.rs"]
mod liquid;
use liquid::Liquid;
pub use liquid::{
    MAX_WMO_LIQUID_TILES, MAX_WMO_LIQUID_VERTICES, WMO_LIQUID_HEADER_ZERO_PADDING_V1,
};

/// Maximum bytes in any one complete WMO root or group input (64 MiB).
pub const MAX_WMO_FILE_BYTES: usize = 64 * 1024 * 1024;
/// Maximum summed root/group input bytes, charging repeated slices repeatedly.
pub const MAX_WMO_TOTAL_BYTES: usize = 256 * 1024 * 1024;
/// Maximum root-declared group files, including groups the source skips.
pub const MAX_WMO_GROUPS: u32 = 1024;
/// Maximum vertices per group, including unused and skipped-group vertices.
pub const MAX_WMO_VERTICES: usize = 65_536;
/// Maximum MOPY triangles per group, before the source's material filtering.
pub const MAX_WMO_TRIANGLES: usize = 65_536;
/// Maximum source MOBA batches per group.
pub const MAX_WMO_BATCHES: usize = 65_536;
/// Maximum chunks per file, counting nested MOGP children as well.
pub const MAX_WMO_CHUNKS: usize = 65_536;
/// Maximum complete raw WMO output (128 MiB).
pub const MAX_WMO_OUTPUT_BYTES: usize = 128 * 1024 * 1024;

const MOGP_HEADER_BYTES: usize = 68;
const GROUP_FIXED_BYTES: usize = 72;
const UNREFERENCED: u32 = u32::MAX;

/// Immutable complete bytes for one root-declared group. `group_index` is the
/// source filename suffix (`_000`, `_001`, ...), not the MOGP group WMO ID.
/// Acquisition and association of the bytes with that filename remain the
/// caller's responsibility; this interface does not establish a source lease.
#[derive(Debug, Clone, Copy)]
pub struct WmoGroupInput<'a> {
    pub group_index: u32,
    pub bytes: &'a [u8],
}

/// Complete raw-geometry projection, before any VMap assembly or placement.
/// Even zero retained groups produce the source's complete 20-byte raw header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WmoCollision {
    pub raw_vmap: Vec<u8>,
    pub declared_group_count: u32,
    pub retained_group_count: u32,
    /// Retained groups with an active final MLIQ, serialized as raw LIQU.
    pub liquid_group_count: u32,
    /// Source filename indices surviving the same validated ShouldSkip pass.
    /// Dependency capture uses these instead of duplicating group skip rules.
    pub retained_group_indices: Vec<u32>,
    /// The source's group return values, stored at raw offset 8. Despite the
    /// upstream local variable's name, this is a triangle count, not vertices.
    pub collision_triangle_count: u32,
}

/// Convert one complete root and every declared group to small (`-s`) raw VMap
/// geometry, preserving the pinned CMaNGOS source's group and triangle order.
///
/// Exactly `MOHD.nGroups` inputs must appear in ascending source filename-index
/// order, including groups later excluded by `ShouldSkip`. All supplied groups
/// are validated before any result is returned. Missing or malformed groups
/// never become partial success or a collision-absence claim.
///
/// The safety envelope requires a leading MVER version 17 in root and groups,
/// singleton consumed chunks, a 64-byte MOHD, a 68-byte MOGP header, and whole
/// MOPY/MOVI/MOVT/MOBA records,
/// matching MOPY triangle/MOVI index counts, in-range indices, finite vertices,
/// and finite ordered root/group bounds. These checks also apply to skipped
/// groups. Unknown and doodad chunks are bounded but their payloads are not
/// interpreted. No renderer parser or render-mesh fallback is used.
///
/// MOGP children begin after exactly 68 bytes; advertised container boundaries
/// must be complete. Header-only MOGP with subsequent sibling geometry is also
/// accepted, as in the source's flattened traversal. Any MLIQ is rejected,
/// including empty chunks, skipped groups, and chunks followed by the source's
/// liquid-state reset. A stale `liquidType` alone does not imply liquid data.
///
/// This is only the raw geometry projection. Source MODN model extraction,
/// MODS/MODD/MODR doodad side effects, placements, archive acquisition, files,
/// assembly, map products, and gameplay admission are outside this interface.
pub fn extract_wmo_collision(
    root_bytes: &[u8],
    inputs: &[WmoGroupInput<'_>],
) -> Result<WmoCollision, WmoError> {
    extract(root_bytes, inputs, LiquidProfile::Dry)
}

/// Convert complete WMO bytes with bounded MLIQ support and the source member
/// spelling required by the case-sensitive `Stratholme_raid` liquid override.
///
/// The caller must supply the witnessed source lookup member after the pinned
/// basename normalization, not the original listfile spelling, a basename alone,
/// or a fabricated filename profile. Directory spelling remains unchanged. This function validates a bounded
/// ASCII archive path ending in `.wmo`; it does not establish a source lease.
/// Numeric group suffixes leave the source substring test unchanged.
///
/// All dry validation remains in force. Every MLIQ is validated, including in
/// skipped groups or before a later chunk resets liquid state. Repeated MLIQ
/// chunks are allowed, with only the last active source chunk emitted. Tiles must have
/// positive dimensions, vertices must be tiles plus one on each axis, all
/// positions/heights must be finite, and the 30-byte header plus 8-byte source
/// vertices and tile flags must exactly consume the chunk. The final source
/// chunk alone determines whether LIQU is emitted. Output normalizes only the
/// proven-unused header tail bytes with [`WMO_LIQUID_HEADER_ZERO_PADDING_V1`];
/// their source values are undefined, so this is not a full-byte parity claim.
/// LIQU retains the source advertised-size quirk. No map assembly is performed.
pub fn extract_wmo_collision_from_source(
    root_member: &str,
    root_bytes: &[u8],
    inputs: &[WmoGroupInput<'_>],
) -> Result<WmoCollision, WmoError> {
    liquid::validate_root_member(root_member)?;
    extract(root_bytes, inputs, LiquidProfile::Source(root_member))
}

#[derive(Clone, Copy)]
enum LiquidProfile<'a> {
    Dry,
    Source(&'a str),
}

fn extract(
    root_bytes: &[u8],
    inputs: &[WmoGroupInput<'_>],
    profile: LiquidProfile<'_>,
) -> Result<WmoCollision, WmoError> {
    file_limit(WmoFile::Root, root_bytes.len())?;
    let root = parse_root(root_bytes)?;
    if root.group_count > MAX_WMO_GROUPS {
        return Err(WmoError::GroupCountLimit {
            count: root.group_count,
            maximum: MAX_WMO_GROUPS,
        });
    }
    if inputs.len() != root.group_count as usize {
        return Err(WmoError::GroupCountMismatch {
            declared: root.group_count,
            supplied: inputs.len(),
        });
    }
    let mut total_input = root_bytes.len();
    for (index, input) in inputs.iter().enumerate() {
        if input.group_index != index as u32 {
            return Err(WmoError::GroupOrder {
                expected: index as u32,
                actual: input.group_index,
            });
        }
        file_limit(WmoFile::Group(input.group_index), input.bytes.len())?;
        total_input = total_input
            .checked_add(input.bytes.len())
            .filter(|&n| n <= MAX_WMO_TOTAL_BYTES)
            .ok_or(WmoError::TotalInputLimit {
                maximum: MAX_WMO_TOTAL_BYTES,
            })?;
    }

    let mut groups = Vec::new();
    groups
        .try_reserve_exact(inputs.len())
        .map_err(|_| WmoError::AllocationFailed)?;
    let mut remap = Vec::new();
    let mut total_output = 20;
    let mut triangles = 0u32;
    let mut retained = 0u32;
    let mut liquid_groups = 0u32;
    let mut retained_group_indices = Vec::new();
    retained_group_indices
        .try_reserve_exact(inputs.len())
        .map_err(|_| WmoError::AllocationFailed)?;
    for input in inputs {
        let group = parse_group(input.bytes, input.group_index, profile)?;
        if should_skip(&group, root.names)? {
            continue;
        }
        let counts = collision_remap(&group, &mut remap)?;
        total_output = output_size(total_output, &group, counts)?;
        // Caps bound the total to 1024 * 65536, below the source's signed int.
        triangles += counts.triangles as u32;
        retained += 1;
        liquid_groups += u32::from(group.liquid.is_some());
        retained_group_indices.push(input.group_index);
        groups.push(group);
    }

    let mut output = Vec::new();
    output
        .try_reserve_exact(total_output)
        .map_err(|_| WmoError::AllocationFailed)?;
    output.extend_from_slice(b"VMAPs05\0");
    put_word(&mut output, triangles);
    put_word(&mut output, retained);
    put_word(&mut output, root.id);
    for group in &groups {
        let counts = collision_remap(group, &mut remap)?;
        write_group(&mut output, group, &remap, counts, &root, profile);
    }
    debug_assert_eq!(output.len(), total_output);
    Ok(WmoCollision {
        raw_vmap: output,
        declared_group_count: root.group_count,
        retained_group_count: retained,
        liquid_group_count: liquid_groups,
        retained_group_indices,
        collision_triangle_count: triangles,
    })
}

struct Root<'a> {
    group_count: u32,
    id: u32,
    names: &'a [u8],
    flags: u32,
}

fn parse_root(bytes: &[u8]) -> Result<Root<'_>, WmoError> {
    let file = WmoFile::Root;
    let mut version = None;
    let mut header = None;
    let mut names = None;
    let mut chunks = Chunks::new(bytes, file, 0);
    let mut count = 0;
    while let Some(chunk) = chunks.next(&mut count)? {
        require_leading_version(&chunk, file)?;
        match &chunk.tag {
            b"MVER" => singleton(&mut version, chunk, file)?,
            b"MOHD" => singleton(&mut header, chunk, file)?,
            b"MOGN" => singleton(&mut names, chunk, file)?,
            b"MOGP" => {
                return Err(WmoError::UnexpectedChunk {
                    file,
                    chunk: chunk.tag,
                })
            }
            _ => (),
        }
    }
    check_version(required(version, file, *b"MVER")?, file)?;
    let header = required(header, file, *b"MOHD")?;
    exact_size(header, file, *b"MOHD", 64)?;
    check_bounds(&header[36..60], file, *b"MOHD")?;
    Ok(Root {
        group_count: word(header, 4),
        id: word(header, 32),
        names: names.unwrap_or_default(),
        flags: word(header, 60),
    })
}

struct Group<'a> {
    index: u32,
    header: &'a [u8],
    materials: &'a [u8],
    indices: &'a [u8],
    vertices: &'a [u8],
    batches: &'a [u8],
    liquid: Option<Liquid<'a>>,
}

#[derive(Default)]
struct GroupChunks<'a> {
    materials: Option<&'a [u8]>,
    indices: Option<&'a [u8]>,
    vertices: Option<&'a [u8]>,
    batches: Option<&'a [u8]>,
    liquid: Option<Liquid<'a>>,
}

impl<'a> GroupChunks<'a> {
    fn consume(
        &mut self,
        chunk: Chunk<'a>,
        index: u32,
        profile: LiquidProfile<'_>,
    ) -> Result<(), WmoError> {
        // WMOGroup::open resets LiquEx_size and liquflags for every chunk.
        self.liquid = None;
        let file = WmoFile::Group(index);
        match &chunk.tag {
            b"MOPY" => singleton(&mut self.materials, chunk, file),
            b"MOVI" => singleton(&mut self.indices, chunk, file),
            b"MOVT" => singleton(&mut self.vertices, chunk, file),
            b"MOBA" => singleton(&mut self.batches, chunk, file),
            b"MLIQ" => match profile {
                LiquidProfile::Dry => Err(WmoError::UnsupportedLiquid { group: index }),
                LiquidProfile::Source(_) => {
                    self.liquid = Some(Liquid::parse(chunk.payload, index)?);
                    Ok(())
                }
            },
            b"MVER" | b"MOGP" | b"MOHD" => Err(WmoError::UnexpectedChunk {
                file,
                chunk: chunk.tag,
            }),
            _ => Ok(()),
        }
    }
}

fn parse_group<'a>(
    bytes: &'a [u8],
    index: u32,
    profile: LiquidProfile<'_>,
) -> Result<Group<'a>, WmoError> {
    let file = WmoFile::Group(index);
    let mut version = None;
    let mut header = None;
    let mut fields = GroupChunks::default();
    let mut chunks = Chunks::new(bytes, file, 0);
    let mut count = 0;
    while let Some(chunk) = chunks.next(&mut count)? {
        require_leading_version(&chunk, file)?;
        fields.liquid = None;
        match &chunk.tag {
            b"MVER" => singleton(&mut version, chunk, file)?,
            b"MOGP" => {
                if header.is_some() {
                    return Err(WmoError::DuplicateChunk {
                        file,
                        chunk: chunk.tag,
                    });
                }
                if chunk.payload.len() < MOGP_HEADER_BYTES {
                    return Err(WmoError::InvalidChunkSize {
                        file,
                        chunk: chunk.tag,
                        bytes: chunk.payload.len(),
                    });
                }
                header = Some(&chunk.payload[..MOGP_HEADER_BYTES]);
                let mut children = Chunks::new(
                    &chunk.payload[MOGP_HEADER_BYTES..],
                    file,
                    chunk.offset + 8 + MOGP_HEADER_BYTES,
                );
                while let Some(child) = children.next(&mut count)? {
                    fields.consume(child, index, profile)?;
                }
            }
            _ => fields.consume(chunk, index, profile)?,
        }
    }
    check_version(required(version, file, *b"MVER")?, file)?;
    let header = required(header, file, *b"MOGP")?;
    check_bounds(&header[12..36], file, *b"MOGP")?;
    let group = Group {
        index,
        header,
        materials: required(fields.materials, file, *b"MOPY")?,
        indices: required(fields.indices, file, *b"MOVI")?,
        vertices: required(fields.vertices, file, *b"MOVT")?,
        batches: required(fields.batches, file, *b"MOBA")?,
        liquid: fields.liquid,
    };
    let triangles = record_count(group.materials, file, *b"MOPY", 2, MAX_WMO_TRIANGLES)?;
    let indices = record_count(group.indices, file, *b"MOVI", 2, MAX_WMO_TRIANGLES * 3)?;
    let vertices = record_count(group.vertices, file, *b"MOVT", 12, MAX_WMO_VERTICES)?;
    record_count(group.batches, file, *b"MOBA", 24, MAX_WMO_BATCHES)?;
    if indices != triangles * 3 {
        return Err(WmoError::TriangleIndexCount {
            group: index,
            triangles,
            indices,
        });
    }
    for (ordinal, raw) in group.indices.as_chunks::<2>().0.iter().enumerate() {
        let value = u16::from_le_bytes(*raw);
        if usize::from(value) >= vertices {
            return Err(WmoError::IndexOutOfRange {
                group: index,
                ordinal,
                index: value,
                vertex_count: vertices,
            });
        }
    }
    check_finite(group.vertices, file, *b"MOVT")?;
    Ok(group)
}

fn should_skip(group: &Group<'_>, names: &[u8]) -> Result<bool, WmoError> {
    // ShouldSkip tests unreachable and antiportal flags before dereferencing names.
    if word(group.header, 8) & (0x80 | 0x400_0000) != 0 {
        return Ok(true);
    }
    let offset = i32::from_le_bytes(group.header[..4].try_into().unwrap());
    if offset < 0 {
        return Err(WmoError::NegativeGroupName {
            group: group.index,
            offset,
        });
    }
    let offset = offset as usize;
    if offset >= names.len() {
        return Ok(false);
    }
    // strcmp against this literal reads at most eleven bytes. A mismatch is
    // decisive without reading the rest of an otherwise unused name string.
    for (relative, &expected) in b"antiportal\0".iter().enumerate() {
        let actual = names
            .get(offset + relative)
            .ok_or(WmoError::UnterminatedGroupName {
                group: group.index,
                offset,
            })?;
        if *actual != expected {
            return Ok(false);
        }
    }
    Ok(true)
}

#[derive(Clone, Copy)]
struct CollisionCounts {
    triangles: usize,
    vertices: usize,
}

fn collision_remap(group: &Group<'_>, remap: &mut Vec<u32>) -> Result<CollisionCounts, WmoError> {
    let vertex_count = group.vertices.len() / 12;
    if vertex_count > remap.len() {
        remap
            .try_reserve_exact(vertex_count - remap.len())
            .map_err(|_| WmoError::AllocationFailed)?;
    }
    remap.resize(vertex_count, UNREFERENCED);
    remap.fill(UNREFERENCED);
    let mut triangles = 0;
    for (material, indices) in group
        .materials
        .as_chunks::<2>()
        .0
        .iter()
        .zip(group.indices.as_chunks::<6>().0)
    {
        if collides(material[0]) {
            triangles += 1;
            for raw in indices.as_chunks::<2>().0 {
                remap[usize::from(u16::from_le_bytes(*raw))] = 0;
            }
        }
    }
    let mut vertices = 0;
    // Source remaps in ascending original index order, never first-use order.
    for index in remap.iter_mut() {
        if *index != UNREFERENCED {
            *index = vertices as u32;
            vertices += 1;
        }
    }
    Ok(CollisionCounts {
        triangles,
        vertices,
    })
}

fn collides(flags: u8) -> bool {
    flags & 0x08 != 0 || (flags & 0x20 != 0 && flags & 0x04 == 0)
}

fn output_size(
    total: usize,
    group: &Group<'_>,
    counts: CollisionCounts,
) -> Result<usize, WmoError> {
    total
        .checked_add(GROUP_FIXED_BYTES)
        .and_then(|n| n.checked_add(group.batches.len() / 24 * 4))
        .and_then(|n| n.checked_add(counts.triangles * 6))
        .and_then(|n| n.checked_add(counts.vertices * 12))
        .and_then(|n| n.checked_add(group.liquid.as_ref().map_or(0, Liquid::output_size)))
        .filter(|&n| n <= MAX_WMO_OUTPUT_BYTES)
        .ok_or(WmoError::OutputLimit {
            maximum: MAX_WMO_OUTPUT_BYTES,
        })
}

fn write_group(
    output: &mut Vec<u8>,
    group: &Group<'_>,
    remap: &[u32],
    counts: CollisionCounts,
    root: &Root<'_>,
    profile: LiquidProfile<'_>,
) {
    output.extend_from_slice(&group.header[8..12]); // mogpFlags
    output.extend_from_slice(&group.header[56..60]); // groupWMOID
    output.extend_from_slice(&group.header[12..36]); // authored bounds, unchanged
    put_word(output, u32::from(group.liquid.is_some()));
    output.extend_from_slice(b"GRP ");
    let batches = (group.batches.len() / 24) as u32;
    put_word(output, batches * 4 + 4);
    put_word(output, batches);
    // MOBA is uint16: element 8 in each 12-element record, widened to int32.
    for batch in group.batches.as_chunks::<24>().0 {
        put_word(
            output,
            u32::from(u16::from_le_bytes(batch[16..18].try_into().unwrap())),
        );
    }
    output.extend_from_slice(b"INDX");
    put_word(output, counts.triangles as u32 * 6 + 4);
    put_word(output, counts.triangles as u32 * 3);
    for (material, indices) in group
        .materials
        .as_chunks::<2>()
        .0
        .iter()
        .zip(group.indices.as_chunks::<6>().0)
    {
        if collides(material[0]) {
            for raw in indices.as_chunks::<2>().0 {
                // WMO retains source winding, unlike the M2 raw writer.
                let mapped = remap[usize::from(u16::from_le_bytes(*raw))] as u16;
                output.extend_from_slice(&mapped.to_le_bytes());
            }
        }
    }
    output.extend_from_slice(b"VERT");
    put_word(output, counts.vertices as u32 * 12 + 4);
    put_word(output, counts.vertices as u32);
    for (vertex, &mapped) in group.vertices.as_chunks::<12>().0.iter().zip(remap) {
        if mapped != UNREFERENCED {
            output.extend_from_slice(vertex);
        }
    }
    if let (Some(liquid), LiquidProfile::Source(root_member)) = (&group.liquid, profile) {
        liquid.write(
            output,
            root.flags,
            word(group.header, 52),
            word(group.header, 8),
            root_member,
        );
    }
}

struct Chunk<'a> {
    tag: [u8; 4],
    payload: &'a [u8],
    offset: usize,
}
struct Chunks<'a> {
    bytes: &'a [u8],
    cursor: usize,
    base: usize,
    file: WmoFile,
}

impl<'a> Chunks<'a> {
    fn new(bytes: &'a [u8], file: WmoFile, base: usize) -> Self {
        Self {
            bytes,
            cursor: 0,
            base,
            file,
        }
    }
    fn next(&mut self, count: &mut usize) -> Result<Option<Chunk<'a>>, WmoError> {
        if self.cursor == self.bytes.len() {
            return Ok(None);
        }
        *count += 1;
        if *count > MAX_WMO_CHUNKS {
            return Err(WmoError::ChunkLimit {
                file: self.file,
                maximum: MAX_WMO_CHUNKS,
            });
        }
        let offset = self.base + self.cursor;
        let rest = &self.bytes[self.cursor..];
        if rest.len() < 8 {
            return Err(WmoError::TruncatedChunkHeader {
                file: self.file,
                offset,
            });
        }
        let mut tag: [u8; 4] = rest[..4].try_into().unwrap();
        tag.reverse(); // Source flipcc: WMO stores e.g. MVER as REVM.
        let bytes = word(rest, 4);
        let end = self
            .cursor
            .checked_add(8)
            .and_then(|n| n.checked_add(bytes as usize));
        let end = end
            .filter(|&n| n <= self.bytes.len())
            .ok_or(WmoError::TruncatedChunk {
                file: self.file,
                chunk: tag,
                offset,
                bytes,
            })?;
        let payload = &self.bytes[self.cursor + 8..end];
        self.cursor = end;
        Ok(Some(Chunk {
            tag,
            payload,
            offset,
        }))
    }
}

fn singleton<'a>(
    slot: &mut Option<&'a [u8]>,
    chunk: Chunk<'a>,
    file: WmoFile,
) -> Result<(), WmoError> {
    if slot.replace(chunk.payload).is_some() {
        return Err(WmoError::DuplicateChunk {
            file,
            chunk: chunk.tag,
        });
    }
    Ok(())
}
fn required(value: Option<&[u8]>, file: WmoFile, chunk: [u8; 4]) -> Result<&[u8], WmoError> {
    value.ok_or(WmoError::MissingChunk { file, chunk })
}
fn exact_size(
    bytes: &[u8],
    file: WmoFile,
    chunk: [u8; 4],
    expected: usize,
) -> Result<(), WmoError> {
    if bytes.len() != expected {
        return Err(WmoError::InvalidChunkSize {
            file,
            chunk,
            bytes: bytes.len(),
        });
    }
    Ok(())
}
fn require_leading_version(chunk: &Chunk<'_>, file: WmoFile) -> Result<(), WmoError> {
    // The pinned group opener compares MOGP before terminating its first tag.
    // Canonical leading MVER avoids that source undefined-behavior shape.
    if chunk.offset == 0 && chunk.tag != *b"MVER" {
        return Err(WmoError::UnexpectedChunk {
            file,
            chunk: chunk.tag,
        });
    }
    Ok(())
}

fn check_version(bytes: &[u8], file: WmoFile) -> Result<(), WmoError> {
    exact_size(bytes, file, *b"MVER", 4)?;
    let version = word(bytes, 0);
    if version != 17 {
        return Err(WmoError::UnsupportedVersion { file, version });
    }
    Ok(())
}
fn record_count(
    bytes: &[u8],
    file: WmoFile,
    chunk: [u8; 4],
    stride: usize,
    maximum: usize,
) -> Result<usize, WmoError> {
    if !bytes.len().is_multiple_of(stride) {
        return Err(WmoError::InvalidChunkSize {
            file,
            chunk,
            bytes: bytes.len(),
        });
    }
    let count = bytes.len() / stride;
    if count > maximum {
        return Err(WmoError::CountLimit {
            file,
            chunk,
            count,
            maximum,
        });
    }
    Ok(count)
}
fn file_limit(file: WmoFile, bytes: usize) -> Result<(), WmoError> {
    if bytes > MAX_WMO_FILE_BYTES {
        return Err(WmoError::InputLimit {
            file,
            bytes,
            maximum: MAX_WMO_FILE_BYTES,
        });
    }
    Ok(())
}
fn check_finite(bytes: &[u8], file: WmoFile, chunk: [u8; 4]) -> Result<(), WmoError> {
    for (ordinal, raw) in bytes.as_chunks::<12>().0.iter().enumerate() {
        for (axis, coordinate) in vertex(raw).into_iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(WmoError::NonFinite {
                    file,
                    chunk,
                    ordinal,
                    axis,
                });
            }
        }
    }
    Ok(())
}
fn check_bounds(bytes: &[u8], file: WmoFile, chunk: [u8; 4]) -> Result<(), WmoError> {
    check_finite(bytes, file, chunk)?;
    let corners = bytes.as_chunks::<12>().0;
    for (axis, (low, high)) in vertex(&corners[0])
        .into_iter()
        .zip(vertex(&corners[1]))
        .enumerate()
    {
        if low > high {
            return Err(WmoError::ReversedBounds { file, axis });
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "wmo_tests.rs"]
mod tests;
