// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Authored F4.21 composition, not F1 admission or persisted continuation.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MissingManagerOwner {
    F3SourceCutover,
    F5MapSessions,
    F5PlayerUpdate,
    F5ScriptInitialization,
    F5InstanceInitialization,
    F6Navigation,
    F6DynamicCollisionTree,
    F7Continuation,
}
impl MissingManagerOwner {
    pub fn refusal(self) -> Refusal {
        refuse(match self {
            Self::F3SourceCutover => "execution.managers.f3_13.source_cutover_owner",
            Self::F5MapSessions => "execution.managers.f5.map_sessions_owner",
            Self::F5PlayerUpdate => "execution.managers.f5.player_update_owner",
            Self::F5ScriptInitialization => "execution.managers.f5.script_initialization_owner",
            Self::F5InstanceInitialization => "execution.managers.f5.instance_initialization_owner",
            Self::F6Navigation => "execution.managers.f6.navigation_owner",
            Self::F6DynamicCollisionTree => "execution.managers.f6.dynamic_collision_tree_owner",
            Self::F7Continuation => "execution.managers.f7.continuation_owner",
        })
    }
}

/// These are ordered manager points, not permission to skip intervening World loaders.
/// F3.13 must execute those loaders before returning from each enclosing point.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum ManagerStartupPhase {
    TransportAnimations,
    Linking,
    Pools,
    Weather,
    Quests,
    Events,
    Conditions,
    Groups,
    TransportTemplates,
    PersistentMaps,
    CreatureRespawns,
    GameObjectRespawns,
    Graveyards,
    Corpses,
    Scripts,
    WorldTimers,
    MapSystem,
    EventInitialization,
}
pub const MANAGER_STARTUP_PHASES: [ManagerStartupPhase; 18] = [
    ManagerStartupPhase::TransportAnimations,
    ManagerStartupPhase::Linking,
    ManagerStartupPhase::Pools,
    ManagerStartupPhase::Weather,
    ManagerStartupPhase::Quests,
    ManagerStartupPhase::Events,
    ManagerStartupPhase::Conditions,
    ManagerStartupPhase::Groups,
    ManagerStartupPhase::TransportTemplates,
    ManagerStartupPhase::PersistentMaps,
    ManagerStartupPhase::CreatureRespawns,
    ManagerStartupPhase::GameObjectRespawns,
    ManagerStartupPhase::Graveyards,
    ManagerStartupPhase::Corpses,
    ManagerStartupPhase::Scripts,
    ManagerStartupPhase::WorldTimers,
    ManagerStartupPhase::MapSystem,
    ManagerStartupPhase::EventInitialization,
];
impl ManagerStartupPhase {
    /// World.cpp at the pinned revision. Consecutive points name both ends of each edge.
    pub fn source_line(self) -> u32 {
        match self {
            Self::TransportAnimations => 970,
            Self::Linking => 1092,
            Self::Pools => 1095,
            Self::Weather => 1098,
            Self::Quests => 1101,
            Self::Events => 1109,
            Self::Conditions => 1120,
            Self::Groups => 1123,
            Self::TransportTemplates => 1127,
            Self::PersistentMaps => 1130,
            Self::CreatureRespawns => 1134,
            Self::GameObjectRespawns => 1137,
            Self::Graveyards => 1158,
            Self::Corpses => 1196,
            Self::Scripts => 1333,
            Self::WorldTimers => 1345,
            Self::MapSystem => 1389,
            Self::EventInitialization => 1416,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MapManagerStartupPhase {
    InstanceData,
    PersistentBinding,
    Pools,
    Graveyards,
    Variables,
    SpawnManager,
    Navigation,
    ActiveEntities,
    Transports,
    MapSpecific,
}
pub const MAP_MANAGER_STARTUP_PHASES: [MapManagerStartupPhase; 10] = [
    MapManagerStartupPhase::InstanceData,
    MapManagerStartupPhase::PersistentBinding,
    MapManagerStartupPhase::Pools,
    MapManagerStartupPhase::Graveyards,
    MapManagerStartupPhase::Variables,
    MapManagerStartupPhase::SpawnManager,
    MapManagerStartupPhase::Navigation,
    MapManagerStartupPhase::ActiveEntities,
    MapManagerStartupPhase::Transports,
    MapManagerStartupPhase::MapSpecific,
];
impl MapManagerStartupPhase {
    /// Map.cpp; MapManager.cpp:96-110 launches both continents and joins before events.
    pub fn source_line(self) -> u32 {
        match self {
            Self::InstanceData => 190,
            Self::PersistentBinding => 192,
            Self::Pools => 194,
            Self::Graveyards => 196,
            Self::Variables => 198,
            Self::SpawnManager => 200,
            Self::Navigation => 202,
            Self::ActiveEntities => 214,
            Self::Transports => 216,
            Self::MapSpecific => 218,
        }
    }
}

pub trait ManagerStartupWork {
    fn navigation_manager(&mut self) -> Result<&mut SourceMMapManager> {
        Err(MissingManagerOwner::F6Navigation.refusal())
    }
    fn startup(&mut self, phase: ManagerStartupPhase) -> Result<()> {
        Err(match phase {
            ManagerStartupPhase::Scripts => MissingManagerOwner::F5ScriptInitialization,
            _ => MissingManagerOwner::F3SourceCutover,
        }
        .refusal())
    }
    fn initialize_map(&mut self, map: MapIdentity, phase: MapManagerStartupPhase) -> Result<()> {
        if phase == MapManagerStartupPhase::Navigation {
            return self.navigation_manager()?.initialize_map(map);
        }
        Err(match phase {
            MapManagerStartupPhase::InstanceData => MissingManagerOwner::F5InstanceInitialization,
            _ => MissingManagerOwner::F3SourceCutover,
        }
        .refusal())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ManagerStart {
    Fresh,
    /// SQL status is not a source-world continuation. Never rerun startup on Open.
    Continuation,
}
#[derive(Default)]
pub struct SourceManagerStartup {
    started: bool,
}
impl SourceManagerStartup {
    pub fn run(&mut self, start: ManagerStart, work: &mut impl ManagerStartupWork) -> Result<()> {
        require(!self.started, "execution.managers.startup_repeated")?;
        self.started = true;
        if start == ManagerStart::Continuation {
            return Err(MissingManagerOwner::F7Continuation.refusal());
        }
        for phase in MANAGER_STARTUP_PHASES {
            work.startup(phase)?;
        }
        Ok(())
    }
}
/// The caller is the real MapManager initializer job, not a new scheduler.
/// SourceContinents owns the GUID/grid/visibility/TerrainInfo prelude before this phase dispatcher.
pub struct SourceMapManagerStartup {
    map: MapIdentity,
    started: bool,
}
impl SourceMapManagerStartup {
    pub fn new(map: MapIdentity) -> Result<Self> {
        require(
            map.creation_generation > 0,
            "execution.managers.map_generation",
        )?;
        Ok(Self {
            map,
            started: false,
        })
    }
    pub fn run(&mut self, start: ManagerStart, work: &mut impl ManagerStartupWork) -> Result<()> {
        require(!self.started, "execution.managers.map_startup_repeated")?;
        self.started = true;
        if start == ManagerStart::Continuation {
            return Err(MissingManagerOwner::F7Continuation.refusal());
        }
        for phase in MAP_MANAGER_STARTUP_PHASES {
            work.initialize_map(self.map, phase)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum MapManagerPhase {
    Calendar,
    CollisionTree,
    Messages,
    SpawnManager,
    Transports,
    Sessions,
    Players,
    CollectObjects,
    ObjectUpdates,
    ObjectFields,
    GridStates,
    Scripts,
    Instance,
    Weather,
}
impl MapManagerPhase {
    /// Map.cpp, disabled-playerbots branch. Each consecutive edge retains these endpoints.
    pub fn source_line(self) -> u32 {
        match self {
            Self::Calendar => 702,
            Self::CollisionTree => 712,
            Self::Messages => 714,
            Self::SpawnManager => 715,
            Self::Transports => 725,
            Self::Sessions => 743,
            Self::Players => 823,
            Self::CollectObjects => 894,
            Self::ObjectUpdates => 995,
            Self::ObjectFields => 1007,
            Self::GridStates => 1011,
            Self::Scripts => 1024,
            Self::Instance => 1027,
            Self::Weather => 1030,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct ManagerDispatchEvent {
    pub map: MapIdentity,
    pub phase: MapManagerPhase,
    /// Begin is emitted before invoking the owner. A refusal has no successful end.
    pub complete: bool,
}

/// Borrow the landed owners. No selected answers, visible-only roster or extra RNG stream.
pub struct MapManagerContext<'a> {
    pub spawn: &'a mut SourceSpawnManager,
    pub persistent: &'a mut SourceManagers,
    pub messages: &'a mut SourceMessager,
    pub transports: &'a mut SourceMoTransports,
    pub templates: &'a SourceTransportTemplates,
    pub weather: &'a mut SourceMapWeather,
    pub objects: &'a mut SourceObjects,
    pub cells: &'a mut SourceCells,
}
impl MapManagerContext<'_> {
    pub fn validate(&self) -> Result<()> {
        let map = self.cells.map();
        require(
            self.spawn.map() == map
                && self.weather.map() == map
                && self.messages.owner() == MessageOwner::Map(map)
                && self
                    .persistent
                    .state(self.spawn.persistent())?
                    .map_binding()
                    == Some(map),
            "execution.managers.cross_map_binding",
        )
    }
}
/// Only authored implementations exist before F3.13/F5/F6. Defaults refuse explicitly.
pub trait MapManagerWork:
    EventMapOwner
    + EventCreatureOwner
    + MoTransportOwner
    + SourceVisitInputs
    + SourceObjectUpdateOwner
    + SourceMapEndWork
{
    fn calendar(&mut self, _map: MapIdentity) -> Result<()> {
        Err(MissingManagerOwner::F3SourceCutover.refusal())
    }
    fn dynamic_geometry(
        &mut self,
        _map: MapIdentity,
    ) -> Result<(&mut SourceDynamicMap, &SourceDynamicModels)> {
        Err(MissingManagerOwner::F6DynamicCollisionTree.refusal())
    }
    fn collision_tree(&mut self, map: MapIdentity, diff: u32) -> Result<()> {
        let (dynamic, models) = self.dynamic_geometry(map)?;
        require(
            dynamic.map() == map,
            "execution.dynamic_collision.cross_map_binding",
        )?;
        dynamic.update(models, diff)
    }
    fn sessions(&mut self, _context: &mut MapManagerContext<'_>, _diff: u32) -> Result<()> {
        Err(MissingManagerOwner::F5MapSessions.refusal())
    }
    fn players(&mut self, _context: &mut MapManagerContext<'_>, _diff: u32) -> Result<()> {
        Err(MissingManagerOwner::F5PlayerUpdate.refusal())
    }
    fn begin_map_message(&mut self, message: &MessageEnvelope) -> Result<()>;
}
struct MapMessages<'a, 'b, W> {
    context: &'a mut MapManagerContext<'b>,
    work: &'a mut W,
}
impl<W: MapManagerWork> MessageWork for MapMessages<'_, '_, W> {
    fn begin_message(&mut self, message: &MessageEnvelope) -> Result<()> {
        self.work.begin_map_message(message)
    }
    fn forward_game_event(&mut self, _: u64, _: &[EventMapGuids], _: bool) -> Result<()> {
        Err(refuse("execution.message_owner"))
    }
    fn map_game_event(
        &mut self,
        _: u64,
        _: MapIdentity,
        creatures: &[u32],
        gameobjects: &[u32],
        spawn: bool,
    ) -> Result<()> {
        execute_map_game_event(
            self.context.spawn,
            self.context.persistent,
            creatures,
            gameobjects,
            spawn,
            self.work,
        )
    }
    fn creature_present(&self, map: MapIdentity, guid: u64) -> bool {
        self.context
            .objects
            .lookup(map, LiveLookupKind::Creature, SourceGuid::from_raw(guid))
            .is_some()
    }
    fn update_creature_event(
        &mut self,
        _: u64,
        map: MapIdentity,
        update: EventCreatureUpdate,
    ) -> Result<()> {
        execute_creature_event(map, update, self.work)
    }
    fn attach_transport(
        &mut self,
        _: u64,
        _: MapIdentity,
        transfer: TransportTransfer,
    ) -> Result<()> {
        self.context.transports.deliver_transfer(
            transfer,
            self.context.cells,
            self.context.objects,
            self.work,
        )
    }
}
#[derive(Default)]
pub struct SourceMapManagerDispatch {
    stopped: bool,
}
impl SourceMapManagerDispatch {
    pub fn is_stopped(&self) -> bool {
        self.stopped
    }
    /// Run inside the existing SourceWorld map job/barrier, including off-camera maps.
    /// Failure stops this execution rather than retrying a partially applied callback.
    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        context: &mut MapManagerContext<'_>,
        work: &mut impl MapManagerWork,
        weather_context: &mut impl benilla_sim::source_weather::Context<Error = Refusal>,
        diff: u32,
        world_snapshot: u32,
        map_visibility: f32,
        observe: &mut impl FnMut(ManagerDispatchEvent) -> Result<()>,
        observe_objects: &mut impl FnMut(SourceUpdateEvent) -> Result<()>,
    ) -> Result<()> {
        require(!self.stopped, "execution.managers.map_stopped")?;
        let result = (|| {
            context.validate()?;
            let map = context.cells.map();
            let mut phase = |phase, complete| {
                observe(ManagerDispatchEvent {
                    map,
                    phase,
                    complete,
                })
            };
            phase(MapManagerPhase::Calendar, false)?;
            work.calendar(map)?;
            phase(MapManagerPhase::Calendar, true)?;
            phase(MapManagerPhase::CollisionTree, false)?;
            work.collision_tree(map, diff)?;
            phase(MapManagerPhase::CollisionTree, true)?;
            phase(MapManagerPhase::Messages, false)?;
            let batch = context.messages.take_batch(MessageOwner::Map(map))?;
            batch.execute(
                MessageOwner::Map(map),
                work.thread(),
                &mut MapMessages { context, work },
            )?;
            phase(MapManagerPhase::Messages, true)?;
            phase(MapManagerPhase::SpawnManager, false)?;
            context.spawn.update(context.persistent, work)?;
            phase(MapManagerPhase::SpawnManager, true)?;
            // Map.cpp:718 resets before transport and player callbacks.
            context.cells.reset_update_marks();
            phase(MapManagerPhase::Transports, false)?;
            context.transports.update_map(
                context.templates,
                map,
                world_snapshot,
                diff,
                context.objects,
                work,
            )?;
            phase(MapManagerPhase::Transports, true)?;
            phase(MapManagerPhase::Sessions, false)?;
            work.sessions(context, diff)?;
            phase(MapManagerPhase::Sessions, true)?;
            phase(MapManagerPhase::Players, false)?;
            work.players(context, diff)?;
            phase(MapManagerPhase::Players, true)?;
            phase(MapManagerPhase::CollectObjects, false)?;
            let set = super::visitors::collect_source_updates_after_reset(
                context.objects,
                context.cells,
                map_visibility,
                work,
                observe_objects,
            )?;
            phase(MapManagerPhase::CollectObjects, true)?;
            phase(MapManagerPhase::ObjectUpdates, false)?;
            let retained_set =
                set.dispatch(diff, context.objects, context.cells, work, observe_objects)?;
            phase(MapManagerPhase::ObjectUpdates, true)?;
            for (end, dispatch) in [
                (MapEndPhase::ObjectFields, MapManagerPhase::ObjectFields),
                (MapEndPhase::GridStates, MapManagerPhase::GridStates),
                (MapEndPhase::Scripts, MapManagerPhase::Scripts),
                (MapEndPhase::Instance, MapManagerPhase::Instance),
            ] {
                phase(dispatch, false)?;
                work.map_end_phase(end, map, diff, context.objects)?;
                phase(dispatch, true)?;
            }
            phase(MapManagerPhase::Weather, false)?;
            context
                .weather
                .update(diff, context.objects, weather_context, work)?;
            phase(MapManagerPhase::Weather, true)?;
            // Map.cpp:1031 ends the local unordered-set lifetime only after weather.
            drop(retained_set);
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct ManagerDispatchDivergence {
    pub ordinal: usize,
    pub expected: Option<ManagerDispatchEvent>,
    pub actual: Option<ManagerDispatchEvent>,
    pub reason: Refusal,
}
impl DifferentialOracle {
    /// Diagnostics only. Expected operations cannot be passed to the dispatcher.
    pub fn compare_manager_dispatch(
        expected: impl IntoIterator<Item = ManagerDispatchEvent>,
        actual: impl IntoIterator<Item = ManagerDispatchEvent>,
    ) -> std::result::Result<usize, ManagerDispatchDivergence> {
        let mut expected = expected.into_iter();
        let mut actual = actual.into_iter();
        for ordinal in 0.. {
            let e = expected.next();
            let a = actual.next();
            if e.is_none() && a.is_none() {
                return Ok(ordinal);
            }
            if e != a {
                return Err(ManagerDispatchDivergence {
                    ordinal,
                    expected: e,
                    actual: a,
                    reason: refuse(if e.is_none() || a.is_none() {
                        "execution.managers.dispatch_exhaustion"
                    } else {
                        "execution.managers.dispatch_divergence"
                    }),
                });
            }
        }
        unreachable!()
    }
}
