// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:906-934,1687-1740; Unit.cpp:9389-9395,10957-10970.
use super::*;
fn apply_addon<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    data: &EffectiveSpawnData,
    template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    // Creature.cpp:439-444 precedes the former :3142 boundary. GUID27036
    // has no water inhabit bit (trace60433354 and SQL snapshot line 425150).
    if prefix.map_entry.id == 1 && data.row.guid == 27036 {
        assert_eq!(
            prefix.swimming_census,
            SourceCreatureSwimmingCensus {
                reached_441: 2,
                swimming_not_set: 2,
                ..SourceCreatureSwimmingCensus::default()
            }
        );
    }
    prefix.lookups.push("creature_load_addon");
    let guid = context
        .objects
        .get(object)?
        .guid
        .ok_or_else(|| refuse("execution.object_guid_absent"))?;
    let addon = get_creature_addon(
        Some(prefix.creature_addons),
        if guid.high() == HighGuid::Pet as u16 {
            HighGuid::Pet
        } else {
            HighGuid::Creature
        },
        data.row.guid,
        template.words[0],
    )?;
    if guid.high() != HighGuid::Pet as u16 {
        prefix.lookups.push("creature_db_addon_lookup");
    }
    if guid.high() == HighGuid::Pet as u16
        || prefix
            .creature_addons
            .table_get(CreatureAddonTable::Guid, data.row.guid)
            .is_none()
    {
        prefix.lookups.push("creature_template_addon_lookup");
    }
    let fields = prefix.creature_entry.as_mut().unwrap();
    let live = prefix.units.get_mut(context.objects, object)?;
    let flags = live.flags;
    let movement = live.movement.as_ref().unwrap().flags.unwrap();
    let level = live.power_fields.as_ref().unwrap().level;
    if prefix.map_entry.id == 0 && data.row.guid == 40520 {
        assert!(!live.auras.has_aura_holders());
        assert!(live.auras.aura_type_ids().next().is_none());
        assert!(prefix.lookups.ends_with(&[
            "creature_load_addon",
            "creature_db_addon_lookup",
            "creature_template_addon_lookup",
        ]));
    }
    // Unit.cpp:330-366; Unit.h:2487,2507 construct the aura lists empty.
    // UpdateEntry has applied its immunity set, but has not cast a spell.
    let unit = SpellUnitState {
        player: false,
        gameobject: false,
        alive: true,
        standing: live.stand_state == 0,
        player_controlled: flags & 8 != 0,
        combat: flags & UNIT_FLAG_COMBAT != 0,
        battleground_wait_leave: false,
        gm: false,
        moving: movement & 0x0400_60cf != 0,
        falling_far: movement & 0x4000 != 0,
        stealth: live.auras.has_aura_type(16),
        form: live.shapeshift_form,
        class: live.unit_class.unwrap(),
        combo_target: None,
        mounted: live.is_mounted(),
        taxi: flags & 0x0010_0000 != 0,
        totem: false, // Generic Creature subtype, Creature.h:595,622.
        untargetable: flags & 0x0001_0000 != 0,
        has_ai: false,
        position: context.objects.get(object)?.pose.unwrap().0,
    };
    if context.objects.get(object)?.database_guid == 40520 {
        assert_eq!(live.cast_speed, fields.cast_speed);
        assert_eq!(live.power_type, Some(fields.power));
    }
    let raw = prefix.random.creature_raw_position()?;
    let result = fields.addon.load(
        addon,
        false,
        context.objects,
        object,
        &mut prefix.units,
        &mut addon_cast::Execution {
            terrain: context.products,
            stores: prefix.spell_stores,
            targets: &prefix.spell_targets,
            unit,
            level,
            clocks: prefix.clocks,
            world_clock: prefix.world_clock,
            random: prefix.random,
            creature_spells: prefix.creature_spells,
            raw,
            max_casts: addon_cast::max_casts(prefix.config)?,
        },
    );
    fields.power = prefix
        .units
        .get(context.objects, object)?
        .power_type
        .unwrap();
    result?;
    prefix.lookups.push("creature_addon_applied");
    prefix.lookups.push("creature_create_return");
    Ok(())
}

fn map_add_visibility<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    context.objects.map_add_visibility(
        SourceVisibilityUpdate {
            object,
            map_distance_bits: prefix.map_visibility_bits,
            bounding_radius_bits: prefix
                .units
                .get(context.objects, object)?
                .bounding_radius
                .unwrap(),
        },
        context.cells,
        &mut prefix.visibility,
        |_, _, _, _| {
            Err(refuse(
                "execution.visibility.GridNotifiers_36.camera_update_owner",
            ))
        },
        |_, _, _, _| Err(refuse("execution.visibility.Map_1348.player_update_owner")),
    )?;
    prefix.lookups.push("creature_map_visibility_complete");
    Ok(())
}

fn create_notifications<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    object: ObjectHandle,
    database: u32,
    template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<()> {
    use CreatureCreateObservation::*;
    let map = prefix.map_entry;
    notify_loaded_creature_create(
        context.objects,
        &prefix.units,
        object,
        map,
        prefix.outdoor_pvp,
        prefix.instance_data.as_deref_mut(),
        |pose| context.products.zone_and_area(map.id, pose),
        |observation| match observation {
            Battleground(battleground) => {
                prefix.creature_battleground = Some(battleground);
                prefix.lookups.push("creature_create_battleground");
                if map.id == 0 && database == 636 {
                    let current = prefix.creature_entry.as_ref().unwrap();
                    assert_eq!((template.words[0], current.equipment), (14508, 14508));
                    assert_eq!(current.virtual_items, [7420, 0, 0]);
                    assert_eq!(current.virtual_item_info, [[0x0d010702, 3], [0, 0], [0, 0]]);
                    assert_ne!(template.words[17] & 2, 0);
                    assert_eq!(template.words[23] & 0x8000, 0);
                    assert_eq!(prefix.swimming_census.reached_441, 1);
                    assert_eq!(prefix.swimming_census.swimming_not_set, 1);
                    assert_eq!(
                        current.movement_flags
                            & benilla_sim::creature_capabilities::MOVEFLAG_SWIMMING,
                        0
                    );
                    assert!(prefix.lookups.contains(&"creature_init_entry_remainder"));
                    assert!(prefix.lookups.ends_with(&[
                        "creature_final_position_relocated_valid",
                        "creature_create_battleground",
                    ]));
                    assert_eq!(
                        (prefix.creature_battleground, prefix.creature_zone),
                        (Some(false), None)
                    );
                }
            }
            Zone(location) => {
                prefix.zone_lookups.insert(
                    SpawnKey {
                        kind: GridSpawnKind::Creature,
                        guid: database,
                    },
                    location,
                );
                prefix.creature_zone = Some(location[0]);
                prefix.lookups.push("creature_create_zone");
            }
            OutdoorScriptLookup => prefix.lookups.push("creature_outdoor_pvp_get_script"),
            SilithusCreatureCreate => prefix.lookups.push("creature_silithus_pvp_on_create"),
            EasternPlaguelandsCreatureCreate { entry, arm } => {
                if map.id == 0 && database == 59514 {
                    assert_eq!((entry, arm), (8602, PlaguelandsCreatureCreate::NoCase));
                    assert_eq!(prefix.creature_zone, Some(139));
                    assert_eq!(
                        prefix.lookups.last(),
                        Some(&"creature_outdoor_pvp_get_script")
                    );
                }
                prefix.lookups.push("creature_ep_pvp_on_create");
            }
            InstanceDataLookup => prefix.lookups.push("creature_instance_data"),
            InstanceScriptCalled => prefix.lookups.push("creature_instance_on_creature_create"),
        },
    )
}

fn bind_linking(
    prefix: &mut Prefix<'_>,
    state: &mut CreatureConstruction,
    objects: &SourceObjects,
) -> Result<()> {
    let result = register_loaded_creature_linking(
        state,
        prefix.linking,
        prefix.link_holder,
        objects,
        &mut prefix.units,
        |notice| {
            prefix.lookups.push(match notice {
                CreatureLinkingRegistrationObservation::SlaveLookup => {
                    "creature_linking_slave_lookup"
                }
                CreatureLinkingRegistrationObservation::TriggerLookup => {
                    "creature_linking_trigger_lookup"
                }
            });
        },
    );
    prefix.creature_linking_trigger = state.linking_trigger;
    result
}

#[path = "seal16_addon_cast.rs"]
mod addon_cast;
#[path = "seal16_creature_ai.rs"]
mod ai_initialize;
#[path = "seal16_creature_motion.rs"]
mod motion_initialize;

fn db_creature_spawn(state: &CreatureConstruction) -> Result<&CreatureSpawn> {
    state
        .spawn
        .as_ref()
        .ok_or_else(|| refuse("execution.creature.Creature_1676.spawn_data_owner"))
}

impl<'a> Prefix<'a> {
    pub(super) fn loaded_creature_spawn(&self, guid: u32) -> Result<&'a EffectiveSpawnData> {
        self.spawns
            .complete_data(SpawnKey {
                kind: GridSpawnKind::Creature,
                guid,
            })?
            .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))
    }

    pub(super) fn loaded_creature_template(&self, entry: u32) -> Result<&'a LoadedCreatureInfo> {
        self.creature_templates
            .get(entry)
            .ok_or_else(|| refuse("execution.creature.template_binding"))
    }

    fn loaded_creature_entry(
        &self,
        state: &CreatureConstruction,
    ) -> Result<LoadedCreatureEntry<'a>> {
        let data = self.loaded_creature_spawn(db_creature_spawn(state)?.database_guid)?;
        let spawn = data
            .spawn_template
            .and_then(|entry| self.spawn_templates.rows().get(&entry))
            .ok_or_else(|| refuse("execution.creature.spawn_template_undefined"))?;
        Ok(LoadedCreatureEntry {
            original: self.loaded_creature_template(state.original_entry)?,
            effective: self.loaded_creature_template(state.template.entry)?,
            spawn: Some(spawn),
            movement: u32::from(data.row.movement),
            map: self.map_entry.id,
        })
    }
}

impl<D: GridDefinitions<Row = SpawnKey>> CreatureFactoryOwner<D, SourceTerrainManager>
    for Prefix<'_>
{
    fn creature_thread(&self) -> u64 {
        self.thread
    }

    fn creature_construct_candidate(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        CreatureConstruction::construct_loaded_candidate(
            object,
            context.objects,
            &mut self.units,
            self.passengers,
            &mut self.motions,
        )?;
        self.candidates.push(object);
        Ok(())
    }

    fn creature_spawn(
        &mut self,
        _: GridLoadAttempt,
        key: SpawnKey,
    ) -> Result<Option<CreatureSpawn>> {
        let data = self.loaded_creature_spawn(key.guid)?;
        self.lookups.push("creature_data");
        let row = &data.row;
        Ok(Some(CreatureSpawn {
            database_guid: row.guid,
            entry: row.entry,
            position: row.pose,
            // Creature.cpp:666 and :1742 access the spawn template at different sites.
            model: 0,
            respawn_seconds: row.respawn,
            respawn_radius: row.radius,
            relay: 0,
        }))
    }

    fn creature_event_guid(&mut self, _: u32) -> Result<bool> {
        // World.cpp:1383-1420 initializes events after the continent jobs join.
        let events = self.events.inventory();
        require(
            !events.initialized && events.active.is_empty(),
            "execution.creature.ObjectGridLoader_134.IsEventGuid_owner",
        )?;
        Ok(false)
    }

    fn creature_force_dynamic(&mut self) -> Result<bool> {
        Ok(source_map_forces_dynamic_guids(self.guids.map().map))
    }

    fn creature_force_load_map(&mut self) -> Result<bool> {
        // Bound to the empty forced-map set before active loading.
        Ok(false)
    }

    fn creature_group(&mut self, database: u32) -> Result<Option<CreatureGroup>> {
        let group = self.groups.creature_group(self.guids.map(), database)?;
        self.lookups.push("spawn_group");
        Ok(group)
    }

    fn creature_current_group(
        &mut self,
        object: ObjectHandle,
        context: &GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<Option<CreatureGroup>> {
        let live = context.objects.get(object)?;
        let map = live
            .map
            .ok_or_else(|| refuse("execution.creature.map_binding"))?;
        if self
            .groups
            .creature_group(map, live.database_guid)?
            .is_some()
        {
            return Err(refuse(
                "execution.creature.Creature_1751.GetFormationData_owner",
            ));
        }
        self.lookups.push("creature_respawn_group_checked");
        Ok(None)
    }

    fn creature_entry(&mut self, _: u32, owner: CreatureEntryOwner) -> Result<u32> {
        Err(refuse(match owner {
            CreatureEntryOwner::Group(_) => "execution.creature.loaded_group_entry_owner",
            CreatureEntryOwner::Conditional => "execution.creature.loaded_conditional_entry_owner",
            CreatureEntryOwner::Random { .. } => "execution.creature.loaded_random_entry_owner",
        }))
    }

    fn creature_event(&mut self, database: u32) -> Result<Option<CreatureEventData>> {
        let event = self.events.creature_event_data(database)?;
        self.lookups.push("active_event");
        Ok(event)
    }

    fn creature_template(&mut self, entry: u32) -> Result<Option<CreatureTemplate>> {
        Ok(self
            .creature_templates
            .get(entry)
            .map(CreatureTemplate::from))
    }

    fn creature_model(&mut self, display: u32) -> Result<Option<CreatureModel>> {
        Ok(self.creature_models.get(display).map(CreatureModel::from))
    }

    fn creature_spawn_model(&mut self, spawn: &CreatureSpawn) -> Result<u32> {
        let model = self
            .spawn_templates
            .model_id(self.loaded_creature_spawn(spawn.database_guid)?)?;
        self.lookups.push("creature_spawn_template_model");
        Ok(model)
    }

    fn observe_creature_factory(&mut self, observation: CreatureFactoryObservation) {
        use CreatureFactoryObservation::*;
        let lookup = match observation {
            FinalPositionRelocated => "creature_final_position_relocated_valid",
            RespawnDelay(delay) => {
                self.creature_entry.as_mut().unwrap().respawn_delay = Some(delay);
                "creature_respawn_delay_draw"
            }
            RespawnTime { time, corpse_delay } => {
                let fields = self.creature_entry.as_mut().unwrap();
                fields.respawn_time = Some(time);
                fields.corpse_delay = corpse_delay;
                return;
            }
            MapGridLinked => "creature_map_add_grid_link",
            WorldLookupRegistered => "creature_world_guid_registered",
            BaseAddedToWorld => "creature_base_add_to_world",
            AiNotifyEnqueued(delay) => {
                self.creature_entry.as_mut().unwrap().notify_delay = Some(delay);
                self.lookups.push("creature_schedule_ai_notify");
                "creature_ai_notify_enqueued"
            }
            ForceActiveChecked => "creature_force_active_checked",
            SpawnCountChecked => "creature_spawn_count_checked",
            MapActiveChecked => "creature_map_active_checked",
            NoExistingCreature => "existing_creature",
            ConditionalSkipped => "conditional_skipped_nonzero_entry",
            RandomSkipped => "random_skipped_nonzero_entry",
            OriginalTemplate(entry) => {
                self.entry = Some(entry);
                "creature_template"
            }
            GuidAllocated => "creature_guid",
            CreateFromProto => "creature_create_from_proto",
            DisplayChance(chance) => {
                self.creature_display_chance = Some(chance);
                "creature_display_chance"
            }
            DisplayDrawn => "creature_display_draw",
            DisplayFallback => "creature_display_fallback_template",
            ModelSelected => "creature_model_random_gender",
            MissingModelGender(notice) => panic!("unexpected missing alternate gender: {notice:?}"),
        };
        self.lookups.push(lookup);
    }

    fn creature_guids(&mut self) -> &mut MapGuidAllocators {
        self.guids
    }

    fn creature_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        Ok(self.random)
    }

    fn creature_units(&self) -> &SourceUnits {
        &self.units
    }

    fn creature_respawn_time(
        &mut self,
        database: u32,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<i64> {
        let time = context.saved_respawn(GridSpawnKind::Creature, database)?;
        self.lookups.push("creature_persistent_respawn_read");
        Ok(time)
    }

    fn creature_save_respawn_time(
        &mut self,
        _: u32,
        _: i64,
        _: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        Err(refuse(
            "execution.creature.Creature_1717.loaded_save_respawn_owner",
        ))
    }

    fn creature_wall_time(&mut self, _: ObjectHandle) -> Result<ClockSample> {
        let clock = self
            .clocks
            .creature_respawn(self.thread, self.random.creature_raw_position()?)?;
        self.creature_entry.as_mut().unwrap().respawn_clock =
            Some((clock.site_line, clock.time_line, clock.seconds));
        self.lookups.push("creature_respawn_calendar_consumed");
        Ok(ClockSample::authored(ClockKind::WallSeconds, clock.seconds))
    }

    fn creature_spawned_by_linked_mob(
        &mut self,
        state: &CreatureConstruction,
        context: &GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<bool> {
        self.lookups.push("creature_spawned_by_linked_mob_lookup");
        Ok(
            source_creature_link_info(self.linking, context.objects, &self.units, state.object)?
                .is_some_and(|info| info.spawning_linked()),
        )
    }

    fn creature_linked_can_spawn(
        &mut self,
        state: &CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<bool> {
        let map = context
            .objects
            .get(state.object)?
            .map
            .ok_or_else(|| refuse("execution.creature.map_binding"))?;
        let entry = self.units.get(context.objects, state.object)?.entry;
        let (managers, persistent) = context.definitions.persistent_state(map)?;
        self.link_holder.can_spawn_creature(
            self.linking,
            context.objects,
            managers.state(persistent)?,
            &mut MapLinkingGate {
                spawns: self.spawns,
                pools: self.pools,
                script: self.instance_data.as_deref(),
            },
            state.object,
            entry,
        )
    }

    fn creature_dead_flying_height(
        &mut self,
        state: &CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<f32> {
        let map = context
            .objects
            .get(state.object)?
            .map
            .ok_or_else(|| refuse("execution.creature.map_binding"))?;
        let [x, y, z, _] = db_creature_spawn(state)?.position;
        // Creature.cpp:1708,1731 and GridDefines.h:63 use static height, no VMaps, range 10.
        context
            .products
            .height_static(map.map, [x, y, z], false, 10.)
    }
    fn creature_units_mut(&mut self) -> &mut SourceUnits {
        &mut self.units
    }

    fn creature_spawn_relay(&mut self, state: &CreatureConstruction) -> Result<u32> {
        // Creature.cpp:1742 skips relay for null CreatureData.
        let Some(spawn) = &state.spawn else {
            return Ok(0);
        };
        let template = self
            .loaded_creature_spawn(spawn.database_guid)?
            .spawn_template
            .and_then(|entry| self.spawn_templates.rows().get(&entry))
            .ok_or_else(|| refuse("execution.creature.Creature_1742.spawn_template_undefined"))?;
        if template.relay_id == 0 {
            self.lookups.push("creature_spawn_relay_checked");
        }
        Ok(template.relay_id)
    }
    fn creature_select_ai(
        &mut self,
        state: &CreatureConstruction,
        objects: &SourceObjects,
    ) -> Result<SelectedCreatureAi> {
        let template = self.loaded_creature_template(state.template.entry)?;
        self.ai_registry.select_loaded_db_creature(
            objects,
            state.object,
            self.units.get(objects, state.object)?,
            template,
            self.scripts,
            self.ai_factions,
            |notice| {
                self.lookups.push(match notice {
                    SourceCreatureAiScriptObservation::RegistryLookup => {
                        "creature_ai_script_registry_lookup"
                    }
                    SourceCreatureAiScriptObservation::NullFactory => "creature_ai_script_null",
                    SourceCreatureAiScriptObservation::BuiltinRegistry => {
                        "creature_ai_registry_bound"
                    }
                })
            },
        )
    }

    fn creature_builtin_ai_owners(
        &mut self,
        selected: &SelectedCreatureAi,
    ) -> Result<BuiltinAiFactoryOwners<'_>> {
        let template = self
            .ai_templates
            .get(&selected.inputs().effective_entry)
            .ok_or_else(|| refuse("execution.unit_ai.effective_entry"))?;
        Ok(BuiltinAiFactoryOwners {
            units: &mut self.units,
            ais: &mut self.ais,
            random: self.random,
            base: BuiltinAiBase::Loaded {
                combat: &mut self.combat,
                template,
                config: self.ai_config,
            },
        })
    }

    fn creature_construct_scripted_ai(
        &mut self,
        selected: &SelectedCreatureAi,
        kind: SourceScriptedAiKind,
        binding: &std::sync::Arc<ScriptDevAiBinding>,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        if kind != SourceScriptedAiKind::Solenor {
            let owners = <Self as CreatureFactoryOwner<D, SourceTerrainManager>>::creature_builtin_ai_owners(self, selected)?;
            return SourceCreatureAis::construct_loaded_script(
                context.objects,
                selected,
                kind,
                binding,
                owners,
                &mut (),
            );
        }
        let template = self
            .ai_templates
            .get(&selected.inputs().effective_entry)
            .ok_or_else(|| refuse("execution.unit_ai.effective_entry"))?;
        motion_initialize::with_solenor_binding(
            self,
            context,
            |objects, owners, reset| {
                SourceCreatureAis::construct_loaded_script(
                    objects, selected, kind, binding, owners, reset,
                )
            },
            template,
        )?;
        // The constructor's Reset reads wall seconds 1794314190 at trace :60657681.
        assert_eq!(
            self.units
                .get(context.objects, selected.object())?
                .respawn_time,
            1794316290
        );
        Ok(())
    }

    fn creature_ai_construct(
        &mut self,
        state: &mut CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        ai_initialize::construct(self, state, context)
    }

    fn creature_phase(
        &mut self,
        phase: CreaturePhase,
        state: &mut CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        match phase {
            CreaturePhase::InitEntryBase => {
                let template = self.loaded_creature_template(state.template.entry)?;
                self.lookups.push("init_entry_template");
                let live = self.units.get_mut(context.objects, state.object)?;
                CreatureConstruction::reset_loaded_proto_fields(live);
                CreatureConstruction::init_loaded_entry_base(live, template);
                let operands = (
                    live.entry,
                    live.loaded_creature.scale.unwrap(),
                    [live.loaded_creature.race, live.unit_class.unwrap() as u8],
                );
                assert_eq!(
                    operands,
                    (
                        state.template.entry,
                        template.words[14],
                        [0, template.words[36] as u8]
                    )
                );
                self.init_entry_fields = Some(operands);
                self.lookups.push("init_entry_fields");
                Ok(())
            }
            CreaturePhase::ModelAndDisplay => {
                let model = self.creature_models.get(state.model.unwrap().id).unwrap();
                if self.creature_model.is_none() {
                    eprintln!(
                        "F6_CREATURE_GENDER_MODELS map={} missing_alternate_rows={}",
                        self.map_entry.id,
                        self.creature_models.missing_other_gender_count(),
                    );
                }
                self.creature_model = Some(model.words);
                let template = self.loaded_creature_template(state.template.entry)?;
                creature_entry::apply_model(
                    self,
                    state.object,
                    template,
                    model.words,
                    context.objects,
                )
            }
            CreaturePhase::InitEntryEquipmentAndFlags => {
                let entry = self.loaded_creature_entry(state)?;
                state.corpse_delay = 60; // Creature.cpp:140 constructor before the InitEntry policy.
                creature_entry::finish_init_entry(
                    self,
                    state,
                    &entry,
                    context.products,
                    context.objects,
                )
            }
            CreaturePhase::SelectLevelStats => {
                let entry = self.loaded_creature_entry(state)?;
                creature_entry::apply_level(self, state, &entry, context.objects)
            }
            CreaturePhase::UpdateEntryFlagsAndSpells => {
                let entry = self.loaded_creature_entry(state)?;
                creature_entry::update_entry(self, state, &entry, context.objects)
            }
            CreaturePhase::SelectFinalPointAndRelocate => {
                self.lookups.push("creature_select_final_point_static");
                Ok(())
            }
            CreaturePhase::PvpAndInstanceCreate => {
                let template = self.loaded_creature_template(state.template.entry)?;
                create_notifications(
                    self,
                    state.object,
                    db_creature_spawn(state)?.database_guid,
                    template,
                    context,
                )
            }
            CreaturePhase::LinkingRegistration => bind_linking(self, state, context.objects),
            CreaturePhase::LoadAddon => {
                let data = self.loaded_creature_spawn(db_creature_spawn(state)?.database_guid)?;
                let template = self.loaded_creature_template(state.template.entry)?;
                apply_addon(self, state.object, data, template, context)
            }
            CreaturePhase::AttachGroup => Err(refuse(
                "execution.creature.Creature_1681.loaded_creature_group_owner",
            )),
            CreaturePhase::RespawnDefinition => {
                let radius = db_creature_spawn(state)?.respawn_radius;
                self.motions
                    .get_mut(&state.object)
                    .ok_or_else(|| refuse("execution.creature.respawn_motion_binding"))?
                    .bind_respawn_radius(radius);
                self.creature_entry.as_mut().unwrap().respawn_radius = Some(radius.to_bits());
                self.lookups.push("creature_respawn_coordinate_radius");
                Ok(())
            }
            CreaturePhase::SetDeadAndZeroHealth => CreatureConstruction::clear_loaded_health(
                self.units.get_mut(context.objects, state.object)?,
            ),
            CreaturePhase::MapAddToWorld => require(
                context.objects.get(state.object)?.map == Some(self.guids.map()),
                "execution.creature.map_binding",
            ),
            CreaturePhase::MapAddViewpoint => {
                let cell = context
                    .cells
                    .registered_cell(state.object)
                    .ok_or_else(|| refuse("execution.grid_candidate_not_registered"))?;
                context.objects.viewpoint_added_to_world(
                    state.object,
                    cell,
                    context.cells,
                    |objects, cells, camera| {
                        <Self as GridObjectFactory<D, SourceTerrainManager>>::camera_added_to_world(
                            self, objects, cells, camera,
                        )
                    },
                )?;
                self.lookups.push("creature_map_viewpoint_added");
                Ok(())
            }
            CreaturePhase::MapAddVisibility => map_add_visibility(self, state.object, context),
            CreaturePhase::MotionInitialize => {
                motion_initialize::initialize(self, state.object, context)?;
                self.lookups.push("creature_motion_initialized");
                Ok(())
            }
            CreaturePhase::LinkingRespawn => {
                motion_initialize::linking_respawn(self, state.object, context)
            }
            CreaturePhase::Relay(_) => Err(refuse(
                "execution.creature.Creature_1743.ScriptsStart_owner",
            )),
            CreaturePhase::AiSpellListChanged | CreaturePhase::AiJustRespawned => {
                ai_initialize::callback(self, phase, state.object, context)
            }
            CreaturePhase::InstanceRespawnAndCreatorLoot => {
                ai_initialize::finish(self, state.object, context.objects)
            }
            _ => Err(phase.missing_owner()),
        }
    }

    fn creature_created(
        &mut self,
        state: CreatureConstruction,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        state.check_builtin_ai_discard(context.objects, &self.ais)?;
        if let Some(AiExecutable::ScriptDevGetAi(binding)) =
            state.selected_ai().map(SelectedCreatureAi::executable)
        {
            let ai = self.ais.get(context.objects, state.object)?;
            assert!(
                matches!(&ai.loaded_class, LoadedCreatureAiClass::Scripted(script)
                    if script.binding() == binding.as_ref())
            );
            self.lookups.push("creature_scripted_ai_retained");
        }
        self.lookups.push("creature_load_from_db_return");
        Ok(())
    }

    fn creature_destroy_candidate(
        &mut self,
        object: ObjectHandle,
        _: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        CreatureConstruction::discard_loaded_candidate(
            object,
            &mut self.units,
            self.passengers,
            &mut self.motions,
        )
    }
}

impl CreatureFactoryConfig for Prefix<'_> {
    fn creature_respawn_aggro_config(&self) -> Result<i32> {
        super::super::super::super::spawn_loader_config::assignment(
            self.config,
            "CreatureRespawnAggroDelay",
        )?
        .1
        .parse()
        .map_err(|_| refuse("execution.creature.loaded_notify_delay_config"))
    }

    fn creature_rabbit_day(&self) -> Result<u32> {
        super::super::super::super::spawn_loader_config::assignment(self.config, "RabbitDay")?
            .1
            .parse()
            .map_err(|_| refuse("execution.creature.rabbit_day_config"))
    }
}
