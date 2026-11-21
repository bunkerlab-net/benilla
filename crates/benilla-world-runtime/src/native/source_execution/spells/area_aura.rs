// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! SpellAuras.cpp:298-328,410-634. Construction and the later update boundary.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceAreaAuraKind {
    Party,
    Pet,
}

impl SourceAreaAuraKind {
    // SpellMgr.h:895-901.
    pub(super) fn for_effect(effect: u32) -> Option<Self> {
        match effect {
            35 => Some(Self::Party),
            119 => Some(Self::Pet),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAreaAura {
    pub kind: SourceAreaAuraKind,
    pub radius_bits: u32,
    pub original_rank_spell: Option<u32>,
}

impl SourceAreaAura {
    // SpellAuras.cpp:302-327. Aura's base constructor already ran.
    pub(super) fn attach(
        aura: &mut SourceAura,
        kind: SourceAreaAuraKind,
        stores: &SourceSpellStores,
        radius_index: u32,
        mod_owner: impl FnOnce() -> Result<Option<ObjectHandle>>,
        totem: bool,
    ) -> Result<()> {
        let radius_bits = stores.spell_radius_bits(radius_index)?;
        require(
            mod_owner()?.is_none(),
            "execution.spells.AreaAura_309.ApplySpellMod_radius_owner",
        )?;
        aura.area = Some(Self {
            kind,
            radius_bits,
            original_rank_spell: None,
        });
        if totem {
            aura.kind = 0;
        }
        Ok(())
    }

    // SpellAuras.cpp:5675-5677 dispatches after the holder's skip-tick check.
    pub fn update(&self, _diff: u32) -> Result<()> {
        Err(refuse("execution.spells.AreaAura_410.Update_owner"))
    }
}

impl SourceSpellStores {
    // DBCStructure.h SpellRadiusEntry; SpellMgr.h:100-103 gives zero on null.
    pub(super) fn spell_radius_bits(&self, index: u32) -> Result<u32> {
        self.dbc("SpellRadius.dbc", index)?.map_or(Ok(0), |row| {
            row.words
                .get(&1)
                .copied()
                .ok_or_else(|| refuse("execution.spells.SpellRadius_dbc_column_owner"))
        })
    }
}

#[cfg(test)]
#[path = "area_aura_tests.rs"]
mod tests;
