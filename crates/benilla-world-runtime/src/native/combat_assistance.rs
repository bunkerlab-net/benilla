// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
// Unit.cpp:559-582,6365-6385,7972-8013; Creature.cpp:140-142,463-464,2178-2197;
// GridNotifiers.cpp:265-281; CellImpl.h:37-193. Upstream GPL-2.0-or-later.
//! Selected combat-entry assistance, with authored complete non-assistable traversal.
//! This owns no map traversal implementation and grants no content admission.
use super::*;
use benilla_sim::creature_motion::Pose;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

mod state;
pub use state::*;
#[cfg(test)]
mod tests;

pub const COMBAT_ASSISTANCE_POLICY: &str = "cmangos-8ec338a1-combat-assistance-v2";
pub const MAX_ASSISTANCE_HISTORY: usize = MAX_RECORDS;
pub(in crate::native) const NO_CALL_ASSIST: u32 = 0x800;

/// Independently captured hook rows, not inferred from receiver emptiness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum CombatAssistanceHook {
    CreatureLinking,
    CreatureGroup,
    InstanceCallback,
    ExternalControl,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CombatAssistanceSource {
    pub spawn_id: u64,
    pub entry: u32,
    pub generation_source: crate::Digest,
    /// Raw template CallForHelp. Nonpositive fallback selection is not admitted.
    pub call_for_help: ai::Real,
    pub excluded_hooks: SourceVisitorRows<CombatAssistanceHook>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(in crate::native) struct CombatAssistanceCatalog {
    policy: &'static str,
    sources: BTreeMap<u64, CombatAssistanceSource>,
    aggro_delay_ms: u32,
}
impl CombatAssistanceCatalog {
    pub(in crate::native) fn validate(&self, profile: &Profile) -> Result<(), NativeError> {
        if self.policy != COMBAT_ASSISTANCE_POLICY || self.sources.len() != profile.npcs.len() {
            return Err(NativeError::AuthorityBinding);
        }
        let visitors =
            profile
                .source_visitors
                .as_ref()
                .ok_or(NativeError::AuthorityUnavailable(
                    "assistance selected source visitors",
                ))?;
        let mut entry_radii = BTreeMap::new();
        for npc in &profile.npcs {
            let row = self.source(npc.ai.creature_id)?;
            if entry_radii
                .insert(row.entry, row.call_for_help)
                .is_some_and(|previous| previous != row.call_for_help)
            {
                return Err(NativeError::AuthorityBinding);
            }
            let generation = profile
                .creature_generations
                .as_ref()
                .and_then(|c| c.source(row.spawn_id))
                .ok_or(NativeError::AuthorityUnavailable(
                    "assistance generation source",
                ))?;
            if row.entry != npc.entry
                || row.generation_source != generation.identity()
                || row.spawn_id != generation.spawn_id
                || row.entry != generation.spawn_entry
                || visitors
                    .owner(row.spawn_id)
                    .is_none_or(|o| o.entry != row.entry)
            {
                return Err(NativeError::AuthorityBinding);
            }
            if row.call_for_help.as_f32() <= 0.0
                || npc.combat.as_ref().is_none_or(|c| c.creature_is_critter)
            {
                return Err(NativeError::UnsupportedMechanic(
                    "positive assistance CallForHelp and ordinary creature",
                ));
            }
            match &row.excluded_hooks {
                SourceVisitorRows::Complete(rows) if rows.is_empty() => {}
                SourceVisitorRows::Complete(_) => {
                    return Err(NativeError::UnsupportedMechanic(
                        "assistance linking/group/control hooks",
                    ))
                }
                _ => {
                    return Err(NativeError::AuthorityUnavailable(
                        "complete assistance hook sources",
                    ))
                }
            }
        }
        Ok(())
    }
    pub(in crate::native) fn source(
        &self,
        spawn: u64,
    ) -> Result<&CombatAssistanceSource, NativeError> {
        self.sources
            .get(&spawn)
            .ok_or(NativeError::AuthorityBinding)
    }
    pub(in crate::native) fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-native-combat-assistance-source-v46\0", self)
    }
    pub(in crate::native) fn delay_ms(&self) -> u32 {
        self.aggro_delay_ms
    }
}
impl Profile {
    /// Select immutable captured fields independently of the authored traversal provider.
    pub fn initialize_combat_assistance(
        &mut self,
        sources: Vec<CombatAssistanceSource>,
        aggro_delay_ms: u32,
    ) -> Result<(), NativeError> {
        if sources.len() > crate::MAX_NPCS {
            return Err(NativeError::TooLarge);
        }
        let mut rows = BTreeMap::new();
        for source in sources {
            if rows.insert(source.spawn_id, source).is_some() {
                return Err(NativeError::InvalidProfile);
            }
        }
        let mut next = self.clone();
        next.interactions
            .as_mut()
            .and_then(|c| c.combat_authority.as_mut())
            .ok_or(NativeError::AuthorityUnavailable(
                "assistance combat source catalog",
            ))?
            .assistance = Some(CombatAssistanceCatalog {
            policy: COMBAT_ASSISTANCE_POLICY,
            sources: rows,
            aggro_delay_ms,
        });
        next.validate()?;
        *self = next;
        Ok(())
    }
    pub(in crate::native) fn combat_assistance(&self) -> Option<&CombatAssistanceCatalog> {
        self.interactions
            .as_ref()?
            .combat_authority
            .as_ref()?
            .assistance
            .as_ref()
    }
    pub(in crate::native) fn assistance_extra_flags(&self, spawn: u64) -> Result<u32, NativeError> {
        let entry = self.npc(spawn).ok_or(NativeError::UnknownTarget)?.entry;
        self.interactions
            .as_ref()
            .and_then(|c| c.combat_authority.as_ref())
            .and_then(|c| c.npcs.get(&entry))
            .map(|r| r.extra_flags)
            .ok_or(NativeError::AuthorityBinding)
    }
}

/// The exact callback after EnterCombat/EventAI, before MarkCallAssistanceOnPull.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CombatAssistanceRequest {
    pub binding: Binding,
    pub content: crate::Digest,
    pub source: crate::Digest,
    pub generation_source: crate::Digest,
    pub callback: EventAiCallbackKind,
    pub moment: InteractionMoment,
    pub now_ms: u64,
    pub occurrence: u64,
    pub unit_clock_ms: u64,
    pub owner_pose: Pose,
    pub enemy: crate::PlayerId,
    pub enemy_pose: Pose,
    pub radius: ai::Real,
    pub owner_bounding_radius: ai::Real,
    /// Exact ordered current scene, including the detached owner and source queues.
    pub scene: crate::Digest,
    /// Complete current scene in canonical spawn order, including generations.
    /// A traversal provider must account for every represented creature exactly once.
    pub scene_inventory: Vec<crate::NpcId>,
    pub rng_counter: u64,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    pub event_ai_callback: Option<crate::Digest>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistanceSelfContainer {
    Grid,
    World,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum CombatAssistanceFacts {
    /// Authored complete Cell::VisitAllObjects coverage: grid traversal, then world
    /// traversal, contains exactly this owner once and no other creature candidate.
    /// The source excludes self before all eligibility/range/LOS predicates.
    AuthoredCompleteEmptyTraversal {
        request: Box<CombatAssistanceRequest>,
        self_container: AssistanceSelfContainer,
    },
    /// Authored Cell::VisitAllObjects membership, not evidence of real map coverage.
    /// Creature callbacks run in the supplied grid order, then supplied world order.
    /// All other represented creatures must explicitly occur in outside_coverage.
    AuthoredCompleteTraversal {
        request: Box<CombatAssistanceRequest>,
        grid: Vec<crate::NpcId>,
        world: Vec<crate::NpcId>,
        outside_coverage: Vec<crate::NpcId>,
    },
    Missing,
    Unknown,
    NonemptyCandidates,
}
impl CombatAssistanceRequest {
    pub fn traversal_radius(&self) -> Result<ai::Real, NativeError> {
        if self.radius.as_f32() <= 0.0 || self.owner_bounding_radius.as_f32() < 0.0 {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(ai::Real::new(
            (self.radius.as_f32() + self.owner_bounding_radius.as_f32())
                .min(f32::from_bits(0x4405_5555)),
        )?)
    }
    pub fn validate(&self, facts: &CombatAssistanceFacts) -> Result<(), NativeError> {
        self.owner_pose.validate()?;
        self.enemy_pose.validate()?;
        self.traversal_radius()?;
        if self.binding.npc.is_none()
            || self.occurrence == 0
            || self.scene == [0; 32]
            || self.scene_inventory.is_empty()
            || self.scene_inventory.len() > crate::MAX_NPCS
            || self
                .scene_inventory
                .windows(2)
                .any(|pair| pair[0].spawn >= pair[1].spawn)
            || !self.scene_inventory.contains(&self.binding.npc.unwrap())
            || !matches!(self.moment, InteractionMoment::Frame { .. })
        {
            return Err(NativeError::AuthorityBinding);
        }
        match facts {
            CombatAssistanceFacts::AuthoredCompleteEmptyTraversal { request, .. }
                if request.as_ref() == self
                    && self.scene_inventory.as_slice() == [self.binding.npc.unwrap()] =>
            {
                Ok(())
            }
            CombatAssistanceFacts::AuthoredCompleteEmptyTraversal { .. } => {
                Err(NativeError::AuthorityBinding)
            }
            CombatAssistanceFacts::AuthoredCompleteTraversal {
                request,
                grid,
                world,
                outside_coverage,
            } => {
                if request.as_ref() != self
                    || grid.len() + world.len() + outside_coverage.len()
                        != self.scene_inventory.len()
                    || !grid
                        .iter()
                        .chain(world)
                        .any(|id| Some(*id) == self.binding.npc)
                {
                    return Err(NativeError::AuthorityBinding);
                }
                let represented: BTreeSet<_> = grid
                    .iter()
                    .chain(world)
                    .chain(outside_coverage)
                    .copied()
                    .collect();
                if represented.len() != self.scene_inventory.len()
                    || !represented
                        .iter()
                        .copied()
                        .eq(self.scene_inventory.iter().copied())
                {
                    return Err(NativeError::AuthorityBinding);
                }
                Ok(())
            }
            CombatAssistanceFacts::NonemptyCandidates => Err(NativeError::UnsupportedMechanic(
                "nonempty assistance candidate traversal",
            )),
            _ => Err(NativeError::AuthorityUnavailable(
                "complete assistance candidate traversal",
            )),
        }
    }
    pub(in crate::native) fn candidates(
        &self,
        facts: &CombatAssistanceFacts,
    ) -> Result<Vec<(AssistanceSelfContainer, crate::NpcId)>, NativeError> {
        self.validate(facts)?;
        Ok(match facts {
            CombatAssistanceFacts::AuthoredCompleteEmptyTraversal { self_container, .. } => {
                vec![(*self_container, self.binding.npc.unwrap())]
            }
            CombatAssistanceFacts::AuthoredCompleteTraversal { grid, world, .. } => grid
                .iter()
                .map(|id| (AssistanceSelfContainer::Grid, *id))
                .chain(world.iter().map(|id| (AssistanceSelfContainer::World, *id)))
                .collect(),
            _ => unreachable!("validated authored traversal"),
        })
    }
}

/// First source predicate rejecting a candidate. AI reaction is not a predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssistanceDenial {
    SelfCandidate,
    AlreadyInCombat,
    OwnerUninteractible,
    NotFriendlyToOwner,
    CannotAttackEnemyOnSight,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistanceCandidateReceipt {
    pub container: AssistanceSelfContainer,
    pub npc: crate::NpcId,
    pub denial: AssistanceDenial,
}
/// Derived only by the exact current-state host; providers supply membership alone.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AssistanceTraversalReceipt {
    pub request: crate::Digest,
    pub candidates: Vec<AssistanceCandidateReceipt>,
}
impl AssistanceTraversalReceipt {
    pub(in crate::native) fn validate(
        &self,
        request: &CombatAssistanceRequest,
        facts: &CombatAssistanceFacts,
    ) -> Result<(), NativeError> {
        let candidates = request.candidates(facts)?;
        if self.request != crate::digest(b"benilla-native-assistance-traversal-v46\0", request)
            || candidates.len() != self.candidates.len()
            || candidates
                .iter()
                .zip(&self.candidates)
                .any(|((container, npc), receipt)| {
                    *container != receipt.container
                        || *npc != receipt.npc
                        || (Some(*npc) == request.binding.npc)
                            != (receipt.denial == AssistanceDenial::SelfCandidate)
                })
        {
            return Err(NativeError::AuthorityBinding);
        }
        Ok(())
    }
}
