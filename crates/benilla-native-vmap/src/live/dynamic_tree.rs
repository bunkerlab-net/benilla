// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 src/game/vmap/{RegularGrid.h,BIHWrap.h,DynamicTree.cpp}.
use super::*;
use benilla_sim::source_time::ShortTimeTracker;
const CELL_SIZE: f32 = (533.333_3_f32 * 64.0) / 64.0;

// G3D HashTrait.h:26; Table.h:209-243,741-810,872-881.
// This is not the libstdc++ IdentityTable used by the Map update set.
#[derive(Default)]
struct PointerSet {
    buckets: Vec<Option<usize>>,
    nodes: Vec<Option<(DynamicModelHandle, Option<usize>)>>,
    free: Vec<usize>,
    len: usize,
}
impl PointerSet {
    fn resize(&mut self, size: usize) {
        let mut buckets = vec![None; size];
        for head in &self.buckets {
            let mut at = *head;
            while let Some(i) = at {
                let (h, next) = self.nodes[i].as_mut().unwrap();
                at = *next;
                let bucket = h.pointer().unwrap() as usize % size;
                *next = buckets[bucket];
                buckets[bucket] = Some(i);
            }
        }
        self.buckets = buckets;
    }
    fn insert(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        if self.buckets.is_empty() {
            self.resize(10);
        }
        let mut at = self.buckets[h.pointer().unwrap() as usize % self.buckets.len()];
        let mut length = 1;
        let mut all_same = true;
        while let Some(i) = at {
            let (old, next) = self.nodes[i].as_ref().unwrap();
            all_same &= old.pointer() == h.pointer();
            if old.pointer() == h.pointer() {
                if old != h {
                    return Err(DynamicError::StaleModel);
                }
                return Ok(());
            }
            at = *next;
            length += 1;
        }
        if length > 3 && !all_same && self.buckets.len() < self.len * 15 {
            let factor = if self.buckets.len() > 1_000_000 {
                1.5_f32
            } else if self.buckets.len() > 100_000 {
                2.0
            } else {
                3.0
            };
            self.resize(
                ((self.buckets.len() as f32 * factor) as usize + 1)
                    .max((self.len as f32 * factor) as usize),
            );
        }
        let bucket = h.pointer().unwrap() as usize % self.buckets.len();
        let node = (h.clone(), self.buckets[bucket]);
        let index = if let Some(i) = self.free.pop() {
            self.nodes[i] = Some(node);
            i
        } else {
            self.nodes.push(Some(node));
            self.nodes.len() - 1
        };
        self.buckets[bucket] = Some(index);
        self.len += 1;
        Ok(())
    }
    fn remove(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        if self.buckets.is_empty() {
            return Ok(());
        }
        let bucket = h.pointer().unwrap() as usize % self.buckets.len();
        let mut previous: Option<usize> = None;
        let mut at = self.buckets[bucket];
        while let Some(i) = at {
            let (old, next) = self.nodes[i].as_ref().unwrap();
            let next = *next;
            if old.pointer() == h.pointer() {
                if old != h {
                    return Err(DynamicError::StaleModel);
                }
                if let Some(p) = previous {
                    self.nodes[p].as_mut().unwrap().1 = next;
                } else {
                    self.buckets[bucket] = next;
                }
                self.nodes[i] = None;
                self.free.push(i);
                self.len -= 1;
                return Ok(());
            }
            previous = Some(i);
            at = next;
        }
        Ok(())
    }
    fn members(&self, out: &mut Vec<DynamicModelHandle>) {
        // Set.h:97-99 delegates to Table.h:872-881, which REPLACES out.
        out.clear();
        for head in &self.buckets {
            let mut at = *head;
            while let Some(i) = at {
                let (h, next) = self.nodes[i].as_ref().unwrap();
                out.push(h.clone());
                at = *next;
            }
        }
    }
}
enum PendingModels {
    Bound(PointerSet),
    Unbound(BTreeMap<u64, DynamicModelHandle>),
}
impl Default for PendingModels {
    fn default() -> Self {
        Self::Bound(PointerSet::default())
    }
}
impl PendingModels {
    fn insert(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        if h.pointer().is_none() && matches!(self, Self::Bound(_)) {
            let Self::Bound(exact) = std::mem::take(self) else {
                unreachable!()
            };
            *self = Self::Unbound(
                exact
                    .nodes
                    .into_iter()
                    .flatten()
                    .map(|(h, _)| (h.key(), h))
                    .collect(),
            );
        }
        match self {
            Self::Bound(exact) => exact.insert(h),
            Self::Unbound(logical) => {
                match logical.entry(h.key()) {
                    std::collections::btree_map::Entry::Occupied(old) => {
                        if old.get() != h {
                            return Err(DynamicError::StaleModel);
                        }
                    }
                    std::collections::btree_map::Entry::Vacant(slot) => {
                        slot.insert(h.clone());
                    }
                }
                Ok(())
            }
        }
    }
    fn remove(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        match self {
            Self::Bound(exact) => exact.remove(h),
            Self::Unbound(logical) => {
                if logical.get(&h.key()).is_some_and(|old| old != h) {
                    return Err(DynamicError::StaleModel);
                }
                logical.remove(&h.key());
                Ok(())
            }
        }
    }
    fn is_bound(&self) -> bool {
        matches!(self, Self::Bound(_))
    }
    fn bucket_count(&self) -> Option<usize> {
        match self {
            Self::Bound(exact) => Some(exact.buckets.len()),
            Self::Unbound(_) => None,
        }
    }
    fn members(&self, out: &mut Vec<DynamicModelHandle>) {
        match self {
            Self::Bound(exact) => exact.members(out),
            Self::Unbound(logical) => {
                // Fixed native-generation order is usable only where callbacks are
                // bit-identical; Node::intersect refuses every other unbound query.
                out.clear();
                out.extend(logical.values().cloned());
            }
        }
    }
}
#[derive(Default)]
struct Node {
    pending: PendingModels,
    objects: Vec<DynamicModelHandle>,
    tree: Option<Bih>,
    unbalanced: i32,
}
fn increment(value: &mut i32) -> DynamicResult<()> {
    *value = value
        .checked_add(1)
        .ok_or(DynamicError::UnbalancedCounterOverflow)?;
    Ok(())
}
impl Node {
    fn insert(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        increment(&mut self.unbalanced)?;
        self.pending.insert(h)
    }
    fn remove(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        increment(&mut self.unbalanced)?;
        // BIHWrap.h:62-98 never populates m_obj2Idx. Preserve the pending-set path.
        self.pending.remove(h)
    }
    fn balance(&mut self, models: &DynamicModels) -> DynamicResult<()> {
        if self.unbalanced == 0 {
            return Ok(());
        }
        self.unbalanced = 0;
        self.pending.members(&mut self.objects);
        let bounds = self
            .objects
            .iter()
            .map(|h| models.bounds(h).map(|b| [b.low, b.high]))
            .collect::<DynamicResult<Vec<_>>>()?;
        let built = crate::SourceBih::build(&bounds, 3, || false).map_err(|e| {
            DynamicError::Vmap(match e {
                crate::BihBuildError::Cancelled => Error::Deadline,
                crate::BihBuildError::Limit | crate::BihBuildError::Allocation => Error::Budget,
                crate::BihBuildError::Bounds | crate::BihBuildError::Numeric => Error::Unsupported,
            })
        })?;
        let (bounds, tree, objects) = built.into_parts();
        self.tree = Some(Bih {
            bounds: Bounds {
                low: bounds[0],
                high: bounds[1],
            },
            tree: tree.into_boxed_slice(),
            objects: objects.into_boxed_slice(),
        });
        Ok(())
    }
    fn intersect(
        &mut self,
        scene: (&DynamicModels, &Manager),
        ray: Ray,
        distance: &mut f32,
        did_hit: &mut bool,
        budget: &mut QueryBudget,
        ignore: bool,
    ) -> DynamicResult<()> {
        let (models, manager) = scene;
        self.balance(models)?;
        if !self.pending.is_bound() {
            if let Some(first) = self.objects.first() {
                for other in &self.objects[1..] {
                    if !models.equivalent(first, other)? {
                        return Err(DynamicError::ModelAllocationUnbound);
                    }
                }
            }
        }
        // Singleton and bit-identical callbacks cannot observe object identity:
        // DynamicTree.cpp:145-148 writes only distance and the overwritten hit bit.
        let Some(tree) = &self.tree else {
            return Ok(());
        };
        let mut failure = None;
        let result = tree.intersect_source(ray, distance, budget, true, |i, d, b| {
            let Some(h) = self.objects.get(i as usize) else {
                return Ok(false);
            };
            match models.intersect(manager, h, ray, d, b, ignore) {
                // Copy the overwrite, not OR, DynamicTree.cpp:147.
                Ok(hit) => {
                    *did_hit = hit;
                    Ok(hit)
                }
                Err(e) => {
                    failure = Some(e);
                    Err(Error::StaleGeneration)
                }
            }
        });
        if let Some(e) = failure {
            return Err(e);
        }
        result?;
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicNodeInventory {
    pub cell: [i32; 2],
    pub buckets: Option<usize>,
    pub pointer_order_bound: bool,
    pub bounds: Option<[[f32; 3]; 2]>,
    pub pending: Vec<DynamicModelHandle>,
    pub objects: Vec<DynamicModelHandle>,
    pub unbalanced: i32,
    pub tree: Vec<u32>,
    pub primitives: Vec<u32>,
}
pub struct DynamicMapTree {
    // Source node-array order is x outside y. Only allocated cells are stored.
    nodes: BTreeMap<[i32; 2], Node>,
    // Equal-pointer membership includes duplicates; other-key iteration is never observed.
    members: BTreeMap<u64, (DynamicModelHandle, Vec<[i32; 2]>)>,
    size: usize,
    timer: ShortTimeTracker,
    unbalanced: i32,
}
impl Default for DynamicMapTree {
    fn default() -> Self {
        Self {
            nodes: BTreeMap::new(),
            members: BTreeMap::new(),
            size: 0,
            timer: ShortTimeTracker::new(200),
            unbalanced: 0,
        }
    }
}
impl DynamicMapTree {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn timer_ms(&self) -> u32 {
        self.timer.get_expiry()
    }
    pub fn unbalanced(&self) -> i32 {
        self.unbalanced
    }
    pub fn size(&self) -> usize {
        self.size
    }
    pub fn membership_inventory(&self) -> Vec<(DynamicModelHandle, Vec<[i32; 2]>)> {
        self.members.values().cloned().collect()
    }
    pub fn contains(&self, h: &DynamicModelHandle) -> bool {
        self.members.get(&h.key()).is_some_and(|(old, _)| old == h)
    }
    pub fn inventory(&self) -> Vec<DynamicNodeInventory> {
        self.nodes
            .iter()
            .map(|(&cell, n)| {
                let mut pending = Vec::new();
                n.pending.members(&mut pending);
                DynamicNodeInventory {
                    cell,
                    buckets: n.pending.bucket_count(),
                    pending,
                    pointer_order_bound: n.pending.is_bound(),
                    bounds: n.tree.as_ref().map(|t| [t.bounds.low, t.bounds.high]),
                    objects: n.objects.clone(),
                    unbalanced: n.unbalanced,
                    tree: n.tree.as_ref().map_or_else(Vec::new, |t| t.tree.to_vec()),
                    primitives: n
                        .tree
                        .as_ref()
                        .map_or_else(Vec::new, |t| t.objects.to_vec()),
                }
            })
            .collect()
    }
    pub fn insert(&mut self, models: &DynamicModels, h: &DynamicModelHandle) -> DynamicResult<()> {
        let bounds = models.bounds(h)?;
        let low = cell(bounds.low)?;
        let high = cell(bounds.high)?;
        for x in low[0]..=high[0] {
            for y in low[1]..=high[1] {
                // getGrid asserts only upper bounds, then indexes the source array.
                if x >= 64 || y >= 64 {
                    return Err(DynamicError::CellIndexAssertion);
                }
                if x < 0 || y < 0 {
                    return Err(DynamicError::CellIndexUndefined);
                }
                let cell = [x, y];
                self.nodes.entry(cell).or_default().insert(h)?;
                let member = self
                    .members
                    .entry(h.key())
                    .or_insert_with(|| (h.clone(), Vec::new()));
                if member.0 != *h {
                    return Err(DynamicError::StaleModel);
                }
                member.1.push(cell);
                self.size += 1;
            }
        }
        increment(&mut self.unbalanced)
    }
    pub fn remove(&mut self, h: &DynamicModelHandle) -> DynamicResult<()> {
        if self.members.get(&h.key()).is_some_and(|(old, _)| old != h) {
            return Err(DynamicError::StaleModel);
        }
        if let Some((_, cells)) = self.members.get(&h.key()) {
            // libstdc++ unordered_multimap inserts equivalent keys at group head.
            for cell in cells.iter().rev() {
                self.nodes.get_mut(cell).unwrap().remove(h)?;
            }
            self.size -= cells.len();
            self.members.remove(&h.key());
        }
        increment(&mut self.unbalanced)
    }
    pub fn balance(&mut self, models: &DynamicModels) -> DynamicResult<()> {
        for node in self.nodes.values_mut() {
            node.balance(models)?;
        }
        self.unbalanced = 0;
        Ok(())
    }
    pub fn update(&mut self, models: &DynamicModels, diff: u32) -> DynamicResult<()> {
        // RegularGrid.h:131 narrows the memberTable cardinality to uint32 first.
        if self.size as u32 == 0 {
            return Ok(());
        }
        self.timer.update(diff);
        if self.timer.passed() {
            self.timer.reset(200);
            if self.unbalanced > 0 {
                self.balance(models)?;
            }
        }
        Ok(())
    }
    fn ray(
        &mut self,
        scene: (&DynamicModels, &Manager),
        ray: Ray,
        end: Vector,
        distance: &mut f32,
        ignore: bool,
        deadline: Instant,
    ) -> DynamicResult<bool> {
        let (models, manager) = scene;
        let mut at = cell(ray.origin)?;
        if !valid(at) {
            return Ok(false);
        }
        let last = cell(end)?;
        let mut hit = false;
        let mut budget = QueryBudget::new(deadline);
        if at == last {
            if let Some(node) = self.nodes.get_mut(&at) {
                node.intersect(
                    (models, manager),
                    ray,
                    distance,
                    &mut hit,
                    &mut budget,
                    ignore,
                )?;
            }
            return Ok(hit);
        }
        let inv = [1.0 / ray.direction[0], 1.0 / ray.direction[1]];
        let step = inv.map(|v| if v >= 0.0 { 1 } else { -1 });
        // RegularGrid.h:186-206 uses cell indices directly, without subtracting 32.
        let mut max: [f32; 2] = std::array::from_fn(|i| {
            ((at[i] + step[i]) as f32 * CELL_SIZE - ray.origin[i]) * inv[i]
        });
        let delta = inv.map(|v| CELL_SIZE * v.abs());
        loop {
            budget.take()?;
            if let Some(node) = self.nodes.get_mut(&at) {
                node.intersect(
                    (models, manager),
                    ray,
                    distance,
                    &mut hit,
                    &mut budget,
                    ignore,
                )?;
            }
            if at == last {
                break;
            }
            let axis = if max[0] < max[1] { 0 } else { 1 };
            max[axis] += delta[axis];
            at[axis] += step[axis];
            if !valid(at) {
                break;
            }
        }
        Ok(hit)
    }
    pub fn line_of_sight(
        &mut self,
        models: &DynamicModels,
        manager: &Manager,
        from: Vector,
        to: Vector,
        ignore: bool,
        deadline: Instant,
    ) -> DynamicResult<bool> {
        arithmetic_environment()?;
        let delta = sub(to, from);
        let mut distance = dot(delta, delta).sqrt();
        // G3D g3dmath.h:787-810 uses binary64 fuzzyGt, not the static tiny-ray test.
        let d = f64::from(distance);
        let aa = d.abs() + 1.0;
        let epsilon = f64::from(0.00001_f32) * if aa == f64::INFINITY { 1.0 } else { aa };
        if d.partial_cmp(&epsilon) != Some(std::cmp::Ordering::Greater) {
            return Ok(true);
        }
        // G3D Vector3.h:138-140 divides by multiplying one rounded reciprocal.
        let ray = Ray {
            origin: from,
            direction: scale(delta, 1.0 / distance),
        };
        Ok(!self.ray((models, manager), ray, to, &mut distance, ignore, deadline)?)
    }
    pub fn height(
        &mut self,
        models: &DynamicModels,
        manager: &Manager,
        position: Vector,
        mut distance: f32,
        deadline: Instant,
    ) -> DynamicResult<f32> {
        arithmetic_environment()?;
        let at = cell(position)?;
        let mut hit = false;
        if valid(at) {
            if let Some(node) = self.nodes.get_mut(&at) {
                node.intersect(
                    (models, manager),
                    Ray {
                        origin: position,
                        direction: [0.0, 0.0, -1.0],
                    },
                    &mut distance,
                    &mut hit,
                    &mut QueryBudget::new(deadline),
                    false,
                )?;
            }
        }
        Ok(if hit {
            position[2] - distance
        } else {
            f32::NEG_INFINITY
        })
    }
    pub fn hit_position(
        &mut self,
        models: &DynamicModels,
        manager: &Manager,
        from: Vector,
        to: Vector,
        modify: f32,
        deadline: Instant,
    ) -> DynamicResult<(bool, Vector)> {
        arithmetic_environment()?;
        let delta = sub(to, from);
        let mut distance = dot(delta, delta).sqrt();
        if !distance.is_finite() || distance >= f32::MAX {
            return Err(Error::NumericBinding.into());
        }
        if distance < 1e-10 {
            return Ok((false, to));
        }
        let direction = scale(delta, 1.0 / distance);
        if !self.ray(
            (models, manager),
            Ray {
                origin: from,
                direction,
            },
            to,
            &mut distance,
            false,
            deadline,
        )? {
            return Ok((false, to));
        }
        let mut position = std::array::from_fn(|i| from[i] + direction[i] * distance);
        if modify < 0.0 && dot(sub(position, from), sub(position, from)).sqrt() <= -modify {
            position = from;
        } else {
            for i in 0..3 {
                position[i] += direction[i] * modify;
            }
        }
        #[cfg(not(target_arch = "x86_64"))]
        if position.iter().any(|v| v.is_nan()) {
            return Err(Error::HitPositionNaN.into());
        }
        Ok((true, position))
    }
}
fn cell(position: Vector) -> DynamicResult<[i32; 2]> {
    let mut out = [0; 2];
    for i in 0..2 {
        let f = position[i] * (1.0 / CELL_SIZE) + 32.0;
        if !f.is_finite() || !(-2147483648.0..2147483648.0).contains(&f) {
            return Err(DynamicError::CellConversionUndefined);
        }
        out[i] = f as i32;
    }
    Ok(out)
}
fn valid(cell: [i32; 2]) -> bool {
    cell.iter().all(|i| (0..64).contains(i))
}

#[cfg(test)]
mod counter_tests {
    use super::*;
    #[test]
    fn source_signed_unbalanced_counter_refuses_only_at_overflow() {
        let mut value = i32::MAX - 1;
        increment(&mut value).unwrap();
        assert_eq!(value, i32::MAX);
        assert_eq!(
            increment(&mut value),
            Err(DynamicError::UnbalancedCounterOverflow)
        );
        assert_eq!(value, i32::MAX);
    }

    #[test]
    fn removal_counter_refusal_keeps_members_until_the_source_erase() {
        let (mut manager, list, mut models) = super::super::dynamic_tests::ordinary();
        let h = models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0,
                },
                Some(10),
            )
            .unwrap()
            .unwrap();
        let mut tree = DynamicMapTree::new();
        tree.insert(&models, &h).unwrap();
        let last = *tree.members[&h.key()].1.last().unwrap();
        tree.nodes.get_mut(&last).unwrap().unbalanced = i32::MAX;
        assert_eq!(
            tree.remove(&h),
            Err(DynamicError::UnbalancedCounterOverflow)
        );
        // RegularGrid.h:116-119 erases only after every node removal returned.
        assert!(tree.contains(&h));
        assert_eq!(tree.size(), 4);
        assert_eq!(tree.unbalanced(), 1);
        let mut pending = Vec::new();
        tree.nodes[&last].pending.members(&mut pending);
        // BIHWrap.h:78 increments before it removes the pending pointer.
        assert_eq!(pending, std::slice::from_ref(&h));
        let mut control = DynamicMapTree::new();
        control.insert(&models, &h).unwrap();
        control.nodes.get_mut(&last).unwrap().unbalanced = i32::MAX - 1;
        control.remove(&h).unwrap();
        assert_eq!(control.nodes[&last].unbalanced, i32::MAX);
        assert!(!control.contains(&h));
        assert_eq!(control.size(), 0);
        assert_eq!(control.unbalanced(), 2);
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn source_zero_low_word_cardinality_does_not_advance_timer() {
        let mut tree = DynamicMapTree::new();
        tree.size = 1usize << 32;
        tree.unbalanced = 1;
        tree.update(&DynamicModels::default(), 200).unwrap();
        assert_eq!(tree.timer_ms(), 200);
        assert_eq!(tree.unbalanced(), 1);
    }

    #[test]
    fn wrapper_counter_refusal_retains_completed_grid_mutations() {
        let (mut manager, list, mut models) = super::super::dynamic_tests::ordinary();
        let h = models
            .construct(
                &mut manager,
                &list,
                Some(7),
                DynamicModelPose {
                    position: [0.0; 3],
                    orientation: 0.0,
                    scale: 1.0,
                },
                Some(10),
            )
            .unwrap()
            .unwrap();
        let mut insertion = DynamicMapTree::new();
        insertion.unbalanced = i32::MAX;
        // DynamicTree.cpp:66-76 increments only after the base mutation returns.
        assert_eq!(
            insertion.insert(&models, &h),
            Err(DynamicError::UnbalancedCounterOverflow)
        );
        assert!(insertion.contains(&h));
        assert_eq!(insertion.size(), 4);
        assert_eq!(insertion.unbalanced(), i32::MAX);
        for node in insertion.inventory() {
            assert_eq!(node.pending, std::slice::from_ref(&h));
            assert_eq!(node.unbalanced, 1);
        }
        let mut removal = DynamicMapTree::new();
        removal.insert(&models, &h).unwrap();
        removal.unbalanced = i32::MAX;
        assert_eq!(
            removal.remove(&h),
            Err(DynamicError::UnbalancedCounterOverflow)
        );
        // RegularGrid.h:114-119 finished both node removals and memberTable erase.
        assert!(!removal.contains(&h));
        assert_eq!(removal.size(), 0);
        assert_eq!(removal.unbalanced(), i32::MAX);
        for node in removal.inventory() {
            assert!(node.pending.is_empty());
            assert_eq!(node.unbalanced, 2);
        }
    }
}
