// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
// ObjectMgr.cpp:1758-2070; Creature.cpp:1613-1746,1864-1959; Creature.h:806.
//! Static ordinary spawn loader projection. No source authentication, cold-load
//! order, geometry provider or assistance traversal is manufactured here.
use super::captured::{float, shape, uint};
use super::*;
use benilla_sim::creature_motion as motion;

pub(super) const EXCLUSION_TABLES: &[&str] = &[
    "creature_spawn_entry",
    "creature_conditional_spawn",
    "spawn_group_spawn",
    "creature_linking",
    "creature_linking_template",
    "game_event_creature",
    "game_event_creature_data",
    "pool_creature",
    "pool_creature_template",
];
const SPAWN_FIELDS: &[&str] = &[
    "guid",
    "id",
    "map",
    "position_x",
    "position_y",
    "position_z",
    "orientation",
    "spawntimesecsmin",
    "spawntimesecsmax",
    "spawndist",
    "MovementType",
    "spawnMask",
];
const TEMPLATE_FIELDS: &[&str] = &[
    "Entry",
    "NpcFlags",
    "UnitFlags",
    "Faction",
    "ModelId",
    "EquipmentId",
    "CurHealth",
    "CurMana",
    "SpawnFlags",
    "RelayId",
    "StringId",
    "Name",
];

#[derive(Clone, Debug, Serialize)]
pub struct CapturedSpawnContext {
    pub spawns: init::Rows<CreatureCapturedRow>,
    pub spawn_data: init::Rows<CreatureCapturedRow>,
    pub spawn_templates: init::Rows<CreatureCapturedRow>,
    /// Complete queries for all selected GUIDs and entries, including both ends
    /// of linking relationships. Any present row is outside this selected domain.
    pub exclusions: BTreeMap<String, init::Rows<CreatureCapturedRow>>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SelectedSpawn {
    pub guid: u64,
    pub entry: u32,
    pub position: ai::Point,
    pub orientation: Orientation,
    pub default_movement: motion::DefaultMovement,
    pub respawn_seconds: [u32; 2],
    spawn_template: init::SpawnTemplate,
}
#[derive(Clone, Debug, Serialize)]
pub struct CompiledSpawnContext {
    map: u32,
    spawns: BTreeMap<u64, SelectedSpawn>,
}
fn unsupported(reason: &'static str) -> NativeError {
    NativeError::UnsupportedMechanic(reason)
}
fn signed(row: &CreatureCapturedRow, field: &str) -> Result<i64, NativeError> {
    row.get(field)
        .and_then(serde_json::Value::as_i64)
        .ok_or_else(|| unsupported("captured spawn signed field"))
}
fn spawn_template(raw: &CreatureCapturedRow) -> Result<init::SpawnTemplate, NativeError> {
    shape(raw, TEMPLATE_FIELDS)?;
    if raw
        .get("Name")
        .and_then(serde_json::Value::as_str)
        .is_none_or(|name| name.len() > 4096)
    {
        return Err(unsupported("captured spawn template name"));
    }
    Ok(init::SpawnTemplate {
        entry: uint(raw, "Entry")?,
        model_id: uint(raw, "ModelId")?,
        equipment_id: i32::try_from(signed(raw, "EquipmentId")?)
            .map_err(|_| unsupported("spawn equipment domain"))?,
        npc_flags: i32::try_from(signed(raw, "NpcFlags")?)
            .map_err(|_| unsupported("spawn NPC flags domain"))?,
        unit_flags: signed(raw, "UnitFlags")?,
        faction: uint(raw, "Faction")?,
        current_health: uint(raw, "CurHealth")?,
        current_mana: uint(raw, "CurMana")?,
        spawn_flags: uint(raw, "SpawnFlags")?,
        relay_id: uint(raw, "RelayId")?,
        string_id: uint(raw, "StringId")?,
    })
}
impl CapturedSpawnContext {
    pub fn compile(
        &self,
        map: u32,
        selected_guids: &[u64],
    ) -> Result<CompiledSpawnContext, NativeError> {
        let spawns = rows(&self.spawns)?;
        let bindings = rows(&self.spawn_data)?;
        let templates = rows(&self.spawn_templates)?;
        if selected_guids.len() > MAX_SOURCES
            || spawns.len() != selected_guids.len()
            || bindings.len() > MAX_SOURCES
            || templates.len() > MAX_SOURCES + 1
        {
            return Err(NativeError::TooLarge);
        }
        bounded_encoded_size(self, MAX_SOURCE_BYTES)?;
        let selected: BTreeSet<_> = selected_guids.iter().copied().collect();
        if selected.len() != selected_guids.len()
            || selected
                .iter()
                .any(|&id| id <= SOLO_UNIT || id > u64::from(u32::MAX))
        {
            return Err(unsupported("captured selected spawn GUIDs"));
        }
        if self.exclusions.len() != EXCLUSION_TABLES.len() {
            return Err(NativeError::AuthorityUnavailable(
                "complete spawn group/link/event/pool/alternative queries",
            ));
        }
        for &table in EXCLUSION_TABLES {
            let captured = self
                .exclusions
                .get(table)
                .ok_or(NativeError::AuthorityUnavailable(table))?;
            let values = rows(captured)?;
            if !values.is_empty() {
                return Err(unsupported(table));
            }
        }
        let mut template_map = BTreeMap::new();
        for raw in templates {
            let value = spawn_template(raw)?;
            if template_map.insert(value.entry, value).is_some() {
                return Err(unsupported("duplicate spawn template identity"));
            }
        }
        let mut binding_map = BTreeMap::new();
        for raw in bindings {
            shape(raw, &["Guid", "Id"])?;
            let guid = u64::from(uint(raw, "Guid")?);
            if !selected.contains(&guid) || binding_map.insert(guid, uint(raw, "Id")?).is_some() {
                return Err(unsupported("spawn template binding coverage"));
            }
        }
        let mut result = BTreeMap::new();
        for raw in spawns {
            shape(raw, SPAWN_FIELDS)?;
            let guid = u64::from(uint(raw, "guid")?);
            let entry = uint(raw, "id")?;
            if !selected.contains(&guid)
                || entry == 0
                || entry > 0xff_ffff
                || uint(raw, "map")? != map
                || uint(raw, "spawnMask")? != 1
            {
                return Err(unsupported("ordinary static map spawn identity"));
            }
            let point = |field| -> Result<ai::Real, NativeError> {
                Ok(ai::Real::from_bits(float(raw, field)?.bits())?)
            };
            let position = ai::Point {
                x: point("position_x")?,
                y: point("position_y")?,
                z: point("position_z")?,
            };
            // The pin's MapDefines limit is SIZE_OF_GRIDS * MAX_NUMBER_OF_GRIDS / 2 minus 0.5.
            if [position.x, position.y, position.z]
                .iter()
                .any(|v| v.as_f32().abs() > 17066.166)
            {
                return Err(unsupported("spawn map coordinate domain"));
            }
            let orientation = Orientation::from_bits(float(raw, "orientation")?.bits())?;
            let minimum = uint(raw, "spawntimesecsmin")?;
            let maximum = uint(raw, "spawntimesecsmax")?.max(minimum);
            if maximum > MAX_RESPAWN_SECONDS {
                return Err(unsupported("spawn respawn timer overflow"));
            }
            let radius = point("spawndist")?;
            let movement = uint(raw, "MovementType")?;
            let default_movement = match movement {
                0 => motion::DefaultMovement::Idle,
                1 if radius.as_f32() < 0.0 => {
                    return Err(unsupported("negative random spawn radius repair"))
                }
                1 if radius.as_f32() == 0.0 => motion::DefaultMovement::Idle,
                1 => motion::DefaultMovement::Wander {
                    center: position,
                    radius,
                },
                _ => return Err(unsupported("spawn waypoint or external movement")),
            };
            default_movement.validate()?;
            let template_id = binding_map.get(&guid).copied().unwrap_or(0);
            let spawn_template = *template_map
                .get(&template_id)
                .ok_or(unsupported("captured effective spawn template absent"))?;
            if result
                .insert(
                    guid,
                    SelectedSpawn {
                        guid,
                        entry,
                        position,
                        orientation,
                        default_movement,
                        respawn_seconds: [minimum, maximum],
                        spawn_template,
                    },
                )
                .is_some()
            {
                return Err(unsupported("duplicate captured spawn GUID"));
            }
        }
        Ok(CompiledSpawnContext {
            map,
            spawns: result,
        })
    }
}
impl CompiledSpawnContext {
    pub fn spawn(&self, guid: u64) -> Option<&SelectedSpawn> {
        self.spawns.get(&guid)
    }
    /// Reuse the generator's override validation and per-life RNG/death timers.
    pub fn bind_generation(
        &self,
        mut source: CreatureGenerationSource,
    ) -> Result<CreatureGenerationSource, NativeError> {
        let row = self
            .spawn(source.spawn_id)
            .ok_or(NativeError::AuthorityBinding)?;
        if source.spawn_entry != row.entry || source.input.template.entry != row.entry {
            return Err(NativeError::AuthorityBinding);
        }
        source.respawn_delay_min_seconds = row.respawn_seconds[0];
        source.respawn_delay_max_seconds = row.respawn_seconds[1];
        source.requested_spawn_template_entry = row.spawn_template.entry;
        source.input.spawn_template = init::Rows::Complete(vec![row.spawn_template]);
        source.validate()?;
        Ok(source)
    }
    /// Populate only source-owned spawn coordinates and default movement. Other
    /// motion, Home, geometry and assistance owners remain independently required.
    pub fn bind_motion(
        &self,
        map: u32,
        npc: &mut NpcProfile,
        motion: &mut NpcMotionGenesis,
    ) -> Result<(), NativeError> {
        let row = self
            .spawn(npc.ai.creature_id)
            .ok_or(NativeError::AuthorityBinding)?;
        if self.map != map || npc.entry != row.entry {
            return Err(NativeError::AuthorityBinding);
        }
        npc.ai.spawn = row.position;
        motion.npc_spawn_orientation = row.orientation;
        motion.default_movement = row.default_movement;
        Ok(())
    }
}
