// SPDX-License-Identifier: GPL-3.0-or-later
// GNU libstdc++ GCC 12.2 translation, copyright FSF 2007-2022;
// upstream GPL-3.0-or-later WITH GCC-exception-3.1; see runtime README.
//! Logical nodes implement the pinned pointer set, never a supplied permutation.
use super::*;

use super::identity_table::IdentityTable;
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
pub struct SourceUpdateSet {
    map: MapIdentity,
    table: IdentityTable<ObjectHandle>,
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
            table: IdentityTable::default(),
            completed: false,
        })
    }
    pub fn len(&self) -> usize {
        self.table.len()
    }
    pub fn is_empty(&self) -> bool {
        self.table.len() == 0
    }
    pub fn bucket_count(&self) -> usize {
        self.table.bucket_count()
    }
    fn bucket(&self, pointer: u64) -> usize {
        self.table.bucket(pointer)
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
        if let Some(previous) = self.table.get(pointer) {
            require(
                *previous == object,
                "execution.update_set.stale_pointer_generation",
            )?;
        }
        require(
            self.table.get(pointer).is_some() || self.len() < UPDATE_SET_MEMBER_LIMIT,
            "execution.update_set.member_limit",
        )?;
        let inserted = self
            .table
            .insert(pointer, object, UPDATE_SET_MEMBER_LIMIT)?;
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
        self.table.iter().map(|(_, object)| *object)
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
        for (pointer, &handle) in self.table.iter() {
            let object = objects.get(handle)?;
            require(
                object.map == Some(self.map) && object.source_allocation == Some(pointer),
                "execution.update_set.stale_dispatch_generation",
            )?;
            observe(SourceUpdateEvent::Yield {
                object: handle,
                bucket: self.bucket(pointer),
            })?;
            observe(SourceUpdateEvent::UpdateBegin(handle))?;
            owner.update(handle, diff, objects, cells)?;
            observe(SourceUpdateEvent::UpdateEnd(handle))?;
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
