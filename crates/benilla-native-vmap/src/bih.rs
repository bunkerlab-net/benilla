// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded port of pinned CMaNGOS BIH::build/subdivide/writeToFile.
use std::fmt;
pub type BihBounds = [[f32; 3]; 2];
pub const MAX_BIH_PRIMITIVES: usize = 1_048_576;
pub const MAX_BIH_WORDS: usize = 12_582_912;
pub const MAX_BIH_WORK: usize = 67_108_864;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BihBuildError {
    Cancelled,
    Limit,
    Allocation,
    Bounds,
    Numeric,
}
impl fmt::Display for BihBuildError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Cancelled => "vmap.bih_cancelled",
            Self::Limit => "vmap.bih_limit",
            Self::Allocation => "vmap.bih_allocation",
            Self::Bounds => "vmap.bih_bounds",
            Self::Numeric => "vmap.bih_numeric",
        })
    }
}
impl std::error::Error for BihBuildError {}
#[derive(Debug)]
pub struct SourceBih {
    bounds: BihBounds,
    tree: Vec<u32>,
    objects: Vec<u32>,
}
impl SourceBih {
    pub fn bounds(&self) -> BihBounds {
        self.bounds
    }
    pub fn tree_words(&self) -> &[u32] {
        &self.tree
    }
    pub fn objects(&self) -> &[u32] {
        &self.objects
    }
    pub fn build(
        input: &[BihBounds],
        leaf_size: u32,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, BihBuildError> {
        if input.len() > MAX_BIH_PRIMITIVES
            || leaf_size == 0
            || leaf_size > MAX_BIH_PRIMITIVES as u32
        {
            return Err(BihBuildError::Limit);
        }
        check(&cancelled)?;
        let mut objects = Vec::new();
        objects
            .try_reserve_exact(input.len())
            .map_err(|_| BihBuildError::Allocation)?;
        let mut bounds = input.first().copied().unwrap_or([[0.0; 3]; 2]);
        for (i, b) in input.iter().enumerate() {
            if i % 1024 == 0 {
                check(&cancelled)?;
            }
            for axis in 0..3 {
                if !b[0][axis].is_finite() || !b[1][axis].is_finite() || b[0][axis] > b[1][axis] {
                    return Err(BihBuildError::Bounds);
                };
                // G3D bounds.merge uses new first in std::min/max, including equal zeros.
                bounds[0][axis] = if bounds[0][axis] < b[0][axis] {
                    bounds[0][axis]
                } else {
                    b[0][axis]
                };
                bounds[1][axis] = if b[1][axis] < bounds[1][axis] {
                    bounds[1][axis]
                } else {
                    b[1][axis]
                };
            }
            objects.push(i as u32);
        }
        let mut builder = Builder {
            input,
            objects,
            tree: Vec::new(),
            leaf: leaf_size as isize,
            work: 0,
            cancelled: &cancelled,
        };
        builder.allocate_node()?;
        builder.tree[0] = 3 << 30;
        if !input.is_empty() {
            builder.subdivide(0, input.len() as isize - 1, bounds, bounds, 0, 1)?;
        }
        check(&cancelled)?;
        Ok(Self {
            bounds,
            tree: builder.tree,
            objects: builder.objects,
        })
    }
    pub fn serialize(&self, cancelled: impl Fn() -> bool) -> Result<Vec<u8>, BihBuildError> {
        let count = 32usize
            .checked_add(
                self.tree
                    .len()
                    .checked_add(self.objects.len())
                    .and_then(|n| n.checked_mul(4))
                    .ok_or(BihBuildError::Limit)?,
            )
            .ok_or(BihBuildError::Limit)?;
        check(&cancelled)?;
        let mut out = Vec::new();
        out.try_reserve_exact(count)
            .map_err(|_| BihBuildError::Allocation)?;
        for value in self.bounds.into_iter().flatten() {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out.extend_from_slice(&(self.tree.len() as u32).to_le_bytes());
        for (i, w) in self.tree.iter().enumerate() {
            if i % 1024 == 0 {
                check(&cancelled)?;
            }
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(&(self.objects.len() as u32).to_le_bytes());
        for (i, w) in self.objects.iter().enumerate() {
            if i % 1024 == 0 {
                check(&cancelled)?;
            }
            out.extend_from_slice(&w.to_le_bytes());
        }
        check(&cancelled)?;
        Ok(out)
    }
}
fn check(c: &dyn Fn() -> bool) -> Result<(), BihBuildError> {
    if c() {
        Err(BihBuildError::Cancelled)
    } else {
        Ok(())
    }
}
fn finite(x: f32) -> Result<f32, BihBuildError> {
    if x.is_finite() {
        Ok(x)
    } else {
        Err(BihBuildError::Numeric)
    }
}
fn primary(d: [f32; 3]) -> usize {
    if d[0] > d[1] {
        if d[0] > d[2] {
            0
        } else {
            2
        }
    } else if d[1] > d[2] {
        1
    } else {
        2
    }
}
fn fuzzy(a: f32, b: f32) -> bool {
    let a = f64::from(a);
    let b = f64::from(b);
    a == b || (a - b).abs() <= f64::from(0.00001_f32) * (a.abs() + 1.0)
}
fn node_end(words: usize) -> Result<usize, BihBuildError> {
    words
        .checked_add(3)
        .filter(|n| *n <= MAX_BIH_WORDS)
        .ok_or(BihBuildError::Limit)
}
struct Builder<'a> {
    input: &'a [BihBounds],
    objects: Vec<u32>,
    tree: Vec<u32>,
    leaf: isize,
    work: usize,
    cancelled: &'a dyn Fn() -> bool,
}
impl Builder<'_> {
    fn charge(&mut self) -> Result<(), BihBuildError> {
        if self.work >= MAX_BIH_WORK {
            return Err(BihBuildError::Limit);
        };
        self.work += 1;
        if self.work % 1024 == 1 {
            check(self.cancelled)?;
        }
        Ok(())
    }
    fn allocate_node(&mut self) -> Result<usize, BihBuildError> {
        let n = self.tree.len();
        let required = node_end(n)?;
        if required > self.tree.capacity() {
            let target = self
                .tree
                .capacity()
                .saturating_mul(2)
                .max(required)
                .min(MAX_BIH_WORDS);
            self.tree
                .try_reserve_exact(target - n)
                .map_err(|_| BihBuildError::Allocation)?;
        }
        self.tree.extend_from_slice(&[0; 3]);
        Ok(n)
    }
    fn leaf(&mut self, node: usize, left: isize, right: isize) {
        self.tree[node] = (3 << 30) | (left as u32);
        self.tree[node + 1] = (right - left + 1) as u32;
    }
    #[allow(clippy::too_many_arguments)]
    fn subdivide(
        &mut self,
        left: isize,
        mut right: isize,
        mut grid: BihBounds,
        mut node_box: BihBounds,
        mut node: usize,
        mut depth: u32,
    ) -> Result<(), BihBuildError> {
        self.charge()?;
        if right - left < self.leaf || depth >= 64 {
            self.leaf(node, left, right);
            return Ok(());
        }
        let mut axis = None;
        let mut split = f32::NAN;
        let mut previous_clip = f32::NAN;
        let mut was_left = true;
        let (clip_l, clip_r, right_original, chosen_axis) = loop {
            self.charge()?;
            let previous_axis = axis;
            let previous_split = split;
            let mut extent = [0.0; 3];
            for i in 0..3 {
                extent[i] = finite(grid[1][i] - grid[0][i])?;
                if extent[i] < 0.0 || node_box[1][i] < grid[0][i] || node_box[0][i] > grid[1][i] {
                    return Err(BihBuildError::Bounds);
                }
            }
            let a = primary(extent);
            axis = Some(a);
            split = finite(0.5 * finite(grid[0][a] + grid[1][a])?)?;
            let mut clip_l = f32::NEG_INFINITY;
            let mut clip_r = f32::INFINITY;
            let right_original = right;
            let mut node_l = f32::INFINITY;
            let mut node_r = f32::NEG_INFINITY;
            let mut i = left;
            while i <= right {
                self.charge()?;
                let object = self.objects[i as usize] as usize;
                let min = self.input[object][0][a];
                let max = self.input[object][1][a];
                let center = finite(finite(min + max)? * 0.5)?;
                if center <= split {
                    i += 1;
                    if clip_l < max {
                        clip_l = max;
                    }
                } else {
                    self.objects.swap(i as usize, right as usize);
                    right -= 1;
                    if clip_r > min {
                        clip_r = min;
                    }
                }
                // std::min/max(old,new) keep old on a tie here, unlike AABox merge.
                if min < node_l {
                    node_l = min;
                }
                if node_r < max {
                    node_r = max;
                }
            }
            if node_l > node_box[0][a] && node_r < node_box[1][a] {
                let old = finite(node_box[1][a] - node_box[0][a])?;
                let new = finite(node_r - node_l)?;
                if finite(1.3 * new)? < old {
                    let next = self.allocate_node()?;
                    self.tree[node] = (a as u32) << 30 | (1 << 29) | next as u32;
                    self.tree[node + 1] = node_l.to_bits();
                    self.tree[node + 2] = node_r.to_bits();
                    node_box[0][a] = node_l;
                    node_box[1][a] = node_r;
                    return self.subdivide(left, right_original, grid, node_box, next, depth + 1);
                }
            }
            if right == right_original {
                if previous_axis == axis && fuzzy(previous_split, split) {
                    self.leaf(node, left, right);
                    return Ok(());
                }
                if clip_l <= split {
                    grid[1][a] = split;
                    previous_clip = clip_l;
                    was_left = true;
                    continue;
                }
                grid[1][a] = split;
                previous_clip = f32::NAN;
            } else if left > right {
                right = right_original;
                if previous_axis == axis && fuzzy(previous_split, split) {
                    self.leaf(node, left, right);
                    return Ok(());
                }
                if clip_r >= split {
                    grid[0][a] = split;
                    previous_clip = clip_r;
                    was_left = false;
                    continue;
                }
                grid[0][a] = split;
                previous_clip = f32::NAN;
            } else {
                if let Some(old_axis) = previous_axis {
                    if !previous_clip.is_nan() {
                        let next = self.allocate_node()?;
                        if was_left {
                            self.tree[node] = (old_axis as u32) << 30 | next as u32;
                            self.tree[node + 1] = previous_clip.to_bits();
                            self.tree[node + 2] = f32::INFINITY.to_bits();
                        } else {
                            self.tree[node] = (old_axis as u32) << 30 | (next - 3) as u32;
                            self.tree[node + 1] = f32::NEG_INFINITY.to_bits();
                            self.tree[node + 2] = previous_clip.to_bits();
                        }
                        depth += 1;
                        node = next;
                    }
                }
                break (clip_l, clip_r, right_original, a);
            }
        };
        // Source reaches here only with both partitions nonempty.
        let next = self.allocate_node()?;
        self.allocate_node()?;
        self.tree[node] = (chosen_axis as u32) << 30 | next as u32;
        self.tree[node + 1] = clip_l.to_bits();
        self.tree[node + 2] = clip_r.to_bits();
        let mut grid_l = grid;
        let mut grid_r = grid;
        let mut box_l = node_box;
        let mut box_r = node_box;
        grid_l[1][chosen_axis] = split;
        grid_r[0][chosen_axis] = split;
        box_l[1][chosen_axis] = clip_l;
        box_r[0][chosen_axis] = clip_r;
        self.subdivide(left, right, grid_l, box_l, next, depth + 1)?;
        self.subdivide(
            right + 1,
            right_original,
            grid_r,
            box_r,
            next + 3,
            depth + 1,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(s: &str) -> Vec<u8> {
        s.as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|v| u8::from_str_radix(std::str::from_utf8(v).unwrap(), 16).unwrap())
            .collect()
    }
    #[test]
    fn original_builder_fixture_bytes_match_all_authored_cases() {
        let fixture = include_str!("../tools/fixtures/source-bih.tsv");
        let mut count = 0;
        for line in fixture.lines().filter(|s| !s.starts_with('#')) {
            let f: Vec<_> = line.split('\t').collect();
            let raw = hex(f[2]);
            let input: Vec<_> = raw
                .as_chunks::<24>()
                .0
                .iter()
                .map(|b| {
                    std::array::from_fn(|side| {
                        std::array::from_fn(|axis| {
                            f32::from_le_bytes(
                                b[(side * 3 + axis) * 4..(side * 3 + axis + 1) * 4]
                                    .try_into()
                                    .unwrap(),
                            )
                        })
                    })
                })
                .collect();
            let tree = SourceBih::build(&input, f[1].parse().unwrap(), || false).unwrap();
            assert_eq!(tree.serialize(|| false).unwrap(), hex(f[3]), "{}", f[0]);
            count += 1;
        }
        assert_eq!(count, 199);
    }
    #[test]
    fn explicit_domain_and_work_limits_reject_without_partial_output() {
        assert_eq!(node_end(MAX_BIH_WORDS), Err(BihBuildError::Limit));
        assert_eq!(node_end(usize::MAX), Err(BihBuildError::Limit));
        assert_eq!(node_end(MAX_BIH_WORDS - 3), Ok(MAX_BIH_WORDS));
        assert_eq!(
            SourceBih::build(&[], 0, || false).unwrap_err(),
            BihBuildError::Limit
        );
        assert_eq!(
            SourceBih::build(&[[[1.0; 3], [0.0; 3]]], 3, || false).unwrap_err(),
            BihBuildError::Bounds
        );
        assert_eq!(
            SourceBih::build(&[[[f32::NAN; 3]; 2]], 3, || false).unwrap_err(),
            BihBuildError::Bounds
        );
        assert_eq!(
            SourceBih::build(&[[[f32::MAX; 3]; 2]; 4], 3, || false).unwrap_err(),
            BihBuildError::Numeric
        );
        let mut builder = Builder {
            input: &[],
            objects: Vec::new(),
            tree: Vec::new(),
            leaf: 3,
            work: MAX_BIH_WORK,
            cancelled: &|| false,
        };
        assert_eq!(builder.charge(), Err(BihBuildError::Limit));
    }
    #[test]
    fn cancellation_sweep_never_returns_partial_builder_or_serialization() {
        let input: Vec<_> = (0..257)
            .map(|i| [[i as f32, 0.0, 0.0], [i as f32 + 0.5, 1.0, 1.0]])
            .collect();
        let polls = std::cell::Cell::new(0usize);
        let tree = SourceBih::build(&input, 1, || {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        for cutoff in 0..polls.get() {
            let n = std::cell::Cell::new(0);
            assert_eq!(
                SourceBih::build(&input, 1, || {
                    let at = n.get();
                    n.set(at + 1);
                    at >= cutoff
                })
                .unwrap_err(),
                BihBuildError::Cancelled
            );
        }
        polls.set(0);
        tree.serialize(|| {
            polls.set(polls.get() + 1);
            false
        })
        .unwrap();
        for cutoff in 0..polls.get() {
            let n = std::cell::Cell::new(0);
            assert_eq!(
                tree.serialize(|| {
                    let at = n.get();
                    n.set(at + 1);
                    at >= cutoff
                })
                .unwrap_err(),
                BihBuildError::Cancelled
            );
        }
    }
    fn fixture(path: &str) -> Vec<u8> {
        std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../benilla-native-geometry-tests/tools/fixtures")
                .join(path),
        )
        .unwrap()
    }
    fn word(b: &[u8], at: usize) -> usize {
        u32::from_le_bytes(b[at..at + 4].try_into().unwrap()) as usize
    }
    fn point(b: &[u8], at: usize) -> [f32; 3] {
        std::array::from_fn(|i| {
            f32::from_le_bytes(b[at + i * 4..at + i * 4 + 4].try_into().unwrap())
        })
    }
    fn bound(b: &[u8], at: usize) -> BihBounds {
        [point(b, at), point(b, at + 12)]
    }
    fn bih_size(b: &[u8], at: usize) -> usize {
        let words = word(b, at + 24);
        let objects = word(b, at + 28 + words * 4);
        32 + 4 * (words + objects)
    }
    #[test]
    fn existing_original_mesh_and_group_sections_match_byte_for_byte() {
        for path in [
            "m2_vmap_source/products/authored.m2.vmo",
            "m2_vmap_source/products/signed-zero.m2.vmo",
            "wmo_vmap_source/products/Authored.wmo.vmo",
            "vmap_source/stack.wmo.vmo",
        ] {
            let bytes = fixture(path);
            assert_eq!(&bytes[20..24], b"GMOD");
            let groups = word(&bytes, 24);
            let mut at = 28;
            let mut group_bounds = Vec::new();
            for _ in 0..groups {
                group_bounds.push(bound(&bytes, at));
                at += 32;
                assert_eq!(&bytes[at..at + 4], b"VERT");
                let count = word(&bytes, at + 8);
                at += 12;
                let vertices: Vec<_> = (0..count).map(|i| point(&bytes, at + i * 12)).collect();
                at += 12 * count;
                if count == 0 {
                    continue;
                }
                assert_eq!(&bytes[at..at + 4], b"TRIM");
                let triangles = word(&bytes, at + 8);
                at += 12;
                let mut bounds = Vec::new();
                for i in 0..triangles {
                    let points: [_; 3] =
                        std::array::from_fn(|j| vertices[word(&bytes, at + i * 12 + j * 4)]);
                    let mut b = [points[0]; 2];
                    for p in &points[1..] {
                        for axis in 0..3 {
                            b[0][axis] = if b[0][axis] < p[axis] {
                                b[0][axis]
                            } else {
                                p[axis]
                            };
                            b[1][axis] = if p[axis] < b[1][axis] {
                                b[1][axis]
                            } else {
                                p[axis]
                            };
                        }
                    }
                    bounds.push(b);
                }
                at += 12 * triangles;
                assert_eq!(&bytes[at..at + 4], b"MBIH");
                at += 4;
                let size = bih_size(&bytes, at);
                let actual = SourceBih::build(&bounds, 3, || false)
                    .unwrap()
                    .serialize(|| false)
                    .unwrap();
                assert_eq!(actual, &bytes[at..at + size], "mesh {path}");
                at += size;
                assert_eq!(&bytes[at..at + 4], b"LIQU");
                let liquid = word(&bytes, at + 4);
                at += 8 + liquid;
            }
            assert_eq!(&bytes[at..at + 4], b"GBIH");
            at += 4;
            let size = bih_size(&bytes, at);
            assert_eq!(
                SourceBih::build(&group_bounds, 1, || false)
                    .unwrap()
                    .serialize(|| false)
                    .unwrap(),
                &bytes[at..at + size],
                "groups {path}"
            );
            assert_eq!(at + size, bytes.len());
        }
    }
    #[test]
    fn existing_original_map_bih_sections_match_ascending_spawn_input() {
        for directory in [
            "m2_vmap_source/products",
            "wmo_vmap_source/products",
            "vmap_source",
        ] {
            let tile = fixture(&format!("{directory}/000_32_32.vmtile"));
            let map = fixture(&format!("{directory}/000.vmtree"));
            let count = word(&tile, 8);
            let mut at = 12;
            let mut spawns = Vec::new();
            for _ in 0..count {
                assert_ne!(word(&tile, at) & 4, 0);
                let id = word(&tile, at + 6);
                spawns.push((id, bound(&tile, at + 38)));
                let names = word(&tile, at + 62);
                at += 66 + names + 4;
            }
            spawns.sort_by_key(|p| p.0);
            spawns.dedup_by_key(|p| p.0);
            let bounds: Vec<_> = spawns.into_iter().map(|p| p.1).collect();
            assert_eq!(&map[9..13], b"NODE");
            let size = bih_size(&map, 13);
            assert_eq!(
                SourceBih::build(&bounds, 3, || false)
                    .unwrap()
                    .serialize(|| false)
                    .unwrap(),
                &map[13..13 + size],
                "{directory}"
            );
        }
    }
}
