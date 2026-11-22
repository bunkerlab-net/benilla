// SPDX-License-Identifier: GPL-3.0-or-later
//! Structural safety for live addTile, not the bounded query-product contract.
use super::{point, real, section, short, word};
use crate::{live::LiveError, Error, TileInput};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndefinedTile {
    ZeroMaxLinks,
    PolygonVertexCount,
    CoarseVertexIndex,
    InternalPolygonIndex,
    DetailRange,
    DetailVertexIndex,
    NonCardinalPortal,
    MissingDetailBoundary,
    BvIndex,
    BvEscape,
    OffMeshPolygonIndex,
}
impl From<UndefinedTile> for LiveError {
    fn from(value: UndefinedTile) -> Self {
        Self::UndefinedTile(value)
    }
}

pub(crate) fn tile(input: TileInput<'_>) -> Result<(), LiveError> {
    let bytes = input.bytes;
    if bytes.len() < 120 {
        return Err(Error::Framing.into());
    }
    if bytes.len() > 16 * 1024 * 1024 {
        return Err(Error::Budget.into());
    }
    // MoveMap.cpp:235-267 ignores outer dtVersion and usesLiquids.
    if word(bytes, 0)? != 0x4d4d4150 || word(bytes, 8)? != 8 {
        return Err(Error::Format.into());
    }
    if word(bytes, 12)? as usize != bytes.len() - 20 {
        return Err(Error::Framing.into());
    }
    if input.grid.iter().any(|v| *v >= 64) {
        return Err(Error::Identity.into());
    }
    let b = &bytes[20..];
    // Retain the existing coordinate profile before native neighbour increments.
    if [word(b, 8)? as i32, word(b, 12)? as i32]
        .iter()
        .any(|v| !(-128..=128).contains(v))
    {
        return Err(Error::Identity.into());
    }
    let n = word(b, 24)? as usize;
    let nv = word(b, 28)? as usize;
    let nl = word(b, 32)? as usize;
    let nd = word(b, 36)? as usize;
    let ndv = word(b, 40)? as usize;
    let ndt = word(b, 44)? as usize;
    let nb = word(b, 48)? as usize;
    let nc = word(b, 52)? as usize;
    let offmesh_base = word(b, 56)?;
    if nl == 0 {
        // DetourNavMesh.cpp:1004 writes links[maxLinkCount - 1].
        return Err(UndefinedTile::ZeroMaxLinks.into());
    }
    // Section bounds also bound every native size calculation before addTile.
    let mut at = 100;
    let vertices = section(b, &mut at, nv, 12)?;
    let polys = section(b, &mut at, n, 32)?;
    let _links = section(b, &mut at, nl, 16)?;
    let details = section(b, &mut at, nd, 12)?;
    let dvertices = section(b, &mut at, ndv, 12)?;
    let triangles = section(b, &mut at, ndt, 4)?;
    let bv = section(b, &mut at, nb, 16)?;
    let connections = section(b, &mut at, nc, 36)?;
    if at != b.len() {
        return Err(Error::Framing.into());
    }
    let dimensions = point(b, 60)?;
    if dimensions[0] <= 0.
        || dimensions[1] <= 0.
        || dimensions[2] < 0.
        || dimensions.iter().any(|v| *v > 100.)
    {
        return Err(Error::Format.into());
    }
    let min = point(b, 72)?;
    let max = point(b, 84)?;
    let q = real(b, 96)?;
    // DetourNavMesh.cpp:829-834 converts the clamped header range to uint16.
    if q <= 0. || (0..3).any(|i| min[i] > max[i] || q * (max[i] - min[i]) + 1. > 65535.) {
        return Err(Error::Format.into());
    }
    for data in [vertices, dvertices] {
        for at in (0..data.len()).step_by(12) {
            // Keep the existing finite numeric envelope, not geometric enclosure.
            point(data, at)?;
        }
    }
    for (pi, p) in polys.as_chunks::<32>().0.iter().enumerate() {
        let count = usize::from(p[30]);
        let offmesh = p[31] >> 6 == 1;
        if count > 6 {
            // Fixed arrays at DetourNavMesh.cpp:540-543,687-690.
            return Err(UndefinedTile::PolygonVertexCount.into());
        }
        if if offmesh { count != 2 } else { count < 3 } {
            return Err(Error::Unsupported.into());
        }
        for j in 0..count {
            if usize::from(short(p, 4 + j * 2)?) >= nv {
                return Err(UndefinedTile::CoarseVertexIndex.into());
            }
            let nei = short(p, 16 + j * 2)?;
            if nei & 0x8000 != 0 {
                // calcSlabEndPoints only initializes cardinal sides, :76-112.
                if ![0, 2, 4, 6].contains(&(nei & 0xff)) {
                    return Err(UndefinedTile::NonCardinalPortal.into());
                }
            } else if usize::from(nei) > n {
                return Err(UndefinedTile::InternalPolygonIndex.into());
            }
        }
        if offmesh {
            // getOffMeshConnectionByRef :1521-1523 indexes from offMeshBase.
            if (pi as u32).wrapping_sub(offmesh_base) as usize >= nc {
                return Err(UndefinedTile::OffMeshPolygonIndex.into());
            }
            continue;
        }
        if pi >= nd {
            return Err(UndefinedTile::DetailRange.into());
        }
        let d = &details[pi * 12..pi * 12 + 12];
        let base = word(d, 0)? as usize;
        let first = word(d, 4)? as usize;
        let count_extra = usize::from(d[8]);
        let count_tri = usize::from(d[9]);
        if base + count_extra > ndv || first + count_tri > ndt {
            return Err(UndefinedTile::DetailRange.into());
        }
        let mut boundary = false;
        for t in triangles[first * 4..(first + count_tri) * 4]
            .as_chunks::<4>()
            .0
        {
            if t[..3]
                .iter()
                .any(|&v| usize::from(v) >= count + count_extra)
            {
                // Native coarse/detail indexing at :646-648,705-708.
                return Err(UndefinedTile::DetailVertexIndex.into());
            }
            boundary |= t[3] & 0x15 != 0;
        }
        if !boundary {
            // closestPointOnDetailEdges<true> dereferences null at :673.
            return Err(UndefinedTile::MissingDetailBoundary.into());
        }
    }
    for (i, node) in bv.as_chunks::<16>().0.iter().enumerate() {
        let reference = word(node, 12)? as i32;
        if reference >= 0 {
            if reference as usize >= n {
                return Err(UndefinedTile::BvIndex.into());
            }
        } else if reference == i32::MIN || i + (-reference) as usize > nb {
            // queryPolygonsInTile :854-855 must not form an out-of-array pointer.
            return Err(UndefinedTile::BvEscape.into());
        }
    }
    for con in connections.as_chunks::<36>().0 {
        point(con, 0)?;
        point(con, 12)?;
        if real(con, 24)? < 0. {
            return Err(Error::Format.into());
        }
        let pi = usize::from(short(con, 28)?);
        if pi >= n {
            return Err(UndefinedTile::OffMeshPolygonIndex.into());
        }
        if polys[pi * 32 + 31] >> 6 != 1 {
            return Err(Error::Format.into());
        }
    }
    // DetourNavMesh.cpp:760-805 uses squared nearest distances for offmesh links.
    // Topology, all portal matches and link exhaustion belong to pinned addTile.
    Ok(())
}
