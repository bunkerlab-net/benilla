// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, CreatureAISelector.cpp:38-90, CreatureEventAI.cpp:
// 51-62/149-162/1634-1664, CreatureAI.cpp:33-46 and UnitAI.cpp:51/68-101.
// Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
//! Immutable authored selection of a complete one/two-owner outdoor visitor scene.
//! It proves no real map traversal or production source completeness.
use super::*;
use serde::Serialize;
use std::collections::BTreeSet;

pub const SOURCE_VISITOR_POLICY: &str = "cmangos-8ec338a1-complete-source-visitors-v1";

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
    Addon,
    LinkingOrSpawnGroup,
    OtherCreationHook,
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
    pub overrides: SourceVisitorRows<SourceVisitorOverride>,
}
impl SourceVisitorOwner {
    fn validate(&self) -> Result<(), NativeError> {
        if self.spawn_id <= SOLO_UNIT || self.entry == 0 || self.entry > 0xff_ffff {
            return Err(NativeError::InvalidProfile);
        }
        if self.ai_name.len() > 128 || self.script_name.len() > 128 {
            return Err(NativeError::TooLarge);
        }
        // The empty-name default and the explicit EventAI name reach the same
        // ordinary EventAI selection, never NullAI or a fabricated Passive AI.
        if !matches!(self.ai_name.as_str(), "" | "EventAI")
            || !self.script_name.is_empty()
            || self.script_id != 0
            || self.default_movement_type != 0
            || self.spell_list_id != 0
        {
            return Err(NativeError::UnsupportedMechanic(
                "ordinary visitor EventAI selection",
            ));
        }
        self.entry_events
            .require_empty("complete empty visitor entry EventAI")?;
        self.guid_events
            .require_empty("complete empty visitor GUID EventAI")?;
        self.overrides
            .require_empty("complete empty visitor creation overrides")
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
}
impl SourceVisitorCatalog {
    /// Validate source shape before an executable Profile exists. This is neither
    /// a gameplay admission token nor a permissive world constructor.
    pub fn validate_sources(input: SourceVisitorInput) -> Result<Self, NativeError> {
        let rows = input.owners.require("complete visitor owner sources")?;
        if !matches!(rows.len(), 1 | 2) || input.owner_order.len() != rows.len() {
            return Err(NativeError::InvalidProfile);
        }
        if input.map_instance_type != 0 {
            return Err(NativeError::UnsupportedMechanic("outdoor visitor map"));
        }
        let mut owners = BTreeMap::new();
        let mut entries = BTreeSet::new();
        for row in rows {
            row.validate()?;
            if !entries.insert(row.entry) || owners.insert(row.spawn_id, row.clone()).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let scheduled: BTreeSet<_> = input.owner_order.iter().copied().collect();
        if scheduled.len() != input.owner_order.len() || !scheduled.iter().eq(owners.keys()) {
            return Err(NativeError::InvalidProfile);
        }
        Ok(Self {
            policy: SOURCE_VISITOR_POLICY,
            owner_order: input.owner_order,
            map_id: input.map_id,
            map_instance_type: input.map_instance_type,
            owners,
        })
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
    /// Immutable identity coverage only; full dependencies are checked by Profile.
    pub(super) fn validate_scene(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != SOURCE_VISITOR_POLICY
            || !self.outdoor()
            || self.map_id != profile.map_id
            || !matches!(profile.npcs.len(), 1 | 2)
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
        let mut entries = BTreeSet::new();
        for npc in &profile.npcs {
            let row = self
                .owner(npc.ai.creature_id)
                .ok_or(NativeError::InvalidProfile)?;
            row.validate()?;
            if row.entry != npc.entry || !entries.insert(npc.entry) {
                return Err(NativeError::InvalidProfile);
            }
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
    fn raw_owner_schedule_and_complete_coverage_reject_missing_extra_duplicate_and_third_owner() {
        for case in 0..12 {
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
                9 => {
                    let entry = rows(&mut input)[0].entry;
                    rows(&mut input)[1].entry = entry;
                }
                10 => {
                    let mut third = rows(&mut input)[1].clone();
                    third.spawn_id = 4;
                    third.entry += 1;
                    rows(&mut input).push(third);
                    input.owner_order.push(4);
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
                12 => row.default_movement_type = 1,
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
            let mut input = input();
            rows(&mut input)[1].overrides = SourceVisitorRows::Complete(vec![kind]);
            assert!(SourceVisitorCatalog::validate_sources(input).is_err());
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
        assert_eq!(NATIVE_FORMAT_VERSION, 33);
        assert_eq!(NATIVE_POLICY, "benilla-native-complete-source-visitors-v33");
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
