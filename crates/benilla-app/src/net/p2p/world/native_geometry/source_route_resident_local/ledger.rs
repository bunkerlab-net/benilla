// SPDX-License-Identifier: GPL-3.0-or-later
//! Cold-only by default; distinct component policies bound retained route epochs.
//! No policy here is a gameplay installation. This ledger
//! never turns a historical reply back into fresh authority and never owns an ECS borrow.
use super::super::source_route_filter::resident::retained::straight_retained::{
    straight_storage_upper_bound_for_test, MAX_SOURCE_REVISION, STRAIGHT_MAX_POINTS,
};
use super::super::{
    core::Session,
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_route_filter::resident::{owned::f05::F05ResumePermit, ResidentProviderIdentity},
};
use super::retained::ResidentRetainedCandidate;
pub(in super::super) use super::retained::{ParkedRetainedF05, ReadyRetainedScene};
pub(in super::super) use super::straight_loop::StraightSegmentPhase;
use super::three_epoch::{StagedWarmProposal, WarmDependency, WarmHead, WarmPredecessor};
use super::*;
use benilla_world_runtime::{native::Profile, NpcId, PlayerId};
use std::rc::{Rc, Weak};

/// Copied proof bounds for adversarial fixture placement, never Route authority.
pub(in super::super) fn support_prism_bounds_for_test(
    from: [f32; 3],
    to: [f32; 3],
    body: core::Body,
    floor: f32,
    ceiling: f32,
) -> Result<(bevy::prelude::Vec3, bevy::prelude::Vec3), Failure> {
    super::straight_loop::support_prism_bounds_for_test(from, to, body, floor, ceiling)
}

/// Independent fixture observations only; neither tuple member authorizes Route facts.
pub(in super::super) fn diagnose_support_prism_for_test(
    local: &mut LocalCorridor<'_, '_, '_>,
    from: [f32; 3],
    to: [f32; 3],
    body: core::Body,
    floor: f32,
    ceiling: f32,
) -> Result<
    (
        Option<bevy::prelude::Entity>,
        Result<bevy::prelude::Entity, Failure>,
    ),
    Failure,
> {
    super::straight_loop::diagnose_support_prism_for_test(local, from, to, body, floor, ceiling)
}

#[path = "pair.rs"]
mod pair;

const MAX_CANDIDATES: usize = 16;
struct Entry {
    id: u64,
    candidate: Candidate,
    committed: bool,
}
enum Candidate {
    Cold(Rc<ResidentColdCandidate>),
    Retained(Box<ResidentRetainedCandidate>),
    WarmHead(Rc<WarmHead>),
    WarmProposal(Rc<StagedWarmProposal>),
}
impl Candidate {
    // Historical ledger access only; no reply accessor escapes this module.
    fn reply(&self) -> &AuthorityReply {
        match self {
            Self::Cold(candidate) => &candidate.reply,
            Self::Retained(candidate) => &candidate.reply,
            Self::WarmHead(head) => &head.reply,
            Self::WarmProposal(proposal) => &proposal.head.reply,
        }
    }
    fn raw_revision(&self) -> [u8; 32] {
        match self {
            Self::Cold(candidate) => candidate.raw_revision,
            Self::Retained(candidate) => candidate.raw_revision,
            Self::WarmHead(head) => head.raw_revision,
            Self::WarmProposal(proposal) => proposal.head.raw_revision,
        }
    }
    fn identity(&self) -> (usize, Option<usize>) {
        match self {
            Self::Cold(candidate) => (Rc::as_ptr(candidate) as usize, None),
            Self::Retained(candidate) => (
                (&**candidate as *const ResidentRetainedCandidate) as usize,
                Some(Rc::as_ptr(&candidate.predecessor) as usize),
            ),
            Self::WarmHead(head) => (
                Rc::as_ptr(head) as usize,
                Some(Rc::as_ptr(&head.origin) as usize),
            ),
            Self::WarmProposal(proposal) => (
                Rc::as_ptr(proposal) as usize,
                Some(Rc::as_ptr(&proposal.head) as usize),
            ),
        }
    }
}
/// Non-owning witness for the actual cold terminal, not its identity token.
pub(in super::super) struct ColdLiveness {
    cold: Weak<ResidentColdCandidate>,
}
impl ColdLiveness {
    pub(in super::super) fn is_alive(&self) -> bool {
        self.cold.strong_count() != 0
    }
}
/// Weak witnesses name the actual owned head/proposal, never its native identity token.
pub(in super::super) struct WarmLiveness {
    head: Weak<WarmHead>,
    cold: Weak<ResidentColdCandidate>,
}
impl WarmLiveness {
    pub(in super::super) fn is_alive(&self) -> bool {
        self.head.strong_count() != 0
    }
    pub(in super::super) fn cold_is_alive(&self) -> bool {
        self.cold.strong_count() != 0
    }
}
pub(in super::super) struct ProposalLiveness {
    proposal: Weak<StagedWarmProposal>,
    head: Weak<WarmHead>,
    cold: Weak<ResidentColdCandidate>,
}
impl ProposalLiveness {
    pub(in super::super) fn is_alive(&self) -> bool {
        self.proposal.strong_count() != 0
    }
    pub(in super::super) fn head_is_alive(&self) -> bool {
        self.head.strong_count() != 0
    }
    pub(in super::super) fn cold_is_alive(&self) -> bool {
        self.cold.strong_count() != 0
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct OwnershipDiagnostic {
    pub(in super::super) terminal_count: usize,
    pub(in super::super) staged_count: usize,
    /// Logical slice lengths for owned backing/trace/cache bytes, not allocator capacity,
    /// fixed bookkeeping, or shared source/query allocations.
    pub(in super::super) storage_bytes: usize,
    pub(in super::super) upper_bound_bytes: usize,
}
pub(in super::super) struct StraightGeometryDiagnostic {
    pub(in super::super) points: Vec<benilla_sim::creature_ai::Point>,
    pub(in super::super) body: super::super::core::Body,
    pub(in super::super) trace: Vec<benilla_native_nav::SourceWarmTrace>,
}
pub(in super::super) struct ResidentLedger {
    session: Session,
    profile: Profile,
    player: PlayerId,
    provider: ResidentProviderIdentity,
    owner: benilla_formats::NativeInventorySource,
    raw: Option<[u8; 32]>,
    boundary: Option<RouteCommitBoundary>,
    last_notice: Option<RouteCommitNotice>,
    candidates: Vec<Entry>,
    next_id: u64,
    transport_identity: std::sync::Arc<()>,
    route_epoch_committed: bool,
    one_retained: bool,
    two_owner_first_retained: bool,
    retained_epoch_committed: bool,
    two_retained: bool,
    finite_retained: bool,
    finite_straight: bool,
    epochs_committed: u8,
}
/// Owner-local native dependency with the exact original cold ledger baseline.
pub(in super::super) struct PreparedCold {
    source: super::PreparedColdSource,
    baseline: Diagnostic,
    request: AuthorityRequest,
}
impl PreparedCold {
    pub(in super::super) fn deadline(&self) -> Instant {
        self.source.deadline()
    }
}
/// Includes every mutable ledger field. Candidate addresses identify immutable
/// opaque objects, including their private live seals, rather than serialized DTOs.
#[derive(Clone, Debug)]
pub(in super::super) struct Diagnostic {
    profile: Profile,
    boundary: Option<RouteCommitBoundary>,
    last_notice: Option<RouteCommitNotice>,
    candidates: Vec<(u64, usize, Option<usize>, bool)>,
    candidate_raw_revisions: Vec<[u8; 32]>,
    capacity: usize,
    flat_candidates: Vec<(u8, usize, usize, usize)>,
    raw: Option<[u8; 32]>,
    next_id: u64,
    route_epoch_committed: bool,
    one_retained: bool,
    two_owner_first_retained: bool,
    retained_epoch_committed: bool,
    two_retained: bool,
    finite_retained: bool,
    finite_straight: bool,
    epochs_committed: u8,
}
impl PartialEq for Diagnostic {
    fn eq(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.boundary == other.boundary
            && self.candidates == other.candidates
            && self.candidate_raw_revisions == other.candidate_raw_revisions
            && self.capacity == other.capacity
            && self.flat_candidates == other.flat_candidates
            && self.raw == other.raw
            && self.next_id == other.next_id
            && self.route_epoch_committed == other.route_epoch_committed
            && self.two_owner_first_retained == other.two_owner_first_retained
            && self.one_retained == other.one_retained
            && self.retained_epoch_committed == other.retained_epoch_committed
            && self.two_retained == other.two_retained
            && self.finite_retained == other.finite_retained
            && self.finite_straight == other.finite_straight
            && self.epochs_committed == other.epochs_committed
            && match (&self.last_notice, &other.last_notice) {
                (Some(a), Some(b)) => same_notice(&self.profile, a, b),
                (None, None) => true,
                _ => false,
            }
    }
}
impl Eq for Diagnostic {}
impl Diagnostic {
    /// Allocation reservation is not semantic staging and may grow capacity.
    pub(in super::super) fn same_semantic_state(&self, before: &Self) -> bool {
        let mut current = self.clone();
        current.capacity = before.capacity;
        current == *before
    }

    /// Transport retirement may retain raw provenance and monotonic IDs, never ACK.
    pub(in super::super) fn same_acknowledged_state(&self, before: &Self) -> bool {
        let normalize = |value: &Self| {
            let mut value = value.clone();
            let committed = value
                .candidates
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| entry.3.then_some(index))
                .collect::<Vec<_>>();
            value.candidates = committed.iter().map(|&i| value.candidates[i]).collect();
            value.candidate_raw_revisions = committed
                .iter()
                .map(|&i| value.candidate_raw_revisions[i])
                .collect();
            value.flat_candidates = committed
                .iter()
                .map(|&i| value.flat_candidates[i])
                .collect();
            value.raw = None;
            value.next_id = 0;
            value.capacity = 0;
            value
        };
        normalize(self) == normalize(before)
    }

    /// Exact first-cold cleanup: retain raw provenance and the consumed staging ID.
    pub(in super::super) fn only_cold_candidate_retired_since(&self, staged: &Self) -> bool {
        if !matches!(staged.candidates.as_slice(), [(_, _, None, false)])
            || staged.candidate_raw_revisions.as_slice() != staged.raw.as_slice()
            || staged.raw.is_none()
            || staged.flat_candidates.as_slice() != [(0, 0, 0, 0)]
        {
            return false;
        }
        let mut retired = staged.clone();
        retired.candidates.clear();
        retired.candidate_raw_revisions.clear();
        retired.flat_candidates.clear();
        *self == retired
    }

    /// First cold certification pins raw provenance and stages one candidate, but
    /// cannot change any acknowledged boundary, notice, epoch, or committed head.
    pub(in super::super) fn only_initial_cold_staged_since(&self, before: &Self) -> bool {
        if before.raw.is_some()
            || before.next_id != 0
            || !before.candidates.is_empty()
            || !before.candidate_raw_revisions.is_empty()
            || !before.flat_candidates.is_empty()
            || self.raw.is_none()
            || self.candidate_raw_revisions.as_slice() != self.raw.as_slice()
            || self.next_id != 1
            || !matches!(self.candidates.as_slice(), [(0, _, None, false)])
            || self.flat_candidates.as_slice() != [(0, 0, 0, 0)]
        {
            return false;
        }
        // Compare every other diagnostic field exactly, including capacity. Only
        // the five explicitly checked initial-staging fields may differ.
        let mut unstaged = self.clone();
        unstaged.raw = None;
        unstaged.next_id = 0;
        unstaged.candidates.clear();
        unstaged.candidate_raw_revisions.clear();
        unstaged.flat_candidates.clear();
        unstaged == *before
    }

    /// Exercise the oracle against mutations of an actual successful FIFO stage.
    pub(in super::super) fn assert_initial_cold_staging_rejects_mutations(&self, before: &Self) {
        assert!(self.only_initial_cold_staged_since(before));
        assert!(before.boundary.is_some());
        assert!(before.last_notice.is_some());
        let mutations: &[fn(&mut Self)] = &[
            |d| d.candidates[0].3 = true,
            |d| d.candidates[0].0 = 1,
            |d| d.candidates[0].2 = Some(1),
            |d| d.candidates.push(d.candidates[0]),
            |d| d.flat_candidates[0].0 = 1,
            |d| d.raw = None,
            |d| d.raw.as_mut().unwrap()[0] ^= 1,
            |d| d.candidate_raw_revisions[0][0] ^= 1,
            |d| d.next_id = 2,
            |d| d.capacity += 1,
            |d| d.boundary = None,
            |d| d.last_notice = None,
            |d| d.route_epoch_committed = !d.route_epoch_committed,
            |d| d.retained_epoch_committed = !d.retained_epoch_committed,
            |d| d.epochs_committed += 1,
            |d| d.one_retained = !d.one_retained,
        ];
        for (index, mutate) in mutations.iter().enumerate() {
            let mut wrong = self.clone();
            mutate(&mut wrong);
            assert!(
                !wrong.only_initial_cold_staged_since(before),
                "initial cold staging mutation {index} was accepted"
            );
        }
    }

    /// First retained FIFO certification may append exactly one uncommitted
    /// candidate rooted in the acknowledged cold owner, and change nothing else.
    pub(in super::super) fn only_first_retained_staged_since(&self, before: &Self) -> bool {
        let [(cold_id, cold, None, true)] = before.candidates.as_slice() else {
            return false;
        };
        if *cold_id != 0
            || before.next_id != 1
            || before.raw.is_none()
            || before.candidate_raw_revisions.as_slice() != before.raw.as_slice()
            || before.flat_candidates.as_slice() != [(0, 0, 0, 0)]
            || self.candidates.len() != 2
            || self.candidates[0] != before.candidates[0]
            || self.candidates[1].0 != 1
            || self.candidates[1].1 == *cold
            || self.candidates[1].2 != Some(*cold)
            || self.candidates[1].3
            || self.candidate_raw_revisions.as_slice() != [before.raw.unwrap(); 2]
            || self.flat_candidates.as_slice() != [(0, 0, 0, 0); 2]
            || self.next_id != 2
        {
            return false;
        }
        let mut unstaged = self.clone();
        unstaged.candidates.pop();
        unstaged.candidate_raw_revisions.pop();
        unstaged.flat_candidates.pop();
        unstaged.next_id = before.next_id;
        unstaged == *before
    }

    pub(in super::super) fn assert_first_retained_staging_rejects_mutations(&self, before: &Self) {
        assert!(self.only_first_retained_staged_since(before));
        let mutations: &[fn(&mut Self)] = &[
            |d| d.candidates[0].3 = false,
            |d| d.candidates[1].3 = true,
            |d| d.candidates[1].0 += 1,
            |d| d.candidates[1].2 = None,
            |d| d.candidates[1].2 = Some(d.candidates[1].1),
            |d| d.candidates.push(d.candidates[1]),
            |d| d.candidate_raw_revisions[1][0] ^= 1,
            |d| d.flat_candidates[1].0 = 1,
            |d| d.raw = None,
            |d| d.next_id += 1,
            |d| d.capacity += 1,
            |d| d.boundary = None,
            |d| d.last_notice = None,
            |d| d.route_epoch_committed = !d.route_epoch_committed,
            |d| d.retained_epoch_committed = !d.retained_epoch_committed,
            |d| d.epochs_committed += 1,
            |d| d.one_retained = !d.one_retained,
        ];
        for (index, mutate) in mutations.iter().enumerate() {
            let mut wrong = self.clone();
            mutate(&mut wrong);
            assert!(
                !wrong.only_first_retained_staged_since(before),
                "first retained staging mutation {index} was accepted"
            );
        }
    }

    pub(in super::super) fn same_committed_root(&self, other: &Self) -> bool {
        self.profile == other.profile
            && self.boundary == other.boundary
            && self.raw == other.raw
            && self.route_epoch_committed == other.route_epoch_committed
            && self.two_owner_first_retained == other.two_owner_first_retained
            && self.one_retained == other.one_retained
            && self.retained_epoch_committed == other.retained_epoch_committed
            && self.two_retained == other.two_retained
            && self.finite_retained == other.finite_retained
            && self.finite_straight == other.finite_straight
            && self.epochs_committed == other.epochs_committed
            && self
                .candidates
                .iter()
                .filter(|entry| entry.3)
                .eq(other.candidates.iter().filter(|entry| entry.3))
            && match (&self.last_notice, &other.last_notice) {
                (Some(a), Some(b)) => same_notice(&self.profile, a, b),
                (None, None) => true,
                _ => false,
            }
    }
}

/// Immutable name of one genuinely staged candidate, not a geometry certificate.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct ResidentCandidateStamp {
    id: u64,
    raw: [u8; 32],
}

impl ResidentLedger {
    pub(in super::super) fn staged_transport_candidate(
        &self,
        context: &AuthorityRequest,
        facts: &Facts,
    ) -> Result<ResidentCandidateStamp, Failure> {
        let mut matches = self.candidates.iter().filter(|entry| {
            !entry.committed
                && same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
                && entry.candidate.reply().facts == *facts
        });
        let entry = matches.next().ok_or(Failure::Binding)?;
        if matches.next().is_some() || self.raw != Some(entry.candidate.raw_revision()) {
            return Err(Failure::Binding);
        }
        self.validate_entry(&entry.candidate)?;
        Ok(ResidentCandidateStamp {
            id: entry.id,
            raw: entry.candidate.raw_revision(),
        })
    }
    pub(in super::super) fn validate_transport_candidate(
        &self,
        stamp: &ResidentCandidateStamp,
        context: &AuthorityRequest,
        facts: &Facts,
    ) -> Result<(), Failure> {
        if &self.staged_transport_candidate(context, facts)? != stamp {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    #[cfg(test)]
    pub(in super::super) fn assert_committed_transport_refusal(&self) {
        let before = self.diagnostic();
        for entry in self.candidates.iter().filter(|entry| entry.committed) {
            let stamp = ResidentCandidateStamp {
                id: entry.id,
                raw: entry.candidate.raw_revision(),
            };
            let reply = entry.candidate.reply();
            assert_eq!(
                self.validate_transport_candidate(&stamp, &reply.request, &reply.facts),
                Err(Failure::Binding)
            );
        }
        assert_eq!(self.diagnostic(), before);
    }
    #[cfg(test)]
    pub(in super::super) fn assert_transport_stamp_refusals(
        &self,
        context: &AuthorityRequest,
        facts: &Facts,
    ) {
        let original = self.staged_transport_candidate(context, facts).unwrap();
        let before = self.diagnostic();
        let wrong_id = ResidentCandidateStamp {
            id: original.id + 1,
            raw: original.raw,
        };
        assert_eq!(
            self.validate_transport_candidate(&wrong_id, context, facts),
            Err(Failure::Binding)
        );
        let mut wrong_revision = original;
        wrong_revision.raw[0] ^= 1;
        assert_eq!(
            self.validate_transport_candidate(&wrong_revision, context, facts),
            Err(Failure::Binding)
        );
        assert_eq!(self.diagnostic(), before);
    }
    pub(in super::super) fn transport_identity(&self) -> std::sync::Arc<()> {
        self.transport_identity.clone()
    }
    pub(in super::super) fn next_transport_candidate(&self) -> u64 {
        self.next_id
    }
    /// Only a failed synchronous certification can retire its never-emitted candidate.
    pub(in super::super) fn retire_rejected_transport_attempt(
        &mut self,
        proof: &super::super::source_route_filter::resident::owned::transport::RejectedResidentCertification,
    ) -> Result<(), Failure> {
        let (identity, context, first_id) = proof.parts()?;
        if !std::sync::Arc::ptr_eq(identity, &self.transport_identity) {
            return Err(Failure::Binding);
        }
        if self.next_id == first_id {
            return Ok(());
        }
        if first_id.checked_add(1) != Some(self.next_id) {
            return Err(Failure::Binding);
        }
        let position = self
            .candidates
            .iter()
            .position(|entry| entry.id == first_id)
            .ok_or(Failure::Binding)?;
        let entry = &self.candidates[position];
        if entry.committed
            || !same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
            || self
                .boundary
                .as_ref()
                .is_none_or(|boundary| boundary.tick() >= context.binding.tick)
        {
            return Err(Failure::Binding);
        }
        self.candidates.remove(position);
        Ok(())
    }
    /// The private proof is minted only after the actual sender proves no Facts delivery.
    /// Source pin, monotonic ID, native constructor and acknowledged head stay intact.
    pub(in super::super) fn retire_unsent_transport_candidate(
        &mut self,
        proof: &super::super::core::UndeliveredResidentResult,
    ) -> Result<(), Failure> {
        let (identity, stamp, context, facts) = proof.result()?.retirement_parts();
        if !std::sync::Arc::ptr_eq(identity, &self.transport_identity) {
            return Err(Failure::Binding);
        }
        self.validate_transport_candidate(stamp, context, facts)?;
        if self
            .boundary
            .as_ref()
            .is_none_or(|boundary| boundary.tick() >= context.binding.tick)
        {
            return Err(Failure::Binding);
        }
        let position = self
            .candidates
            .iter()
            .position(|entry| entry.id == stamp.id)
            .ok_or(Failure::Binding)?;
        if self.candidates[position].committed {
            return Err(Failure::Binding);
        }
        self.candidates.remove(position);
        Ok(())
    }

    pub(in super::super) fn bind(
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        deadline: Instant,
    ) -> Result<Self, ResidentFilterError> {
        let identity = provider.ledger_identity(session, profile)?;
        provider
            .verify_installed_owner(compatibility, compatibility.owner(), deadline, || false)?;
        let one_retained = identity.allows_one_retained_route();
        let two_owner_first_retained = identity.allows_two_owner_first_retained();
        let two_retained = identity.allows_two_retained_routes();
        let finite_retained = identity.allows_finite_retained_routes();
        let finite_straight = identity.allows_finite_straight_routes();
        if (one_retained || finite_straight) && profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery.into());
        }
        if two_owner_first_retained
            && (profile.npcs.len() != 2
                || one_retained
                || two_retained
                || finite_retained
                || finite_straight)
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let mut candidates = vec![];
        if one_retained || finite_straight || two_owner_first_retained {
            // Failed fresh attempts cannot change even diagnostic reservation state.
            candidates
                .try_reserve_exact(MAX_CANDIDATES)
                .map_err(|_| Failure::QueueCapacity)?;
        }
        Ok(Self {
            session: session.clone(),
            profile: profile.clone(),
            player,
            provider: identity,
            owner: compatibility.owner().clone(),
            raw: None,
            boundary: None,
            last_notice: None,
            candidates,
            next_id: 0,
            transport_identity: std::sync::Arc::new(()),
            route_epoch_committed: false,
            one_retained,
            two_owner_first_retained,
            retained_epoch_committed: false,
            two_retained,
            finite_retained,
            finite_straight,
            epochs_committed: 0,
        })
    }
    fn binding(&self, boundary: &RouteCommitBoundary) -> Result<(), Failure> {
        if boundary.world() != self.session.world
            || boundary.player() != self.player
            || boundary.profile() != self.profile.digest()
            || boundary.profile() != self.session.profile
            || boundary.geometry() != self.session.geometry
            || boundary.authority() != self.session.policy
            || boundary.heads().len() != self.profile.npcs.len()
        {
            return Err(Failure::Binding);
        }
        let mut previous = None;
        for head in boundary.heads() {
            if !self.session.bodies.contains_key(&head.npc().spawn)
                || previous.is_some_and(|spawn| spawn >= head.npc().spawn)
                || head.suspended_wander().is_some()
                || !matches!(
                    head.constructor_kind(),
                    None | Some(motion::PathfinderConstructorKind::ChaseLazy)
                )
                || head.cache().is_some() && head.construction().is_none()
                || head.construction().is_some() && head.constructor_kind().is_none()
            {
                return Err(Failure::Binding);
            }
            previous = Some(head.npc().spawn);
        }
        Ok(())
    }
    pub(in super::super) fn initialize(
        &mut self,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        self.binding(boundary)?;
        if let Some(old) = &self.boundary {
            return if old == boundary && self.last_notice.is_none() {
                Ok(())
            } else {
                Err(Failure::Binding)
            };
        }
        if !self.candidates.is_empty()
            || boundary
                .owner_heads()
                .any(|(_, head)| head.cache().is_some())
        {
            return Err(Failure::Binding);
        }
        self.boundary = Some(boundary.clone());
        Ok(())
    }
    /// Reconstruction cannot adopt a previously attached or attempted owner.
    pub(in super::super) fn initialize_reconstruction(
        &mut self,
        session: &Session,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        if self.session != *session
            || !self.provider.matches(provider)
            || provider.retained_constructor_count() != 0
            || self.boundary.is_some()
            || self.last_notice.is_some()
            || self.raw.is_some()
            || !self.candidates.is_empty()
            || self.next_id != 0
            || self.route_epoch_committed
            || self.retained_epoch_committed
            || self.epochs_committed != 0
        {
            return Err(Failure::Binding);
        }
        self.initialize(boundary)
    }
    /// Reserve before any source/concrete work. Successful emission and staging
    /// are one synchronous operation; failed attempts do not consume ledger slots.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        constructor: ResidentFilterDependency,
        water: ResidentChaseWaterDependency,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.cold_preflight(compatibility, provider, context)?;
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        let (candidate, facts) = super::certify(
            world,
            compatibility,
            provider,
            context,
            constructor,
            water,
            ticket,
            deadline,
            cancelled,
            || {},
        )?;
        if !candidate.installed_owner.same_owner(&self.owner)
            || self.raw.is_some_and(|raw| raw != candidate.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        self.validate_candidate(&candidate)?;
        self.raw = Some(candidate.raw_revision);
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Candidate::Cold(Rc::new(candidate)),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    fn cold_preflight(
        &mut self,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        let previous = self.boundary.as_ref().ok_or(Failure::Binding)?;
        // Synchronous controller pairing only: its ACK barrier excludes another
        // attempt after a successful step. Until then only an equal-input retry
        // may supersede this owner's live receipt, never another construction.
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply().request.binding.npc == context.binding.npc
                && entry.candidate.reply().request.binding.tick == context.binding.tick
                && entry.candidate.reply().request != *context
        }) {
            return Err(Failure::UnsupportedQuery.into());
        }
        let dispatch = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if self.route_epoch_committed {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || !self.owner.same_owner(compatibility.owner())
            || previous.tick().checked_add(1) != Some(context.binding.tick)
            || !dispatch.first_construction()
            || self.candidates.iter().any(|entry| {
                same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
            })
        {
            return Err(Failure::Binding.into());
        }
        if self.candidates.len() == MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        self.candidates
            .try_reserve(1)
            .map_err(|_| Failure::QueueCapacity)?;
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn prepare_cold(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        constructor: ResidentFilterDependency,
        water: ResidentChaseWaterDependency,
        ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<PreparedCold, ResidentFilterError> {
        self.cold_preflight(compatibility, provider, context)?;
        if self.profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery.into());
        }
        let baseline = self.diagnostic();
        let source = super::prepare_cold_source(
            world,
            compatibility,
            provider,
            context,
            constructor,
            water,
            ticket,
            deadline,
            cancelled,
        )?;
        Ok(PreparedCold {
            source,
            baseline,
            request: context.clone(),
        })
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn finish_cold(
        &mut self,
        prepared: PreparedCold,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.cold_preflight(compatibility, provider, context)?;
        if self.profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !same_reply_dispatch(&self.profile, &prepared.request, context)
            || !self.diagnostic().same_semantic_state(&prepared.baseline)
        {
            return Err(Failure::Binding.into());
        }
        let (candidate, facts) = super::finish_cold_source(
            prepared.source,
            world,
            compatibility,
            provider,
            context,
            deadline,
            cancelled,
        )?;
        self.cold_preflight(compatibility, provider, context)?;
        if self.profile.npcs.len() != 1 {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.diagnostic().same_semantic_state(&prepared.baseline)
            || !candidate.installed_owner.same_owner(&self.owner)
            || self.raw.is_some_and(|raw| raw != candidate.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        self.validate_candidate(&candidate)?;
        self.raw = Some(candidate.raw_revision);
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Candidate::Cold(Rc::new(candidate)),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    /// A private synchronous capability: never expose the committed candidate to
    /// callers or let a staged sibling substitute for the ACK-selected native root.
    fn committed_cold(&self) -> Result<&Rc<ResidentColdCandidate>, Failure> {
        if !self.route_epoch_committed
            || self.retained_epoch_committed
            || self.profile.npcs.len() != 1
        {
            return Err(Failure::Binding);
        }
        let mut committed = self.candidates.iter().filter(|entry| entry.committed);
        let entry = committed.next().ok_or(Failure::Binding)?;
        if committed.next().is_some() {
            return Err(Failure::Binding);
        }
        let Candidate::Cold(candidate) = &entry.candidate else {
            return Err(Failure::Binding);
        };
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        let head = boundary.heads().first().ok_or(Failure::Binding)?;
        let dispatch = candidate
            .reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if boundary.heads().len() != 1
            || Some(head.npc()) != candidate.reply.request.binding.npc
            || head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
            || head.construction() != Some(dispatch.construction_binding())
            || head.cache() != Some(&calculation(&candidate.reply)?.after_cache)
        {
            return Err(Failure::Binding);
        }
        self.validate_candidate(candidate)?;
        Ok(candidate)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_retained(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.certify_retained_observed(
            world,
            compatibility,
            provider,
            context,
            first_ticket,
            deadline,
            cancelled,
            || {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_retained_observed(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        let predecessor = self.retained_preflight(compatibility, provider, context)?;
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        let (candidate, facts) = super::retained::certify(
            world,
            compatibility,
            provider,
            predecessor,
            context,
            self.player,
            first_ticket,
            deadline,
            cancelled,
            after,
        )?;
        // Recheck the exact root before staging; no ACK or pending-source yield
        // can silently redirect this immutable native predecessor.
        if !Rc::ptr_eq(self.committed_cold()?, &candidate.predecessor)
            || self.raw != Some(candidate.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        self.validate_retained_candidate(&candidate)?;
        let candidate = if self.two_retained || self.finite_retained {
            let proposal = StagedWarmProposal::first(candidate);
            self.validate_warm_proposal(&proposal)?;
            Candidate::WarmProposal(Rc::new(proposal))
        } else {
            Candidate::Retained(Box::new(candidate))
        };
        self.candidates.push(Entry {
            id: self.next_id,
            candidate,
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    fn retained_preflight(
        &mut self,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) -> Result<Rc<ResidentColdCandidate>, ResidentFilterError> {
        provider.require_one_retained_route()?;
        if !self.one_retained || self.retained_epoch_committed {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || !self.owner.same_owner(compatibility.owner())
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = Rc::clone(self.committed_cold()?);
        self.validate_retained_context(&predecessor, context)?;
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply().request.binding.npc == context.binding.npc
                && entry.candidate.reply().request.binding.tick == context.binding.tick
                && entry.candidate.reply().request != *context
        }) {
            return Err(Failure::UnsupportedQuery.into());
        }
        if self.candidates.iter().any(|entry| {
            same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
        }) {
            return Err(Failure::Binding.into());
        }
        if self.candidates.len() == MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        self.candidates
            .try_reserve(1)
            .map_err(|_| Failure::QueueCapacity)?;
        Ok(predecessor)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn prepare_retained(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<super::retained::PreparedRetained, ResidentFilterError> {
        let predecessor = self.retained_preflight(compatibility, provider, context)?;
        super::retained::prepare(
            world,
            compatibility,
            provider,
            predecessor,
            context,
            self.player,
            first_ticket,
            deadline,
            cancelled,
        )
    }
    /// Begin only the first pre-clearance root; no candidate or staging ID is reserved.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn begin_retained_f05(
        &mut self,
        prepared: super::retained::PreparedRetained,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ParkedRetainedF05, ResidentFilterError> {
        let predecessor = self.retained_preflight(compatibility, provider, context)?;
        if !Rc::ptr_eq(&predecessor, &prepared.predecessor) {
            return Err(Failure::Binding.into());
        }
        super::retained::begin_f05(
            prepared,
            world,
            compatibility,
            provider,
            context,
            deadline,
            cancelled,
        )
    }

    /// No speculative ledger check precedes the original F05 post-source context check.
    pub(in super::super) fn resume_retained_f05(
        parked: ParkedRetainedF05,
        source: super::super::products::source_height::owned::freshness::F05VerifiedSuccess,
        permit: &F05ResumePermit,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        cancelled: impl Fn() -> bool,
    ) -> Result<ReadyRetainedScene, ResidentFilterError> {
        super::retained::resume_f05(parked, source, permit, provider, context, cancelled)
    }

    /// Current physics must have been synchronized after the ready observation.
    pub(in super::super) fn finish_retained_f05(
        &mut self,
        ready: ReadyRetainedScene,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        let (candidate, facts) = super::retained::finish_f05(
            ready,
            world,
            compatibility,
            provider,
            context,
            cancelled,
        )?;
        self.stage_finished_retained(candidate, facts, compatibility, provider, context)
    }

    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn finish_retained(
        &mut self,
        prepared: super::retained::PreparedRetained,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        let predecessor = self.retained_preflight(compatibility, provider, context)?;
        if !Rc::ptr_eq(&predecessor, &prepared.predecessor) {
            return Err(Failure::Binding.into());
        }
        let (candidate, facts) = super::retained::finish(
            prepared,
            world,
            compatibility,
            provider,
            context,
            deadline,
            cancelled,
            || {},
        )?;
        self.stage_finished_retained(candidate, facts, compatibility, provider, context)
    }

    fn stage_finished_retained(
        &mut self,
        candidate: ResidentRetainedCandidate,
        facts: Facts,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) -> Result<Facts, ResidentFilterError> {
        // Never reserve a staging identifier across the owner yield.
        self.retained_preflight(compatibility, provider, context)?;
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        // Recheck the exact root before staging; no ACK or pending-source yield
        // can silently redirect this immutable native predecessor.
        if !Rc::ptr_eq(self.committed_cold()?, &candidate.predecessor)
            || self.raw != Some(candidate.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        self.validate_retained_candidate(&candidate)?;
        let candidate = if self.two_retained || self.finite_retained {
            let proposal = StagedWarmProposal::first(candidate);
            self.validate_warm_proposal(&proposal)?;
            Candidate::WarmProposal(Rc::new(proposal))
        } else {
            Candidate::Retained(Box::new(candidate))
        };
        self.candidates.push(Entry {
            id: self.next_id,
            candidate,
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    /// Diagnostic source-only work rooted inside the ledger. No candidate, Facts,
    /// native terminal or clearance escapes, and concrete certification reruns it.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn probe_retained_source_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(
            super::super::source_route_filter::resident::retained::ResidentRetainedPhase,
        ),
    ) -> Result<(), ResidentFilterError> {
        self.inspect_retained_prefix_for_test(
            provider,
            context,
            first_ticket,
            deadline,
            cancelled,
            observe,
            |_, _, _| {},
        )
    }
    /// Borrow actual guarded dependencies only for a local differential test.
    /// The callback cannot return or retain these fresh borrows as authority.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn inspect_retained_prefix_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(
            super::super::source_route_filter::resident::retained::ResidentRetainedPhase,
        ),
        inspect: impl for<'scope> FnOnce(
            &'scope ResidentFilterProvider<'_, '_, '_, '_>,
            &'scope super::super::source_route_filter::resident::retained::ResidentJoinedRetainedDependency,
            &'scope super::super::source_route_filter::resident::joined::ResidentJoinedColdDependency,
        ),
    ) -> Result<(), ResidentFilterError> {
        provider.require_one_retained_route()?;
        if !self.one_retained || self.retained_epoch_committed {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = self.committed_cold()?;
        self.validate_retained_context(predecessor, context)?;
        let dependency = provider.observe_joined_retained_observed(
            &predecessor.dependency,
            context,
            self.player,
            first_ticket,
            benilla_native_nav::SourceSamePolygonBudget {
                lookup_primitives: 8,
                terminal_primitives: 1,
            },
            deadline,
            cancelled,
            observe,
        )?;
        inspect(provider, &dependency, &predecessor.dependency);
        Ok(())
    }
    fn validate_entry(&self, candidate: &Candidate) -> Result<(), Failure> {
        match candidate {
            Candidate::Cold(candidate) => self.validate_candidate(candidate),
            Candidate::Retained(candidate) => self.validate_retained_candidate(candidate),
            Candidate::WarmHead(head) if self.two_owner_first_retained => {
                self.validate_pair_warm_head(head)
            }
            Candidate::WarmHead(head) => self.validate_warm_head(head),
            Candidate::WarmProposal(proposal) if self.two_owner_first_retained => {
                self.validate_pair_proposal(proposal)
            }
            Candidate::WarmProposal(proposal) => self.validate_warm_proposal(proposal),
        }
    }
    fn validate_retained_context(
        &self,
        predecessor: &ResidentColdCandidate,
        context: &AuthorityRequest,
    ) -> Result<(), Failure> {
        self.validate_retained_reply_context(&predecessor.reply, context)
    }
    fn validate_retained_reply_context(
        &self,
        predecessor: &AuthorityReply,
        context: &AuthorityRequest,
    ) -> Result<(), Failure> {
        let Query::SourceMotion(MotionQuery::Route(old)) = &predecessor.request.query else {
            return Err(Failure::Binding);
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding);
        };
        let saved = predecessor
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if route.request.prior_cache.as_deref() != Some(&calculation(predecessor)?.after_cache)
            || route.owner != old.owner
            || route.collision_width != old.collision_width
            || route.collision_height != old.collision_height
            || route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.binding.query_id <= old.request.binding.query_id
            || route.request.binding.now_ms <= old.request.binding.now_ms
            || context.binding.tick <= predecessor.request.binding.tick
            || context.binding.world != predecessor.request.binding.world
            || context.binding.profile != predecessor.request.binding.profile
            || context.binding.geometry != predecessor.request.binding.geometry
            || context.binding.authority_policy != predecessor.request.binding.authority_policy
            || context.binding.npc != predecessor.request.binding.npc
            || current.first_construction()
            || current.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || current.construction_binding() != saved.construction_binding()
            || current.configuration_binding() != saved.configuration_binding()
            || current.same_live_dispatch(saved)
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    fn validate_retained_candidate(
        &self,
        candidate: &ResidentRetainedCandidate,
    ) -> Result<(), Failure> {
        if !self.one_retained || self.profile.npcs.len() != 1 {
            return Err(Failure::Binding);
        }
        self.validate_candidate(&candidate.predecessor)?;
        self.validate_retained_context(&candidate.predecessor, &candidate.reply.request)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &candidate.reply.request.query else {
            return Err(Failure::Binding);
        };
        let calculation = calculation(&candidate.reply)?;
        calculation
            .validate_for(&route.request)
            .map_err(|_| Failure::Binding)?;
        let before = &self::calculation(&candidate.predecessor.reply)?.after_cache;
        if !candidate.installed_owner.same_owner(&self.owner)
            || !candidate
                .installed_owner
                .same_owner(&candidate.predecessor.installed_owner)
            || candidate.raw_revision != candidate.predecessor.raw_revision
            || !self.provider.matches_cache(&calculation.after_cache)
            || calculation.source != motion::RouteSource::OwnerPathFinder
            || calculation.prior_cache.as_deref() != Some(before)
            || before.revision != 1
            || calculation.after_cache.revision != 2
            || calculation.after_cache.active_len != 1
            || calculation.after_cache.path_type != 1
            || calculation.after_cache.binding != before.binding
            || calculation.after_cache.include_flags != before.include_flags
            || calculation.after_cache.exclude_flags != before.exclude_flags
            || !matches!(calculation.outcome, motion::RouteOutcome::Complete(_))
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    fn validate_candidate(&self, candidate: &ResidentColdCandidate) -> Result<(), Failure> {
        let reply = &candidate.reply;
        let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
            return Err(Failure::Binding);
        };
        let dispatch = reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let calculation = calculation(reply)?;
        calculation
            .validate_for(&route.request)
            .map_err(|_| Failure::Binding)?;
        let include = benilla_sim::route_filter::initial_include_flags(
            route.owner.capabilities().map_err(|_| Failure::Binding)?,
        )
        .ok_or(Failure::Binding)?;
        if !candidate.installed_owner.same_owner(&self.owner)
            || !self.provider.matches_cache(&calculation.after_cache)
            || route.request.kind != motion::RouteKind::ChaseInitial
            || !dispatch.first_construction()
            || route.request.prior_cache.is_some()
            || calculation.prior_cache.is_some()
            || calculation.source != motion::RouteSource::OwnerPathFinder
            || calculation.after_cache.revision != 1
            || calculation.after_cache.active_len != 1
            || calculation.after_cache.path_type != 1
            || calculation.after_cache.include_flags != include
            || calculation.after_cache.exclude_flags != 0
            || calculation.after_cache.binding.owner_generation != route.owner.npc().generation
            || calculation.after_cache.binding.navigation_policy != self.session.geometry
            || reply.request.binding.world != self.session.world
            || reply.request.binding.profile != self.session.profile
            || reply.request.binding.geometry != self.session.geometry
            || reply.request.binding.npc != Some(route.owner.npc())
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    /// Historical bookkeeping only: no source read, wall-clock or physics query.
    /// All selections and heads validate before any boundary/pruning/publication.
    pub(in super::super) fn acknowledge(
        &mut self,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        let boundary = notice.boundary();
        self.binding(boundary)?;
        if self
            .last_notice
            .as_ref()
            .is_some_and(|old| same_notice(&self.profile, old, notice))
        {
            return Ok(());
        }
        let previous = self.boundary.as_ref().ok_or(Failure::Binding)?;
        if boundary.session() != previous.session()
            || previous.tick().checked_add(1) != Some(boundary.tick())
            || notice.previous_record() != previous.record()
            || notice.before() != previous.state()
            || boundary.record() == previous.record()
            || self.route_epoch_committed
                && !notice.routes().is_empty()
                && (if self.finite_retained || self.finite_straight {
                    self.epochs_committed >= 8
                } else if self.two_owner_first_retained {
                    self.retained_epoch_committed
                } else if self.two_retained {
                    self.epochs_committed >= 3
                } else {
                    !self.one_retained || self.retained_epoch_committed
                })
        {
            return Err(Failure::Binding);
        }
        if self.two_owner_first_retained {
            self.validate_pair_notice_owners(notice)?;
        }
        let mut selected: Vec<(NpcId, usize)> = vec![];
        let mut last_id = None;
        for reply in notice.routes() {
            if reply.request.binding.tick != boundary.tick() {
                return Err(Failure::Binding);
            }
            let mut matching = self.candidates.iter().enumerate().filter(|(_, entry)| {
                entry.candidate.reply() == reply
                    && same_reply_dispatch(
                        &self.profile,
                        &entry.candidate.reply().request,
                        &reply.request,
                    )
            });
            let (index, entry) = matching.next().ok_or(Failure::Binding)?;
            if matching.next().is_some()
                || entry.committed
                || last_id.is_some_and(|id| id >= entry.id)
            {
                return Err(Failure::Binding);
            }
            self.validate_entry(&entry.candidate)?;
            if self.raw != Some(entry.candidate.raw_revision()) {
                return Err(Failure::Binding);
            }
            let npc = reply.request.binding.npc.ok_or(Failure::Binding)?;
            // Old and retried calculations can have identical bytes/cache. Only
            // the last synchronous attempt can be the successful controller step.
            // Retain older allocations diagnostically until this whole ACK validates.
            if self.candidates.iter().any(|later| {
                later.id > entry.id
                    && later.candidate.reply().request.binding.npc == Some(npc)
                    && later.candidate.reply().request.binding.tick == boundary.tick()
            }) {
                return Err(Failure::Binding);
            }

            if selected.iter().any(|(owner, _)| *owner == npc) {
                return Err(Failure::Binding);
            }
            let old = previous
                .heads()
                .iter()
                .find(|head| head.npc() == npc)
                .ok_or(Failure::Binding)?;
            match &entry.candidate {
                Candidate::Cold(_) if old.cache().is_none() => {}
                Candidate::Retained(candidate) => {
                    let committed = self.committed_cold()?;
                    if notice.routes().len() != 1
                        || !Rc::ptr_eq(committed, &candidate.predecessor)
                        || old.cache() != Some(&calculation(&committed.reply)?.after_cache)
                    {
                        return Err(Failure::Binding);
                    }
                }
                Candidate::WarmProposal(proposal) if self.two_owner_first_retained => {
                    self.validate_pair_proposal(proposal)?;
                    let committed = self.committed_pair_cold(npc)?;
                    let WarmPredecessor::Cold(origin) = &proposal.predecessor else {
                        return Err(Failure::Binding);
                    };
                    if !Rc::ptr_eq(committed, origin)
                        || old.cache() != Some(&calculation(&committed.reply)?.after_cache)
                    {
                        return Err(Failure::Binding);
                    }
                }
                Candidate::WarmProposal(proposal)
                    if self.two_retained || self.finite_retained || self.finite_straight =>
                {
                    if notice.routes().len() != 1 {
                        return Err(Failure::Binding);
                    }
                    match &proposal.predecessor {
                        WarmPredecessor::Cold(origin) => {
                            let committed = self.committed_cold()?;
                            if self.epochs_committed != 1
                                || !Rc::ptr_eq(committed, origin)
                                || old.cache() != Some(&calculation(&committed.reply)?.after_cache)
                            {
                                return Err(Failure::Binding);
                            }
                        }
                        WarmPredecessor::Warm(prior) => {
                            let committed = self.committed_warm()?;
                            if !(if self.finite_retained || self.finite_straight {
                                (2..8).contains(&self.epochs_committed)
                            } else {
                                self.epochs_committed == 2
                            }) || !Rc::ptr_eq(committed, prior)
                                || old.cache() != Some(&calculation(&committed.reply)?.after_cache)
                            {
                                return Err(Failure::Binding);
                            }
                        }
                    }
                }
                _ => return Err(Failure::Binding),
            }
            selected.push((npc, index));
            last_id = Some(entry.id);
        }
        for (old, head) in previous.heads().iter().zip(boundary.heads()) {
            if old.npc() != head.npc() {
                return Err(Failure::Binding);
            }
            if let Some((_, index)) = selected.iter().find(|(npc, _)| *npc == head.npc()) {
                let reply = self.candidates[*index].candidate.reply();
                let dispatch = reply
                    .request
                    .require_pathfinder_dispatch(&self.profile)
                    .map_err(|_| Failure::Binding)?;
                if head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
                    || head.construction() != Some(dispatch.construction_binding())
                    || head.cache() != Some(&calculation(reply)?.after_cache)
                {
                    return Err(Failure::Binding);
                }
            } else if head != old {
                // Deliberately no Home/Wander/replacement/drop-owner lifecycle here.
                // A cache-free Chase may become the active generator before its first query.
                if self.route_epoch_committed
                    || head.cache().is_some()
                    || old.cache().is_some()
                    || old.construction().is_some()
                    || head.construction().is_some()
                    || old.constructor_kind().is_some()
                    || head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
                {
                    return Err(Failure::Binding);
                }
            }
        }
        // Prepare bounded bookkeeping before the first mutation. This is not a
        // recoverable allocator-OOM contract; promotion itself only moves/clones
        // Rc handles and retires already-owned entries after semantic validation.
        let next_boundary = boundary.clone();
        let next_notice = notice.clone();
        let selected_ids: Vec<_> = selected
            .iter()
            .map(|(_, index)| self.candidates[*index].id)
            .collect();
        let retained_selected = selected.iter().any(|(_, index)| {
            matches!(
                &self.candidates[*index].candidate,
                Candidate::Retained(_) | Candidate::WarmProposal(_)
            )
        });
        let next_epochs = if selected_ids.is_empty() {
            self.epochs_committed
        } else {
            self.epochs_committed
                .checked_add(1)
                .ok_or(Failure::Binding)?
        };
        // No failure or allocation follows this line. Staged rollback ancestors
        // survive earlier idle notices until their actual tick is acknowledged.
        for entry in &mut self.candidates {
            if selected_ids.contains(&entry.id) {
                if let Candidate::WarmProposal(proposal) = &entry.candidate {
                    entry.candidate = Candidate::WarmHead(Rc::clone(&proposal.head));
                }
                entry.committed = true;
            } else if selected
                .iter()
                .any(|(npc, _)| entry.candidate.reply().request.binding.npc == Some(*npc))
            {
                entry.committed = false;
            }
        }
        self.candidates.retain(|entry| {
            entry.committed
                || (!(self.finite_retained || self.finite_straight) || selected_ids.is_empty())
                    && entry.candidate.reply().request.binding.tick > boundary.tick()
        });
        self.epochs_committed = next_epochs;
        self.route_epoch_committed |= !selected_ids.is_empty();
        self.retained_epoch_committed |= retained_selected;
        self.boundary = Some(next_boundary);
        self.last_notice = Some(next_notice);
        Ok(())
    }
    /// Source-only diagnostic check, never a reply re-emitter or physics certificate.
    pub(in super::super) fn check_fresh_candidate(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<(), ResidentFilterError> {
        if !self.provider.matches(provider) {
            return Err(Failure::Binding.into());
        }
        let candidate = self
            .candidates
            .iter()
            .find(|entry| {
                same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
            })
            .ok_or(Failure::Binding)?;
        match &candidate.candidate {
            Candidate::Cold(candidate) => provider
                .joined_for(&candidate.dependency, context, deadline, || false)
                .map(|_| ()),
            Candidate::Retained(candidate) => {
                // Compare the entire live native-derived storage, including its
                // inactive tail, rather than assuming the retained backing length.
                let native_cache = provider.retained_cache(
                    &candidate.dependency,
                    &candidate.predecessor.dependency,
                    context,
                    deadline,
                    || false,
                )?;
                if native_cache != calculation(&candidate.reply)?.after_cache {
                    return Err(Failure::Binding.into());
                }
                Ok(())
            }
            Candidate::WarmHead(head) => self.check_fresh_warm(provider, head, context, deadline),
            Candidate::WarmProposal(proposal) => {
                self.check_fresh_warm(provider, &proposal.head, context, deadline)
            }
        }
    }
    pub(in super::super) fn require_open_epoch(&self) -> Result<(), Failure> {
        if self.route_epoch_committed {
            Err(Failure::UnsupportedQuery)
        } else {
            Ok(())
        }
    }
    /// Read-only component probe, not retained calculation authority. Its root is
    /// the ACK-selected opaque candidate, never a copied prior-cache descriptor.
    pub(in super::super) fn check_retained_dispatch_for_test(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) -> Result<(), Failure> {
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        if !self.route_epoch_committed
            || !self.provider.matches(provider)
            || self.counts() != (1, 1)
            || boundary.tick().checked_add(1) != Some(context.binding.tick)
        {
            return Err(Failure::Binding);
        }
        let candidate = self.committed_cold()?;
        self.validate_candidate(candidate)?;
        let Query::SourceMotion(MotionQuery::Route(old)) = &candidate.reply.request.query else {
            return Err(Failure::Binding);
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding);
        };
        let saved = candidate
            .reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let cache = &calculation(&candidate.reply)?.after_cache;
        let head = boundary.heads().first().ok_or(Failure::Binding)?;
        if boundary.heads().len() != 1
            || head.npc() != route.owner.npc()
            || head.construction() != Some(saved.construction_binding())
            || head.cache() != Some(cache)
            || route.request.prior_cache.as_deref() != Some(cache)
            || route.owner != old.owner
            || route.collision_width != old.collision_width
            || route.collision_height != old.collision_height
            || route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.binding.query_id <= old.request.binding.query_id
            || route.request.binding.now_ms <= old.request.binding.now_ms
            || context.binding.tick <= candidate.reply.request.binding.tick
            || context.binding.world != candidate.reply.request.binding.world
            || context.binding.profile != candidate.reply.request.binding.profile
            || context.binding.geometry != candidate.reply.request.binding.geometry
            || context.binding.authority_policy != candidate.reply.request.binding.authority_policy
            || context.binding.npc != candidate.reply.request.binding.npc
            || current.first_construction()
            || current.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || current.construction_binding() != saved.construction_binding()
            || current.configuration_binding() != saved.configuration_binding()
            || current.same_live_dispatch(saved)
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }
    /// A post-revision-two witness only: no source freshness, terminal calculation,
    /// scene clearance or Route emission. The two-epoch certification stop is unchanged.
    pub(in super::super) fn check_second_retained_dispatch_for_test(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        installed_owner: &benilla_formats::NativeInventorySource,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        if !self.one_retained
            || !self.route_epoch_committed
            || !self.retained_epoch_committed
            || !self.provider.matches(provider)
            || !self.owner.same_owner(installed_owner)
            || self.counts() != (1, 1)
            || boundary.tick().checked_add(1) != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let Candidate::Retained(candidate) = &self.candidates[0].candidate else {
            return Err(Failure::Binding.into());
        };
        self.validate_retained_candidate(candidate)?;
        let saved_context = &candidate.reply.request;
        let cache = &calculation(&candidate.reply)?.after_cache;
        provider.check_historical_retained_cache_for_test(
            &candidate.dependency,
            &candidate.predecessor.dependency,
            saved_context,
            cache,
        )?;
        let Query::SourceMotion(MotionQuery::Route(old)) = &saved_context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        let saved = saved_context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let head = boundary.heads().first().ok_or(Failure::Binding)?;
        if boundary.heads().len() != 1
            || head.npc() != route.owner.npc()
            || head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
            || head.construction() != Some(saved.construction_binding())
            || head.cache() != Some(cache)
            || route.request.prior_cache.as_deref() != Some(cache)
            || !candidate.installed_owner.same_owner(installed_owner)
            || self.raw != Some(candidate.raw_revision)
            || cache.revision != 2
            || route.owner != old.owner
            || route.collision_width != old.collision_width
            || route.collision_height != old.collision_height
            || route.request.kind != motion::RouteKind::ChaseReplan
            || route.request.binding.query_id <= old.request.binding.query_id
            || route.request.binding.now_ms <= old.request.binding.now_ms
            || context.binding.tick <= saved_context.binding.tick
            || context.binding.world != saved_context.binding.world
            || context.binding.profile != saved_context.binding.profile
            || context.binding.geometry != saved_context.binding.geometry
            || context.binding.authority_policy != saved_context.binding.authority_policy
            || context.binding.npc != saved_context.binding.npc
            || current.first_construction()
            || current.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || current.construction_binding() != saved.construction_binding()
            || current.configuration_binding() != saved.configuration_binding()
            || current.same_live_dispatch(saved)
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    fn committed_warm(&self) -> Result<&Rc<WarmHead>, Failure> {
        let limit = if self.finite_retained || self.finite_straight {
            8
        } else {
            3
        };
        if !(self.two_retained || self.finite_retained || self.finite_straight)
            || !(2..=limit).contains(&self.epochs_committed)
        {
            return Err(Failure::Binding);
        }
        let mut committed = self.candidates.iter().filter(|entry| entry.committed);
        let entry = committed.next().ok_or(Failure::Binding)?;
        if committed.next().is_some() {
            return Err(Failure::Binding);
        }
        let Candidate::WarmHead(head) = &entry.candidate else {
            return Err(Failure::Binding);
        };
        self.validate_warm_head(head)?;
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        let owner = boundary.heads().first().ok_or(Failure::Binding)?;
        let dispatch = head
            .reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let cache = &calculation(&head.reply)?.after_cache;
        if boundary.heads().len() != 1
            || Some(owner.npc()) != head.reply.request.binding.npc
            || owner.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
            || owner.construction() != Some(dispatch.construction_binding())
            || owner.cache() != Some(cache)
            || cache.revision != u64::from(self.epochs_committed)
            || self.raw != Some(head.raw_revision)
        {
            return Err(Failure::Binding);
        }
        Ok(head)
    }
    fn validate_warm_head(&self, head: &WarmHead) -> Result<(), Failure> {
        if !(self.two_retained || self.finite_retained || self.finite_straight)
            || self.profile.npcs.len() != 1
        {
            return Err(Failure::Binding);
        }
        self.validate_warm_head_payload(head)
    }
    fn validate_warm_head_payload(&self, head: &WarmHead) -> Result<(), Failure> {
        self.validate_candidate(&head.origin)?;
        let Query::SourceMotion(MotionQuery::Route(origin)) = &head.origin.reply.request.query
        else {
            return Err(Failure::Binding);
        };
        let Query::SourceMotion(MotionQuery::Route(route)) = &head.reply.request.query else {
            return Err(Failure::Binding);
        };
        let current = head
            .reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let cold = head
            .origin
            .reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let calculation = calculation(&head.reply)?;
        calculation
            .validate_for(&route.request)
            .map_err(|_| Failure::Binding)?;
        let prior = calculation.prior_cache.as_deref().ok_or(Failure::Binding)?;
        let revision = match &head.dependency {
            WarmDependency::First(_)
                if self.two_retained || self.finite_retained || self.two_owner_first_retained =>
            {
                2
            }
            WarmDependency::Second(_) if self.two_retained && !self.finite_retained => 3,
            WarmDependency::Repeated(dependency) if self.finite_retained => {
                let revision = dependency.source_revision();
                if !(3..=8).contains(&revision) {
                    return Err(Failure::Binding);
                }
                revision
            }
            WarmDependency::Straight(dependency) if self.finite_straight => {
                let revision = dependency.source_revision();
                if !(2..=MAX_SOURCE_REVISION).contains(&revision) {
                    return Err(Failure::Binding);
                }
                let motion::RouteOutcome::Complete(complete) = &calculation.outcome else {
                    return Err(Failure::Binding);
                };
                dependency
                    .check_historical_native_points_for_test(&complete.points)
                    .map_err(|_| Failure::Binding)?;
                revision
            }
            _ => return Err(Failure::Binding),
        };
        if !head.installed_owner.same_owner(&self.owner)
            || !head
                .installed_owner
                .same_owner(&head.origin.installed_owner)
            || head.raw_revision != head.origin.raw_revision
            || !self.provider.matches_cache(&calculation.after_cache)
            || route.owner != origin.owner
            || route.collision_width != origin.collision_width
            || route.collision_height != origin.collision_height
            || route.request.kind != motion::RouteKind::ChaseReplan
            || current.first_construction()
            || current.constructor_kind() != motion::PathfinderConstructorKind::ChaseLazy
            || current.construction_binding() != cold.construction_binding()
            || current.configuration_binding() != cold.configuration_binding()
            || current.same_live_dispatch(cold)
            || head.reply.request.binding.tick <= head.origin.reply.request.binding.tick
            || head.reply.request.binding.world != self.session.world
            || head.reply.request.binding.profile != self.session.profile
            || head.reply.request.binding.geometry != self.session.geometry
            || head.reply.request.binding.authority_policy != self.session.policy
            || head.reply.request.binding.npc != Some(route.owner.npc())
            || route.request.prior_cache.as_deref() != Some(prior)
            || calculation.source != motion::RouteSource::OwnerPathFinder
            || prior.revision.checked_add(1) != Some(revision)
            || calculation.after_cache.revision != revision
            || (!self.finite_straight && calculation.after_cache.active_len != 1)
            || calculation.after_cache.path_type != 1
            || calculation.after_cache.binding != prior.binding
            || calculation.after_cache.binding
                != self::calculation(&head.origin.reply)?.after_cache.binding
            || !matches!(calculation.outcome, motion::RouteOutcome::Complete(_))
        {
            return Err(Failure::Binding);
        }
        head.dependency.check_cache(&calculation.after_cache)
    }
    fn validate_warm_proposal(&self, proposal: &StagedWarmProposal) -> Result<(), Failure> {
        self.validate_warm_head(&proposal.head)?;
        match (&proposal.predecessor, &proposal.head.dependency) {
            (WarmPredecessor::Cold(origin), WarmDependency::First(_)) => {
                if !Rc::ptr_eq(origin, &proposal.head.origin) {
                    return Err(Failure::Binding);
                }
                self.validate_retained_context(origin, &proposal.head.reply.request)
            }
            (WarmPredecessor::Warm(prior), WarmDependency::Second(_)) => {
                if !matches!(&prior.dependency, WarmDependency::First(_))
                    || !Rc::ptr_eq(&prior.origin, &proposal.head.origin)
                {
                    return Err(Failure::Binding);
                }
                self.validate_warm_head(prior)?;
                self.validate_retained_reply_context(&prior.reply, &proposal.head.reply.request)
            }
            (WarmPredecessor::Warm(prior), WarmDependency::Repeated(dependency))
                if self.finite_retained =>
            {
                if !Rc::ptr_eq(&prior.origin, &proposal.head.origin) {
                    return Err(Failure::Binding);
                }
                self.validate_warm_head(prior)?;
                self.validate_retained_reply_context(&prior.reply, &proposal.head.reply.request)?;
                dependency
                    .check_historical_native_predecessor_for_test(
                        &prior.dependency.current_native()?,
                    )
                    .map_err(|_| Failure::Binding)
            }
            (WarmPredecessor::Cold(origin), WarmDependency::Straight(dependency))
                if self.finite_straight =>
            {
                if !Rc::ptr_eq(origin, &proposal.head.origin) {
                    return Err(Failure::Binding);
                }
                self.validate_retained_context(origin, &proposal.head.reply.request)?;
                dependency
                    .check_historical_native_predecessor_for_test(
                        &origin.dependency.current_straight_native(),
                    )
                    .map_err(|_| Failure::Binding)
            }
            (WarmPredecessor::Warm(prior), WarmDependency::Straight(dependency))
                if self.finite_straight =>
            {
                let WarmDependency::Straight(previous) = &prior.dependency else {
                    return Err(Failure::Binding);
                };
                if !Rc::ptr_eq(&prior.origin, &proposal.head.origin) {
                    return Err(Failure::Binding);
                }
                self.validate_warm_head(prior)?;
                self.validate_retained_reply_context(&prior.reply, &proposal.head.reply.request)?;
                dependency
                    .check_historical_native_predecessor_for_test(
                        &previous.current_straight_native(),
                    )
                    .map_err(|_| Failure::Binding)
            }
            _ => Err(Failure::Binding),
        }
    }
    fn check_fresh_warm(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        head: &WarmHead,
        context: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<(), ResidentFilterError> {
        let cache = match &head.dependency {
            WarmDependency::First(dependency) => provider.retained_cache(
                dependency,
                &head.origin.dependency,
                context,
                deadline,
                || false,
            )?,
            WarmDependency::Second(dependency) => {
                // The previous warm head may already be gone: only bounded native
                // ancestry, its fixed cold origin and this current dispatch remain.
                provider.check_historical_second_retained_cache_for_test(
                    dependency,
                    &head.origin.dependency,
                    context,
                    &calculation(&head.reply)?.after_cache,
                )?;
                provider.second_retained_cache(
                    dependency,
                    &head.origin.dependency,
                    context,
                    deadline,
                    || false,
                )?
            }
            WarmDependency::Repeated(dependency) => {
                provider.check_historical_repeated_cache_for_test(
                    dependency,
                    &head.origin.dependency,
                    context,
                    &calculation(&head.reply)?.after_cache,
                )?;
                provider.repeated_cache(
                    dependency,
                    &head.origin.dependency,
                    context,
                    deadline,
                    || false,
                )?
            }
            WarmDependency::Straight(dependency) => {
                provider.check_historical_straight_cache_for_test(
                    dependency,
                    &head.origin.dependency,
                    context,
                    &calculation(&head.reply)?.after_cache,
                )?;
                provider.straight_cache_for(
                    dependency,
                    &head.origin.dependency,
                    context,
                    deadline,
                    || false,
                )?
            }
        };
        if cache != calculation(&head.reply)?.after_cache {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_second_retained(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.certify_second_retained_observed(
            world,
            compatibility,
            provider,
            context,
            first_ticket,
            deadline,
            cancelled,
            || {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_second_retained_observed(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        // The fourth epoch is closed before source reads, scene borrows or allocation.
        provider.require_two_retained_routes()?;
        if !self.two_retained || self.epochs_committed != 2 {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || !self.owner.same_owner(compatibility.owner())
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = Rc::clone(self.committed_warm()?);
        self.validate_retained_reply_context(&predecessor.reply, context)?;
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply().request.binding.npc == context.binding.npc
                && entry.candidate.reply().request.binding.tick == context.binding.tick
                && entry.candidate.reply().request != *context
        }) {
            return Err(Failure::UnsupportedQuery.into());
        }
        if self.candidates.iter().any(|entry| {
            same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
        }) {
            return Err(Failure::Binding.into());
        }
        if self.candidates.len() == MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        // Binding reserves the complete bounded staging capacity before certification.
        if self.candidates.capacity() < MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let (proposal, facts) = super::three_epoch::certify(
            world,
            compatibility,
            provider,
            predecessor,
            context,
            self.player,
            first_ticket,
            deadline,
            cancelled,
            after,
        )?;
        let WarmPredecessor::Warm(prior) = &proposal.predecessor else {
            return Err(Failure::Binding.into());
        };
        if !Rc::ptr_eq(self.committed_warm()?, prior)
            || self.raw != Some(proposal.head.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        self.validate_warm_proposal(&proposal)?;
        let (WarmDependency::Second(dependency), WarmDependency::First(previous)) =
            (&proposal.head.dependency, &prior.dependency)
        else {
            return Err(Failure::Binding.into());
        };
        provider.check_historical_second_retained_predecessor_for_test(
            dependency,
            &proposal.head.origin.dependency,
            previous,
            context,
        )?;
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Candidate::WarmProposal(Rc::new(proposal)),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    /// Source diagnostics rooted at the current ACK-selected first warm terminal.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn probe_second_retained_source_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(
            super::super::source_route_filter::resident::retained::ResidentRetainedPhase,
        ),
    ) -> Result<(), ResidentFilterError> {
        provider.require_two_retained_routes()?;
        if !self.two_retained || self.epochs_committed != 2 {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = self.committed_warm()?;
        self.validate_retained_reply_context(&predecessor.reply, context)?;
        let WarmDependency::First(prior) = &predecessor.dependency else {
            return Err(Failure::Binding.into());
        };
        provider
            .observe_joined_second_retained_observed(
                &predecessor.origin.dependency,
                prior,
                context,
                self.player,
                first_ticket,
                deadline,
                cancelled,
                observe,
            )
            .map(|_| ())
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_repeated(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.certify_repeated_observed(
            world,
            compatibility,
            provider,
            context,
            first_ticket,
            deadline,
            cancelled,
            || {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_repeated_observed(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        // The ninth epoch is closed before source reads, scene borrows or allocation.
        provider.require_finite_retained_routes()?;
        if !self.finite_retained || !(2..8).contains(&self.epochs_committed) {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || !self.owner.same_owner(compatibility.owner())
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = Rc::clone(self.committed_warm()?);
        self.validate_retained_reply_context(&predecessor.reply, context)?;
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply().request.binding.npc == context.binding.npc
                && entry.candidate.reply().request.binding.tick == context.binding.tick
                && entry.candidate.reply().request != *context
        }) {
            return Err(Failure::UnsupportedQuery.into());
        }
        if self.candidates.iter().any(|entry| {
            same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
        }) {
            return Err(Failure::Binding.into());
        }
        if self.candidates.len() == MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        // Binding reserves the complete bounded staging capacity before certification.
        if self.candidates.capacity() < MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let (proposal, facts) = super::finite_loop::certify(
            world,
            compatibility,
            provider,
            predecessor,
            context,
            self.player,
            first_ticket,
            deadline,
            cancelled,
            after,
        )?;
        let WarmPredecessor::Warm(prior) = &proposal.predecessor else {
            return Err(Failure::Binding.into());
        };
        if !Rc::ptr_eq(self.committed_warm()?, prior)
            || self.raw != Some(proposal.head.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        self.validate_warm_proposal(&proposal)?;
        let WarmDependency::Repeated(dependency) = &proposal.head.dependency else {
            return Err(Failure::Binding.into());
        };
        provider.check_historical_repeated_predecessor_for_test(
            dependency,
            &proposal.head.origin.dependency,
            &prior.dependency.current_native()?,
            context,
        )?;
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Candidate::WarmProposal(Rc::new(proposal)),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    /// Source diagnostics rooted at the actual ACK-selected current warm terminal.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn probe_repeated_source_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(
            super::super::source_route_filter::resident::retained::ResidentRetainedPhase,
        ),
    ) -> Result<(), ResidentFilterError> {
        provider.require_finite_retained_routes()?;
        if !self.finite_retained || !(2..8).contains(&self.epochs_committed) {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = self.committed_warm()?;
        self.validate_retained_reply_context(&predecessor.reply, context)?;
        let prior = predecessor.dependency.current_native()?;
        provider
            .observe_joined_repeated_observed(
                &predecessor.origin.dependency,
                &prior,
                context,
                self.player,
                first_ticket,
                deadline,
                cancelled,
                observe,
            )
            .map(|_| ())
    }
    /// Diagnostic selection remains valid at the final epoch, although no new
    /// native work or Facts may be issued there. It always selects the actual ACK head.
    pub(in super::super) fn check_repeated_dispatch_for_test(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        installed_owner: &benilla_formats::NativeInventorySource,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        provider.require_finite_retained_routes()?;
        if !self.finite_retained
            || !self.provider.matches(provider)
            || !self.owner.same_owner(installed_owner)
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let head = self.committed_warm()?;
        if !head.installed_owner.same_owner(installed_owner) {
            return Err(Failure::Binding.into());
        }
        self.validate_retained_reply_context(&head.reply, context)?;
        let cache = &calculation(&head.reply)?.after_cache;
        match &head.dependency {
            WarmDependency::First(dependency) => {
                provider.check_historical_retained_cache_for_test(
                    dependency,
                    &head.origin.dependency,
                    &head.reply.request,
                    cache,
                )?;
            }
            WarmDependency::Repeated(dependency) => {
                provider.check_historical_repeated_cache_for_test(
                    dependency,
                    &head.origin.dependency,
                    &head.reply.request,
                    cache,
                )?;
            }
            WarmDependency::Second(_) | WarmDependency::Straight(_) => {
                return Err(Failure::Binding.into())
            }
        }
        Ok(())
    }
    /// Select only the actual ACK-owned cold or straight head, never a staged sibling.
    fn committed_straight(&self) -> Result<WarmPredecessor, Failure> {
        if !self.finite_straight {
            return Err(Failure::UnsupportedQuery);
        }
        if self.epochs_committed == 1 {
            Ok(WarmPredecessor::Cold(Rc::clone(self.committed_cold()?)))
        } else {
            let head = self.committed_warm()?;
            if !matches!(&head.dependency, WarmDependency::Straight(_)) {
                return Err(Failure::Binding);
            }
            Ok(WarmPredecessor::Warm(Rc::clone(head)))
        }
    }
    fn same_committed_straight(&self, prior: &WarmPredecessor) -> Result<(), Failure> {
        let same = match prior {
            WarmPredecessor::Cold(origin) => Rc::ptr_eq(self.committed_cold()?, origin),
            WarmPredecessor::Warm(head) => Rc::ptr_eq(self.committed_warm()?, head),
        };
        if same {
            Ok(())
        } else {
            Err(Failure::Binding)
        }
    }
    fn straight_predecessor_for(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) -> Result<WarmPredecessor, ResidentFilterError> {
        provider.require_finite_straight_routes()?;
        if !self.finite_straight
            || !self.provider.matches(provider)
            || self
                .boundary
                .as_ref()
                .and_then(|boundary| boundary.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let predecessor = self.committed_straight()?;
        // Native full backing and immediate owned root precede current request validation.
        provider.check_historical_straight_current_for_test(
            &predecessor.origin().dependency,
            &predecessor.current_straight_native()?,
            &calculation(predecessor.reply())?.after_cache,
        )?;
        self.validate_retained_reply_context(predecessor.reply(), context)?;
        Ok(predecessor)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_straight(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.certify_straight_observed(
            world,
            compatibility,
            provider,
            context,
            first_ticket,
            deadline,
            cancelled,
            || {},
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_straight_observed(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        self.certify_straight_segments_observed(
            world,
            compatibility,
            provider,
            context,
            first_ticket,
            deadline,
            cancelled,
            |_, _| {},
            after,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn certify_straight_segments_observed(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        on_segment: impl FnMut(StraightSegmentPhase, usize),
        after: impl FnOnce(),
    ) -> Result<Facts, ResidentFilterError> {
        provider.require_finite_straight_routes()?;
        // No ninth source attempt, ECS borrow or dynamic work is allowed.
        if !self.finite_straight
            || !(1..MAX_SOURCE_REVISION).contains(&u64::from(self.epochs_committed))
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.owner.same_owner(compatibility.owner()) {
            return Err(Failure::Binding.into());
        }
        let predecessor = self.straight_predecessor_for(provider, context)?;
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply().request.binding.npc == context.binding.npc
                && entry.candidate.reply().request.binding.tick == context.binding.tick
                && entry.candidate.reply().request != *context
        }) {
            return Err(Failure::UnsupportedQuery.into());
        }
        if self.candidates.iter().any(|entry| {
            same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
        }) {
            return Err(Failure::Binding.into());
        }
        if self.candidates.len() == MAX_CANDIDATES || self.candidates.capacity() < MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        let (proposal, facts) = super::straight_loop::certify(
            world,
            compatibility,
            provider,
            predecessor,
            context,
            self.player,
            first_ticket,
            deadline,
            cancelled,
            on_segment,
            after,
        )?;
        self.same_committed_straight(&proposal.predecessor)?;
        if self.raw != Some(proposal.head.raw_revision) {
            return Err(Failure::Binding.into());
        }
        self.validate_warm_proposal(&proposal)?;
        let WarmDependency::Straight(dependency) = &proposal.head.dependency else {
            return Err(Failure::Binding.into());
        };
        provider.check_historical_straight_predecessor_for_test(
            dependency,
            &proposal.head.origin.dependency,
            &proposal.predecessor.current_straight_native()?,
            context,
        )?;
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Candidate::WarmProposal(Rc::new(proposal)),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn probe_straight_source_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        observe: impl FnMut(
            super::super::source_route_filter::resident::retained::ResidentRetainedPhase,
        ),
    ) -> Result<(), ResidentFilterError> {
        provider.require_finite_straight_routes()?;
        if !self.finite_straight
            || !(1..MAX_SOURCE_REVISION).contains(&u64::from(self.epochs_committed))
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let predecessor = self.straight_predecessor_for(provider, context)?;
        let prior = predecessor.current_straight_native()?;
        provider
            .observe_joined_straight_observed(
                &predecessor.origin().dependency,
                &prior,
                context,
                self.player,
                first_ticket,
                prior.budget()?,
                deadline,
                cancelled,
                observe,
            )
            .map(|_| ())
    }
    /// Source-only identity controls from the actual committed root. Neither
    /// fresh diagnostic attempt can stage a candidate or escape as Route Facts.
    pub(in super::super) fn probe_generated_flat_bindings_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
    ) -> Result<(), ResidentFilterError> {
        provider.require_generated_flat_support()?;
        if !self.finite_straight
            || !(1..MAX_SOURCE_REVISION).contains(&u64::from(self.epochs_committed))
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let before = self.diagnostic();
        let constructors = provider.retained_constructor_count();
        let predecessor = self.straight_predecessor_for(provider, context)?;
        let prior = predecessor.current_straight_native()?;
        let first = provider.observe_joined_straight(
            &predecessor.origin().dependency,
            &prior,
            context,
            self.player,
            first_ticket,
            deadline,
            || false,
        )?;
        let other = provider.observe_joined_straight(
            &predecessor.origin().dependency,
            &prior,
            context,
            self.player,
            first_ticket,
            deadline,
            || false,
        )?;
        let result = provider.assert_generated_flat_support_bindings_for_test(
            &first,
            &other,
            &predecessor.origin().dependency,
            context,
            deadline,
            || false,
        );
        self.same_committed_straight(&predecessor)?;
        assert_eq!(
            self.diagnostic(),
            before,
            "source-only support controls cannot publish"
        );
        assert_eq!(provider.retained_constructor_count(), constructors);
        result
    }
    /// Bounded copied diagnostics only; the temporary native proposal is dropped here.
    #[allow(clippy::too_many_arguments)]
    pub(in super::super) fn probe_straight_geometry_for_test(
        &self,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<StraightGeometryDiagnostic, ResidentFilterError> {
        provider.require_finite_straight_routes()?;
        if !self.finite_straight
            || !(1..MAX_SOURCE_REVISION).contains(&u64::from(self.epochs_committed))
        {
            return Err(Failure::UnsupportedQuery.into());
        }
        let predecessor = self.straight_predecessor_for(provider, context)?;
        let prior = predecessor.current_straight_native()?;
        let dependency = provider.observe_joined_straight(
            &predecessor.origin().dependency,
            &prior,
            context,
            self.player,
            first_ticket,
            deadline,
            &cancelled,
        )?;
        let observation = provider.straight_for(
            &dependency,
            &predecessor.origin().dependency,
            context,
            dependency.deadline(deadline),
            cancelled,
        )?;
        if observation.points.len() > STRAIGHT_MAX_POINTS || observation.trace.len() > 8050 {
            return Err(Failure::Binding.into());
        }
        let mut points = Vec::new();
        points
            .try_reserve_exact(observation.points.len())
            .map_err(|_| Failure::QueueCapacity)?;
        for bits in observation.points {
            let [x, y, z] = bits.map(f32::from_bits);
            let point =
                benilla_sim::creature_ai::Point::new(x, y, z).map_err(|_| Failure::Invalid)?;
            if [point.x.bits(), point.y.bits(), point.z.bits()] != *bits {
                return Err(Failure::UnsupportedQuery.into());
            }
            points.push(point);
        }
        let mut trace = Vec::new();
        trace
            .try_reserve_exact(observation.trace.len())
            .map_err(|_| Failure::QueueCapacity)?;
        trace.extend_from_slice(observation.trace);
        Ok(StraightGeometryDiagnostic {
            points,
            body: observation.body,
            trace,
        })
    }
    /// Historical diagnostic selection remains available after epoch eight.
    pub(in super::super) fn check_straight_dispatch_for_test(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        installed_owner: &benilla_formats::NativeInventorySource,
        context: &AuthorityRequest,
    ) -> Result<(), ResidentFilterError> {
        if !self.owner.same_owner(installed_owner) {
            return Err(Failure::Binding.into());
        }
        let predecessor = self.straight_predecessor_for(provider, context)?;
        if !predecessor
            .origin()
            .installed_owner
            .same_owner(installed_owner)
        {
            return Err(Failure::Binding.into());
        }
        Ok(())
    }
    /// Compare against the latest exact live dispatch and the actual native terminal.
    pub(in super::super) fn check_straight_result_for_test(
        &self,
        context: &AuthorityRequest,
        facts: &Facts,
    ) -> Result<(), Failure> {
        if !self.finite_straight {
            return Err(Failure::UnsupportedQuery);
        }
        let entry = self
            .candidates
            .iter()
            .rev()
            .find(|entry| {
                same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
            })
            .ok_or(Failure::Binding)?;
        if &entry.candidate.reply().facts != facts
            || self.candidates.iter().any(|later| {
                later.id > entry.id
                    && later.candidate.reply().request.binding.npc == context.binding.npc
                    && later.candidate.reply().request.binding.tick == context.binding.tick
            })
        {
            return Err(Failure::Binding);
        }
        match &entry.candidate {
            Candidate::WarmHead(head)
                if matches!(&head.dependency, WarmDependency::Straight(_)) =>
            {
                self.validate_warm_head(head)
            }
            Candidate::WarmProposal(proposal)
                if matches!(&proposal.head.dependency, WarmDependency::Straight(_)) =>
            {
                self.validate_warm_proposal(proposal)
            }
            _ => Err(Failure::Binding),
        }
    }
    pub(in super::super) fn cold_liveness_for_test(&self) -> Result<ColdLiveness, Failure> {
        self.candidates
            .iter()
            .find_map(|entry| match &entry.candidate {
                Candidate::Cold(cold) => Some(ColdLiveness {
                    cold: Rc::downgrade(cold),
                }),
                Candidate::Retained(retained) => Some(ColdLiveness {
                    cold: Rc::downgrade(&retained.predecessor),
                }),
                _ => None,
            })
            .ok_or(Failure::Binding)
    }

    pub(in super::super) fn committed_warm_liveness_for_test(
        &self,
    ) -> Result<WarmLiveness, Failure> {
        let head = self.committed_warm()?;
        Ok(WarmLiveness {
            head: Rc::downgrade(head),
            cold: Rc::downgrade(&head.origin),
        })
    }
    pub(in super::super) fn staged_warm_liveness_for_test(
        &self,
        context: &AuthorityRequest,
    ) -> Result<ProposalLiveness, Failure> {
        let entry = self
            .candidates
            .iter()
            .find(|entry| {
                !entry.committed
                    && same_reply_dispatch(&self.profile, &entry.candidate.reply().request, context)
            })
            .ok_or(Failure::Binding)?;
        let Candidate::WarmProposal(proposal) = &entry.candidate else {
            return Err(Failure::Binding);
        };
        Ok(ProposalLiveness {
            proposal: Rc::downgrade(proposal),
            head: Rc::downgrade(&proposal.head),
            cold: Rc::downgrade(&proposal.head.origin),
        })
    }
    pub(in super::super) fn check_committed_warm_for_test(&self) -> Result<(), Failure> {
        self.committed_warm().map(|_| ())
    }
    pub(in super::super) fn flat_ownership_for_test(&self) -> Result<OwnershipDiagnostic, Failure> {
        if !(self.two_retained || self.finite_retained || self.finite_straight) {
            return Err(Failure::UnsupportedQuery);
        }
        self.flat_ownership_inner(1)
    }
    fn flat_ownership_inner(&self, owners: usize) -> Result<OwnershipDiagnostic, Failure> {
        let mut cold = Vec::<&Rc<ResidentColdCandidate>>::new();
        let mut warm = Vec::<&Rc<WarmHead>>::new();
        let mut staged_count = 0;
        for entry in &self.candidates {
            match &entry.candidate {
                Candidate::Cold(origin) => push_unique(&mut cold, origin),
                Candidate::WarmHead(head) => push_unique(&mut warm, head),
                Candidate::WarmProposal(proposal) => {
                    staged_count += 1;
                    push_unique(&mut warm, &proposal.head);
                    match &proposal.predecessor {
                        WarmPredecessor::Cold(origin) => push_unique(&mut cold, origin),
                        WarmPredecessor::Warm(head) => push_unique(&mut warm, head),
                    }
                }
                Candidate::Retained(_) => return Err(Failure::Binding),
            }
        }
        for head in &warm {
            push_unique(&mut cold, &head.origin);
        }
        let terminal_count = cold.len() + warm.len();
        let mut storage_bytes = 0;
        for origin in &cold {
            storage_bytes += super::super::source_route_filter::resident::retained::cold_storage_footprint_for_test(&origin.dependency);
            storage_bytes += reply_storage_bytes(&origin.reply)?;
        }
        for head in &warm {
            storage_bytes += head.dependency.storage_bytes();
            storage_bytes += reply_storage_bytes(&head.reply)?;
        }
        if let Some(boundary) = &self.boundary {
            storage_bytes += boundary_storage_bytes(boundary);
        }
        if let Some(notice) = &self.last_notice {
            storage_bytes += boundary_storage_bytes(notice.boundary());
            for reply in notice.routes() {
                storage_bytes += reply_storage_bytes(reply)?;
            }
        }
        // Each terminal has three native storage slices, at most three source-context
        // copies and four reply caches. The notice adds four copies plus two heads.
        // The old same-polygon scan bound is 2 * (4095 - 74) + 8 = 8050;
        // straight diagnostics use their separately checked complete work budget.
        let cache_bound = motion::MAX_PATHFINDER_CACHE_REFS * std::mem::size_of::<u64>();
        let points_and_checks = 2 * std::mem::size_of::<[u32; 3]>()
            + 2 * std::mem::size_of::<benilla_sim::creature_ai::Point>()
            + std::mem::size_of::<SegmentChecks>();
        let upper_bound_bytes = if self.finite_straight {
            let route_geometry = STRAIGHT_MAX_POINTS
                * std::mem::size_of::<benilla_sim::creature_ai::Point>()
                + (STRAIGHT_MAX_POINTS - 1) * std::mem::size_of::<SegmentChecks>();
            // Ledger-owned slices, plus one bounded synchronous emitted Facts envelope.
            // External diagnostic retention and allocator capacity are not measured here.
            terminal_count
                * (straight_storage_upper_bound_for_test() + 4 * cache_bound + route_geometry)
                + 6 * cache_bound
                + route_geometry
                + 3 * cache_bound
                + route_geometry
        } else {
            terminal_count
                * (10 * cache_bound
                    + 8050 * std::mem::size_of::<benilla_native_nav::SourceWarmTrace>()
                    + points_and_checks)
                + owners * (6 * cache_bound + points_and_checks)
        };
        if terminal_count > 2 * owners + staged_count || storage_bytes > upper_bound_bytes {
            return Err(Failure::Binding);
        }
        Ok(OwnershipDiagnostic {
            terminal_count,
            staged_count,
            storage_bytes,
            upper_bound_bytes,
        })
    }
    /// Cold refusal injections do not require a retained predecessor or raw pin.
    pub(in super::super) fn invalidate_prepared_cold_for_test(
        &mut self,
        mutation: u8,
    ) -> Box<dyn FnOnce(&mut Self)> {
        match mutation {
            15 => {
                let saved = self.boundary.take();
                Box::new(move |ledger| ledger.boundary = saved)
            }
            16 => {
                let saved = self.raw;
                let mut raw = saved.unwrap_or([0; 32]);
                raw[0] ^= 1;
                self.raw = Some(raw);
                Box::new(move |ledger| ledger.raw = saved)
            }
            17 => {
                let saved = self.route_epoch_committed;
                self.route_epoch_committed = true;
                Box::new(move |ledger| ledger.route_epoch_committed = saved)
            }
            18 => {
                let saved = self.next_id;
                self.next_id = u64::MAX;
                Box::new(move |ledger| ledger.next_id = saved)
            }
            19 => {
                let saved = self.retained_epoch_committed;
                self.retained_epoch_committed = true;
                Box::new(move |ledger| ledger.retained_epoch_committed = saved)
            }
            _ => panic!("unknown cold ledger refusal injection"),
        }
    }
    pub(in super::super) fn replace_cold_boundary_for_test(
        &mut self,
        boundary: RouteCommitBoundary,
    ) -> Box<dyn FnOnce(&mut Self)> {
        let saved = self.boundary.replace(boundary);
        Box::new(move |ledger| ledger.boundary = saved)
    }
    /// Failure injection only; restoration cannot manufacture a candidate or ACK.
    #[cfg(test)]
    pub(in super::super) fn invalidate_prepared_for_test(
        &mut self,
        mutation: u8,
    ) -> Box<dyn FnOnce(&mut Self)> {
        match mutation {
            15 => {
                let saved = self.boundary.take();
                Box::new(move |ledger| ledger.boundary = saved)
            }
            16 => {
                let saved = self.raw;
                self.raw.as_mut().unwrap()[0] ^= 1;
                Box::new(move |ledger| ledger.raw = saved)
            }
            17 => {
                let saved = self.candidates.remove(0);
                Box::new(move |ledger| ledger.candidates.insert(0, saved))
            }
            18 => {
                let saved = self.next_id;
                self.next_id = u64::MAX;
                Box::new(move |ledger| ledger.next_id = saved)
            }
            19 => {
                let saved = self.retained_epoch_committed;
                self.retained_epoch_committed = true;
                Box::new(move |ledger| ledger.retained_epoch_committed = saved)
            }
            20 => {
                let cold = Rc::clone(self.committed_cold().unwrap());
                let mut full = Vec::with_capacity(MAX_CANDIDATES);
                for id in 0..MAX_CANDIDATES {
                    full.push(Entry {
                        id: id as u64,
                        candidate: Candidate::Cold(Rc::clone(&cold)),
                        committed: id == 0,
                    });
                }
                let saved = std::mem::replace(&mut self.candidates, full);
                Box::new(move |ledger| ledger.candidates = saved)
            }
            _ => panic!("unknown ledger refusal injection"),
        }
    }
    pub(in super::super) fn diagnostic(&self) -> Diagnostic {
        Diagnostic {
            profile: self.profile.clone(),
            boundary: self.boundary.clone(),
            last_notice: self.last_notice.clone(),
            candidates: self
                .candidates
                .iter()
                .map(|entry| {
                    (
                        entry.id,
                        entry.candidate.identity().0,
                        entry.candidate.identity().1,
                        entry.committed,
                    )
                })
                .collect(),
            capacity: self.candidates.capacity(),
            candidate_raw_revisions: self
                .candidates
                .iter()
                .map(|entry| entry.candidate.raw_revision())
                .collect(),
            flat_candidates: self
                .candidates
                .iter()
                .map(|entry| match &entry.candidate {
                    Candidate::WarmHead(head) => (
                        1,
                        Rc::as_ptr(head) as usize,
                        Rc::as_ptr(&head.origin) as usize,
                        0,
                    ),
                    Candidate::WarmProposal(proposal) => (
                        2,
                        Rc::as_ptr(&proposal.head) as usize,
                        Rc::as_ptr(&proposal.head.origin) as usize,
                        match &proposal.predecessor {
                            WarmPredecessor::Cold(origin) => Rc::as_ptr(origin) as usize,
                            WarmPredecessor::Warm(prior) => Rc::as_ptr(prior) as usize,
                        },
                    ),
                    _ => (0, 0, 0, 0),
                })
                .collect(),
            raw: self.raw,
            next_id: self.next_id,
            route_epoch_committed: self.route_epoch_committed,
            one_retained: self.one_retained,
            two_owner_first_retained: self.two_owner_first_retained,
            retained_epoch_committed: self.retained_epoch_committed,
            two_retained: self.two_retained,
            finite_retained: self.finite_retained,
            finite_straight: self.finite_straight,
            epochs_committed: self.epochs_committed,
        }
    }
    pub(in super::super) fn counts(&self) -> (usize, usize) {
        (
            self.candidates.len(),
            self.candidates
                .iter()
                .filter(|entry| entry.committed)
                .count(),
        )
    }
}
fn calculation(reply: &AuthorityReply) -> Result<&motion::RouteCalculation, Failure> {
    let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &reply.facts else {
        return Err(Failure::Binding);
    };
    let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
        return Err(Failure::Binding);
    };
    if spatial.request != motion::Query::Route(route.request.clone()) {
        return Err(Failure::Binding);
    }
    let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
        return Err(Failure::Binding);
    };
    Ok(calculation)
}
fn same_reply_dispatch(profile: &Profile, a: &AuthorityRequest, b: &AuthorityRequest) -> bool {
    a == b
        && a.require_pathfinder_dispatch(profile)
            .ok()
            .zip(b.require_pathfinder_dispatch(profile).ok())
            .is_some_and(|(a, b)| a.same_live_dispatch(b))
}
fn same_notice(profile: &Profile, a: &RouteCommitNotice, b: &RouteCommitNotice) -> bool {
    a == b
        && a.routes()
            .iter()
            .zip(b.routes())
            .all(|(a, b)| same_reply_dispatch(profile, &a.request, &b.request))
}
fn push_unique<'a, T>(values: &mut Vec<&'a Rc<T>>, value: &'a Rc<T>) {
    if !values.iter().any(|old| Rc::ptr_eq(old, value)) {
        values.push(value);
    }
}
fn boundary_storage_bytes(boundary: &RouteCommitBoundary) -> usize {
    boundary
        .owner_heads()
        .filter_map(|(_, head)| head.cache())
        .map(|cache| std::mem::size_of_val(cache.poly_refs.as_slice()))
        .sum()
}
fn request_storage_bytes(request: &motion::RouteRequest) -> usize {
    request
        .prior_cache
        .as_ref()
        .map_or(0, |cache| std::mem::size_of_val(cache.poly_refs.as_slice()))
}
fn reply_storage_bytes(reply: &AuthorityReply) -> Result<usize, Failure> {
    let Query::SourceMotion(MotionQuery::Route(route)) = &reply.request.query else {
        return Err(Failure::Binding);
    };
    let calculation = calculation(reply)?;
    let mut bytes = 2 * request_storage_bytes(&route.request)
        + calculation
            .prior_cache
            .as_ref()
            .map_or(0, |cache| std::mem::size_of_val(cache.poly_refs.as_slice()))
        + std::mem::size_of_val(calculation.after_cache.poly_refs.as_slice());
    if let motion::RouteOutcome::Complete(complete) = &calculation.outcome {
        bytes += std::mem::size_of_val(complete.points.as_slice())
            + std::mem::size_of_val(complete.segments.as_slice());
    }
    Ok(bytes)
}
