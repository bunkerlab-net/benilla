// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 SpellMgr::LoadSpellAreas and Player area/quest/aura queries.
//! Complete raw table scope is retained; no loader acceptance or predicate is guessed.
use super::*;

/// Exact SELECT columns, before loader validation and integer/bool narrowing.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SpellAreaSourceRow {
    pub spell: u32,
    pub area: u32,
    pub quest_start: u32,
    pub quest_start_active: u32,
    pub quest_end: u32,
    pub condition_id: u32,
    pub aura_spell: i32,
    pub racemask: u32,
    pub gender: u32,
    pub autocast: u32,
}
/// CompleteTable means the complete unfiltered raw SELECT, including every
/// unrelated row and exact order. This marker does not certify a raw producer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SpellAreaSourceRows {
    Unknown,
    CompleteTable(Vec<SpellAreaSourceRow>),
}
impl SpellAreaSourceRows {
    pub(in crate::native) fn complete(&self) -> Result<&[SpellAreaSourceRow], NativeError> {
        match self {
            Self::Unknown => Err(NativeError::AuthorityUnavailable(
                "complete spell_area table scope",
            )),
            Self::CompleteTable(rows) if rows.len() > 4096 => Err(NativeError::TooLarge),
            Self::CompleteTable(rows) => Ok(rows),
        }
    }
    pub(in crate::native) fn empty(&self) -> Result<(), NativeError> {
        if !self.complete()?.is_empty() {
            return Err(NativeError::UnsupportedMechanic(
                "spell_area source closure",
            ));
        }
        Ok(())
    }
}
impl SpellAuthorityCatalog {
    pub(in crate::native) fn requires_source_location(&self) -> bool {
        self.spells
            .values()
            .any(SpellAuthoritySource::requires_full_domain)
    }
    pub(in crate::native) fn spell_area_scope_binding(&self) -> Result<crate::Digest, NativeError> {
        self.modifier_hooks.spell_area.complete()?;
        self.modifier_spell_ids
            .complete("spell_area exact spell/aura scope")?;
        Ok(crate::digest(
            b"cmangos-8ec338a1-spell-area-query-scope-v1\0",
            &(&self.modifier_hooks.spell_area, &self.modifier_spell_ids),
        ))
    }
    pub(super) fn validate_area_sources(&self) -> Result<(), NativeError> {
        let table = &self.modifier_hooks.spell_area;
        table.complete()?;
        if self
            .spells
            .values()
            .any(|row| &row.hooks.spell_area != table)
        {
            return Err(NativeError::AuthorityBinding);
        }
        // Preserve the old fixture policy unless actual location lookup is selected
        // or the full-raw source domain makes it a mandatory execution dependency.
        if !self.requires_source_location() && self.source_location.is_none() {
            table.empty()?;
        }
        Ok(())
    }
    pub(in crate::native) fn validate_location_scope(
        &self,
        zone: u32,
        area: u32,
    ) -> Result<(), NativeError> {
        self.validate_area_sources()?;
        let SpellSourceRows::Complete(ids) = &self.modifier_spell_ids else {
            return Err(NativeError::AuthorityUnavailable(
                "spell_area exact spell/aura scope",
            ));
        };
        for row in self.modifier_hooks.spell_area.complete()? {
            // SpellMgr indexes ALL accepted rows by spell ID and nonzero aura
            // requirement, even when area==0. Its area index inserts only nonzero
            // keys: AddQuest/RewardQuest's additional index0 query is source-empty.
            // Any possibly reachable raw row remains closed rather than inventing
            // its loader disposition, condition, quest or IsFitToRequirements result.
            if ids.binary_search(&row.spell).is_ok()
                || (row.aura_spell != 0
                    && ids.binary_search(&row.aura_spell.unsigned_abs()).is_ok())
                || (row.area != 0 && (row.area == zone || row.area == area))
            {
                return Err(NativeError::UnsupportedMechanic(
                    "applicable spell_area source row",
                ));
            }
        }
        Ok(())
    }
}
