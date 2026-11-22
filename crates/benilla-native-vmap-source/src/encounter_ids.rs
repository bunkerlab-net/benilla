// SPDX-License-Identifier: GPL-3.0-or-later
//! Source GenerateUniqueObjectId first-encounter semantics, bounded in memory.
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
//! contrib/vmap_extractor/vmapextract/vmapexport.cpp:84-87.
//! Ordered-container iteration is never used here: only lookup and insertion
//! affect the assigned ID. A duplicate returns its original ID without consuming
//! another ID, even when the distinct-key budget has been reached.
use crate::Error;
use std::collections::HashMap;

pub(crate) const POLICY: &str = "cmangos-client-u32-child-u16-first-encounter-v1";
pub(crate) const MAX_IDS: usize = 1_048_576;

#[derive(Default)]
pub(crate) struct EncounterIds {
    ids: HashMap<(u32, u16), u32>,
}
impl EncounterIds {
    pub(crate) fn intern(&mut self, client: u32, child: u16) -> Result<u32, Error> {
        self.intern_bounded(client, child, MAX_IDS)
    }
    fn intern_bounded(&mut self, client: u32, child: u16, limit: usize) -> Result<u32, Error> {
        if let Some(&id) = self.ids.get(&(client, child)) {
            return Ok(id);
        }
        if self.ids.len() >= limit {
            return Err(Error::EncounterIdLimit);
        }
        let id = u32::try_from(self.ids.len() + 1).map_err(|_| Error::EncounterIdLimit)?;
        self.ids
            .try_reserve(1)
            .map_err(|_| Error::AllocationFailed)?;
        self.ids.insert((client, child), id);
        Ok(id)
    }
    pub(crate) fn len(&self) -> usize {
        self.ids.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_keys_share_one_global_first_encounter_namespace() {
        let mut ids = EncounterIds::default();
        for (client, child, expected) in [
            (42, 0, 1),
            (42, 1, 2),
            (7, 0, 3),
            (42, 0, 1),
            (42, 2, 4),
            (7, 1, 5),
            (42, 1, 2),
            (u32::MAX, u16::MAX, 6),
        ] {
            assert_eq!(ids.intern(client, child).unwrap(), expected);
        }
        assert_eq!(ids.len(), 6);
    }

    #[test]
    fn duplicate_at_limit_does_not_allocate_or_advance() {
        let mut ids = EncounterIds::default();
        assert_eq!(ids.intern_bounded(8, 0, 1).unwrap(), 1);
        assert!(matches!(
            ids.intern_bounded(9, 0, 1),
            Err(Error::EncounterIdLimit)
        ));
        assert_eq!(ids.intern_bounded(8, 0, 1).unwrap(), 1);
        assert_eq!(ids.len(), 1);
    }

    #[test]
    fn source_unsigned_child_wrap_reuses_parent_key() {
        let mut ids = EncounterIds::default();
        assert_eq!(ids.intern(42, 0).unwrap(), 1);
        let mut ordinal = 0_u16;
        for expected in 2..=65_536 {
            ordinal = ordinal.wrapping_add(1);
            assert_eq!(ids.intern(42, ordinal).unwrap(), expected);
        }
        ordinal = ordinal.wrapping_add(1);
        assert_eq!(ordinal, 0);
        assert_eq!(ids.intern(42, ordinal).unwrap(), 1);
        assert_eq!(ids.len(), 65_536);
    }
}
