// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors, upstream GPL-2.0-or-later.
//! Map::Update membership differs from camera visibility and Cell::VisitCircle.
use super::*;
use benilla_sim::resident_map::{normalized_cell_pair, CellPair};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceUpdateEvent {
    Cell { pair: CellPair, loaded: bool },
    SuppressedCell(CellPair),
    Insert(UpdateInsertReceipt),
    Traversal { members: usize, buckets: usize },
    Yield { object: ObjectHandle, bucket: usize },
    UpdateBegin(ObjectHandle),
    UpdateEnd(ObjectHandle),
    TraversalEnd,
}
/// Actual visibility data and PLAYER_FARSIGHT owners, not recorded memberships.
pub trait SourceVisitInputs {
    fn visibility_distance(&mut self, object: ObjectHandle) -> Result<f32>;
    fn farsight_guid(&mut self, player: ObjectHandle) -> Result<SourceGuid>;
}
fn valid_position(pose: SourcePose) -> bool {
    let [x, y, z, o] = pose.0.map(f32::from_bits);
    // GridDefines.h:38,56-57,177-194; the subtraction promotes to double.
    let half = f32::from_bits(0x4405_5555) * 64.0_f32 / 2.0_f32;
    [x, y, z, o].into_iter().all(f32::is_finite)
        && f64::from(x.abs()) <= f64::from(half) - 0.5
        && f64::from(y.abs()) <= f64::from(half) - 0.5
}
struct Membership {
    set: SourceUpdateSet,
}
impl Membership {
    fn visit(
        &mut self,
        pose: SourcePose,
        radius: f32,
        objects: &SourceObjects,
        cells: &mut SourceCells,
        observe: &mut impl FnMut(SourceUpdateEvent) -> Result<()>,
    ) -> Result<()> {
        let [x, y, _, _] = pose.0.map(f32::from_bits);
        require(
            x.is_finite() && y.is_finite() && radius.is_finite(),
            "execution.visitors.cell_area_numeric",
        )?;
        // CellImpl.h:37-49. No bounding radius, cap, standing-first or octagon.
        let (low, high) = if radius <= 0.0 {
            let center = normalized_cell_pair(x.to_bits(), y.to_bits());
            (center, center)
        } else {
            (
                normalized_cell_pair((x - radius).to_bits(), (y - radius).to_bits()),
                normalized_cell_pair((x + radius).to_bits(), (y + radius).to_bits()),
            )
        };
        let low = low.map_err(|_| refuse("execution.visitors.cell_area_numeric"))?;
        let high = high.map_err(|_| refuse("execution.visitors.cell_area_numeric"))?;
        for x in low.x()..=high.x() {
            for y in low.y()..=high.y() {
                let pair = CellPair::from_indices(x, y)
                    .map_err(|_| refuse("execution.visitors.cell_coordinates"))?;
                if !cells.mark_update_cell(pair) {
                    observe(SourceUpdateEvent::SuppressedCell(pair))?;
                    continue;
                }
                observe(SourceUpdateEvent::Cell {
                    pair,
                    loaded: cells.object_data_loaded(pair.grid_x(), pair.grid_y()),
                })?;
                // Mark even a cold cell. NoCreate must never reach a loader;
                // Map.cpp:678-686, GridNotifiers.h:119-122.
                for container in [CellContainer::Grid, CellContainer::World] {
                    if let Some(members) = cells.visit_no_create(pair, container)? {
                        for object in members {
                            let overload = match objects.get(object)?.kind {
                                SourceObjectKind::Creature | SourceObjectKind::Pet => {
                                    UpdateInsertion::Creature
                                }
                                SourceObjectKind::GameObject
                                | SourceObjectKind::Transport
                                | SourceObjectKind::DynamicObject => UpdateInsertion::Generic,
                                // Both bones and resurrectable corpses have CorpseMapType.
                                SourceObjectKind::Player
                                | SourceObjectKind::Corpse
                                | SourceObjectKind::Bones
                                | SourceObjectKind::Camera => continue,
                                SourceObjectKind::Item | SourceObjectKind::Container => {
                                    return Err(refuse("execution.visitors.invalid_cell_type"))
                                }
                            };
                            observe(SourceUpdateEvent::Insert(
                                self.set.insert(objects, object, overload)?,
                            ))?;
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
/// Collection starts after messages/spawn-manager/player updates. Transport and
/// player Updates remain separate reached owners in Map::Update, not this set.
/// Disabled-playerbots source branch only, as in the pinned F1 configuration.
pub fn collect_source_updates(
    objects: &SourceObjects,
    cells: &mut SourceCells,
    map_visibility: f32,
    inputs: &mut impl SourceVisitInputs,
    observe: &mut impl FnMut(SourceUpdateEvent) -> Result<()>,
) -> Result<SourceUpdateSet> {
    require(
        map_visibility.is_finite(),
        "execution.visitors.map_visibility",
    )?;
    let map = cells.map();
    cells.reset_update_marks();
    let mut membership = Membership {
        set: SourceUpdateSet::new(map)?,
    };
    // Map.cpp:894-927. A farsight target is resolved, not accepted as an order.
    for player in objects.map_players(map) {
        let object = objects.get(player)?;
        if !object.in_world || !valid_position(objects.pose(player)?) {
            continue;
        }
        membership.visit(
            objects.pose(player)?,
            inputs.visibility_distance(player)?,
            objects,
            cells,
            observe,
        )?;
        if let Some(target) = objects.lookup_world_object(map, inputs.farsight_guid(player)?) {
            let radius = if objects.get(target)?.in_world {
                inputs.visibility_distance(target)?
            } else {
                map_visibility
            };
            membership.visit(objects.pose(target)?, radius, objects, cells, observe)?;
        }
    }
    // Map.cpp:945-992. Direct insertion precedes that actor's cell visits,
    // uses the map radius, and advances the live iterator before processing.
    cells.active_mut().begin_visit()?;
    let result = (|| {
        while let Some(object) = cells.active_mut().next_member()? {
            if !objects.get(object)?.in_world || !valid_position(objects.pose(object)?) {
                continue;
            }
            observe(SourceUpdateEvent::Insert(membership.set.insert(
                objects,
                object,
                UpdateInsertion::Active,
            )?))?;
            membership.visit(
                objects.pose(object)?,
                map_visibility,
                objects,
                cells,
                observe,
            )?;
        }
        Ok(membership.set)
    })();
    cells.active_mut().finish_visit();
    result
}
