// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Transports.cpp:442-487; GPL-2.0-or-later.
use super::*;
use std::sync::Arc;

#[derive(Debug)]
pub struct SourceElevator {
    pub animation: Option<Arc<SourceTransportAnimation>>,
    pub stationary: SourcePose,
    pub progress: u32,
    pub movement_started: u32,
    pub current_segment: u32,
}
impl SourceElevator {
    pub fn new(animation: Option<Arc<SourceTransportAnimation>>, stationary: SourcePose) -> Self {
        Self {
            animation,
            stationary,
            progress: 0,
            movement_started: 0,
            current_segment: 0,
        }
    }
    pub fn position(
        &mut self,
        snapshot: u32,
        rotation: [u32; 4],
        orientation: u32,
    ) -> Result<Option<SourcePose>> {
        let Some(animation) = &self.animation else {
            return Ok(None);
        };
        // Unsigned subtraction precedes modulo, with zero total time undefined; Transports.cpp:459.
        require(
            animation.total_time != 0,
            "execution.elevator.period_modulo_zero_undefined",
        )?;
        self.progress = snapshot.wrapping_sub(self.movement_started) % animation.total_time;
        // lower_bound is inclusive for next, strictly earlier for prev, with no endpoint wrap; TransportMgr.cpp:314-332.
        let Some((&next_time, next)) = animation.nodes.range(self.progress..).next() else {
            return Ok(None);
        };
        let Some((&previous_time, previous)) = animation.nodes.range(..next_time).next_back()
        else {
            return Ok(None);
        };
        self.current_segment = previous_time;
        let position = benilla_sim::source_transports::elevator_position(
            previous.position,
            next.position,
            self.progress,
            previous_time,
            next_time,
            rotation,
            self.stationary.0[..3].try_into().unwrap(),
        )
        .map_err(|_| refuse("execution.elevator.numeric_domain"))?;
        Ok(Some(SourcePose([
            position[0],
            position[1],
            position[2],
            orientation,
        ])))
    }
}

pub(crate) fn update_source_elevator<
    D: GridDefinitions,
    P: ResidencyProductOwner,
    F: GameObjectLifecycleOwner<D, P>,
>(
    owner: &mut F,
    object: ObjectHandle,
    grid: &mut GridFactoryContext<'_, D, P>,
) -> Result<()> {
    if owner
        .gameobjects()
        .get(object)?
        .elevator
        .as_ref()
        .unwrap()
        .animation
        .is_none()
    {
        return Ok(());
    }
    let snapshot = owner.elevator_map_millis(grid.cells.map())?;
    let value = owner.gameobjects().get_mut(object)?;
    let elevator = value.elevator.as_mut().unwrap();
    let Some(pose) = elevator.position(
        snapshot,
        value.rotation_bits,
        grid.objects.pose(object)?.0[3],
    )?
    else {
        return Ok(());
    };
    let progress = elevator.progress;
    // Map relocation completes model and visibility work before passengers, even when the cell move declines; Transports.cpp:483-486.
    grid.move_gameobject(object, pose, true, owner)?;
    let value = grid.objects.get(object)?;
    let transport = PassengerTransport {
        handle: object,
        guid: value
            .guid
            .ok_or_else(|| refuse("execution.object_guid_absent"))?
            .raw(),
        progress,
        pose: grid.objects.pose(object)?,
        map: value.map,
    };
    let mut passengers = std::mem::take(owner.elevator_passengers()?);
    let GridFactoryContext {
        definitions,
        products,
        sets,
        objects,
        cells,
    } = grid;
    let result = passengers.update_positions_with(transport, objects, &mut |effect, objects| {
        owner.elevator_passenger_effect(
            effect,
            &mut GridFactoryContext {
                definitions,
                products,
                sets,
                objects,
                cells,
            },
        )
    });
    *owner.elevator_passengers()? = passengers;
    result
}
