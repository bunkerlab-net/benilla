// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World-script construction only. Live callbacks remain separate F5 owners.
use super::*;

/// ScriptedInstance's four default-constructed stores, sc_instance.h:74-78.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct WorldScriptStores {
    pub gameobjects: BTreeMap<u32, u64>,
    pub creatures: BTreeMap<u32, u64>,
    pub creature_collections: BTreeMap<u32, Vec<u64>>,
    pub gameobject_collections: BTreeMap<u32, Vec<u64>>,
}
#[derive(Debug, PartialEq, Eq)]
pub struct GhostOPlasmEvent {
    pub guid: u64,
    pub despawn_timer: u32,
    pub phase_counter: u8,
    pub summoned_magrami: Vec<u64>,
}
/// world_kalimdor.cpp:44-53; MAX_ENCOUNTER is five, MAX_ELEMENTS is four.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct KalimdorWorldState {
    pub murkdeep_killed_adds: u8,
    pub ghost_events: Vec<GhostOPlasmEvent>,
    pub omen_reset_timer: u32,
    pub omen_moonlight_timer: u32,
    pub rockets_counter: u8,
    pub encounters: [u32; 5],
    pub omen_spell_credit_done: bool,
    pub elemental_rifts: [Vec<u64>; 4],
    pub drones_timer: u32,
    pub freed_sprite_darter: u32,
}
#[derive(Debug, PartialEq, Eq)]
pub enum WorldScriptState {
    EasternKingdoms,
    Kalimdor(KalimdorWorldState),
}
#[derive(Debug, PartialEq, Eq)]
pub struct SourceWorldScript {
    pub map: MapIdentity,
    pub stores: WorldScriptStores,
    pub state: WorldScriptState,
}
impl SourceWorldScript {
    /// Neither continent overrides InstanceData.h:75's inherited query.
    pub fn encounter_in_progress(&self) -> bool {
        false
    }
    /// world_eastern_kingdoms.cpp:100-108; world_kalimdor.cpp:173-200.
    /// True requests the caller's SetActiveObjectState at world_kalimdor.cpp:179.
    pub fn gameobject_create(&mut self, entry: u32, guid: u64) -> bool {
        if entry == 181136 {
            self.stores
                .gameobject_collections
                .entry(entry)
                .or_default()
                .push(guid);
        }
        if let WorldScriptState::Kalimdor(state) = &mut self.state {
            match entry {
                177746 => {
                    state.ghost_events.push(GhostOPlasmEvent {
                        guid,
                        despawn_timer: 0,
                        phase_counter: 0,
                        summoned_magrami: Vec::new(),
                    });
                    return true;
                }
                180875 => {
                    self.stores.gameobjects.insert(entry, guid);
                }
                179664 => state.elemental_rifts[0].push(guid),
                179665 => state.elemental_rifts[3].push(guid),
                179666 => state.elemental_rifts[2].push(guid),
                179667 => state.elemental_rifts[1].push(guid),
                _ => {}
            }
        }
        false
    }
    /// ScriptDevAIMgr.cpp:405-410 dispatches the registered factory by script ID/name.
    /// Registration: world_eastern_kingdoms.cpp:135-145; world_kalimdor.cpp:392-402.
    pub fn construct<E: RawGenerator, S: FnMut(TraceEvent)>(
        map: MapIdentity,
        name: &str,
        variables: &mut SourceWorldStateVariables,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<Self> {
        let state = match name {
            "world_map_eastern_kingdoms" => WorldScriptState::EasternKingdoms,
            "world_map_kalimdor" => WorldScriptState::Kalimdor(KalimdorWorldState::default()),
            _ => return Err(MissingManagerOwner::F5InstanceInitialization.refusal()),
        };
        let mut script = Self {
            map,
            stores: WorldScriptStores::default(),
            state,
        };
        // Both constructors call Initialize after base/member construction.
        script.initialize(variables, random)?;
        Ok(script)
    }
    pub fn initialize<E: RawGenerator, S: FnMut(TraceEvent)>(
        &mut self,
        variables: &mut SourceWorldStateVariables,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<()> {
        match &mut self.state {
            WorldScriptState::EasternKingdoms => {
                // world_eastern_kingdoms.cpp:37-38, WorldStateDefines.h:164-165.
                // Cold WorldStateVariableManager entries have send=false, so no broadcast.
                let first = random
                    .urand(0, 1)
                    .map_err(|_| refuse("execution.world_script.urand"))?;
                variables.set(330001, first as i32)?;
                let second = random
                    .urand(0, 1)
                    .map_err(|_| refuse("execution.world_script.urand"))?;
                variables.set(330002, second as i32)?;
            }
            WorldScriptState::Kalimdor(state) => {
                // world_kalimdor.cpp:57-67, in source assignment order. No random calls.
                state.murkdeep_killed_adds = 0;
                state.ghost_events.clear();
                state.omen_reset_timer = 0;
                state.omen_moonlight_timer = 0;
                state.rockets_counter = 0;
                state.freed_sprite_darter = 0;
                state.omen_spell_credit_done = false;
                for rifts in &mut state.elemental_rifts {
                    rifts.clear();
                }
                state.drones_timer = 0;
                state.encounters.fill(0);
            }
        }
        Ok(())
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum WorldInstanceRow<'a> {
    Missing,
    Null,
    Data(&'a [u8]),
}
/// Character storage must distinguish absent rows from NULL and empty data.
pub trait WorldInstanceStorage {
    fn query(&mut self, map: u32, statement: &str) -> Result<WorldInstanceRow<'_>>;
    fn insert(&mut self, map: u32) -> Result<()>;
}
/// Reads the same bound character snapshot as MapPersistentStateManager.
/// Submissions use PersistentEffects::sql; snapshot rows never imply write completion.
pub struct SnapshotWorldStorage<'a> {
    pub initial: &'a ManagerInitialInputs<'a>,
    pub effects: &'a mut dyn PersistentEffects,
    pub pending: std::collections::BTreeSet<u32>,
    pub issued: Vec<String>,
}
impl WorldInstanceStorage for SnapshotWorldStorage<'_> {
    fn query(&mut self, map: u32, statement: &str) -> Result<WorldInstanceRow<'_>> {
        require(
            !self.pending.contains(&map),
            "execution.world_script.pending_world_read_owner",
        )?;
        let row = self.initial.world_data(map)?;
        self.issued.push(statement.to_owned());
        Ok(row)
    }
    fn insert(&mut self, map: u32) -> Result<()> {
        self.effects.sql(PersistentSql::CreateWorld { map })?;
        self.pending.insert(map);
        self.issued
            .push(format!("INSERT INTO world VALUES ('{map}', '')"));
        Ok(())
    }
}
