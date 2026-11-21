// SPDX-License-Identifier: GPL-3.0-or-later
//! Separate two-owner, first-retained ledger capability. Neither singleton nor
//! sustained/multi-owner gameplay authority follows from this component policy.
use super::*;

impl ResidentLedger {
    pub(super) fn committed_pair_cold(
        &self,
        npc: NpcId,
    ) -> Result<&Rc<ResidentColdCandidate>, Failure> {
        if !self.two_owner_first_retained
            || self.profile.npcs.len() != 2
            || !self.route_epoch_committed
            || self.retained_epoch_committed
        {
            return Err(Failure::Binding);
        }
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        let mut committed = self.candidates.iter().filter(|entry| entry.committed);
        if committed.clone().count() != 2 {
            return Err(Failure::Binding);
        }
        let entry = committed
            .find(|entry| entry.candidate.reply().request.binding.npc == Some(npc))
            .ok_or(Failure::Binding)?;
        let Candidate::Cold(candidate) = &entry.candidate else {
            return Err(Failure::Binding);
        };
        let head = boundary
            .heads()
            .iter()
            .find(|head| head.npc() == npc)
            .ok_or(Failure::Binding)?;
        let dispatch = candidate
            .reply
            .request
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if boundary.heads().len() != 2
            || dispatch.npc() != npc
            || head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
            || head.construction() != Some(dispatch.construction_binding())
            || head.cache() != Some(&calculation(&candidate.reply)?.after_cache)
        {
            return Err(Failure::Binding);
        }
        self.validate_candidate(candidate)?;
        Ok(candidate)
    }

    pub(super) fn validate_pair_warm_head(&self, head: &WarmHead) -> Result<(), Failure> {
        if !self.two_owner_first_retained
            || self.profile.npcs.len() != 2
            || !matches!(&head.dependency, WarmDependency::First(_))
        {
            return Err(Failure::Binding);
        }
        self.validate_retained_context(&head.origin, &head.reply.request)?;
        let before = &calculation(&head.origin.reply)?.after_cache;
        let after = &calculation(&head.reply)?.after_cache;
        if before.revision != 1
            || after.revision != 2
            || after.include_flags != before.include_flags
            || after.exclude_flags != before.exclude_flags
        {
            return Err(Failure::Binding);
        }
        self.validate_warm_head_payload(head)
    }

    pub(super) fn validate_pair_proposal(
        &self,
        proposal: &StagedWarmProposal,
    ) -> Result<(), Failure> {
        self.validate_pair_warm_head(&proposal.head)?;
        let WarmPredecessor::Cold(origin) = &proposal.predecessor else {
            return Err(Failure::Binding);
        };
        if !Rc::ptr_eq(origin, &proposal.head.origin) {
            return Err(Failure::Binding);
        }
        Ok(())
    }

    pub(super) fn validate_pair_notice_owners(
        &self,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        if self.profile.npcs.len() != 2 {
            return Err(Failure::Binding);
        }
        if notice.routes().is_empty() {
            return Ok(());
        }
        let previous = self.boundary.as_ref().ok_or(Failure::Binding)?;
        if notice.routes().len() != 2
            || previous.heads().len() != 2
            || previous.heads().iter().any(|head| {
                notice
                    .routes()
                    .iter()
                    .filter(|reply| reply.request.binding.npc == Some(head.npc()))
                    .count()
                    != 1
            })
        {
            return Err(Failure::Binding);
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub(in super::super::super) fn certify_two_owner_retained(
        &mut self,
        world: &mut World,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.certify_two_owner_retained_observed(
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
    pub(in super::super::super) fn certify_two_owner_retained_observed(
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
        provider.require_two_owner_first_retained()?;
        if !self.two_owner_first_retained || self.retained_epoch_committed {
            return Err(Failure::UnsupportedQuery.into());
        }
        if !self.provider.matches(provider)
            || !self.owner.same_owner(compatibility.owner())
            || self.boundary.as_ref().and_then(|b| b.tick().checked_add(1))
                != Some(context.binding.tick)
        {
            return Err(Failure::Binding.into());
        }
        let npc = context.binding.npc.ok_or(Failure::Binding)?;
        let predecessor = Rc::clone(self.committed_pair_cold(npc)?);
        self.validate_retained_context(&predecessor, context)?;
        if self.candidates.iter().any(|entry| {
            entry.candidate.reply().request.binding.npc == Some(npc)
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
        if self.candidates.len() >= MAX_CANDIDATES || self.candidates.capacity() < MAX_CANDIDATES {
            return Err(Failure::QueueCapacity.into());
        }
        let next = self.next_id.checked_add(1).ok_or(Failure::QueueCapacity)?;
        // Capacity was reserved at bind; failed source work cannot alter diagnostics.
        let (candidate, facts) = super::super::retained::certify_two_owner(
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
        if !Rc::ptr_eq(self.committed_pair_cold(npc)?, &candidate.predecessor)
            || self.raw != Some(candidate.raw_revision)
        {
            return Err(Failure::Binding.into());
        }
        let proposal = StagedWarmProposal::first(candidate);
        self.validate_pair_proposal(&proposal)?;
        self.candidates.push(Entry {
            id: self.next_id,
            candidate: Candidate::WarmProposal(Rc::new(proposal)),
            committed: false,
        });
        self.next_id = next;
        Ok(facts)
    }

    /// Check actual owned ancestry without performing source work or emitting facts.
    pub(in super::super::super) fn check_pair_isolation_for_test(
        &self,
        context: &AuthorityRequest,
    ) -> Result<(), Failure> {
        let npc = context.binding.npc.ok_or(Failure::Binding)?;
        let own = self.committed_pair_cold(npc)?;
        self.validate_retained_context(own, context)?;
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        let other_npc = boundary
            .heads()
            .iter()
            .find(|head| head.npc() != npc)
            .ok_or(Failure::Binding)?
            .npc();
        let other = self.committed_pair_cold(other_npc)?;
        if Rc::ptr_eq(own, other) || self.validate_retained_context(other, context).is_ok() {
            return Err(Failure::Binding);
        }
        Ok(())
    }

    pub(in super::super::super) fn pair_ownership_for_test(
        &self,
    ) -> Result<OwnershipDiagnostic, Failure> {
        if !self.two_owner_first_retained {
            return Err(Failure::UnsupportedQuery);
        }
        self.flat_ownership_inner(2)
    }

    pub(in super::super::super) fn check_committed_pair_for_test(&self) -> Result<(), Failure> {
        if !self.two_owner_first_retained
            || !self.retained_epoch_committed
            || self.counts() != (2, 2)
        {
            return Err(Failure::Binding);
        }
        let boundary = self.boundary.as_ref().ok_or(Failure::Binding)?;
        for entry in &self.candidates {
            let Candidate::WarmHead(warm) = &entry.candidate else {
                return Err(Failure::Binding);
            };
            self.validate_pair_warm_head(warm)?;
            let npc = warm.reply.request.binding.npc.ok_or(Failure::Binding)?;
            let head = boundary
                .heads()
                .iter()
                .find(|head| head.npc() == npc)
                .ok_or(Failure::Binding)?;
            let dispatch = warm
                .reply
                .request
                .require_pathfinder_dispatch(&self.profile)
                .map_err(|_| Failure::Binding)?;
            if head.cache() != Some(&calculation(&warm.reply)?.after_cache)
                || head.construction() != Some(dispatch.construction_binding())
                || head.constructor_kind() != Some(motion::PathfinderConstructorKind::ChaseLazy)
            {
                return Err(Failure::Binding);
            }
        }
        Ok(())
    }
}
