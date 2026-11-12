// SPDX-License-Identifier: GPL-3.0-or-later
// Checked BIH serialization, point and nearest-hit traversal from pinned CMaNGOS
// `src/game/vmap/BIH.{h,cpp}`; see the products CMANGOS-NOTICE and PROVENANCE.
use super::{Bounds, Cursor, Error, QueryBudget, Ray, Vector, MAX_OBJECT_WORDS, MAX_TREE_WORDS};

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
        let tree = read_words(c, MAX_TREE_WORDS)?;
        if tree.len() < 3 || tree.len() % 3 != 0 {
            return Err(Error::Format);
        }
        let objects = read_words(c, MAX_OBJECT_WORDS)?;
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
        callback: impl FnMut(u32, &mut f32, &mut QueryBudget) -> Result<bool, Error>,
    ) -> Result<bool, Error> {
        self.intersect_source(ray, distance, budget, false, callback)
    }

    /// Source LOS uses stopAtFirstHit at every hierarchy level.
    pub(super) fn intersect_source(
        &self,
        ray: Ray,
        distance: &mut f32,
        budget: &mut QueryBudget,
        stop: bool,
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
                        if callback(primitive, distance, budget)? {
                            any_hit = true;
                            if stop {
                                return Ok(true);
                            }
                        }
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

fn read_words(c: &mut Cursor<'_>, limit: usize) -> Result<Box<[u32]>, Error> {
    let count = c.word()? as usize;
    if count > limit {
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
