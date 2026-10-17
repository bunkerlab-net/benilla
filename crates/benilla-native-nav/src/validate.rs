// SPDX-License-Identifier: GPL-3.0-or-later
//! Safe decoding of the pinned native layout before any native allocator/parser sees it.
use crate::{Error, TileInput};
use std::collections::{BTreeMap, BTreeSet};
pub(crate) const GRID: f32 = 533.3333;
pub(crate) const MAX_POLYS: usize = 4096;
const MAX_VERTS: usize = 65_534;
const MAX_LINKS: usize = 73_728;
const MAX_DETAIL: usize = 65_536;
pub(crate) struct Validated<'a> {
    pub params: &'a [u8],
    pub tiles: Vec<&'a [u8]>,
    pub origin: [f32; 3],
    pub agent: [f32; 3],
}
fn bad() -> Error {
    Error::Format
}
fn word(b: &[u8], i: usize) -> Result<u32, Error> {
    Ok(u32::from_le_bytes(
        b.get(i..i + 4).ok_or(Error::Framing)?.try_into().unwrap(),
    ))
}
fn short(b: &[u8], i: usize) -> Result<u16, Error> {
    Ok(u16::from_le_bytes(
        b.get(i..i + 2).ok_or(Error::Framing)?.try_into().unwrap(),
    ))
}
fn real(b: &[u8], i: usize) -> Result<f32, Error> {
    let f = f32::from_bits(word(b, i)?);
    if !f.is_finite() || f.abs() > 100_000. {
        return Err(bad());
    }
    Ok(f)
}
fn point(b: &[u8], i: usize) -> Result<[f32; 3], Error> {
    Ok([real(b, i)?, real(b, i + 4)?, real(b, i + 8)?])
}
fn section<'a>(b: &'a [u8], at: &mut usize, n: usize, width: usize) -> Result<&'a [u8], Error> {
    let end = at
        .checked_add(n.checked_mul(width).ok_or(Error::Budget)?)
        .ok_or(Error::Budget)?;
    let out = b.get(*at..end).ok_or(Error::Framing)?;
    *at = end;
    Ok(out)
}
fn count(b: &[u8], i: usize, max: usize) -> Result<usize, Error> {
    let n = word(b, i)? as usize;
    if n > max {
        return Err(Error::Budget);
    }
    Ok(n)
}
fn reserve<T>(n: usize) -> Result<Vec<T>, Error> {
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| Error::Budget)?;
    Ok(v)
}
fn cross(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f64 {
    (b[0] as f64 - a[0] as f64) * (c[2] as f64 - a[2] as f64)
        - (b[2] as f64 - a[2] as f64) * (c[0] as f64 - a[0] as f64)
}
#[derive(Clone)]
struct Poly {
    verts: Vec<usize>,
    neis: Vec<u16>,
    detail: Vec<[f32; 3]>,
}
struct TileWork {
    xy: [i32; 2],
    edges: usize,
    portals: [usize; 8],
}
const MAX_LOAD_WORK: usize = 1_048_576;
struct Portal {
    tile: [i32; 2],
    side: u16,
    a: [f32; 3],
    b: [f32; 3],
}
impl<'a> Validated<'a> {
    pub fn decode(params: &'a [u8], inputs: &'a [TileInput<'a>]) -> Result<Self, Error> {
        if params.len() != 28 {
            return Err(Error::Framing);
        }
        if inputs.is_empty() || inputs.len() > 64 {
            return Err(Error::Budget);
        }
        let origin = point(params, 0)?;
        if real(params, 12)?.to_bits() != GRID.to_bits()
            || real(params, 16)?.to_bits() != GRID.to_bits()
            || !(1..=4096).contains(&word(params, 20)?)
            || (word(params, 20)? as usize) < inputs.len()
            || word(params, 24)? != 0x8000_0000
        {
            return Err(bad());
        }
        let mut tiles = reserve(inputs.len())?;
        let mut locations = BTreeSet::new();
        let mut grids = BTreeSet::new();
        let mut total = 0;
        let mut total_bytes = 0;
        let mut agent = [f32::MAX; 3];
        let mut all_portals = Vec::new();
        let mut load_tiles = reserve(inputs.len())?;
        let mut total_detail = 0;
        for input in inputs {
            total_bytes += input.bytes.len();
            if total_bytes > 16 * 1024 * 1024 {
                return Err(Error::Budget);
            }
            if input.grid.iter().any(|v| *v >= 64) || !grids.insert(input.grid) {
                return Err(Error::Identity);
            }
            let b = input.bytes;
            if b.len() < 120 || word(b, 0)? != 0x4d4d4150 || word(b, 4)? != 7 || word(b, 8)? != 8 {
                return Err(bad());
            }
            if word(b, 12)? as usize != b.len() - 20 {
                return Err(Error::Framing);
            }
            // usesLiquids describes generator inclusion, not actual water presence. Actual
            // polygon areas/flags are checked below; either source boolean is admissible.
            if word(b, 16)? > 1 {
                return Err(Error::Unsupported);
            }
            let b = &b[20..];
            if word(b, 0)? != 0x444e4156 || word(b, 4)? != 7 || word(b, 16)? != 0 {
                return Err(Error::Unsupported);
            }
            let xy = [word(b, 8)? as i32, word(b, 12)? as i32];
            if xy.iter().any(|v| !(-128..=128).contains(v)) || !locations.insert(xy) {
                return Err(Error::Identity);
            }
            let n = count(b, 24, MAX_POLYS)?;
            let nv = count(b, 28, MAX_VERTS)?;
            total += n;
            if n == 0 || total > MAX_POLYS || nv < 3 {
                return Err(Error::Budget);
            }
            let nl = count(b, 32, MAX_LINKS)?;
            let nd = count(b, 36, MAX_POLYS)?;
            let ndv = count(b, 40, MAX_DETAIL)?;
            let ndt = count(b, 44, MAX_DETAIL)?;
            let nb = count(b, 48, MAX_POLYS * 2)?;
            total_detail += ndv + ndt;
            if total_detail > MAX_DETAIL {
                return Err(Error::Budget);
            }
            if nl == 0 || nd != n || word(b, 52)? != 0 || word(b, 56)? as usize != n {
                return Err(Error::Unsupported);
            }
            let dimensions = point(b, 60)?;
            if dimensions[0] <= 0.
                || dimensions[1] <= 0.
                || dimensions[2] < 0.
                || dimensions.iter().any(|v| *v > 100.)
            {
                return Err(bad());
            }
            for i in 0..3 {
                agent[i] = agent[i].min(dimensions[i]);
            }
            let min = point(b, 72)?;
            let max = point(b, 84)?;
            let q = real(b, 96)?;
            if q <= 0. || (0..3).any(|i| min[i] > max[i] || q * (max[i] - min[i]) + 1. > 65535.) {
                return Err(bad());
            }
            // Source getTileBounds uses (max - GRID), not (31-grid)*GRID.
            for (axis, grid) in [(0, input.grid[1]), (2, input.grid[0])] {
                let hi = (32 - i32::from(grid)) as f32 * GRID;
                let lo = hi - GRID;
                if min[axis].to_bits() != lo.to_bits() || max[axis].to_bits() != hi.to_bits() {
                    return Err(Error::Identity);
                }
            }
            for (i, axis) in [0, 2].into_iter().enumerate() {
                let source = ((min[axis] + max[axis]) / 2. - origin[axis]) / GRID;
                if source as i32 != xy[i] {
                    return Err(Error::Identity);
                }
            }
            let mut at = 100;
            let vs = section(b, &mut at, nv, 12)?;
            let ps = section(b, &mut at, n, 32)?;
            let links = section(b, &mut at, nl, 16)?;
            let details = section(b, &mut at, n, 12)?;
            let dvs = section(b, &mut at, ndv, 12)?;
            let tris = section(b, &mut at, ndt, 4)?;
            let bv = section(b, &mut at, nb, 16)?;
            if at != b.len() {
                return Err(Error::Framing);
            }
            if links.iter().any(|v| *v != 0) {
                return Err(bad());
            } // freshly serialized dynamic slots
            let mut vertices = reserve(nv)?;
            let mut dvertices = reserve(ndv)?;
            for (bytes, count, out) in [(vs, nv, &mut vertices), (dvs, ndv, &mut dvertices)] {
                for j in 0..count {
                    let p = point(bytes, j * 12)?;
                    if (0..3).any(|i| p[i] < min[i] || p[i] > max[i]) {
                        return Err(bad());
                    }
                    out.push(p);
                }
            }
            let mut polys = reserve(n)?;
            let mut edge_count = 0;
            let mut portals = 0;
            let mut portal_sides = [0usize; 8];
            let mut next_dv = 0;
            let mut next_tri = 0;
            for pi in 0..n {
                let p = &ps[pi * 32..(pi + 1) * 32];
                let nv = p[30] as usize;
                if !(3..=6).contains(&nv) || word(p, 0)? != 0 {
                    return Err(bad());
                }
                if short(p, 28)? != 1 || p[31] != 11 {
                    return Err(Error::Unsupported);
                } // dry CMaNGOS ground
                let mut poly = Poly {
                    verts: reserve(nv)?,
                    neis: reserve(nv)?,
                    detail: Vec::new(),
                };
                for i in 0..6 {
                    let v = short(p, 4 + 2 * i)? as usize;
                    let nei = short(p, 16 + 2 * i)?;
                    if i >= nv {
                        if v != 0 || nei != 0 {
                            return Err(bad());
                        }
                        continue;
                    }
                    if v >= vertices.len() || poly.verts.contains(&v) {
                        return Err(bad());
                    }
                    if nei & 0x8000 != 0 {
                        if ![0x8000, 0x8002, 0x8004, 0x8006].contains(&nei) {
                            return Err(Error::Unsupported);
                        }
                        portals += 1;
                        portal_sides[(nei & 7) as usize] += 1;
                    } else if nei as usize > n || nei as usize == pi + 1 {
                        return Err(bad());
                    }
                    poly.verts.push(v);
                    poly.neis.push(nei);
                    edge_count += 1;
                }
                // Strict convex projected polygons, with source clockwise winding.
                for i in 0..nv {
                    let a = vertices[poly.verts[i]];
                    let b = vertices[poly.verts[(i + 1) % nv]];
                    for j in 0..nv {
                        if j != i && j != (i + 1) % nv && cross(a, b, vertices[poly.verts[j]]) >= 0.
                        {
                            return Err(Error::Unsupported);
                        }
                    }
                    if poly.neis[i] & 0x8000 != 0 {
                        let side = poly.neis[i] & 7;
                        let axis = if side == 0 || side == 4 { 0 } else { 2 };
                        let along = 2 - axis;
                        let boundary = if side == 0 || side == 2 {
                            max[axis]
                        } else {
                            min[axis]
                        };
                        if a[axis] != boundary || b[axis] != boundary || a[along] == b[along] {
                            return Err(bad());
                        }
                        all_portals.try_reserve(1).map_err(|_| Error::Budget)?;
                        all_portals.push(Portal {
                            tile: xy,
                            side,
                            a,
                            b,
                        });
                    }
                }
                let d = &details[pi * 12..pi * 12 + 12];
                let base = word(d, 0)? as usize;
                let tb = word(d, 4)? as usize;
                let dn = d[8] as usize;
                let tn = d[9] as usize;
                if base != next_dv
                    || tb != next_tri
                    || base + dn > ndv
                    || tb + tn > ndt
                    || tn == 0
                    || d[10..] != [0, 0]
                {
                    return Err(bad());
                }
                next_dv += dn;
                next_tri += tn;
                poly.detail = reserve(nv + dn)?;
                for &v in &poly.verts {
                    poly.detail.push(vertices[v]);
                }
                poly.detail.extend_from_slice(&dvertices[base..base + dn]);
                for &v in &poly.detail {
                    for i in 0..nv {
                        if cross(
                            vertices[poly.verts[i]],
                            vertices[poly.verts[(i + 1) % nv]],
                            v,
                        ) > 0.
                        {
                            return Err(bad());
                        }
                    }
                }
                let mut boundary = false;
                let mut edges = BTreeMap::new();
                for t in tris[tb * 4..(tb + tn) * 4].as_chunks::<4>().0 {
                    if t[..3].iter().any(|v| *v as usize >= poly.detail.len()) || t[3] & !0x15 != 0
                    {
                        return Err(bad());
                    }
                    let a = poly.detail[t[0] as usize];
                    let b = poly.detail[t[1] as usize];
                    let c = poly.detail[t[2] as usize];
                    // Match native binary32 degeneracy threshold. Rejection is not a
                    // geometric tolerance granting support; these shapes are unsupported.
                    let area = (c[0] - a[0]) * (b[2] - a[2]) - (c[2] - a[2]) * (b[0] - a[0]);
                    if cross(a, b, c) >= 0. || !area.is_finite() || area < 1e-6 {
                        return Err(Error::Unsupported);
                    }
                    for k in 0..3 {
                        let a = t[k] as usize;
                        let b = t[(k + 1) % 3] as usize;
                        if a == b || edges.insert((a, b), t[3] & (1 << (k * 2)) != 0).is_some() {
                            return Err(bad());
                        }
                    }
                    boundary |= t[3] != 0;
                }
                if !boundary {
                    return Err(bad());
                } // native closestPointOnDetailEdges must select an edge
                validate_detail_boundary(&poly, &edges)?;
                polys.push(poly);
            }
            if next_dv != ndv || next_tri != ndt || nl != edge_count + portals * 2 {
                return Err(bad());
            }
            // Internal adjacency must agree on the same oppositely oriented shared edge.
            for (pi, p) in polys.iter().enumerate() {
                for i in 0..p.verts.len() {
                    let nei = p.neis[i];
                    if nei == 0 || nei & 0x8000 != 0 {
                        continue;
                    }
                    let other = &polys[nei as usize - 1];
                    let a = p.verts[i];
                    let b = p.verts[(i + 1) % p.verts.len()];
                    if !(0..other.verts.len()).any(|j| {
                        other.verts[j] == b
                            && other.verts[(j + 1) % other.verts.len()] == a
                            && other.neis[j] as usize == pi + 1
                    }) {
                        return Err(bad());
                    }
                }
            }
            validate_bv(bv, &polys, min, q)?;
            load_tiles.push(TileWork {
                xy,
                edges: edge_count,
                portals: portal_sides,
            });
            tiles.push(b);
        }
        native_load_work(&load_tiles)?;
        // Restrict loaded cross-tile topology to exact one-to-one dry portals.
        // This prevents source four-match truncation and link-pool exhaustion;
        // absent adjacent tiles stay absent, never invented as closed-world coverage.
        let mut work = 0usize;
        for a in &all_portals {
            let delta = match a.side {
                0 => [1, 0],
                2 => [0, 1],
                4 => [-1, 0],
                6 => [0, -1],
                _ => unreachable!(),
            };
            let target = [a.tile[0] + delta[0], a.tile[1] + delta[1]];
            let mut matches = 0;
            for b in &all_portals {
                work += 1;
                if work > 1_048_576 {
                    return Err(Error::Budget);
                }
                if b.tile != target || b.side != (a.side + 4) % 8 {
                    continue;
                }
                let along = if a.side == 0 || a.side == 4 { 2 } else { 0 };
                let lo = a.a[along].min(a.b[along]).max(b.a[along].min(b.b[along]));
                let hi = a.a[along].max(a.b[along]).min(b.a[along].max(b.b[along]));
                if hi <= lo {
                    continue;
                }
                if a.a != b.b || a.b != b.a {
                    return Err(Error::Unsupported);
                }
                matches += 1;
                if matches > 1 {
                    return Err(Error::Unsupported);
                }
            }
        }
        Ok(Self {
            params,
            tiles,
            origin,
            agent,
        })
    }
}
fn validate_bv(b: &[u8], polys: &[Poly], origin: [f32; 3], q: f32) -> Result<(), Error> {
    if b.is_empty() {
        return Err(Error::Unsupported);
    } // source generator builds BV trees
      // Pinned builder reserves 2*n, writes a 2*n-1-node tree, leaves a zero sentinel.
    if b.len() != polys.len() * 2 * 16 || b[b.len() - 16..].iter().any(|v| *v != 0) {
        return Err(bad());
    }
    let count = b.len() / 16 - 1;
    let mut seen = vec![false; polys.len()];
    fn node(
        b: &[u8],
        at: usize,
        end: usize,
        depth: usize,
        polys: &[Poly],
        seen: &mut [bool],
        origin: [f32; 3],
        q: f32,
    ) -> Result<([u16; 3], [u16; 3], usize), Error> {
        if at >= end || depth > 32 {
            return Err(bad());
        }
        let data = &b[at * 16..at * 16 + 16];
        let lo = [short(data, 0)?, short(data, 2)?, short(data, 4)?];
        let hi = [short(data, 6)?, short(data, 8)?, short(data, 10)?];
        if (0..3).any(|i| lo[i] > hi[i]) {
            return Err(bad());
        }
        let reference = word(data, 12)? as i32;
        if reference >= 0 {
            let p = reference as usize;
            if p >= polys.len() || seen[p] {
                return Err(bad());
            }
            seen[p] = true;
            for v in &polys[p].detail {
                for i in 0..3 {
                    let quant = ((v[i] - origin[i]) * q) as u16;
                    if quant < lo[i] || quant > hi[i] {
                        return Err(bad());
                    }
                }
            }
            Ok((lo, hi, at + 1))
        } else {
            let skip = reference.checked_neg().ok_or(Error::Format)? as usize;
            let stop = at.checked_add(skip).ok_or(Error::Budget)?;
            if skip < 3 || stop > end {
                return Err(bad());
            }
            let (ll, lh, next) = node(b, at + 1, stop, depth + 1, polys, seen, origin, q)?;
            let (rl, rh, next) = node(b, next, stop, depth + 1, polys, seen, origin, q)?;
            if next != stop
                || (0..3).any(|i| ll[i] < lo[i] || rl[i] < lo[i] || lh[i] > hi[i] || rh[i] > hi[i])
            {
                return Err(bad());
            }
            Ok((lo, hi, stop))
        }
    }
    let (_, _, end) = node(b, 0, count, 0, polys, &mut seen, origin, q)?;
    if end != count || seen.contains(&false) {
        return Err(bad());
    }
    Ok(())
}

// Directed detail triangles must form exactly the polygon boundary. Every interior
// edge has one reverse; every boundary edge is genuinely on a polygon edge, covers
// it without gaps/overlap, and carries the source boundary bit. This excludes null
// native fallback pointers and malformed surfaces masquerading as a complete mesh.
fn validate_detail_boundary(
    poly: &Poly,
    edges: &BTreeMap<(usize, usize), bool>,
) -> Result<(), Error> {
    let n = poly.verts.len();
    let mut coverage: Vec<Vec<(f64, f64)>> = reserve(n)?;
    coverage.resize_with(n, Vec::new);
    for (&(a, b), &flag) in edges {
        if let Some(reverse) = edges.get(&(b, a)) {
            if flag || *reverse {
                return Err(bad());
            }
            continue;
        }
        if !flag {
            return Err(bad());
        }
        let from = poly.detail[a];
        let to = poly.detail[b];
        let mut found = false;
        for (i, intervals) in coverage.iter_mut().enumerate() {
            let start = poly.detail[i];
            let end = poly.detail[(i + 1) % n];
            if cross(start, end, from) != 0. || cross(start, end, to) != 0. {
                continue;
            }
            let axis = if (end[0] - start[0]).abs() >= (end[2] - start[2]).abs() {
                0
            } else {
                2
            };
            let denom = end[axis] as f64 - start[axis] as f64;
            if denom == 0. {
                return Err(bad());
            }
            let a = (from[axis] as f64 - start[axis] as f64) / denom;
            let b = (to[axis] as f64 - start[axis] as f64) / denom;
            if !(0. ..=1.).contains(&a) || !(0. ..=1.).contains(&b) || a >= b {
                continue;
            }
            intervals.try_reserve(1).map_err(|_| Error::Budget)?;
            intervals.push((a, b));
            found = true;
            break;
        }
        if !found {
            return Err(bad());
        }
    }
    for edges in &mut coverage {
        edges.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut end = 0.;
        for &(a, b) in edges.iter() {
            if a != end {
                return Err(bad());
            }
            end = b;
        }
        if end != 1. {
            return Err(bad());
        }
    }
    Ok(())
}

// addTile connects every neighboring tile in both directions. Before native init,
// bound source-polygon scans plus findConnectingPolys' nested target-edge scans.
fn native_load_work(tiles: &[TileWork]) -> Result<usize, Error> {
    let mut work = 0usize;
    for a in tiles {
        work = work.checked_add(a.edges).ok_or(Error::Budget)?;
        for b in tiles {
            let delta = [b.xy[0] - a.xy[0], b.xy[1] - a.xy[1]];
            let side = match delta {
                [1, 0] => 0,
                [1, 1] => 1,
                [0, 1] => 2,
                [-1, 1] => 3,
                [-1, 0] => 4,
                [-1, -1] => 5,
                [0, -1] => 6,
                [1, -1] => 7,
                _ => continue,
            };
            let scans = a.portals[side]
                .checked_mul(b.edges)
                .and_then(|n| n.checked_add(a.edges))
                .ok_or(Error::Budget)?;
            work = work.checked_add(scans).ok_or(Error::Budget)?;
        }
        if work > MAX_LOAD_WORK {
            return Err(Error::Budget);
        }
    }
    Ok(work)
}
#[cfg(test)]
mod work_tests {
    use super::*;
    #[test]
    fn sparse_small_bytes_do_not_hide_quadratic_native_portal_scans() {
        let a = TileWork {
            xy: [0, 0],
            edges: 10_000,
            portals: [100, 0, 0, 0, 0, 0, 0, 0],
        };
        let b = TileWork {
            xy: [1, 0],
            edges: 10_000,
            portals: [0, 0, 0, 0, 100, 0, 0, 0],
        };
        assert_eq!(native_load_work(&[a, b]), Err(Error::Budget));
        let a = TileWork {
            xy: [0, 0],
            edges: 16,
            portals: [1, 0, 0, 0, 0, 0, 0, 0],
        };
        let b = TileWork {
            xy: [1, 0],
            edges: 16,
            portals: [0, 0, 0, 0, 1, 0, 0, 0],
        };
        assert_eq!(native_load_work(&[a, b]), Ok(96));
    }
}
