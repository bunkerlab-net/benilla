// SPDX-License-Identifier: GPL-3.0-or-later
//! GameObject.cpp:178-302,870-969 executes through the existing GO and F6 owners.
use super::*;
use std::sync::Arc;
impl<D: GridDefinitions<Row = SpawnKey>> GameObjectFactoryOwner<D, SourceTerrainManager>
    for Prefix<'_>
{
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
        self.go_factory_elevator
            .get_or_insert(template.as_ref().is_some_and(|t| t.kind == 11));
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
    fn gameobject_force_dynamic(&mut self) -> Result<bool> {
        self.lookups.push("go_force_dynamic");
        Ok(matches!(self.guids.map().map, 533 | 543))
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
    fn gameobject_write_persistent_respawn(
        &mut self,
        _: u32,
        _: i64,
        _: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        Err(refuse(
            "execution.gameobject.loaded_save_respawn_effects_owner",
        ))
    }
    fn gameobject_calendar(&mut self) -> Result<i64> {
        Err(refuse("execution.gameobject.loaded_respawn_calendar_owner"))
    }
    fn gameobject_rotation(&mut self, orientation: u32) -> Result<[u32; 2]> {
        let _binding = self.trig_dispatch;
        Ok(benilla_sim::source_trig_fma::sin_cos_bits(
            (f32::from_bits(orientation) / 2.0).to_bits(),
        ))
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
    fn gameobject_callback(
        &mut self,
        handle: ObjectHandle,
        callback: GameObjectCallback,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        if callback == GameObjectCallback::CreateBattlegroundOrOutdoor {
            let battleground = self.map_entry.battleground();
            self.go_battleground = Some(battleground);
            self.lookups.push("go_battleground");
            if battleground {
                return Err(refuse(
                    "execution.gameobject.loaded_battleground_create_owner",
                ));
            }
            let pose = context.objects.pose(handle)?.0;
            let position = std::array::from_fn(|index| f32::from_bits(pose[index]));
            let zone = context
                .products
                .zone_and_area(self.map_entry.id, position)?[0];
            self.go_zone = Some(zone);
            self.lookups.push("go_zone");
            self.lookups.push("go_outdoor_pvp_get_script");
            if self.outdoor_pvp.get_script(zone)?.is_some() {
                return Err(refuse(
                    "execution.gameobject.loaded_outdoor_pvp_create_owner",
                ));
            }
            return Ok(());
        }
        if callback == GameObjectCallback::CreateInstance {
            self.lookups.push("go_instance_data");
            if let Some(script) = self.instance_data.as_mut() {
                self.lookups.push("go_instance_on_object_create");
                let entry = self.gameobjects.get(handle)?.template.entry;
                let guid = context.objects.get(handle)?.guid.unwrap().raw();
                if script.gameobject_create(entry, guid) {
                    context.set_active_object_state(handle, true, self)?;
                }
            }
            return Ok(());
        }
        if callback == GameObjectCallback::LargeVisibility {
            self.lookups.push("go_large_object_predicate");
            if self.gameobjects.get(handle)?.template.large() {
                context
                    .objects
                    .set_visibility_distance_override(handle, SourceVisibilityDistance::Large)?;
            }
            return Ok(());
        }
        if callback == GameObjectCallback::MapViewpoint {
            self.lookups.push("go_map_viewpoint_added");
            let cell = context
                .cells
                .registered_cell(handle)
                .ok_or_else(|| refuse("execution.visibility.map_add_cell_binding"))?;
            context
                .objects
                .viewpoint_added_to_world(handle, cell, context.cells, |_, _, _| {
                    Err(refuse("execution.camera.Camera_104.added_to_world_owner"))
                })?;
            return Ok(());
        }
        if callback == GameObjectCallback::MapVisibility {
            self.lookups.push("go_map_visibility_traversal");
            context.objects.map_add_visibility(
                SourceVisibilityUpdate {
                    object: handle,
                    map_distance_bits: self.map_visibility_bits,
                    // GameObject.cpp:2209-2212; ObjectDefines.h:42, not model bounds.
                    bounding_radius_bits: 0x3ec7_2b02,
                },
                context.cells,
                &mut self.visibility,
                |_, _, _, _| {
                    Err(refuse(
                        "execution.visibility.GridNotifiers_36.camera_update_owner",
                    ))
                },
                |_, _, _, _| Err(refuse("execution.visibility.Map_1348.player_update_owner")),
            )?;
            self.lookups.push("go_map_visibility_complete");
            return Ok(());
        }
        if callback == GameObjectCallback::InitializeAi {
            self.lookups.push("go_ai_script_factory_lookup");
            // GameObject.cpp:2530 -> ScriptDevAIMgr.cpp:375 evaluates GetScriptId before GetScript.
            self.lookups.push("go_ai_gameobject_info");
            let entry = self.gameobjects.get(handle)?.template.entry;
            self.go_script_id = Some(
                self.gameobject_templates
                    .get(entry)
                    .ok_or_else(|| refuse("execution.gameobject.template_binding"))?
                    .words[37],
            );
            self.lookups.push("go_ai_script_registry_lookup");
            self.scripts.gameobject_ai(self.go_script_id.unwrap())?;
            self.gameobjects.get_mut(handle)?.has_ai = false; // GameObject.cpp:2530.
            self.lookups.push("go_ai_reset_null");
            // GameObject.cpp:2532 skips JustSpawned for the returned null AI.
            self.lookups.push("go_ai_just_spawned_skipped");
            return Ok(());
        }
        Err(refuse(match callback {
            GameObjectCallback::TemplateString(_) | GameObjectCallback::GroupString(_) => {
                "execution.gameobject.loaded_string_id_owner"
            }
            GameObjectCallback::CreateBattlegroundOrOutdoor => {
                "execution.gameobject.loaded_outdoor_pvp_create_owner"
            }
            GameObjectCallback::CreateInstance => {
                "execution.gameobject.loaded_instance_create_owner"
            }
            GameObjectCallback::BindGroup(_) => "execution.gameobject.loaded_bind_group_owner",
            GameObjectCallback::LargeVisibility
            | GameObjectCallback::MapViewpoint
            | GameObjectCallback::MapVisibility
            | GameObjectCallback::InitializeAi => unreachable!("handled above"),
            GameObjectCallback::RemoveOutdoorAndOwner => "execution.gameobject.loaded_remove_owner",
        }))
    }
}
