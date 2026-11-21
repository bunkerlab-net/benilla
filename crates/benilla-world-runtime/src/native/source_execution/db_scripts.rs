// SPDX-License-Identifier: GPL-3.0-or-later
//! ScriptMgr.cpp:108-999 at CMaNGOS Classic 8ec338a1. Immutable, non-admitting definitions.
use super::*;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum DbScriptType {
    QuestEnd,
    QuestStart,
    Spell,
    GameObject,
    GameObjectTemplate,
    Event,
    Gossip,
    CreatureDeath,
    CreatureMovement,
    Relay,
}
impl DbScriptType {
    pub const ALL: [Self; 10] = [
        Self::QuestEnd,
        Self::QuestStart,
        Self::Spell,
        Self::GameObject,
        Self::GameObjectTemplate,
        Self::Event,
        Self::Gossip,
        Self::CreatureDeath,
        Self::CreatureMovement,
        Self::Relay,
    ];
    pub fn table(self) -> &'static str {
        match self {
            Self::QuestEnd => "dbscripts_on_quest_end",
            Self::QuestStart => "dbscripts_on_quest_start",
            Self::Spell => "dbscripts_on_spell",
            Self::GameObject => "dbscripts_on_go_use",
            Self::GameObjectTemplate => "dbscripts_on_go_template_use",
            Self::Event => "dbscripts_on_event",
            Self::Gossip => "dbscripts_on_gossip",
            Self::CreatureDeath => "dbscripts_on_creature_death",
            Self::CreatureMovement => "dbscripts_on_creature_movement",
            Self::Relay => "dbscripts_on_relay",
        }
    }
}
/// Field::GetUInt32/GetInt32/GetFloat conversions, ScriptMgr.cpp:139-159.
/// `floats` contains datafloat, x, y, z, o, speed as binary32 bits.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct DbScriptRow {
    pub ordinal: u64,
    pub id: u32,
    pub delay: u32,
    pub command: u32,
    pub data: [u32; 3],
    pub buddy: u32,
    pub radius_or_guid: u32,
    pub flags: u32,
    pub text: [i32; 4],
    pub floats: [u32; 6],
    pub condition: u32,
}
impl DbScriptRow {
    // ScriptMgr.h:543-579. GO flag only affects these three commands.
    pub(super) fn creature_buddy(&self) -> bool {
        match self.command {
            31 | 36 | 37 => self.flags & 0x400 == 0,
            9 | 11 | 12 | 13 | 27 | 40 | 43 => false,
            _ => true,
        }
    }
    fn additional_flag(&self) -> bool {
        // CAST_CUSTOM_SPELL deliberately absent, ScriptMgr.h:586-608.
        matches!(
            self.command,
            3 | 10 | 13 | 14 | 15 | 16 | 17 | 20 | 23 | 24 | 31 | 34 | 36 | 37 | 39
        )
    }
    fn valid_coordinates(&self) -> bool {
        // GridDefines.h:35-38,177-194. z and o have no magnitude limit.
        let bound = f64::from((533.333_3_f32 * 64.0) / 2.0) - 0.5;
        self.floats[1..=4].iter().enumerate().all(|(i, bits)| {
            let v = f32::from_bits(*bits);
            v.is_finite() && (i >= 2 || f64::from(v.abs()) <= bound)
        })
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum DbScriptLookup {
    Condition,
    Creature,
    GameObject,
    CreatureGuid,
    GameObjectGuid,
    CreaturePool,
    GameObjectPool,
    SpawnGroup,
    StringId,
    Emote,
    Map,
    Spell,
    Sound,
    Item,
    Faction,
    Display,
    TaxiPath,
    MailTemplate,
    WorldStateName,
    Area,
    Quest,
}
impl DbScriptLookup {
    pub(super) fn owner(self) -> &'static str {
        match self {
            Self::Condition => "condition",
            Self::Creature => "creature_template",
            Self::GameObject => "gameobject_template",
            Self::CreatureGuid => "creature_spawn",
            Self::GameObjectGuid => "gameobject_spawn",
            Self::CreaturePool => "creature_pool",
            Self::GameObjectPool => "gameobject_pool",
            Self::SpawnGroup => "spawn_group",
            Self::StringId => "string_id",
            Self::Emote => "emote",
            Self::Map => "map",
            Self::Spell => "spell_template",
            Self::Sound => "sound",
            Self::Item => "item_prototype",
            Self::Faction => "faction_template",
            Self::Display => "creature_display",
            Self::TaxiPath => "taxi_path",
            Self::MailTemplate => "mail_template",
            Self::WorldStateName => "worldstate_name",
            Self::Area => "area",
            Self::Quest => "quest_template",
        }
    }
}
fn missing(owner: &str) -> Refusal {
    refuse(&format!("execution.db_scripts.{owner}_owner"))
}
/// Return values come from the named loaded owner, never expected script outcomes.
/// Quest mutation is immediate, even if a later distance check drops the row.
pub trait DbScriptLoaderOwner {
    fn contains(&mut self, kind: DbScriptLookup, _id: u32) -> Result<bool> {
        Err(missing(kind.owner()))
    }
    fn quest_exploration(&mut self, _id: u32) -> Result<Option<bool>> {
        Err(missing("effective_quest_flags"))
    }
    fn set_quest_exploration(&mut self, _id: u32) -> Result<()> {
        Err(missing("quest_flags_mutation"))
    }
    fn gameobject_entry(&mut self, _guid: u32) -> Result<Option<u32>> {
        Err(missing("gameobject_spawn"))
    }
    fn gameobject_type(&mut self, _entry: u32) -> Result<Option<u32>> {
        Err(missing("gameobject_template"))
    }
    fn max_pool_id(&mut self) -> Result<u32> {
        Err(missing("pool_max_id"))
    }
    /// None means strict DB checking is filtered, or no matching spell exists.
    /// Unbound LogFilter_DbStrictedCheck must refuse, not default to either side.
    fn taxi_spell(&mut self, _path: u32) -> Result<Option<u32>> {
        Err(missing("taxi_spell_and_log_filter"))
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum DbScriptDisposition {
    Loaded,
    Dropped,
    Refused(Refusal),
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DbScriptLog {
    pub ordinal: u64,
    pub id: u32,
    pub command: u32,
    pub source_line: u32,
    pub reason: &'static str,
    pub value: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct DbScriptRowLoad {
    pub ordinal: u64,
    pub id: u32,
    pub disposition: DbScriptDisposition,
    pub logs: Vec<DbScriptLog>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DbScriptRandomRow {
    pub ordinal: u64,
    pub id: u32,
    pub kind: u32,
    pub target: i32,
    pub chance: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DbScriptRandomDrop {
    pub row: DbScriptRandomRow,
    pub source_line: u32,
    pub reason: &'static str,
}
/// ScriptMgr.h:681-682,705,710; vector order is SQL occurrence order.
#[derive(Default, Debug)]
pub struct SourceDbScriptTemplates {
    strings: BTreeMap<u32, Vec<(i32, u32)>>,
    relays: BTreeMap<u32, Vec<(i32, u32)>>,
    dropped: Vec<DbScriptRandomDrop>,
}
impl SourceDbScriptTemplates {
    pub fn load(rows: impl IntoIterator<Item = DbScriptRandomRow>) -> Result<Self> {
        let mut result = Self::default();
        for row in rows {
            // :1239-1244 converts uint32 to int32, then only checks the upper bound.
            // Negative type indexes m_scriptTemplates before its beginning.
            require(
                (row.kind as i32) >= 0,
                "execution.db_scripts.random_template_index_undefined",
            )?;
            let map = match row.kind {
                0 => &mut result.strings,
                1 => &mut result.relays,
                _ => {
                    result.dropped.push(DbScriptRandomDrop {
                        row,
                        source_line: 1251,
                        reason: "random_template_type_invalid",
                    });
                    continue;
                }
            };
            map.entry(row.id)
                .or_default()
                .push((row.target, row.chance));
        }
        Ok(result)
    }
    /// Raw random-template operands, not LoadDbScriptStrings' loaded text owner.
    pub fn string(&self, id: u32) -> Option<&[(i32, u32)]> {
        self.strings.get(&id).map(Vec::as_slice)
    }
    pub fn relay(&self, id: u32) -> Option<&[(i32, u32)]> {
        self.relays.get(&id).map(Vec::as_slice)
    }
    pub fn dropped(&self) -> &[DbScriptRandomDrop] {
        &self.dropped
    }
}
/// ScriptMgr.h:653-655 uses map<uint32 id,multimap<uint32 delay,shared_ptr<ScriptInfo>>>.
/// IDs and delays ascend; equal delays retain SQL `ORDER BY priority` occurrence order.
/// This retained F2 cursor is not an F1 equal-priority execution certificate.
#[derive(Debug)]
pub struct LoadedDbScriptMap {
    kind: DbScriptType,
    scripts: BTreeMap<u32, Vec<DbScriptRow>>,
    rows: Vec<DbScriptRowLoad>,
}
impl LoadedDbScriptMap {
    pub fn kind(&self) -> DbScriptType {
        self.kind
    }
    pub fn scripts(&self) -> &BTreeMap<u32, Vec<DbScriptRow>> {
        &self.scripts
    }
    pub fn rows(&self) -> &[DbScriptRowLoad] {
        &self.rows
    }
    pub fn loaded_count(&self) -> usize {
        self.scripts.values().map(Vec::len).sum()
    }
    pub fn load(
        kind: DbScriptType,
        rows: impl IntoIterator<Item = DbScriptRow>,
        templates: &SourceDbScriptTemplates,
        relay: Option<&Self>,
        owner: &mut impl DbScriptLoaderOwner,
    ) -> Result<Self> {
        Self::collect(kind, rows, templates, relay, owner, true)?.into_loaded()
    }
    /// Diagnostic census continues after refusals, but cannot publish a partial loaded map.
    pub fn inspect(
        kind: DbScriptType,
        rows: impl IntoIterator<Item = DbScriptRow>,
        templates: &SourceDbScriptTemplates,
        relay: Option<&Self>,
        owner: &mut impl DbScriptLoaderOwner,
    ) -> DbScriptLoadReport {
        Self::collect(kind, rows, templates, relay, owner, false)
            .expect("diagnostic collection records refusals")
    }
    fn collect(
        kind: DbScriptType,
        rows: impl IntoIterator<Item = DbScriptRow>,
        templates: &SourceDbScriptTemplates,
        relay: Option<&Self>,
        owner: &mut impl DbScriptLoaderOwner,
        fail_fast: bool,
    ) -> Result<DbScriptLoadReport> {
        let mut map = Self {
            kind,
            scripts: BTreeMap::new(),
            rows: Vec::new(),
        };
        for row in rows {
            let mut logs = Vec::new();
            let outcome = validate(&row, kind, templates, relay, owner, &mut logs);
            let ordinal = row.ordinal;
            let id = row.id;
            let disposition = match outcome {
                Ok(true) => {
                    map.scripts.entry(id).or_default().push(row);
                    DbScriptDisposition::Loaded
                }
                Ok(false) => DbScriptDisposition::Dropped,
                Err(e) if fail_fast => return Err(e),
                Err(e) => DbScriptDisposition::Refused(e),
            };
            map.rows.push(DbScriptRowLoad {
                ordinal,
                id,
                disposition,
                logs,
            });
        }
        // Stable sort reproduces multimap::emplace insertion order for equivalent keys.
        for steps in map.scripts.values_mut() {
            steps.sort_by_key(|r| r.delay);
        }
        Ok(DbScriptLoadReport { map })
    }
    /// :1118-1143,1265-1271 logs missing nested relay IDs without removing rows.
    pub fn check_relays(&self, templates: &SourceDbScriptTemplates) -> Result<Vec<DbScriptLog>> {
        require(
            self.kind == DbScriptType::Relay,
            "execution.db_scripts.relay_map_type",
        )?;
        let mut logs = Vec::new();
        for row in self
            .scripts
            .values()
            .flatten()
            .filter(|r| r.command == 45 && r.data[0] != 0)
        {
            if !self.scripts.contains_key(&row.data[0]) {
                log(row, &mut logs, 1131, "nested_relay_missing", row.data[0]);
            }
        }
        for (&id, values) in &templates.relays {
            for &(target, _) in values {
                if target != 0 && !self.scripts.contains_key(&(target as u32)) {
                    logs.push(DbScriptLog {
                        ordinal: 0,
                        id,
                        command: 45,
                        source_line: 1271,
                        reason: "template_relay_missing",
                        value: target as u32,
                    });
                }
            }
        }
        Ok(logs)
    }
}
#[derive(Debug)]
pub struct DbScriptLoadReport {
    map: LoadedDbScriptMap,
}
impl DbScriptLoadReport {
    pub fn rows(&self) -> &[DbScriptRowLoad] {
        &self.map.rows
    }
    pub fn validated_count(&self) -> usize {
        self.map.loaded_count()
    }
    pub fn into_loaded(self) -> Result<LoadedDbScriptMap> {
        if let Some(e) = self.map.rows.iter().find_map(|r| match &r.disposition {
            DbScriptDisposition::Refused(e) => Some(e),
            _ => None,
        }) {
            return Err(e.clone());
        }
        // :122-129 returns before :995 publishes a map. Reload needs its prior owner.
        require(
            !self.map.rows.is_empty(),
            "execution.db_scripts.empty_query_previous_map_owner",
        )?;
        Ok(self.map)
    }
}
fn log(
    row: &DbScriptRow,
    logs: &mut Vec<DbScriptLog>,
    source_line: u32,
    reason: &'static str,
    value: u32,
) {
    logs.push(DbScriptLog {
        ordinal: row.ordinal,
        id: row.id,
        command: row.command,
        source_line,
        reason,
        value,
    });
}
fn validate(
    row: &DbScriptRow,
    kind: DbScriptType,
    templates: &SourceDbScriptTemplates,
    relay: Option<&LoadedDbScriptMap>,
    owner: &mut impl DbScriptLoaderOwner,
    logs: &mut Vec<DbScriptLog>,
) -> Result<bool> {
    use DbScriptLookup::*;
    let [a, b, c] = row.data;
    let f = row.flags;
    macro_rules! drop_if {
        ($condition:expr, $line:expr, $reason:expr, $value:expr) => {
            if $condition {
                log(row, logs, $line, $reason, $value);
                return Ok(false);
            }
        };
    }
    macro_rules! absent {
        ($domain:expr, $id:expr, $line:expr, $reason:expr) => {
            drop_if!(!owner.contains($domain, $id)?, $line, $reason, $id);
        };
    }
    if row.condition != 0 {
        absent!(Condition, row.condition, 163, "condition_missing");
    }
    let creature = row.creature_buddy();
    if row.buddy != 0 && f & (0x10 | 0x100 | 0x800) == 0 {
        absent!(
            if creature { Creature } else { GameObject },
            row.buddy,
            if creature { 172 } else { 177 },
            "buddy_template_missing"
        );
        drop_if!(row.radius_or_guid == 0, 182, "buddy_radius_zero", row.buddy);
    }
    if f != 0 {
        drop_if!(f & !0xfff != 0, 191, "flags_invalid", f);
        drop_if!(
            f & 8 != 0 && !row.additional_flag(),
            196,
            "additional_flag_invalid",
            f
        );
        drop_if!(
            f & 1 != 0 && f & (0x80 | 0x100 | 0x800 | 0x10 | 0x400) == 0 && row.buddy == 0,
            201,
            "buddy_required",
            f
        );
        if f & 0x10 != 0 {
            absent!(
                if creature {
                    CreatureGuid
                } else {
                    GameObjectGuid
                },
                row.radius_or_guid,
                if creature { 211 } else { 220 },
                "buddy_guid_missing"
            );
        } else if f & 0x80 != 0 {
            // PoolManager.h:186-187 narrows to uint16, then indexes unchecked.
            // Both vectors have max_pool_id+1 slots, PoolManager.cpp:653,789.
            let pool = u32::from(row.radius_or_guid as u16);
            require(
                pool <= owner.max_pool_id()?,
                "execution.db_scripts.buddy_pool_index_undefined",
            )?;
            drop_if!(
                !owner.contains(
                    if creature {
                        CreaturePool
                    } else {
                        GameObjectPool
                    },
                    pool
                )?,
                if creature { 232 } else { 241 },
                "buddy_pool_empty",
                row.radius_or_guid
            );
        } else if f & 0x100 != 0 {
            absent!(SpawnGroup, row.buddy, 251, "buddy_group_missing");
        } else if f & 0x800 != 0 && row.buddy != 0 {
            absent!(StringId, row.buddy, 260, "buddy_string_missing");
        }
    }
    match row.command {
        0 => {
            drop_if!(a == 0 && row.text[0] == 0, 274, "talk_text_zero", 0);
        }
        1 => {
            absent!(Emote, a, 284, "emote_missing");
            for &id in &row.text {
                if id != 0 && !owner.contains(Emote, id as u32)? {
                    log(row, logs, 291, "optional_emote_missing_retained", id as u32);
                }
            }
        }
        2..=5 | 13 | 18 | 21 | 25 | 26 | 32 | 33 | 35 | 36 | 39..=41 | 43 | 49 | 50 | 52 => {}
        6 => {
            absent!(Map, a, 305, "map_missing");
            drop_if!(
                !row.valid_coordinates(),
                311,
                "teleport_coordinates_invalid",
                0
            );
        }
        7 => {
            let quest = owner.quest_exploration(a)?;
            drop_if!(quest.is_none(), 321, "quest_missing", a);
            if quest == Some(false) {
                owner.set_quest_exploration(a)?;
                log(row, logs, 327, "quest_exploration_flag_set", a);
            }
            // The duplicate >100 check at :342 cannot be reached after :335.
            drop_if!(b as f32 > 100.0, 337, "quest_distance_too_large", b);
            drop_if!(
                b != 0 && (b as f32) < 5.0,
                351,
                "quest_distance_too_small",
                b
            );
        }
        8 => {
            if a != 0 {
                absent!(Creature, a, 362, "kill_credit_creature_missing");
            }
        }
        9 | 11 | 12 => {
            let entry = if a == 0 {
                drop_if!(
                    row.buddy == 0,
                    if row.command == 9 { 374 } else { 429 },
                    "gameobject_and_buddy_missing",
                    0
                );
                row.buddy
            } else {
                let entry = owner.gameobject_entry(a)?;
                drop_if!(
                    entry.is_none(),
                    if row.command == 9 { 384 } else { 439 },
                    "gameobject_guid_missing",
                    a
                );
                entry.unwrap()
            };
            let go_type = owner.gameobject_type(entry)?;
            drop_if!(
                go_type.is_none(),
                if row.command == 9 { 393 } else { 448 },
                "gameobject_template_missing",
                entry
            );
            let go_type = go_type.unwrap();
            drop_if!(
                if row.command == 9 {
                    matches!(go_type, 0 | 17 | 25)
                } else {
                    !matches!(go_type, 0 | 1)
                },
                if row.command == 9 { 401 } else { 454 },
                "gameobject_type_unsupported",
                go_type
            );
        }
        10 => {
            drop_if!(
                !row.valid_coordinates(),
                410,
                "summon_coordinates_invalid",
                0
            );
            absent!(Creature, a, 416, "summon_creature_missing");
        }
        14 => {
            absent!(Spell, a, 466, "remove_aura_spell_missing");
            drop_if!(b > 2, 472, "remove_aura_mode_invalid", b);
        }
        15 => {
            absent!(Spell, a, 482, "cast_spell_missing");
            let mut errored = false;
            for &id in &row.text {
                if id != 0 && !owner.contains(Spell, id as u32)? {
                    log(row, logs, 491, "cast_alternative_missing", id as u32);
                    errored = true;
                }
            }
            if errored {
                return Ok(false);
            }
        }
        16 => {
            absent!(Sound, a, 504, "sound_missing");
            if b & !31 != 0 {
                log(row, logs, 510, "sound_unknown_flags_retained", b);
            }
            if b & 3 != 0 && b & 28 != 0 {
                log(row, logs, 512, "sound_mixed_flags_retained", b);
            }
        }
        17 => {
            absent!(Item, a, 519, "item_missing");
            drop_if!(b == 0, 525, "item_amount_zero", b);
        }
        19 => {
            drop_if!(true, 538, "movie_unsupported", a);
        }
        20 => {
            drop_if!(a >= 5 && a != 15 && a != 18, 546, "movement_invalid", a);
            drop_if!(
                !(0..=3).contains(&row.text[0]),
                553,
                "forced_movement_invalid",
                row.text[0] as u32
            );
        }
        22 => {
            if a != 0 {
                absent!(Faction, a, 566, "faction_missing");
            }
        }
        23 | 24 => {
            if a != 0 {
                absent!(
                    if f & 8 != 0 { Display } else { Creature },
                    a,
                    match (row.command, f & 8 != 0) {
                        (23, true) => 578,
                        (23, false) => 586,
                        (24, true) => 599,
                        _ => 607,
                    },
                    "morph_or_mount_missing"
                );
            }
        }
        27 => {
            drop_if!(
                a == 0 || a >= 16 || a & 3 == 3 || a & 12 == 12,
                628,
                "lock_state_invalid",
                a
            );
        }
        28 => {
            drop_if!(a >= 10, 637, "stand_state_invalid", a);
        }
        29 | 48 => {
            if b > 2 {
                log(
                    row,
                    logs,
                    if row.command == 29 { 645 } else { 825 },
                    "flag_change_invalid_retained",
                    b,
                );
            }
        }
        30 => {
            absent!(TaxiPath, a, 652, "taxi_path_missing");
            if let Some(spell) = owner.taxi_spell(a)? {
                drop_if!(true, 674, "taxi_spell_exists", spell);
            }
        }
        31 => {
            if a != 0 {
                absent!(
                    if creature { Creature } else { GameObject },
                    a,
                    if creature { 686 } else { 694 },
                    "terminate_buddy_missing"
                );
            }
            drop_if!(
                c != 0 && c > owner.max_pool_id()?,
                700,
                "terminate_pool_invalid",
                c
            );
        }
        34 => {
            absent!(Condition, a, 713, "terminate_condition_missing");
            if b != 0 {
                absent!(Quest, b, 718, "terminate_quest_missing");
            }
        }
        37 => {
            drop_if!(a < b, 731, "dynamic_distance_reversed", b);
        }
        38 => {
            absent!(MailTemplate, a, 740, "mail_template_missing");
            if b != 0 {
                absent!(Creature, b, 745, "mail_sender_missing");
            }
        }
        42 => {
            drop_if!(
                row.text[..3].iter().any(|i| *i < 0),
                758,
                "equipment_negative",
                0
            );
        }
        44 => {
            absent!(Creature, a, 769, "update_template_missing");
        }
        45 => {
            if kind != DbScriptType::Relay && a != 0 {
                let relay = relay.ok_or_else(|| missing("loaded_relay_map"))?;
                drop_if!(!relay.scripts.contains_key(&a), 783, "relay_missing", a);
            }
            drop_if!(
                b != 0 && templates.relay(b).is_none(),
                792,
                "relay_template_missing",
                b
            );
        }
        46 => {
            absent!(Spell, a, 802, "custom_spell_missing");
            drop_if!(row.text[..3] == [0, 0, 0], 808, "custom_basepoints_zero", 0);
        }
        47 => {
            drop_if!(a >= 4, 817, "current_spell_type_invalid", a);
        }
        51 => match a {
            // Signed text compared to uint32 enum, SpawnGroupDefines.h:128-138.
            150 => {
                drop_if!(
                    row.text[0] as u32 >= 7,
                    841,
                    "formation_shape_invalid",
                    row.text[0] as u32
                );
            }
            151 => {
                absent!(SpawnGroup, b, 852, "formation_group_missing");
            }
            100 => {
                drop_if!(b >= 7, 883, "formation_shape_invalid", b);
            }
            101 => {
                let spread = f32::from_bits(row.floats[1]);
                // :891 deliberately accepts NaN through ordered comparisons.
                #[allow(clippy::manual_range_contains)]
                let invalid = spread < 0.5 || spread > 15.0;
                drop_if!(invalid, 893, "formation_spread_invalid", row.floats[1]);
            }
            102 => {}
            _ => {
                drop_if!(true, 905, "formation_command_unknown", a);
            }
        },
        53 => {
            absent!(
                WorldStateName,
                row.text[0] as u32,
                926,
                "worldstate_name_missing"
            );
        }
        54 => {
            drop_if!(a > 2, 935, "sheathe_invalid", a);
        }
        55 => {
            absent!(StringId, a, 944, "string_id_missing");
            drop_if!(b > 1, 949, "string_apply_invalid", b);
        }
        58 => {
            absent!(SpawnGroup, a, 959, "spawn_group_missing");
        }
        200 => {
            absent!(Area, a, 977, "area_missing");
        }
        _ => {
            drop_if!(true, 969, "command_unknown", row.command);
        }
    }
    Ok(true)
}

/// Immutable ScriptMgr products. No row may cross a refused dependency.
#[derive(Debug)]
pub struct SourceDbScriptMaps {
    maps: BTreeMap<DbScriptType, LoadedDbScriptMap>,
    templates: SourceDbScriptTemplates,
    relay_logs: Vec<DbScriptLog>,
}
impl SourceDbScriptMaps {
    pub fn load(
        mut tables: Vec<(DbScriptType, Vec<DbScriptRow>)>,
        templates: SourceDbScriptTemplates,
        owner: &mut impl DbScriptLoaderOwner,
    ) -> Result<Self> {
        // World.cpp:1217-1226. Quest mutations also observe this table order.
        let startup = [
            DbScriptType::Relay,
            DbScriptType::Gossip,
            DbScriptType::QuestStart,
            DbScriptType::QuestEnd,
            DbScriptType::Spell,
            DbScriptType::GameObject,
            DbScriptType::GameObjectTemplate,
            DbScriptType::Event,
            DbScriptType::CreatureDeath,
            DbScriptType::CreatureMovement,
        ];
        tables.sort_by_key(|(kind, _)| startup.iter().position(|k| k == kind).unwrap());
        let mut result = Self {
            maps: BTreeMap::new(),
            templates,
            relay_logs: Vec::new(),
        };
        for (kind, rows) in tables {
            require(
                !result.maps.contains_key(&kind),
                "execution.db_scripts.duplicate_table_input",
            )?;
            let map = LoadedDbScriptMap::load(
                kind,
                rows,
                &result.templates,
                result.maps.get(&DbScriptType::Relay),
                owner,
            )?;
            if kind == DbScriptType::Relay {
                result.relay_logs = map.check_relays(&result.templates)?;
            }
            result.maps.insert(kind, map);
        }
        Ok(result)
    }
    pub fn map(&self, kind: DbScriptType) -> Result<&LoadedDbScriptMap> {
        self.maps.get(&kind).ok_or_else(|| missing(kind.table()))
    }
    pub fn templates(&self) -> &SourceDbScriptTemplates {
        &self.templates
    }
    pub fn relay_logs(&self) -> &[DbScriptLog] {
        &self.relay_logs
    }
    /// Exact CheckDbscript input, projected only from complete validated maps.
    pub fn waypoint_maps(&self) -> Result<WaypointScriptMaps> {
        let movement = self.map(DbScriptType::CreatureMovement)?;
        let relay = self.map(DbScriptType::Relay)?;
        Ok(WaypointScriptMaps {
            movement: movement
                .scripts
                .iter()
                .map(|(&id, rows)| {
                    (
                        id,
                        rows.iter()
                            .map(|row| WaypointScriptStep {
                                delay: row.delay,
                                command: row.command,
                                despawn_delay: row.data[0],
                                relay: row.data[0],
                                relay_template: row.data[1],
                            })
                            .collect(),
                    )
                })
                .collect(),
            relay: relay.scripts.keys().copied().collect(),
            relay_templates: std::cell::RefCell::new(
                self.templates
                    .relays
                    .iter()
                    .map(|(&id, rows)| {
                        (id, rows.iter().map(|&(target, _)| target as u32).collect())
                    })
                    .collect(),
            ),
        })
    }
    /// Version-one immutable inventory, not F7 live script continuation.
    pub fn inventory(&self) -> serde_json::Value {
        serde_json::json!({"version":1,"tables":self.maps.iter().map(|(kind,map)|
            (kind.table(),map.loaded_count())).collect::<BTreeMap<_,_>>(),
            "live_restore":"execution.db_scripts.f7_script_scheduler_owner"})
    }
    pub fn validate(&self) -> Result<()> {
        for map in self.maps.values() {
            for (&id, steps) in &map.scripts {
                require(
                    steps.iter().all(|r| r.id == id)
                        && steps.windows(2).all(|s| s[0].delay <= s[1].delay),
                    "execution.db_scripts.loaded_map_order",
                )?;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "db_scripts_tests.rs"]
mod tests;
