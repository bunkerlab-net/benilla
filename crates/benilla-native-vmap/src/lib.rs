//! Bounded M2/WMO collision extraction and standalone source BIH construction.
//!
//! M2 accepts the MD20 256/257 profile; WMO accepts bounded dry version 17 roots
//! with every declared group; the source-member API also accepts validated MLIQ. Both emit little-endian `VMAPs05`
//! bytes. A separate pure BIH builder is documented in BIH-BUILDER.md.
//! Pure raw-model VMO conversion is documented in MODEL-WRITER.md.
//! Safe exact source model identities are documented in SOURCE-MODEL-IDENTITY.md.
//! This crate does not open archives, write files, assemble complete VMaps, generate MMaps,
//! establish a source lease, or admit geometry for gameplay. See `PROVENANCE.md`
//! for the exact source and the stricter input-validation envelope.

mod bih;
mod error;
mod model_writer;
mod source_model_name;
pub use source_model_name::{SourceModelName, SourceModelNameError};
mod wmo;
mod wmo_error;

pub use bih::{
    BihBounds, BihBuildError, SourceBih, MAX_BIH_PRIMITIVES, MAX_BIH_WORDS, MAX_BIH_WORK,
};
pub use error::{CollisionArray, Error};
pub use model_writer::{assemble_raw_model, ModelWriteError};
pub use wmo::{
    extract_wmo_collision, extract_wmo_collision_from_source, WmoCollision, WmoGroupInput,
    MAX_WMO_BATCHES, MAX_WMO_CHUNKS, MAX_WMO_FILE_BYTES, MAX_WMO_GROUPS, MAX_WMO_LIQUID_TILES,
    MAX_WMO_LIQUID_VERTICES, MAX_WMO_OUTPUT_BYTES, MAX_WMO_TOTAL_BYTES, MAX_WMO_TRIANGLES,
    MAX_WMO_VERTICES, WMO_LIQUID_HEADER_ZERO_PADDING_V1,
};
pub use wmo_error::{WmoError, WmoFile};

/// Maximum complete M2 input accepted by this bounded adapter (64 MiB).
pub const MAX_M2_BYTES: usize = 64 * 1024 * 1024;
/// Maximum collision vertices, including vertices unused by any triangle.
pub const MAX_BOUNDING_VERTICES: u32 = 65_536;
/// Maximum collision indices, corresponding to at most 65,536 triangles.
pub const MAX_BOUNDING_INDICES: u32 = 3 * 65_536;

const HEADER_BYTES: usize = 0x144;
const INDICES_HEADER: usize = 0xec;
const VERTICES_HEADER: usize = 0xf4;
const RAW_FIXED_BYTES: usize = 96;

/// The source extractor's explicit absence case or its complete raw output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum M2Collision {
    /// The validated profile's `nBoundingTriangles` is zero. The source does not
    /// inspect vertices, normals, or their offsets in this case.
    NoCollision,
    /// A single raw model, before the separate VMap assembler's transformations.
    RawVmap(Vec<u8>),
}

/// Extract the collision arrays from immutable, complete raw M2 bytes.
///
/// The input must have a complete 324-byte packed header, `MD20` magic and
/// version 256 or 257. A nonempty hull additionally requires bounded, complete
/// index/vertex arrays outside that header, finite vertices, whole triangles,
/// and indices within the vertex array. Unused vertices remain in source order.
/// Normals and render data are not consumed by the pinned extractor and are not
/// validated here. Invalid input is an error, never an empty collision result.
///
/// Output is portable little-endian IEEE-754 binary32 serialization matching
/// the pinned source's little-endian ABI. One checked allocation holds the
/// entire output; input counts bound all loops. There is no filesystem or MPQ
/// access and no inferred or serialized replacement for the source's zero box.
pub fn extract_m2_collision(bytes: &[u8]) -> Result<M2Collision, Error> {
    if bytes.len() > MAX_M2_BYTES {
        return Err(Error::InputLimit {
            bytes: bytes.len(),
            maximum: MAX_M2_BYTES,
        });
    }
    if bytes.len() < HEADER_BYTES {
        return Err(Error::TruncatedHeader { bytes: bytes.len() });
    }
    let magic = bytes[..4].try_into().unwrap();
    if &magic != b"MD20" {
        return Err(Error::UnsupportedMagic(magic));
    }
    let version = word(bytes, 4);
    if !matches!(version, 256 | 257) {
        return Err(Error::UnsupportedVersion(version));
    }

    let index_count = word(bytes, INDICES_HEADER);
    // Model::open returns false before reading any arrays when this is zero.
    if index_count == 0 {
        return Ok(M2Collision::NoCollision);
    }
    check_count(CollisionArray::Indices, index_count, MAX_BOUNDING_INDICES)?;
    if !index_count.is_multiple_of(3) {
        return Err(Error::IncompleteTriangle { index_count });
    }
    let vertex_count = word(bytes, VERTICES_HEADER);
    check_count(
        CollisionArray::Vertices,
        vertex_count,
        MAX_BOUNDING_VERTICES,
    )?;
    let indices = array(
        bytes,
        CollisionArray::Indices,
        index_count,
        word(bytes, INDICES_HEADER + 4),
        2,
    )?;
    let vertices = array(
        bytes,
        CollisionArray::Vertices,
        vertex_count,
        word(bytes, VERTICES_HEADER + 4),
        12,
    )?;

    for (ordinal, raw) in indices.as_chunks::<2>().0.iter().enumerate() {
        let index = u16::from_le_bytes(*raw);
        if u32::from(index) >= vertex_count {
            return Err(Error::IndexOutOfRange {
                ordinal,
                index,
                vertex_count,
            });
        }
    }
    for (ordinal, raw) in vertices.as_chunks::<12>().0.iter().enumerate() {
        for (axis, coordinate) in vertex(raw).into_iter().enumerate() {
            if !coordinate.is_finite() {
                return Err(Error::NonFiniteVertex { ordinal, axis });
            }
        }
    }

    let total = RAW_FIXED_BYTES
        .checked_add(indices.len())
        .and_then(|n| n.checked_add(vertices.len()))
        .ok_or(Error::OutputSizeOverflow)?;
    let index_chunk_bytes = chunk_bytes(indices.len())?;
    let vertex_chunk_bytes = chunk_bytes(vertices.len())?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(total)
        .map_err(|_| Error::AllocationFailed)?;

    // Model::ConvertToVMAPModel: one group, zero root/flags/id, zero AABB/liquid.
    output.extend_from_slice(b"VMAPs05\0");
    put_word(&mut output, vertex_count);
    put_word(&mut output, 1);
    output.extend_from_slice(&[0; 40]);
    output.extend_from_slice(b"GRP ");
    put_word(&mut output, 8);
    put_word(&mut output, 1);
    put_word(&mut output, index_count);
    output.extend_from_slice(b"INDX");
    put_word(&mut output, index_chunk_bytes);
    put_word(&mut output, index_count);
    for triangle in indices.as_chunks::<6>().0 {
        // The source exchanges the second and third indices of each triangle.
        output.extend_from_slice(&triangle[..2]);
        output.extend_from_slice(&triangle[4..6]);
        output.extend_from_slice(&triangle[2..4]);
    }
    output.extend_from_slice(b"VERT");
    put_word(&mut output, vertex_chunk_bytes);
    put_word(&mut output, vertex_count);
    for raw in vertices.as_chunks::<12>().0 {
        // Keep both source coordinate passes, including the two sign-bit flips.
        let [x, y, z] = vertex(raw);
        let opened = [x, z, -y]; // Model::open / fixCoordSystem
        let written = [opened[0], -opened[2], opened[1]]; // ConvertToVMAPModel
        for coordinate in written {
            output.extend_from_slice(&coordinate.to_le_bytes());
        }
    }
    debug_assert_eq!(output.len(), total);
    Ok(M2Collision::RawVmap(output))
}

fn check_count(array: CollisionArray, count: u32, maximum: u32) -> Result<(), Error> {
    if count > maximum {
        Err(Error::CountLimit {
            array,
            count,
            maximum,
        })
    } else {
        Ok(())
    }
}

fn array(
    bytes: &[u8],
    array: CollisionArray,
    count: u32,
    offset: u32,
    stride: u32,
) -> Result<&[u8], Error> {
    if count == 0 {
        return Ok(&bytes[..0]);
    }
    if offset < HEADER_BYTES as u32 {
        return Err(Error::ArrayBeforeHeader { array, offset });
    }
    // Check the 32-bit source file-offset domain before host-size arithmetic.
    let end = count
        .checked_mul(stride)
        .and_then(|length| offset.checked_add(length))
        .ok_or(Error::ArrayRangeOverflow(array))?;
    let start = usize::try_from(offset).map_err(|_| Error::ArrayRangeOverflow(array))?;
    let end = usize::try_from(end).map_err(|_| Error::ArrayRangeOverflow(array))?;
    bytes.get(start..end).ok_or(Error::TruncatedArray(array))
}

fn chunk_bytes(payload: usize) -> Result<u32, Error> {
    // Source `wsize` is a signed 32-bit integer; the explicit caps fit it.
    let size = payload.checked_add(4).ok_or(Error::OutputSizeOverflow)?;
    i32::try_from(size)
        .map(|size| size as u32)
        .map_err(|_| Error::OutputSizeOverflow)
}

fn word(bytes: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(bytes[at..at + 4].try_into().unwrap())
}

fn vertex(bytes: &[u8; 12]) -> [f32; 3] {
    std::array::from_fn(|axis| {
        f32::from_le_bytes(bytes[4 * axis..4 * axis + 4].try_into().unwrap())
    })
}

fn put_word(output: &mut Vec<u8>, value: u32) {
    output.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests;
