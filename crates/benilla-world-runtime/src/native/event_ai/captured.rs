// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, CreatureEventAIMgr.cpp:210-269,594-614; CreatureEventAI.cpp:102-163,676-730; ObjectMgr.cpp:7782-7880.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Bounded capture projection into the existing catalog, without admission.
use super::*;

#[derive(Clone, Debug, Serialize)]
pub struct EventAiCapturedOwner {
    pub spawn_id: u64,
    pub database_guid: u32,
    pub entry: u32,
    /// Captured creature.id, not an entry chosen by a pool or substitution.
    pub captured_entry: u32,
    pub gender: u8,
    pub name: String,
    pub name_locales: EventAiRows<EventAiNameLocale>,
}

#[derive(Clone, Debug, Serialize)]
pub struct EventAiCapturedInput {
    pub owners: EventAiRows<EventAiCapturedOwner>,
    /// Complete signed selectors queried without filtering event fields.
    pub selectors: EventAiRows<i32>,
    /// Captured global ORDER BY id, before splitting entry and GUID lists.
    pub events: EventAiRows<EventAiEventSource>,
    pub broadcasts: EventAiRows<EventAiBroadcastSource>,
    pub locales: EventAiRows<EventAiBroadcastLocale>,
}

impl EventAiCapturedInput {
    pub fn compile(self) -> Result<EventAiCatalog, NativeError> {
        encoded_budget(&self)?;
        let owners = self.owners.require(crate::MAX_NPCS)?;
        let selectors = self.selectors.require(crate::MAX_NPCS * 2)?;
        let events = self.events.require(crate::MAX_NPCS * MAX_EVENTS)?;
        let mut expected = BTreeSet::new();
        for owner in owners {
            if owner.entry == 0
                || owner.entry != owner.captured_entry
                || owner.entry > 0xff_ffff
                || owner.database_guid == 0
                || owner.database_guid > i32::MAX as u32
            {
                return Err(NativeError::UnsupportedMechanic(
                    "EventAI captured static entry scope",
                ));
            }
            expected.insert(owner.entry as i32);
            expected.insert(-(owner.database_guid as i32));
        }
        if selectors.len() != expected.len() || !selectors.iter().copied().eq(expected) {
            return Err(NativeError::AuthorityBinding);
        }
        if events.windows(2).any(|rows| rows[0].id >= rows[1].id)
            || events
                .iter()
                .any(|row| selectors.binary_search(&row.creature_id).is_err())
        {
            return Err(NativeError::AuthorityBinding);
        }
        let owners = owners
            .iter()
            .map(|owner| EventAiOwnerSource {
                spawn_id: owner.spawn_id,
                entry: owner.entry,
                database_guid: owner.database_guid,
                gender: owner.gender,
                name: owner.name.clone(),
                name_locales: owner.name_locales.clone(),
                // InitAI appends entry then negative GUID lists; CreatureEventAI.cpp:102-163.
                entry_events: EventAiRows::Complete(
                    events
                        .iter()
                        .filter(|row| row.creature_id == owner.entry as i32)
                        .cloned()
                        .collect(),
                ),
                guid_events: EventAiRows::Complete(
                    events
                        .iter()
                        .filter(|row| row.creature_id == -(owner.database_guid as i32))
                        .cloned()
                        .collect(),
                ),
            })
            .collect();
        EventAiCatalog::validate_sources(EventAiInput {
            owners: EventAiRows::Complete(owners),
            broadcasts: self.broadcasts,
            locales: self.locales,
        })
    }

    /// Unknown loaded configuration cannot be replaced with a default audience.
    pub fn compile_runtime(
        self,
        config: EventAiRows<EventAiRuntimeSource>,
    ) -> Result<EventAiRuntimeCatalog, NativeError> {
        let [config] = config.require(1)? else {
            return Err(NativeError::AuthorityUnavailable(
                "complete EventAI loaded configuration",
            ));
        };
        EventAiRuntimeCatalog::validate_sources(self.compile()?, config.clone())
    }
}
