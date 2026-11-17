// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Creature.cpp:2306-2346. Spell execution and Mount retain their own owners.
use super::*;
use std::collections::BTreeSet;

/// InitEntry sets sheath separately, Creature.cpp:512.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CreatureAddonState {
    pub mount: u32,
    pub stand: u8,
    pub sheath: u8,
    pub emote: u32,
    /// Unit.h:1647-1650 tests holder membership, not individual effects.
    pub aura_holders: BTreeSet<u32>,
    /// Unit.h:2507 default-constructs every per-type AuraList empty.
    pub aura_types: std::collections::BTreeMap<u32, Vec<u64>>,
    pub duplicate_aura_errors: Vec<u32>,
}

pub trait CreatureAddonExecutionOwner {
    fn mount(&mut self, _state: &mut CreatureAddonState, _display: u32) -> Result<()> {
        Err(refuse("execution.creature_addon.Creature_2313.Mount_owner"))
    }
    fn cast_spell(&mut self, _state: &mut CreatureAddonState, _spell: u32) -> Result<()> {
        Err(refuse(
            "execution.creature_addon.Creature_2343.CastSpell_owner",
        ))
    }
}

impl CreatureAddonState {
    pub fn has_aura(&self, spell: u32) -> bool {
        self.aura_holders.contains(&spell)
    }
    pub fn has_aura_type(&self, kind: u32) -> bool {
        self.aura_types
            .get(&kind)
            .is_some_and(|auras| !auras.is_empty())
    }
    pub fn load(
        &mut self,
        addon: Option<&LoadedCreatureAddon>,
        reload: bool,
        owner: &mut impl CreatureAddonExecutionOwner,
    ) -> Result<bool> {
        let Some(addon) = addon else { return Ok(false) };
        if addon.mount != 0 {
            owner.mount(self, addon.mount)?;
        }
        if addon.stand_state != 0 {
            self.stand = addon.stand_state;
        }
        self.sheath = addon.sheath_state;
        if addon.emote != 0 {
            self.emote = addon.emote;
        }
        match &addon.auras {
            CreatureAddonAuras::Empty => {}
            CreatureAddonAuras::Raw(_) => {
                return Err(refuse(
                    "execution.creature_addon.Creature_2333.converted_auras_owner",
                ));
            }
            CreatureAddonAuras::Spells(spells) => {
                for &spell in spells {
                    if spell == 0 {
                        return Ok(true);
                    }
                    if self.has_aura(spell) {
                        if !reload {
                            self.duplicate_aura_errors.push(spell);
                        }
                        continue;
                    }
                    // CastSpell owns all aura insertion and spell effects, not this caller.
                    owner.cast_spell(self, spell)?;
                }
                return Err(refuse(
                    "execution.creature_addon.Creature_2333.aura_terminator_out_of_bounds_undefined",
                ));
            }
        }
        Ok(true)
    }
}
