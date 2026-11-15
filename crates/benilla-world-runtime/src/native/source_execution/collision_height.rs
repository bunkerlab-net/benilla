// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Unit.cpp:11930-11957 and Creature.cpp:439-442, native-display collision height.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::creature_capabilities::MOVEFLAG_SWIMMING;
use std::collections::BTreeMap;

/// The existing pinned DbcView owns layouts and last-physical duplicate lookup.
#[derive(Debug, Default)]
pub struct SourceCollisionModels {
    displays: BTreeMap<u32, u32>,
    heights: BTreeMap<u32, u32>,
}
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SourceCreatureSwimmingCensus {
    pub reached_441: usize,
    pub swimming_set: usize,
    pub swimming_not_set: usize,
    pub display_assert_11952: usize,
    pub model_assert_11954: usize,
}
impl SourceCollisionModels {
    /// Consumes ID/ModelId and ID/CollisionHeight, DBCStructure.h:223-224,263-278.
    pub fn load(catalog: &mut VerifiedCatalog) -> Result<Self> {
        fn word(row: &serde_json::Value, column: u32) -> Result<u32> {
            row["values"]
                .as_array()
                .and_then(|values| values.iter().find(|value| value[0] == column))
                .and_then(|value| value[2].as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .ok_or_else(|| refuse("execution.collision_height.dbc_layout"))
        }
        let mut this = Self::default();
        let mut view = catalog.dbc_view("CreatureDisplayInfo.dbc")?;
        for ordinal in 0..view.record_count() {
            let row = view.record(ordinal)?;
            this.displays.insert(word(&row, 0)?, word(&row, 1)?);
        }
        drop(view);
        let mut view = catalog.dbc_view("CreatureModelData.dbc")?;
        for ordinal in 0..view.record_count() {
            let row = view.record(ordinal)?;
            this.heights.insert(word(&row, 0)?, word(&row, 15)?);
        }
        Ok(this)
    }

    /// Unit.cpp:11932,11951-11957. Neither DBC scale nor current display participates.
    pub fn collision_height(&self, native_display: u32, object_scale: f32) -> Result<f32> {
        let model = self
            .displays
            .get(&native_display)
            .ok_or_else(|| refuse("execution.collision_height.Unit_11952.display_info_assert"))?;
        let height = self
            .heights
            .get(model)
            .ok_or_else(|| refuse("execution.collision_height.Unit_11954.model_data_assert"))?;
        let height = object_scale * f32::from_bits(*height);
        // ObjectDefines.h:47. Equality includes both signed zeros, not NaN.
        Ok(if height == 0.0 { 2.03128 } else { height })
    }

    /// Called only after Creature.cpp:439-440. A refusal leaves MovementInfo unchanged.
    pub fn init_entry_swimming(
        &self,
        native_display: u32,
        object_scale: f32,
        flags: &mut u32,
        swimmable: impl FnOnce(f32) -> Result<bool>,
        census: &mut SourceCreatureSwimmingCensus,
    ) -> Result<()> {
        census.reached_441 += 1;
        let height = self
            .collision_height(native_display, object_scale)
            .inspect_err(|error| match error.id.as_str() {
                "execution.collision_height.Unit_11952.display_info_assert" => {
                    census.display_assert_11952 += 1;
                }
                "execution.collision_height.Unit_11954.model_data_assert" => {
                    census.model_assert_11954 += 1;
                }
                _ => {}
            })?;
        if swimmable(height)? {
            *flags |= MOVEFLAG_SWIMMING;
            census.swimming_set += 1;
        } else {
            census.swimming_not_set += 1;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "collision_height_tests.rs"]
mod tests;
