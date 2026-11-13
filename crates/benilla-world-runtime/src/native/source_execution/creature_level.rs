// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:523,1265-1489. Construction consumes RNG, arithmetic never does.
use super::*;
use benilla_sim::classic_spell::SpellFloat;
use benilla_sim::creature_init::{
    self as init, ClassLevelRow, SourceLevelRates, SourceLevelStats, SourceLevelTemplate,
};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct CreatureLevelConfig {
    pub sha256: Sha256Id,
    /// Normal, elite, rare elite, world boss, rare. HP and damage source lines.
    pub lines: [[usize; 2]; 5],
    rates: [SourceLevelRates; 5],
}
impl CreatureLevelConfig {
    pub fn recorded(path: &Path, approved: &Sha256Id) -> Result<Self> {
        let file = ExecutionFile::open(path, 1_048_576)?;
        let mut bytes = vec![0; file.len() as usize];
        file.read_into(0, &mut bytes)?;
        file.check()?;
        require(
            Sha256Id::of(&bytes) == *approved,
            "execution.creature.level_config_identity",
        )?;
        Self::parse(&bytes, approved.clone())
    }
    pub(super) fn parse(bytes: &[u8], sha256: Sha256Id) -> Result<Self> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| refuse("execution.creature.level_config_encoding"))?;
        let mut values = [[None; 2]; 5];
        let mut lines = [[0; 2]; 5];
        const KEYS: [[&str; 2]; 5] = [
            ["Rate.Creature.Normal.HP", "Rate.Creature.Normal.Damage"],
            [
                "Rate.Creature.Elite.Elite.HP",
                "Rate.Creature.Elite.Elite.Damage",
            ],
            [
                "Rate.Creature.Elite.RAREELITE.HP",
                "Rate.Creature.Elite.RAREELITE.Damage",
            ],
            [
                "Rate.Creature.Elite.WORLDBOSS.HP",
                "Rate.Creature.Elite.WORLDBOSS.Damage",
            ],
            [
                "Rate.Creature.Elite.RARE.HP",
                "Rate.Creature.Elite.RARE.Damage",
            ],
        ];
        for (line, raw) in text.lines().enumerate() {
            let Some((key, value)) = raw.split('#').next().unwrap_or("").split_once('=') else {
                continue;
            };
            for (rank, keys) in KEYS.iter().enumerate() {
                for (kind, name) in keys.iter().enumerate() {
                    if key.trim() != *name {
                        continue;
                    }
                    require(
                        values[rank][kind].is_none(),
                        "execution.creature.level_config_duplicate",
                    )?;
                    let value = value
                        .trim()
                        .parse::<f32>()
                        .map_err(|_| refuse("execution.creature.level_config_value"))?;
                    // World.cpp:428-437 uses setConfigPos. Missing evidence never
                    // selects that loader's default, even when the value is one.
                    require(
                        value.is_finite() && value >= 0.0,
                        "execution.creature.level_config_value",
                    )?;
                    values[rank][kind] = Some(
                        SpellFloat::from_bits(value.to_bits())
                            .map_err(|_| refuse("execution.creature.level_config_value"))?,
                    );
                    lines[rank][kind] = line + 1;
                }
            }
        }
        let mut rates = [SourceLevelRates {
            health: SpellFloat::ZERO,
            damage: SpellFloat::ZERO,
        }; 5];
        for (rate, pair) in rates.iter_mut().zip(values) {
            *rate = SourceLevelRates {
                health: pair[0]
                    .ok_or_else(|| refuse("execution.creature.level_config_health_owner"))?,
                damage: pair[1]
                    .ok_or_else(|| refuse("execution.creature.level_config_damage_owner"))?,
            };
        }
        Ok(Self {
            sha256,
            lines,
            rates,
        })
    }
    /// Creature.cpp:1492-1527, including the unknown-rank elite fallback.
    pub fn rates(&self, rank: u32, pet: bool) -> SourceLevelRates {
        self.rates[if pet {
            0
        } else {
            match rank {
                0..=4 => rank as usize,
                _ => 1,
            }
        }]
    }
}
#[derive(Clone, Debug)]
pub struct CreatureLevelDefinitions {
    templates: BTreeMap<u32, SourceLevelTemplate>,
    classes: BTreeMap<(u32, u32), init::SourceClassLevelRow>,
}
fn class_index(class: u32) -> u32 {
    match class {
        2 => 1,
        8 => 2,
        _ => 0,
    }
}
fn level_refusal(error: init::InitError) -> Refusal {
    match error {
        init::InitError::Numeric(site) => Refusal {
            id: format!("execution.creature.{site}"),
        },
        other => Refusal {
            id: format!("execution.creature.{other}"),
        },
    }
}
impl CreatureLevelDefinitions {
    pub fn load(catalog: &mut crate::native::production_compiler::VerifiedCatalog) -> Result<Self> {
        Self::from_rows(
            catalog.creature_level_templates()?,
            catalog.creature_level_classes()?,
        )
    }
    pub fn from_rows(rows: Vec<SourceLevelTemplate>, mut raw: Vec<ClassLevelRow>) -> Result<Self> {
        let mut templates = BTreeMap::new();
        for row in rows {
            require(
                !templates.contains_key(&row.entry),
                "execution.creature.level_template_occurrence_owner",
            )?;
            templates.insert(row.entry, row);
        }
        // ObjectMgr.cpp:724-725 source query order, not catalog ordinal order.
        raw.sort_by_key(|r| (r.class, r.level));
        let mut classes = BTreeMap::new();
        for row in raw {
            if row.level == 0 || row.level > 70 {
                continue;
            }
            require(
                (1..=32).contains(&row.class),
                "execution.creature.LoadClassStats_class_shift_undefined",
            )?;
            if !matches!(row.class, 1 | 2 | 8) {
                continue;
            }
            let key = (class_index(row.class), u32::from(row.level));
            let loaded = init::load_source_class_level(row).map_err(level_refusal)?;
            classes.insert(key, loaded);
        }
        Ok(Self { templates, classes })
    }
    pub fn template(&self, entry: u32) -> Result<&SourceLevelTemplate> {
        self.templates
            .get(&entry)
            .ok_or_else(|| refuse("execution.creature.SelectLevel_template_owner"))
    }
    pub fn stats(
        &self,
        entry: u32,
        level: u32,
        forced: bool,
        config: &CreatureLevelConfig,
    ) -> Result<SourceLevelStats> {
        let t = self.template(entry)?;
        init::select_level_stats(
            t,
            self.classes.get(&(class_index(t.unit_class), level)),
            level,
            forced,
            config.rates(t.rank, false),
        )
        .map_err(level_refusal)
    }
    /// Called only at SelectLevelStats after InitEntry's model, equipment and
    /// swimming/hover callbacks. Events/groups already selected effective entry.
    pub fn resolve(
        &self,
        state: &CreatureConstruction,
        config: &CreatureLevelConfig,
        spawn: &init::SpawnTemplate,
        alive: bool,
    ) -> Result<SourceLevelStats> {
        let t = self.template(state.template.entry)?;
        require(
            state.model.is_some(),
            "execution.creature.InitEntry_model_owner",
        )?;
        require(
            t.levels == state.template.levels
                && state.level >= t.levels[0]
                && state.level <= t.levels[1],
            "execution.creature.SelectLevel_construction_operands",
        )?;
        require(
            state.rolls.level == (t.levels[0] != t.levels[1]).then_some(state.level),
            "execution.creature.SelectLevel_consumed_draw_owner",
        )?;
        let mut stats = self.stats(t.entry, state.level, false, config)?;
        stats.apply_spawn(
            spawn.current_health,
            spawn.current_mana,
            alive,
            t.unit_class,
            t.regenerate_stats,
        );
        Ok(stats)
    }
}
/// Creature.cpp:1278-1279. A fixed level does not even request a stream owner.
pub fn draw_source_creature_level(
    levels: [u32; 2],
    random: Option<&mut dyn CreatureRandom>,
) -> Result<u32> {
    if levels[0] == levels[1] {
        return Ok(levels[0]);
    }
    random
        .ok_or_else(|| refuse("execution.creature.SelectLevel_urand_thread_owner"))?
        .creature_urand(levels[0], levels[1])
}
impl SourceUnitFields {
    pub fn apply_source_level(&mut self, value: &SourceLevelStats, class: u32) {
        self.vitals = Some(SourceUnitVitals {
            health: value.health,
            max_health: value.maximum_health,
            mana: value.power[0],
            max_mana: value.maximum_power[0],
            has_mana: matches!(class, 2 | 8),
            energy: value.power[3],
            max_energy: value.maximum_power[3],
        });
    }
}
impl SourceUnits {
    pub fn initialize_creature_level(
        &mut self,
        objects: &SourceObjects,
        state: &CreatureConstruction,
        definitions: &CreatureLevelDefinitions,
        config: &CreatureLevelConfig,
        spawn: &init::SpawnTemplate,
    ) -> Result<SourceLevelStats> {
        let stats = definitions.resolve(state, config, spawn, objects.get(state.object)?.alive)?;
        let fields = self.get_mut(objects, state.object)?;
        require(
            fields.entry == state.template.entry,
            "execution.creature.SelectLevel_unit_entry",
        )?;
        fields.apply_source_level(&stats, definitions.template(fields.entry)?.unit_class);
        Ok(stats)
    }
}
#[cfg(test)]
#[path = "creature_level_tests.rs"]
mod tests;
