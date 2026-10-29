// SPDX-License-Identifier: GPL-3.0-or-later
//! Source constructor-stage cell membership only; no live registration/lookup.
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
        // Source Map::AddToGrid ordinary Creature: insert reference, THEN assign
        // CurrentCell. Stable boxed entry retains the exact one-shot object.
        store.insert(
            0,
            Box::new(GridCreature {
                fresh: Some(object),
                attached: None,
            }),
        );
        let entry = store.first_mut().ok_or(NativeError::AuthorityBinding)?;
        let fresh = entry.fresh.take().ok_or(NativeError::AuthorityBinding)?;
        entry.attached =
            Some(fresh.attach_initial_cell(self.slot.map_id, self.slot.instance_id, cell)?);
        Ok(())
    }
    pub(crate) fn initial_creatures(&self) -> impl Iterator<Item = &AttachedInitialCreatureCell> {
        self.nodes
            .values()
            .flat_map(|node| node.cells.iter().flatten())
            .flat_map(|cell| &cell.creatures)
            .filter_map(|entry| entry.attached.as_ref())
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
