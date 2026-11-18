// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Creature.cpp:2306-2346. Spell execution and Mount retain their own owners.
use super::*;
use std::collections::BTreeMap;

/// InitEntry sets sheath separately, Creature.cpp:512.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CreatureAddonState {
    pub mount: u32,
    pub stand: u8,
    pub sheath: u8,
    pub emote: u32,
    pub duplicate_aura_errors: Vec<u32>,
}

/// Unit.h:1076. Order by spell and then by insertion into this unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceAuraHolderId {
    spell: u32,
    ordinal: u64,
}

impl SourceAuraHolderId {
    pub fn spell(self) -> u32 {
        self.spell
    }
}

/// Unit.h:1080. A type-list entry identifies an effect on a specific holder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAuraEffectRef {
    pub holder: SourceAuraHolderId,
    pub effect: u8,
}

/// Unit.h:2487,2507; Unit.cpp:355,365-366, fresh live Unit storage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceUnitAuras {
    aura_holders: BTreeMap<SourceAuraHolderId, SourceAuraHolder>,
    last_holder_ordinal: u64,
    /// Unit.h:2507 default-constructs every per-type AuraList empty.
    aura_types: BTreeMap<u32, Vec<SourceAuraEffectRef>>,
    /// Object.cpp:1173 constructs the triggered cast depth at zero.
    pub cast_counter: u32,
    /// Unit.cpp:330-366, fresh UNIT_FIELD_BYTES_1 visibility byte.
    pub vis_flags: u8,
    pub spell_procs_happening: bool,
    pub aura_fields: SourceAuraFields,
    pub heartbeat_proc_counter: u32,
}

pub trait CreatureAddonExecutionOwner {
    fn mount(&mut self, _state: &mut CreatureAddonState, _display: u32) -> Result<()> {
        Err(refuse("execution.creature_addon.Creature_2313.Mount_owner"))
    }
    fn cast_spell(
        &mut self,
        _state: &mut CreatureAddonState,
        _unit: &mut SourceUnitFields,
        _spell: u32,
    ) -> Result<()> {
        Err(refuse(
            "execution.creature_addon.Creature_2343.CastSpell_owner",
        ))
    }
}

impl SourceUnitAuras {
    pub fn aura_holders(
        &self,
    ) -> impl ExactSizeIterator<Item = (SourceAuraHolderId, &SourceAuraHolder)> {
        self.aura_holders.iter().map(|(&id, holder)| (id, holder))
    }

    pub fn has_aura_holders(&self) -> bool {
        !self.aura_holders.is_empty()
    }

    pub fn aura_spell_ids(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.aura_holders.keys().map(|id| id.spell)
    }

    pub fn aura_type_ids(&self) -> impl ExactSizeIterator<Item = u32> + '_ {
        self.aura_types.keys().copied()
    }

    pub fn aura_holder(&self, id: SourceAuraHolderId) -> Option<&SourceAuraHolder> {
        self.aura_holders.get(&id)
    }

    pub fn aura_holder_mut(&mut self, id: SourceAuraHolderId) -> Option<&mut SourceAuraHolder> {
        self.aura_holders.get_mut(&id)
    }

    // Unit.cpp:10907-10910. find returns the first holder for this spell.
    pub fn first_aura_holder(&self, spell: u32) -> Option<&SourceAuraHolder> {
        let first = SourceAuraHolderId { spell, ordinal: 0 };
        self.aura_holders
            .range(first..)
            .next()
            .filter(|(id, _)| id.spell == spell)
            .map(|(_, holder)| holder)
    }

    pub fn auras_of_type(&self, kind: u32) -> &[SourceAuraEffectRef] {
        self.aura_types.get(&kind).map_or(&[], Vec::as_slice)
    }

    // Unit.cpp:4849-4853,4871-4874. Insert the holder, then append each effect.
    pub(in crate::native::source_execution) fn store_aura_holder(
        &mut self,
        spell: u32,
        pending: &mut Option<SourceAuraHolder>,
    ) -> Result<SourceAuraHolderId> {
        let ordinal = self
            .last_holder_ordinal
            .checked_add(1)
            .ok_or_else(|| refuse("execution.spells.Unit_4849.aura_holder_identity_exhausted"))?;
        let id = SourceAuraHolderId { spell, ordinal };
        let holder = pending
            .take()
            .ok_or_else(|| refuse("execution.spells.Unit_4849.pending_aura_holder_owner"))?;
        self.last_holder_ordinal = ordinal;
        let entry = self.aura_holders.entry(id).insert_entry(holder);
        for (i, aura) in entry.get().effects.iter().enumerate() {
            if let Some(aura) = aura {
                if aura.kind < 192 {
                    self.aura_types
                        .entry(aura.kind)
                        .or_default()
                        .push(SourceAuraEffectRef {
                            holder: id,
                            effect: i as u8,
                        });
                }
            }
        }
        Ok(id)
    }

    // Unit.h:1647-1650. Membership tests the holder, not its effects.
    pub fn has_aura(&self, spell: u32) -> bool {
        self.first_aura_holder(spell).is_some()
    }
    pub fn has_aura_type(&self, kind: u32) -> bool {
        !self.auras_of_type(kind).is_empty()
    }
}
impl CreatureAddonState {
    pub fn load(
        &mut self,
        addon: Option<&LoadedCreatureAddon>,
        reload: bool,
        unit: &mut SourceUnitFields,
        owner: &mut impl CreatureAddonExecutionOwner,
    ) -> Result<bool> {
        let Some(addon) = addon else { return Ok(false) };
        if addon.mount != 0 {
            owner.mount(self, addon.mount)?;
        }
        if addon.stand_state != 0 {
            self.stand = addon.stand_state;
            unit.stand_state = addon.stand_state;
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
                    if unit.auras.has_aura(spell) {
                        if !reload {
                            self.duplicate_aura_errors.push(spell);
                        }
                        continue;
                    }
                    // CastSpell owns all aura insertion and spell effects, not this caller.
                    owner.cast_spell(self, unit, spell)?;
                }
                return Err(refuse(
                    "execution.creature_addon.Creature_2333.aura_terminator_out_of_bounds_undefined",
                ));
            }
        }
        Ok(true)
    }
}

#[test]
fn storing_without_a_pending_holder_refuses_without_consuming_identity() {
    let mut auras = SourceUnitAuras::default();
    assert_eq!(
        auras.store_aura_holder(90, &mut None).unwrap_err().id,
        "execution.spells.Unit_4849.pending_aura_holder_owner"
    );
    assert_eq!(auras, SourceUnitAuras::default());
}

#[cfg(test)]
#[path = "aura_holder_tests.rs"]
mod aura_holder_tests;
#[cfg(test)]
pub(in crate::native::source_execution) use aura_holder_tests::authored_effect_holder;
