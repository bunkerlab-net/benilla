// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
// Original GPL-2.0-or-later source grant; Rust translation GPL-3.0-or-later.
//! Bounded player-mage resource dependencies, not a native character compiler.
//! Callers supply authoritative effective stats, current skills, modifier totals
//! and update events. No database rows, default stats or level-derived skills.
//! Source branches, exclusions, numeric contract and attribution: ../MAGE-RESOURCES.md.
use crate::classic_spell::{CasterResources, SpellError, SpellEvent, SpellFloat};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};
use std::collections::{BTreeMap, BTreeSet};

pub const MAGE_RESOURCE_POLICY_ID: &str = "cmangos-8ec338a1-mage-resource-softfloat-v1";
const R: Round = Round::NearestTiesToEven;
const MAX_AMOUNT: u32 = 1_000_000;

fn checked<T>(value: StatusAnd<T>) -> Result<T, SpellError> {
    if !(value.status & !Status::INEXACT).is_empty() {
        return Err(SpellError::Domain("mage resource floating exception"));
    }
    Ok(value.value)
}
fn integer(value: i64) -> Result<Single, SpellError> {
    checked(Single::from_i128_r(i128::from(value), R))
}
fn single(value: SpellFloat) -> Single {
    Single::from_bits(u128::from(value.bits()))
}
fn output(value: Single) -> Result<SpellFloat, SpellError> {
    SpellFloat::from_bits(value.to_bits() as u32)
}
fn unsigned(value: Single) -> Result<u32, SpellError> {
    let result = checked(value.to_u128_r(32, Round::TowardZero, &mut false))?;
    u32::try_from(result).map_err(|_| SpellError::Domain("mage resource uint32 conversion"))
}
fn stat(value: u32) -> Result<Single, SpellError> {
    if value > MAX_AMOUNT {
        return Err(SpellError::Domain("effective mage stat"));
    }
    integer(i64::from(value))
}

/// Indexed DBC row ID and the two fields used by Player::GetSpellRank. A caller
/// must include ALL mappings for queried spell IDs, including other skill lines.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillLineAbilityRow {
    pub row_id: u32,
    pub spell_id: u32,
    pub skill_id: u32,
}
/// The first mapping is the earliest source DBC row ID, not the best known skill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SkillLineAbilities(BTreeMap<u32, SkillLineAbilityRow>);
impl SkillLineAbilities {
    pub fn new(mut rows: Vec<SkillLineAbilityRow>) -> Result<Self, SpellError> {
        rows.sort_by_key(|row| row.row_id);
        let mut ids = BTreeSet::new();
        let mut first = BTreeMap::new();
        for row in rows {
            if !ids.insert(row.row_id) {
                return Err(SpellError::Domain("duplicate SkillLineAbility row ID"));
            }
            first.entry(row.spell_id).or_insert(row);
        }
        Ok(Self(first))
    }
    pub fn first(&self, spell_id: u32) -> Option<SkillLineAbilityRow> {
        self.0.get(&spell_id).copied()
    }
}
/// GetSkillValue's current value and signed bonuses, not maximum or pure skill.
/// Fields must come from learned, live player state; SQL candidates are not it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CurrentSkill {
    pub id: u16,
    pub value: u16,
    pub permanent_bonus: i16,
    pub temporary_bonus: i16,
    pub deleted: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CurrentSkills(BTreeMap<u16, CurrentSkill>);
impl CurrentSkills {
    pub fn new(rows: Vec<CurrentSkill>) -> Result<Self, SpellError> {
        let mut skills = BTreeMap::new();
        for row in rows {
            if skills.insert(row.id, row).is_some() {
                return Err(SpellError::Domain("duplicate current skill ID"));
            }
        }
        Ok(Self(skills))
    }
    pub fn value(&self, id: u16) -> u16 {
        let Some(row) = self.0.get(&id).filter(|row| id != 0 && !row.deleted) else {
            return 0;
        };
        // C++ clamps negative int32, then converts to uint16 (including wrap).
        (i32::from(row.value) + i32::from(row.permanent_bonus) + i32::from(row.temporary_bonus))
            .max(0) as u16
    }
}
/// Player::GetSpellRank: absent mapping/skill means zero, never level * 5.
/// Native maxLevel is uint32; reject its otherwise-wrapping multiplication.
pub fn spell_rank(
    spell_id: u32,
    max_level: u32,
    abilities: &SkillLineAbilities,
    skills: &CurrentSkills,
) -> Result<u32, SpellError> {
    let Some(row) = abilities.first(spell_id) else {
        return Ok(0);
    };
    // Source GetSkillValue accepts uint16, so preserve its implicit conversion.
    let mut rank = u32::from(skills.value(row.skill_id as u16));
    let cap = max_level
        .checked_mul(5)
        .ok_or(SpellError::Domain("spell maxLevel * 5"))?;
    if max_level > 0 && rank >= cap {
        rank = cap;
    }
    Ok(rank)
}

/// Explicit UpdateSpellCritChance aggregates. Only zero/zero is admitted here;
/// final Unit::GetCritChance spell/victim/item/aura modifiers remain external.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellCritModifiers {
    pub all_schools: i32,
    pub school: i32,
}
/// GetSpellCritFromIntellect + UpdateSpellCritChance's no-modifier path. Upstream
/// explicitly says its vanilla coefficients MUST BE CHECKED: pinned-port parity
/// only, not independently verified retail Classic accuracy. Percent, not die.
pub fn mage_spell_crit(
    level: u32,
    intellect: u32,
    modifiers: SpellCritModifiers,
) -> Result<SpellFloat, SpellError> {
    if !(1..=60).contains(&level) {
        return Err(SpellError::Domain("mage level"));
    }
    if modifiers.all_schools != 0 || modifiers.school != 0 {
        return Err(SpellError::Unsupported("spell crit aura modifiers"));
    }
    let ratio = checked(Single::from_bits(0x416c_51ec).add_r(
        checked(Single::from_bits(0x3f26_6666).mul_r(integer(i64::from(level))?, R))?,
        R,
    ))?;
    let crit = checked(
        Single::from_bits(0x406c_cccd).add_r(checked(stat(intellect)?.div_r(ratio, R))?, R),
    )?;
    let crit = checked(Single::ZERO.add_r(crit, R))?;
    let crit = checked(crit.add_r(integer(i64::from(modifiers.all_schools))?, R))?;
    output(checked(
        crit.add_r(integer(i64::from(modifiers.school))?, R),
    )?)
}
/// Resolved target classification for Unit::GetMissChance(SpellSchoolMask).
/// Ordinary includes player and non-totem creature; this is not final miss chance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MagicMissTarget {
    Ordinary,
    CreatureTotem,
}
pub fn base_magic_miss(target: MagicMissTarget) -> SpellFloat {
    match target {
        MagicMissTarget::Ordinary => SpellFloat::from_bits(0x4080_0000).expect("4.0"),
        MagicMissTarget::CreatureTotem => SpellFloat::ZERO,
    }
}

/// Unit::OCTRegenMPPerSpirit, mage branch. The unsuffixed 12.5 is binary64:
/// float( double(float(Spirit / 4)) + 12.5 ) / 2.0f.
pub fn mage_spirit_regen(spirit: u32) -> Result<SpellFloat, SpellError> {
    let quarter = checked(stat(spirit)?.div_r(integer(4)?, R))?;
    let wide: Double = checked(quarter.convert_r(R, &mut false))?;
    let wide = checked(wide.add_r(Double::from_bits(0x4029_0000_0000_0000), R))?;
    let narrowed: Single = checked(wide.convert_r(R, &mut false))?;
    output(checked(narrowed.div_r(integer(2)?, R))?)
}
/// Resolved source aura aggregates, with no inferred neutral defaults. Aggregation
/// order, spell eligibility and passive/item application must be proven upstream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaRegenModifiers {
    pub spirit_multiplier: SpellFloat,
    pub flat_mana_per_five: i32,
    pub interrupted_percent: i32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaRegen {
    pub normal_per_second: SpellFloat,
    pub interrupted_per_second: SpellFloat,
}
/// UpdateManaRegen. Only the upper interrupt-percent clamp exists in the source.
/// Negative intermediate totals are allowed, but a negative final mana rate is
/// rejected before the source's undefined float-to-uint conversion can occur.
pub fn update_mana_regen(
    spirit: u32,
    modifiers: ManaRegenModifiers,
) -> Result<ManaRegen, SpellError> {
    if modifiers.flat_mana_per_five.unsigned_abs() > MAX_AMOUNT
        || modifiers.interrupted_percent.unsigned_abs() > MAX_AMOUNT
        || single(modifiers.spirit_multiplier) < Single::ZERO
    {
        return Err(SpellError::Domain("mana regen modifiers"));
    }
    let power =
        checked(single(mage_spirit_regen(spirit)?).mul_r(single(modifiers.spirit_multiplier), R))?;
    let mp5 = checked(integer(i64::from(modifiers.flat_mana_per_five))?.div_r(integer(5)?, R))?;
    let percent = integer(i64::from(modifiers.interrupted_percent.min(100)))?;
    let interrupted = checked(mp5.add_r(
        checked(checked(power.mul_r(percent, R))?.div_r(integer(100)?, R))?,
        R,
    ))?;
    let normal = checked(mp5.add_r(power, R))?;
    if normal < Single::ZERO || interrupted < Single::ZERO {
        return Err(SpellError::Domain("negative resolved mana regeneration"));
    }
    Ok(ManaRegen {
        normal_per_second: output(normal)?,
        interrupted_per_second: output(interrupted)?,
    })
}
impl ManaRegen {
    fn validate(self) -> Result<(), SpellError> {
        if single(self.normal_per_second) < Single::ZERO
            || single(self.interrupted_per_second) < Single::ZERO
        {
            return Err(SpellError::Domain("negative mana regeneration"));
        }
        Ok(())
    }
}
/// The exact source uint32(float(diff) / 1000), not integer diff / 1000.
/// This accepts all uint32 diffs to preserve their binary32 rounding boundaries.
pub fn regeneration_seconds(diff_ms: u32) -> Result<u32, SpellError> {
    unsigned(checked(
        integer(i64::from(diff_ms))?.div_r(integer(1000)?, R),
    )?)
}
/// Player::Regenerate(POWER_MANA). Integer truncation occurs once per call and
/// fractional mana is discarded. Invalid float conversions and uint32 addition
/// overflow are rejected rather than silently manufacturing a safe result.
pub fn regenerate_mana(
    current: u32,
    maximum: u32,
    diff_ms: u32,
    recent_mana_use: bool,
    regen: ManaRegen,
    world_mana_rate: SpellFloat,
) -> Result<u32, SpellError> {
    regen.validate()?;
    if current > maximum || maximum > MAX_AMOUNT || single(world_mana_rate) < Single::ZERO {
        return Err(SpellError::Domain("mana state/rate"));
    }
    let per_second = if recent_mana_use {
        regen.interrupted_per_second
    } else {
        regen.normal_per_second
    };
    let addition = checked(
        checked(single(per_second).mul_r(single(world_mana_rate), R))?
            .mul_r(integer(i64::from(regeneration_seconds(diff_ms)?))?, R),
    )?;
    let value = current
        .checked_add(unsigned(addition)?)
        .ok_or(SpellError::Domain("mana uint32 addition"))?;
    Ok(value.min(maximum))
}

/// Explicit proof obligations for bridging the existing bounded spell component.
/// These observations must be resolved from the admitted raw rows/state; there is
/// intentionally no Default or guessed-true constructor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaUseExclusions {
    pub cast_item: bool,
    pub triggered_by_aura: bool,
    pub dont_block_mana_regen: bool,
    pub other_mana_mutation: bool,
}
impl ManaUseExclusions {
    fn validate(self) -> Result<(), SpellError> {
        if self.cast_item
            || self.triggered_by_aura
            || self.dont_block_mana_regen
            || self.other_mana_mutation
        {
            return Err(SpellError::Unsupported(
                "mana use branch outside mage projectile component",
            ));
        }
        Ok(())
    }
}
/// A snapshot of the resource part of Player state; timers are source counters,
/// not `now - last_mana_use_ms`. Restore requires explicit authoritative counters.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MageManaState {
    pub caster: CasterResources,
    maximum: u32,
    regen: ManaRegen,
    regen_elapsed_ms: u32,
    last_mana_use_timer_ms: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ManaFrame {
    pub diff_ms: u32,
    pub in_world: bool,
    pub alive_after_events: bool,
    pub world_mana_rate: SpellFloat,
    pub exclusions: ManaUseExclusions,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ManaFrameResult {
    pub spell_events: Vec<SpellEvent>,
    pub regenerated_diff_ms: Option<u32>,
    pub mana_before_regeneration: u32,
    pub mana_after_regeneration: u32,
}
impl MageManaState {
    pub fn from_state(
        caster: CasterResources,
        maximum: u32,
        regen: ManaRegen,
        regen_elapsed_ms: u32,
        last_mana_use_timer_ms: u32,
    ) -> Result<Self, SpellError> {
        regen.validate()?;
        if caster.mana > maximum
            || maximum > MAX_AMOUNT
            || last_mana_use_timer_ms > 5000
            || regen_elapsed_ms >= 2000
        {
            return Err(SpellError::Domain("mage mana snapshot"));
        }
        Ok(Self {
            caster,
            maximum,
            regen,
            regen_elapsed_ms,
            last_mana_use_timer_ms,
        })
    }
    pub fn regen_elapsed_ms(&self) -> u32 {
        self.regen_elapsed_ms
    }
    pub fn last_mana_use_timer_ms(&self) -> u32 {
        self.last_mana_use_timer_ms
    }
    pub fn set_regen(&mut self, regen: ManaRegen) -> Result<(), SpellError> {
        regen.validate()?;
        self.regen = regen;
        Ok(())
    }
    /// Invoke AFTER the caller executes spell-event/aura updates, supplying every
    /// returned spell event for this frame exactly once. Successful positive
    /// ManaSpent resets to 5000 even when wall-clock timestamps are identical;
    /// then Unit::Update decrements by the SAME diff before Player regeneration.
    /// Dead players decrement mana-use time, but do not accumulate regen time.
    /// Out-of-world updates are no-ops and cannot contain spell events.
    /// Validation errors leave this resource adapter unchanged. Spell execution
    /// outside it has its own commit boundary, so callers must stage a full frame.
    pub fn after_spell_and_aura_events(
        &mut self,
        frame: ManaFrame,
        spell_events: Vec<SpellEvent>,
    ) -> Result<ManaFrameResult, SpellError> {
        frame.exclusions.validate()?;
        if !frame.in_world {
            if !spell_events.is_empty() {
                return Err(SpellError::State("out-of-world spell events"));
            }
            return Ok(ManaFrameResult {
                spell_events,
                regenerated_diff_ms: None,
                mana_before_regeneration: self.caster.mana,
                mana_after_regeneration: self.caster.mana,
            });
        }
        if self.caster.mana > self.maximum || single(frame.world_mana_rate) < Single::ZERO {
            return Err(SpellError::Domain("mage mana frame"));
        }
        let mut next = self.clone();
        if spell_events
            .iter()
            .any(|event| matches!(event, SpellEvent::ManaSpent(amount) if *amount > 0))
        {
            next.last_mana_use_timer_ms = 5000;
        }
        next.last_mana_use_timer_ms = next.last_mana_use_timer_ms.saturating_sub(frame.diff_ms);
        let before = next.caster.mana;
        let mut scheduled = None;
        if frame.alive_after_events {
            next.regen_elapsed_ms = next
                .regen_elapsed_ms
                .checked_add(frame.diff_ms)
                .ok_or(SpellError::Domain("regen timer uint32 addition"))?;
            if next.regen_elapsed_ms >= 2000 {
                let diff = next.regen_elapsed_ms / 100 * 100;
                next.caster.mana = regenerate_mana(
                    next.caster.mana,
                    next.maximum,
                    diff,
                    next.last_mana_use_timer_ms != 0,
                    next.regen,
                    frame.world_mana_rate,
                )?;
                next.regen_elapsed_ms -= diff;
                scheduled = Some(diff);
            }
        }
        let result = ManaFrameResult {
            spell_events,
            regenerated_diff_ms: scheduled,
            mana_before_regeneration: before,
            mana_after_regeneration: next.caster.mana,
        };
        *self = next;
        Ok(result)
    }
}
