// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 SpellAuras.cpp:3617-3646,4953-4990;
// Object.cpp:1183-1190; Unit.cpp:539-550,10176-10191.
use super::*;
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsumableHolder {
    pub spell: u32,
    pub cast_id: u64,
    pub slot: u8,
    pub level: u8,
    pub applied_tick: u64,
    pub aura: spell::ConsumableAura,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoodDrinkState {
    pub holders: BTreeMap<u32, ConsumableHolder>,
    pub heartbeat_remaining_ms: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum FoodDrinkEvent {
    Cast {
        spell: u32,
        cast_id: u64,
        failure: Option<spell::CastFailure>,
    },
    Applied {
        spell: u32,
        slot: u8,
        duration_ms: u32,
    },
    Removed {
        spell: u32,
    },
    Stand {
        standing: bool,
        acknowledge: bool,
    },
    Health {
        before: u32,
        after: u32,
    },
    EatEmote,
    PlaySpellVisual {
        kit: u32,
    },
}
pub(in crate::native) fn source(
    profile: &Profile,
    id: u32,
) -> Result<&SpellAuthoritySource, NativeError> {
    profile
        .spell_location_catalog()
        .and_then(|c| c.consumable_source(id))
        .ok_or(NativeError::AuthorityUnavailable("food/drink spell source"))
}
pub(in crate::native) fn selected(profile: &Profile) -> bool {
    source(profile, 430).is_ok() && source(profile, 433).is_ok()
}
pub(in crate::native) fn amount(profile: &Profile, id: u32) -> Result<u32, NativeError> {
    source(profile, id)?.words[76]
        .checked_add(1)
        .ok_or(NativeError::InvalidProfile)
}
pub(in crate::native) fn aura(
    profile: &Profile,
    id: u32,
) -> Result<spell::ConsumableAura, NativeError> {
    let duration = source(profile, id)?
        .duration
        .ok_or(NativeError::InvalidProfile)?;
    Ok(spell::ConsumableAura::new(
        u32::try_from(duration.base_ms).map_err(|_| NativeError::InvalidProfile)?,
        if id == 433 { 5000 } else { 2000 },
    )?)
}
pub(in crate::native) fn resources(
    profile: &Profile,
    id: u32,
) -> Result<spell::InstantAuraResources, NativeError> {
    let raw = source(profile, id)?;
    let loot = profile
        .quest_progression
        .as_ref()
        .and_then(|q| q.loot.as_ref())
        .ok_or(NativeError::InvalidProfile)?;
    let item = loot.inventory.item(if id == 430 { 159 } else { 2070 })?;
    let placement =
        loot.starter_items
            .get(&item.entry)
            .ok_or(NativeError::AuthorityUnavailable(
                "starting consumable item requirements",
            ))?;
    if placement.allowable_class & 128 == 0
        || placement.allowable_race & 1 == 0
        || placement.required_level > 1
        || placement.required_skill != 0
        || placement.required_skill_rank != 0
        || placement.required_spell != 0
        || placement.required_honor_rank != 0
        || placement.required_reputation_faction != 0
    {
        return Err(NativeError::UnsupportedMechanic(
            "starting consumable use requirements",
        ));
    }
    let use_spell = item.spells[0];
    if item.class != 0
        || item.stackable != 20
        || use_spell.id != id
        || use_spell.trigger != 0
        || use_spell.charges != -1
        || use_spell.category != raw.words[2]
        || item.spells[1..].iter().any(|s| s.id != 0)
    {
        return Err(NativeError::UnsupportedMechanic(
            "starting consumable item source",
        ));
    }
    Ok(spell::InstantAuraResources {
        spell_id: id,
        mana_cost: 0,
        global_category: 0,
        global_recovery_ms: 0,
        recovery_ms: u32::try_from(use_spell.cooldown).unwrap_or(raw.words[19]),
        category: use_spell.category,
        category_recovery_ms: u32::try_from(use_spell.category_cooldown).unwrap_or(raw.words[20]),
    })
}
impl State {
    pub fn food_drink(&self) -> Option<&FoodDrinkState> {
        self.food_drink.as_ref()
    }
    pub(in crate::native) fn validate_food_drink(
        &self,
        profile: &Profile,
    ) -> Result<(), NativeError> {
        if selected(profile) != self.food_drink.is_some() {
            return Err(NativeError::InvalidState);
        }
        let Some(state) = &self.food_drink else {
            return Ok(());
        };
        let cadence = 5000u64.div_ceil(u64::from(profile.tick_ms));
        let expected_heartbeat = if self.tick == 0 {
            0
        } else {
            5000 - (((self.tick - 1) % cadence) as u32 * profile.tick_ms)
        };
        if state.heartbeat_remaining_ms != expected_heartbeat || state.holders.len() > 2 {
            return Err(NativeError::InvalidState);
        }
        for (&id, holder) in &state.holders {
            if holder.spell != id
                || holder.applied_tick == 0
                || holder.applied_tick > self.tick
                || holder.cast_id == 0
                || holder.cast_id >= self.next_cast_id
                || holder.slot > 2
                || holder.level == 0
                || holder.level > self.level(profile)
                || (holder.slot == 0 && self.frost_armor().is_some())
                || state
                    .holders
                    .values()
                    .any(|other| other.spell != id && other.slot == holder.slot)
                || !self.player_alive()
                || self.combat.as_ref().is_none_or(|c| c.standing)
            {
                return Err(NativeError::InvalidState);
            }
            if !holder.aura.matches_fixed_steps(
                &aura(profile, id)?,
                self.tick - holder.applied_tick,
                profile.tick_ms,
            ) {
                return Err(NativeError::InvalidState);
            }
        }
        Ok(())
    }
}
