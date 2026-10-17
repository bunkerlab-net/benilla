// SPDX-License-Identifier: GPL-3.0-or-later
// Unit::SetInCombatState's assistance suffix follows the synchronous AI callback.
use super::*;
use benilla_sim::creature_motion::Pose;
impl<A: Authority> Frame<'_, A> {
    pub(super) fn combat_entry_assistance(
        &mut self,
        npc: &mut Npc,
        target: u64,
        callback: Option<&EventAiCallbackKind>,
        mut transaction: Option<&mut benilla_sim::event_processor::EventTransaction<'_>>,
    ) -> Result<(), NativeError> {
        let Some(catalog) = self.profile.combat_assistance() else {
            return Ok(());
        };
        let callback = callback
            .ok_or(NativeError::UnsupportedMechanic(
                "assistance late combat entry",
            ))?
            .clone();
        if self.read_only
            || target != SOLO_UNIT
            || npc.ai.phase() != ai::Phase::Combat
            || npc.ai.target() != Some(target)
            || matches!(&callback, EventAiCallbackKind::CombatReceipt { receipt } if receipt.id != npc.ai.last_receipt_id())
            || npc.events.callback.is_some() != transaction.is_some()
            || callback.owns_active_queue(npc.id) != transaction.is_some()
        {
            return Err(NativeError::AuthorityBinding);
        }
        match &callback {
            EventAiCallbackKind::CombatReceipt { receipt }
                if receipt.id == npc.ai.last_receipt_id() => {}
            EventAiCallbackKind::Sight {
                notification_owner, ..
            } if *notification_owner == VisitorParticipant::Player(self.state.player)
                || *notification_owner == VisitorParticipant::Creature(npc.id) => {}
            EventAiCallbackKind::PeerHelp { request, receipt }
                if request.observer == VisitorParticipant::Creature(npc.id) =>
            {
                receipt.validate_for(self.profile, self.state.player, request)?;
            }
            _ => return Err(NativeError::AuthorityBinding),
        }
        let mut cause_scene = self.state.clone();
        if cause_scene.npcs.insert(npc.id.spawn, npc.clone()).is_some() {
            return Err(NativeError::AuthorityBinding);
        }
        if let Some((owner, sight)) = callback.source_sight() {
            match owner {
                VisitorParticipant::Player(player)
                    if *player == self.state.player
                        && self.state.player_events.notify == Some(sight.due.event_id) =>
                {
                    // The player dispatcher lends its queue too; this immutable
                    // validation overlay uses its exact passed callback snapshot.
                    cause_scene.player_events.queue =
                        benilla_sim::event_processor::EventProcessor::restore(sight.queue.clone())?;
                }
                VisitorParticipant::Creature(owner)
                    if cause_scene.npcs.get(&owner.spawn).is_some_and(|n| {
                        n.id == *owner && n.events.callback.as_ref() == Some(sight)
                    }) => {}
                _ => return Err(NativeError::AuthorityBinding),
            }
        }
        callback.validate_for(self.profile, &cause_scene, npc.id, self.state.tick)?;
        let source = catalog.source(npc.id.spawn)?;
        if npc
            .source_generation
            .as_ref()
            .is_none_or(|g| g.source_identity() != source.generation_source)
        {
            return Err(NativeError::AuthorityBinding);
        }
        let source_digest = catalog.digest();
        let extra_flags = self.profile.assistance_extra_flags(npc.id.spawn)?;
        let token = match transaction.as_deref_mut() {
            Some(queue) => npc.events.prepare_assistance_with_transaction(
                catalog.delay_ms(),
                extra_flags,
                Some(queue),
            )?,
            None => npc
                .events
                .prepare_assistance(catalog.delay_ms(), extra_flags)?,
        };
        let event_ai_callback = self
            .state
            .event_ai
            .as_ref()
            .and_then(|state| state.history.last())
            .and_then(|h| {
                let crate::native::event_ai::EventAiHistory::EnterCombat(e) = h else {
                    return None;
                };
                (e.callback.npc == npc.id
                    && e.callback.tick == self.state.tick
                    && e.callback.kind == callback)
                    .then(|| e.callback.digest())
            });
        let mut scene = self.state.clone();
        if scene.npcs.insert(npc.id.spawn, npc.clone()).is_some() {
            return Err(NativeError::AuthorityBinding);
        }
        let witness =
            interactions::witness(self.profile_digest, &scene, self.interaction_moment())?;
        let projected = npc.effective_profile(self.profile)?;
        let request = CombatAssistanceRequest {
            binding: Binding {
                world: self.state.world,
                tick: self.state.tick,
                profile: self.profile_digest,
                authority_policy: self.profile.authority_policy,
                geometry: self.profile.geometry_revision,
                npc: Some(npc.id),
            },
            content: self.profile.content_digest,
            source: source_digest,
            generation_source: source.generation_source,
            callback,
            moment: witness.moment,
            now_ms: self.state.now_ms,
            occurrence: token.occurrence,
            unit_clock_ms: npc.events.clock_ms(),
            owner_pose: npc
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .motion
                .pose(),
            enemy: self.state.player,
            enemy_pose: Pose {
                position: self.state.position,
                orientation: ai::Real::from_bits(self.state.orientation.bits())?,
            },
            radius: source.call_for_help,
            owner_bounding_radius: projected.ai.bounding_radius,
            scene: witness.draft,
            scene_inventory: scene.npcs.values().map(|npc| npc.id).collect(),
            rng_counter: self.state.rng_counter,
            event_ai_callback,
        };
        let (receivers, traversal) = if extra_flags
            & crate::native::combat_assistance::NO_CALL_ASSIST
            == 0
        {
            let Facts::CombatAssistance(facts) = self.ask(
                Some(npc.id),
                Query::CombatAssistance(Box::new(request.clone())),
            )?
            else {
                return Err(NativeError::AuthorityBinding);
            };
            request.validate(&facts)?;
            let traversal =
                interactions::source_assistance_traversal(self.profile, &scene, &request, &facts)?;
            (Some(facts), Some(traversal))
        } else {
            (None, None)
        };
        match transaction {
            Some(queue) => npc.events.finish_assistance_with_transaction(
                request,
                receivers,
                traversal,
                token,
                extra_flags,
                Some(queue),
            ),
            None => npc
                .events
                .finish_assistance(request, receivers, traversal, token, extra_flags),
        }
    }
}
