// SPDX-License-Identifier: GPL-3.0-or-later
//! GameObject.cpp:178-302,870-969 executes through the existing GO and F6 owners.
use super::*;
use std::sync::Arc;
impl<D: GridDefinitions<Row = SpawnKey>> GameObjectFactoryOwner<D, SourceTerrainManager>
    for Prefix<'_>
{
    fn gameobject_constructed(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.passengers.construct(handle, objects)
    }
    fn gameobjects(&mut self) -> &mut SourceGameObjects {
        &mut self.gameobjects
    }
    fn gameobject_guids(&mut self) -> &mut MapGuidAllocators {
        self.guids
    }
    fn gameobject_spawn(&mut self, _: GridLoadAttempt, key: SpawnKey) -> Result<GameObjectSpawn> {
        let data = self
            .spawns
            .complete_data(key)?
            .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?;
        self.go_load_operands = Some((
            data.row.entry,
            data.row.pose.map(f32::to_bits),
            data.row.rotation.map(f32::to_bits),
        ));
        self.go_animprogress = Some(data.animprogress);
        self.entry = Some(data.row.entry);
        Ok(GameObjectSpawn {
            database_guid: key.guid,
            entry: data.row.entry,
            position_bits: data.row.pose.map(f32::to_bits),
            rotation_bits: data.row.rotation.map(f32::to_bits),
            animation: data.animprogress,
            state: data.go_state,
            respawn_min: data.row.respawn[0] as i32,
            respawn_max: data.row.respawn[1] as i32,
        })
    }
    fn gameobject_template(&mut self, entry: u32) -> Result<Option<GameObjectTemplate>> {
        self.lookups.push("go_template");
        let template = self
            .gameobject_templates
            .get(entry)
            .map(LoadedGameObjectInfo::template);
        self.go_factory_elevator.get_or_insert(
            source_gameobject_factory(template.as_ref()) == GameObjectFactoryKind::Elevator,
        );
        Ok(template)
    }
    fn gameobject_thread(&self) -> u64 {
        self.thread
    }
    fn gameobject_event_guid(&mut self, guid: u32) -> Result<bool> {
        self.lookups.push("go_event_guid");
        Ok(self
            .spawns
            .complete_data(SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid,
            })?
            .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?
            .row
            .event
            != 0)
    }
    fn observe_gameobject_factory(&mut self, observation: GameObjectFactoryObservation) {
        match observation {
            GameObjectFactoryObservation::DynamicGuidLookup => {
                self.lookups.push("go_force_dynamic");
            }
            GameObjectFactoryObservation::LargeObjectPredicate => {
                self.lookups.push("go_large_object_predicate");
            }
            GameObjectFactoryObservation::NullAi => {
                self.lookups.push("go_ai_reset_null");
                self.lookups.push("go_ai_just_spawned_skipped");
            }
            GameObjectFactoryObservation::ScriptAiActiveInserted => {
                self.lookups.push("go_ai_active_inserted");
            }
            GameObjectFactoryObservation::ScriptAiFactoryReturned => {
                self.lookups.push("go_ai_factory_returned");
            }
            GameObjectFactoryObservation::ScriptAiJustSpawned => {
                self.lookups.push("go_ai_just_spawned");
            }
            GameObjectFactoryObservation::OutdoorOnObjectCreate => {
                self.lookups.push("go_outdoor_pvp_on_object_create");
            }
            GameObjectFactoryObservation::InstanceOnObjectCreate => {
                self.lookups.push("go_instance_on_object_create");
            }
            GameObjectFactoryObservation::InstanceOnObjectSpawn => {
                self.lookups.push("go_instance_on_object_spawn");
            }
        }
    }
    fn gameobject_group(&mut self, guid: u32) -> Result<Option<GameObjectGroupEntry>> {
        self.lookups.push("go_spawn_group");
        self.groups.gameobject_group(self.guids.map(), guid)
    }
    fn gameobject_group_entry(&mut self, group: u32, guid: u32) -> Result<u32> {
        let identity = self
            .groups
            .group_for(
                self.guids.map(),
                SpawnKey {
                    kind: GridSpawnKind::GameObject,
                    guid,
                },
            )?
            .ok_or_else(|| refuse("execution.group.constructor_group_absent"))?;
        require(identity.handle.id == group, "execution.group.identity")?;
        self.groups.guid_entry(identity, guid)
    }
    fn gameobject_random_entries(&mut self, guid: u32) -> Result<Option<Arc<[u32]>>> {
        self.lookups.push("go_random_entries");
        Ok(self
            .inputs
            .random
            .get(&SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid,
            })
            .map(|(_, entries)| Arc::from(entries.as_slice())))
    }
    fn gameobject_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.random.creature_irand(min, max)
    }
    fn gameobject_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.random.creature_urand(min, max)
    }
    fn gameobject_read_persistent_respawn(
        &mut self,
        guid: u32,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<i64> {
        let saved = context.saved_respawn(GridSpawnKind::GameObject, guid)?;
        self.lookups.push("go_persistent_respawn_read");
        Ok(saved)
    }
    fn gameobject_sin_cos(&mut self, half_yaw_bits: u32) -> Result<[u32; 2]> {
        let _binding = self.trig_dispatch;
        Ok(benilla_sim::source_trig_fma::sin_cos_bits(half_yaw_bits))
    }
    fn gameobject_model(
        &mut self,
        handle: ObjectHandle,
        operation: GameObjectModelOperation,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<bool> {
        if !self.candidates.contains(&handle) {
            self.candidates.push(handle);
        }
        self.lookups.push("go_model");
        let value = self.gameobjects.get(handle)?;
        self.entry = Some(value.template.entry);
        self.dynamic_models.operation(
            &mut self.dynamic_map,
            &mut context.products.vmap,
            DynamicGameObject {
                handle,
                display: value.display,
                pose: context.objects.pose(handle)?,
                scale_bits: value.template.scale_bits,
            },
            operation,
        )
    }
    fn gameobject_is_battleground(
        &mut self,
        handle: ObjectHandle,
        objects: &SourceObjects,
    ) -> Result<bool> {
        let battleground = self.map_entry.battleground();
        self.go_battleground = Some(battleground);
        self.lookups.push("go_battleground");
        if self.map_entry.id == 1 && objects.get(handle)?.database_guid == 32019 {
            assert_eq!(self.gameobjects.get(handle)?.template.entry, 176768);
            assert_eq!(
                self.swimming_census,
                SourceCreatureSwimmingCensus {
                    reached_441: 2,
                    swimming_not_set: 2,
                    ..SourceCreatureSwimmingCensus::default()
                }
            );
            assert!(self.lookups.ends_with(&[
                "go_template",
                "go_template",
                "go_model",
                "go_battleground",
            ]));
        }
        Ok(battleground)
    }
    fn gameobject_zone(
        &mut self,
        handle: ObjectHandle,
        position: [f32; 3],
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<u32> {
        let location = context
            .products
            .zone_and_area(self.map_entry.id, position)?;
        self.zone_lookups.insert(
            SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid: context.objects.get(handle)?.database_guid,
            },
            location,
        );
        self.go_zone = Some(location[0]);
        self.lookups.push("go_zone");
        Ok(location[0])
    }
    fn gameobject_area(
        &mut self,
        handle: ObjectHandle,
        position: [f32; 3],
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<u32> {
        self.lookups.push("go_ai_area");
        // GridMap.cpp:1397-1403,1415-1421 resolve the same area field.
        let before = context.products.wmo_area_lookups.len();
        let area = context
            .products
            .zone_and_area(self.map_entry.id, position)?[1];
        if self.map_entry.id == 0 && context.objects.get(handle)?.database_guid == 15508 {
            // :60637844 and :60638050 resolve the same WMO group.
            let hits = &context.products.wmo_area_lookups[before..];
            assert_eq!(hits.len(), 1);
            assert_eq!(hits[0], (0, [138, 2, 756], None, 10757));
            // WMOAreaTable row 20490 has no area. Tile 0005134 cell 209
            // supplies flag 147, which AreaTable maps to area 42 on map 0.
            assert_eq!(area, 42);
        }
        Ok(area)
    }
    fn gameobject_outdoor_script(&mut self, zone: u32) -> Result<Option<OutdoorPvpScript>> {
        self.lookups.push("go_outdoor_pvp_get_script");
        self.outdoor_pvp.get_script(zone)
    }
    fn gameobject_capture_point_slider(
        &mut self,
        entry: u32,
    ) -> Result<Option<CapturePointSlider>> {
        Ok(self.outdoor_pvp.capture_point_slider(entry))
    }
    fn gameobject_outdoor_pvp(&mut self) -> Result<&mut SourceOutdoorPvp> {
        Ok(self.outdoor_pvp)
    }
    fn gameobject_instance_data(
        &mut self,
    ) -> Result<Option<(&SourceGameObjects, &mut SourceWorldScript)>> {
        self.lookups.push("go_instance_data");
        Ok(self
            .instance_data
            .as_deref_mut()
            .map(|script| (&self.gameobjects, script)))
    }
    fn gameobject_map_visibility_distance(&mut self) -> Result<u32> {
        Ok(self.map_visibility_bits)
    }
    fn gameobject_visibility_census(&mut self) -> Result<&mut SourceVisibilityCensus> {
        Ok(&mut self.visibility)
    }
    fn gameobject_script_id(&mut self, entry: u32) -> Result<u32> {
        self.lookups.push("go_ai_script_factory_lookup");
        self.lookups.push("go_ai_gameobject_info");
        let id = self
            .gameobject_templates
            .get(entry)
            .ok_or_else(|| refuse("execution.gameobject.template_binding"))?
            .words[37];
        self.go_script_id = Some(id);
        Ok(id)
    }
    fn gameobject_script_registry(&mut self) -> Result<&SourceScriptRegistry> {
        self.lookups.push("go_ai_script_registry_lookup");
        Ok(self.scripts)
    }
    fn gameobject_callback(
        &mut self,
        handle: ObjectHandle,
        callback: GameObjectCallback,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        match callback {
            GameObjectCallback::MapViewpoint => self.lookups.push("go_map_viewpoint_added"),
            GameObjectCallback::MapVisibility => self.lookups.push("go_map_visibility_traversal"),
            GameObjectCallback::InitializeAi
                if self.map_entry.id == 0
                    && context.objects.get(handle)?.database_guid == 15508 =>
            {
                self.lookups.push("go15508_ai_initialize");
            }
            GameObjectCallback::InitializeAi
                if self.map_entry.id == 1
                    && context.objects.get(handle)?.database_guid == 49337 =>
            {
                self.lookups.push("go49337_ai_initialize");
            }
            _ => {}
        }
        gameobjects::execute_source_gameobject_callback(self, handle, callback, context)?;
        if callback == GameObjectCallback::MapVisibility {
            self.lookups.push("go_map_visibility_complete");
        }
        Ok(())
    }
}
