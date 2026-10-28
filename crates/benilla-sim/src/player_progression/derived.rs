// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, Project contributors.
//! StatSystem.cpp's mandatory ancillary hooks for audited static solo mages.
//! This reducer owns real fields. A containing Host must still implement skill
//! learning and prove live dependency closure; this is not a no-op Host adapter.
use super::{Error, Result, SkillStatus, Snapshot, StatHook, MAX_SUPPORTED_LEVEL};
use crate::{
    classic_spell::SpellFloat,
    mage_resource::{self, ManaRegenModifiers, SpellCritModifiers},
};
use rustc_apfloat::{ieee::Single, Float, Round, Status, StatusAnd};
use serde::{Deserialize, Serialize};

pub const POLICY_ID: &str = "cmangos-8ec338a1-static-mage-ancillary-softfloat-v1";

/// Absence requires a complete caller-side dependency scan, never a default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DerivedEvidence {
    Unknown,
    Absent,
    Present,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedAudit {
    /// Includes AP cells, damage cells, weapon haste and base-DPS multipliers.
    pub attack_power_or_damage_modifiers: DerivedEvidence,
    pub crit_or_avoidance_modifiers: DerivedEvidence,
    pub spell_damage_or_hit_modifiers: DerivedEvidence,
    pub regeneration_modifiers: DerivedEvidence,
    pub weapon_enchantments: DerivedEvidence,
    pub ammo: DerivedEvidence,
    pub shapeshift_or_disarm: DerivedEvidence,
    pub parry_block_or_dual_wield: DerivedEvidence,
}
impl DerivedAudit {
    pub fn validate(&self) -> Result<()> {
        for (domain, evidence) in [
            (
                "ancillary attack power/damage modifiers",
                self.attack_power_or_damage_modifiers,
            ),
            (
                "ancillary crit/avoidance modifiers",
                self.crit_or_avoidance_modifiers,
            ),
            (
                "ancillary spell damage/hit modifiers",
                self.spell_damage_or_hit_modifiers,
            ),
            (
                "ancillary regeneration modifiers",
                self.regeneration_modifiers,
            ),
            ("ancillary weapon enchantments", self.weapon_enchantments),
            ("ancillary ammo", self.ammo),
            ("ancillary shapeshift/disarm", self.shapeshift_or_disarm),
            (
                "ancillary parry/block/dual wield",
                self.parry_block_or_dual_wield,
            ),
        ] {
            if evidence != DerivedEvidence::Absent {
                return Err(Error::Unsupported(domain));
            }
        }
        Ok(())
    }
}
/// Actual item_template weapon fields, with the caller's current broken state.
/// All five native damage rows must be present, including trailing zero rows.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedWeapon {
    pub slot: u8,
    pub skill: u16,
    pub delay_ms: u32,
    /// Native DamageMin bits, DamageMax bits, DamageType.
    pub damage: Vec<(u32, u32, u32)>,
    pub broken: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedInputs {
    /// Complete equipped weapon list. Armor must not be represented as a weapon.
    /// Only a mainhand is supported; offhand/ranged/ammo require a wider audit.
    pub weapons: Vec<DerivedWeapon>,
    pub audit: DerivedAudit,
}
impl DerivedInputs {
    pub fn validate(&self, view: &Snapshot) -> Result<()> {
        self.audit.validate()?;
        view.audit.validate()?;
        if view.class != 8
            || !(1..=8).contains(&view.race)
            || !(1..=MAX_SUPPORTED_LEVEL).contains(&view.level)
        {
            return Err(Error::Unsupported("ancillary solo mage identity/level"));
        }
        if view
            .numeric
            .attributes
            .0
            .iter()
            .any(|v| !(0..=1_000_000).contains(v))
        {
            return Err(Error::Numeric("ancillary attributes"));
        }
        if self.weapons.len() > 1 {
            return Err(Error::Unsupported("ancillary mainhand-only equipment"));
        }
        for w in &self.weapons {
            if w.slot != 15 || w.skill == 0 || w.delay_ms > 1_000_000 {
                return Err(Error::Unsupported("ancillary weapon slot/skill/delay"));
            }
            if w.damage.len() != 5 {
                return Err(Error::Missing {
                    table: "five item_template damage rows",
                    id: u32::from(w.slot),
                });
            }
            // This bounded host does not claim elemental or multi-line weapon
            // combat. Reject rather than drop a caller-supplied damage line.
            if w.damage[0].2 != 0
                // Native DamageMax == 0 ends the rows for either zero sign.
                || w.damage[0].1 & 0x7fff_ffff == 0
                || w.damage[1..].iter().any(|r| *r != (0, 0, 0))
            {
                return Err(Error::Unsupported(
                    "ancillary one physical weapon damage line",
                ));
            }
            let min = raw(w.damage[0].0)?;
            let max = raw(w.damage[0].1)?;
            if min < Single::ZERO || min > max || max > integer(1_000_000)? {
                return Err(Error::Numeric("ancillary weapon damage"));
            }
        }
        let mut ids = std::collections::BTreeSet::new();
        if view.skills.len() > 127 || view.skills.iter().any(|s| s.id == 0 || !ids.insert(s.id)) {
            return Err(Error::Invalid("ancillary current skills"));
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttackPower {
    pub base: u32,
    pub positive_bonus: i16,
    pub negative_bonus: i16,
    pub multiplier: SpellFloat,
    pub total: SpellFloat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DamageRange {
    pub minimum: SpellFloat,
    pub maximum: SpellFloat,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DefenseBonuses {
    /// m_modDodgeChance is unadjusted; display contains the defense-skill delta.
    pub dodge: SpellFloat,
    pub dodge_display: SpellFloat,
    pub parry: SpellFloat,
    pub parry_display: SpellFloat,
    pub block: SpellFloat,
    pub block_display: SpellFloat,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedStats {
    /// Mainhand/melee and ranged, matching the two native AP field groups.
    pub attack_power: [AttackPower; 2],
    /// Mainhand, offhand, ranged. No dual wield means offhand retains initialized zero.
    pub damage: [DamageRange; 3],
    /// PCT_MOD for all three crit groups, including the source's offhand group.
    pub crit_base_percent: [SpellFloat; 3],
    /// Native m_modCritChance. UpdateCritPercentage(OFF_ATTACK) returns untouched.
    pub crit_percent: [SpellFloat; 3],
    /// Native melee and ranged UI values, adjusted by current weapon skill.
    pub crit_display_percent: [SpellFloat; 2],
    pub spell_crit_percent: [SpellFloat; 7],
    pub defense: DefenseBonuses,
    /// Normal retains its initialized zero; UpdateSpellDamageBonus writes schools 1–6.
    pub spell_damage_bonus: [i32; 7],
    pub normal_mana_regen: SpellFloat,
    pub interrupted_mana_regen: SpellFloat,
    pub weapon_hit_chance: [i32; 3],
}
impl DerivedStats {
    /// Compute a full UpdateAllStats result. Partial native callbacks can leave
    /// other cached fields unchanged; do not use this as a universal validator
    /// after an operation that only updated defense, equipment, or one skill.
    pub fn from_snapshot(view: &Snapshot, inputs: &DerivedInputs) -> Result<Self> {
        inputs.validate(view)?;
        let zero = SpellFloat::ZERO;
        let mut out = Self {
            attack_power: [AttackPower {
                base: 0,
                positive_bonus: 0,
                negative_bonus: 0,
                multiplier: zero,
                total: zero,
            }; 2],
            damage: [DamageRange {
                minimum: zero,
                maximum: zero,
            }; 3],
            crit_base_percent: [zero; 3],
            crit_percent: [zero; 3],
            crit_display_percent: [zero; 2],
            spell_crit_percent: [zero; 7],
            defense: DefenseBonuses {
                dodge: zero,
                dodge_display: zero,
                parry: zero,
                parry_display: zero,
                block: zero,
                block_display: zero,
            },
            spell_damage_bonus: [0; 7],
            normal_mana_regen: zero,
            interrupted_mana_regen: zero,
            weapon_hit_chance: [0; 3],
        };
        for hook in [
            StatHook::AttackPowerAndDamage { ranged: false },
            StatHook::AttackPowerAndDamage { ranged: true },
            StatHook::AllCritPercentages,
            StatHook::AllSpellCritChances,
            StatHook::DefenseBonuses,
            StatHook::SpellDamageBonus,
            StatHook::ManaRegen,
            StatHook::WeaponHitChance { attack: 0 },
            StatHook::WeaponHitChance { attack: 1 },
            StatHook::WeaponHitChance { attack: 2 },
        ] {
            out.apply_hook(hook, view, inputs)?;
        }
        Ok(out)
    }
    /// Execute exactly this hook using the callback-time snapshot. In particular,
    /// skill-defense callbacks may see the new level but OLD effective agility.
    /// A failed callback leaves this value unchanged.
    pub fn apply_hook(
        &mut self,
        hook: StatHook,
        view: &Snapshot,
        inputs: &DerivedInputs,
    ) -> Result<()> {
        inputs.validate(view)?;
        let mut next = self.clone();
        next.apply_inner(hook, view, inputs)?;
        *self = next;
        Ok(())
    }
    /// Replay the admitted _ApplyItemBonuses(false) ancillary callbacks before
    /// durability crosses zero. The caller must obtain `recompute` by invoking
    /// the authoritative prebreak StatRecipe::set_item_broken(slot, true, live),
    /// and must atomically commit recipe, durability, and this updated host.
    /// Restore MUST replay that recipe operation and compare the entire witness;
    /// caller-supplied attribute_steps are not independently authoritative.
    /// Repairs, item replacements, and weapon unequip are deliberately excluded.
    pub fn apply_item_break(
        &mut self,
        before_view: &Snapshot,
        before_inputs: &DerivedInputs,
        after_inputs: &DerivedInputs,
        slot: u8,
        recompute: &crate::starter_init::EquipmentRecompute,
    ) -> Result<()> {
        before_inputs.validate(before_view)?;
        after_inputs.validate(before_view)?;
        if slot > 18 || recompute.attribute_steps.len() > 10 {
            return Err(Error::Invalid("ancillary item-break witness shape"));
        }
        let mut expected_inputs = before_inputs.clone();
        let weapon = expected_inputs.weapons.iter_mut().find(|w| w.slot == slot);
        let update_damage = if let Some(weapon) = weapon {
            if weapon.broken {
                return Err(Error::Invalid("ancillary weapon already broken"));
            }
            weapon.broken = true;
            // Source clears damage rows regardless, but only calls physical
            // damage refresh when the actual item_template Delay is nonzero.
            weapon.delay_ms != 0
        } else {
            false
        };
        if expected_inputs != *after_inputs
            || after_inputs
                .weapons
                .iter()
                .any(|w| w.broken != recompute.disabled_weapon_slots.contains(&w.slot))
        {
            return Err(Error::Invalid("ancillary item-break equipment identity"));
        }
        let mut next = self.clone();
        let attributes =
            next.apply_item_attribute_callbacks(before_view, before_inputs, recompute)?;
        if attributes != recompute.effective_attributes {
            return Err(Error::Invalid("ancillary item-break final attributes"));
        }
        if update_damage {
            next.update_damage(false, after_inputs)?;
        }
        *self = next;
        Ok(())
    }
    /// Replay the ancillary callbacks for genuine original body-armor membership.
    /// The enclosing runtime must authorize original object custody, invoke the
    /// authoritative `StatRecipe::set_item_active(slot, active, live)`, and replay
    /// that operation and compare the entire witness on restore BEFORE trusting
    /// `recompute`. Shape checks here do not authenticate supplied stat changes.
    /// Commit recipe, placement, resources, and this host atomically. Broken armor
    /// produces no attribute callbacks; this is not a durability-break operation.
    /// Weapons, replacements, and unsupported item effects remain excluded.
    pub fn apply_armor_membership(
        &mut self,
        before_view: &Snapshot,
        before_inputs: &DerivedInputs,
        after_inputs: &DerivedInputs,
        slot: u8,
        recompute: &crate::starter_init::EquipmentRecompute,
    ) -> Result<()> {
        before_inputs.validate(before_view)?;
        after_inputs.validate(before_view)?;
        if slot >= 15 || recompute.attribute_steps.len() > 10 {
            return Err(Error::Invalid("ancillary armor-membership witness shape"));
        }
        let disabled_weapons: std::collections::BTreeSet<_> = before_inputs
            .weapons
            .iter()
            .filter(|weapon| weapon.broken)
            .map(|weapon| weapon.slot)
            .collect();
        if before_inputs != after_inputs || recompute.disabled_weapon_slots != disabled_weapons {
            return Err(Error::Invalid(
                "ancillary armor-membership equipment identity",
            ));
        }
        let mut next = self.clone();
        let attributes =
            next.apply_item_attribute_callbacks(before_view, before_inputs, recompute)?;
        if attributes != recompute.effective_attributes {
            return Err(Error::Invalid(
                "ancillary armor-membership final attributes",
            ));
        }
        *self = next;
        Ok(())
    }
    // Player::_ApplyItemBonuses calls HandleStatModifier in ItemStat order;
    // StatSystem.cpp::UpdateStats runs these hooks for EACH attribute callback.
    fn apply_item_attribute_callbacks(
        &mut self,
        before_view: &Snapshot,
        before_inputs: &DerivedInputs,
        recompute: &crate::starter_init::EquipmentRecompute,
    ) -> Result<crate::starter_init::Attributes> {
        let mut view = before_view.clone();
        for &(index, attributes) in &recompute.attribute_steps {
            if index >= 5
                || attributes.0.iter().enumerate().any(|(i, value)| {
                    (i != usize::from(index) && *value != view.numeric.attributes.0[i])
                        || !(0..=1_000_000).contains(value)
                })
            {
                return Err(Error::Invalid("ancillary item attribute callback"));
            }
            view.numeric.attributes = attributes;
            match index {
                1 => {
                    self.apply_inner(StatHook::AllCritPercentages, &view, before_inputs)?;
                    self.update_dodge(&view)?;
                }
                3 => self.apply_inner(StatHook::AllSpellCritChances, &view, before_inputs)?,
                _ => {}
            }
            for hook in [
                StatHook::AttackPowerAndDamage { ranged: false },
                StatHook::AttackPowerAndDamage { ranged: true },
                StatHook::SpellDamageBonus,
                StatHook::ManaRegen,
            ] {
                self.apply_inner(hook, &view, before_inputs)?;
            }
        }
        Ok(view.numeric.attributes)
    }
    fn update_damage(&mut self, ranged: bool, inputs: &DerivedInputs) -> Result<()> {
        let ap = raw(self.attack_power[usize::from(ranged)].total.bits())?;
        let (minimum, maximum, delay) = if ranged {
            // Proven no ranged item/ammo: Unit::WeaponDamageInfo defaults.
            (integer(1)?, integer(2)?, 2000)
        } else if let Some(w) = inputs.weapons.first() {
            if w.broken {
                // _ApplyItemBonuses(false) clears weapon rows and delay
                // before committing durability zero. It is NOT disarm.
                (Single::ZERO, Single::ZERO, 2000)
            } else {
                (
                    raw(w.damage[0].0)?,
                    raw(w.damage[0].1)?,
                    if w.delay_ms == 0 { 2000 } else { w.delay_ms },
                )
            }
        } else {
            (integer(1)?, integer(2)?, 2000)
        };
        let speed = div(integer(delay.into())?, integer(1000)?)?;
        let base_value = mul(div(ap, integer(14)?)?, speed)?;
        self.damage[if ranged { 2 } else { 0 }] = DamageRange {
            minimum: output(add(base_value, minimum)?)?,
            maximum: output(add(base_value, maximum)?)?,
        };
        Ok(())
    }
    fn update_dodge(&mut self, view: &Snapshot) -> Result<()> {
        let dodge = crate::npc_melee::mage_base_dodge(
            view.level as u8,
            view.numeric.attributes.0[1] as u32,
        )
        .map_err(|_| Error::Numeric("ancillary mage dodge"))?;
        self.defense.dodge = dodge;
        self.defense.dodge_display = display(dodge, skill_value(view, 95), view.level)?;
        Ok(())
    }
    fn apply_inner(
        &mut self,
        hook: StatHook,
        view: &Snapshot,
        inputs: &DerivedInputs,
    ) -> Result<()> {
        match hook {
            StatHook::AttackPowerAndDamage { ranged } => {
                let index = usize::from(ranged);
                let attribute = view.numeric.attributes.0[index];
                // C++ casts base AP through uint32. Negative values are outside
                // its defined float conversion domain, not an invented clamp.
                let base = u32::try_from(attribute - 10)
                    .map_err(|_| Error::Numeric("negative mage base attack power"))?;
                let ap = integer(i64::from(base))?;
                self.attack_power[index] = AttackPower {
                    base,
                    positive_bonus: 0,
                    negative_bonus: 0,
                    multiplier: SpellFloat::ZERO,
                    total: output(ap)?,
                };
                self.update_damage(ranged, inputs)?;
            }
            StatHook::AllCritPercentages => {
                let base = mage_melee_crit(view.level, view.numeric.attributes.0[1])?;
                self.crit_base_percent = [base; 3];
                self.crit_percent[0] = base;
                self.crit_percent[2] = base;
                let skill = inputs
                    .weapons
                    .first()
                    .filter(|w| !w.broken)
                    .map_or(162, |w| w.skill);
                self.crit_display_percent[0] = display(base, skill_value(view, skill), view.level)?;
                self.crit_display_percent[1] = display(base, 0, view.level)?;
            }
            StatHook::AllSpellCritChances => {
                let crit = mage_resource::mage_spell_crit(
                    view.level,
                    view.numeric.attributes.0[3] as u32,
                    SpellCritModifiers {
                        all_schools: 0,
                        school: 0,
                    },
                )
                .map_err(|_| Error::Numeric("ancillary spell crit"))?;
                self.spell_crit_percent = [crit; 7];
            }
            StatHook::DefenseBonuses => {
                self.defense.parry = SpellFloat::ZERO;
                self.defense.parry_display = SpellFloat::ZERO;
                self.defense.block = SpellFloat::ZERO;
                self.defense.block_display = SpellFloat::ZERO;
                self.update_dodge(view)?;
            }
            StatHook::SpellDamageBonus => {
                // SpellBaseDamageBonusDone begins at zero and iterates the two
                // damage/stat-damage aura lists, both proved empty by the audit.
                self.spell_damage_bonus[1..].fill(0);
            }
            StatHook::ManaRegen => {
                let regen = mage_resource::update_mana_regen(
                    view.numeric.attributes.0[4] as u32,
                    ManaRegenModifiers {
                        spirit_multiplier: output(integer(1)?)?,
                        flat_mana_per_five: 0,
                        interrupted_percent: 0,
                    },
                )
                .map_err(|_| Error::Numeric("ancillary mana regeneration"))?;
                self.normal_mana_regen = regen.normal_per_second;
                self.interrupted_mana_regen = regen.interrupted_per_second;
            }
            StatHook::WeaponHitChance { attack } => {
                let cell = self
                    .weapon_hit_chance
                    .get_mut(usize::from(attack))
                    .ok_or(Error::Invalid("ancillary attack type"))?;
                // Native accumulator is zero; its audited hit-aura list is empty.
                *cell = 0;
            }
        }
        Ok(())
    }
}
fn skill_value(view: &Snapshot, id: u16) -> u16 {
    // Player::GetSkill: absent/deleted is zero; otherwise max(0, sum) then uint16.
    view.skills
        .iter()
        .find(|s| s.id == id && s.status != SkillStatus::Deleted)
        .map_or(0, |s| {
            (i32::from(s.value) + i32::from(s.permanent_bonus) + i32::from(s.temporary_bonus))
                .max(0) as u16
        })
}
fn mage_melee_crit(level: u32, agility: i32) -> Result<SpellFloat> {
    // Player::GetMeleeCritFromAgility, mage constants 12.9f / 20.0f.
    let level = integer(level.into())?;
    let rate = add(
        div(
            mul(Single::from_bits(0x414e6666), sub(integer(60)?, level)?)?,
            integer(59)?,
        )?,
        div(mul(integer(20)?, sub(level, integer(1)?)?)?, integer(59)?)?,
    )?;
    output(div(integer(agility.into())?, rate)?)
}
fn display(base: SpellFloat, current_skill: u16, level: u32) -> Result<SpellFloat> {
    let difference = i64::from(current_skill) - i64::from(level * 5);
    let adjustment = mul(integer(difference)?, Single::from_bits(0x3d23d70a))?;
    output(
        add(raw(base.bits())?, adjustment)?
            .max(Single::ZERO)
            .min(integer(100)?),
    )
}
const R: Round = Round::NearestTiesToEven;
fn check<T>(v: StatusAnd<T>) -> Result<T> {
    if !(v.status & !Status::INEXACT).is_empty() {
        return Err(Error::Numeric("ancillary binary32 exception"));
    }
    Ok(v.value)
}
fn raw(bits: u32) -> Result<Single> {
    if bits & 0x7f800000 == 0x7f800000 || (bits & 0x7f800000 == 0 && bits & 0x7fffff != 0) {
        return Err(Error::Numeric("ancillary nonfinite/subnormal input"));
    }
    Ok(Single::from_bits(bits.into()))
}
fn integer(v: i64) -> Result<Single> {
    check(Single::from_i128_r(v.into(), R))
}
fn output(v: Single) -> Result<SpellFloat> {
    SpellFloat::from_bits(v.to_bits() as u32).map_err(|_| Error::Numeric("ancillary result"))
}
fn add(a: Single, b: Single) -> Result<Single> {
    check(a.add_r(b, R))
}
fn sub(a: Single, b: Single) -> Result<Single> {
    check(a.sub_r(b, R))
}
fn mul(a: Single, b: Single) -> Result<Single> {
    check(a.mul_r(b, R))
}
fn div(a: Single, b: Single) -> Result<Single> {
    check(a.div_r(b, R))
}
