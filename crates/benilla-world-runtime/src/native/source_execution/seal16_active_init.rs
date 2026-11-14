// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded active traversal and the first unowned template call, not complete object construction.
use super::*;
use benilla_sim::corpse_appearance::EquippedItem;
use benilla_sim::resident_map::CellPair;

pub(super) const CREATURE_TEMPLATE_OWNER: &str =
    "execution.object_mgr.loaded_creature_templates_owner";
pub(super) const GO_TEMPLATE_OWNER: &str = "execution.object_mgr.loaded_gameobject_template_owner";

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
            return Err(refuse(GO_TEMPLATE_OWNER));
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
        self.events.creature_event_data(key.guid)?;
        self.lookups.push("active_event");
        self.entry = Some(entry);
        self.lookups.push("creature_template");
        Err(refuse(CREATURE_TEMPLATE_OWNER))
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
    pub entry: u32,
    pub lookups: Vec<&'static str>,
}

#[allow(clippy::too_many_arguments)]
pub(super) fn load_first(
    map: &mut SourceContinent,
    spawns: &SourceEffectiveSpawns,
    inputs: &EffectiveSpawnInputs,
    events: &SourceGameEventManager,
    groups: &SourceSpawnGroups,
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
    Ok(ActivePrefixReport {
        refusal,
        attempt,
        candidates: factory.candidates.len(),
        entry: factory
            .entry
            .ok_or_else(|| refuse("execution.active.template_entry_missing"))?,
        lookups: factory.lookups,
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
