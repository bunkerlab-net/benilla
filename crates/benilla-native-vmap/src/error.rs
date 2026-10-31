use std::fmt;

/// The only two M2 arrays consumed by this extractor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollisionArray {
    Indices,
    Vertices,
}

impl fmt::Display for CollisionArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Indices => "collision indices",
            Self::Vertices => "collision vertices",
        })
    }
}

/// A bounded extraction failure. No variant is a no-collision proof.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    InputLimit {
        bytes: usize,
        maximum: usize,
    },
    TruncatedHeader {
        bytes: usize,
    },
    UnsupportedMagic([u8; 4]),
    UnsupportedVersion(u32),
    CountLimit {
        array: CollisionArray,
        count: u32,
        maximum: u32,
    },
    IncompleteTriangle {
        index_count: u32,
    },
    ArrayBeforeHeader {
        array: CollisionArray,
        offset: u32,
    },
    ArrayRangeOverflow(CollisionArray),
    TruncatedArray(CollisionArray),
    IndexOutOfRange {
        ordinal: usize,
        index: u16,
        vertex_count: u32,
    },
    NonFiniteVertex {
        ordinal: usize,
        axis: usize,
    },
    OutputSizeOverflow,
    AllocationFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InputLimit { bytes, maximum } => {
                write!(f, "M2 input length {bytes} exceeds limit {maximum}")
            }
            Self::TruncatedHeader { bytes } => {
                write!(f, "M2 header is truncated at {bytes} bytes")
            }
            Self::UnsupportedMagic(_) => f.write_str("M2 magic is not MD20"),
            Self::UnsupportedVersion(version) => {
                write!(f, "M2 version {version} is outside the 256/257 profile")
            }
            Self::CountLimit {
                array,
                count,
                maximum,
            } => write!(f, "{array} count {count} exceeds limit {maximum}"),
            Self::IncompleteTriangle { index_count } => {
                write!(
                    f,
                    "collision index count {index_count} is not divisible by three"
                )
            }
            Self::ArrayBeforeHeader { array, offset } => {
                write!(f, "{array} offset {offset} overlaps the M2 header")
            }
            Self::ArrayRangeOverflow(array) => write!(f, "{array} byte range overflows"),
            Self::TruncatedArray(array) => write!(f, "{array} byte range is truncated"),
            Self::IndexOutOfRange {
                ordinal,
                index,
                vertex_count,
            } => write!(
                f,
                "collision index {ordinal} names vertex {index} outside count {vertex_count}"
            ),
            Self::NonFiniteVertex { ordinal, axis } => {
                write!(f, "collision vertex {ordinal} axis {axis} is non-finite")
            }
            Self::OutputSizeOverflow => f.write_str("raw VMap output size overflows"),
            Self::AllocationFailed => f.write_str("raw VMap output allocation failed"),
        }
    }
}

impl std::error::Error for Error {}
