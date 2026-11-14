// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Transports.cpp:206-347,498-618; GPL-3.0-or-later.
use super::*;
use benilla_sim::source_transports::{passenger_offset, passenger_position};
use std::collections::BTreeMap;
use std::ops::Bound::{Excluded, Unbounded};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PassengerMovement {
    pub transport: Option<ObjectHandle>,
    pub guid: u64,
    pub time: u32,
    pub offset: SourcePose,
    pub on_transport: bool,
    pub movement_pose: SourcePose,
    pub ghost: bool,
    pub pet: Option<ObjectHandle>,
    pub mini_pet: Option<ObjectHandle>,
}
#[derive(Clone, Copy, Debug)]
pub struct PassengerTransport {
    pub handle: ObjectHandle,
    pub guid: u64,
    pub progress: u32,
    pub pose: SourcePose,
    pub map: Option<MapIdentity>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PassengerEffect {
    PetNearTeleport {
        pet: ObjectHandle,
        pose: SourcePose,
    },
    Resurrect {
        player: ObjectHandle,
        health: u32,
    },
    PlayerTeleport {
        player: ObjectHandle,
        map: u32,
        offset: SourcePose,
        transport: ObjectHandle,
    },
    CreatureRemove {
        creature: ObjectHandle,
    },
    CreatureRelocate {
        creature: ObjectHandle,
        pose: SourcePose,
    },
    PlayerRelocate {
        player: ObjectHandle,
        pose: SourcePose,
    },
    Visibility {
        transport: ObjectHandle,
        player: ObjectHandle,
        create: bool,
    },
    BuildOutOfRange {
        transport: ObjectHandle,
    },
}
impl PassengerEffect {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::PetNearTeleport { .. } => "execution.transport.f5_pet_near_teleport_owner",
            Self::Resurrect { .. } => "execution.transport.f5_player_resurrection_owner",
            Self::PlayerTeleport { .. } => "execution.transport.f5_player_teleport_owner",
            Self::CreatureRemove { .. } => "execution.transport.f3_creature_remove_list_owner",
            Self::CreatureRelocate { .. } => "execution.transport.f3_creature_relocation_owner",
            Self::PlayerRelocate { .. } => "execution.transport.f5_player_relocation_owner",
            Self::Visibility { .. } | Self::BuildOutOfRange { .. } => {
                "execution.transport.f5_player_visibility_owner"
            }
        })
    }
    /// Route the reached creature calls through the existing F3 map owners.
    pub fn creature<
        D: GridDefinitions,
        P: ResidencyProductOwner,
        F: ResidencyOwner<D, P> + ActiveRelocationOwner<D, P>,
    >(
        self,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut F,
    ) -> Result<()> {
        match self {
            Self::CreatureRemove { creature } => {
                require(
                    context.objects.get(creature)?.kind == SourceObjectKind::Creature,
                    "execution.transport.non_pet_creature",
                )?;
                context.add_to_remove(creature, owner)?;
            }
            Self::CreatureRelocate { creature, pose } => {
                require(
                    matches!(
                        context.objects.get(creature)?.kind,
                        SourceObjectKind::Creature | SourceObjectKind::Pet
                    ),
                    "execution.transport.creature_kind",
                )?;
                context.move_creature(creature, pose, owner)?;
            }
            _ => return Err(self.missing_owner()),
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TransportTransfer {
    pub transport: ObjectHandle,
}
#[derive(Default, Debug)]
pub struct SourcePassengers {
    units: BTreeMap<ObjectHandle, PassengerMovement>,
    sets: BTreeMap<ObjectHandle, BTreeMap<u64, ObjectHandle>>,
    next: BTreeMap<ObjectHandle, Option<u64>>,
}
impl SourcePassengers {
    pub(super) fn validate_manager_owners(&self, objects: &SourceObjects) -> Result<()> {
        require(
            self.next.values().all(Option::is_none),
            "execution.managers.passenger_traversal_active",
        )?;
        for (&transport, set) in &self.sets {
            objects.get(transport)?;
            for (&pointer, &passenger) in set {
                require(
                    objects.get(passenger)?.source_allocation == Some(pointer)
                        && self.movement(passenger)?.transport == Some(transport),
                    "execution.managers.passenger_owner",
                )?;
            }
        }
        for (&unit, state) in &self.units {
            objects.get(unit)?;
            if let Some(transport) = state.transport {
                require(
                    self.members(transport).any(|member| member == unit),
                    "execution.managers.passenger_membership",
                )?;
            }
            for pet in [state.pet, state.mini_pet].into_iter().flatten() {
                objects.get(pet)?;
            }
        }
        Ok(())
    }
    /// Bind actual Unit movement/pet state, never observations of boarding outcomes.
    pub fn bind_unit(
        &mut self,
        handle: ObjectHandle,
        state: PassengerMovement,
        objects: &SourceObjects,
    ) -> Result<()> {
        require(
            matches!(
                objects.get(handle)?.kind,
                SourceObjectKind::Player | SourceObjectKind::Creature | SourceObjectKind::Pet
            ),
            "execution.transport.passenger_unit",
        )?;
        require(
            !self.units.contains_key(&handle),
            "execution.transport.passenger_state_already_bound",
        )?;
        self.units.insert(handle, state);
        Ok(())
    }
    pub fn movement(&self, handle: ObjectHandle) -> Result<&PassengerMovement> {
        self.units
            .get(&handle)
            .ok_or_else(|| refuse("execution.transport.f5_unit_movement_owner"))
    }
    /// The Unit/player/pet handlers share this movement state with transport calls.
    pub fn movement_mut(&mut self, handle: ObjectHandle) -> Result<&mut PassengerMovement> {
        self.units
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.transport.f5_unit_movement_owner"))
    }
    pub fn members(&self, transport: ObjectHandle) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.sets
            .get(&transport)
            .into_iter()
            .flat_map(|s| s.values())
            .copied()
    }
    // Duplicate boarding skips even the pet callbacks; Transports.cpp:273-297.
    pub fn add(
        &mut self,
        transport: PassengerTransport,
        passenger: ObjectHandle,
        adjust: bool,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        self.movement(passenger)?;
        let pointer = objects
            .get(passenger)?
            .source_allocation
            .ok_or_else(|| refuse("execution.transport.passenger_allocation_owner"))?;
        if self
            .sets
            .get(&transport.handle)
            .is_some_and(|s| s.contains_key(&pointer))
        {
            return Ok(());
        }
        self.sets
            .entry(transport.handle)
            .or_default()
            .insert(pointer, passenger);
        let state = self.units.get_mut(&passenger).unwrap();
        state.transport = Some(transport.handle);
        state.on_transport = true;
        let changed = state.guid != transport.guid;
        state.guid = transport.guid;
        state.time = transport.progress;
        if changed && adjust {
            state.offset = objects.pose(passenger)?;
            state.offset = SourcePose(
                passenger_offset(state.offset.0, transport.pose.0).map_err(super::numeric)?,
            );
        }
        let pets = [state.pet, state.mini_pet];
        for pet in pets.into_iter().flatten() {
            self.add(transport, pet, true, objects, owner)?;
            let offset = self.movement(passenger)?.offset;
            let state = self.units.get_mut(&pet).unwrap();
            state.guid = transport.guid;
            state.offset = offset;
            state.time = transport.progress;
            owner.passenger_effect(
                PassengerEffect::PetNearTeleport {
                    pet,
                    pose: self.movement(passenger)?.movement_pose,
                },
                objects,
            )?;
        }
        Ok(())
    }
    // Erasing the preadvanced next iterator advances it again before recursion; Transports.cpp:303-332.
    pub fn remove(
        &mut self,
        transport: ObjectHandle,
        passenger: ObjectHandle,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let pointer = objects
            .get(passenger)?
            .source_allocation
            .ok_or_else(|| refuse("execution.transport.passenger_allocation_owner"))?;
        let Some(set) = self.sets.get_mut(&transport) else {
            return Ok(());
        };
        if !set.contains_key(&pointer) {
            return Ok(());
        }
        if self.next.get(&transport) == Some(&Some(pointer)) {
            self.next.insert(
                transport,
                set.range((Excluded(pointer), Unbounded))
                    .next()
                    .map(|(&p, _)| p),
            );
        }
        set.remove(&pointer);
        let state = self
            .units
            .get_mut(&passenger)
            .ok_or_else(|| refuse("execution.transport.f5_unit_movement_owner"))?;
        state.transport = None;
        state.guid = 0;
        state.time = 0;
        state.offset = SourcePose([0; 4]);
        let pets = [state.pet, state.mini_pet];
        for pet in pets.into_iter().flatten() {
            self.remove(transport, pet, objects, owner)?;
            owner.passenger_effect(
                PassengerEffect::PetNearTeleport {
                    pet,
                    pose: self.movement(passenger)?.movement_pose,
                },
                objects,
            )?;
        }
        Ok(())
    }
    pub(super) fn transfer_passengers(
        &mut self,
        transport: PassengerTransport,
        destination: u32,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let change = transport
            .map
            .ok_or_else(|| refuse("execution.transport.current_map_owner"))?
            .map
            != destination;
        self.next.insert(
            transport.handle,
            self.sets
                .get(&transport.handle)
                .and_then(|s| s.keys().next().copied()),
        );
        while let Some(pointer) = self.next[&transport.handle] {
            let set = &self.sets[&transport.handle];
            let passenger = set[&pointer];
            self.next.insert(
                transport.handle,
                set.range((Excluded(pointer), Unbounded))
                    .next()
                    .map(|(&p, _)| p),
            );
            let offset = self.movement(passenger)?.offset;
            match objects.get(passenger)?.kind {
                SourceObjectKind::Creature | SourceObjectKind::Pet if change => {
                    self.remove(transport.handle, passenger, objects, owner)?;
                    if objects.get(passenger)?.kind == SourceObjectKind::Creature {
                        owner.passenger_effect(
                            PassengerEffect::CreatureRemove {
                                creature: passenger,
                            },
                            objects,
                        )?;
                    }
                }
                SourceObjectKind::Player => {
                    if !objects.get(passenger)?.alive && !self.movement(passenger)?.ghost {
                        owner.passenger_effect(
                            PassengerEffect::Resurrect {
                                player: passenger,
                                health: 1.0_f32.to_bits(),
                            },
                            objects,
                        )?;
                    }
                    owner.passenger_effect(
                        PassengerEffect::PlayerTeleport {
                            player: passenger,
                            map: destination,
                            offset,
                            transport: transport.handle,
                        },
                        objects,
                    )?;
                }
                _ => {}
            }
        }
        Ok(())
    }
    // Cross-map in-world passengers wait for their own teleport; Transports.cpp:506-549.
    pub fn update_positions(
        &mut self,
        transport: PassengerTransport,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        self.update_positions_with(transport, objects, &mut |effect, objects| {
            owner.passenger_effect(effect, objects)
        })
    }
    pub(super) fn update_positions_with(
        &mut self,
        transport: PassengerTransport,
        objects: &mut SourceObjects,
        effect: &mut impl FnMut(PassengerEffect, &mut SourceObjects) -> Result<()>,
    ) -> Result<()> {
        let mut after = None;
        loop {
            let next = self
                .sets
                .get(&transport.handle)
                .and_then(|s| match after {
                    None => s.first_key_value(),
                    Some(p) => s.range((Excluded(p), Unbounded)).next(),
                })
                .map(|(&p, &h)| (p, h));
            let Some((pointer, h)) = next else {
                break;
            };
            after = Some(pointer);
            let object = objects.get(h)?;
            if object.in_world && object.map != transport.map {
                continue;
            }
            let pose = SourcePose(
                passenger_position(self.movement(h)?.offset.0, transport.pose.0)
                    .map_err(super::numeric)?,
            );
            let p = pose.0.map(f32::from_bits);
            let half = (f32::from_bits(0x4405_5555) * 64.0_f32) / 2.0_f32;
            if !p[..3].iter().all(|v| v.is_finite())
                || p[0].abs() as f64 > half as f64 - 0.5
                || p[1].abs() as f64 > half as f64 - 0.5
            {
                continue;
            }
            if object.in_world {
                effect(
                    match object.kind {
                        SourceObjectKind::Player => {
                            PassengerEffect::PlayerRelocate { player: h, pose }
                        }
                        _ => PassengerEffect::CreatureRelocate { creature: h, pose },
                    },
                    objects,
                )?;
            } else {
                let player = object.kind == SourceObjectKind::Player;
                objects.set_pose(h, pose)?;
                if player {
                    self.units.get_mut(&h).unwrap().guid = transport.guid;
                }
            }
            self.units.get_mut(&h).unwrap().time = transport.progress;
        }
        Ok(())
    }
}

/// Compose with the event dispatcher on the same detached map-message batch.
pub struct TransportMessageDispatch<'a, O, W> {
    pub transports: &'a mut SourceMoTransports,
    pub cells: &'a SourceCells,
    pub objects: &'a mut SourceObjects,
    pub owner: &'a mut O,
    pub other: &'a mut W,
}
impl<O: MoTransportOwner, W: MessageWork> MessageWork for TransportMessageDispatch<'_, O, W> {
    fn begin_message(&mut self, message: &MessageEnvelope) -> Result<()> {
        self.other.begin_message(message)
    }
    fn forward_game_event(
        &mut self,
        thread: u64,
        maps: &[EventMapGuids],
        spawn: bool,
    ) -> Result<()> {
        self.other.forward_game_event(thread, maps, spawn)
    }
    fn map_game_event(
        &mut self,
        thread: u64,
        map: MapIdentity,
        creatures: &[u32],
        gameobjects: &[u32],
        spawn: bool,
    ) -> Result<()> {
        self.other
            .map_game_event(thread, map, creatures, gameobjects, spawn)
    }
    fn creature_present(&self, map: MapIdentity, guid: u64) -> bool {
        self.other.creature_present(map, guid)
    }
    fn update_creature_event(
        &mut self,
        thread: u64,
        map: MapIdentity,
        update: EventCreatureUpdate,
    ) -> Result<()> {
        self.other.update_creature_event(thread, map, update)
    }
    fn attach_transport(
        &mut self,
        _: u64,
        map: MapIdentity,
        transfer: TransportTransfer,
    ) -> Result<()> {
        require(
            map == self.cells.map(),
            "execution.transport.transfer_generation",
        )?;
        self.transports
            .deliver_transfer(transfer, self.cells, self.objects, self.owner)
    }
}
