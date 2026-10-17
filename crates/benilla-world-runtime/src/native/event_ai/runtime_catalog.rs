// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Creature.cpp:609-612, Creature.h:639-641,
// CreatureAI.cpp:53-64, UnitAI.cpp:104-112 and Object.cpp:1831-1846.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Explicit runtime selection of the bounded text catalog and source exclusions.
use super::*;

pub const EVENT_AI_RUNTIME_CATALOG_POLICY: &str = "cmangos-8ec338a1-native-aggro-text-catalog-v2";
const GUARD: u32 = 0x0000_0400;

/// The only supported session has one player with its own body as its camera.
/// This source policy does not replace complete callback-time camera evidence.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventAiSessionPolicy {
    SoloSelfCamera,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiRuntimeOwnerSource {
    pub spawn_id: u64,
    pub entry: u32,
    /// CreatureInfo::Civilian is independent of the ExtraFlags civilian bit.
    pub civilian: u32,
    /// Retained complete ExtraFlags, bound to the existing combat source catalog.
    pub extra_flags: u32,
    pub spell_list_id: u32,
    /// Creature::InitEntry resolves template zero to entry * 100, not an empty list.
    pub resolved_spell_list_id: u32,
    /// Complete effective GetSpellList().Spells after that lookup and creation.
    /// Neither a zero template ID nor an unknown/missing lookup proves emptiness.
    pub resolved_spells: EventAiRows<u32>,
}
impl EventAiRuntimeOwnerSource {
    fn validate(&self) -> Result<(), NativeError> {
        if self.spawn_id <= SOLO_UNIT || self.entry == 0 || self.entry > 0xff_ffff {
            return Err(NativeError::InvalidProfile);
        }
        if self.civilian != 0 || self.extra_flags & GUARD != 0 || self.spell_list_id != 0 {
            return Err(NativeError::UnsupportedMechanic(
                "EventAI base combat civilian, guard or spell-list source",
            ));
        }
        if self.resolved_spell_list_id
            != self
                .entry
                .checked_mul(100)
                .ok_or(NativeError::InvalidProfile)?
        {
            return Err(NativeError::AuthorityBinding);
        }
        if !self.resolved_spells.require(4096)?.is_empty() {
            return Err(NativeError::UnsupportedMechanic(
                "complete empty EventAI base combat spell list",
            ));
        }
        Ok(())
    }
}

/// Effective runtime configuration and explicit source closure. Values are not
/// inferred from the combat victim or from the last projected visible-NPC set.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiRuntimeSource {
    pub session: EventAiSessionPolicy,
    pub locale: EventAiLocale,
    /// Effective ListenRange.Say after configuration parsing, before visibility scaling.
    pub configured_say_range: ai::Real,
    /// Source GetVisibilityData().GetVisibilityDistance for the admitted speakers.
    pub visibility_distance: ai::Real,
    pub owners: EventAiRows<EventAiRuntimeOwnerSource>,
}
impl EventAiRuntimeSource {
    fn validate(&self, catalog: &EventAiCatalog) -> Result<(), NativeError> {
        let range = self.configured_say_range.as_f32();
        let visibility = self.visibility_distance.as_f32();
        // setConfigPos permits zero; the upper bound keeps the source's x4
        // multiplication inside Real's existing finite arithmetic domain.
        if !(0.0..=250_000_000_000.0).contains(&range) || visibility < 0.0 {
            return Err(NativeError::UnsupportedMechanic(
                "bounded EventAI SAY configuration",
            ));
        }
        let owners = self.owners.require(crate::MAX_NPCS)?;
        if owners.is_empty() || owners.len() != catalog.owners.len() {
            return Err(NativeError::InvalidProfile);
        }
        let mut spawns = BTreeSet::new();
        let mut entries: BTreeMap<u32, &EventAiRuntimeOwnerSource> = BTreeMap::new();
        for row in owners {
            row.validate()?;
            let owner = catalog
                .owner(row.spawn_id)
                .ok_or(NativeError::AuthorityBinding)?;
            if row.entry != owner.entry || !spawns.insert(row.spawn_id) {
                return Err(NativeError::AuthorityBinding);
            }
            if let Some(shared) = entries.insert(row.entry, row) {
                if shared.civilian != row.civilian
                    || shared.extra_flags != row.extra_flags
                    || shared.spell_list_id != row.spell_list_id
                    || shared.resolved_spell_list_id != row.resolved_spell_list_id
                    || shared.resolved_spells != row.resolved_spells
                {
                    return Err(NativeError::AuthorityBinding);
                }
            }
        }
        if !spawns.iter().eq(catalog.owners.keys()) {
            return Err(NativeError::AuthorityBinding);
        }
        // DoDisplayText overwrites its female selection whenever default male
        // text is nonempty. This is the actual source proof that an untracked
        // runtime model gender cannot alter presentation in this bounded branch.
        if catalog
            .broadcasts
            .values()
            .any(|row| row.text.as_deref().is_none_or(str::is_empty))
        {
            return Err(NativeError::UnsupportedMechanic(
                "EventAI runtime gender-independent male-default text",
            ));
        }
        Ok(())
    }
}

/// Immutable explicit execution selection. This is not production admission.
/// ```compile_fail
/// use benilla_world_runtime::native::EventAiRuntimeCatalog;
/// let _: EventAiRuntimeCatalog = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiRuntimeCatalog {
    policy: &'static str,
    catalog: EventAiCatalog,
    config: EventAiRuntimeSource,
}
impl EventAiRuntimeCatalog {
    pub fn validate_sources(
        catalog: EventAiCatalog,
        config: EventAiRuntimeSource,
    ) -> Result<Self, NativeError> {
        encoded_budget(&(&catalog, &config))?;
        config.validate(&catalog)?;
        let result = Self {
            policy: EVENT_AI_RUNTIME_CATALOG_POLICY,
            catalog,
            config,
        };
        encoded_budget(&result)?;
        Ok(result)
    }
    pub fn catalog(&self) -> &EventAiCatalog {
        &self.catalog
    }
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-event-ai-runtime-catalog-v1\0", self)
    }
    pub fn locale(&self) -> EventAiLocale {
        self.config.locale
    }
    pub fn say_range(&self) -> ai::Real {
        self.config.configured_say_range
    }
    pub fn visibility_distance(&self) -> ai::Real {
        self.config.visibility_distance
    }
    pub fn session_policy(&self) -> EventAiSessionPolicy {
        self.config.session
    }
    pub fn owner(&self, spawn: u64) -> Option<&EventAiRuntimeOwnerSource> {
        let EventAiRows::Complete(owners) = &self.config.owners else {
            return None;
        };
        owners.iter().find(|row| row.spawn_id == spawn)
    }
    pub(in crate::native) fn owner_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.catalog.owners.keys().copied()
    }
    pub(in crate::native) fn validate_scene(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != EVENT_AI_RUNTIME_CATALOG_POLICY {
            return Err(NativeError::InvalidProfile);
        }
        self.config.validate(&self.catalog)?;
        if !self
            .owner_ids()
            .eq(profile.npcs.iter().map(|npc| npc.ai.creature_id))
        {
            return Err(NativeError::AuthorityBinding);
        }
        for npc in &profile.npcs {
            let owner = self
                .catalog
                .owner(npc.ai.creature_id)
                .ok_or(NativeError::AuthorityBinding)?;
            let source = self
                .owner(npc.ai.creature_id)
                .ok_or(NativeError::AuthorityBinding)?;
            if owner.entry != npc.entry
                || owner.name != npc.name
                || source.extra_flags
                    != profile.source_pathfinding_extra_flags(npc.ai.creature_id)?
            {
                return Err(NativeError::AuthorityBinding);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "runtime_catalog_tests.rs"]
mod tests;
