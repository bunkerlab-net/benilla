// SPDX-License-Identifier: GPL-3.0-or-later
//! Source constructor cell + dual-index prefix; no completed/live registration.
use super::*;
use benilla_world_runtime::{
    native::{Authority, NativeError, NativeWorld},
    NpcId, PlayerId, WorldId,
};

#[derive(Debug)]
pub(crate) enum InitialCellError {
    Boundary(GenerationError),
    Constructor(NativeError),
}
impl<'terrain, 'bundle, 'source> FreshSingleGrid<'terrain, 'bundle, 'source> {
    /// Consume the complete grid transaction. Any constructor error drops every
    /// staged cell entry and geometry; no partially populated owner is returned.
    /// The callback sees constructor-stage membership only. Moving or retaining
    /// the returned NativeWorld does not retain registration or CurrentCell.
    #[allow(
        clippy::too_many_arguments,
        reason = "explicit native constructor operands and scoped consumer"
    )]
    pub(crate) fn with_initial_creatures<R>(
        mut self,
        profile: Profile,
        world: WorldId,
        player: PlayerId,
        seed: u64,
        authority: &mut dyn Authority,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        consume: impl FnOnce(&Self, NativeWorld, SourceMapQuerySnapshot) -> R,
    ) -> Result<R, InitialCellError> {
        poll(deadline, &cancelled).map_err(InitialCellError::Boundary)?;
        if profile.digest() != self.profile_identity {
            return Err(InitialCellError::Constructor(NativeError::AuthorityBinding));
        }
        let native = NativeWorld::synthetic_component_test_with_initialization_and_fresh_creatures(
            profile,
            world,
            player,
            seed,
            None,
            authority,
            &mut |object| {
                self.ensure(object.initial_position_bits(), deadline, &cancelled)
                    .map_err(|_| NativeError::AuthorityBinding)?;
                self.insert_initial_creature(object, world)
            },
        )
        .map_err(InitialCellError::Constructor)?;
        self.height()
            .verify_unchanged(|| cancelled() || Instant::now() >= deadline)
            .map_err(|e| InitialCellError::Boundary(GenerationError::Composition(e)))?;
        poll(deadline, &cancelled).map_err(InitialCellError::Boundary)?;
        self.with_query(|grid, query| consume(grid, native, query))
            .map_err(InitialCellError::Boundary)
    }
    fn insert_initial_creature(
        &mut self,
        object: FreshCreatureCellObject,
        world: WorldId,
    ) -> Result<(), NativeError> {
        if object.profile_identity() != self.profile_identity
            || object.world() != world
            || object.map_id() != self.slot.map_id
            || object.instance_id() != self.slot.instance_id
            || object.is_active()
            || object.viewer_count() != 0
            || object.viewpoint_grid().is_some()
        {
            return Err(NativeError::AuthorityBinding);
        }
        if object.identity().guid() >> 48 != 0xf130 || object.identity().database_guid() == 0 {
            return Err(NativeError::AuthorityBinding);
        }
        let cell = object.cell();
        let key = (cell.grid_x(), cell.grid_y());
        if self.linked.as_slice() != [key] || self.nodes.len() != 1 {
            return Err(NativeError::AuthorityBinding);
        }
        if self.initial_creatures().any(|old| {
            old.object().npc() == object.npc()
                || old.object().identity().guid() == object.identity().guid()
        }) {
            return Err(NativeError::AuthorityBinding);
        }
        let node = self
            .nodes
            .get_mut(&key)
            .ok_or(NativeError::AuthorityBinding)?;
        if node.state != GridState::Idle || node.terrain.is_none() || node.object_data_loaded {
            return Err(NativeError::AuthorityBinding);
        }
        let store =
            &mut node.cells[usize::from(cell.x() % 16)][usize::from(cell.y() % 16)].creatures;
        // Source Map::AddToGrid inserts first, then sets CurrentCell. All
        // subsequent map indexes share this same stable allocation, not copies.
        let entry = Rc::new(GridCreature {
            fresh: RefCell::new(Some(object)),
            attached: OnceCell::new(),
        });
        store.insert(0, Rc::clone(&entry));
        let fresh = entry
            .fresh
            .borrow_mut()
            .take()
            .ok_or(NativeError::AuthorityBinding)?;
        entry
            .attached
            .set(fresh.attach_initial_cell(self.slot.map_id, self.slot.instance_id, cell)?)
            .map_err(|_| NativeError::AuthorityBinding)?;
        let identity = entry
            .attached
            .get()
            .ok_or(NativeError::AuthorityBinding)?
            .object()
            .identity();
        // Creature::AddToWorld's fresh ordinary prefix: GUID store, then DBGUID
        // append. This does not set inWorld or implement the rest of AddToWorld.
        insert_guid(&mut self.initial_guids, identity.guid(), Rc::clone(&entry))?;
        append_database_guid(
            &mut self.initial_dbguids,
            0xf130,
            identity.database_guid(),
            entry,
        );

        Ok(())
    }
    pub(crate) fn initial_creatures(&self) -> impl Iterator<Item = &AttachedInitialCreatureCell> {
        self.nodes
            .values()
            .flat_map(|node| node.cells.iter().flatten())
            .flat_map(|cell| &cell.creatures)
            .filter_map(|entry| entry.attached.get())
    }
    /// Borrowed constructor-prefix candidate only, not Map::GetUnit or live lookup.
    pub(crate) fn initial_guid_candidate(&self, guid: u64) -> Option<&AttachedInitialCreatureCell> {
        self.initial_guids
            .get(&guid)
            .and_then(|entry| entry.attached.get())
    }
    /// Source DBGUID storage is an append-ordered vector. This is not the alive-
    /// first GetCreature(dbGuid) operation, whose lifecycle state is unowned here.
    pub(crate) fn initial_database_candidates(
        &self,
        parent_high: u16,
        dbguid: u32,
    ) -> impl Iterator<Item = &AttachedInitialCreatureCell> {
        self.initial_dbguids
            .get(&(parent_high, dbguid))
            .into_iter()
            .flatten()
            .filter_map(|entry| entry.attached.get())
    }
    pub(crate) fn initial_creature_count(&self) -> usize {
        self.initial_creatures().count()
    }
    pub(crate) fn initial_cell_for(&self, npc: NpcId) -> Option<CellPair> {
        self.initial_creatures()
            .find(|entry| entry.object().npc() == npc)
            .map(|entry| entry.current_cell())
    }
}

// TypeContainer.h: a repeat key may identify the same object, never replace it.
fn insert_guid<T>(
    store: &mut BTreeMap<u64, Rc<T>>,
    guid: u64,
    entry: Rc<T>,
) -> Result<bool, NativeError> {
    use std::collections::btree_map::Entry;
    match store.entry(guid) {
        Entry::Vacant(slot) => {
            slot.insert(entry);
            Ok(true)
        }
        Entry::Occupied(slot) if Rc::ptr_eq(slot.get(), &entry) => Ok(false),
        Entry::Occupied(_) => Err(NativeError::AuthorityBinding),
    }
}
fn append_database_guid<T>(
    store: &mut BTreeMap<(u16, u32), Vec<Rc<T>>>,
    parent_high: u16,
    dbguid: u32,
    entry: Rc<T>,
) {
    store.entry((parent_high, dbguid)).or_default().push(entry);
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Pure container semantics only: these unit allocations are NOT creatures.
    /// Genuine token/cell/index composition is covered by the app constructor test.
    #[test]
    fn source_guid_container_preserves_same_allocation_and_rejects_replacement() {
        let first = Rc::new(());
        let other = Rc::new(());
        let mut store = BTreeMap::new();
        assert_eq!(insert_guid(&mut store, 7, Rc::clone(&first)), Ok(true));
        assert_eq!(insert_guid(&mut store, 7, Rc::clone(&first)), Ok(false));
        assert_eq!(
            insert_guid(&mut store, 7, other),
            Err(NativeError::AuthorityBinding)
        );
        assert_eq!(store.len(), 1);
        assert!(Rc::ptr_eq(&store[&7], &first));
    }
    #[test]
    fn source_database_container_appends_references_in_order_without_deduplication() {
        let first = Rc::new(());
        let other = Rc::new(());
        let mut store = BTreeMap::new();
        append_database_guid(&mut store, 0xf130, 7, Rc::clone(&first));
        append_database_guid(&mut store, 0xf130, 7, Rc::clone(&other));
        append_database_guid(&mut store, 0xf130, 7, Rc::clone(&first));
        let entries = &store[&(0xf130, 7)];
        assert_eq!(entries.len(), 3);
        assert!(Rc::ptr_eq(&entries[0], &first));
        assert!(Rc::ptr_eq(&entries[1], &other));
        assert!(Rc::ptr_eq(&entries[2], &first));
        assert!(!store.contains_key(&(0xf140, 7)));
    }
}
