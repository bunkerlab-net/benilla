// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! ObjectMgr loader state, with explicitly separate pre-terrain projections.
use super::super::production_compiler::{ObjectMgrSpawnRow, ObjectMgrSpawnRows, VerifiedCatalog};
use super::*;
use benilla_sim::resident_map::compute_cell_pair;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Default)]
pub struct EffectiveSpawnTemplate {
    pub flags: u32,
    pub totem_ai: bool,
    pub display: u32,
    pub despawn_at_action: bool,
}
#[derive(Default)]
pub struct EffectiveSpawnInputs {
    pub creatures: BTreeMap<u32, EffectiveSpawnTemplate>,
    pub gameobjects: BTreeMap<u32, EffectiveSpawnTemplate>,
    pub conditional: BTreeMap<u32, [u32; 2]>,
    /// These vectors must come from the preceding source loader, in its order.
    pub random: BTreeMap<SpawnKey, (bool, Vec<u32>)>,
    pub maps: BTreeMap<u32, u32>,
    /// Effective LoadInstanceTemplate result, not raw instance_template IDs.
    pub instances: Option<BTreeSet<u32>>,
    pub displays: BTreeSet<u32>,
    pub spawn_templates: BTreeSet<u32>,
    pub strings: BTreeSet<u32>,
}
pub trait EffectiveSpawnOwner {
    /// Borrow the F3 world-thread engine at World.cpp:1066, never reseed here.
    fn startup_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        Err(refuse(
            "execution.effective_spawns.world_thread_startup_rng_position_owner",
        ))
    }
    fn original_zone(&mut self, _map: u32, _pose: [f32; 3]) -> Result<u32> {
        Err(refuse(
            "execution.effective_spawns.terrain_original_zone_owner",
        ))
    }
    fn autoload_active(&mut self) -> Result<bool> {
        Err(refuse(
            "execution.effective_spawns.autoload_active_config_owner",
        ))
    }
}
#[derive(Clone, Debug)]
pub struct EffectiveSpawnData {
    pub row: ObjectMgrSpawnRow,
    pub validation_entry: u32,
    pub spawn_template: Option<u32>,
    original_zone: Option<u16>,
    pub animprogress: u32,
    pub go_state: i32,
    pub string_id: u32,
}
impl EffectiveSpawnData {
    pub fn original_zone(&self) -> Result<u16> {
        self.original_zone
            .ok_or_else(|| refuse("execution.effective_spawns.terrain_original_zone_owner"))
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct EffectiveSpawnRecord {
    pub key: SpawnKey,
    pub ordinal: u64,
    pub reason: String,
    pub line: u32,
}
#[derive(Default)]
pub struct SourceEffectiveSpawns {
    /// Inserted before map/coordinate checks, ObjectMgr.cpp:1955,2145.
    pre_terrain: BTreeMap<SpawnKey, EffectiveSpawnData>,
    refusals: BTreeMap<SpawnKey, Refusal>,
    completed: BTreeSet<SpawnKey>,
    pub records: Vec<EffectiveSpawnRecord>,
    pub max_creature_guid: u32,
    pub max_gameobject_guid: u32,
    pub static_grids: BTreeMap<(u32, u16, u16, GridSpawnKind), BTreeSet<u32>>,
    pub dynamic_guids: BTreeMap<(u32, GridSpawnKind), Vec<u32>>,
    pub active: BTreeMap<(u32, GridSpawnKind), Vec<u32>>,
    pub random_rows: [u64; 2],
    pub single_entry_draws: [u64; 2],
    pub conditional_rows: u64,
    pub draws_needed: u64,
    pub random_entries: BTreeSet<SpawnKey>,
}
impl EffectiveSpawnInputs {
    /// Prior alternative loader, ObjectMgr.cpp:1854-1864,2315-2325.
    pub fn load_random_entry(&mut self, kind: GridSpawnKind, guid: u32, entry: u32) {
        let creature = kind == GridSpawnKind::Creature;
        let templates = if creature {
            &self.creatures
        } else {
            &self.gameobjects
        };
        let Some(info) = templates.get(&entry) else {
            return;
        };
        let list = self.random.entry(SpawnKey { kind, guid }).or_default();
        // The creature source checks ACTIVE, not DYNGUID, at :1862.
        list.0 |= info.flags & if creature { 0x1000 } else { 2 } != 0;
        list.1.push(entry);
    }
}
pub trait EffectiveSpawnRows {
    fn next_spawn(&mut self, kind: GridSpawnKind) -> Result<Option<ObjectMgrSpawnRow>>;
}
impl EffectiveSpawnRows for ObjectMgrSpawnRows {
    fn next_spawn(&mut self, kind: GridSpawnKind) -> Result<Option<ObjectMgrSpawnRow>> {
        Ok(self.next(kind.table())?)
    }
}
impl SourceEffectiveSpawns {
    /// Execute World.cpp:1066 then :1072. A missing owner stops the prefix.
    /// The caller binds prerequisite loader products and the actual F3 stream.
    pub fn load_rows(
        &mut self,
        rows: &mut impl EffectiveSpawnRows,
        inputs: &EffectiveSpawnInputs,
        addons: &[[u32; 4]],
        owner: &mut impl EffectiveSpawnOwner,
    ) -> Result<()> {
        let mut gameobjects_seen = false;
        for kind in [GridSpawnKind::Creature, GridSpawnKind::GameObject] {
            while let Some(row) = rows.next_spawn(kind)? {
                gameobjects_seen |= kind == GridSpawnKind::GameObject;
                self.inspect_row(kind, row, inputs, owner)?;
            }
        }
        // An empty main GO query returns before the addon query, :2096-2102.
        if gameobjects_seen {
            self.apply_gameobject_addons(addons, &inputs.strings)?;
        }
        Ok(())
    }
    fn record(&mut self, key: SpawnKey, ordinal: u64, reason: &str, line: u32) {
        self.records.push(EffectiveSpawnRecord {
            key,
            ordinal,
            reason: reason.into(),
            line,
        });
    }
    /// Diagnostic continuation only. A refused row never becomes complete.
    pub fn inspect_row(
        &mut self,
        kind: GridSpawnKind,
        row: ObjectMgrSpawnRow,
        inputs: &EffectiveSpawnInputs,
        owner: &mut impl EffectiveSpawnOwner,
    ) -> Result<()> {
        let key = SpawnKey {
            kind,
            guid: row.guid,
        };
        let ordinal = row.ordinal;
        let result = self.load_row(kind, row, inputs, owner);
        if let Err(error) = &result {
            self.refusals.insert(key, error.clone());
        }
        self.record(
            key,
            ordinal,
            result
                .as_ref()
                .err()
                .map_or("complete_or_source_skip", |e| e.id.as_str()),
            0,
        );
        result
    }
    pub fn pre_terrain_data(&self, key: SpawnKey) -> Result<Option<&EffectiveSpawnData>> {
        if let Some(data) = self.pre_terrain.get(&key) {
            return Ok(Some(data));
        }
        if let Some(error) = self.refusals.get(&key) {
            return Err(error.clone());
        }
        Ok(None)
    }
    pub fn complete_data(&self, key: SpawnKey) -> Result<Option<&EffectiveSpawnData>> {
        if let Some(error) = self.refusals.get(&key) {
            return Err(error.clone());
        }
        require(
            !self.pre_terrain.contains_key(&key) || self.completed.contains(&key),
            "execution.effective_spawns.source_skipped_post_insert_owner",
        )?;
        Ok(self.pre_terrain.get(&key))
    }
    /// Only assigned map fields. This cannot supply terrain or grid side effects.
    pub fn pre_terrain_group_definition(
        &self,
        key: SpawnKey,
    ) -> Result<Option<GroupSpawnDefinition>> {
        self.pre_terrain_data(key)?
            .map(|data| {
                let r = &data.row;
                let cell = compute_cell_pair(r.pose[0].to_bits(), r.pose[1].to_bits())
                    .map_err(|_| refuse("execution.effective_spawns.pre_terrain_cell_owner"))?;
                Ok(GroupSpawnDefinition {
                    map: r.map,
                    cell,
                    position_bits: [
                        r.pose[0].to_bits(),
                        r.pose[1].to_bits(),
                        r.pose[2].to_bits(),
                    ],
                    entry: r.entry,
                    spawn_mask: r.mask,
                    game_event: r.event,
                    guid_pool: r.pools[0] as u16,
                    entry_pool: r.pools[1] as u16,
                })
            })
            .transpose()
    }
    fn load_row(
        &mut self,
        kind: GridSpawnKind,
        mut row: ObjectMgrSpawnRow,
        inputs: &EffectiveSpawnInputs,
        owner: &mut impl EffectiveSpawnOwner,
    ) -> Result<()> {
        let creature = kind == GridSpawnKind::Creature;
        let slot = usize::from(!creature);
        let key = SpawnKey {
            kind,
            guid: row.guid,
        };
        let ordinal = row.ordinal;
        let templates = if creature {
            &inputs.creatures
        } else {
            &inputs.gameobjects
        };
        let mut dynamic = false;
        let conditional = if creature && row.entry == 0 {
            inputs.conditional.get(&row.guid)
        } else {
            None
        };
        if row.entry == 0 {
            if let Some(&[alliance, horde]) = conditional {
                self.conditional_rows += 1;
                row.entry = if alliance != 0 { alliance } else { horde };
            } else if let Some((promoted, entries)) = inputs.random.get(&key) {
                self.random_rows[slot] += 1;
                self.draws_needed += 1;
                self.single_entry_draws[slot] += u64::from(entries.len() == 1);
                // ObjectMgr.h:603 indexes the result even for an empty list.
                require(
                    !entries.is_empty(),
                    "execution.effective_spawns.empty_random_list_index_undefined",
                )?;
                require(
                    entries.len() <= i32::MAX as usize,
                    "execution.effective_spawns.random_list_signed_bound_owner",
                )?;
                let index = owner
                    .startup_random()?
                    .creature_irand(0, entries.len() as i32 - 1)?;
                let entry = *entries
                    .get(index as usize)
                    .ok_or_else(|| refuse("execution.effective_spawns.random_index"))?;
                if entry != 0 {
                    row.entry = entry;
                    dynamic = *promoted;
                }
            }
        }
        let info = if row.entry == 0 {
            None
        } else {
            let Some(info) = templates.get(&row.entry) else {
                self.record(
                    key,
                    ordinal,
                    "missing_template",
                    if creature { 1941 } else { 2131 },
                );
                return Ok(());
            };
            if creature && info.totem_ai {
                self.record(key, ordinal, "totem_ai", 1947);
                return Ok(());
            }
            if !creature && info.display != 0 && !inputs.displays.contains(&info.display) {
                self.record(key, ordinal, "invalid_display", 2137);
                return Ok(());
            }
            dynamic |= info.flags & if creature { 0x10_0000 } else { 2 } != 0;
            Some(*info)
        };
        let validation_entry = row.entry;
        // CreatureData::mapid is uint16, unlike GO's uint32, Creature.h:295.
        if creature {
            row.map = u32::from(row.map as u16);
        }
        let data = EffectiveSpawnData {
            row,
            validation_entry,
            spawn_template: inputs.spawn_templates.contains(&0).then_some(0),
            original_zone: None,
            animprogress: 100,
            go_state: -1,
            string_id: 0,
        };
        self.pre_terrain.insert(key, data);
        self.completed.remove(&key);
        let data = self.pre_terrain.get_mut(&key).unwrap();
        if creature {
            self.max_creature_guid = self.max_creature_guid.max(key.guid);
        } else {
            self.max_gameobject_guid = self.max_gameobject_guid.max(key.guid);
        }
        let r = &mut data.row;
        let Some(map_type) = inputs.maps.get(&r.map) else {
            self.record(
                key,
                ordinal,
                "missing_map",
                if creature { 1981 } else { 2173 },
            );
            return Ok(());
        };
        // GridDefines.h:177-194. Creature orientation is deliberately unchecked.
        let limit = f64::from(533.333_3_f32 * 64.0 / 2.0) - 0.5;
        if !r.pose[..3].iter().all(|v| v.is_finite())
            || f64::from(r.pose[0].abs()) > limit
            || f64::from(r.pose[1].abs()) > limit
        {
            self.record(
                key,
                ordinal,
                "invalid_xyz",
                if creature { 1987 } else { 2179 },
            );
            return Ok(());
        }
        let mut logs = [("", 0); 3];
        let mut log_count = 0;
        let mut log = |record| {
            logs[log_count] = record;
            log_count += 1;
        };
        if !creature && r.respawn[0] == 0 && info.is_some_and(|i| i.despawn_at_action) {
            log(("zero_despawn_delay", 2185));
        }
        let inverted = if creature {
            r.respawn[1] < r.respawn[0]
        } else {
            (r.respawn[1] as i32) < (r.respawn[0] as i32)
        };
        if inverted {
            r.respawn[1] = r.respawn[0];
            log(("respawn_max_raised", if creature { 1995 } else { 2192 }));
        }
        if creature {
            if r.radius < 0.0 {
                r.radius = 0.0;
                log(("negative_radius_cleared", 2001));
            } else if r.movement == 1 {
                if r.radius == 0.0 {
                    r.movement = 0;
                    log(("random_to_idle", 2008));
                }
            } else if r.movement == 0 && r.radius != 0.0 {
                r.radius = 0.0;
                log(("idle_radius_cleared", 2016));
            }
            if r.spawn_template > 0 {
                if inputs.spawn_templates.contains(&r.spawn_template) {
                    data.spawn_template = Some(r.spawn_template);
                } else {
                    log(("missing_spawn_template_warning", 2025));
                }
            }
        } else {
            for (i, value) in r.rotation.iter().enumerate() {
                if *value < -1.0 || *value > 1.0 {
                    let line = 2197 + i as u32 * 6;
                    let reason = format!("invalid_rotation{i}");
                    for (reason, line) in logs.into_iter().take(log_count) {
                        self.record(key, ordinal, reason, line);
                    }
                    self.record(key, ordinal, &reason, line);
                    return Ok(());
                }
            }
            self.records
                .extend(logs.into_iter().take(log_count).map(|(reason, line)| {
                    EffectiveSpawnRecord {
                        key,
                        ordinal,
                        reason: reason.into(),
                        line,
                    }
                }));
            log_count = 0;
            // MapManager.cpp:245-248 checks the loaded instance template first.
            let valid_map = if *map_type == 1 || *map_type == 2 {
                inputs
                    .instances
                    .as_ref()
                    .ok_or_else(|| {
                        refuse("execution.effective_spawns.loaded_instance_template_owner")
                    })?
                    .contains(&r.map)
            } else {
                true
            };
            if !valid_map || !r.pose[3].is_finite() {
                for (reason, line) in logs.into_iter().take(log_count) {
                    self.record(key, ordinal, reason, line);
                }
                self.record(key, ordinal, "invalid_map_orientation", 2221);
                return Ok(());
            }
        }
        let map = r.map;
        let pose = [r.pose[0], r.pose[1], r.pose[2]];
        for (reason, line) in logs.into_iter().take(log_count) {
            self.record(key, ordinal, reason, line);
        }
        let zone = if map == 0 || map == 1 {
            owner.original_zone(map, pose)?
        } else {
            0
        };
        let data = self.pre_terrain.get_mut(&key).unwrap();
        data.original_zone = Some(zone as u16);
        let r = &data.row;
        if dynamic && r.event == 0 {
            self.dynamic_guids
                .entry((map, kind))
                .or_default()
                .push(key.guid);
        }
        let ordinary = r.event == 0 && r.pools == [0, 0];
        if ordinary && (creature || !dynamic) {
            if r.mask != 0 {
                let cell = compute_cell_pair(r.pose[0].to_bits(), r.pose[1].to_bits())
                    .map_err(|_| refuse("execution.effective_spawns.cell_conversion_undefined"))?;
                self.static_grids
                    .entry((map, cell.x(), cell.y(), kind))
                    .or_default()
                    .insert(key.guid);
            }
            if owner.autoload_active()? && info.is_some_and(|i| i.flags & 0x1000 != 0) {
                self.active.entry((map, kind)).or_default().push(key.guid);
            }
        }
        if conditional.is_some() {
            self.pre_terrain.get_mut(&key).unwrap().row.entry = 0;
        }
        self.completed.insert(key);
        Ok(())
    }
    /// ObjectMgr.cpp:2252-2282 checks the previous state, not the incoming one.
    pub fn apply_gameobject_addons(
        &mut self,
        rows: &[[u32; 4]],
        strings: &BTreeSet<u32>,
    ) -> Result<()> {
        require(
            !rows.is_empty(),
            "execution.effective_spawns.empty_gameobject_addon_query_dereference_undefined",
        )?;
        for (ordinal, &[guid, anim, state, string]) in rows.iter().enumerate() {
            let key = SpawnKey {
                kind: GridSpawnKind::GameObject,
                guid,
            };
            let Some(data) = self.pre_terrain.get_mut(&key) else {
                continue;
            };
            data.animprogress = anim;
            if data.go_state != -1 && data.go_state >= 3 {
                self.record(key, ordinal as u64, "previous_go_state_invalid", 2269);
                continue;
            }
            let missing = string != 0 && !strings.contains(&string);
            data.string_id = if missing { 0 } else { string };
            data.go_state = state as i32;
            if missing {
                self.record(key, ordinal as u64, "missing_string_cleared", 2276);
            }
        }
        Ok(())
    }
}

#[path = "effective_spawns_catalog.rs"]
mod catalog;
#[cfg(test)]
#[path = "effective_spawns_tests.rs"]
mod tests;
