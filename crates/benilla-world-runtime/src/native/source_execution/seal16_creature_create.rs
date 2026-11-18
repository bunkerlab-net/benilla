// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:906-934,1687-1740; Unit.cpp:9389-9395,10957-10970.
use super::*;
pub(super) fn continue_create<D: GridDefinitions<Row = SpawnKey>>(
    prefix: &mut Prefix<'_>,
    construction: &mut CreatureConstruction,
    data: &EffectiveSpawnData,
    template: &LoadedCreatureInfo,
    context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
) -> Result<GridCandidateOutcome> {
    let object = construction.object;
    // CreatureCreatePos has no close object, and staticSpawn is true, Creature.cpp:105-120.
    let position = data.row.pose;
    prefix.lookups.push("creature_select_final_point_static");
    context
        .objects
        .set_pose(object, SourcePose(position.map(f32::to_bits)))?;
    if !creatures::valid_position(position) {
        prefix.units.discard_construction(object);
        context.objects.destroy(object, context.cells)?;
        return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
    }
    prefix
        .lookups
        .push("creature_final_position_relocated_valid");
    let battleground = prefix.map_entry.battleground();
    prefix.creature_battleground = Some(battleground);
    prefix.lookups.push("creature_create_battleground");
    if battleground {
        return Err(refuse(
            "execution.creature.loaded_battleground_create_owner",
        ));
    }
    if prefix.map_entry.id == 0 && data.row.guid == 636 {
        let current = prefix.creature_entry.as_ref().unwrap();
        assert_eq!((template.words[0], current.equipment), (14508, 14508));
        assert_eq!(current.virtual_items, [7420, 0, 0]);
        assert_eq!(current.virtual_item_info, [[0x0d010702, 3], [0, 0], [0, 0]]);
        assert_ne!(template.words[17] & 2, 0);
        assert_eq!(template.words[23] & 0x8000, 0);
        assert_eq!(prefix.swimming_census.reached_441, 1);
        assert_eq!(prefix.swimming_census.swimming_not_set, 1);
        assert_eq!(
            current.movement_flags & benilla_sim::creature_capabilities::MOVEFLAG_SWIMMING,
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
    let location = context
        .products
        .zone_and_area(prefix.map_entry.id, position[..3].try_into().unwrap())?;
    prefix.zone_lookups.insert(
        SpawnKey {
            kind: GridSpawnKind::Creature,
            guid: data.row.guid,
        },
        location,
    );
    let zone = location[0];
    prefix.creature_zone = Some(zone);
    prefix.lookups.push("creature_create_zone");
    prefix.lookups.push("creature_outdoor_pvp_get_script");
    if prefix.outdoor_pvp.get_script(zone)?.is_some() {
        return Err(refuse("execution.creature.loaded_outdoor_pvp_create_owner"));
    }
    prefix.lookups.push("creature_instance_data");
    if let Some(script) = prefix.instance_data.as_mut() {
        prefix.lookups.push("creature_instance_on_creature_create");
        script.creature_create(
            template.words[0],
            context
                .objects
                .get(object)?
                .guid
                .ok_or_else(|| refuse("execution.object_guid_absent"))?
                .raw(),
        );
    }
    prefix.linking.ensure_loaded()?;
    prefix.lookups.push("creature_linking_slave_lookup");
    let slave_link = prefix
        .linking
        .lookup(template.words[0], data.row.guid, prefix.map_entry.id);
    if slave_link.is_some() {
        prefix
            .link_holder
            .add_slave(prefix.linking, context.objects, object, template.words[0])?;
    }
    prefix.lookups.push("creature_linking_trigger_lookup");
    if prefix
        .linking
        .event_trigger(template.words[0], data.row.guid, prefix.map_entry.id)
    {
        prefix.creature_linking_trigger = true; // Creature.cpp:928, before AddMasterToHolder.
        prefix
            .units
            .get_mut(context.objects, object)?
            .linking_event_trigger = Some(true);
        prefix.link_holder.add_master(
            prefix.linking,
            context.objects,
            object,
            template.words[0],
        )?;
    }
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
        standing: fields.addon.stand == 0,
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
        mounted: fields.addon.mount != 0,
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
        live,
        &mut addon_cast::Execution {
            object,
            objects: context.objects,
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
    fields.power = live.power_type.unwrap();
    result?;
    prefix.lookups.push("creature_addon_applied");
    prefix.lookups.push("creature_create_return");
    if prefix
        .groups
        .creature_group(context.objects.get(object)?.map.unwrap(), data.row.guid)?
        .is_some()
    {
        return Err(refuse(
            "execution.creature.Creature_1681.loaded_creature_group_owner",
        ));
    }
    context
        .objects
        .set_respawn_definition(object, SourcePose(position.map(f32::to_bits)), true)?;
    fields.respawn_radius = Some(data.row.radius.to_bits());
    prefix
        .motions
        .get_mut(&object)
        .unwrap()
        .bind_respawn_radius(data.row.radius);
    prefix.lookups.push("creature_respawn_coordinate_radius");
    let delay = prefix
        .random
        .creature_urand(data.row.respawn[0], data.row.respawn[1])?;
    fields.respawn_delay = Some(delay);
    prefix.lookups.push("creature_respawn_delay_draw");
    // Object.cpp:3076-3078 compares the actual allocated GUID counter.
    let new_spawning = data.row.guid != 0 && data.row.guid != guid.counter();
    if !new_spawning {
        construction.corpse_delay = construction.corpse_delay.min(delay.wrapping_mul(9) / 10);
        fields.corpse_delay = construction.corpse_delay;
    }
    context.objects.get_mut(object)?.alive = true; // Creature.cpp:1694.
    fields.respawn_time = Some(context.saved_respawn(GridSpawnKind::Creature, data.row.guid)?);
    prefix.lookups.push("creature_persistent_respawn_read");
    let clock = prefix
        .clocks
        .creature_respawn(prefix.thread, prefix.random.creature_raw_position()?)?;
    fields.respawn_clock = Some((clock.site_line, clock.time_line, clock.seconds));
    prefix.lookups.push("creature_respawn_calendar_consumed");
    if fields.respawn_time.unwrap() > clock.seconds {
        if new_spawning {
            prefix.units.discard_construction(object);
            context.objects.destroy(object, context.cells)?;
            return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
        }
        context.objects.mark_dead(object)?;
        return Err(refuse("execution.creature.Creature_1705.SetHealth_owner"));
    }
    if fields.respawn_time != Some(0) {
        fields.respawn_time = Some(0); // Creature.cpp:1715, before the unbound save call.
        return Err(refuse(
            "execution.creature.Creature_1717.loaded_save_respawn_owner",
        ));
    }
    prefix.lookups.push("creature_spawned_by_linked_mob_lookup");
    let map = context.objects.get(object)?.map.unwrap();
    let spawn_linked = slave_link.is_some_and(|info| info.spawning_linked());
    if spawn_linked && context.objects.get(object)?.alive {
        let (managers, state) = context.definitions.persistent_state(map)?;
        let can_spawn = prefix.link_holder.can_spawn_creature(
            prefix.linking,
            context.objects,
            managers.state(state)?,
            &mut MapLinkingGate {
                spawns: prefix.spawns,
                pools: prefix.pools,
                script: prefix.instance_data.as_deref(),
            },
            object,
            template.words[0],
        )?;
        if !can_spawn {
            context.objects.mark_dead(object)?;
            // Unit.cpp:9250-9271. This ordinary creature has no player group update.
            let live = prefix.units.get_mut(context.objects, object)?;
            live.vitals.as_mut().unwrap().health = 0;
            if live.can_fly {
                // Creature.cpp:1731 and GridDefines.h:63 disable VMaps and search 10 units.
                let [x, y, z, orientation] = data.row.pose;
                let height = context
                    .products
                    .height_static(map.map, [x, y, z], false, 10.)?;
                if f64::from(z - height) > 0.1 {
                    context.objects.set_pose(
                        object,
                        SourcePose([x, y, height, orientation].map(f32::to_bits)),
                    )?;
                }
            }
        }
    }
    if slave_link.is_some() || prefix.creature_linking_trigger {
        prefix.link_births.insert(
            object,
            (fields.respawn_delay.unwrap(), fields.respawn_time.unwrap()),
        );
    }
    let cell =
        benilla_sim::resident_map::compute_cell_pair(position[0].to_bits(), position[1].to_bits())
            .map_err(|_| refuse("execution.creature.map_add_coordinates"))?;
    if context.objects.get(object)?.active {
        context.load_grid(cell, GridLoadTrigger::Enter, prefix)?;
    } else {
        context.load_grid(cell, GridLoadTrigger::TerrainOnly, prefix)?;
    }
    let cell = context.cells.cell(cell)?;
    context.objects.link_cell(object, cell, context.cells)?;
    prefix.lookups.push("creature_map_add_grid_link");
    context.objects.register_world_lookup(object)?;
    prefix.lookups.push("creature_world_guid_registered");
    // WorldObject ctor :1170 starts notification false; UpdateEntry's string gates ran.
    context.objects.mark_in_world(object)?;
    prefix.lookups.push("creature_base_add_to_world");
    let fields = prefix.creature_entry.as_mut().unwrap();
    // Fresh ordinary Creature, no created-by spell or player controller, Unit.cpp:9393-9394.
    let (_, value) = super::super::super::super::spawn_loader_config::assignment(
        prefix.config,
        "CreatureRespawnAggroDelay",
    )?;
    let delay = value
        .parse::<i32>()
        .map_err(|_| refuse("execution.creature.loaded_notify_delay_config"))?;
    fields.notify_delay = Some(notify_delay(delay));
    prefix
        .units
        .get_mut(context.objects, object)?
        .unit_events
        .schedule_ai_notify(fields.notify_delay.unwrap(), false)?;
    prefix.lookups.push("creature_schedule_ai_notify");
    require(
        context.objects.get(object)?.map == Some(map),
        "execution.creature.map_binding",
    )?;
    prefix.lookups.push("creature_ai_notify_enqueued");
    // World.cpp:485-494 was bound to the empty forced-map set in load_first.
    if template.words[23] & 0x1000 != 0 {
        context.set_active_object_state(object, true, prefix)?;
    }
    prefix.lookups.push("creature_force_active_checked");
    if prefix
        .units
        .get(context.objects, object)?
        .loaded_creature
        .count_spawns
    {
        return Err(refuse("execution.creature.Map_2995.AddToSpawnCount_owner"));
    }
    prefix.lookups.push("creature_spawn_count_checked");
    if context.objects.get(object)?.active {
        context.add_to_active(object, prefix)?;
    }
    prefix.lookups.push("creature_map_active_checked");
    context
        .objects
        .viewpoint_added_to_world(object, cell, context.cells, |_, _, _| {
            Err(refuse("execution.camera.Camera_104.added_to_world_owner"))
        })?;
    prefix.lookups.push("creature_map_viewpoint_added");
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
    motion_initialize::initialize(prefix, object, template, context)?;
    prefix.lookups.push("creature_motion_initialized");
    // Creature.cpp:883 -> CreatureAISelector.cpp:42 -> ScriptDevAIMgr.cpp:365.
    let script = prefix.scripts.get_script(template.words[96])?;
    prefix.lookups.push("creature_ai_script_registry_lookup");
    if let Some(script) = script {
        let factory = script
            .slots
            .get("GetAI")
            .ok_or_else(|| refuse("execution.creature.ScriptDevAIMgr_367.GetAI_slot_owner"))?;
        if *factory {
            return Err(refuse("execution.creature.ScriptDevAIMgr_370.GetAI_owner"));
        }
    }
    prefix.lookups.push("creature_ai_script_null");
    // World.cpp:1381 -> CreatureAIRegistry.cpp:35-48. Reuse F5.01's identity and
    // std::map traversal, not a second list of factory names or scores.
    prefix.lookups.push("creature_ai_registry_bound");
    // Creature.h:621-623 and Unit.cpp:330,429: this DB-loaded Creature is not a Pet,
    // temporary summon or Totem and has no charmer or owner.
    let neutral = prefix
        .ai_factions
        .neutral_to_all(prefix.units.get(context.objects, object)?.faction);
    let selected = prefix.ai_registry.select_after_script(
        context.objects,
        object,
        CreatureAiInputs {
            effective_entry: template.words[0],
            ai_name: String::from_utf8(template.strings[2].clone())
                .map_err(|_| refuse("execution.creature.GetAIName_encoding_owner"))?,
            script_name: String::from_utf8(
                prefix
                    .scripts
                    .names()
                    .get(template.words[96] as usize)
                    .ok_or_else(|| refuse("execution.creature.ScriptName_index_owner"))?
                    .clone(),
            )
            .map_err(|_| refuse("execution.creature.ScriptName_encoding_owner"))?,
            control: CreatureAiControl::default(),
            guard: template.words[23] & 0x400 != 0,
            no_aggro_on_sight: template.words[23] & 2 != 0,
            neutral_to_all: neutral,
        },
        prefix.units.get(context.objects, object)?.control_guids,
    )?;
    prefix.lookups.push("creature_ai_selected");
    eprintln!(
        "F6_CREATURE_AI map={} entry={} input={:?} branch={:?} executable={:?}",
        prefix.map_entry.id,
        template.words[0],
        selected.inputs(),
        selected.branch(),
        selected.executable()
    );
    prefix.selected_ai = Some(selected);
    ai_initialize::initialize(prefix, object, context)?;
    if prefix.spawn_templates.rows()[&data.spawn_template.unwrap()].relay_id != 0 {
        return Err(refuse(
            "execution.creature.Creature_1743.ScriptsStart_owner",
        ));
    }
    prefix.lookups.push("creature_spawn_relay_checked");
    if prefix.creature_linking_trigger && context.objects.get(object)?.alive {
        motion_initialize::linking_respawn(prefix, object, context)?;
    }
    if prefix.groups.creature_group(map, data.row.guid)?.is_some() {
        return Err(refuse(
            "execution.creature.Creature_1751.GetFormationData_owner",
        ));
    }
    prefix.lookups.push("creature_respawn_group_checked");
    let rabbit_day =
        super::super::super::super::spawn_loader_config::assignment(prefix.config, "RabbitDay")?
            .1
            .parse::<u32>()
            .map_err(|_| refuse("execution.creature.rabbit_day_config"))?;
    if context.objects.get(object)?.alive && rabbit_day != 0 {
        return Err(refuse("execution.creature.Creature_1761.localtime_owner"));
    }
    prefix.lookups.push("creature_load_from_db_return");
    Ok(GridCandidateOutcome::Loaded(object))
}

#[path = "seal16_addon_cast.rs"]
mod addon_cast;
#[path = "seal16_creature_ai.rs"]
mod ai_initialize;
#[path = "seal16_creature_motion.rs"]
mod motion_initialize;

fn notify_delay(value: i32) -> u32 {
    // World.cpp:704,2395-2402,2430-2437 resets negative uint32 config to its default.
    if value < 0 {
        5000
    } else {
        value as u32
    }
}

#[test]
fn notify_delay_uses_source_default_only_for_negative_values() {
    assert_eq!(notify_delay(-1), 5000);
    assert_eq!(notify_delay(0), 0);
    assert_eq!(notify_delay(5000), 5000);
    assert_eq!(notify_delay(i32::MAX), 2147483647);
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
        let data = self.loaded_creature_spawn(state.spawn.database_guid)?;
        let spawn = data
            .spawn_template
            .and_then(|entry| self.spawn_templates.rows().get(&entry))
            .ok_or_else(|| refuse("execution.creature.spawn_template_undefined"))?;
        Ok(LoadedCreatureEntry {
            original: self.loaded_creature_template(state.original_entry)?,
            effective: self.loaded_creature_template(state.template.entry)?,
            spawn,
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

    fn creature_current_group(&mut self, _: ObjectHandle) -> Result<Option<CreatureGroup>> {
        Err(refuse(
            "execution.creature.Creature_1751.GetFormationData_owner",
        ))
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
        self.lookups.push(match observation {
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
        });
    }

    fn creature_guids(&mut self) -> &mut MapGuidAllocators {
        self.guids
    }

    fn creature_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        Ok(self.random)
    }

    fn creature_respawn_time(&mut self, _: u32) -> Result<i64> {
        Err(refuse(
            "execution.creature.Creature_1696.persistent_respawn_owner",
        ))
    }

    fn creature_save_respawn_time(&mut self, _: u32, _: i64) -> Result<()> {
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

    fn creature_spawned_by_linked_mob(&mut self, _: &CreatureConstruction) -> Result<bool> {
        Err(refuse(
            "execution.creature.Creature_1720.linking_lookup_owner",
        ))
    }

    fn creature_linked_can_spawn(&mut self, _: &CreatureConstruction) -> Result<bool> {
        Err(refuse("execution.creature.Creature_1723.CanSpawn_owner"))
    }

    fn creature_dead_flying_height(&mut self, _: &CreatureConstruction) -> Result<f32> {
        Err(refuse(
            "execution.creature.Creature_1731.GetHeightStatic_owner",
        ))
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
            _ => Err(phase.missing_owner()),
        }
    }

    fn creature_created(
        &mut self,
        _: CreatureConstruction,
        _: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        self.lookups.push("creature_load_from_db_return");
        Ok(())
    }

    fn creature_destroy_candidate(
        &mut self,
        object: ObjectHandle,
        _: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        self.units.discard_construction(object);
        Ok(())
    }
}
