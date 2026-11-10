// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Transports.cpp:118-197,349-440,490-496; GPL-3.0-or-later.
use super::*;
use benilla_sim::source_transports::{MoTransport, TransportMotion, TransportStep};
use std::collections::BTreeMap;

#[derive(Debug)]
pub struct SourceMoTransport {
    pub handle: ObjectHandle,
    pub template: GameObjectTemplate,
    pub motion: TransportMotion,
    pub position: SourcePose,
    pub display: u32,
    pub state: u8,
    pub kind: u8,
    pub animation: u8,
    pub has_model: bool,
}
impl SourceMoTransport {
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
    fn transport_allocation(
        &mut self,
        handle: ObjectHandle,
        objects: &mut SourceObjects,
    ) -> Result<()>;
    fn transport_model(
        &mut self,
        _transport: &SourceMoTransport,
        operation: GameObjectModelOperation,
    ) -> Result<bool> {
        Err(refuse(match operation {
            GameObjectModelOperation::ConstructAndInsert => {
                "execution.transport.f6_dynamic_model_construct_insert_owner"
            }
            _ => "execution.transport.f6_dynamic_model_relocate_owner",
        }))
    }
}
pub struct MoTransportContext<'a, O: MoTransportOwner> {
    pub cells: &'a mut SourceCells,
    pub objects: &'a mut SourceObjects,
    pub owner: &'a mut O,
}
/// Map::m_transports is a separate pointer-ordered set, not grid or world membership.
#[derive(Default)]
pub struct SourceMoTransports {
    objects: BTreeMap<ObjectHandle, SourceMoTransport>,
    maps: BTreeMap<MapIdentity, BTreeMap<u64, ObjectHandle>>,
    initialized: std::collections::BTreeSet<MapIdentity>,
    stopped: bool,
}
impl SourceMoTransports {
    pub fn get(&self, handle: ObjectHandle) -> Result<&SourceMoTransport> {
        self.objects
            .get(&handle)
            .ok_or_else(|| refuse("execution.transport.stale_handle"))
    }
    pub fn members(&self, map: MapIdentity) -> impl Iterator<Item = ObjectHandle> + '_ {
        self.maps
            .get(&map)
            .into_iter()
            .flat_map(|set| set.values())
            .copied()
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
            owner.transport_allocation(handle, objects)?;
            objects.bind_map(handle, map, cells)?;
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
            let display = template.display;
            let mut transport = SourceMoTransport {
                handle,
                template,
                motion: TransportMotion::new(snapshot),
                position,
                display,
                state: 0,
                kind: 0,
                animation: 0,
                has_model: false,
            };
            // SetDisplayId constructs/inserts before READY/type/animation and AddTransport; Transports.cpp:186-192,147.
            transport.has_model =
                owner.transport_model(&transport, GameObjectModelOperation::ConstructAndInsert)?;
            transport.state = 1;
            transport.kind = transport.template.kind as u8;
            transport.animation = 100;
            let pointer = objects
                .get(handle)?
                .source_allocation
                .ok_or_else(|| refuse("execution.transport.allocation_owner"))?;
            let set = self.maps.entry(map).or_default();
            require(
                !set.contains_key(&pointer),
                "execution.transport.pointer_lifetime",
            )?;
            set.insert(pointer, handle);
            self.objects.insert(handle, transport);
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
        let Some(set) = self.maps.get(&map) else {
            return Ok(());
        };
        for &handle in set.values() {
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
                TransportStep::Transfer { .. } => {
                    return Err(refuse("execution.transport.f4_17_passenger_transfer_owner"))
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
                }
            }
        }
        Ok(())
    }
    pub fn attach_passenger(
        &mut self,
        _transport: ObjectHandle,
        _passenger: ObjectHandle,
    ) -> Result<()> {
        Err(refuse(
            "execution.transport.f4_17_passenger_attachment_owner",
        ))
    }
}
#[cfg(test)]
#[path = "transports_movement_tests.rs"]
mod tests;
