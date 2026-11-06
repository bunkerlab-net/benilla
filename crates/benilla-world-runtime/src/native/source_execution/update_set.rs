// SPDX-License-Identifier: GPL-3.0-or-later
// GNU libstdc++ GCC 12.2 translation, copyright FSF 2007-2022;
// upstream GPL-3.0-or-later WITH GCC-exception-3.1; see runtime README.
//! Logical nodes implement the pinned pointer set, never a supplied permutation.
use super::*;

// Default load factor one, no reserve/erase: hashtable_c++0x.cc:102-122,
// hashtable-aux.cc:30-45. Refuse beyond the supported resource domain.
const GROWTH: &[usize] = &[
    1, 13, 29, 59, 127, 257, 541, 1109, 2357, 5087, 10273, 20753, 42043, 85229, 172933,
];
pub const UPDATE_SET_MEMBER_LIMIT: usize = 100_000;
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum UpdateInsertion {
    Generic,
    Creature,
    Active,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct UpdateInsertReceipt {
    pub object: ObjectHandle,
    pub overload: UpdateInsertion,
    pub inserted: bool,
    pub buckets_before: usize,
    pub buckets_after: usize,
    pub bucket: usize,
    pub hash: u64,
}
#[derive(Debug)]
struct Node {
    object: ObjectHandle,
    pointer: u64,
    next: Option<usize>,
}
const BEFORE_BEGIN: usize = usize::MAX;
#[derive(Debug)]
pub struct SourceUpdateSet {
    map: MapIdentity,
    // Bucket predecessors use BEFORE_BEGIN for the inline sentinel.
    head: Option<usize>,
    nodes: Vec<Node>,
    buckets: Vec<Option<usize>>,
    growth: usize,
    completed: bool,
}
impl SourceUpdateSet {
    pub fn new(map: MapIdentity) -> Result<Self> {
        require(
            map.creation_generation > 0,
            "execution.update_set.map_generation",
        )?;
        Ok(Self {
            map,
            head: None,
            nodes: Vec::new(),
            buckets: Vec::new(),
            growth: 0,
            completed: false,
        })
    }
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn bucket_count(&self) -> usize {
        self.buckets.len().max(1)
    }
    fn bucket(&self, pointer: u64) -> usize {
        // functional_hash.h:107-112; hashtable_policy.h:478-488.
        (pointer % self.bucket_count() as u64) as usize
    }
    fn next(&self, previous: usize) -> Option<usize> {
        if previous == BEFORE_BEGIN {
            self.head
        } else {
            self.nodes[previous].next
        }
    }
    fn set_next(&mut self, previous: usize, next: Option<usize>) {
        if previous == BEFORE_BEGIN {
            self.head = next;
        } else {
            self.nodes[previous].next = next;
        }
    }
    fn insert_bucket_begin(&mut self, index: usize) {
        let bucket = self.bucket(self.nodes[index].pointer);
        if let Some(previous) = self.buckets[bucket] {
            self.nodes[index].next = self.next(previous);
            self.set_next(previous, Some(index));
        } else {
            self.nodes[index].next = self.head;
            self.head = Some(index);
            if let Some(next) = self.nodes[index].next {
                let former = self.bucket(self.nodes[next].pointer);
                self.buckets[former] = Some(index);
            }
            self.buckets[bucket] = Some(BEFORE_BEGIN);
        }
    }
    // Consume the old chain, reinserting at bucket heads; hashtable.h:2560-2591.
    fn rehash(&mut self) {
        self.growth += 1;
        self.buckets = vec![None; GROWTH[self.growth]];
        let mut current = self.head.take();
        while let Some(index) = current {
            current = self.nodes[index].next;
            self.insert_bucket_begin(index);
        }
    }
    pub fn insert(
        &mut self,
        objects: &SourceObjects,
        object: ObjectHandle,
        overload: UpdateInsertion,
    ) -> Result<UpdateInsertReceipt> {
        let value = objects.get(object)?;
        require(
            value.map == Some(self.map),
            "execution.update_set.object_map_generation",
        )?;
        let pointer = value
            .source_allocation
            .ok_or_else(|| refuse("execution.update_set.source_allocation_owner"))?;
        self.insert_token(object, pointer, overload)
    }
    // Diagnostic-only token seam. Gameplay must use live SourceObjects above.
    pub(super) fn insert_token(
        &mut self,
        object: ObjectHandle,
        pointer: u64,
        overload: UpdateInsertion,
    ) -> Result<UpdateInsertReceipt> {
        require(!self.completed, "execution.update_set.collection_closed")?;
        require(
            pointer != 0
                && pointer.is_multiple_of(8)
                && object.thread > 0
                && object.construction > 0,
            "execution.update_set.allocation_token",
        )?;
        let before = self.bucket_count();
        let bucket = self.bucket(pointer);
        let mut current = self
            .buckets
            .get(bucket)
            .copied()
            .flatten()
            .and_then(|previous| self.next(previous));
        let mut inserted = true;
        while let Some(index) = current {
            let node = &self.nodes[index];
            if self.bucket(node.pointer) != bucket {
                break;
            }
            if node.pointer == pointer {
                require(
                    node.object == object,
                    "execution.update_set.stale_pointer_generation",
                )?;
                inserted = false;
                break;
            }
            current = node.next;
        }
        // Duplicate detection precedes the growth decision for insert and emplace;
        // hashtable.h:2059-2083,2144-2164,2231-2256.
        if inserted {
            require(
                self.len() < UPDATE_SET_MEMBER_LIMIT,
                "execution.update_set.member_limit",
            )?;
            if self.growth == 0 || self.len() >= self.bucket_count() {
                self.rehash();
            }
            let index = self.nodes.len();
            self.nodes.push(Node {
                object,
                pointer,
                next: None,
            });
            self.insert_bucket_begin(index);
        }
        Ok(UpdateInsertReceipt {
            object,
            overload,
            inserted,
            buckets_before: before,
            buckets_after: self.bucket_count(),
            bucket: self.bucket(pointer),
            hash: pointer,
        })
    }
    pub fn order(&self) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.indices().map(|index| self.nodes[index].object)
    }
    fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.head, |&index| self.nodes[index].next)
    }
    pub fn prove_order(&self, expected: &[ObjectHandle]) -> Result<()> {
        require(
            self.order().eq(expected.iter().copied()),
            "execution.update_set.pinned_iteration_mismatch",
        )
    }
    // Map.cpp:995-1000 does not re-filter IsInWorld, visibility or cell membership.
    // Destruction/reuse of a collected pointer is illegal, not permission to skip.
    /// Retain the returned closed set until Map::Update's scope ends, after
    /// scripts/weather, rather than freeing source node lifetimes at loop end.
    pub fn dispatch(
        mut self,
        diff: u32,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
        owner: &mut impl SourceObjectUpdateOwner,
        observe: &mut impl FnMut(SourceUpdateEvent) -> Result<()>,
    ) -> Result<Self> {
        require(!self.completed, "execution.update_set.traversal_repeated")?;
        self.completed = true;
        require(
            cells.map() == self.map,
            "execution.update_set.dispatch_map_generation",
        )?;
        observe(SourceUpdateEvent::Traversal {
            members: self.len(),
            buckets: self.bucket_count(),
        })?;
        for index in self.indices() {
            let node = &self.nodes[index];
            let object = objects.get(node.object)?;
            require(
                object.map == Some(self.map) && object.source_allocation == Some(node.pointer),
                "execution.update_set.stale_dispatch_generation",
            )?;
            observe(SourceUpdateEvent::Yield {
                object: node.object,
                bucket: self.bucket(node.pointer),
            })?;
            observe(SourceUpdateEvent::UpdateBegin(node.object))?;
            owner.update(node.object, diff, objects, cells)?;
            observe(SourceUpdateEvent::UpdateEnd(node.object))?;
        }
        observe(SourceUpdateEvent::TraversalEnd)?;
        Ok(self)
    }
}
/// Reached entity Update implementations must execute, not acknowledge a receipt.
pub trait SourceObjectUpdateOwner {
    fn update(
        &mut self,
        object: ObjectHandle,
        diff: u32,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
    ) -> Result<()>;
}
