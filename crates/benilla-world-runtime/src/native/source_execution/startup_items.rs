// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! World.cpp:1003,1006 startup prefix. No terrain constructor is synthesized.
use super::*;
use benilla_sim::source_rng::{RawGenerator, SourceRandom, TraceEvent};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupItemValidation {
    pub entry: u32,
    pub original_property: u32,
    pub selected_enchantment: u32,
    pub effective_property: u32,
    pub raw_begin: u64,
    pub raw_end: u64,
}
#[derive(Default, Debug)]
pub struct SourceStartupItems {
    enchantments: BTreeMap<u32, Vec<(u32, f32)>>,
    pub definitions: usize,
    pub items_validated: usize,
    pub validations: Vec<StartupItemValidation>,
}
impl SourceStartupItems {
    /// Retain query order within each entry, ItemEnchantmentMgr.cpp:48-71.
    pub fn load_enchantments(rows: impl IntoIterator<Item = (u32, u32, f32)>) -> Self {
        let mut this = Self::default();
        for (entry, ench, chance) in rows {
            this.definitions += 1;
            if chance > 0.000001_f32 && chance <= 100.0 {
                this.enchantments
                    .entry(entry)
                    .or_default()
                    .push((ench, chance));
            }
        }
        this
    }
    /// The missing-entry path returns zero without a draw, :83-90.
    pub fn enchantment<E: RawGenerator, S: FnMut(TraceEvent)>(
        &self,
        entry: u32,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<u32> {
        if entry == 0 {
            return Ok(0);
        }
        let Some(list) = self.enchantments.get(&entry) else {
            return Ok(0);
        };
        let roll = random
            .rand_chance()
            .map_err(|_| refuse("execution.startup_items.rand_chance"))?;
        let mut total = 0.0_f32;
        for &(ench, chance) in list {
            total += chance;
            if f64::from(total) > roll {
                return Ok(ench);
            }
        }
        // :105 multiplies in float, converts to int, adds one, then divides as integers.
        let bound = f64::from(total * 100.0).floor();
        require(
            bound.is_finite() && bound >= 0.0 && bound < f64::from(i32::MAX),
            "execution.startup_items.fallback_bound_undefined",
        )?;
        let roll = f64::from(
            random
                .irand(0, bound as i32 + 1)
                .map_err(|_| refuse("execution.startup_items.irand"))?
                / 100,
        );
        total = 0.0;
        for &(ench, chance) in list {
            total += chance;
            if f64::from(total) > roll {
                return Ok(ench);
            }
        }
        Ok(0)
    }
    /// Ascending storage IDs and sampled-result validation, ObjectMgr.cpp:2563,2769-2773.
    pub fn validate<E: RawGenerator, S: FnMut(TraceEvent)>(
        &mut self,
        items: BTreeMap<u32, u32>,
        properties: &BTreeSet<u32>,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<()> {
        for (entry, property) in items {
            if entry == 0 {
                continue;
            }
            self.items_validated += 1;
            if property == 0 {
                continue;
            }
            let raw_begin = random.cursor().raw;
            let selected = self.enchantment(property, random)?;
            self.validations.push(StartupItemValidation {
                entry,
                original_property: property,
                selected_enchantment: selected,
                effective_property: if properties.contains(&selected) {
                    property
                } else {
                    0
                },
                raw_begin,
                raw_end: random.cursor().raw,
            });
        }
        Ok(())
    }
    /// Catalog order is diagnostic, not a sealed F1 occurrence binding.
    pub fn catalog_prefix<E: RawGenerator, S: FnMut(TraceEvent)>(
        catalog: &mut super::super::production_compiler::VerifiedCatalog,
        random: &mut SourceRandom<'_, E, S>,
    ) -> Result<Self> {
        require(
            random.cursor().raw == 0,
            "execution.startup_items.initial_world_thread_position",
        )?;
        let mut this = Self::load_enchantments(catalog.execution_item_enchantments()?);
        let items = catalog.execution_item_random_properties()?;
        let mut dbc = catalog.dbc_view("ItemRandomProperties.dbc")?;
        let mut properties = BTreeSet::new();
        for ordinal in 0..dbc.record_count() {
            let record = dbc.record(ordinal)?;
            properties.insert(
                record["key"]
                    .as_u64()
                    .ok_or_else(|| refuse("execution.startup_items.dbc_key"))?
                    as u32,
            );
        }
        this.validate(items, &properties, random)?;
        Ok(this)
    }
    /// Keep the borrowed F3 stream. The first terrain lookup stops this prefix.
    pub fn bind_spawns<'a, 'b, E: RawGenerator, S: FnMut(TraceEvent)>(
        self,
        random: &'a mut SourceRandom<'b, E, S>,
    ) -> Result<ItemStartupSpawnOwner<'a, 'b, E, S>> {
        require(
            random.cursor().raw == self.validations.last().map_or(0, |v| v.raw_end),
            "execution.startup_items.continuing_world_thread_position",
        )?;
        Ok(ItemStartupSpawnOwner {
            items: self,
            random,
            stopped: false,
        })
    }
}
/// Diagnostic source prefix only. It cannot certify F1 SQL occurrence identity.
pub struct ItemStartupSpawnOwner<'a, 'b, E, S> {
    pub items: SourceStartupItems,
    random: &'a mut SourceRandom<'b, E, S>,
    stopped: bool,
}
impl<E: RawGenerator, S: FnMut(TraceEvent)> CreatureRandom for ItemStartupSpawnOwner<'_, '_, E, S> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        require(
            !self.stopped,
            "execution.startup_items.stopped_at_terrain_owner",
        )?;
        self.random.creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        require(
            !self.stopped,
            "execution.startup_items.stopped_at_terrain_owner",
        )?;
        self.random.creature_urand(min, max)
    }
}
impl<E: RawGenerator, S: FnMut(TraceEvent)> EffectiveSpawnOwner
    for ItemStartupSpawnOwner<'_, '_, E, S>
{
    fn startup_random(&mut self) -> Result<&mut dyn CreatureRandom> {
        require(
            !self.stopped,
            "execution.startup_items.stopped_at_terrain_owner",
        )?;
        Ok(self)
    }
    fn original_zone(&mut self, _: u32, _: [f32; 3]) -> Result<u32> {
        self.stopped = true;
        Err(refuse(
            "execution.startup_items.terrain_info_constructor_rng_owner",
        ))
    }
}
#[cfg(test)]
#[path = "startup_items_tests.rs"]
mod tests;
