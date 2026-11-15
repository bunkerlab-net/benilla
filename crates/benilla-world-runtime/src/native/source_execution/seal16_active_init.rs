// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded active traversal stops at the first unowned spawn field.
use super::*;
use benilla_sim::corpse_appearance::EquippedItem;
use benilla_sim::resident_map::CellPair;

pub(super) const CREATURE_SPAWN_MODEL_OWNER: &str =
    "execution.object_mgr.loaded_creature_spawn_data_template_owner";
pub(super) const GO_ANIMPROGRESS_OWNER: &str =
    "execution.effective_spawns.gameobject_animprogress_owner";

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
    pub creature_templates: &'a LoadedCreatureTemplates,
    pub gameobject_templates: &'a LoadedGameObjectTemplates,
    pub guids: &'a mut MapGuidAllocators,
    pub init_entry_fields: Option<(u32, u32, [u8; 2])>,
    pub go_load_operands: Option<(u32, [u32; 4], [u32; 4])>,
    pub go_factory_elevator: Option<bool>,
    pub thread: u64,
    pub attempts: Vec<GridLoadAttempt>,
    pub candidates: Vec<ObjectHandle>,
    pub motions: BTreeMap<ObjectHandle, SourceMotionMaster>,
    pub entry: Option<u32>,
    pub lookups: Vec<&'static str>,
    pub corpses: &'a mut SourceCorpseRegistry,
    pub corpse_transfers: FrozenCorpseTransfers,
    pub instanceable: bool,
}
impl<D: GridDefinitions<Row = SpawnKey>, P: GridProductOwner> GridObjectFactory<D, P>
    for Prefix<'_>
{
    fn load_from_db(
        &mut self,
        attempt: GridLoadAttempt,
        key: SpawnKey,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<GridCandidateOutcome> {
        self.attempts.push(attempt);
        if key.kind == GridSpawnKind::GameObject {
            // ObjectGridLoader.cpp:123-129, GameObject.cpp:107-112 reads the template before allocation.
            let data = self
                .spawns
                .complete_data(key)?
                .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?;
            self.lookups.push("factory_go_data");
            self.entry = Some(data.row.entry);
            self.lookups.push("factory_go_template");
            let template = self.gameobject_templates.get(data.row.entry);
            self.go_factory_elevator = Some(template.is_some_and(|row| row.words[1] == 11));
            let object = context
                .objects
                .construct(self.thread, SourceObjectKind::GameObject)?;
            self.candidates.push(object);
            self.lookups.push("go_construct");
            let data = self
                .spawns
                .complete_data(key)?
                .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?;
            self.lookups.push("go_data");
            if !context.objects.candidate_allowed(
                attempt.grid.map,
                DatabaseLookupKind::GameObject,
                key.guid,
            ) {
                context.objects.destroy(object, context.cells)?;
                return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
            }
            self.lookups.push("existing_gameobject");
            self.go_load_operands = Some((
                data.row.entry,
                data.row.pose.map(f32::to_bits),
                data.row.rotation.map(f32::to_bits),
            ));
            self.lookups.push("go_position_rotation");
            // ObjectMgrSpawnRow does not retain GameObjectData::animprogress, GameObject.cpp:896.
            return Err(refuse(GO_ANIMPROGRESS_OWNER));
        }
        // Creature.cpp:136 and :1603-1658. No selected model, stats or AI is supplied before this lookup.
        let object = context
            .objects
            .construct(self.thread, SourceObjectKind::Creature)?;
        self.candidates.push(object);
        self.motions.insert(
            object,
            SourceMotionMaster::new(
                object,
                MotionDefault {
                    movement_type: 0,
                    default_path: 0,
                },
            ),
        );
        let data = self
            .spawns
            .complete_data(key)?
            .ok_or_else(|| refuse("execution.grid.effective_spawn_absent"))?;
        self.lookups.push("creature_data");
        if !context.objects.candidate_allowed(
            attempt.grid.map,
            DatabaseLookupKind::Creature,
            key.guid,
        ) {
            context.objects.destroy(object, context.cells)?;
            return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
        }
        self.lookups.push("existing_creature");
        let group = self.groups.creature_group(attempt.grid.map, key.guid)?;
        self.lookups.push("spawn_group");
        let entry = data.row.entry;
        if entry == 0 {
            // Group/conditional/random selection needs its live source RNG owner, never a recorded answer.
            return Err(refuse(if group.is_some() {
                "execution.creature.loaded_group_entry_owner"
            } else {
                "execution.creature.loaded_conditional_entry_owner"
            }));
        }
        self.lookups.push("conditional_skipped_nonzero_entry");
        self.lookups.push("random_skipped_nonzero_entry");
        let event = self.events.creature_event_data(key.guid)?;
        self.lookups.push("active_event");
        self.entry = Some(entry);
        self.lookups.push("creature_template");
        let Some(original) = self.creature_templates.get(entry) else {
            context.objects.destroy(object, context.cells)?;
            return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
        };
        // Map.cpp:2966-2975 forces only maps 533 and 543.
        let allocation = self.guids.spawn_counter(SpawnGuidRequest {
            high: HighGuid::Creature,
            database: key.guid,
            requested: key.guid,
            force_dynamic: matches!(attempt.grid.map.map, 533 | 543),
            extra_dynamic: original.words[23] & 0x10_0000 != 0,
            group: group.is_some(),
        })?;
        require(!allocation.stop_now, "execution.creature.guid_stop_now")?;
        self.lookups.push("creature_guid");
        context
            .objects
            .bind_map(object, attempt.grid.map, context.cells)?;
        context.objects.set_respawn_definition(
            object,
            SourcePose(data.row.pose.map(f32::to_bits)),
            true,
        )?;
        context.objects.assign_guid(
            object,
            key.guid,
            SourceGuid::with_entry(HighGuid::Creature, entry, allocation.counter),
        )?;
        self.lookups.push("creature_create_from_proto");
        let effective = event
            .filter(|event| event.entry != 0)
            .map_or(entry, |event| event.entry);
        let Some(template) = self.creature_templates.get(effective) else {
            context.objects.destroy(object, context.cells)?;
            return Ok(GridCandidateOutcome::FailedAndDestroyed(object));
        };
        self.lookups.push("init_entry_template");
        // Creature.cpp:335-344 writes only entry, scale, race and class before ChooseDisplayId.
        self.init_entry_fields =
            Some((effective, template.words[14], [0, template.words[36] as u8]));
        self.lookups.push("init_entry_fields");
        if event.is_some_and(|event| event.model != 0) {
            return Err(refuse(
                "execution.creature.loaded_model_random_gender_owner",
            ));
        }
        Err(refuse(CREATURE_SPAWN_MODEL_OWNER))
    }
    fn viewpoint_added(
        &mut self,
        _: ObjectHandle,
        _: CellIdentity,
        _: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(refuse("execution.grid.loaded_viewpoint_owner"))
    }
    fn battleground_db_load(
        &mut self,
        _: ObjectHandle,
        _: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<()> {
        Err(refuse("execution.grid.loaded_battleground_owner"))
    }
    fn corpse_bindings(&mut self) -> Result<CorpseGridBindings<'_>> {
        Ok(CorpseGridBindings {
            registry: self.corpses,
            other_maps: &mut self.corpse_transfers,
            instanceable: self.instanceable,
        })
    }
}

pub(super) struct ActivePrefixReport {
    pub refusal: Refusal,
    pub attempt: GridLoadAttempt,
    pub candidates: usize,
    pub candidate: ObjectHandle,
    pub entry: u32,
    pub lookups: Vec<&'static str>,
    pub init_entry_fields: Option<(u32, u32, [u8; 2])>,
    pub bound_guid: Option<SourceGuid>,
    pub go_load_operands: Option<(u32, [u32; 4], [u32; 4])>,
    pub go_factory_elevator: Option<bool>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load_first(
    map: &mut SourceContinent,
    spawns: &SourceEffectiveSpawns,
    inputs: &EffectiveSpawnInputs,
    events: &SourceGameEventManager,
    groups: &SourceSpawnGroups,
    creature_templates: &LoadedCreatureTemplates,
    gameobject_templates: &LoadedGameObjectTemplates,
    states: &mut SourceManagers,
    products: &mut SourceTerrainManager,
    objects: &mut SourceObjects,
    corpses: &mut SourceCorpseRegistry,
    thread: u64,
    config: &[u8],
) -> Result<ActivePrefixReport> {
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
    let mut factory = Prefix {
        spawns,
        events,
        groups,
        creature_templates,
        gameobject_templates,
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
        motions: BTreeMap::new(),
        entry: None,
        lookups: Vec::new(),
        corpses,
        corpse_transfers: FrozenCorpseTransfers,
        instanceable: map.entry.instanceable(),
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
    let refusal = active
        .load(false, &mut context, &mut factory)
        .err()
        .ok_or_else(|| refuse("execution.continents.load_transports_owner"))?;
    let attempt = *factory.attempts.last().ok_or_else(|| refusal.clone())?;
    let bound_guid = factory
        .candidates
        .last()
        .map(|handle| context.objects.get(*handle).map(|object| object.guid))
        .transpose()?
        .flatten();
    Ok(ActivePrefixReport {
        refusal,
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
