use std::fmt;

/// Which complete WMO byte input failed validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WmoFile {
    Root,
    Group(u32),
}

impl fmt::Display for WmoFile {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root => f.write_str("WMO root"),
            Self::Group(index) => write!(f, "WMO group {index}"),
        }
    }
}

/// A malformed, unsupported, or resource-bounded WMO input. No error proves
/// collision absence or authorizes use of a partial set of groups.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WmoError {
    InvalidSourceRootMember,
    InvalidLiquidDimensions {
        group: u32,
        vertices: [i32; 2],
        tiles: [i32; 2],
    },
    LiquidSizeOverflow {
        group: u32,
    },
    NonFiniteLiquidHeight {
        group: u32,
        ordinal: usize,
    },
    InputLimit {
        file: WmoFile,
        bytes: usize,
        maximum: usize,
    },
    TotalInputLimit {
        maximum: usize,
    },
    GroupCountLimit {
        count: u32,
        maximum: u32,
    },
    GroupCountMismatch {
        declared: u32,
        supplied: usize,
    },
    GroupOrder {
        expected: u32,
        actual: u32,
    },
    ChunkLimit {
        file: WmoFile,
        maximum: usize,
    },
    TruncatedChunkHeader {
        file: WmoFile,
        offset: usize,
    },
    TruncatedChunk {
        file: WmoFile,
        chunk: [u8; 4],
        offset: usize,
        bytes: u32,
    },
    MissingChunk {
        file: WmoFile,
        chunk: [u8; 4],
    },
    DuplicateChunk {
        file: WmoFile,
        chunk: [u8; 4],
    },
    UnexpectedChunk {
        file: WmoFile,
        chunk: [u8; 4],
    },
    InvalidChunkSize {
        file: WmoFile,
        chunk: [u8; 4],
        bytes: usize,
    },
    UnsupportedVersion {
        file: WmoFile,
        version: u32,
    },
    CountLimit {
        file: WmoFile,
        chunk: [u8; 4],
        count: usize,
        maximum: usize,
    },
    TriangleIndexCount {
        group: u32,
        triangles: usize,
        indices: usize,
    },
    IndexOutOfRange {
        group: u32,
        ordinal: usize,
        index: u16,
        vertex_count: usize,
    },
    NonFinite {
        file: WmoFile,
        chunk: [u8; 4],
        ordinal: usize,
        axis: usize,
    },
    ReversedBounds {
        file: WmoFile,
        axis: usize,
    },
    NegativeGroupName {
        group: u32,
        offset: i32,
    },
    UnterminatedGroupName {
        group: u32,
        offset: usize,
    },
    UnsupportedLiquid {
        group: u32,
    },
    OutputLimit {
        maximum: usize,
    },
    AllocationFailed,
}

impl fmt::Display for WmoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidSourceRootMember => f.write_str("WMO source root member is outside the bounded ASCII archive-path profile"),
            Self::InvalidLiquidDimensions { group, vertices, tiles } => write!(f, "WMO group {group} has incompatible liquid dimensions {vertices:?} vertices and {tiles:?} tiles"),
            Self::LiquidSizeOverflow { group } => write!(f, "WMO group {group} liquid dimensions exceed source arithmetic bounds"),
            Self::NonFiniteLiquidHeight { group, ordinal } => write!(f, "WMO group {group} liquid height {ordinal} is non-finite"),
            Self::InputLimit { file, bytes, maximum } => write!(f, "{file} input length {bytes} exceeds limit {maximum}"),
            Self::TotalInputLimit { maximum } => write!(f, "total WMO input length exceeds limit {maximum}"),
            Self::GroupCountLimit { count, maximum } => write!(f, "WMO group count {count} exceeds limit {maximum}"),
            Self::GroupCountMismatch { declared, supplied } => write!(f, "WMO declares {declared} groups but {supplied} were supplied"),
            Self::GroupOrder { expected, actual } => write!(f, "WMO group {actual} supplied where source group {expected} is required"),
            Self::ChunkLimit { file, maximum } => write!(f, "{file} chunk count exceeds limit {maximum}"),
            Self::TruncatedChunkHeader { file, offset } => write!(f, "{file} chunk header at {offset} is truncated"),
            Self::TruncatedChunk { file, chunk, offset, bytes } => write!(f, "{file} chunk {chunk:?} at {offset} has truncated {bytes}-byte payload"),
            Self::MissingChunk { file, chunk } => write!(f, "{file} is missing required chunk {chunk:?}"),
            Self::DuplicateChunk { file, chunk } => write!(f, "{file} repeats singleton chunk {chunk:?}"),
            Self::UnexpectedChunk { file, chunk } => write!(f, "{file} chunk {chunk:?} is in an invalid location"),
            Self::InvalidChunkSize { file, chunk, bytes } => write!(f, "{file} chunk {chunk:?} has invalid length {bytes}"),
            Self::UnsupportedVersion { file, version } => write!(f, "{file} version {version} is outside the version 17 profile"),
            Self::CountLimit { file, chunk, count, maximum } => write!(f, "{file} chunk {chunk:?} count {count} exceeds limit {maximum}"),
            Self::TriangleIndexCount { group, triangles, indices } => write!(f, "WMO group {group} has {triangles} MOPY triangles but {indices} MOVI indices"),
            Self::IndexOutOfRange { group, ordinal, index, vertex_count } => write!(f, "WMO group {group} index {ordinal} names vertex {index} outside count {vertex_count}"),
            Self::NonFinite { file, chunk, ordinal, axis } => write!(f, "{file} chunk {chunk:?} vector {ordinal} axis {axis} is non-finite"),
            Self::ReversedBounds { file, axis } => write!(f, "{file} bounding box is reversed on axis {axis}"),
            Self::NegativeGroupName { group, offset } => write!(f, "WMO group {group} has unsafe negative group-name offset {offset}"),
            Self::UnterminatedGroupName { group, offset } => write!(f, "WMO group {group} name at {offset} is not terminated in MOGN"),
            Self::UnsupportedLiquid { group } => write!(f, "WMO group {group} contains MLIQ, outside the dry raw-geometry profile"),
            Self::OutputLimit { maximum } => write!(f, "raw WMO output exceeds limit {maximum}"),
            Self::AllocationFailed => f.write_str("WMO extraction allocation failed"),
        }
    }
}

impl std::error::Error for WmoError {}
