// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Creature.cpp:1265-1489; StatSystem.cpp:614-825.
use super::*;

/// SelectLevel operands only. Model, equipment, terrain and event selection
/// execute earlier in InitEntry and must not be reconstructed from these fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLevelTemplate {
    pub entry: u32,
    pub levels: [u32; 2],
    pub unit_class: u32,
    pub rank: u32,
    pub health_multiplier: SpellFloat,
    pub power_multiplier: SpellFloat,
    pub armor_multiplier: SpellFloat,
    pub damage_multiplier: SpellFloat,
    pub damage_variance: SpellFloat,
    pub attribute_multipliers: [SpellFloat; 5],
    pub legacy: LegacyStats,
    pub legacy_ranged_damage: [SpellFloat; 2],
    pub regenerate_stats: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceLevelRates {
    pub health: SpellFloat,
    pub damage: SpellFloat,
}
/// ObjectMgr.cpp:763-782. Preprocessing runs once, not on every construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceClassLevelRow(ClassLevelRow);
pub fn load_source_class_level(mut row: ClassLevelRow) -> Result<SourceClassLevelRow> {
    let mana_bonus = source_uint(
        bonus(n(row.attributes[3])?, 15)?,
        "LoadCreatureClassLvlStats_mana_conversion_undefined",
    )?;
    row.base_mana -= row.base_mana.min(mana_bonus);
    row.base_armor -= row.base_armor.min(row.attributes[1].wrapping_mul(2));
    let rap = sub(
        sf(row.base_ranged_attack_power),
        sf(row.base_ranged_attack_power).min(n(row.attributes[1].saturating_sub(10))?),
    )?;
    row.base_ranged_attack_power = out(rap)?;
    Ok(SourceClassLevelRow(row))
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceLevelStats {
    pub level: u32,
    pub create_attributes_bits: [u32; 5],
    pub attributes: [i32; 5],
    pub create_health: u32,
    pub create_mana: u32,
    pub maximum_health: u32,
    pub health: u32,
    pub maximum_power: [u32; 5],
    pub power: [u32; 5],
    pub armor: i32,
    pub attack_power: [u32; 2],
    /// Mainhand, offhand, ranged, before UpdateDamagePhysical.
    pub weapon_damage_bits: [[u32; 2]; 3],
    pub damage_multiplier_bits: u32,
}
impl SourceLevelStats {
    /// Creature.cpp:526-536. Not called on preserveHPAndPower UpdateEntry.
    pub fn apply_spawn(&mut self, health: u32, mana: u32, alive: bool, class: u32, regen: u32) {
        self.health = if !alive {
            0
        } else if health == 0 {
            self.maximum_health
        } else {
            health.min(self.maximum_health)
        };
        if matches!(class, 2 | 8) {
            self.power[0] = if mana == 0 && regen & 8 != 0 {
                self.maximum_power[0]
            } else {
                mana.min(self.maximum_power[0])
            };
        }
    }
}
fn source_uint(value: Single, site: &'static str) -> Result<u32> {
    let converted = value.to_u128_r(32, Round::TowardZero, &mut false);
    if converted.status.contains(Status::INVALID_OP) {
        return Err(InitError::Numeric(site));
    }
    u32::try_from(converted.value).map_err(|_| InitError::Numeric(site))
}
fn source_int(value: Single, site: &'static str) -> Result<i32> {
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    if converted.status.contains(Status::INVALID_OP) {
        return Err(InitError::Numeric(site));
    }
    i32::try_from(converted.value).map_err(|_| InitError::Numeric(site))
}
fn signed(value: i32) -> Single {
    Single::from_i128(i128::from(value)).value
}
fn class_index(class: u32) -> u32 {
    match class {
        2 => 1,
        8 => 2,
        _ => 0,
    }
}
fn legacy_value(t: &SourceLevelTemplate, level: u32, a: u32, b: u32) -> Result<u32> {
    let relative = if t.levels[0] == t.levels[1] {
        Single::ZERO
    } else {
        div(
            n(level.wrapping_sub(t.levels[0]))?,
            n(t.levels[1].wrapping_sub(t.levels[0]))?,
        )?
    };
    Ok(a.min(b).wrapping_add(source_uint(
        mul(relative, n(a.abs_diff(b))?)?,
        "SelectLevel_legacy_interpolation_conversion_undefined",
    )?))
}
/// No RNG occurs here. The construction owner passes its already consumed level.
/// None is the real ObjectMgr.cpp:795-798 getter miss, not missing catalog evidence.
pub fn select_level_stats(
    t: &SourceLevelTemplate,
    row: Option<&SourceClassLevelRow>,
    level: u32,
    forced: bool,
    rates: SourceLevelRates,
) -> Result<SourceLevelStats> {
    // ObjectMgr.h:1354; SharedDefines.h:104. Guard before the source array read.
    if level > 70 || t.unit_class >= 12 {
        return Err(InitError::Numeric(
            "GetCreatureClassLvlStats_index_undefined",
        ));
    }
    let mut health = u32::MAX;
    let mut mana = u32::MAX;
    let mut armor = -n(1)?;
    let mut weapon = [[Single::ZERO; 2]; 3];
    let mut ap = [Single::ZERO; 2];
    let mut create = [Single::ZERO; 5];
    let damage_multiplier = mul(sf(t.damage_multiplier), sf(rates.damage))?;
    let mut modern_damage = false;
    let row = row.map(|row| &row.0);
    let row = row.filter(|r| {
        (r.base_health != 0 || r.attributes[2] > 0)
            && sf(r.base_damage) > Single::from_bits(0x3dcccccd)
    });
    if let Some(row) = row {
        if u32::from(row.level) != level || class_index(row.class) != class_index(t.unit_class) {
            return Err(InitError::Invalid("SelectLevel classlevel identity"));
        }
        if sf(t.health_multiplier) >= Single::ZERO {
            health = row.base_health;
        }
        if sf(t.power_multiplier) >= Single::ZERO {
            mana = row.base_mana;
        }
        if sf(t.armor_multiplier) >= Single::ZERO {
            armor = round(mul(n(row.base_armor)?, sf(t.armor_multiplier))?)?;
        }
        if sf(t.damage_multiplier) >= Single::ZERO && sf(t.armor_multiplier) >= Single::ZERO {
            modern_damage = true;
            let spread = mul(sf(row.base_damage), div(sf(t.damage_variance), n(2)?)?)?;
            weapon = [[
                sub(sf(row.base_damage), spread)?,
                add(sf(row.base_damage), spread)?,
            ]; 3];
            ap = [
                sf(row.base_melee_attack_power),
                sf(row.base_ranged_attack_power),
            ];
        }
        for (i, value) in create.iter_mut().enumerate() {
            if sf(t.attribute_multipliers[i]) >= Single::ZERO {
                *value = mul(n(row.attributes[i])?, sf(t.attribute_multipliers[i]))?;
            }
        }
    }
    if !modern_damage || armor == -n(1)? {
        if !forced || (t.levels[0]..=t.levels[1]).contains(&level) {
            if armor == -n(1)? {
                health = legacy_value(t, level, t.legacy.minimum_health, t.legacy.maximum_health)?;
                mana = legacy_value(t, level, t.legacy.minimum_mana, t.legacy.maximum_mana)?;
                armor = n(t.legacy.armor)?;
            }
            if !modern_damage {
                weapon[0] = [
                    mul(sf(t.legacy.minimum_melee_damage), damage_multiplier)?,
                    mul(sf(t.legacy.maximum_melee_damage), damage_multiplier)?,
                ];
                weapon[1] = [div(weapon[0][0], n(2)?)?, div(weapon[0][1], n(2)?)?];
                weapon[2] = [
                    mul(sf(t.legacy_ranged_damage[0]), damage_multiplier)?,
                    mul(sf(t.legacy_ranged_damage[1]), damage_multiplier)?,
                ];
                ap = [
                    n(t.legacy.melee_attack_power)?,
                    n(t.legacy.ranged_attack_power)?,
                ];
            }
        } else {
            if t.levels[1] == 0 {
                return Err(InitError::Numeric(
                    "SelectLevel_forced_level_division_undefined",
                ));
            }
            health = (t.legacy.maximum_health / t.levels[1]).wrapping_mul(level);
            mana = (t.legacy.maximum_mana / t.levels[1]).wrapping_mul(level);
            armor = Single::ZERO;
        }
    }
    health = source_uint(
        mul(n(health)?, sf(rates.health))?,
        "SelectLevel_health_rate_conversion_undefined",
    )?
    .max(1);
    let health_multiplier = if is_positive(t.health_multiplier) {
        sf(t.health_multiplier)
    } else {
        n(1)?
    };
    let power_multiplier = if is_positive(t.power_multiplier) {
        sf(t.power_multiplier)
    } else {
        n(1)?
    };
    let mut attrs = [0i32; 5];
    attrs[0] = source_int(create[0], "UpdateStats_attribute_conversion_undefined")?;
    // UpdateAllStats visits strength before agility on a fresh Unit. The rogue
    // transient AP conversion is executable even when the final AP would fit.
    let strength = signed(attrs[0]);
    let base_ap = match t.unit_class {
        8 => strength,
        4 => sub(sub(strength, create[0])?, create[1])?,
        _ => mul(sub(strength, create[0])?, n(2)?)?,
    };
    let melee = source_uint(
        add(base_ap, mul(ap[0], damage_multiplier)?)?,
        "UpdateAttackPowerAndDamage_melee_conversion_undefined",
    )?;
    attrs[1] = source_int(create[1], "UpdateStats_attribute_conversion_undefined")?;
    let armor = source_int(
        add(armor, mul(signed(attrs[1]), n(2)?)?)?,
        "UpdateArmor_conversion_undefined",
    )?;
    let ranged = source_uint(
        add(
            sub(signed(attrs[1]), create[1])?,
            mul(ap[1], damage_multiplier)?,
        )?,
        "UpdateAttackPowerAndDamage_ranged_conversion_undefined",
    )?;
    attrs[2] = source_int(create[2], "UpdateStats_attribute_conversion_undefined")?;
    let stamina_bonus = sub(bonus(signed(attrs[2]), 10)?, bonus(create[2], 10)?)?;
    let maximum_health = source_uint(
        initial_max_health(health, health_multiplier, stamina_bonus)?,
        "UpdateMaxHealth_conversion_undefined",
    )?;
    attrs[3] = source_int(create[3], "UpdateStats_attribute_conversion_undefined")?;
    let mana_bonus = if mana > 0 {
        bonus(signed(attrs[3]), 15)?
    } else {
        Single::ZERO
    };
    let maximum_mana = source_uint(
        initial_max_mana(
            mana,
            if matches!(t.unit_class, 2 | 8) {
                power_multiplier
            } else {
                n(1)?
            },
            mana_bonus,
        )?,
        "UpdateMaxPower_mana_conversion_undefined",
    )?;
    attrs[4] = source_int(create[4], "UpdateStats_attribute_conversion_undefined")?;
    // Creature.cpp:1438-1440; Unit.h:1012-1014, Unit.cpp:9374-9383.
    let maximum_power = [maximum_mana, 0, 100, 0, 1_000_000];
    let create_attributes_bits = create.map(|value| value.to_bits() as u32);
    let weapon_damage_bits = weapon.map(|range| range.map(|value| value.to_bits() as u32));
    Ok(SourceLevelStats {
        level,
        create_attributes_bits,
        attributes: attrs,
        create_health: health,
        create_mana: mana,
        maximum_health,
        health: maximum_health,
        maximum_power,
        power: maximum_power,
        armor,
        attack_power: [melee, ranged],
        weapon_damage_bits,
        damage_multiplier_bits: damage_multiplier.to_bits() as u32,
    })
}
