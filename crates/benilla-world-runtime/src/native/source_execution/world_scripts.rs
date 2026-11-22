// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World-script construction and the two continents' inherited empty respawn callback.
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

    // InstanceData.h:120 is empty. world_eastern_kingdoms.cpp:110-116 overrides it.
    pub(super) fn on_event_happened(
        &mut self,
        objects: &SourceObjects,
        event: u16,
        activate: bool,
    ) -> Result<()> {
        // sc_instance.cpp:243-253 resolves the stored GUID through the map.
        if matches!(self.state, WorldScriptState::EasternKingdoms)
            && event == 16
            && activate
            && self.stores.creatures.get(&14508).is_some_and(|&guid| {
                objects
                    .lookup_world_object(self.map, SourceGuid::from_raw(guid))
                    .is_some()
            })
        {
            return Err(refuse(
                "execution.world_script.world_eastern_kingdoms_115.MoveWaypoint_owner",
            ));
        }
        Ok(())
    }

    /// InstanceData.h:102; world_kalimdor.cpp:40 and world_eastern_kingdoms.cpp:28 inherit this empty callback.
    pub fn creature_respawn(&mut self) -> Result<()> {
        Self::respawn_for_class(match self.state {
            WorldScriptState::EasternKingdoms => "world_map_eastern_kingdoms",
            WorldScriptState::Kalimdor(_) => "world_map_kalimdor",
        })
    }
    fn respawn_for_class(class: &str) -> Result<()> {
        if matches!(class, "world_map_eastern_kingdoms" | "world_map_kalimdor") {
            Ok(())
        } else {
            Err(Refusal {
                id: format!("execution.world_script.{class}.OnCreatureRespawn_override_owner"),
            })
        }
    }
    /// InstanceData.h:93. Both continent classes inherit the empty body.
    pub(in crate::native::source_execution) fn gameobject_spawned(&mut self) -> Result<()> {
        Self::gameobject_spawned_for_class(match self.state {
            WorldScriptState::EasternKingdoms => "world_map_eastern_kingdoms",
            WorldScriptState::Kalimdor(_) => "world_map_kalimdor",
        })
    }
    fn gameobject_spawned_for_class(class: &str) -> Result<()> {
        if matches!(class, "world_map_eastern_kingdoms" | "world_map_kalimdor") {
            Ok(())
        } else {
            Err(refuse(&format!(
                "execution.world_script.{class}.OnObjectSpawn_override_owner"
            )))
        }
    }
    /// world_eastern_kingdoms.cpp:69-86; world_kalimdor.cpp:70-87.
    pub fn creature_create(&mut self, entry: u32, guid: u64) {
        if entry == 16421 {
            self.stores
                .creature_collections
                .entry(entry)
                .or_default()
                .push(guid);
        }
        let store = match &self.state {
            WorldScriptState::EasternKingdoms => {
                matches!(entry, 466 | 1747 | 1748 | 1749 | 12580 | 5635 | 14508)
            }
            WorldScriptState::Kalimdor(_) => {
                matches!(entry, 6019 | 10323 | 15467 | 14464 | 14457 | 14454 | 14461)
            }
        };
        if store {
            self.stores.creatures.insert(entry, guid);
        }
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

#[test]
fn world_creature_create_uses_continent_keys_and_preserves_collection_order() {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    for (state, keys) in [
        (
            WorldScriptState::EasternKingdoms,
            [466, 1747, 1748, 1749, 12580, 5635, 14508],
        ),
        (
            WorldScriptState::Kalimdor(KalimdorWorldState::default()),
            [6019, 10323, 15467, 14464, 14457, 14454, 14461],
        ),
    ] {
        let mut script = SourceWorldScript {
            map,
            stores: WorldScriptStores::default(),
            state,
        };
        script.creature_create(4692, 99);
        script.creature_respawn().unwrap();
        script.gameobject_spawned().unwrap();
        assert_eq!(script.stores, WorldScriptStores::default());
        assert!(script.stores.creatures.is_empty());
        for entry in keys {
            script.creature_create(entry, u64::from(entry) + 1000);
        }
        assert_eq!(
            script
                .stores
                .creatures
                .keys()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            keys.into_iter().collect::<std::collections::BTreeSet<_>>()
        );
        script.creature_create(keys[0], 17);
        assert_eq!(script.stores.creatures[&keys[0]], 17);
        script.creature_create(16421, 9);
        script.creature_create(16421, 10);
        assert_eq!(script.stores.creature_collections[&16421], [9, 10]);
    }
}

#[test]
fn overriding_instance_respawn_requires_its_own_callback() {
    assert_eq!(
        SourceWorldScript::respawn_for_class("instance_blackwing_lair")
            .unwrap_err()
            .id,
        "execution.world_script.instance_blackwing_lair.OnCreatureRespawn_override_owner"
    );
}

#[test]
fn overriding_instance_gameobject_spawn_requires_its_own_callback() {
    assert_eq!(
        SourceWorldScript::gameobject_spawned_for_class("instance_scarlet_monastery")
            .unwrap_err()
            .id,
        "execution.world_script.instance_scarlet_monastery.OnObjectSpawn_override_owner"
    );
}
