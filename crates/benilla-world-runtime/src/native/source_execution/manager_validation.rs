// SPDX-License-Identifier: GPL-3.0-or-later
//! F4 foreign-owner joins at an authored quiescent boundary, not a save schema.
use super::*;
use std::collections::BTreeSet;

pub struct MapManagerInventory {
    pub dispatch_stopped: bool,
    pub spawn: SpawnInventory,
    pub weather: WeatherInventory,
    pub linking: LinkingHolderInventory,
    pub messages: MessageInventory,
}
pub struct ManagerCompositionInventory {
    pub persistent: PersistentManagerInventory,
    pub pools: PoolInventory,
    pub events: GameEventInventory,
    pub groups: SpawnGroupsInventory,
    pub corpses: CorpseRegistryInventory,
    pub event_effects: EventEffectsInventory,
    pub maps: Vec<MapManagerInventory>,
}
impl ManagerCompositionInventory {
    /// No effects run here. Retained unloaded persistent states are legal; linking
    /// holders deliberately retain stale GUIDs (CreatureLinkingMgr.cpp:328-398).
    pub fn validate(
        &self,
        objects: &SourceObjects,
        transports: &SourceMoTransports,
        formations: &SourceFormations,
    ) -> Result<()> {
        self.persistent.validate()?;
        self.pools.require_quiescent()?;
        self.groups.require_quiescent()?;
        formations.validate_manager_owners(&self.groups, objects)?;
        require(
            !self.persistent.stopped && !self.corpses.stopped && !self.events.stopped,
            "execution.managers.stopped_boundary",
        )?;
        self.event_effects.validate()?;
        let mut maps = BTreeSet::new();
        for map in &self.maps {
            let id = map.spawn.map;
            map.spawn.validate()?;
            map.messages.validate()?;
            require(
                maps.insert(id)
                    && map.weather.map == id
                    && map.linking.map == id
                    && map.messages.owner == MessageOwner::Map(id)
                    && !map.spawn.stopped
                    && !map.dispatch_stopped
                    && !map.weather.stopped
                    && !map.linking.stopped
                    && !map.spawn.updating
                    && map.spawn.constructing_depth == 0
                    && !map.spawn.active.iter().any(|s| s.in_use),
                "execution.managers.map_inventory",
            )?;
            let state = self
                .persistent
                .states
                .iter()
                .find(|s| s.identity == map.spawn.persistent)
                .ok_or_else(|| refuse("execution.managers.spawn_persistent_owner"))?;
            require(
                state.used_by_map == Some(id),
                "execution.managers.persistent_map_owner",
            )?;
            for (&key, &handle) in &map.spawn.groups {
                require(
                    key == handle.id
                        && self.groups.states.iter().any(|g| {
                            g.identity == GroupIdentity { map: id, handle }
                                && g.persistent == state.identity
                        }),
                    "execution.managers.spawn_group_owner",
                )?;
            }
            for &handle in map.spawn.string_groups.values() {
                require(
                    self.groups
                        .states
                        .iter()
                        .any(|g| g.identity == GroupIdentity { map: id, handle }),
                    "execution.managers.string_group_owner",
                )?;
            }
            let mut zones = BTreeSet::new();
            require(
                map.linking.version == 1
                    && map
                        .weather
                        .zones_in_source_order
                        .iter()
                        .all(|z| zones.insert(z.zone)),
                "execution.managers.weather_linking_inventory",
            )?;
            require(
                map.linking
                    .entries
                    .values()
                    .chain(map.linking.guids.values())
                    .flatten()
                    .all(|g| !g.in_use),
                "execution.managers.linking_in_use",
            )?;
        }
        for state in &self.persistent.states {
            state.pools.validate_manager_owners(&self.pools)?;
            require(
                state.used_by_map.is_none_or(|map| maps.contains(&map)),
                "execution.managers.omitted_map_owner",
            )?;
        }
        for group in &self.groups.states {
            require(
                maps.contains(&group.identity.map)
                    && self.persistent.states.iter().any(|s| {
                        s.identity == group.persistent && s.used_by_map == Some(group.identity.map)
                    }),
                "execution.managers.group_persistent_owner",
            )?;
            let definition = &self.groups.definitions[&group.identity.handle.id];
            for guid in group.objects.keys() {
                require(
                    definition.members.iter().any(|m| m.guid == *guid),
                    "execution.managers.group_selected_member",
                )?;
            }
        }
        let count = self.events.definitions.len();
        require(
            !self.events.initialized || self.events.loaded,
            "execution.managers.event_initialization",
        )?;
        require(
            self.events.active.iter().all(|id| usize::from(*id) < count)
                && self
                    .events
                    .pools
                    .iter()
                    .flatten()
                    .all(|pool| usize::from(*pool) < self.pools.templates.len())
                && self
                    .events
                    .substitution_events
                    .values()
                    .flatten()
                    .all(|id| usize::from(*id) < count),
            "execution.managers.event_foreign_owner",
        )?;
        let mut owners = BTreeSet::new();
        let mut handles = BTreeSet::new();
        for &(owner, handle) in &self.corpses.owners_in_source_order {
            let corpse = self
                .corpses
                .corpses
                .iter()
                .find(|c| c.object == handle)
                .ok_or_else(|| refuse("execution.managers.corpse_owner"))?;
            require(
                owners.insert(owner.raw())
                    && handles.insert(handle)
                    && corpse.owner == owner
                    && corpse.corpse_type != 0,
                "execution.managers.corpse_registry",
            )?;
        }
        for corpse in &self.corpses.corpses {
            let pair = SourcePose(corpse.position).cell()?;
            if corpse.corpse_type != 0 {
                require(
                    self.corpses.cells.iter().any(|(map, cell, entries)| {
                        *map == corpse.map
                            && *cell == persistent_cell_key(pair)
                            && entries.contains(&(corpse.owner.counter(), corpse.instance))
                    }),
                    "execution.managers.corpse_cell_owner",
                )?;
            }
            let object = objects.get(corpse.object)?;
            require(
                corpse.fields.len() == SOURCE_CORPSE_FIELDS
                    && corpse.corpse_type <= 2
                    && object.kind
                        == if corpse.corpse_type == 0 {
                            SourceObjectKind::Bones
                        } else {
                            SourceObjectKind::Corpse
                        }
                    && corpse
                        .map_binding
                        .is_none_or(|map| map.map == corpse.map && map.instance == corpse.instance)
                    && object.map == corpse.map_binding
                    && object.in_world == corpse.in_world
                    && corpse.map_binding.is_none_or(|map| maps.contains(&map))
                    && (corpse.corpse_type == 0 || handles.contains(&corpse.object))
                    && corpse
                        .reference
                        .is_none_or(|reference| Some(reference.grid.map) == corpse.map_binding),
                "execution.managers.corpse_object_owner",
            )?;
        }
        let mut cell_keys = BTreeSet::new();
        for (map, cell, entries) in &self.corpses.cells {
            let mut players = BTreeSet::new();
            require(
                cell_keys.insert((*map, *cell))
                    && entries.iter().all(|(player, instance)| {
                        players.insert(*player)
                            && self.corpses.corpses.iter().any(|corpse| {
                                corpse.corpse_type != 0
                                    && corpse.map == *map
                                    && corpse.instance == *instance
                                    && corpse.owner.counter() == *player
                                    && SourcePose(corpse.position)
                                        .cell()
                                        .is_ok_and(|pair| persistent_cell_key(pair) == *cell)
                            })
                    }),
                "execution.managers.corpse_cell_owner",
            )?;
        }
        transports.validate_manager_owners(objects, &maps, &self.maps)?;
        Ok(())
    }
    pub fn require_durable_checkpoint(&self) -> Result<()> {
        Err(MissingManagerOwner::F7Continuation.refusal())
    }
}
