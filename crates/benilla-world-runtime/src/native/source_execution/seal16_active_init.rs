// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded active traversal stops at the first unowned source call.
use super::*;
use benilla_sim::corpse_appearance::EquippedItem;
use benilla_sim::resident_map::CellPair;

pub(super) const CREATURE_NEXT_OWNER: [&str; 2] = [
    "execution.ai.ScriptDevAIMgr_370.GetAI_owner.npc_garments_of_quests",
    "execution.continents.load_transports_owner",
];
#[path = "seal16_active_clocks.rs"]
mod clocks;
pub(super) use clocks::{ActiveClock, ActiveClocks};
#[path = "seal16_creature_create.rs"]
mod creature_create;
#[path = "seal16_creature_entry.rs"]
mod creature_entry;
#[path = "seal16_gameobject_init.rs"]
mod gameobject_init;

struct Definitions<'a>(&'a SourceEffectiveSpawns);
impl GridDefinitions for Definitions<'_> {
    type Row = SpawnKey;
    fn read(&mut self, attempt: GridLoadAttempt) -> Result<SpawnKey> {
        let key = SpawnKey {
            kind: attempt.kind,
            guid: attempt.guid,
        };
        let data = self
            .0
            .complete_data(key)?
            .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?;
        require(
            data.row.ordinal == attempt.catalog_ordinal,
            "execution.grid.effective_spawn_ordinal",
        )?;
        Ok(key)
    }
}
impl PersistentGridLookup for Definitions<'_> {
    fn persistent_ordinal(&mut self, kind: GridSpawnKind, guid: u32) -> Result<u64> {
        Ok(self
            .0
            .complete_data(SpawnKey { kind, guid })?
            .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?
            .row
            .ordinal)
    }
}
pub(super) struct Prefix<'a> {
    pub spawns: &'a SourceEffectiveSpawns,
    pub events: &'a SourceGameEventManager,
    pub groups: &'a SourceSpawnGroups,
    pub waypoint_paths: std::rc::Rc<SourceWaypointPaths>,
    pub creature_templates: &'a LoadedCreatureTemplates,
    pub creature_addons: &'a LoadedCreatureAddons,
    pub spell_stores: &'a SourceSpellStores,
    pub spell_targets: SourceSpellTargetMgr,
    pub clocks: &'a mut ActiveClocks,
    pub world_clock: &'a SourceWorldClock,
    pub scripts: &'a SourceScriptRegistry,
    pub gameobject_templates: &'a LoadedGameObjectTemplates,
    pub spawn_templates: &'a LoadedCreatureSpawnTemplates,
    pub creature_models: &'a LoadedCreatureModels,
    pub equipment: &'a LoadedEquipmentTemplates,
    pub collision_models: &'a SourceCollisionModels,
    pub swimming_census: SourceCreatureSwimmingCensus,
    pub creature_spells: Option<&'a CreatureSpellMapData>,
    pub creature_immunities: &'a LoadedCreatureImmunities,
    pub outdoor_pvp: &'a mut SourceOutdoorPvp,
    pub linking: &'a SourceCreatureLinking,
    pub link_holder: &'a mut SourceCreatureLinkingHolder,
    pub pools: &'a SourcePoolManager,
    pub link_follow: Option<(FollowerReferenceId, ObjectHandle, [u32; 2])>,
    pub map_visibility_bits: u32,
    pub relocation_config: Option<SourceUnitRelocationConfig>,
    pub visibility: SourceVisibilityCensus,
    pub creature_battleground: Option<bool>,
    pub creature_zone: Option<u32>,
    pub zone_lookups: BTreeMap<SpawnKey, [u32; 2]>,
    pub creature_linking_trigger: bool,
    pub instance_data: Option<&'a mut SourceWorldScript>,
    pub random: &'a mut dyn CreatureRandom,
    pub spline_thread: &'a mut SourceSplineThread,
    pub dynamic_models: &'a mut SourceDynamicModels,
    pub dynamic_map: SourceDynamicMap,
    pub trig_dispatch: benilla_sim::source_trig_fma::DispatchBinding,
    pub gameobjects: SourceGameObjects,
    pub creature_model: Option<[u32; 8]>,
    pub creature_entry: Option<creature_entry::EntryFields>,
    pub equipment_writes: BTreeMap<u32, SourceCreatureEquipment>,
    pub level_definitions: &'a CreatureLevelDefinitions,
    pub level_config: &'a CreatureLevelConfig,
    pub encounters: &'a LoadedInstanceEncounters,
    pub faction_flags: &'a BTreeMap<u32, u32>,
    pub ai_registry: &'a SourceAiRegistry,
    pub ai_factions: &'a CreatureAiFactions,
    pub selected_ai: Option<SelectedCreatureAi>,
    pub ai_templates: &'a BTreeMap<u32, CreatureAiTemplateOperands>,
    pub ai_config: &'a EventAiLoaderConfig,
    pub event_definitions: std::sync::Arc<EventAiDefinitions>,
    pub units: SourceUnits,
    pub passengers: &'a mut SourcePassengers,
    pub ais: SourceCreatureAis,
    pub threats: SourceThreats,
    pub combat: SourceCombat,
    pub event_ais: BTreeMap<ObjectHandle, SourceEventAi>,
    pub config: &'a [u8],
    pub inputs: &'a EffectiveSpawnInputs,
    pub go_animprogress: Option<u32>,
    pub creature_display_chance: Option<u32>,
    pub guids: &'a mut MapGuidAllocators,
    pub init_entry_fields: Option<(u32, u32, [u8; 2])>,
    pub go_load_operands: Option<(u32, [u32; 4], [u32; 4])>,
    pub go_factory_elevator: Option<bool>,
    pub thread: u64,
    pub attempts: Vec<GridLoadAttempt>,
    pub candidates: Vec<ObjectHandle>,
    pub completed: usize,
    pub completed_objects: Vec<ObjectHandle>,
    pub first_creature: Option<FirstCreature>,
    pub first_go: Option<FirstGo>,
    pub motions: BTreeMap<ObjectHandle, SourceMotionMaster>,
    pub entry: Option<u32>,
    pub lookups: Vec<&'static str>,
    pub corpses: &'a mut SourceCorpseRegistry,
    pub corpse_transfers: FrozenCorpseTransfers,
    pub instanceable: bool,
    pub map_entry: PersistentMapEntry,
    pub go_battleground: Option<bool>,
    pub go_zone: Option<u32>,
    pub go_script_id: Option<u32>,
}
impl<D: GridDefinitions<Row = SpawnKey>> GridObjectFactory<D, SourceTerrainManager> for Prefix<'_> {
    fn load_from_db(
        &mut self,
        attempt: GridLoadAttempt,
        key: SpawnKey,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<GridCandidateOutcome> {
        self.attempts.push(attempt);
        if key.kind == GridSpawnKind::GameObject {
            let outcome = load_source_gameobject(self, attempt, key, context)?;
            if let GridCandidateOutcome::Loaded(candidate) = outcome {
                if self.first_go.is_none() {
                    self.first_go = Some(FirstGo {
                        attempt,
                        candidate,
                        entry: self.entry.unwrap(),
                        lookups: self.lookups.clone(),
                        load_operands: self.go_load_operands,
                        factory_elevator: self.go_factory_elevator,
                        animprogress: self.go_animprogress,
                        battleground: self.go_battleground,
                        zone: self.go_zone,
                        script_id: self.go_script_id,
                        visibility: self.visibility.clone(),
                    });
                }
            }
            return Ok(outcome);
        }
        // Creature.cpp:898-930 and :880-895 diagnostics belong to this candidate.
        self.creature_battleground = None;
        self.creature_zone = None;
        self.creature_linking_trigger = false;
        self.selected_ai = None;
        context.load_creature(self, attempt, key)
    }
    fn viewpoint_added(
        &mut self,
        object: ObjectHandle,
        cell: CellIdentity,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        self.lookups.push("grid_viewpoint_added");
        context.objects.viewpoint_added_to_world(
            object,
            cell,
            context.cells,
            |objects, cells, camera| {
                <Self as GridObjectFactory<D, SourceTerrainManager>>::camera_added_to_world(
                    self, objects, cells, camera,
                )
            },
        )
    }
    fn battleground_db_load(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, SourceTerrainManager>,
    ) -> Result<()> {
        dispatch_grid_battleground_db_load(self.map_entry, || {
            Err(refuse("execution.grid.loaded_battleground_owner"))
        })?;
        self.lookups.push("grid_battleground_null");
        require(
            context.cells.registered_cell(object).is_some(),
            "execution.grid_candidate_not_registered",
        )?;
        self.completed_objects.push(object);
        self.completed += 1;
        if self.first_creature.is_none()
            && context.objects.get(object)?.kind == SourceObjectKind::Creature
        {
            self.first_creature = Some(FirstCreature {
                attempt: *self.attempts.last().unwrap(),
                candidate: object,
                entry: self.entry.unwrap(),
                lookups: self.lookups.clone(),
                fields: self.creature_entry.take().unwrap(),
                model: self.creature_model,
                display_chance: self.creature_display_chance,
                init_entry_fields: self.init_entry_fields,
                selected_ai: self.selected_ai.take(),
                visibility: self.visibility.clone(),
                battleground: self.creature_battleground,
                zone: self.creature_zone,
                linking_trigger: self.creature_linking_trigger,
                go_load_operands: self.go_load_operands,
                go_factory_elevator: self.go_factory_elevator,
                go_animprogress: self.go_animprogress,
                go_battleground: self.go_battleground,
                go_zone: self.go_zone,
                go_script_id: self.go_script_id,
            });
        }
        Ok(())
    }
    fn corpse_bindings(&mut self) -> Result<CorpseGridBindings<'_>> {
        Ok(CorpseGridBindings {
            registry: self.corpses,
            other_maps: &mut self.corpse_transfers,
            instanceable: self.instanceable,
        })
    }
}
pub(super) struct FirstCreature {
    pub attempt: GridLoadAttempt,
    pub candidate: ObjectHandle,
    pub entry: u32,
    pub lookups: Vec<&'static str>,
    pub fields: creature_entry::EntryFields,
    pub model: Option<[u32; 8]>,
    pub display_chance: Option<u32>,
    pub init_entry_fields: Option<(u32, u32, [u8; 2])>,
    pub selected_ai: Option<SelectedCreatureAi>,
    pub visibility: SourceVisibilityCensus,
    pub battleground: Option<bool>,
    pub zone: Option<u32>,
    pub linking_trigger: bool,
    pub go_load_operands: Option<(u32, [u32; 4], [u32; 4])>,
    pub go_factory_elevator: Option<bool>,
    pub go_animprogress: Option<u32>,
    pub go_battleground: Option<bool>,
    pub go_zone: Option<u32>,
    pub go_script_id: Option<u32>,
}

pub(super) struct FirstGo {
    pub attempt: GridLoadAttempt,
    pub candidate: ObjectHandle,
    pub entry: u32,
    pub lookups: Vec<&'static str>,
    pub load_operands: Option<(u32, [u32; 4], [u32; 4])>,
    pub factory_elevator: Option<bool>,
    pub animprogress: Option<u32>,
    pub battleground: Option<bool>,
    pub zone: Option<u32>,
    pub script_id: Option<u32>,
    pub visibility: SourceVisibilityCensus,
}

pub(super) struct ActivePrefixReport {
    pub first_creature: Option<FirstCreature>,
    pub refusal: Refusal,
    pub link_follow: Option<(FollowerReferenceId, ObjectHandle, [u32; 2])>,
    pub first_go: Option<FirstGo>,
    pub completed_objects: Vec<ObjectHandle>,
    pub attempt: GridLoadAttempt,
    pub candidates: usize,
    pub candidate: ObjectHandle,
    pub entry: u32,
    pub lookups: Vec<&'static str>,
    pub init_entry_fields: Option<(u32, u32, [u8; 2])>,
    pub bound_guid: Option<SourceGuid>,
    pub go_load_operands: Option<(u32, [u32; 4], [u32; 4])>,
    pub go_factory_elevator: Option<bool>,
    pub go_animprogress: Option<u32>,
    pub creature_display_chance: Option<u32>,
    pub creature_model: Option<[u32; 8]>,
    pub gameobjects: SourceGameObjects,
    pub creature_entry: Option<creature_entry::EntryFields>,
    pub equipment_writes: BTreeMap<u32, SourceCreatureEquipment>,
    pub swimming_census: SourceCreatureSwimmingCensus,
    pub completed: usize,
    pub go_battleground: Option<bool>,
    pub go_zone: Option<u32>,
    pub go_script_id: Option<u32>,
    pub visibility: SourceVisibilityCensus,
    pub creature_battleground: Option<bool>,
    pub creature_zone: Option<u32>,
    pub zone_lookups: BTreeMap<SpawnKey, [u32; 2]>,
    pub creature_linking_trigger: bool,
    pub selected_ai: Option<SelectedCreatureAi>,
    pub units: SourceUnits,
    pub motions: BTreeMap<ObjectHandle, SourceMotionMaster>,
    pub ais: SourceCreatureAis,
    pub event_ais: BTreeMap<ObjectHandle, SourceEventAi>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load_first(
    map: &mut SourceContinent,
    spawns: &SourceEffectiveSpawns,
    inputs: &EffectiveSpawnInputs,
    events: &SourceGameEventManager,
    groups: &SourceSpawnGroups,
    waypoint_paths: std::rc::Rc<SourceWaypointPaths>,
    creature_templates: &LoadedCreatureTemplates,
    creature_addons: &LoadedCreatureAddons,
    spell_stores: &SourceSpellStores,
    clocks: &mut ActiveClocks,
    world_clock: &SourceWorldClock,
    gameobject_templates: &LoadedGameObjectTemplates,
    scripts: &SourceScriptRegistry,
    spawn_templates: &LoadedCreatureSpawnTemplates,
    creature_models: &LoadedCreatureModels,
    equipment: &LoadedEquipmentTemplates,
    creature_immunities: &LoadedCreatureImmunities,
    collision_models: &SourceCollisionModels,
    random: &mut dyn CreatureRandom,
    spline_thread: &mut SourceSplineThread,
    dynamic_models: &mut SourceDynamicModels,
    trig_dispatch: benilla_sim::source_trig_fma::DispatchBinding,
    level_definitions: &CreatureLevelDefinitions,
    level_config: &CreatureLevelConfig,
    encounters: &LoadedInstanceEncounters,
    faction_flags: &BTreeMap<u32, u32>,
    ai_registry: &SourceAiRegistry,
    ai_factions: &CreatureAiFactions,
    ai_templates: &BTreeMap<u32, CreatureAiTemplateOperands>,
    ai_config: &EventAiLoaderConfig,
    event_definitions: std::sync::Arc<EventAiDefinitions>,
    outdoor_pvp: &mut SourceOutdoorPvp,
    linking: &SourceCreatureLinking,
    pools: &SourcePoolManager,
    states: &mut SourceManagers,
    products: &mut SourceTerrainManager,
    objects: &mut SourceObjects,
    corpses: &mut SourceCorpseRegistry,
    thread: u64,
    config: &[u8],
) -> Result<ActivePrefixReport> {
    require(
        random.creature_stream()? == thread,
        "execution.creature.loaded_rng_thread_mismatch",
    )?;
    // World.cpp:485-494, World.h:524. The frozen configuration has no forced maps.
    let forced =
        super::super::super::spawn_loader_config::assignment(config, "LoadAllGridsOnMaps")?.1;
    require(
        forced.trim_matches('"').is_empty(),
        "execution.active.forced_creature_data_traversal_owner",
    )?;
    // Map.cpp:340 and Config.cpp:117-124 provide the first NGrid's actual inputs.
    let unload = super::super::super::spawn_loader_config::assignment(config, "GridUnload")?.1;
    map.cells.configure_grids(
        map.grid_expiry,
        unload == "1" || unload.eq_ignore_ascii_case("true") || unload.eq_ignore_ascii_case("yes"),
    )?;
    let mut sets = GridSpawnSets::new(map.identity);
    for (&(map_id, x, y, kind), guids) in &spawns.static_grids {
        if map_id != map.identity.map {
            continue;
        }
        let cell =
            CellPair::from_indices(x, y).map_err(|_| refuse("execution.cell_coordinates"))?;
        for &guid in guids {
            let data = spawns
                .complete_data(SpawnKey { kind, guid })?
                .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?;
            sets.register(cell, kind, GridSpawnOrigin::Static, guid, data.row.ordinal)?;
        }
    }
    let mut definitions = PersistentGridDefinitions {
        definitions: Definitions(spawns),
        managers: states,
        state: map
            .persistent
            .ok_or_else(|| refuse("execution.continents.persistent_state_owner"))?,
    };
    let relocation_config = if map.identity.map == 1 {
        let approved =
            Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")?;
        let parsed = SourceUnitRelocationConfig::from_recorded_bytes(
            config,
            &approved,
            map.visibility.to_bits(),
        )?;
        // mangosd.conf:1330-1331, seal16 :12111 and :12139, World.cpp:743-744.
        assert_eq!(parsed.ai_notify_delay_ms, 1000);
        assert_eq!(parsed.lower_limit_squared.to_bits(), 0x42c8_0000);
        Some(parsed)
    } else {
        None
    };
    let mut factory = Prefix {
        spawns,
        events,
        groups,
        waypoint_paths,
        creature_templates,
        creature_addons,
        spell_stores,
        spell_targets: SourceSpellTargetMgr::initialize(spell_stores),
        clocks,
        world_clock,
        gameobject_templates,
        scripts,
        spawn_templates,
        creature_models,
        equipment,
        collision_models,
        swimming_census: SourceCreatureSwimmingCensus::default(),
        creature_spells: map.creature_spells.as_ref(),
        creature_immunities,
        outdoor_pvp,
        linking,
        link_holder: &mut map.linking,
        pools,
        link_follow: None,
        map_visibility_bits: map.visibility.to_bits(),
        relocation_config,
        visibility: SourceVisibilityCensus::default(),
        creature_battleground: None,
        creature_zone: None,
        creature_linking_trigger: false, // Unit.cpp:419.
        instance_data: map.instance_data.as_mut(),
        random,
        spline_thread,
        dynamic_models,
        dynamic_map: SourceDynamicMap::new(map.identity),
        trig_dispatch,
        gameobjects: SourceGameObjects::default(),
        creature_model: None,
        creature_entry: None,
        equipment_writes: BTreeMap::new(),
        level_definitions,
        level_config,
        encounters,
        faction_flags,
        ai_registry,
        ai_factions,
        ai_templates,
        ai_config,
        event_definitions,
        units: SourceUnits::default(),
        passengers: &mut map.passengers,
        ais: SourceCreatureAis::default(),
        threats: SourceThreats::default(),
        combat: SourceCombat::default(),
        event_ais: BTreeMap::new(),
        config,
        inputs,
        go_animprogress: None,
        creature_display_chance: None,
        guids: map
            .guids
            .as_mut()
            .ok_or_else(|| refuse("execution.continents.guid_owner"))?,
        init_entry_fields: None,
        go_load_operands: None,
        go_factory_elevator: None,
        thread,
        attempts: Vec::new(),
        candidates: Vec::new(),
        completed: 0,
        first_creature: None,
        completed_objects: Vec::new(),
        first_go: None,
        zone_lookups: BTreeMap::new(),
        selected_ai: None,
        motions: BTreeMap::new(),
        entry: None,
        lookups: Vec::new(),
        corpses,
        corpse_transfers: FrozenCorpseTransfers,
        instanceable: map.entry.instanceable(),
        map_entry: map.entry,
        go_battleground: None,
        go_zone: None,
        go_script_id: None,
    };
    let autoload =
        super::super::super::spawn_loader_config::assignment(config, "Autoload.Active")?.1 == "1";
    let mut active = ActivePreloadCatalog::default();
    for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
        for &guid in spawns
            .active
            .get(&(map.identity.map, kind))
            .into_iter()
            .flatten()
        {
            let data = spawns
                .complete_data(SpawnKey { kind, guid })?
                .ok_or_else(|| refuse("execution.active.loaded_spawn_absent"))?;
            let templates = if kind == GridSpawnKind::Creature {
                &inputs.creatures
            } else {
                &inputs.gameobjects
            };
            let flags = templates
                .get(&data.validation_entry)
                .ok_or_else(|| refuse("execution.active.template_flags_owner"))?
                .flags;
            active.register_loaded_spawn(
                ActivePreloadSpawn {
                    kind,
                    map: data.row.map,
                    database_guid: guid,
                    pose: SourcePose(data.row.pose.map(f32::to_bits)),
                    game_event: i32::from(data.row.event),
                    guid_pool: data.row.pools[0] as u32,
                    entry_pool: data.row.pools[1] as u32,
                    dynamic_guid: kind == GridSpawnKind::GameObject && flags & 2 != 0,
                    template_extra_flags: Some(flags),
                },
                autoload,
            )?;
        }
    }
    let expected = [GridSpawnKind::Creature, GridSpawnKind::GameObject]
        .into_iter()
        .flat_map(|kind| {
            spawns
                .active
                .get(&(map.identity.map, kind))
                .into_iter()
                .flatten()
                .map(move |&guid| (kind, guid))
        })
        .collect::<Vec<_>>();
    require(
        active
            .normal_order(map.identity.map)
            .map(|spawn| (spawn.kind, spawn.database_guid))
            .collect::<Vec<_>>()
            == expected,
        "execution.active.loaded_membership_binding",
    )?;
    let mut context = GridFactoryContext {
        definitions: &mut definitions,
        products,
        sets: &mut sets,
        objects,
        cells: &mut map.cells,
    };
    let refusal = match active.load(false, &mut context, &mut factory) {
        Err(refusal) => refusal,
        // Map.cpp:214-216 calls LoadTransports after active preload returns.
        Ok(()) => refuse("execution.continents.load_transports_owner"),
    };
    let attempt = *factory.attempts.last().ok_or_else(|| refusal.clone())?;
    let bound_guid = factory
        .candidates
        .last()
        .map(|handle| context.objects.get(*handle).map(|object| object.guid))
        .transpose()?
        .flatten();
    Ok(ActivePrefixReport {
        refusal,
        link_follow: factory.link_follow,
        first_go: factory.first_go,
        completed_objects: factory.completed_objects,
        first_creature: factory.first_creature,
        attempt,
        candidates: factory.candidates.len(),
        candidate: *factory
            .candidates
            .last()
            .ok_or_else(|| refuse("execution.active.candidate_missing"))?,
        entry: factory
            .entry
            .ok_or_else(|| refuse("execution.active.template_entry_missing"))?,
        lookups: factory.lookups,
        init_entry_fields: factory.init_entry_fields,
        bound_guid,
        go_load_operands: factory.go_load_operands,
        go_factory_elevator: factory.go_factory_elevator,
        go_animprogress: factory.go_animprogress,
        creature_display_chance: factory.creature_display_chance,
        creature_model: factory.creature_model,
        gameobjects: factory.gameobjects,
        creature_entry: factory.creature_entry,
        equipment_writes: factory.equipment_writes,
        swimming_census: factory.swimming_census,
        completed: factory.completed,
        go_battleground: factory.go_battleground,
        go_zone: factory.go_zone,
        go_script_id: factory.go_script_id,
        visibility: factory.visibility,
        creature_battleground: factory.creature_battleground,
        creature_zone: factory.creature_zone,
        zone_lookups: factory.zone_lookups,
        creature_linking_trigger: factory.creature_linking_trigger,
        selected_ai: factory.selected_ai,
        units: factory.units,
        motions: factory.motions,
        ais: factory.ais,
        event_ais: factory.event_ais,
    })
}

pub(super) struct FrozenCorpseTransfers;
impl CorpseGridOwner for FrozenCorpseTransfers {}
struct FrozenCorpseQuery {
    status: CharacterQueryStatus,
}
impl CorpseWork for FrozenCorpseQuery {
    fn constructor_clock(&mut self, _: ObjectHandle, _: &mut SourceObjects) -> Result<ClockSample> {
        Err(refuse("execution.corpse.initial_constructor_clock_owner"))
    }
    fn observe(&mut self, _: CorpseEvent) -> Result<()> {
        Err(refuse(
            "execution.corpse.initial_constructor_observation_owner",
        ))
    }
    fn query_corpses(&mut self) -> Result<CharacterQueryStatus> {
        Ok(self.status)
    }
}
struct FrozenCorpseAppearance;
impl CorpseAppearanceOwner for FrozenCorpseAppearance {
    fn player_displays(&mut self, _: u8, _: u8) -> Result<Option<[u32; 2]>> {
        Err(refuse("execution.corpse.initial_player_appearance_owner"))
    }
    fn item(&mut self, _: u32) -> Result<Option<EquippedItem>> {
        Err(refuse("execution.corpse.initial_item_appearance_owner"))
    }
}
pub(super) fn frozen_corpses(recording: &Path) -> Result<SourceCorpseRegistry> {
    // World.cpp:1196 loads ObjectAccessor before either continent's LoadN.
    let root = recording
        .ancestors()
        .find(|path| path.ends_with("benilla-config/full-server/f1"))
        .ok_or_else(|| refuse("execution.startup.recorded_f1_path_component_missing"))?;
    let inputs = CorpseInitialInputs::inspect_snapshot(
        &root.join("pre-start-characters.sql"),
        &Sha256Id::parse("cf538fd2b7bbd96fd7d653d8d2b05f19da49d49989e42ac412aee382bee5a95d")
            .unwrap(),
    )?;
    let status = if inputs.rows().next().is_some() {
        CharacterQueryStatus::Rows
    } else {
        CharacterQueryStatus::Empty
    };
    let mut registry = SourceCorpseRegistry::default();
    let count = registry.load_initial(
        &inputs,
        1,
        &mut SourceObjects::default(),
        &mut FrozenCorpseAppearance,
        &mut FrozenCorpseQuery { status },
    )?;
    require(count == 0, "execution.corpse.frozen_initial_count")?;
    Ok(registry)
}
