// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:1265-1489 and ObjectMgr.cpp:719-798 operands only.
use super::*;
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::creature_init::{ClassLevelRow, LegacyStats, SourceLevelTemplate};

fn relation(table: &str) -> Result<()> {
    check(
        matches!(
            table,
            "creature_template" | "creature_template_classlevelstats"
        ),
        "creature_level.relation",
    )
}
fn template_class(class: u32) -> Result<u32> {
    // ObjectMgr.cpp:477 short-circuits zero. FlagConvertibleEnum.h:56-58.
    check(class <= 32, "creature_level.template_class_shift_undefined")?;
    Ok(if matches!(class, 1 | 2 | 8) { class } else { 1 })
}
fn classlevel_class(class: u32) -> Result<bool> {
    // ObjectMgr.cpp:755 does not short-circuit zero before this shift.
    check(
        (1..=32).contains(&class),
        "creature_level.classlevel_class_shift_undefined",
    )?;
    Ok(matches!(class, 1 | 2 | 8))
}
impl VerifiedCatalog {
    fn creature_level_rows<T>(
        &mut self,
        table: &str,
        mut visit: impl FnMut(&[String], &[Option<&[u8]>; 256]) -> Result<T>,
    ) -> Result<Vec<T>> {
        relation(table)?;
        let schema = bounded_json(&self.object(&descriptor(&self.manifest["schema_root"])?, 0)?)?;
        let cols = spawns::columns(&schema, table)?;
        let mut rows = Vec::new();
        self.scan(table, 0, 256, |row| {
            let mut cells = [None; 256];
            spawns::fill(&row, &mut cells)?;
            rows.push(visit(&cols, &cells)?);
            Ok(())
        })?;
        Ok(rows)
    }
    pub(crate) fn creature_level_templates(&mut self) -> Result<Vec<SourceLevelTemplate>> {
        self.creature_level_rows("creature_template", |cols, cells| {
            let u = |name| spawns::uint(cells[spawns::position(cols, name)?]);
            let f = |name| {
                let value = spawns::floating(cells[spawns::position(cols, name)?])?;
                SpellFloat::from_bits(value.to_bits()).map_err(|_| fail("creature_level.float"))
            };
            // ObjectMgr.cpp:453-480 repairs levels before classlevel lookups.
            let low = u("MinLevel")?.max(1);
            let high = u("MaxLevel")?.max(low);
            let class = u("UnitClass")?;
            Ok(SourceLevelTemplate {
                entry: u("Entry")?,
                levels: [low.min(70), high.min(70)],
                unit_class: template_class(class)?,
                rank: u("Rank")?,
                health_multiplier: f("HealthMultiplier")?,
                power_multiplier: f("PowerMultiplier")?,
                armor_multiplier: f("ArmorMultiplier")?,
                damage_multiplier: f("DamageMultiplier")?,
                damage_variance: f("DamageVariance")?,
                attribute_multipliers: [
                    f("StrengthMultiplier")?,
                    f("AgilityMultiplier")?,
                    f("StaminaMultiplier")?,
                    f("IntellectMultiplier")?,
                    f("SpiritMultiplier")?,
                ],
                legacy: LegacyStats {
                    minimum_health: u("MinLevelHealth")?,
                    maximum_health: u("MaxLevelHealth")?,
                    minimum_mana: u("MinLevelMana")?,
                    maximum_mana: u("MaxLevelMana")?,
                    armor: u("Armor")?,
                    minimum_melee_damage: f("MinMeleeDmg")?,
                    maximum_melee_damage: f("MaxMeleeDmg")?,
                    melee_attack_power: u("MeleeAttackPower")?,
                    ranged_attack_power: u("RangedAttackPower")?,
                },
                legacy_ranged_damage: [f("MinRangedDmg")?, f("MaxRangedDmg")?],
                regenerate_stats: u("RegenerateStats")?,
            })
        })
    }
    pub(crate) fn creature_level_classes(&mut self) -> Result<Vec<ClassLevelRow>> {
        self.creature_level_rows("creature_template_classlevelstats", |cols, cells| {
            let u = |name| spawns::uint(cells[spawns::position(cols, name)?]);
            let f = |name| {
                let value = spawns::floating(cells[spawns::position(cols, name)?])?;
                SpellFloat::from_bits(value.to_bits()).map_err(|_| fail("creature_level.float"))
            };
            let level = u("Level")?;
            if !(1..=70).contains(&level) || !classlevel_class(u("Class")?)? {
                return Ok(None);
            }
            Ok(Some(ClassLevelRow {
                class: u("Class")?,
                level: level as u8,
                base_health: u("BaseHealthExp0")?,
                base_mana: u("BaseMana")?,
                base_armor: u("BaseArmor")?,
                base_melee_attack_power: f("BaseMeleeAttackPower")?,
                base_ranged_attack_power: f("BaseRangedAttackPower")?,
                base_damage: f("BaseDamageExp0")?,
                attributes: [
                    u("Strength")?,
                    u("Agility")?,
                    u("Stamina")?,
                    u("Intellect")?,
                    u("Spirit")?,
                ],
            }))
        })
        .map(|rows| rows.into_iter().flatten().collect())
    }
}
#[test]
fn reader_refuses_other_relations() {
    relation("creature_template").unwrap();
    relation("creature_template_classlevelstats").unwrap();
    for table in [
        "creature",
        "creature_spawn_data_template",
        "spell_template",
        "",
    ] {
        assert_eq!(relation(table).unwrap_err().id, "creature_level.relation");
    }
}

#[test]
fn class_flag_shifts_preserve_defined_32_and_refuse_33() {
    assert_eq!(template_class(0).unwrap(), 1);
    assert_eq!(template_class(32).unwrap(), 1);
    assert_eq!(
        template_class(33).unwrap_err().id,
        "creature_level.template_class_shift_undefined"
    );
    assert!(!classlevel_class(32).unwrap());
    assert_eq!(
        classlevel_class(33).unwrap_err().id,
        "creature_level.classlevel_class_shift_undefined"
    );
    assert_eq!(
        classlevel_class(0).unwrap_err().id,
        "creature_level.classlevel_class_shift_undefined"
    );
}
