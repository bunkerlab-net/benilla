// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked BIH serialization, point and nearest-hit traversal from pinned CMaNGOS
//! `src/game/vmap/BIH.{h,cpp}`; see the products CMANGOS-NOTICE and PROVENANCE.
use super::{Bounds, Cursor, Error, QueryBudget, Ray, Vector};

const MAX_ELEMENTS: usize = 262_144;
const OFFSET_MASK: u32 = 0x1fff_ffff;
const LEAF: u32 = 3 << 30;
const BVH2: u32 = 1 << 29;
const MAX_DEPTH: usize = 63;

#[derive(Debug)]
pub(super) struct Bih {
    pub(super) bounds: Bounds,
    tree: Box<[u32]>,
    objects: Box<[u32]>,
}

#[derive(Clone, Copy, Default)]
struct Pending {
    node: usize,
    near: f32,
    far: f32,
}

impl Bih {
    pub(super) fn decode(c: &mut Cursor<'_>) -> Result<Self, Error> {
        let bounds = c.bounds()?;
        let tree = read_words(c)?;
        if tree.len() < 3 || tree.len() % 3 != 0 {
            return Err(Error::Format);
        }
        let objects = read_words(c)?;
        let result = Self {
            bounds,
            tree,
            objects,
        };
        result.validate_structure()?;
        Ok(result)
    }

    pub(super) fn primitive_count(&self) -> usize {
        self.objects.len()
    }

    fn validate_structure(&self) -> Result<(), Error> {
        // The serialized object array is a permutation, not a sparse index space.
        let mut primitives = zeroed(self.objects.len())?;
        for &primitive in &self.objects {
            mark(&mut primitives, primitive as usize)?;
        }
        let mut nodes = zeroed(self.tree.len() / 3)?;
        let mut slots = zeroed(self.objects.len())?;
        let mut pending = [(0usize, 0usize); MAX_DEPTH + 1];
        let mut length = 1;
        while length != 0 {
            length -= 1;
            let (node, depth) = pending[length];
            if depth > MAX_DEPTH || node % 3 != 0 || node + 2 >= self.tree.len() {
                return Err(Error::Format);
            }
            mark(&mut nodes, node / 3)?;
            let word = self.tree[node];
            let axis = word >> 30;
            let offset = (word & OFFSET_MASK) as usize;
            if axis == 3 {
                if word & BVH2 != 0 || self.tree[node + 2] != 0 {
                    return Err(Error::Format);
                }
                let count = self.tree[node + 1] as usize;
                let end = offset.checked_add(count).ok_or(Error::Format)?;
                if end > self.objects.len() {
                    return Err(Error::Format);
                }
                for slot in offset..end {
                    mark(&mut slots, slot)?;
                }
                continue;
            }
            let lo = f32::from_bits(self.tree[node + 1]);
            let hi = f32::from_bits(self.tree[node + 2]);
            let mut push = |child| -> Result<(), Error> {
                if depth == MAX_DEPTH || length == pending.len() {
                    return Err(Error::Format);
                }
                pending[length] = (child, depth + 1);
                length += 1;
                Ok(())
            };
            if word & BVH2 != 0 {
                if !lo.is_finite() || !hi.is_finite() || lo > hi {
                    return Err(Error::Format);
                }
                push(offset)?;
            } else {
                // BIH.cpp omits storage for an empty child. Only these two
                // signed infinities are valid sentinels; their offsets can alias.
                let has_left = lo != f32::NEG_INFINITY;
                let has_right = hi != f32::INFINITY;
                if (!has_left && !has_right)
                    || (has_left && !lo.is_finite())
                    || (has_right && !hi.is_finite())
                {
                    return Err(Error::Format);
                }
                if has_right {
                    push(offset.checked_add(3).ok_or(Error::Format)?)?;
                }
                if has_left {
                    push(offset)?;
                }
            }
        }
        if nodes.contains(&0) || slots.contains(&0) {
            return Err(Error::Format);
        }
        Ok(())
    }

    /// A map BIH can refer to spawns in unloaded tiles. Those unresolved bounds
    /// remain None; every supplied primitive must fit every ancestor clip and root.
    pub(super) fn validate_bounds(&self, primitives: &[Option<Bounds>]) -> Result<(), Error> {
        if primitives.len() != self.objects.len() {
            return Err(Error::Format);
        }
        let mut pending = [(0usize, self.bounds); MAX_DEPTH + 1];
        let mut length = 1;
        while length != 0 {
            length -= 1;
            let (node, allowed) = pending[length];
            let word = self.tree[node];
            let axis = (word >> 30) as usize;
            let offset = (word & OFFSET_MASK) as usize;
            if axis == 3 {
                let end = offset + self.tree[node + 1] as usize;
                for &index in &self.objects[offset..end] {
                    if primitives[index as usize].is_some_and(|bound| {
                        (0..3).any(|a| {
                            !bound.low[a].is_finite()
                                || !bound.high[a].is_finite()
                                || bound.low[a] > bound.high[a]
                                || bound.low[a] < allowed.low[a]
                                || bound.high[a] > allowed.high[a]
                        })
                    }) {
                        return Err(Error::Format);
                    }
                }
                continue;
            }
            let lo = f32::from_bits(self.tree[node + 1]);
            let hi = f32::from_bits(self.tree[node + 2]);
            let mut push = |child, bound: Bounds| -> Result<(), Error> {
                if bound.low[axis] > bound.high[axis] || length == pending.len() {
                    return Err(Error::Format);
                }
                pending[length] = (child, bound);
                length += 1;
                Ok(())
            };
            if word & BVH2 != 0 {
                let mut bound = allowed;
                bound.low[axis] = bound.low[axis].max(lo);
                bound.high[axis] = bound.high[axis].min(hi);
                push(offset, bound)?;
            } else {
                if hi != f32::INFINITY {
                    let mut bound = allowed;
                    bound.low[axis] = bound.low[axis].max(hi);
                    push(offset + 3, bound)?;
                }
                if lo != f32::NEG_INFINITY {
                    let mut bound = allowed;
                    bound.high[axis] = bound.high[axis].min(lo);
                    push(offset, bound)?;
                }
            }
        }
        Ok(())
    }

    /// BIH::intersectPoint: inclusive clips, left before right on overlaps, and
    /// serialized leaf order. A callback cannot terminate traversal after a hit.
    pub(super) fn intersect_point(
        &self,
        position: Vector,
        budget: &mut QueryBudget,
        mut callback: impl FnMut(u32, &mut QueryBudget) -> Result<(), Error>,
    ) -> Result<(), Error> {
        budget.take()?;
        if position.iter().any(|value| !value.is_finite()) {
            return Err(Error::Format);
        }
        if !self.bounds.contains(position) {
            return Ok(());
        }
        let mut pending = [0usize; MAX_DEPTH + 1];
        let mut length = 0;
        let mut node = 0;
        loop {
            loop {
                budget.take()?;
                let word = self.tree[node];
                let axis = (word >> 30) as usize;
                let offset = (word & OFFSET_MASK) as usize;
                if axis == 3 {
                    let end = offset + self.tree[node + 1] as usize;
                    for &primitive in &self.objects[offset..end] {
                        budget.take()?;
                        callback(primitive, budget)?;
                    }
                    break;
                }
                let left = f32::from_bits(self.tree[node + 1]);
                let right = f32::from_bits(self.tree[node + 2]);
                let value = position[axis];
                if word & BVH2 != 0 {
                    node = offset;
                    if left > value || right < value {
                        break;
                    }
                } else {
                    if left < value && right > value {
                        break;
                    }
                    node = offset + 3;
                    if left < value {
                        continue;
                    }
                    node = offset;
                    if right > value {
                        continue;
                    }
                    // Structural validation limits the maximum saved-right depth.
                    pending[length] = offset + 3;
                    length += 1;
                }
            }
            if length == 0 {
                return Ok(());
            }
            length -= 1;
            node = pending[length];
        }
    }

    /// BIH::intersectRay with stopAtFirst=false. Explicit comparisons preserve
    /// source signed-zero ordering and its NaN choices at a parallel clip plane.
    pub(super) fn intersect(
        &self,
        ray: Ray,
        distance: &mut f32,
        budget: &mut QueryBudget,
        callback: impl FnMut(u32, &mut f32, &mut QueryBudget) -> Result<bool, Error>,
    ) -> Result<bool, Error> {
        if !distance.is_finite() {
            return Err(Error::Format);
        }
        self.intersect_with_limit(ray, distance, budget, callback)
    }

    /// GroupModel::IsInsideObject starts at positive infinity. This internal
    /// traversal keeps that source bound without relaxing public finite searches.
    pub(super) fn intersect_with_limit(
        &self,
        ray: Ray,
        distance: &mut f32,
        budget: &mut QueryBudget,
        mut callback: impl FnMut(u32, &mut f32, &mut QueryBudget) -> Result<bool, Error>,
    ) -> Result<bool, Error> {
        if distance.is_nan()
            || *distance < 0.0
            || ray.origin.into_iter().any(|v| !v.is_finite())
            || ray.direction.into_iter().any(|v| !v.is_finite())
        {
            return Err(Error::Format);
        }
        let mut near = -1.0f32;
        let mut far = -1.0f32;
        let mut inverse = [0.0f32; 3];
        for (axis, inverse_axis) in inverse.iter_mut().enumerate() {
            let dir = ray.direction[axis];
            *inverse_axis = 1.0f32 / dir;
            // G3D::fuzzyNe(double, double), with fuzzyEpsilon a float macro.
            let magnitude = f64::from(dir).abs();
            if magnitude > f64::from(0.00001f32) * (magnitude + 1.0) {
                let mut t1 = (self.bounds.low[axis] - ray.origin[axis]) * *inverse_axis;
                let mut t2 = (self.bounds.high[axis] - ray.origin[axis]) * *inverse_axis;
                if t1 > t2 {
                    std::mem::swap(&mut t1, &mut t2);
                }
                if t1 > near {
                    near = t1;
                }
                if t2 < far || far < 0.0 {
                    far = t2;
                }
                if far <= 0.0 || near >= *distance {
                    return Ok(false);
                }
            }
        }
        if near > far {
            return Ok(false);
        }
        // std::max(near, 0.f) and std::min(far, maxDist), including equal zeros.
        if near < 0.0 {
            near = 0.0;
        }
        if *distance < far {
            far = *distance;
        }
        let front: [usize; 3] = ray.direction.map(|v| (v.to_bits() >> 31) as usize);
        let mut pending = [Pending::default(); MAX_DEPTH + 1];
        let mut length = 0;
        let mut node = 0;
        let mut any_hit = false;
        loop {
            loop {
                budget.take()?;
                let word = self.tree[node];
                let axis = (word >> 30) as usize;
                let offset = (word & OFFSET_MASK) as usize;
                if axis == 3 {
                    let end = offset + self.tree[node + 1] as usize;
                    for &primitive in &self.objects[offset..end] {
                        budget.take()?;
                        any_hit |= callback(primitive, distance, budget)?;
                    }
                    break;
                }
                let front_side = front[axis];
                let back_side = front_side ^ 1;
                let tf = (f32::from_bits(self.tree[node + 1 + front_side]) - ray.origin[axis])
                    * inverse[axis];
                let tb = (f32::from_bits(self.tree[node + 1 + back_side]) - ray.origin[axis])
                    * inverse[axis];
                if word & BVH2 != 0 {
                    node = offset;
                    near = if tf >= near { tf } else { near };
                    far = if tb <= far { tb } else { far };
                    if near > far {
                        break;
                    }
                } else {
                    if tf < near && tb > far {
                        break;
                    }
                    let back = offset + back_side * 3;
                    node = back;
                    if tf < near {
                        near = if tb >= near { tb } else { near };
                        continue;
                    }
                    node = offset + front_side * 3;
                    if tb > far {
                        far = if tf <= far { tf } else { far };
                        continue;
                    }
                    // Decode proved at most 63 interior edges and valid children.
                    pending[length] = Pending {
                        node: back,
                        near: if tb >= near { tb } else { near },
                        far,
                    };
                    length += 1;
                    far = if tf <= far { tf } else { far };
                }
            }
            loop {
                if length == 0 {
                    return Ok(any_hit);
                }
                length -= 1;
                let next = pending[length];
                near = next.near;
                if *distance < near {
                    continue;
                }
                node = next.node;
                far = next.far;
                break;
            }
        }
    }
}

fn read_words(c: &mut Cursor<'_>) -> Result<Box<[u32]>, Error> {
    let count = c.word()? as usize;
    if count > MAX_ELEMENTS {
        return Err(Error::Budget);
    }
    let bytes = c.take(count.checked_mul(4).ok_or(Error::Budget)?)?;
    let mut words = Vec::new();
    words.try_reserve_exact(count).map_err(|_| Error::Budget)?;
    for word in bytes.as_chunks::<4>().0 {
        words.push(u32::from_le_bytes(*word));
    }
    Ok(words.into_boxed_slice())
}

fn zeroed(count: usize) -> Result<Vec<u8>, Error> {
    let mut values = Vec::new();
    values.try_reserve_exact(count).map_err(|_| Error::Budget)?;
    values.resize(count, 0);
    Ok(values)
}

fn mark(values: &mut [u8], index: usize) -> Result<(), Error> {
    let value = values.get_mut(index).ok_or(Error::Format)?;
    if *value != 0 {
        return Err(Error::Format);
    }
    *value = 1;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(tree: &[u32], objects: &[u32]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for bound in [-2.0f32, -2.0, -2.0, 2.0, 2.0, 2.0] {
            bytes.extend_from_slice(&bound.to_le_bytes());
        }
        for words in [tree, objects] {
            bytes.extend_from_slice(&(words.len() as u32).to_le_bytes());
            for word in words {
                bytes.extend_from_slice(&word.to_le_bytes());
            }
        }
        bytes
    }

    fn decode(tree: &[u32], objects: &[u32]) -> Result<Bih, Error> {
        Bih::decode(&mut Cursor {
            bytes: &bytes(tree, objects),
            at: 0,
        })
    }

    fn split(left: f32, right: f32) -> [u32; 9] {
        [
            3,
            left.to_bits(),
            right.to_bits(),
            LEAF,
            1,
            0,
            LEAF | 1,
            1,
            0,
        ]
    }

    fn point(x: f32, y: f32, z: f32) -> Bounds {
        Bounds {
            low: [x, y, z],
            high: [x, y, z],
        }
    }

    #[test]
    fn decodes_empty_and_permuted_leaf_indices() {
        let empty = decode(&[LEAF, 0, 0], &[]).unwrap();
        assert_eq!(empty.primitive_count(), 0);
        empty.validate_bounds(&[]).unwrap();
        let tree = decode(&[LEAF, 3, 0], &[2, 0, 1]).unwrap();
        assert_eq!(tree.primitive_count(), 3);
        tree.validate_bounds(&[None, Some(point(1.0, 0.0, 0.0)), None])
            .unwrap();
    }

    #[test]
    fn rejects_truncation_and_counts_before_allocating() {
        let data = bytes(&split(-0.5, 0.5), &[0, 1]);
        for end in 0..data.len() {
            assert!(Bih::decode(&mut Cursor {
                bytes: &data[..end],
                at: 0
            })
            .is_err());
        }
        let mut excessive = data;
        excessive[24..28].copy_from_slice(&((MAX_ELEMENTS + 1) as u32).to_le_bytes());
        assert!(matches!(
            Bih::decode(&mut Cursor {
                bytes: &excessive[..28],
                at: 0
            }),
            Err(Error::Budget)
        ));
        assert!(matches!(decode(&[], &[]), Err(Error::Format)));
        assert!(matches!(decode(&[LEAF, 0], &[]), Err(Error::Format)));
    }

    #[test]
    fn rejects_references_cycles_unreachable_nodes_and_invalid_leaves() {
        assert!(decode(&[BVH2, (-1.0f32).to_bits(), 1.0f32.to_bits()], &[]).is_err());
        assert!(decode(
            &[BVH2 | 6, (-1.0f32).to_bits(), 1.0f32.to_bits(), LEAF, 0, 0],
            &[]
        )
        .is_err());
        assert!(decode(&[LEAF, 0, 0, LEAF, 0, 0], &[]).is_err());
        assert!(decode(&[LEAF | BVH2, 0, 0], &[]).is_err());
        assert!(decode(&[LEAF, 0, 1], &[]).is_err());
        assert!(decode(&[LEAF | 1, 0, 0], &[]).is_err());
        let mut unaligned = split(0.0, 0.0);
        unaligned[0] = 4;
        assert!(decode(&unaligned, &[0, 1]).is_err());
        let mut reused = split(0.0, 0.0);
        reused[6] = 0;
        reused[7] = 0.0f32.to_bits();
        reused[8] = 0.0f32.to_bits();
        assert!(decode(&reused, &[0]).is_err());
    }

    #[test]
    fn requires_complete_unique_primitive_and_leaf_slot_coverage() {
        assert!(decode(&[LEAF, 2, 0], &[0, 0]).is_err());
        assert!(decode(&[LEAF, 2, 0], &[0, 2]).is_err());
        assert!(decode(&[LEAF, 1, 0], &[0, 1]).is_err());
        assert!(decode(&[LEAF, 3, 0], &[0, 1]).is_err());
        let mut duplicate_slot = split(0.0, 0.0);
        duplicate_slot[6] = LEAF;
        assert!(decode(&duplicate_slot, &[0, 1]).is_err());
    }

    #[test]
    fn permits_only_source_empty_child_infinities() {
        let left = [3, 0.0f32.to_bits(), f32::INFINITY.to_bits(), LEAF, 1, 0];
        let right = [0, f32::NEG_INFINITY.to_bits(), 0.0f32.to_bits(), LEAF, 1, 0];
        decode(&left, &[0])
            .unwrap()
            .validate_bounds(&[Some(point(-1.0, 0.0, 0.0))])
            .unwrap();
        decode(&right, &[0])
            .unwrap()
            .validate_bounds(&[Some(point(1.0, 0.0, 0.0))])
            .unwrap();
        for (a, b) in [
            (f32::NEG_INFINITY, f32::INFINITY),
            (f32::INFINITY, 0.0),
            (0.0, f32::NEG_INFINITY),
            (f32::NAN, 0.0),
            (0.0, f32::NAN),
        ] {
            assert!(decode(&split(a, b), &[0, 1]).is_err());
        }
        for (a, b) in [
            (f32::NEG_INFINITY, 1.0f32),
            (0.0, f32::INFINITY),
            (1.0, 0.0),
        ] {
            assert!(decode(&[BVH2 | 3, a.to_bits(), b.to_bits(), LEAF, 1, 0], &[0]).is_err());
        }
    }

    #[test]
    fn bounds_are_checked_against_root_and_every_ancestor_clip() {
        let tree = decode(&split(-0.5, 0.5), &[1, 0]).unwrap();
        tree.validate_bounds(&[Some(point(1.0, 0.0, 0.0)), Some(point(-1.0, 0.0, 0.0))])
            .unwrap();
        assert!(tree
            .validate_bounds(&[Some(point(-1.0, 0.0, 0.0)), None])
            .is_err());
        assert!(tree
            .validate_bounds(&[Some(point(3.0, 0.0, 0.0)), None])
            .is_err());
        assert!(tree.validate_bounds(&[None]).is_err());
        let nested = [
            3,
            0.0f32.to_bits(),
            0.0f32.to_bits(),
            (1 << 30) | BVH2 | 9,
            (-0.5f32).to_bits(),
            0.5f32.to_bits(),
            LEAF | 1,
            1,
            0,
            LEAF,
            1,
            0,
        ];
        let tree = decode(&nested, &[0, 1]).unwrap();
        tree.validate_bounds(&[None, None]).unwrap();
        tree.validate_bounds(&[Some(point(-1.0, 0.5, 0.0)), None])
            .unwrap();
        assert!(tree
            .validate_bounds(&[Some(point(-1.0, 0.6, 0.0)), None])
            .is_err());
        assert!(tree
            .validate_bounds(&[Some(point(f32::NAN, 0.0, 0.0)), None])
            .is_err());
    }

    #[test]
    fn permits_depth_63_and_rejects_deeper_trees() {
        fn chain(edges: usize) -> Vec<u32> {
            let mut tree = Vec::new();
            for n in 0..edges {
                tree.extend_from_slice(&[
                    BVH2 | ((n + 1) * 3) as u32,
                    (-1.0f32).to_bits(),
                    1.0f32.to_bits(),
                ]);
            }
            tree.extend_from_slice(&[LEAF, 1, 0]);
            tree
        }
        decode(&chain(63), &[0])
            .unwrap()
            .validate_bounds(&[Some(point(0.0, 0.0, 0.0))])
            .unwrap();
        assert!(decode(&chain(64), &[0]).is_err());
    }

    fn budget(remaining: usize) -> QueryBudget {
        QueryBudget {
            remaining,
            deadline: std::time::Instant::now() + std::time::Duration::from_secs(5),
        }
    }

    fn visits(tree: &Bih, origin: [f32; 3], direction: [f32; 3]) -> Vec<u32> {
        let mut seen = Vec::new();
        let hit = tree
            .intersect(
                Ray { origin, direction },
                &mut 10.0,
                &mut budget(1000),
                |id, _, _| {
                    seen.push(id);
                    Ok(false)
                },
            )
            .unwrap();
        assert!(!hit);
        seen
    }

    #[test]
    fn traversal_follows_direction_sign_and_keeps_searching_after_a_hit() {
        let tree = decode(&split(0.0, 0.0), &[1, 0]).unwrap();
        assert_eq!(visits(&tree, [-1.0, 0.0, 0.0], [1.0, 0.0, 0.0]), [1, 0]);
        assert_eq!(visits(&tree, [1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]), [0, 1]);
        let mut seen = Vec::new();
        let mut distance = 10.0;
        assert!(tree
            .intersect(
                Ray {
                    origin: [-1.0, 0.0, 0.0],
                    direction: [1.0, 0.0, 0.0]
                },
                &mut distance,
                &mut budget(1000),
                |id, max, _| {
                    seen.push(id);
                    *max = if id == 1 { 2.0 } else { 1.5 };
                    Ok(true)
                }
            )
            .unwrap());
        assert_eq!(seen, [1, 0]);
        assert_eq!(distance.to_bits(), 1.5f32.to_bits());
    }

    #[test]
    fn nearer_hit_prunes_a_saved_far_node_but_not_current_leaf_objects() {
        let tree = decode(&split(-0.5, 0.5), &[0, 1]).unwrap();
        let mut seen = Vec::new();
        let mut distance = 10.0;
        assert!(tree
            .intersect(
                Ray {
                    origin: [-1.0, 0.0, 0.0],
                    direction: [1.0, 0.0, 0.0]
                },
                &mut distance,
                &mut budget(1000),
                |id, max, _| {
                    seen.push(id);
                    *max = 0.25;
                    Ok(true)
                }
            )
            .unwrap());
        assert_eq!(seen, [0]);
        let leaf = decode(&[LEAF, 2, 0], &[0, 1]).unwrap();
        seen.clear();
        assert!(leaf
            .intersect(
                Ray {
                    origin: [-1.0, 0.0, 0.0],
                    direction: [1.0, 0.0, 0.0]
                },
                &mut 10.0,
                &mut budget(1000),
                |id, max, _| {
                    seen.push(id);
                    *max = 0.25;
                    Ok(true)
                }
            )
            .unwrap());
        assert_eq!(seen, [0, 1]);
    }

    #[test]
    fn signed_zero_clip_nan_preserves_source_order() {
        let tree = decode(&split(0.0, 0.0), &[0, 1]).unwrap();
        assert_eq!(visits(&tree, [0.0, -1.0, 0.0], [0.0, 1.0, 0.0]), [0, 1]);
        assert_eq!(visits(&tree, [0.0, -1.0, 0.0], [-0.0, 1.0, 0.0]), [1, 0]);
        let gap = decode(&split(-0.5, 0.5), &[0, 1]).unwrap();
        for zero in [0.0, -0.0] {
            assert!(visits(&gap, [0.0, -1.0, 0.0], [zero, 1.0, 0.0]).is_empty());
            assert_eq!(visits(&gap, [-0.5, -1.0, 0.0], [zero, 1.0, 0.0]), [0]);
            assert_eq!(visits(&gap, [0.5, -1.0, 0.0], [zero, 1.0, 0.0]), [1]);
        }
    }

    #[test]
    fn empty_child_sentinels_never_visit_the_omitted_offset() {
        let left = decode(
            &[3, 0.0f32.to_bits(), f32::INFINITY.to_bits(), LEAF, 1, 0],
            &[0],
        )
        .unwrap();
        let right = decode(
            &[0, f32::NEG_INFINITY.to_bits(), 0.0f32.to_bits(), LEAF, 1, 0],
            &[0],
        )
        .unwrap();
        for zero in [0.0, -0.0] {
            assert_eq!(visits(&left, [0.0, -1.0, 0.0], [zero, 1.0, 0.0]), [0]);
            assert_eq!(visits(&right, [0.0, -1.0, 0.0], [zero, 1.0, 0.0]), [0]);
            assert!(visits(&left, [0.5, -1.0, 0.0], [zero, 1.0, 0.0]).is_empty());
            assert!(visits(&right, [-0.5, -1.0, 0.0], [zero, 1.0, 0.0]).is_empty());
        }
    }

    #[test]
    fn bvh2_parallel_boundary_retains_interval_when_a_clip_is_nan() {
        let tree = decode(
            &[BVH2 | 3, (-1.0f32).to_bits(), 1.0f32.to_bits(), LEAF, 1, 0],
            &[0],
        )
        .unwrap();
        for zero in [0.0, -0.0] {
            for x in [-1.0, 0.0, 1.0] {
                assert_eq!(visits(&tree, [x, -1.0, 0.0], [zero, 1.0, 0.0]), [0]);
            }
            assert!(visits(&tree, [1.5, -1.0, 0.0], [zero, 1.0, 0.0]).is_empty());
            assert!(visits(&tree, [-1.5, -1.0, 0.0], [zero, 1.0, 0.0]).is_empty());
        }
    }

    #[test]
    fn root_slabs_use_double_fuzzyne_with_a_promoted_float_epsilon() {
        let leaf = decode(&[LEAF, 1, 0], &[0]).unwrap();
        // Adjacent float directions bracket G3D's exact fuzzy-zero threshold.
        let skipped = f32::from_bits(0x3727_c619);
        let checked = f32::from_bits(0x3727_c61a);
        assert_eq!(visits(&leaf, [3.0, -1.0, 0.0], [skipped, 1.0, 0.0]), [0]);
        assert!(visits(&leaf, [3.0, -1.0, 0.0], [checked, 1.0, 0.0]).is_empty());
        assert_eq!(visits(&leaf, [-3.0, -1.0, 0.0], [-skipped, 1.0, 0.0]), [0]);
        assert!(visits(&leaf, [-3.0, -1.0, 0.0], [-checked, 1.0, 0.0]).is_empty());
    }

    #[test]
    fn root_endpoint_rejection_and_query_errors_are_not_hits() {
        let leaf = decode(&[LEAF, 1, 0], &[0]).unwrap();
        assert!(visits(&leaf, [2.0, 0.0, 0.0], [1.0, 0.0, 0.0]).is_empty());
        let ray = Ray {
            origin: [-3.0, 0.0, 0.0],
            direction: [1.0, 0.0, 0.0],
        };
        assert!(!leaf
            .intersect(ray, &mut 1.0, &mut budget(1000), |_, _, _| panic!(
                "endpoint entered a leaf"
            ))
            .unwrap());
        assert_eq!(
            leaf.intersect(ray, &mut 10.0, &mut budget(0), |_, _, _| Ok(false)),
            Err(Error::Budget)
        );
        assert_eq!(
            leaf.intersect(ray, &mut 10.0, &mut budget(1), |_, _, _| panic!(
                "unbudgeted callback"
            )),
            Err(Error::Budget)
        );
        assert_eq!(
            leaf.intersect(ray, &mut 10.0, &mut budget(1000), |_, _, _| Err(
                Error::Missing
            )),
            Err(Error::Missing)
        );
        let mut expired = budget(1000);
        expired.deadline = std::time::Instant::now();
        assert_eq!(
            leaf.intersect(ray, &mut 10.0, &mut expired, |_, _, _| Ok(false)),
            Err(Error::Budget)
        );
        let mut invalid_distance = f32::NAN;
        assert_eq!(
            leaf.intersect(ray, &mut invalid_distance, &mut budget(1000), |_, _, _| Ok(
                false
            )),
            Err(Error::Format)
        );
    }

    fn point_visits(tree: &Bih, position: Vector) -> Vec<u32> {
        let mut seen = Vec::new();
        tree.intersect_point(position, &mut budget(1000), |id, _| {
            seen.push(id);
            Ok(())
        })
        .unwrap();
        seen
    }

    #[test]
    fn point_traversal_preserves_inclusive_clips_overlap_order_and_empty_children() {
        let overlap = decode(&split(0.5, -0.5), &[1, 0]).unwrap();
        for x in [-0.5, 0.0, -0.0, 0.5] {
            assert_eq!(point_visits(&overlap, [x, 0.0, 0.0]), [1, 0]);
        }
        assert_eq!(point_visits(&overlap, [-0.500_001, 0.0, 0.0]), [1]);
        assert_eq!(point_visits(&overlap, [0.500_001, 0.0, 0.0]), [0]);
        let gap = decode(&split(-0.5, 0.5), &[1, 0]).unwrap();
        assert!(point_visits(&gap, [0.0; 3]).is_empty());
        assert_eq!(point_visits(&gap, [-0.5, 0.0, 0.0]), [1]);
        assert_eq!(point_visits(&gap, [0.5, 0.0, 0.0]), [0]);
        let left = decode(
            &[3, 0.0f32.to_bits(), f32::INFINITY.to_bits(), LEAF, 1, 0],
            &[0],
        )
        .unwrap();
        let right = decode(
            &[0, f32::NEG_INFINITY.to_bits(), 0.0f32.to_bits(), LEAF, 1, 0],
            &[0],
        )
        .unwrap();
        for zero in [0.0, -0.0] {
            assert_eq!(point_visits(&left, [zero, 0.0, 0.0]), [0]);
            assert_eq!(point_visits(&right, [zero, 0.0, 0.0]), [0]);
        }
        assert!(point_visits(&left, [0.1, 0.0, 0.0]).is_empty());
        assert!(point_visits(&right, [-0.1, 0.0, 0.0]).is_empty());
        let clipped = decode(
            &[BVH2 | 3, (-1.0f32).to_bits(), 1.0f32.to_bits(), LEAF, 1, 0],
            &[0],
        )
        .unwrap();
        for x in [-1.0, 0.0, 1.0] {
            assert_eq!(point_visits(&clipped, [x, 0.0, 0.0]), [0]);
        }
        assert!(point_visits(&clipped, [1.0f32.next_up(), 0.0, 0.0]).is_empty());
        assert!(point_visits(&clipped, [-1.0f32.next_up(), 0.0, 0.0]).is_empty());
        assert!(point_visits(&overlap, [3.0, 0.0, 0.0]).is_empty());
    }

    #[test]
    fn point_traversal_accounts_nodes_callbacks_deadline_and_invalid_positions() {
        let leaf = decode(&[LEAF, 2, 0], &[1, 0]).unwrap();
        assert_eq!(point_visits(&leaf, [0.0; 3]), [1, 0]);
        assert_eq!(
            leaf.intersect_point([0.0; 3], &mut budget(0), |_, _| Ok(())),
            Err(Error::Budget)
        );
        assert_eq!(
            leaf.intersect_point([0.0; 3], &mut budget(2), |_, _| panic!(
                "unbudgeted callback"
            )),
            Err(Error::Budget)
        );
        let mut expired = budget(1000);
        expired.deadline = std::time::Instant::now();
        assert_eq!(
            leaf.intersect_point([0.0; 3], &mut expired, |_, _| Ok(())),
            Err(Error::Budget)
        );
        assert_eq!(
            leaf.intersect_point([0.0; 3], &mut budget(1000), |_, _| Err(Error::Missing)),
            Err(Error::Missing)
        );
        for value in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                leaf.intersect_point([value; 3], &mut budget(1000), |_, _| Ok(())),
                Err(Error::Format)
            );
        }
    }

    #[test]
    fn infinite_distance_is_internal_to_location_group_searches() {
        let leaf = decode(&[LEAF, 1, 0], &[0]).unwrap();
        let ray = Ray {
            origin: [0.0, 0.0, 1.0],
            direction: [0.0, 0.0, -1.0],
        };
        let mut infinite = f32::INFINITY;
        assert_eq!(
            leaf.intersect(ray, &mut infinite, &mut budget(1000), |_, _, _| Ok(false)),
            Err(Error::Format)
        );
        let mut distance = f32::INFINITY;
        assert!(leaf
            .intersect_with_limit(ray, &mut distance, &mut budget(1000), |_, distance, _| {
                *distance = 1.0;
                Ok(true)
            })
            .unwrap());
        assert_eq!(distance.to_bits(), 1.0f32.to_bits());
        for mut invalid in [f32::NAN, f32::NEG_INFINITY, -1.0] {
            assert_eq!(
                leaf.intersect_with_limit(ray, &mut invalid, &mut budget(1000), |_, _, _| Ok(
                    false
                )),
                Err(Error::Format)
            );
        }
    }

    #[test]
    fn loaded_point_tree_words_match_original_cpp_callback_order() {
        let fixture = include_str!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/scopedlocation_bih.tsv"
        );
        let word = |value: &str| u32::from_str_radix(value, 16).unwrap();
        let list = |value: &str| -> Vec<u32> {
            if value == "-" {
                Vec::new()
            } else {
                value.split(',').map(word).collect()
            }
        };
        let mut count = 0;
        for row in fixture
            .lines()
            .filter(|row| !row.starts_with('#') && !row.is_empty())
        {
            let columns: Vec<_> = row.split('\t').collect();
            assert_eq!(columns.len(), 7, "{row}");
            let tree = decode(&list(columns[1]), &list(columns[2])).unwrap();
            let position = std::array::from_fn(|axis| f32::from_bits(word(columns[3 + axis])));
            assert_eq!(
                point_visits(&tree, position),
                list(columns[6]),
                "{}",
                columns[0]
            );
            count += 1;
        }
        assert_eq!(count, 492);
    }

    #[test]
    fn matches_original_header_callback_order_and_distance_oracle() {
        // Generated by the source-hash-guarded C++ helper, without native parsing.
        let fixture = include_str!(
            "../../../../../../../../benilla-native-geometry-tests/tools/fixtures/vmap_source/bih-oracle.tsv"
        );
        let word = |v: &str| u32::from_str_radix(v, 16).unwrap();
        let list = |v: &str| -> Vec<u32> {
            if v.is_empty() {
                Vec::new()
            } else {
                v.split(',').map(word).collect()
            }
        };
        let mut cases = 0;
        for row in fixture
            .lines()
            .filter(|row| !row.starts_with('#') && !row.is_empty())
        {
            let columns: Vec<_> = row.split('\t').collect();
            assert_eq!(columns.len(), 13, "{row}");
            let tree = decode(&list(columns[1]), &list(columns[2])).unwrap();
            let ray = Ray {
                origin: std::array::from_fn(|axis| f32::from_bits(word(columns[3 + axis]))),
                direction: std::array::from_fn(|axis| f32::from_bits(word(columns[6 + axis]))),
            };
            let initial = word(columns[9]);
            let mut distance = f32::from_bits(initial);
            let mode: u32 = columns[10].parse().unwrap();
            let mut seen = Vec::new();
            let hit = tree
                .intersect(ray, &mut distance, &mut budget(1000), |id, max, _| {
                    seen.push(id);
                    if mode == 0 {
                        return Ok(false);
                    }
                    let candidate = if mode == 1 {
                        0.25
                    } else if id == 1 {
                        2.0
                    } else {
                        1.5
                    };
                    if candidate < *max {
                        *max = candidate;
                        Ok(true)
                    } else {
                        Ok(false)
                    }
                })
                .unwrap();
            assert_eq!(seen, list(columns[11]), "callback order: {}", columns[0]);
            assert_eq!(
                distance.to_bits(),
                word(columns[12]),
                "distance: {}",
                columns[0]
            );
            assert_eq!(hit, initial != word(columns[12]), "hit: {}", columns[0]);
            cases += 1;
        }
        assert_eq!(cases, 188);
    }
}
