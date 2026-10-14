// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! Source-derived bounded level-one mage initialization from explicit source rows.
//! No bundled game rows, guessed race profiles, or neutral missing dependencies.
//! This initializes state; it does not certify the caller's capture completeness.
//! See STARTER-INIT.md for row closure, source ordering, and excluded semantics.
mod equipment;
mod items;
mod skills;
mod stats;
mod training;
use crate::mage_resource::{CurrentSkill, CurrentSkills};
pub use equipment::{EquipmentItemModifiers, EquipmentRecompute, StatRecipe, StatRecipeSnapshot};
pub use items::{
    EquippedItem, GrantSource, InventoryStack, ItemDamage, ItemGrant, ItemRow, ItemSpell, ItemStat,
};
pub use skills::{
    first_entry, first_entry_skills, DefaultSkill, FirstEntryState, InitializedSkill,
    SkillAbilityRow, SkillLineRow, SkillRaceClassRow, SkillTierRow,
};
pub use stats::{Attributes, ModifierSnapshot, Resources};
use std::collections::{BTreeMap, BTreeSet};
pub use training::{
    LiveTraining, TrainingCallSnapshot, TrainingSnapshot, TrainingView, LIVE_TRAINING_POLICY_ID,
};
pub const STARTER_INIT_POLICY_ID: &str = "cmangos-8ec338a1-level1-mage-init-softfloat-v2";
pub type Result<T> = std::result::Result<T, InitError>;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitError {
    Missing {
        table: &'static str,
        id: u32,
    },
    Unsupported {
        domain: &'static str,
        id: u32,
        detail: u32,
    },
    Invalid(&'static str),
    Numeric(&'static str),
}
impl std::fmt::Display for InitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for InitError {}
fn unsupported(domain: &'static str, id: u32, detail: u32) -> InitError {
    InitError::Unsupported { domain, id, detail }
}
fn missing(table: &'static str, id: u32) -> InitError {
    InitError::Missing { table, id }
}
/// Every queried source relationship must be captured, including an empty result.
/// Missing is intentionally distinct from a proven empty vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rows<T> {
    Missing,
    Complete(Vec<T>),
}
impl<T> Rows<T> {
    fn require(&self, table: &'static str, id: u32) -> Result<&[T]> {
        match self {
            Self::Complete(v) => Ok(v),
            Self::Missing => Err(missing(table, id)),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LearnSpell {
    pub spell_id: u32,
    pub active: bool,
    pub auto_learned: bool,
}
/// Canonical server-authoritative SpellEntry fields at native 173-word indices.
/// All numeric spell_template overrides MUST overlay client fields before use.
/// Text/visual fields are retained but not interpreted. No identity is inferred.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpellRow {
    pub id: u32,
    pub words: [u32; 173],
    pub duration_ms: Option<i32>,
    pub previous_rank: Option<u32>,
    pub talent: bool,
    pub learned_spells: Rows<LearnSpell>,
    /// True if any script/condition/custom proc/affect hook can change initial casting.
    pub external_initial_hooks: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub enum AbilityAvailability {
    Available,
    Unavailable(String),
    PassiveApplied,
    PassiveDormant,
    SkillStep,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct InitialAura {
    pub spell_id: u32,
    pub effect_index: u8,
    pub aura_type: u32,
    pub misc_value: i32,
    pub amount: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnownAbility {
    pub spell_id: u32,
    pub active: bool,
    pub availability: AbilityAvailability,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ActiveAbilitySupport {
    pub spell_id: u32,
    pub unavailable_reason: Option<String>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InitEvent {
    InitStatsForLevel,
    LoadSavedSkills,
    TrainLoadedSkill {
        skill: u16,
        value: u16,
    },
    FillInitialResources(Resources),
    DefaultSkills,
    DefaultSpells,
    SkillSet {
        skill: u16,
        value: u16,
        max: u16,
        step: u16,
    },
    SpellLearned {
        spell: u32,
    },
    InitialCast {
        spell: u32,
    },
    EffectApplied {
        spell: u32,
        index: u8,
        effect: u32,
        aura: u32,
    },
    Stats {
        attributes: Attributes,
        resources: Resources,
        armor: i32,
    },
    Equip {
        item: u32,
        slot: u8,
    },
    Store {
        item: u32,
        count: u32,
    },
    SecondInventoryPass,
    EnableStatModification,
    UpdateAllStats,
    RestoreCreatedResources(Resources),
}
/// A fresh non-GM, not-in-world, unshapeshifted mage. No persisted character input.
#[derive(Clone, Debug)]
pub struct StarterInput {
    pub race: u8,
    pub class: u8,
    pub gender: u8,
    pub level: u8,
    pub class_power_type: u32,
    pub create_attributes: Attributes,
    pub create_health: u32,
    pub create_mana: u32,
    pub default_skills: Rows<DefaultSkill>,
    pub default_spells: Rows<u32>,
    pub skill_lines: Vec<SkillLineRow>,
    pub skill_race_class: Rows<SkillRaceClassRow>,
    pub skill_tiers: Vec<SkillTierRow>,
    /// Complete union of all rows for every reachable skill AND spell.
    pub skill_abilities: Rows<SkillAbilityRow>,
    pub spells: Vec<SpellRow>,
    pub active_support: Vec<ActiveAbilitySupport>,
    /// CharStartOutfit order, then playercreateinfo_item source order.
    pub item_grants: Rows<ItemGrant>,
    pub items: Vec<ItemRow>,
    /// Captured hooks must explicitly be absent for the admitted item subset.
    pub item_creation_hooks: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InitializedMage {
    pub create_attributes: Attributes,
    pub effective_attributes: Attributes,
    pub create_health: u32,
    pub create_mana: u32,
    pub resources: Resources,
    pub resistances: [i32; 7],
    pub skills: BTreeMap<u16, InitializedSkill>,
    pub abilities: BTreeMap<u32, KnownAbility>,
    /// Kept even when a skill aura currently targets an unlearned skill.
    pub initial_auras: Vec<InitialAura>,
    pub stat_recipe: StatRecipe,
    pub equipped: BTreeMap<u8, EquippedItem>,
    pub inventory: Vec<InventoryStack>,
    pub weapon_proficiency: u32,
    pub armor_proficiency: u32,
    pub can_dodge: bool,
    pub can_parry: bool,
    pub can_block: bool,
    pub trace: Vec<InitEvent>,
}
impl InitializedMage {
    /// No admitted initial aura changes crit or regeneration directly; only the
    /// proven effective attributes feed the existing source-derived mage kernels.
    pub fn mana_regeneration(&self) -> Result<crate::mage_resource::ManaRegen> {
        crate::mage_resource::update_mana_regen(
            self.effective_attributes.0[4] as u32,
            crate::mage_resource::ManaRegenModifiers {
                spirit_multiplier: crate::classic_spell::SpellFloat::from_bits(0x3f800000)
                    .map_err(|_| InitError::Numeric("one"))?,
                flat_mana_per_five: 0,
                interrupted_percent: 0,
            },
        )
        .map_err(|_| InitError::Numeric("initialized mage regeneration"))
    }
    pub fn spell_crit_percent(&self) -> Result<crate::classic_spell::SpellFloat> {
        crate::mage_resource::mage_spell_crit(
            1,
            self.effective_attributes.0[3] as u32,
            crate::mage_resource::SpellCritModifiers {
                all_schools: 0,
                school: 0,
            },
        )
        .map_err(|_| InitError::Numeric("initialized mage spell crit"))
    }
    pub fn current_skills(&self) -> Result<CurrentSkills> {
        CurrentSkills::new(
            self.skills
                .values()
                .map(|s| CurrentSkill {
                    id: s.id,
                    value: s.value,
                    permanent_bonus: s.permanent_bonus,
                    temporary_bonus: s.temporary_bonus,
                    deleted: false,
                })
                .collect(),
        )
        .map_err(|_| InitError::Invalid("current skills"))
    }
}
struct Init<'a> {
    input: &'a StarterInput,
    stats: stats::Stats,
    skills: BTreeMap<u16, InitializedSkill>,
    known: BTreeMap<u32, KnownAbility>,
    trace: Vec<InitEvent>,
    /// Post-order addSpell successes, projected only by the in-world caller.
    /// Creation/login keep their insertion trace and send no learned packets.
    learned_spells: Vec<u32>,
    skill_auras: Vec<(u16, i16, bool)>,
    applied_auras: BTreeSet<(u32, i32)>,
    initial_auras: Vec<InitialAura>,
    weapon_proficiency: u32,
    armor_proficiency: u32,
    can_parry: bool,
    can_block: bool,
    budget: usize,
}
fn unique<T>(rows: &[T], id: impl Fn(&T) -> u32) -> Result<()> {
    let mut seen = BTreeSet::new();
    for row in rows {
        if !seen.insert(id(row)) {
            return Err(InitError::Invalid("duplicate row ID"));
        }
    }
    Ok(())
}
/// Atomically derives a fresh state or returns the exact missing/unsupported row.
/// Active ability availability is explicitly supplied by a separate gameplay compiler.
pub fn initialize_mage(input: &StarterInput) -> Result<InitializedMage> {
    if input.class != 8
        || input.class_power_type != 0
        || input.level != 1
        || !(1..=8).contains(&input.race)
        || input.gender > 1
    {
        return Err(InitError::Invalid(
            "level-one mage identity / ChrClasses power",
        ));
    }
    unique(&input.spells, |r| r.id)?;
    unique(&input.skill_lines, |r| u32::from(r.id))?;
    unique(&input.skill_tiers, |r| r.id)?;
    unique(&input.items, |r| r.id)?;
    unique(&input.active_support, |r| r.spell_id)?;
    if input.active_support.iter().any(|s| {
        s.unavailable_reason
            .as_ref()
            .is_some_and(|r| r.trim().is_empty())
    }) {
        return Err(InitError::Invalid("empty active-unavailable reason"));
    }
    unique(
        input.skill_race_class.require("SkillRaceClassInfo", 0)?,
        |r| r.row_id,
    )?;
    unique(input.skill_abilities.require("SkillLineAbility", 0)?, |r| {
        r.row_id
    })?;
    let mut s = Init {
        input,
        stats: stats::Stats::new(
            input.create_attributes,
            input.create_health,
            input.create_mana,
        )?,
        skills: BTreeMap::new(),
        known: BTreeMap::new(),
        trace: vec![InitEvent::InitStatsForLevel],
        learned_spells: vec![],
        skill_auras: vec![],
        applied_auras: BTreeSet::new(),
        initial_auras: vec![],
        weapon_proficiency: 0,
        armor_proficiency: 0,
        can_parry: false,
        can_block: false,
        budget: 4096,
    };
    s.trace
        .push(InitEvent::FillInitialResources(s.stats.resources));
    s.trace.push(InitEvent::DefaultSkills);
    for row in input.default_skills.require("playercreateinfo_skills", 0)? {
        s.default_skill(*row)?;
    }
    s.trace.push(InitEvent::DefaultSpells);
    for &id in input.default_spells.require("playercreateinfo_spell", 0)? {
        s.learn(id, true)?;
    }
    let (equipped, inventory) = s.items()?;
    s.trace.push(InitEvent::SecondInventoryPass);
    // For the admitted no-offhand/no-special-bag subset, second pass is a no-op.
    s.trace.push(InitEvent::EnableStatModification);
    s.stats.update()?;
    s.trace.push(InitEvent::UpdateAllStats);
    s.trace
        .push(InitEvent::RestoreCreatedResources(s.stats.resources));
    s.stat_trace();
    let stat_recipe = StatRecipe::new(&s.stats, &equipped, &input.items)?;
    Ok(InitializedMage {
        create_attributes: s.stats.create,
        effective_attributes: s.stats.effective,
        create_health: s.stats.create_health,
        create_mana: s.stats.create_mana,
        resources: s.stats.resources,
        resistances: s.stats.resistances,
        skills: s.skills,
        abilities: s.known,
        initial_auras: s.initial_auras,
        stat_recipe,
        equipped,
        inventory,
        weapon_proficiency: s.weapon_proficiency,
        armor_proficiency: s.armor_proficiency,
        can_dodge: true,
        can_parry: s.can_parry,
        can_block: s.can_block,
        trace: s.trace,
    })
}
impl Init<'_> {
    fn step(&mut self) -> Result<()> {
        self.budget = self
            .budget
            .checked_sub(1)
            .ok_or(InitError::Invalid("initial dependency work bound"))?;
        Ok(())
    }
    fn stat_trace(&mut self) {
        self.trace.push(InitEvent::Stats {
            attributes: self.stats.effective,
            resources: self.stats.resources,
            armor: self.stats.resistances[0],
        });
    }
    fn spell(&self, id: u32) -> Result<&SpellRow> {
        self.input
            .spells
            .iter()
            .find(|s| s.id == id)
            .ok_or(missing("spell_template", id))
    }
    fn learn(&mut self, id: u32, active: bool) -> Result<()> {
        self.step()?;
        if let Some(known) = self.known.get(&id) {
            if known.active != active {
                return Err(unsupported("initial spell active-state transition", id, 0));
            }
            return Ok(());
        }
        let row = self.spell(id)?.clone();
        let w = &row.words;
        if id == 0 || w[0] != id {
            return Err(InitError::Invalid("SpellEntry ID"));
        }
        if row.talent || row.previous_rank.is_some() {
            return Err(unsupported("initial spell rank/talent", id, 0));
        }
        if let Some(link) = self
            .input
            .skill_abilities
            .require("SkillLineAbility", id)?
            .iter()
            .find(|link| link.spell == id && link.forward_spell_id != 0)
        {
            return Err(unsupported(
                "initial spellbook successor",
                id,
                link.forward_spell_id,
            ));
        }
        if row.external_initial_hooks {
            return Err(unsupported("initial spell external hook", id, 0));
        }
        let passive = w[6] & 0x40 != 0;
        let cast = w[7] & 0x80000000 != 0
            || (passive && (w[11] == 0 || w[8] & 0x80000 != 0) && w[16] == 0);
        let skill_step = w[61..64].contains(&44);
        if passive && !cast {
            return Err(unsupported("conditional initial passive", id, w[16]));
        }
        let availability = if cast {
            AbilityAvailability::PassiveApplied
        } else if skill_step {
            AbilityAvailability::SkillStep
        } else if passive {
            AbilityAvailability::PassiveDormant
        } else {
            match self.input.active_support.iter().find(|a| a.spell_id == id) {
                Some(a) => match &a.unavailable_reason {
                    None => AbilityAvailability::Available,
                    Some(reason) => AbilityAvailability::Unavailable(reason.clone()),
                },
                None => AbilityAvailability::Unavailable(
                    "active ability has no admitted executor".into(),
                ),
            }
        };
        self.known.insert(
            id,
            KnownAbility {
                spell_id: id,
                active,
                availability,
            },
        );
        self.trace.push(InitEvent::SpellLearned { spell: id });
        if cast || skill_step {
            self.cast_initial(&row)?;
        }
        // Source SKILL_STEP branch returns before UpdateSpellTrainedSkills/dependencies.
        if !cast && skill_step {
            return Ok(());
        }
        self.spell_trained_skills(&row)?;
        for link in row
            .learned_spells
            .require("spell_learn_spell closure", id)?
        {
            if !link.auto_learned {
                self.learn(link.spell_id, link.active)?;
            }
        }
        // Player::learnSpell sends SMSG_LEARNED_SPELL only after addSpell
        // returns true. Inactive addSpell calls still notify active descendants.
        if active {
            self.learned_spells.push(id);
        }
        Ok(())
    }
    fn cast_initial(&mut self, row: &SpellRow) -> Result<()> {
        self.step()?;
        let w = &row.words;
        let id = row.id;
        // Explicitly admitted passive flags; all other cast constraints stay closed.
        let allow0 = 0x40 | 0x80 | 0x10 | 0x100 | 0x80000000;
        let allow1 = 0x80000000 | 0x400 | 0x10000000;
        if w[6] & !allow0 != 0
            || w[7] & !allow1 != 0
            || w[8] & !0x80000 != 0
            || w[9] != 0
            || w[10] != 0
        {
            return Err(unsupported("initial spell attributes", id, 0));
        }
        if w[12] != 0
            || w[14] != 0
            || w[15] != 0
            || w[16] != 0
            || w[17] != 0
            || w[24] != 0
            || w[26] != 0
            || w[39] != 0
            || w[160] != 0
            || w[161] != 0
            || w[162] != 0
        {
            return Err(unsupported("initial spell condition/proc/family", id, 0));
        }
        if w[32..36].iter().any(|v| *v != 0) || w[156] != 0 || w[40..58].iter().any(|v| *v != 0) {
            return Err(unsupported("initial spell cost/reagent", id, 0));
        }
        self.trace.push(InitEvent::InitialCast { spell: id });
        for i in 0..3 {
            let effect = w[61 + i];
            if effect == 0 {
                continue;
            }
            if w[82 + i] != 1
                || w[85 + i] != 0
                || w[88 + i] != 0
                || w[100 + i] > 1
                || w[103 + i] != 0
            {
                return Err(unsupported("initial effect target", id, i as u32));
            }
            let amount = stats::constant_effect_amount(w[76 + i] as i32, w[67 + i])?;
            if !matches!(w[64 + i] as i32, 0 | 1)
                || w[70 + i] != 0
                || w[73 + i] != 0
                || w[112 + i] != 0
            {
                return Err(unsupported("initial effect random/scaling", id, i as u32));
            }
            let misc = w[106 + i] as i32;
            let aura = w[91 + i];
            match effect {
                // Pinned EffectEmpty/EffectSkill bodies, not guessed inert IDs.
                20 | 21 | 25 | 26 | 37 | 39 | 118 => {}
                22 => self.can_parry = true,
                23 => self.can_block = true,
                36 => {
                    if w[109 + i] == 0 {
                        return Err(missing("learned effect spell", id));
                    }
                    self.learn(w[109 + i], true)?;
                }
                44 => {
                    if amount < 0 {
                        return Err(unsupported("negative skill step", id, i as u32));
                    }
                    self.set_skill_step(misc as u16, amount as u16)?;
                }
                60 => match w[58] as i32 {
                    // Source EffectProficiency grants the full mask only when
                    // none of its bits are already known, not the missing bits.
                    2 => {
                        if self.weapon_proficiency & w[59] == 0 {
                            self.weapon_proficiency |= w[59];
                        }
                    }
                    4 => {
                        if self.armor_proficiency & w[59] == 0 {
                            self.armor_proficiency |= w[59];
                        }
                    }
                    _ => return Err(unsupported("proficiency item class", id, w[58])),
                },
                6 => {
                    // SpellAuraHolder constructor marks passive index-zero
                    // auras permanent even though GetSpellDuration returns 0.
                    // Do not extend this to active auras or any other zero row.
                    let permanent = row.duration_ms == Some(-1)
                        || (row.duration_ms == Some(0) && w[30] == 0 && w[6] & 0x40 != 0);
                    if !permanent || w[94 + i] != 0 || w[109 + i] != 0 || w[58] as i32 != -1 {
                        return Err(unsupported(
                            "initial aura duration/trigger/equipment",
                            id,
                            i as u32,
                        ));
                    }
                    if !self.applied_auras.insert((aura, misc)) {
                        return Err(unsupported("initial aura stacking", id, aura));
                    }
                    self.initial_auras.push(InitialAura {
                        spell_id: id,
                        effect_index: i as u8,
                        aura_type: aura,
                        misc_value: misc,
                        amount,
                    });
                    match aura {
                        29 | 80 | 137 => {
                            let min = if aura == 29 { -2 } else { -1 };
                            if !(min..=4).contains(&misc) {
                                return Err(unsupported("stat aura misc", id, misc as u32));
                            }
                            let before = self.stats.resources;
                            for stat in 0..5 {
                                if misc < 0 || misc == stat as i32 {
                                    self.stats.attribute(
                                        stat,
                                        match aura {
                                            29 => 2,
                                            80 => 1,
                                            _ => 3,
                                        },
                                        amount,
                                    )?;
                                }
                            }
                            if aura == 137
                                && misc == 2
                                && w[6] & 0x10 != 0
                                && before.max_health > 0
                                && self.stats.resources.health > 0
                            {
                                self.stats.resources.health = self
                                    .stats
                                    .resources
                                    .max_health
                                    .checked_mul(before.health)
                                    .ok_or(InitError::Numeric("stamina aura HP product"))?
                                    / before.max_health;
                                self.stats.resources.health = self
                                    .stats
                                    .resources
                                    .health
                                    .max(1)
                                    .min(self.stats.resources.max_health);
                            }
                            self.stat_trace();
                        }
                        22 | 83 | 101 | 142 => {
                            if misc <= 0 || misc & !127 != 0 {
                                return Err(unsupported("resistance aura mask", id, misc as u32));
                            }
                            for school in 0..7 {
                                if misc & (1 << school) != 0 {
                                    self.stats.resistance(
                                        school,
                                        match aura {
                                            22 => 2,
                                            83 => 0,
                                            101 => 3,
                                            _ => 1,
                                        },
                                        amount,
                                    )?;
                                }
                            }
                            self.stat_trace();
                        }
                        30 | 98 => {
                            let skill = misc as u16;
                            let amount = i16::try_from(amount)
                                .map_err(|_| InitError::Numeric("initial skill aura amount"))?;
                            self.skill_auras.push((skill, amount, aura == 98));
                            self.skill_bonus(skill, amount, aura == 98)?;
                        }
                        _ => return Err(unsupported("initial aura", id, aura)),
                    }
                }
                _ => return Err(unsupported("initial effect", id, effect)),
            }
            self.trace.push(InitEvent::EffectApplied {
                spell: id,
                index: i as u8,
                effect,
                aura,
            });
        }
        Ok(())
    }
}
