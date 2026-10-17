// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, CreatureAISelector.cpp:38-90, CreatureEventAI.cpp:
// 51-62/149-162/1634-1664, CreatureAI.cpp:33-46 and UnitAI.cpp:51/68-101.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Immutable authored selection of a complete bounded outdoor visitor scene.
//! It proves no real map traversal or production source completeness.
use super::*;
use serde::Serialize;
use std::collections::BTreeSet;

pub const SOURCE_VISITOR_POLICY: &str = "cmangos-8ec338a1-complete-source-visitors-wander-v5";

/// A producer must explicitly classify every applicable row. Unknown or missing
/// coverage cannot supply an empty EventAI or override lookup.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum SourceVisitorRows<T> {
    Missing,
    Unknown,
    Complete(Vec<T>),
}
impl<T> SourceVisitorRows<T> {
    fn require(&self, dependency: &'static str) -> Result<&[T], NativeError> {
        match self {
            Self::Complete(rows) => Ok(rows),
            Self::Missing | Self::Unknown => Err(NativeError::AuthorityUnavailable(dependency)),
        }
    }
    fn require_empty(&self, dependency: &'static str) -> Result<(), NativeError> {
        if !self.require(dependency)?.is_empty() {
            return Err(NativeError::UnsupportedMechanic(dependency));
        }
        Ok(())
    }
}

/// All applicable ordinary-AI substitutions and creation hooks must be captured.
/// Empty linking rows do not suppress UnitAI's runtime CheckForHelp invocation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SourceVisitorOverride {
    NamedOrCustomAi,
    Guard,
    PetTotemOrControl,
    Script,
    /// An addon writer outside the selected immutable generation source.
    Addon,
    LinkingOrSpawnGroup,
    OtherCreationHook,
}

/// Both addon queries are either absent or owned by the selected immutable
/// generation source. Other addon writers remain creation overrides.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum SourceVisitorAddonClosure {
    /// Capture contract: neither the spawn nor the template addon row exists.
    Absent,
    /// Exact source retained by the profile's validated generation catalog.
    GenerationSource { source_identity: crate::Digest },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceVisitorOwner {
    /// Stable runtime spawn key, not a current-generation identity or imported GUID.
    pub spawn_id: u64,
    pub entry: u32,
    pub ai_name: String,
    pub script_name: String,
    pub script_id: u32,
    pub default_movement_type: u32,
    pub spell_list_id: u32,
    /// Complete independently applicable entry and GUID EventAI event IDs.
    pub entry_events: SourceVisitorRows<u32>,
    pub guid_events: SourceVisitorRows<u32>,
    pub addon_closure: SourceVisitorAddonClosure,
    pub overrides: SourceVisitorRows<SourceVisitorOverride>,
}
impl SourceVisitorOwner {
    fn validate(&self, event_ai: Option<&EventAiRuntimeCatalog>) -> Result<(), NativeError> {
        if self.spawn_id <= SOLO_UNIT || self.entry == 0 || self.entry > 0xff_ffff {
            return Err(NativeError::InvalidProfile);
        }
        if self.ai_name.len() > 128 || self.script_name.len() > 128 {
            return Err(NativeError::TooLarge);
        }
        // This bounded nonempty-event policy selects EventAI explicitly.
        // The source can also choose it through the empty-name Permit fallback.
        if !matches!(self.ai_name.as_str(), "" | "EventAI")
            || !self.script_name.is_empty()
            || self.script_id != 0
            || self.default_movement_type > 1
            || self.spell_list_id != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "ordinary visitor EventAI selection",
            ));
        }
        match event_ai {
            None => {
                self.entry_events
                    .require_empty("complete empty visitor entry EventAI")?;
                self.guid_events
                    .require_empty("complete empty visitor GUID EventAI")?;
            }
            Some(runtime) => {
                let owner = runtime
                    .catalog()
                    .owner(self.spawn_id)
                    .ok_or(NativeError::AuthorityBinding)?;
                let source = runtime
                    .owner(self.spawn_id)
                    .ok_or(NativeError::AuthorityBinding)?;
                if owner.entry != self.entry || source.spell_list_id != self.spell_list_id {
                    return Err(NativeError::AuthorityBinding);
                }
                if self.ai_name != "EventAI" && !runtime.catalog().events(self.spawn_id)?.is_empty()
                {
                    return Err(NativeError::UnsupportedMechanic(
                        "nonempty events require explicit EventAI selection",
                    ));
                }
                for (actual, retained) in [
                    (&self.entry_events, &owner.entry_events),
                    (&self.guid_events, &owner.guid_events),
                ] {
                    let EventAiRows::Complete(retained) = retained else {
                        return Err(NativeError::AuthorityUnavailable(
                            "complete visitor EventAI rows",
                        ));
                    };
                    if !actual
                        .require("complete visitor EventAI IDs")?
                        .iter()
                        .copied()
                        .eq(retained.iter().map(|event| event.id))
                    {
                        return Err(NativeError::AuthorityBinding);
                    }
                }
            }
        }
        self.overrides
            .require_empty("complete empty visitor creation overrides")
    }
    fn validate_addon_closure(
        &self,
        generations: Option<&CreatureGenerationCatalog>,
    ) -> Result<(), NativeError> {
        let Some(generations) = generations else {
            return match self.addon_closure {
                SourceVisitorAddonClosure::Absent => Ok(()),
                SourceVisitorAddonClosure::GenerationSource { .. } => Err(
                    NativeError::AuthorityUnavailable("visitor addon generation source"),
                ),
            };
        };
        let source = generations
            .source(self.spawn_id)
            .ok_or(NativeError::AuthorityBinding)?;
        if source.spawn_id != self.spawn_id || source.spawn_entry != self.entry {
            return Err(NativeError::AuthorityBinding);
        }
        match self.addon_closure {
            SourceVisitorAddonClosure::Absent => {
                if !matches!(&source.input.spawn_addons, benilla_sim::creature_init::Rows::Complete(rows) if rows.is_empty())
                    || !matches!(&source.input.template_addons, benilla_sim::creature_init::Rows::Complete(rows) if rows.is_empty())
                {
                    return Err(NativeError::UnsupportedMechanic("visitor addon closure"));
                }
            }
            SourceVisitorAddonClosure::GenerationSource { source_identity } => {
                if source_identity != source.identity() {
                    return Err(NativeError::AuthorityBinding);
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceVisitorInput {
    /// An explicit authored permutation. Map::Update uses an unordered object set;
    /// this order is not a claim of universal ascending-spawn server execution.
    pub owner_order: Vec<u64>,
    pub map_id: u32,
    /// Raw Map.dbc instance type. Only the actual outdoor value zero is admitted.
    pub map_instance_type: u32,
    pub owners: SourceVisitorRows<SourceVisitorOwner>,
}

/// Private retained source shape, deliberately without Deserialize or writable rows.
/// ```compile_fail
/// use benilla_world_runtime::native::SourceVisitorCatalog;
/// let _: SourceVisitorCatalog = serde_json::from_str("{}").unwrap();
/// ```
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct SourceVisitorCatalog {
    policy: &'static str,
    owner_order: Vec<u64>,
    map_id: u32,
    map_instance_type: u32,
    owners: BTreeMap<u64, SourceVisitorOwner>,
    event_ai: Option<EventAiRuntimeCatalog>,
}
impl SourceVisitorCatalog {
    /// Validate source shape before an executable Profile exists. This is neither
    /// a gameplay admission token nor a permissive world constructor.
    pub fn validate_sources(input: SourceVisitorInput) -> Result<Self, NativeError> {
        Self::validate_selected_sources(input, None)
    }
    /// Explicitly select exact retained text events without relaxing the old
    /// constructor's complete-empty event requirement.
    pub fn validate_sources_with_event_ai(
        input: SourceVisitorInput,
        runtime: EventAiRuntimeCatalog,
    ) -> Result<Self, NativeError> {
        Self::validate_selected_sources(input, Some(runtime))
    }
    fn validate_selected_sources(
        input: SourceVisitorInput,
        event_ai: Option<EventAiRuntimeCatalog>,
    ) -> Result<Self, NativeError> {
        super::event_ai::encoded_budget(&(&input, &event_ai))?;
        let rows = input.owners.require("complete visitor owner sources")?;
        if rows.len() > crate::MAX_NPCS {
            return Err(NativeError::TooLarge);
        }
        if rows.is_empty() || input.owner_order.len() != rows.len() {
            return Err(NativeError::InvalidProfile);
        }
        if input.map_instance_type != 0 {
            return Err(NativeError::UnsupportedMechanic("outdoor visitor map"));
        }
        let mut owners = BTreeMap::new();
        let mut entries: BTreeMap<u32, &SourceVisitorOwner> = BTreeMap::new();
        for row in rows {
            row.validate(event_ai.as_ref())?;
            if let Some(shared) = entries.insert(row.entry, row) {
                // CreatureInfo fields and entry EventAI belong to the template;
                // GUID events, addon sources and overrides remain spawn-owned.
                if shared.ai_name != row.ai_name
                    || shared.script_name != row.script_name
                    || shared.script_id != row.script_id
                    || shared.spell_list_id != row.spell_list_id
                    || shared.entry_events != row.entry_events
                {
                    return Err(NativeError::AuthorityBinding);
                }
            }
            if owners.insert(row.spawn_id, row.clone()).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let scheduled: BTreeSet<_> = input.owner_order.iter().copied().collect();
        if scheduled.len() != input.owner_order.len() || !scheduled.iter().eq(owners.keys()) {
            return Err(NativeError::InvalidProfile);
        }
        if event_ai
            .as_ref()
            .is_some_and(|runtime| !runtime.owner_ids().eq(owners.keys().copied()))
        {
            return Err(NativeError::AuthorityBinding);
        }
        let catalog = Self {
            policy: SOURCE_VISITOR_POLICY,
            owner_order: input.owner_order,
            map_id: input.map_id,
            map_instance_type: input.map_instance_type,
            owners,
            event_ai,
        };
        super::event_ai::encoded_budget(&catalog)?;
        Ok(catalog)
    }
    pub fn event_ai(&self) -> Option<&EventAiRuntimeCatalog> {
        self.event_ai.as_ref()
    }
    pub fn owner_order(&self) -> &[u64] {
        &self.owner_order
    }
    pub fn owner(&self, spawn: u64) -> Option<&SourceVisitorOwner> {
        self.owners.get(&spawn)
    }
    pub fn outdoor(&self) -> bool {
        self.map_instance_type == 0
    }
    /// Immutable identity and source-owner coverage; full dependencies need Profile.
    pub(super) fn validate_scene(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != SOURCE_VISITOR_POLICY
            || !self.outdoor()
            || self.map_id != profile.map_id
            || profile.npcs.is_empty()
            || profile.npcs.len() > crate::MAX_NPCS
            || self.owners.len() != profile.npcs.len()
            || self.owner_order.len() != self.owners.len()
        {
            return Err(NativeError::InvalidProfile);
        }
        let scheduled: BTreeSet<_> = self.owner_order.iter().copied().collect();
        if scheduled.len() != self.owner_order.len()
            || !scheduled.iter().eq(self.owners.keys())
            || !self
                .owners
                .keys()
                .copied()
                .eq(profile.npcs.iter().map(|n| n.ai.creature_id))
        {
            return Err(NativeError::InvalidProfile);
        }
        // Callers can validate a scene independently of Profile::validate. The
        // immutable generation catalog owns full supported-addon validation.
        if let Some(generations) = &profile.creature_generations {
            generations.validate(profile)?;
        }
        for npc in &profile.npcs {
            let row = self
                .owner(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            row.validate(self.event_ai.as_ref())?;
            if row.entry != npc.entry
                || profile.source_motion.as_ref().is_some_and(|motion| {
                    motion.npcs.get(&npc.ai.creature_id).is_none_or(|owner| {
                        row.default_movement_type != owner.default_movement.movement_type()
                    })
                })
            {
                return Err(NativeError::InvalidProfile);
            }
            row.validate_addon_closure(profile.creature_generations.as_ref())?;
        }
        if let Some(runtime) = &self.event_ai {
            runtime.validate_scene(profile)?;
        }
        Ok(())
    }
    pub(super) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        self.validate_scene(profile)?;
        if profile.source_motion.is_none()
            || profile.combat.is_none()
            || !profile.source_combat_authority()
        {
            return Err(NativeError::UnsupportedMechanic(
                "complete visitor initialized combat authority",
            ));
        }
        // Retained catalogs own the flags/control/faction/neutrality and actual
        // GROUND checks. Do not duplicate raw fields with contradictory defaults.
        profile
            .interactions
            .as_ref()
            .ok_or(NativeError::InvalidProfile)?
            .validate(profile)?;
        profile
            .creature_capabilities
            .as_ref()
            .ok_or(NativeError::AuthorityUnavailable(
                "complete visitor ground capability sources",
            ))?
            .validate(profile)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::{synthetic_fixture as f, synthetic_two_owner_fixture as fixture};

    fn rows(input: &mut SourceVisitorInput) -> &mut Vec<SourceVisitorOwner> {
        let SourceVisitorRows::Complete(rows) = &mut input.owners else {
            unreachable!()
        };
        rows
    }
    fn input() -> SourceVisitorInput {
        let profile = fixture::profile();
        fixture::visitor_input(&profile, vec![2, 3])
    }
    #[test]
    fn exact_one_or_two_owner_selection_preserves_authored_permutation() {
        let a = fixture::profile_with_order([2, 3]);
        let b = fixture::profile_with_order([3, 2]);
        assert_eq!(a.source_visitors.as_ref().unwrap().owner_order(), &[2, 3]);
        assert_eq!(b.source_visitors.as_ref().unwrap().owner_order(), &[3, 2]);
        assert_ne!(a.digest(), b.digest());
        for profile in [&a, &b] {
            profile.validate().unwrap();
            let (a, b) = (
                profile.npc_motion(2).unwrap(),
                profile.npc_motion(3).unwrap(),
            );
            assert_ne!(a.npc_policy.run_speed, b.npc_policy.run_speed);
            assert_ne!(a.npc_policy.walk_speed, b.npc_policy.walk_speed);
            assert_ne!(
                a.npc_policy.settings.collision_height,
                b.npc_policy.settings.collision_height
            );
            assert_ne!(a.npc_spawn_orientation, b.npc_spawn_orientation);
            assert!(a.npc_can_enter_combat && b.npc_can_enter_combat);
        }
        assert!(a
            .npcs
            .iter()
            .all(|npc| npc.ai.reaction == ai::Reaction::Aggressive));
        assert!(a.npcs[1].ai.neutral_to_all);
        let mut singleton = synthetic_combat_fixture::profile();
        singleton.creature_capabilities = Some(
            CreatureCapabilityCatalog::validate_sources(
                synthetic_motion_fixture::creature_capability_sources(&singleton),
            )
            .unwrap(),
        );
        singleton.source_visitors = Some(
            SourceVisitorCatalog::validate_sources(fixture::visitor_input(&singleton, vec![2]))
                .unwrap(),
        );
        singleton.validate().unwrap();
        let mut unselected = a;
        unselected.source_visitors = None;
        assert!(unselected.validate().is_err());
    }
    #[test]
    fn raw_owner_schedule_and_complete_coverage_reject_missing_extra_and_duplicate_owners() {
        for case in 0..10 {
            let mut input = input();
            match case {
                0 => input.owner_order.clear(),
                1 => input.owner_order = vec![2],
                2 => input.owner_order = vec![2, 2],
                3 => input.owner_order = vec![2, 4],
                4 => input.owner_order.push(4),
                5 => input.owners = SourceVisitorRows::Missing,
                6 => input.owners = SourceVisitorRows::Unknown,
                7 => {
                    rows(&mut input).remove(1);
                }
                8 => {
                    let duplicate = rows(&mut input)[0].clone();
                    rows(&mut input)[1] = duplicate;
                }
                _ => rows(&mut input)[1].spawn_id = SOLO_UNIT,
            }
            assert!(
                SourceVisitorCatalog::validate_sources(input).is_err(),
                "case {case}"
            );
        }
    }
    #[test]
    fn shared_entry_owners_preserve_exact_spawns_and_authored_order() {
        let mut input = input();
        let template = rows(&mut input)[0].clone();
        *rows(&mut input) = (2..9)
            .map(|spawn_id| SourceVisitorOwner {
                spawn_id,
                addon_closure: SourceVisitorAddonClosure::GenerationSource {
                    source_identity: [spawn_id as u8; 32],
                },
                ..template.clone()
            })
            .collect();
        input.owner_order = vec![8, 3, 5, 2, 7, 4, 6];
        let catalog = SourceVisitorCatalog::validate_sources(input.clone()).unwrap();
        assert_eq!(catalog.owner_order(), &[8, 3, 5, 2, 7, 4, 6]);
        for spawn in 2..9 {
            assert_eq!(catalog.owner(spawn).unwrap().spawn_id, spawn);
            assert_eq!(catalog.owner(spawn).unwrap().entry, template.entry);
        }
        // Both AI names are individually supported for complete-empty events,
        // but the same template cannot supply different AIName fields.
        rows(&mut input)[6].ai_name = "EventAI".into();
        assert_eq!(
            SourceVisitorCatalog::validate_sources(input),
            Err(NativeError::AuthorityBinding)
        );
    }
    #[test]
    fn visitor_owner_bound_uses_the_world_limit() {
        let mut input = input();
        let template = rows(&mut input)[0].clone();
        *rows(&mut input) = (0..crate::MAX_NPCS)
            .map(|index| SourceVisitorOwner {
                spawn_id: index as u64 + 2,
                ..template.clone()
            })
            .collect();
        input.owner_order = (2..crate::MAX_NPCS as u64 + 2).collect();
        SourceVisitorCatalog::validate_sources(input.clone()).unwrap();
        rows(&mut input).push(SourceVisitorOwner {
            spawn_id: crate::MAX_NPCS as u64 + 2,
            ..template
        });
        input.owner_order.push(crate::MAX_NPCS as u64 + 2);
        assert_eq!(
            SourceVisitorCatalog::validate_sources(input),
            Err(NativeError::TooLarge)
        );
    }
    #[test]
    fn visitor_source_budget_precedes_nested_row_cloning() {
        let mut input = input();
        rows(&mut input)[0].entry_events =
            SourceVisitorRows::Complete(vec![u32::MAX; MAX_BYTES / 4]);
        assert_eq!(
            SourceVisitorCatalog::validate_sources(input),
            Err(NativeError::TooLarge)
        );
    }
    #[test]
    fn ordinary_ai_requires_both_empty_event_sets_and_all_creation_overrides() {
        for case in 0..17 {
            let mut input = input();
            let row = &mut rows(&mut input)[1];
            match case {
                0 => row.entry_events = SourceVisitorRows::Missing,
                1 => row.entry_events = SourceVisitorRows::Unknown,
                2 => row.entry_events = SourceVisitorRows::Complete(vec![101]),
                3 => row.guid_events = SourceVisitorRows::Missing,
                4 => row.guid_events = SourceVisitorRows::Unknown,
                5 => row.guid_events = SourceVisitorRows::Complete(vec![102]),
                6 => row.overrides = SourceVisitorRows::Missing,
                7 => row.overrides = SourceVisitorRows::Unknown,
                8 => row.ai_name = "NullAI".into(),
                9 => row.ai_name = "CustomAI".into(),
                10 => row.script_name = "custom_script".into(),
                11 => row.script_id = 1,
                12 => row.default_movement_type = 2,
                13 => row.spell_list_id = 1,
                14 => row.entry = 0,
                15 => row.entry = 0x100_0000,
                _ => row.ai_name = "x".repeat(129),
            }
            assert!(
                SourceVisitorCatalog::validate_sources(input).is_err(),
                "case {case}"
            );
        }
        for kind in [
            SourceVisitorOverride::NamedOrCustomAi,
            SourceVisitorOverride::Guard,
            SourceVisitorOverride::PetTotemOrControl,
            SourceVisitorOverride::Script,
            SourceVisitorOverride::Addon,
            SourceVisitorOverride::LinkingOrSpawnGroup,
            SourceVisitorOverride::OtherCreationHook,
        ] {
            for addon_closure in [
                SourceVisitorAddonClosure::Absent,
                SourceVisitorAddonClosure::GenerationSource {
                    source_identity: [7; 32],
                },
            ] {
                let mut input = input();
                rows(&mut input)[1].addon_closure = addon_closure;
                rows(&mut input)[1].overrides = SourceVisitorRows::Complete(vec![kind]);
                assert!(SourceVisitorCatalog::validate_sources(input).is_err());
            }
        }
        let mut explicit_eventai = input();
        rows(&mut explicit_eventai)[1].ai_name = "EventAI".into();
        SourceVisitorCatalog::validate_sources(explicit_eventai).unwrap();
        for raw_type in [1, 2, 3, u32::MAX] {
            let mut input = input();
            input.map_instance_type = raw_type;
            assert!(SourceVisitorCatalog::validate_sources(input).is_err());
        }
    }
    #[test]
    fn complete_selection_reuses_ground_initialized_combat_and_raw_neutrality_checks() {
        for case in 0..11 {
            let mut profile = fixture::profile();
            match case {
                0 => profile.map_id += 1,
                1 => profile.npcs[1].ai.creature_id += 1,
                2 => profile.npcs[1].entry += 1,
                3 => profile.source_motion = None,
                4 => profile.creature_capabilities = None,
                5 => profile.interactions = None,
                6 => profile.combat = None,
                7 => profile.npcs[1].ai.neutral_to_all = false,
                8 => profile.npcs[1].ai.reaction = ai::Reaction::Passive,
                9 => profile.npcs[1].ai.reaction = ai::Reaction::Defensive,
                _ => {
                    profile
                        .source_motion
                        .as_mut()
                        .unwrap()
                        .npcs
                        .get_mut(&3)
                        .unwrap()
                        .npc_can_enter_combat = false
                }
            }
            assert!(profile.validate().is_err(), "case {case}");
            assert!(
                NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).is_err()
            );
        }
        let mut profile = fixture::profile();
        let before = profile.clone();
        let mut interaction_rows = synthetic_combat_fixture::source_rows(&profile);
        interaction_rows.npcs[1].control.owner_guid = 77;
        assert!(profile.initialize_interactions(interaction_rows).is_err());
        assert_eq!(profile, before);
        let mut capability_rows = synthetic_motion_fixture::creature_capability_sources(&profile);
        let CreatureCapabilityRows::Complete(rows) = &mut capability_rows else {
            unreachable!()
        };
        let CreatureCapabilityRows::Complete(templates) = &mut rows[1].templates else {
            unreachable!()
        };
        templates[0].inhabit_type = 2;
        profile.creature_capabilities =
            Some(CreatureCapabilityCatalog::validate_sources(capability_rows).unwrap());
        assert!(profile.validate().is_err());
    }
    #[test]
    fn source_shape_builder_does_not_require_or_grant_an_executable_profile() {
        let mut draft = fixture::profile();
        draft.source_motion = None;
        draft.creature_capabilities = None;
        draft.interactions = None;
        draft.combat = None;
        let catalog =
            SourceVisitorCatalog::validate_sources(fixture::visitor_input(&draft, vec![3, 2]))
                .unwrap();
        catalog.validate_scene(&draft).unwrap();
        assert!(catalog.validate(&draft).is_err());
        draft.source_visitors = Some(catalog);
        assert!(draft.validate().is_err());
    }
    #[test]
    fn visitor_addon_reference_requires_its_selected_generation_owner() {
        let mut profile = fixture::profile();
        let original = profile.digest();
        for source_identity in [[0; 32], [7; 32]] {
            let mut input = fixture::visitor_input(&profile, vec![2, 3]);
            rows(&mut input)[0].addon_closure =
                SourceVisitorAddonClosure::GenerationSource { source_identity };
            let catalog = SourceVisitorCatalog::validate_sources(input).unwrap();
            assert_eq!(
                catalog.validate_scene(&profile),
                Err(NativeError::AuthorityUnavailable(
                    "visitor addon generation source"
                ))
            );
            profile.source_visitors = Some(catalog);
            assert_ne!(profile.digest(), original);
            assert!(profile.validate().is_err());
        }
    }
    #[test]
    fn visitor_addon_absence_and_reference_bind_both_retained_queries() {
        use benilla_sim::creature_init as init;

        let mut input = input();
        let mut owner = rows(&mut input)[0].clone();
        for (spawn_addon, template_addon) in
            [(false, false), (false, true), (true, false), (true, true)]
        {
            let mut input = synthetic_creature_model_fixture::input();
            input.spawn_id = owner.spawn_id;
            input.template.entry = owner.entry;
            let addon = init::Addon {
                mount: 0,
                stand_state: 0,
                sheath_state: 2,
                emote: 0,
                move_flags: 0,
                auras: vec![],
            };
            input.spawn_addons = init::Rows::Complete(
                spawn_addon
                    .then(|| init::SpawnAddonRow {
                        guid: owner.spawn_id,
                        addon: addon.clone(),
                    })
                    .into_iter()
                    .collect(),
            );
            input.template_addons = init::Rows::Complete(
                template_addon
                    .then_some(init::TemplateAddonRow {
                        entry: owner.entry,
                        addon,
                    })
                    .into_iter()
                    .collect(),
            );
            let source = CreatureGenerationSource {
                spawn_id: owner.spawn_id,
                spawn_entry: owner.entry,
                requested_spawn_template_entry: 0,
                respawn_delay_min_seconds: 0,
                respawn_delay_max_seconds: 0,
                input,
            };
            let catalog = CreatureGenerationCatalog::validate_sources(
                init::Rows::Complete(vec![source.clone()]),
                vec![owner.spawn_id],
            )
            .unwrap();
            owner.addon_closure = SourceVisitorAddonClosure::Absent;
            let absence = owner.validate_addon_closure(Some(&catalog));
            if spawn_addon || template_addon {
                assert_eq!(
                    absence,
                    Err(NativeError::UnsupportedMechanic("visitor addon closure"))
                );
            } else {
                absence.unwrap();
            }
            owner.addon_closure = SourceVisitorAddonClosure::GenerationSource {
                source_identity: source.identity(),
            };
            owner.validate_addon_closure(Some(&catalog)).unwrap();
            for case in 0..3 {
                let mut changed = owner.clone();
                match case {
                    0 => changed.spawn_id += 1,
                    1 => changed.entry += 1,
                    _ => {
                        changed.addon_closure = SourceVisitorAddonClosure::GenerationSource {
                            source_identity: [0; 32],
                        }
                    }
                }
                assert_eq!(
                    changed.validate_addon_closure(Some(&catalog)),
                    Err(NativeError::AuthorityBinding)
                );
            }
        }
    }
    #[test]
    fn native32_decode_restore_and_replay_reject_even_after_valid_rehashes() {
        let mut world = f::world();
        let initial = world.checkpoint().unwrap();
        for (version, policy, hash_version) in [
            (32, "benilla-native-owner-motion-genesis-v32", 32),
            (32, "benilla-native-owner-motion-genesis-v32", 33),
            (33, "benilla-native-owner-motion-genesis-v32", 33),
        ] {
            let mut value = serde_json::to_value(&initial).unwrap();
            value["version"] = version.into();
            value["state"]["version"] = version.into();
            value["state"]["policy"] = policy.into();
            let state: persistence::StateCheckpoint =
                serde_json::from_value(value["state"].clone()).unwrap();
            let digest = crate::digest(
                format!("benilla-native-state-v{hash_version}\0").as_bytes(),
                &state,
            );
            let tip: crate::Digest = serde_json::from_value(value["history_tip"].clone()).unwrap();
            value["digest"] = serde_json::to_value(digest).unwrap();
            value["checksum"] = serde_json::to_value(crate::digest(
                format!("benilla-native-checkpoint-v{hash_version}\0").as_bytes(),
                &(&state, digest, tip),
            ))
            .unwrap();
            let old: Checkpoint = serde_json::from_value(value).unwrap();
            assert!(Checkpoint::decode(&old.encode().unwrap()).is_err());
            assert!(
                NativeWorld::restore_synthetic_component_test(world.profile.clone(), old).is_err()
            );
        }
        world
            .step(1, None, &mut f::SyntheticAuthority::default())
            .unwrap();
        let mut records = world.records().to_vec();
        records[0].version = 32;
        assert!(decode_records(&encode_records(&records).unwrap()).is_err());
        let mut restored =
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial.clone())
                .unwrap();
        assert!(restored.replay(&records).is_err());
        assert_eq!(restored.checkpoint().unwrap(), initial);
        assert_eq!(NATIVE_FORMAT_VERSION, 49);
        assert_eq!(NATIVE_POLICY, "benilla-native-home-wander-v49");
        assert_eq!(crate::FORMAT_VERSION, 2);
    }
    #[test]
    fn owner_permutation_change_is_profile_bound_and_cannot_restore() {
        let profile = fixture::profile();
        let world =
            NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap();
        let checkpoint = world.checkpoint().unwrap();
        assert!(NativeWorld::restore_synthetic_component_test(
            fixture::profile_with_order([3, 2]),
            checkpoint,
        )
        .is_err());
    }
}
