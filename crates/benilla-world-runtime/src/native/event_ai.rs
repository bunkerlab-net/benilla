// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// CreatureEventAIMgr.cpp:210–269,594–614; CreatureEventAI.cpp:102–163,676–730;
// ObjectMgr.cpp:7782–7880,9978–10066; Object.cpp:1798–1847.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Retained bounded source inputs, not a raw content compiler or admission token.
use super::*;
use benilla_sim::event_ai::AggroText;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod runtime_catalog;
pub use runtime_catalog::*;
mod audience;
pub use audience::*;
mod runtime;
pub use runtime::*;

pub const EVENT_AI_POLICY: &str = "cmangos-8ec338a1-aggro-text-v1";
const MAX_EVENTS: usize = 32;
const MAX_TEXT_BYTES: usize = 16 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum EventAiRows<T> {
    Missing,
    Unknown,
    Complete(Vec<T>),
}
impl<T> EventAiRows<T> {
    fn require(&self, bound: usize) -> Result<&[T], NativeError> {
        match self {
            Self::Complete(rows) if rows.len() <= bound => Ok(rows),
            Self::Complete(_) => Err(NativeError::TooLarge),
            Self::Missing | Self::Unknown => Err(NativeError::AuthorityUnavailable(
                "complete EventAI source dependency",
            )),
        }
    }
}

/// Raw ACTION_T_TEXT parameters remain signed. Unsupported actions are rejected,
/// never cleared as a substitute for the loader's missing-dependency diagnostics.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiActionSource {
    pub action_type: u8,
    pub parameters: [i32; 3],
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiEventSource {
    pub id: u32,
    pub creature_id: i32,
    pub event_type: u8,
    pub inverse_phase_mask: i32,
    pub chance: u32,
    pub flags: u32,
    pub parameters: [i32; 6],
    pub actions: [EventAiActionSource; 3],
    pub comment: String,
}
impl EventAiEventSource {
    fn shape(&self) -> Result<AggroText, NativeError> {
        if self.event_type != 4
            || self.inverse_phase_mask != 0
            || self.flags != 0
            || self.parameters != [0; 6]
            || self.actions[0].action_type != 1
            || self.actions[1..]
                .iter()
                .any(|a| a.action_type != 0 || a.parameters != [0; 3])
        {
            return Err(NativeError::UnsupportedMechanic("bounded AGGRO text event"));
        }
        if self.comment.chars().count() > 255 || self.comment.len() > 1020 {
            return Err(NativeError::TooLarge);
        }
        AggroText::new(
            self.id,
            self.chance
                .try_into()
                .map_err(|_| NativeError::InvalidProfile)?,
            self.actions[0].parameters,
        )
        .map_err(NativeError::EventAi)
    }
}

/// The locale name, not the process-dependent storage-vector index. This set is
/// the pinned LocaleConstant domain; an empty/missing translation falls back.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum EventAiLocale {
    EnUs,
    KoKr,
    FrFr,
    DeDe,
    ZhCn,
    ZhTw,
    EsEs,
}
impl EventAiLocale {
    /// Exact GetLocaleByName domain; newer locale names are default in this core.
    pub fn from_source_name(name: &str) -> Self {
        match name {
            "koKR" => Self::KoKr,
            "frFR" => Self::FrFr,
            "deDE" => Self::DeDe,
            "zhCN" => Self::ZhCn,
            "zhTW" => Self::ZhTw,
            "esES" => Self::EsEs,
            _ => Self::EnUs,
        }
    }
    fn source_name(self) -> &'static str {
        match self {
            Self::EnUs => "enUS",
            Self::KoKr => "koKR",
            Self::FrFr => "frFR",
            Self::DeDe => "deDE",
            Self::ZhCn => "zhCN",
            Self::ZhTw => "zhTW",
            Self::EsEs => "esES",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiNameLocale {
    pub locale: EventAiLocale,
    pub name: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiOwnerSource {
    pub spawn_id: u64,
    pub entry: u32,
    /// Source database GUID, distinct from the runtime stable spawn key.
    pub database_guid: u32,
    pub gender: u8,
    pub name: String,
    pub name_locales: EventAiRows<EventAiNameLocale>,
    /// Loader ORDER BY id within each vector; InitAI appends entry then GUID.
    pub entry_events: EventAiRows<EventAiEventSource>,
    pub guid_events: EventAiRows<EventAiEventSource>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiBroadcastSource {
    pub id: i32,
    pub text: Option<String>,
    pub text1: Option<String>,
    pub chat_type: i32,
    pub language: i32,
    /// Retained metadata not consumed by the pinned broadcast loader.
    pub condition_id: i32,
    pub emotes_id: i32,
    pub flags: i32,
    pub sounds: [i32; 2],
    pub emote_ids: [i32; 3],
    pub emote_delays: [i32; 3],
    pub verified_build: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiBroadcastLocale {
    pub id: i32,
    /// Raw SQL name retained even when the pinned core maps it to the default.
    pub locale: String,
    pub text: Option<String>,
    pub text1: Option<String>,
    pub verified_build: i32,
}
#[derive(Clone, Debug, Serialize)]
pub struct EventAiInput {
    pub owners: EventAiRows<EventAiOwnerSource>,
    pub broadcasts: EventAiRows<EventAiBroadcastSource>,
    pub locales: EventAiRows<EventAiBroadcastLocale>,
}

/// Private immutable rows, deliberately without Deserialize or a public issuer.
/// ```compile_fail
/// use benilla_world_runtime::native::EventAiCatalog;
/// let _: EventAiCatalog = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiCatalog {
    policy: &'static str,
    owners: BTreeMap<u64, EventAiOwnerSource>,
    broadcasts: BTreeMap<i32, EventAiBroadcastSource>,
    locales: BTreeMap<i32, BTreeMap<String, EventAiBroadcastLocale>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EventAiLocalizedText {
    pub speaker_name: String,
    pub text: String,
    pub language: u32,
}
fn text_bound(text: &str, limit: usize) -> Result<(), NativeError> {
    if text.len() > limit {
        return Err(NativeError::TooLarge);
    }
    if text.contains('\0') {
        return Err(NativeError::UnsupportedMechanic(
            "embedded NUL EventAI text",
        ));
    }
    Ok(())
}
fn nullable_text_bound(text: &Option<String>) -> Result<(), NativeError> {
    text_bound(text.as_deref().unwrap_or(""), MAX_TEXT_BYTES)
}
fn encoded_budget(value: &impl Serialize) -> Result<(), NativeError> {
    struct Counter(usize);
    impl std::io::Write for Counter {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0 = self
                .0
                .checked_add(bytes.len())
                .filter(|n| *n <= MAX_BYTES)
                .ok_or_else(|| std::io::Error::other("EventAI source budget"))?;
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    serde_json::to_writer(Counter(0), value).map_err(|_| NativeError::TooLarge)
}
impl EventAiCatalog {
    pub fn validate_sources(input: EventAiInput) -> Result<Self, NativeError> {
        // Reject aggregate/escaped-byte amplification before cloning any rows.
        encoded_budget(&input)?;
        let mut owners = BTreeMap::new();
        let mut referenced = BTreeSet::new();
        let mut events = BTreeSet::new();
        let mut entries = BTreeSet::new();
        let mut database_guids = BTreeSet::new();
        let owner_rows = input.owners.require(2)?;
        if owner_rows.is_empty() {
            return Err(NativeError::InvalidProfile);
        }
        for owner in owner_rows {
            if owner.spawn_id <= SOLO_UNIT
                || owner.entry == 0
                || owner.entry > 0xff_ffff
                || owner.database_guid == 0
                || owner.database_guid > i32::MAX as u32
                || owner.gender > 2
                || owner.name.is_empty()
                || !entries.insert(owner.entry)
                || !database_guids.insert(owner.database_guid)
            {
                return Err(NativeError::InvalidProfile);
            }
            text_bound(&owner.name, 256)?;
            let mut name_locales = BTreeSet::new();
            for row in owner.name_locales.require(8)? {
                if row.locale == EventAiLocale::EnUs || !name_locales.insert(row.locale) {
                    return Err(NativeError::InvalidProfile);
                }
                text_bound(&row.name, 256)?;
            }
            let entry = owner.entry_events.require(MAX_EVENTS)?;
            let guid = owner.guid_events.require(MAX_EVENTS)?;
            if entry.len() + guid.len() > MAX_EVENTS {
                return Err(NativeError::TooLarge);
            }
            for (selector, rows) in [
                (owner.entry as i32, entry),
                (-(owner.database_guid as i32), guid),
            ] {
                if rows.windows(2).any(|w| w[0].id >= w[1].id) {
                    return Err(NativeError::InvalidProfile);
                }
                for event in rows {
                    if event.creature_id != selector || !events.insert(event.id) {
                        return Err(NativeError::InvalidProfile);
                    }
                    event.shape()?;
                    referenced.extend(event.actions[0].parameters.into_iter().filter(|id| *id > 0));
                }
            }
            if owners.insert(owner.spawn_id, owner.clone()).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        if events.is_empty() {
            return Err(NativeError::InvalidProfile);
        }
        let mut broadcasts = BTreeMap::new();
        for row in input.broadcasts.require(MAX_EVENTS * 6)? {
            if row.id <= 0 || !referenced.contains(&row.id) {
                return Err(NativeError::InvalidProfile);
            }
            if row.chat_type != 0
                || !matches!(row.language, 0 | 7)
                || row.sounds != [0; 2]
                || row.emote_ids != [0; 3]
                || row.emote_delays != [0; 3]
            {
                return Err(NativeError::UnsupportedMechanic(
                    "silent-effect monster SAY broadcast",
                ));
            }
            nullable_text_bound(&row.text)?;
            nullable_text_bound(&row.text1)?;
            if broadcasts.insert(row.id, row.clone()).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        if !referenced.iter().eq(broadcasts.keys()) {
            return Err(NativeError::AuthorityUnavailable(
                "complete EventAI broadcast rows",
            ));
        }
        let mut locales: BTreeMap<i32, BTreeMap<String, EventAiBroadcastLocale>> = BTreeMap::new();
        for row in input.locales.require(MAX_EVENTS * 6 * 9)? {
            if !broadcasts.contains_key(&row.id) {
                return Err(NativeError::InvalidProfile);
            }
            nullable_text_bound(&row.text)?;
            nullable_text_bound(&row.text1)?;
            text_bound(&row.locale, 40)?;
            if row.locale.chars().count() > 10 {
                return Err(NativeError::TooLarge);
            }
            if locales
                .entry(row.id)
                .or_default()
                .insert(row.locale.clone(), row.clone())
                .is_some()
            {
                return Err(NativeError::InvalidProfile);
            }
        }
        let catalog = Self {
            policy: EVENT_AI_POLICY,
            owners,
            broadcasts,
            locales,
        };
        encoded_budget(&catalog)?;
        Ok(catalog)
    }
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-event-ai-catalog-v1\0", self)
    }
    pub fn owner(&self, spawn: u64) -> Option<&EventAiOwnerSource> {
        self.owners.get(&spawn)
    }
    pub fn events(&self, spawn: u64) -> Result<Vec<AggroText>, NativeError> {
        let owner = self.owner(spawn).ok_or(NativeError::InvalidProfile)?;
        owner
            .entry_events
            .require(MAX_EVENTS)?
            .iter()
            .chain(owner.guid_events.require(MAX_EVENTS)?)
            .map(EventAiEventSource::shape)
            .collect()
    }
    /// DoDisplayText's male-default override happens BEFORE per-recipient locale
    /// fallback. It intentionally differs from BroadcastText::GetText.
    pub fn localized(
        &self,
        spawn: u64,
        broadcast: i32,
        locale: EventAiLocale,
    ) -> Result<EventAiLocalizedText, NativeError> {
        let gender = self.owner(spawn).ok_or(NativeError::InvalidProfile)?.gender;
        self.localized_for_gender(spawn, broadcast, locale, gender)
    }
    /// Localize against the caller's retained generation model selection.
    /// This computes presentation only; it grants no event or audience authority.
    pub fn localized_for_gender(
        &self,
        spawn: u64,
        broadcast: i32,
        locale: EventAiLocale,
        gender: u8,
    ) -> Result<EventAiLocalizedText, NativeError> {
        if gender > 2 {
            return Err(NativeError::InvalidState);
        }
        let owner = self.owner(spawn).ok_or(NativeError::InvalidProfile)?;
        if !owner
            .entry_events
            .require(MAX_EVENTS)?
            .iter()
            .chain(owner.guid_events.require(MAX_EVENTS)?)
            .any(|event| event.actions[0].parameters.contains(&broadcast))
        {
            return Err(NativeError::AuthorityBinding);
        }
        let base = self
            .broadcasts
            .get(&broadcast)
            .ok_or(NativeError::InvalidProfile)?;
        let male = base.text.as_deref().unwrap_or("");
        let female = base.text1.as_deref().unwrap_or("");
        let female_selected = gender != 0 && !female.is_empty() && male.is_empty();
        let default = if female_selected { female } else { male };
        // enUS is the default vector index, never a localized override. The
        // loader observes but ignores an enUS row (storage locale index -1).
        let translated = (locale != EventAiLocale::EnUs)
            .then(|| {
                self.locales
                    .get(&broadcast)
                    .and_then(|rows| rows.get(locale.source_name()))
            })
            .flatten()
            .and_then(|row| {
                if female_selected {
                    row.text1.as_deref()
                } else {
                    row.text.as_deref()
                }
            })
            .filter(|s| !s.is_empty())
            .unwrap_or(default);
        let name = owner
            .name_locales
            .require(8)?
            .iter()
            .find(|r| r.locale == locale && !r.name.is_empty())
            .map_or(owner.name.as_str(), |r| r.name.as_str());
        Ok(EventAiLocalizedText {
            speaker_name: name.into(),
            text: translated.into(),
            language: base.language as u32,
        })
    }
}

#[cfg(test)]
mod tests;
