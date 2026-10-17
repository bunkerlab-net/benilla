// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use interactions::WorkingNpc;

impl<A: Authority> Frame<'_, A> {
    fn combat_request(
        &self,
        npc: &WorkingNpc<'_>,
        phase: CombatPhase,
        action: Option<u64>,
    ) -> Result<CombatRequest, NativeError> {
        npc.request(
            self.profile_digest,
            self.state,
            self.interaction_moment(),
            phase,
            action,
        )
    }
    /// This private path records an internally derived reply. The external host
    /// may verify the ordered receipt, but cannot replace its current-state facts.
    fn record_combat(
        &mut self,
        id: NpcId,
        query: CombatQuery,
        facts: CombatFacts,
    ) -> Result<(), NativeError> {
        if self.replies.len() >= 20_000 {
            return Err(NativeError::TooLarge);
        }
        let reply = AuthorityReply {
            request: AuthorityRequest {
                binding: Binding {
                    world: self.state.world,
                    tick: self.state.tick,
                    profile: self.profile_digest,
                    authority_policy: self.profile.authority_policy,
                    geometry: self.profile.geometry_revision,
                    npc: Some(id),
                },
                query: Query::SourceMotion(MotionQuery::Combat(query)),
            },
            facts: Facts::SourceMotion(MotionFacts::Combat(facts)),
        };
        self.authority.observe_derived(&reply)?;
        self.replies.push(reply);
        Ok(())
    }
    pub(super) fn source_npc_facts(
        &mut self,
        npc: &Npc,
        phase: CombatPhase,
    ) -> Result<NpcFacts, NativeError> {
        self.source_npc_facts_with_visibility(npc, phase, None)
    }
    /// The selected notifier already evaluated directional AI visibility before
    /// MoveInLineOfSight's semantic prefix. Do not ask a second detector later.
    pub(super) fn source_npc_facts_with_visibility(
        &mut self,
        npc: &Npc,
        phase: CombatPhase,
        notifier_visibility: Option<bool>,
    ) -> Result<NpcFacts, NativeError> {
        let working = WorkingNpc::from_npc(npc)?;
        let request = self.combat_request(&working, phase, None)?;
        let facts = working.npc_semantics(self.profile, self.state)?;
        self.record_combat(
            npc.id,
            CombatQuery::NpcSemantics(request),
            CombatFacts::NpcSemantics(facts.clone()),
        )?;
        // Only MoveInLineOfSight consumes these spatial predicates. The other
        // callback phases cannot authorize sight with the unused false values.
        let (mut visible, mut accessible, mut line_of_sight) = (false, false, false);
        let policy = self
            .profile
            .npc(npc.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        if phase == CombatPhase::Sight
            && policy.ai.reaction == ai::Reaction::Aggressive
            && !policy.ai.neutral_to_all
            && npc.ai.phase() == ai::Phase::Idle
            && self.state.health > 0
            && npc.health > 0
            && facts.can_initiate_attack
            && facts.can_attack_on_sight
        {
            visible = match notifier_visibility {
                Some(visible) => visible,
                None => {
                    let query =
                        CombatQuery::NpcVisibility(self.combat_request(&working, phase, None)?);
                    let Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcVisibility(check))) =
                        self.ask(
                            Some(npc.id),
                            Query::SourceMotion(MotionQuery::Combat(query)),
                        )?
                    else {
                        return Err(NativeError::AuthorityBinding);
                    };
                    spatial(check)?
                }
            };
            let query = CombatQuery::NpcAccessibility(self.combat_request(&working, phase, None)?);
            let Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcAccessibility(check))) =
                self.ask(
                    Some(npc.id),
                    Query::SourceMotion(MotionQuery::Combat(query)),
                )?
            else {
                return Err(NativeError::AuthorityBinding);
            };
            accessible = spatial(check)?;
            let query = CombatQuery::NpcLineOfSight(self.combat_request(&working, phase, None)?);
            let Facts::SourceMotion(MotionFacts::Combat(CombatFacts::NpcLineOfSight(check))) = self
                .ask(
                    Some(npc.id),
                    Query::SourceMotion(MotionQuery::Combat(query)),
                )?
            else {
                return Err(NativeError::AuthorityBinding);
            };
            line_of_sight = spatial(check)?;
        }
        Ok(NpcFacts {
            can_attack_in_combat: facts.can_attack_in_combat,
            can_attack_on_sight: facts.can_attack_on_sight,
            can_initiate_attack: facts.can_initiate_attack,
            stationary_melee_refresh_eligible: facts.stationary_melee_refresh_eligible,
            creature_moving_and_running: facts.creature_moving_and_running,
            visible,
            accessible,
            line_of_sight,
            // Source MotionState owns chase routes; no provider-written goal.
            chase_goal: None,
        })
    }
    pub(super) fn source_melee_front(
        &mut self,
        npc: &WorkingNpc<'_>,
        action: u64,
    ) -> Result<bool, NativeError> {
        if npc.ai.phase() != ai::Phase::Combat
            || npc.ai.target() != Some(SOLO_UNIT)
            || npc.ai.victim_detached_at_ms().is_some()
            || npc.ai.position() != npc.motion.motion.pose().position
        {
            return Err(NativeError::InvalidState);
        }
        let query = CombatQuery::MeleeFront(self.combat_request(
            npc,
            CombatPhase::MainhandFront,
            Some(action),
        )?);
        let Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeFront(check))) = self.ask(
            Some(npc.id),
            Query::SourceMotion(MotionQuery::Combat(query)),
        )?
        else {
            return Err(NativeError::AuthorityBinding);
        };
        spatial(check)
    }
    pub(super) fn source_melee_entry(
        &mut self,
        npc: &WorkingNpc<'_>,
        action: u64,
    ) -> Result<MeleeEntryFacts, NativeError> {
        let request = self.combat_request(npc, CombatPhase::AfterMeleeDamage, Some(action))?;
        let facts = npc.combat_entry(self.profile)?;
        self.record_combat(
            npc.id,
            CombatQuery::MeleeCombatEntry(request),
            CombatFacts::MeleeCombatEntry(facts.clone()),
        )?;
        Ok(facts)
    }
    pub(super) fn source_melee_combat(
        &mut self,
        npc: &WorkingNpc<'_>,
        action: u64,
    ) -> Result<MeleeCombatFacts, NativeError> {
        if self.state.health == 0 {
            return Err(NativeError::InvalidState);
        }
        let request = self.combat_request(npc, CombatPhase::SurvivingMeleeDamage, Some(action))?;
        let facts = npc.melee_combat(self.profile, self.state)?;
        self.record_combat(
            npc.id,
            CombatQuery::MeleeDamage(request),
            CombatFacts::MeleeDamage(facts.clone()),
        )?;
        Ok(facts)
    }
}
fn spatial(check: ai::movement::Check) -> Result<bool, NativeError> {
    match check {
        ai::movement::Check::Passed => Ok(true),
        ai::movement::Check::Failed => Ok(false),
        ai::movement::Check::Unknown => Err(NativeError::AuthorityUnavailable(
            "source combat spatial predicate",
        )),
    }
}
