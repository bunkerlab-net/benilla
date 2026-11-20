// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

/// Live callbacks for relocation within the loaded map. A cold grid requires the reentrant factory.
pub struct SourceLoadedRelocation<'a> {
    pub units: &'a mut SourceUnits,
    pub config: SourceUnitRelocationConfig,
    pub visibility: &'a mut SourceVisibilityCensus,
}
impl<D: GridDefinitions, P: GridProductOwner> GridObjectFactory<D, P>
    for SourceLoadedRelocation<'_>
{
    fn load_from_db(
        &mut self,
        _: GridLoadAttempt,
        _: D::Row,
        _: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<GridCandidateOutcome> {
        Err(refuse(
            "execution.relocation.ObjectGridLoader_138.LoadFromDB_owner",
        ))
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(refuse(
            "execution.relocation.ObjectGridLoader_149.Event_AddedToWorld_owner",
        ))
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(refuse(
            "execution.relocation.ObjectGridLoader_157.OnObjectDBLoad_owner",
        ))
    }
}
impl<D: GridDefinitions, P: GridProductOwner> ActiveRelocationOwner<D, P>
    for SourceLoadedRelocation<'_>
{
    fn allow_grid_load_at_relocation(
        &self,
        cells: &SourceCells,
        target: benilla_sim::resident_map::CellPair,
    ) -> Result<()> {
        require(
            cells.object_data_loaded(target.grid_x(), target.grid_y()),
            "execution.relocation.Map_1229.EnsureGridLoadedAtEnter_owner",
        )
    }
    fn relocation_phase(
        &mut self,
        object: ObjectHandle,
        phase: RelocationPhase,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        match phase {
            RelocationPhase::ViewpointGridChanged { new, .. } => context
                .objects
                .viewpoint_grid_changed(object, new, context.cells),
            RelocationPhase::CreatureOnRelocated => context.objects.unit_on_relocated(
                object,
                context.cells,
                self.units,
                self.config,
                self.visibility,
            ),
            RelocationPhase::CombatStopWithPets => Err(refuse(
                "execution.relocation.Map_1179.CreatureRespawnRelocation_owner",
            )),
            _ => Err(phase.missing_owner()),
        }
    }
}
