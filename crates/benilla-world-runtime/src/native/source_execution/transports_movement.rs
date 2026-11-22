// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Transports.cpp:118-197,349-440,490-496; GPL-3.0-or-later.
use super::*;
use crate::native::source_execution::active::SourcePointerSet;
use benilla_sim::event_processor::{EventError, EventPayload, EventProcessor};
use benilla_sim::source_transports::{MoTransport, TransportMotion, TransportStep};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::native::source_execution) enum NoTransportEvent {}
impl EventPayload for NoTransportEvent {
    fn validate(self) -> std::result::Result<(), EventError> {
        match self {}
    }
}

#[derive(Debug)]
pub struct SourceMoTransport {
    pub handle: ObjectHandle,
    pub template: GameObjectTemplate,
    pub motion: TransportMotion,
    pub position: SourcePose,
    pub scale: f32,
    pub faction: u32,
    pub flags: u32,
    pub entry: u32,
    pub display: u32,
    pub state: u8,
    pub kind: u32,
    pub animation: u32,
    pub has_model: bool,
    pub update_flags: u8,
    pub name: Vec<u8>,
    // Object.h:1146. No transport event payload is ported.
    pub(in crate::native::source_execution) events: EventProcessor<NoTransportEvent>,
}
impl SourceMoTransport {
    pub fn event_processor_is_initial(&self) -> bool {
        self.events == EventProcessor::default()
    }

    fn definition(&self) -> MoTransport {
        MoTransport {
            entry: self.template.entry,
            path: self.template.data[0],
            speed: self.template.data[1],
            acceleration: self.template.data[2],
        }
    }
}
/// Executed ObjectMgr and F6 operations, never expected model outcomes.
pub trait MoTransportOwner {
    fn transport_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>>;
    fn transport_name(&mut self, _entry: u32) -> Result<&[u8]> {
        Err(refuse(
            "execution.transport.Transports_194.gameobject_name_owner",
        ))
    }
    fn transport_allocation(
        &mut self,
        handle: ObjectHandle,
        objects: &mut SourceObjects,
    ) -> Result<()>;
    fn transport_geometry(
        &mut self,
        _handle: ObjectHandle,
    ) -> Result<(
        &mut SourceDynamicModels,
        &mut SourceDynamicMap,
        &mut SourceVMapManager,
    )> {
        Err(refuse(
            "execution.dynamic_collision.transport_world_geometry_owner",
        ))
    }
    fn transport_model(
        &mut self,
        transport: &SourceMoTransport,
        operation: GameObjectModelOperation,
    ) -> Result<bool> {
        let object = DynamicGameObject {
            handle: transport.handle,
            display: transport.display,
            pose: transport.position,
            scale_bits: transport.scale.to_bits(),
        };
        let (models, dynamic, vmap) = self.transport_geometry(transport.handle)?;
        models.operation(dynamic, vmap, object, operation)
    }
    fn passenger_effect(
        &mut self,
        effect: PassengerEffect,
        _objects: &mut SourceObjects,
    ) -> Result<()> {
        Err(effect.missing_owner())
    }
    /// Execute MapManager::CreateMap, including destination initialization.
    fn transport_destination(
        &mut self,
        _map: u32,
        _transport: ObjectHandle,
    ) -> Result<MapIdentity> {
        Err(refuse("execution.transport.map_manager_create_map_owner"))
    }
    /// Allocate a causal message identity and enqueue on the actual destination Messager.
    fn transport_message(&mut self, _map: MapIdentity, _transfer: TransportTransfer) -> Result<()> {
        Err(refuse("execution.transport.map_message_schedule_owner"))
    }
}
pub struct MoTransportContext<'a, O: MoTransportOwner> {
    pub cells: &'a mut SourceCells,
    pub objects: &'a mut SourceObjects,
    pub owner: &'a mut O,
}
impl SourceContinent {
    // Map.cpp:216-228 loads transports, then runs the map-specific tail.
    #[allow(clippy::too_many_arguments)]
    pub fn initialize_tail(
        &mut self,
        templates: &SourceTransportTemplates,
        definitions: &LoadedGameObjectTemplates,
        objects: &mut SourceObjects,
        models: &mut SourceDynamicModels,
        vmap: &mut SourceVMapManager,
        world_clock: &SourceWorldClock,
        thread: u64,
    ) -> Result<()> {
        let mut owner = StartupTransportOwner {
            definitions,
            models,
            tree: &mut self.dynamic_tree,
            vmap,
        };
        self.transports.initialize_map(
            templates,
            &mut MoTransportContext {
                cells: &mut self.cells,
                objects,
                owner: &mut owner,
            },
            thread,
            world_clock.current_ms() as u32, // Map.cpp:78-80, World.cpp:107.
        )?;
        if self.identity.map == 533 {
            return Err(refuse("execution.continents.Map_223.force_load_grid_owner"));
        }
        Ok(())
    }
}

struct StartupTransportOwner<'a> {
    definitions: &'a LoadedGameObjectTemplates,
    models: &'a mut SourceDynamicModels,
    tree: &'a mut SourceDynamicMap,
    vmap: &'a mut SourceVMapManager,
}
impl MoTransportOwner for StartupTransportOwner<'_> {
    fn transport_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>> {
        Ok(self
            .definitions
            .get(entry)
            .map(LoadedGameObjectInfo::template))
    }

    fn transport_name(&mut self, entry: u32) -> Result<&[u8]> {
        self.definitions
            .get(entry)
            .map(|row| row.strings[0].as_slice())
            .ok_or_else(|| refuse("execution.transport.Transports_194.gameobject_name_owner"))
    }

    // SourceObjects constructed the object. Startup has no source address binding.
    fn transport_allocation(&mut self, _: ObjectHandle, _: &mut SourceObjects) -> Result<()> {
        Ok(())
    }

    fn transport_geometry(
        &mut self,
        _: ObjectHandle,
    ) -> Result<(
        &mut SourceDynamicModels,
        &mut SourceDynamicMap,
        &mut SourceVMapManager,
    )> {
        Ok((self.models, self.tree, self.vmap))
    }
}

#[derive(Clone, Copy)]
pub enum TransportWalk {
    Destroy,
    GetTransport,
    Update,
    SendInit,
    SendRemove,
}
impl TransportWalk {
    fn refusal_id(self) -> &'static str {
        match self {
            Self::Destroy => "execution.transport.Map_74.pointer_order_owner",
            Self::GetTransport => "execution.transport.Map_106.pointer_order_owner",
            Self::Update => "execution.transport.Map_725.pointer_order_owner",
            Self::SendInit => "execution.transport.Map_1407.pointer_order_owner",
            Self::SendRemove => "execution.transport.Map_1434.pointer_order_owner",
        }
    }
}

/// Map::m_transports is a separate pointer-ordered set, not grid or world membership.
#[derive(Default)]
pub struct SourceMoTransports {
    objects: BTreeMap<ObjectHandle, SourceMoTransport>,
    maps: BTreeMap<MapIdentity, SourcePointerSet>,
    initialized: std::collections::BTreeSet<MapIdentity>,
    stopped: bool,
    pub passengers: SourcePassengers,
    pending: BTreeMap<ObjectHandle, MapIdentity>,
}
impl SourceMoTransports {
    /// Join all map sets and in-flight messages, including off-camera transports.
    pub fn validate_manager_owners(
        &self,
        objects: &SourceObjects,
        maps: &std::collections::BTreeSet<MapIdentity>,
        inventories: &[MapManagerInventory],
    ) -> Result<()> {
        require(
            !self.stopped && self.initialized == *maps,
            "execution.managers.transport_map_owner",
        )?;
        let mut members = std::collections::BTreeSet::new();
        for (map, set) in &self.maps {
            require(maps.contains(map), "execution.managers.transport_map_owner")?;
            for (handle, address) in set.bindings() {
                let object = objects.get(handle)?;
                require(
                    members.insert(handle)
                        && self.objects.contains_key(&handle)
                        && object.map == Some(*map)
                        && object.source_allocation == address
                        && !self.pending.contains_key(&handle),
                    "execution.managers.transport_object_owner",
                )?;
            }
        }
        for (&handle, &destination) in &self.pending {
            require(
                maps.contains(&destination)
                    && objects.get(handle)?.map.is_none()
                    && self.objects.contains_key(&handle)
                    && members.insert(handle),
                "execution.managers.transport_transfer_owner",
            )?;
            let deliveries = inventories
                .iter()
                .flat_map(|map| &map.messages.queue)
                .filter(|message| {
                    matches!(&message.payload, SourceMessage::AttachTransport { transfer }
                    if transfer.transport == handle)
                })
                .count();
            require(
                deliveries == 1
                    && inventories.iter().any(|map| {
                        map.messages.owner == MessageOwner::Map(destination)
                            && map.messages.queue.iter().any(|message| {
                                matches!(&message.payload,
                    SourceMessage::AttachTransport { transfer } if transfer.transport == handle)
                            })
                    }),
                "execution.managers.transport_transfer_message",
            )?;
        }
        require(
            members.len() == self.objects.len(),
            "execution.managers.omitted_transport_owner",
        )?;
        for map in inventories {
            for message in &map.messages.queue {
                if let SourceMessage::AttachTransport { transfer } = &message.payload {
                    require(
                        self.pending.get(&transfer.transport) == Some(&map.spawn.map),
                        "execution.managers.transport_transfer_message",
                    )?;
                }
            }
        }
        self.passengers.validate_manager_owners(objects)?;
        Ok(())
    }
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceMoTransport> {
        self.objects
            .get(&handle)
            .ok_or_else(|| refuse("execution.transport.stale_handle"))
    }
    /// Membership enumeration is for order-independent consumers only.
    pub fn members(&self, map: MapIdentity) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.maps
            .get(&map)
            .into_iter()
            .flat_map(|set| set.members())
    }

    pub fn contains(&self, map: MapIdentity, handle: ObjectHandle) -> bool {
        self.maps.get(&map).is_some_and(|set| set.contains(handle))
    }

    // Map.cpp:119-122 inserts into the transport set without walking it.
    fn insert_member(
        &mut self,
        map: MapIdentity,
        handle: ObjectHandle,
        address: Option<u64>,
    ) -> Result<()> {
        require(
            self.maps.entry(map).or_default().insert(
                handle,
                address,
                "execution.transport.pointer_lifetime",
                |_| Ok(()),
            )?,
            "execution.transport.Map_121.duplicate_identity",
        )
    }

    fn ordered_map(
        &self,
        map: MapIdentity,
        walk: TransportWalk,
    ) -> Result<Option<&BTreeMap<u64, ObjectHandle>>> {
        self.maps
            .get(&map)
            .map(|set| set.ordered(walk.refusal_id()))
            .transpose()
    }

    pub fn walk(
        &self,
        map: MapIdentity,
        walk: TransportWalk,
    ) -> Result<impl Iterator<Item = ObjectHandle> + '_> {
        Ok(self
            .ordered_map(map, walk)?
            .into_iter()
            .flat_map(BTreeMap::values)
            .copied())
    }
    // Map initialization creates transports after active grids and before player entry; Map.cpp:214-216,1445-1449.
    pub fn initialize_map(
        &mut self,
        templates: &SourceTransportTemplates,
        context: &mut MoTransportContext<'_, impl MoTransportOwner>,
        thread: u64,
        snapshot: u32,
    ) -> Result<()> {
        require(
            !self.stopped && !self.initialized.contains(&context.cells.map()),
            "execution.transport.initialization_state",
        )?;
        let result = self.initialize_inner(templates, context, thread, snapshot);
        self.stopped |= result.is_err();
        result
    }
    fn initialize_inner(
        &mut self,
        templates: &SourceTransportTemplates,
        context: &mut MoTransportContext<'_, impl MoTransportOwner>,
        thread: u64,
        snapshot: u32,
    ) -> Result<()> {
        let MoTransportContext {
            cells,
            objects,
            owner,
        } = context;
        let map = cells.map();
        for &entry in templates.by_map()?.get(&map.map).into_iter().flatten() {
            let path = templates
                .template(entry)?
                .ok_or_else(|| refuse("execution.transport.template_owner"))?;
            let handle = objects.construct(thread, SourceObjectKind::Transport)?;
            let events = EventProcessor::default(); // EventProcessor.cpp:21-25.
            let update_flags = 0x52; // Transports.cpp:120; UpdateData.h:43,46,48.
            owner.transport_allocation(handle, objects)?;
            let frame = path
                .key_frames
                .first()
                .ok_or_else(|| refuse("execution.transport.empty_key_frames_undefined"))?;
            let node = &path.nodes[frame.node];
            let position = SourcePose([
                node.position[0],
                node.position[1],
                node.position[2],
                frame.orientation,
            ]);
            // Transports.cpp:131-138 reads the first node and orientation before SetMap.
            objects.bind_map(handle, map, cells)?;
            objects.set_pose(handle, position)?;
            let p = position.0.map(f32::from_bits);
            let half = (f32::from_bits(0x4405_5555) * 64.0_f32) / 2.0_f32;
            if !p.iter().all(|x| x.is_finite())
                || p[0].abs() as f64 > half as f64 - 0.5
                || p[1].abs() as f64 > half as f64 - 0.5
            {
                objects.destroy(handle, cells)?;
                continue;
            }
            // MO GUID uses entry as the low counter, entry field zero, and no allocator draw; Transports.cpp:161.
            objects.assign_guid(
                handle,
                entry,
                SourceGuid::with_entry(HighGuid::MoTransport, 0, entry),
            )?;
            let Some(template) = owner.transport_template(entry)? else {
                objects.destroy(handle, cells)?;
                continue;
            };
            // Transports.cpp:127 and Transports.h:93 copy pathTime into m_period.
            // The immutable template's path_time also represents m_period here.
            let display = template.display;
            let mut transport = SourceMoTransport {
                handle,
                template,
                motion: TransportMotion::new(snapshot), // Transports.cpp:118,174-177.
                position,
                scale: 0.0, // Object.cpp:105-108 clears the update words.
                faction: 0,
                flags: 0,
                entry: 0,
                display: 0,
                state: 0,
                kind: 0,
                animation: 0,
                has_model: false, // GameObject.cpp:65-66.
                update_flags,
                name: Vec::new(), // Object.h:1185 default string.
                events,
            };
            // Transports.cpp:179-186 writes these values before SetDisplayId can fail.
            objects.set_update_value(
                handle,
                OBJECT_FIELD_SCALE_X,
                &mut transport.scale,
                f32::from_bits(transport.template.scale_bits),
            )?;
            objects.set_update_value(
                handle,
                GAMEOBJECT_FACTION,
                &mut transport.faction,
                transport.template.faction,
            )?;
            objects.set_update_value(
                handle,
                GAMEOBJECT_FLAGS,
                &mut transport.flags,
                transport.template.flags,
            )?;
            objects.set_update_value(
                handle,
                OBJECT_FIELD_ENTRY,
                &mut transport.entry,
                transport.template.entry,
            )?;
            // GameObject.cpp:2083-2086 writes the display before constructing the model.
            objects.set_update_value(
                handle,
                GAMEOBJECT_DISPLAYID,
                &mut transport.display,
                display,
            )?;
            let transport = self.objects.entry(handle).or_insert(transport);
            transport.has_model =
                owner.transport_model(transport, GameObjectModelOperation::ConstructAndInsert)?;
            // Transports.cpp:187-194. The display write repeats; the changed bit stays set once.
            objects.set_update_value(
                handle,
                GAMEOBJECT_DISPLAYID,
                &mut transport.display,
                display,
            )?;
            objects.set_update_value(handle, GAMEOBJECT_STATE, &mut transport.state, 1)?;
            objects.set_update_value(
                handle,
                GAMEOBJECT_TYPE_ID,
                &mut transport.kind,
                transport.template.kind,
            )?;
            objects.set_update_value(
                handle,
                GAMEOBJECT_ANIMPROGRESS,
                &mut transport.animation,
                100,
            )?;
            transport
                .name
                .extend_from_slice(owner.transport_name(entry)?);
            self.insert_member(map, handle, objects.get(handle)?.source_allocation)?;
        }
        self.initialized.insert(map);
        Ok(())
    }
    // Call from MapPreludePhase::Transports with World::GetCurrentMSTime, not a fresh sample; Map.cpp:78-80,725-729.
    pub fn update_map(
        &mut self,
        templates: &SourceTransportTemplates,
        map: MapIdentity,
        snapshot: u32,
        supplied_diff: u32,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        require(
            !self.stopped && self.initialized.contains(&map),
            "execution.transport.update_state",
        )?;
        let result = self.update_inner(templates, map, snapshot, supplied_diff, objects, owner);
        self.stopped |= result.is_err();
        result
    }
    fn update_inner(
        &mut self,
        templates: &SourceTransportTemplates,
        map: MapIdentity,
        snapshot: u32,
        supplied_diff: u32,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let mut after = None;
        loop {
            let next = self
                .ordered_map(map, TransportWalk::Update)?
                .and_then(|set| match after {
                    None => set.first_key_value(),
                    Some(p) => set
                        .range((std::ops::Bound::Excluded(p), std::ops::Bound::Unbounded))
                        .next(),
                })
                .map(|(&p, &h)| (p, h));
            let Some((pointer, handle)) = next else {
                break;
            };
            after = Some(pointer);
            let transport = self.objects.get_mut(&handle).unwrap();
            require(
                objects.get(handle)?.map == Some(map),
                "execution.transport.map_binding",
            )?;
            let path = templates
                .template(transport.template.entry)?
                .ok_or_else(|| refuse("execution.transport.template_owner"))?;
            let definition = transport.definition();
            match transport
                .motion
                .update(path, definition, map.map, snapshot, supplied_diff)
                .map_err(|error| match error {
                    benilla_sim::source_transports::Error::SourceUndefined(site) => {
                        refuse(&format!("execution.transport.{site}_undefined"))
                    }
                    other => super::numeric(other),
                })? {
                TransportStep::Unchanged => {}
                TransportStep::Transfer { next, .. } => {
                    let frame = &path.key_frames[next];
                    let node = &path.nodes[frame.node];
                    let pose = SourcePose([
                        node.position[0],
                        node.position[1],
                        node.position[2],
                        frame.orientation,
                    ]);
                    self.teleport(handle, node.map, pose, objects, owner)?;
                }
                TransportStep::Position { pose, .. } => {
                    transport.position = SourcePose(pose);
                    objects.set_pose(handle, transport.position)?;
                    // Model transform keeps Contains, Remove, Relocate, Insert order; GameObject.cpp:2129-2139.
                    if transport.has_model
                        && owner.transport_model(transport, GameObjectModelOperation::Contains)?
                    {
                        owner.transport_model(transport, GameObjectModelOperation::Remove)?;
                        owner.transport_model(transport, GameObjectModelOperation::Relocate)?;
                        owner.transport_model(transport, GameObjectModelOperation::Insert)?;
                    }
                    let view = self.passenger_transport(handle, objects)?;
                    self.passengers.update_positions(view, objects, owner)?;
                }
            }
        }
        Ok(())
    }
    pub fn passenger_transport(
        &self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<PassengerTransport> {
        let t = self.get(handle)?;
        let object = objects.get(handle)?;
        Ok(PassengerTransport {
            handle,
            guid: object
                .guid
                .ok_or_else(|| refuse("execution.transport.guid_owner"))?
                .raw(),
            progress: t.motion.progress(),
            pose: t.position,
            map: object.map,
        })
    }
    pub fn attach_passenger(
        &mut self,
        transport: ObjectHandle,
        passenger: ObjectHandle,
        adjust: bool,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let view = self.passenger_transport(transport, objects)?;
        self.passengers.add(view, passenger, adjust, objects, owner)
    }
    pub fn detach_passenger(
        &mut self,
        transport: ObjectHandle,
        passenger: ObjectHandle,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        self.passengers.remove(transport, passenger, objects, owner)
    }
    pub fn pending_transfer(&self, handle: ObjectHandle) -> Option<MapIdentity> {
        self.pending.get(&handle).copied()
    }
    // Relocate(x,y,z) deliberately retains the old orientation and ignores o; Transports.cpp:208-209.
    pub fn teleport(
        &mut self,
        handle: ObjectHandle,
        destination: u32,
        pose: SourcePose,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let old = objects
            .get(handle)?
            .map
            .ok_or_else(|| refuse("execution.transport.current_map_owner"))?;
        let transport = self
            .objects
            .get_mut(&handle)
            .ok_or_else(|| refuse("execution.transport.stale_handle"))?;
        transport.position.0[..3].copy_from_slice(&pose.0[..3]);
        objects.set_pose(handle, transport.position)?;
        let view = self.passenger_transport(handle, objects)?;
        self.passengers
            .transfer_passengers(view, destination, objects, owner)?;
        if old.map != destination {
            let transport = self.get(handle)?;
            if transport.has_model
                && owner.transport_model(transport, GameObjectModelOperation::Contains)?
            {
                owner.transport_model(transport, GameObjectModelOperation::Remove)?;
            }
            // Map.cpp:124-135 removes the identity without requiring a source address.
            self.maps
                .get_mut(&old)
                .ok_or_else(|| refuse("execution.transport.map_binding"))?
                .remove(handle);
            self.update_for_map(handle, old, false, objects, owner)?;
            objects.reset_map(handle)?;
            let destination = owner.transport_destination(destination, handle)?;
            self.pending.insert(handle, destination);
            owner.transport_message(destination, TransportTransfer { transport: handle })?;
        } else {
            let transport = self.get(handle)?;
            if transport.has_model
                && owner.transport_model(transport, GameObjectModelOperation::Contains)?
            {
                owner.transport_model(transport, GameObjectModelOperation::Remove)?;
                owner.transport_model(transport, GameObjectModelOperation::Relocate)?;
                owner.transport_model(transport, GameObjectModelOperation::Insert)?;
            }
        }
        Ok(())
    }
    // Player-list order and attached-player exclusions are independent of the passenger set; Transports.cpp:591-618.
    pub fn update_for_map(
        &self,
        handle: ObjectHandle,
        map: MapIdentity,
        create: bool,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let players = objects.map_players(map).collect::<Vec<_>>();
        if players.is_empty() {
            return Ok(());
        }
        if !create {
            owner.passenger_effect(
                PassengerEffect::BuildOutOfRange { transport: handle },
                objects,
            )?;
        }
        for player in players {
            if self.passengers.transport(player)? != Some(handle) {
                owner.passenger_effect(
                    PassengerEffect::Visibility {
                        transport: handle,
                        player,
                        create,
                    },
                    objects,
                )?;
            }
        }
        Ok(())
    }
    // The destination message attaches before model Contains/Relocate/Insert and visibility; Transports.cpp:259-265.
    pub fn deliver_transfer(
        &mut self,
        transfer: TransportTransfer,
        cells: &SourceCells,
        objects: &mut SourceObjects,
        owner: &mut impl MoTransportOwner,
    ) -> Result<()> {
        let handle = transfer.transport;
        require(
            self.pending.get(&handle) == Some(&cells.map()) && objects.get(handle)?.map.is_none(),
            "execution.transport.transfer_generation",
        )?;
        objects.bind_map(handle, cells.map(), cells)?;
        self.insert_member(cells.map(), handle, objects.get(handle)?.source_allocation)?;
        self.pending.remove(&handle);
        let transport = self.get(handle)?;
        if transport.has_model
            && !owner.transport_model(transport, GameObjectModelOperation::Contains)?
        {
            owner.transport_model(transport, GameObjectModelOperation::Relocate)?;
            owner.transport_model(transport, GameObjectModelOperation::Insert)?;
        }
        self.update_for_map(handle, cells.map(), true, objects, owner)
    }
}
#[cfg(test)]
#[path = "transports_movement_tests.rs"]
mod tests;
