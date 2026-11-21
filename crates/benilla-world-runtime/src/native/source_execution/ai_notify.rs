// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Unit.cpp:10926-10950. The shared Unit queue owns the BasicEvent lifetime.
use super::*;

/// Live Unit/Creature operands, not recorded callback results.
#[derive(Clone, Debug)]
pub struct SourceAiNotifyState {
    pub detection_range: u32,
    pub is_player: bool,
    pub alive: bool,
    pub can_aggro: bool,
}

/// Cell::VisitAllObjects receives PlayerVisitObjectsNotifier for players and
/// CreatureVisitObjectsNotifier for creatures, Unit.cpp:10931-10942.
pub fn execute_source_ai_notify(
    state: &mut SourceAiNotifyState,
    config: &[u8],
    mut visit: impl FnMut(bool, f32) -> Result<()>,
) -> Result<()> {
    let (_, value) = spawn_loader_config::assignment(config, "Rate.Creature.Aggro")?;
    let rate = value
        .parse::<f32>()
        .map_err(|_| refuse("execution.creature.Unit_10930.aggro_config"))?;
    // World.cpp:443,2420-2427 resets a negative rate to one.
    let rate = if rate < 0.0 { 1.0 } else { rate };
    // Unit.h:1002,2393; Creature.h:870,879. std::max selects uint32 before float conversion.
    let radius = state.detection_range.max(45) as f32 * rate;
    if !state.is_player {
        state.can_aggro = state.alive;
    }
    visit(state.is_player, radius)
}

/// Live creature callback binding. The Unit queue supplies this owner only at Execute.
pub struct SourceCreatureAiNotifyOwner<'a> {
    pub object: ObjectHandle,
    pub objects: &'a SourceObjects,
    pub cells: &'a SourceCells,
    pub detection_range: u32,
    pub bounding_radius_bits: u32,
    pub can_aggro: &'a mut bool,
    pub config: &'a [u8],
}
impl UnitEventOwner for SourceCreatureAiNotifyOwner<'_> {
    fn forced_despawn(&mut self, _: bool) -> Result<()> {
        Err(refuse("execution.unit_events.forced_despawn_owner"))
    }
    fn ai_notify(&mut self) -> Result<()> {
        let object = self.objects.get(self.object)?;
        require(
            matches!(
                object.kind,
                SourceObjectKind::Creature | SourceObjectKind::Pet
            ),
            "execution.creature.ai_notify_type_binding",
        )?;
        let mut state = SourceAiNotifyState {
            detection_range: self.detection_range,
            is_player: false,
            alive: object.alive,
            can_aggro: *self.can_aggro,
        };
        execute_source_ai_notify(&mut state, self.config, |_, radius| {
            *self.can_aggro = object.alive;
            visit_creature_objects(
                self.objects,
                self.cells,
                self.object,
                radius,
                self.bounding_radius_bits,
            )
        })
    }
}

fn visit_creature_objects(
    objects: &SourceObjects,
    cells: &SourceCells,
    creature: ObjectHandle,
    radius: f32,
    bounding_radius_bits: u32,
) -> Result<()> {
    // Cell.h:114 defaults NoCreate; CellImpl.h:202-209 visits grid then world.
    let pose = objects.pose(creature)?.0;
    let standing = benilla_sim::resident_map::compute_cell_pair(pose[0], pose[1])
        .map_err(|_| refuse("execution.cell_coordinates"))?;
    for container in [CellContainer::Grid, CellContainer::World] {
        let pairs = benilla_sim::resident_map::source_cell_visit(
            standing,
            pose[0],
            pose[1],
            radius.to_bits(),
            bounding_radius_bits,
        )
        .map_err(|_| refuse("execution.cell_visit_numeric"))?;
        for pair in pairs {
            if !cells.object_data_loaded(pair.grid_x(), pair.grid_y()) {
                continue;
            }
            let cell = cells.cell(pair)?;
            let slots: &[usize] = if container == CellContainer::Grid {
                &[1]
            } else {
                &[4, 5]
            };
            for &slot in slots {
                // GridNotifiersImpl.h:150,169 checks the owner before each type list.
                if !objects.get(creature)?.alive {
                    continue;
                }
                let mut next = cells.first_in_slot(cell, slot)?;
                while let Some(other) = next {
                    if slot == 4 {
                        if objects.get(other)?.alive {
                            return Err(refuse(
                                "execution.creature.GridNotifiersImpl_156.IsTaxiFlying_owner",
                            ));
                        }
                    } else if other != creature && objects.get(other)?.alive {
                        return Err(refuse(
                            "execution.creature.GridNotifiersImpl_178.UnitVisitObjectsNotifierWorker_owner",
                        ));
                    }
                    next = cells.next_in_slot(other, cell, slot)?;
                }
            }
        }
    }
    Ok(())
}
