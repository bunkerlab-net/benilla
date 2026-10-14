// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Source GPL-2.0-or-later; this bounded translation GPL-3.0-or-later.
//! Transactional GiveXP/GiveLevel for explicit solo mage rows, levels 1–60 with unspent talents.
//! See PLAYER-PROGRESSION.md. The audited host is mandatory, not a no-op adapter.
pub mod derived;
pub mod kill_xp;
mod numeric;
mod skills;
mod talents;

use crate::starter_init::{Attributes, ModifierSnapshot, Rows, SkillLineRow, SkillRaceClassRow};
pub use numeric::NumericState;
use sha2::{Digest, Sha256};
pub use skills::{SkillDependency, SkillState, SkillStatus};
use std::collections::{BTreeMap, BTreeSet};

pub const POLICY_ID: &str = "cmangos-8ec338a1-solo-mage-progression-unspent-talents-softfloat-v2";
pub const MAX_SUPPORTED_LEVEL: u32 = 60;
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Missing { table: &'static str, id: u32 },
    Unsupported(&'static str),
    Invalid(&'static str),
    Numeric(&'static str),
    Host(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for Error {}

/// Explicit authoritative rules. No captured-server configuration is inferred.
/// `global_skill_max` is World::GetConfigMaxSkillValue's resolved uint16 result.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub max_player_level: u32,
    pub always_max_skill_for_level: bool,
    pub global_skill_max: u16,
    /// Actual resolved World Rate.Talent binary32 bits, including valid zero.
    pub talent_rate_bits: u32,
}
impl Rules {
    /// Source World::GetConfigMaxSkillValue and World.cpp's MaxPlayerLevel bound.
    /// The inputs must be the session's actual rules, not assumed realm defaults.
    pub fn from_classic_config(
        max_player_level: u32,
        always_max_skill_for_level: bool,
        talent_rate_bits: u32,
    ) -> Result<Self> {
        if !(1..=100).contains(&max_player_level) {
            return Err(Error::Invalid("MaxPlayerLevel"));
        }
        talents::validate_rate(talent_rate_bits)?;
        let cap = max_player_level;
        let global_skill_max = if cap > 60 {
            300 + ((cap - 60) * 75) / 10
        } else {
            cap * 5
        };
        Ok(Self {
            max_player_level,
            always_max_skill_for_level,
            global_skill_max: global_skill_max as u16,
            talent_rate_bits,
        })
    }
    /// Source CalculateTalentsPoints with used talent count proven zero.
    /// Numeric rejection is transactional, never a saturating C++ cast guess.
    pub fn unspent_talent_points(self, level: u32) -> Result<u32> {
        talents::points(level, self.talent_rate_bits)
    }
}
/// Selected actual `player_levelstats` row; no gap-fill or extrapolation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LevelStatsRow {
    pub race: u8,
    pub class: u8,
    pub level: u32,
    pub attributes: Attributes,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClassLevelStatsRow {
    pub class: u8,
    pub level: u32,
    pub base_health: u32,
    pub base_mana: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Resolved GetXPForLevel value. Below cap this requires the actual positive SQL
/// row; at the configured cap the getter returns zero because its vector ends.
pub struct XpRow {
    pub level: u32,
    pub xp_for_next_level: u32,
}

/// A dependency scan is mandatory. False/default booleans cannot mean unscanned.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Evidence {
    Unknown,
    Absent,
    Present,
}
/// Live-state admission, rechecked by the host before each transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Audit {
    pub group_or_raid: Evidence,
    pub pet_or_controlled_unit: Evidence,
    pub used_talents: Evidence,
    pub shapeshift: Evidence,
    pub level_change_scripts: Evidence,
    pub dynamic_or_level_scaling_modifiers: Evidence,
    pub resource_or_stat_aura_hooks: Evidence,
    pub unsupported_item_effects: Evidence,
    pub non_mana_power_modifiers: Evidence,
    pub extra_spell_crit_or_regen_modifiers: Evidence,
}
impl Audit {
    fn validate(self) -> Result<()> {
        for (name, evidence) in [
            ("group or raid", self.group_or_raid),
            ("pet or controlled unit", self.pet_or_controlled_unit),
            ("used talents", self.used_talents),
            ("shapeshift", self.shapeshift),
            ("level-change scripts", self.level_change_scripts),
            (
                "dynamic or level-scaling modifiers",
                self.dynamic_or_level_scaling_modifiers,
            ),
            (
                "resource or stat aura hooks",
                self.resource_or_stat_aura_hooks,
            ),
            ("unsupported item effects", self.unsupported_item_effects),
            ("non-mana power modifiers", self.non_mana_power_modifiers),
            (
                "additional crit or regeneration modifiers",
                self.extra_spell_crit_or_regen_modifiers,
            ),
        ] {
            if evidence != Evidence::Absent {
                return Err(Error::Unsupported(name));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct LevelRows {
    attributes: Attributes,
    health: u32,
    mana: u32,
    next_xp: u32,
}
/// Loader-resolved SQL rows and complete DBC lookup results, never bundled rows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    race: u8,
    class: u8,
    rules: Rules,
    levels: BTreeMap<u32, LevelRows>,
    skills: BTreeMap<u16, SkillDependency>,
    identity: [u8; 32],
}
impl Catalog {
    pub fn new(
        race: u8,
        class: u8,
        rules: Rules,
        level_stats: Vec<LevelStatsRow>,
        class_stats: Vec<ClassLevelStatsRow>,
        xp_rows: Vec<XpRow>,
        skill_dependencies: Vec<SkillDependency>,
    ) -> Result<Self> {
        // Actual race/class legality remains the compiler's dependency, not an ID guess.
        if !(1..=8).contains(&race) || class != 8 {
            return Err(Error::Unsupported("solo mage identity"));
        }
        if rules
            != Rules::from_classic_config(
                rules.max_player_level,
                rules.always_max_skill_for_level,
                rules.talent_rate_bits,
            )?
        {
            return Err(Error::Invalid("progression rules"));
        }
        if level_stats.is_empty() || level_stats.len() > MAX_SUPPORTED_LEVEL as usize {
            return Err(Error::Invalid("bounded level rows"));
        }
        let mut levels = BTreeMap::new();
        let mut classes = BTreeMap::new();
        let mut thresholds = BTreeMap::new();
        for r in class_stats {
            if r.class != class
                || !(1..=MAX_SUPPORTED_LEVEL).contains(&r.level)
                || r.level > rules.max_player_level
                || r.base_health == 0
                || r.base_health > u16::MAX as u32
                || r.base_mana == 0
                || r.base_mana > u16::MAX as u32
                || classes.insert(r.level, r).is_some()
            {
                return Err(Error::Invalid("player_classlevelstats row"));
            }
        }
        for r in xp_rows {
            if !(1..=MAX_SUPPORTED_LEVEL).contains(&r.level)
                || r.level > rules.max_player_level
                || (r.level < rules.max_player_level && r.xp_for_next_level == 0)
                || (r.level >= rules.max_player_level && r.xp_for_next_level != 0)
                || thresholds.insert(r.level, r.xp_for_next_level).is_some()
            {
                return Err(Error::Invalid("player_xp_for_level row"));
            }
        }
        for r in level_stats {
            rules.unspent_talent_points(r.level)?;
            if r.race != race
                || r.class != class
                || !(1..=MAX_SUPPORTED_LEVEL).contains(&r.level)
                || r.level > rules.max_player_level
                || r.attributes.0.iter().any(|v| !(0..=255).contains(v))
                // ObjectMgr treats zero strength as an absent row, then fails
                // level one or gap-fills. This actual-row contract does neither.
                || r.attributes.0[0] == 0
            {
                return Err(Error::Invalid("player_levelstats row"));
            }
            let c = classes.remove(&r.level).ok_or(Error::Missing {
                table: "player_classlevelstats",
                id: r.level,
            })?;
            let next_xp = thresholds.remove(&r.level).ok_or(Error::Missing {
                table: "player_xp_for_level",
                id: r.level,
            })?;
            if levels
                .insert(
                    r.level,
                    LevelRows {
                        attributes: r.attributes,
                        health: c.base_health,
                        mana: c.base_mana,
                        next_xp,
                    },
                )
                .is_some()
            {
                return Err(Error::Invalid("duplicate player_levelstats row"));
            }
        }
        if !classes.is_empty() || !thresholds.is_empty() {
            return Err(Error::Invalid("unmatched level rows"));
        }
        let mut skills = BTreeMap::new();
        if skill_dependencies.len() > 127 {
            return Err(Error::Invalid("skill dependency count"));
        }
        for mut dep in skill_dependencies {
            dep.validate()?;
            if let Rows::Complete(rows) = &mut dep.race_class {
                rows.sort_by_key(|r| r.row_id);
            }
            if skills.insert(dep.skill, dep).is_some() {
                return Err(Error::Invalid("duplicate skill dependency"));
            }
        }
        let mut out = Self {
            race,
            class,
            rules,
            levels,
            skills,
            identity: [0; 32],
        };
        out.identity = out.hash();
        Ok(out)
    }
    fn hash(&self) -> [u8; 32] {
        let mut h = Sha256::new();
        h.update(POLICY_ID.as_bytes());
        h.update(talents::POLICY_ID.as_bytes());
        h.update([self.race, self.class]);
        h.update(self.rules.max_player_level.to_le_bytes());
        h.update([u8::from(self.rules.always_max_skill_for_level)]);
        h.update(self.rules.global_skill_max.to_le_bytes());
        h.update(self.rules.talent_rate_bits.to_le_bytes());
        h.update((self.levels.len() as u32).to_le_bytes());
        for (level, row) in &self.levels {
            h.update(level.to_le_bytes());
            for v in row.attributes.0 {
                h.update(v.to_le_bytes());
            }
            for v in [row.health, row.mana, row.next_xp] {
                h.update(v.to_le_bytes());
            }
        }
        h.update((self.skills.len() as u32).to_le_bytes());
        for (id, dep) in &self.skills {
            h.update(id.to_le_bytes());
            match dep.line {
                Some(line) => {
                    h.update([1]);
                    h.update(line.category.to_le_bytes());
                }
                None => h.update([0]),
            }
            let Rows::Complete(rows) = &dep.race_class else {
                unreachable!("validated dependency")
            };
            h.update((rows.len() as u32).to_le_bytes());
            for row in rows {
                for v in [
                    row.row_id,
                    row.race_mask,
                    row.class_mask,
                    row.flags,
                    row.tier,
                ] {
                    h.update(v.to_le_bytes());
                }
            }
        }
        h.finalize().into()
    }
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
    pub fn race(&self) -> u8 {
        self.race
    }
    pub fn class(&self) -> u8 {
        self.class
    }
    pub fn rules(&self) -> Rules {
        self.rules
    }
    /// Actual supplied rows, in level order; no gap-fill or extrapolation.
    pub fn supplied_levels(&self) -> impl Iterator<Item = u32> + '_ {
        self.levels.keys().copied()
    }
    fn level(&self, level: u32) -> Result<LevelRows> {
        if level > self.rules.max_player_level {
            return Err(Error::Unsupported("level above configured row-loader cap"));
        }
        if !(1..=MAX_SUPPORTED_LEVEL).contains(&level) {
            return Err(Error::Unsupported("mage levels outside 1–60"));
        }
        self.levels.get(&level).copied().ok_or(Error::Missing {
            table: "level progression rows",
            id: level,
        })
    }
}

/// Checkpoint DTO. Bind to the catalog, modifier provenance, and host-policy hash
/// in the owner's authenticated atomic envelope; this is not a save-file format.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub policy: String,
    pub catalog_identity: [u8; 32],
    pub race: u8,
    pub class: u8,
    pub level: u32,
    pub alive: bool,
    pub xp: u32,
    pub next_level_xp: u32,
    pub played_time_level: u32,
    pub session_level: u32,
    pub free_talent_points: u32,
    pub used_talent_points: u32,
    pub modifiers: ModifierSnapshot,
    pub numeric: NumericState,
    /// Exact host-established traversal order of mSkillStatus. Never sorted here.
    pub skills: Vec<SkillState>,
    pub audit: Audit,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct State(Snapshot);
impl State {
    pub fn restore(catalog: &Catalog, snapshot: Snapshot) -> Result<Self> {
        snapshot.audit.validate()?;
        let row = catalog.level(snapshot.level)?;
        if snapshot.policy != POLICY_ID
            || snapshot.catalog_identity != catalog.identity
            || snapshot.race != catalog.race
            || snapshot.class != catalog.class
            || snapshot.session_level != snapshot.level
            || snapshot.next_level_xp != row.next_xp
            || snapshot.modifiers.create != row.attributes
            || snapshot.modifiers.create_health != row.health
            || snapshot.modifiers.create_mana != row.mana
            || snapshot.used_talent_points != 0
            || snapshot.free_talent_points != catalog.rules.unspent_talent_points(snapshot.level)?
        {
            return Err(Error::Invalid("progression checkpoint identity/base rows"));
        }
        snapshot
            .numeric
            .validate(snapshot.level, &snapshot.modifiers, snapshot.alive)?;
        let mut ids = BTreeSet::new();
        let mut positions = BTreeSet::new();
        if snapshot.skills.len() > 127 {
            return Err(Error::Invalid("skill count"));
        }
        for s in &snapshot.skills {
            if s.id == 0
                || !ids.insert(s.id)
                || s.position >= 127
                || (s.status != SkillStatus::Deleted
                    && (!positions.insert(s.position) || s.value == 0 || s.maximum == 0))
            {
                return Err(Error::Invalid("live skill state"));
            }
            if s.status != SkillStatus::Deleted && !catalog.skills.contains_key(&s.id) {
                return Err(Error::Missing {
                    table: "complete skill dependency",
                    id: s.id.into(),
                });
            }
        }
        Ok(Self(snapshot))
    }
    pub fn snapshot(&self) -> Snapshot {
        self.0.clone()
    }
    pub fn view(&self) -> &Snapshot {
        &self.0
    }
}
/// The named unresolved native hooks must execute against a deep clone. The host
/// MUST prove the full SkillLineAbility/SpellLearnSkill/spell-chain closure and
/// reject any operation that changes this module's fixed modifiers/skill set.
/// It owns derived attack/crit/defense/damage/hit fields and spell knowledge.
/// No method may publish packets, persist, share interior-mutable state with the
/// original, or recursively run quest callbacks. Events are already applied.
pub trait Host: Clone {
    fn preflight(&self, catalog: &Catalog, state: &Snapshot) -> Result<()>;
    fn apply(&mut self, event: &Event, view: &Snapshot) -> Result<()>;
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum StatHook {
    AttackPowerAndDamage { ranged: bool },
    AllCritPercentages,
    AllSpellCritChances,
    DefenseBonuses,
    SpellDamageBonus,
    ManaRegen,
    WeaponHitChance { attack: u8 },
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Event {
    /// Victim GUID zero, given XP, type byte 1; no kill-only payload/rested bonus.
    NonKillXpLog {
        xp: u32,
    },
    /// Native wire uint32 payload: level, HP delta, five power deltas, five stats.
    LevelUpPacket {
        words: [u32; 12],
    },
    NextLevelXp(u32),
    PlayedTimeReset,
    Level(u32),
    SkillValue {
        id: u16,
        value: u16,
        maximum: u16,
    },
    SkillTrainedSpells {
        id: u16,
        value: u16,
        step: u16,
    },
    SkillDefenseBonuses,
    CreateStat {
        index: u8,
        value: i32,
    },
    CreateHealth(u32),
    CreateMana(u32),
    InitTalentForLevel,
    Stat {
        index: u8,
        value: i32,
    },
    Hook(StatHook),
    Armor(i32),
    Resistance {
        school: u8,
        value: i32,
    },
    MaxHealth(u32),
    MaxPower {
        power: u8,
        value: u32,
    },
    Health(u32),
    Power {
        power: u8,
        value: u32,
    },
    SessionLevel(u32),
    Xp(u32),
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub event: Event,
    pub view: Snapshot,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Transaction<H> {
    pub state: State,
    pub host: H,
    pub events: Vec<Observation>,
}
struct Staged<'a, H> {
    catalog: &'a Catalog,
    state: State,
    host: H,
    events: Vec<Observation>,
}
impl<H: Host> Staged<'_, H> {
    fn emit(&mut self, event: Event) -> Result<()> {
        self.host.apply(&event, &self.state.0)?;
        self.events.push(Observation {
            event,
            view: self.state.snapshot(),
        });
        Ok(())
    }
    fn level(&mut self, level: u32) -> Result<()> {
        if level == self.state.0.level {
            return Ok(());
        }
        if level < self.state.0.level && self.state.0.level >= 10 {
            return Err(Error::Unsupported("decreasing post-ten mage progression"));
        }
        let row = self.catalog.level(level)?;
        let old = &self.state.0.modifiers;
        // GetCreateStat is float; a negative float-to-u32 packet conversion is undefined C++.
        if row
            .attributes
            .0
            .iter()
            .zip(old.create.0)
            .any(|(a, b)| *a < b)
        {
            return Err(Error::Numeric("negative float level-up stat delta"));
        }
        let mut words = [0; 12];
        words[0] = level;
        words[1] = row.health.wrapping_sub(old.create_health);
        words[2] = row.mana.wrapping_sub(old.create_mana);
        for i in 0..5 {
            words[i + 7] = (row.attributes.0[i] - old.create.0[i]) as u32;
        }
        self.emit(Event::LevelUpPacket { words })?;
        self.state.0.next_level_xp = row.next_xp;
        self.emit(Event::NextLevelXp(row.next_xp))?;
        self.state.0.played_time_level = 0;
        self.emit(Event::PlayedTimeReset)?;
        self.state.0.level = level;
        self.emit(Event::Level(level))?;
        self.skills(false)?;
        for i in 0..5 {
            self.state.0.modifiers.create.0[i] = row.attributes.0[i];
            self.emit(Event::CreateStat {
                index: i as u8,
                value: row.attributes.0[i],
            })?;
        }
        self.state.0.modifiers.create_health = row.health;
        self.emit(Event::CreateHealth(row.health))?;
        self.state.0.modifiers.create_mana = row.mana;
        self.emit(Event::CreateMana(row.mana))?;
        // Source UpdateFreeTalentPoints is a no-op below ten when no talents
        // are used. Monotonic progression makes the reachable free count zero.
        if level >= 10 {
            self.state.0.free_talent_points = self.catalog.rules.unspent_talent_points(level)?;
        }
        self.emit(Event::InitTalentForLevel)?;
        self.update_all_stats()?;
        if self.state.0.alive {
            self.set_health(self.state.0.numeric.max_health)?;
        }
        self.set_power(0, self.state.0.numeric.max_powers[0])?;
        self.set_power(3, self.state.0.numeric.max_powers[3])?;
        if self.state.0.numeric.powers[1] > self.state.0.numeric.max_powers[1] {
            self.set_power(1, self.state.0.numeric.max_powers[1])?;
        }
        // Pet synchronization is absent only under the rechecked admission audit.
        self.state.0.session_level = level;
        self.emit(Event::SessionLevel(level))
    }
}
fn stage<'a, H: Host>(catalog: &'a Catalog, state: &State, host: &H) -> Result<Staged<'a, H>> {
    // Revalidate complete identity, rows and values at every external boundary.
    State::restore(catalog, state.snapshot())?;
    host.preflight(catalog, &state.0)?;
    Ok(Staged {
        catalog,
        state: state.clone(),
        host: host.clone(),
        events: vec![],
    })
}
/// Quest/non-kill GiveXP(xp,nullptr). Kill/rested/group-rate paths are excluded.
/// Source uint32 addition wraps BEFORE threshold comparison, even for huge grants.
/// At cap the final remainder may exceed next XP; it is not clamped or discarded.
pub fn give_xp<H: Host>(
    catalog: &Catalog,
    state: &State,
    host: &H,
    xp: u32,
) -> Result<Transaction<H>> {
    let mut s = stage(catalog, state, host)?;
    if xp != 0 && s.state.0.alive && s.state.0.level < catalog.rules.max_player_level {
        s.emit(Event::NonKillXpLog { xp })?;
        let mut new_xp = s.state.0.xp.wrapping_add(xp);
        while new_xp >= s.state.0.next_level_xp && s.state.0.level < catalog.rules.max_player_level
        {
            new_xp -= s.state.0.next_level_xp;
            s.level(s.state.0.level + 1)?;
        }
        s.state.0.xp = new_xp;
        s.emit(Event::Xp(new_xp))?;
    }
    Ok(Transaction {
        state: s.state,
        host: s.host,
        events: s.events,
    })
}
/// Direct GiveLevel keeps XP unchanged. Post-ten lowering/reset is excluded;
/// source does not itself impose the GiveXP cap, but this bounded row-loader
/// contract rejects levels above that cap. Equal level is a source no-op.
/// Lower create attributes are rejected: the source packet would cast a negative
/// float to uint32, outside its defined C++ conversion domain.
pub fn give_level<H: Host>(
    catalog: &Catalog,
    state: &State,
    host: &H,
    level: u32,
) -> Result<Transaction<H>> {
    let mut s = stage(catalog, state, host)?;
    s.level(level)?;
    Ok(Transaction {
        state: s.state,
        host: s.host,
        events: s.events,
    })
}

/// Direct source skill-cap update, including the explicit maximize override.
/// It still requires the same audited full native skill/spell host closure.
pub fn update_skills_for_level<H: Host>(
    catalog: &Catalog,
    state: &State,
    host: &H,
    maximize: bool,
) -> Result<Transaction<H>> {
    let mut s = stage(catalog, state, host)?;
    s.skills(maximize)?;
    Ok(Transaction {
        state: s.state,
        host: s.host,
        events: s.events,
    })
}
