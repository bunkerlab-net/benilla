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
    fn gameobject_saved_respawn(&mut self, _: u32) -> Result<i64> {
        Err(refuse(
            "execution.gameobject.loaded_persistent_respawn_owner",
        ))
    }
    fn gameobject_save_respawn(&mut self, _: u32, _: i64) -> Result<()> {
        Err(refuse("execution.gameobject.loaded_save_respawn_owner"))
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
            return if self.gameobjects.get(handle)?.template.large() {
                Err(refuse("execution.gameobject.loaded_large_visibility_owner"))
            } else {
                Ok(())
            };
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
            GameObjectCallback::LargeVisibility => {
                "execution.gameobject.loaded_large_visibility_owner"
            }
            GameObjectCallback::BindGroup(_) => "execution.gameobject.loaded_bind_group_owner",
            GameObjectCallback::MapViewpoint => "execution.gameobject.loaded_map_viewpoint_owner",
            GameObjectCallback::MapVisibility => "execution.gameobject.loaded_map_visibility_owner",
            GameObjectCallback::InitializeAi => "execution.gameobject.loaded_ai_initialize_owner",
            GameObjectCallback::RemoveOutdoorAndOwner => "execution.gameobject.loaded_remove_owner",
        }))
    }
}
