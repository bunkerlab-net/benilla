// SPDX-License-Identifier: GPL-3.0-or-later
// Source EnterCombat callback runs after base combat position/notify setup, before Motion.
use super::*;
use benilla_sim::{creature_motion as m, event_ai as source};

impl<A: Authority> Frame<'_, A> {
    pub(super) fn event_ai_enter_combat(
        &mut self,
        npc: &mut Npc,
        target: u64,
        kind: Option<&EventAiCallbackKind>,
    ) -> Result<(), NativeError> {
        let Some(catalog) = self.profile.event_ai() else {
            return Ok(());
        };
        let events = catalog.catalog().events(npc.id.spawn)?;
        if events.is_empty() {
            return Ok(());
        }
        if self
            .profile
            .source_visitors
            .as_ref()
            .and_then(|v| v.owner(npc.id.spawn))
            .is_none_or(|o| o.ai_name != "EventAI")
        {
            return Err(NativeError::InvalidState);
        }
        if self.read_only
            || target != SOLO_UNIT
            || npc.ai.target() != Some(SOLO_UNIT)
            || npc.ai.phase() != ai::Phase::Combat
            || npc.health == 0
            || self.state.health == 0
            || npc.source_motion.is_none()
            || self.profile.source_visitors.is_none()
        {
            return Err(NativeError::InvalidState);
        }
        let kind = kind
            .ok_or(NativeError::UnsupportedMechanic(
                "late EventAI combat callback",
            ))?
            .clone();
        if let EventAiCallbackKind::CombatReceipt { receipt } = &kind {
            if receipt.id != npc.ai.last_receipt_id() {
                return Err(NativeError::AuthorityBinding);
            }
        }
        // The callback owns the detached NPC. Bind that exact post-enter pose and
        // queue state, rather than the prior frame or a stale visibility cache.
        let mut scene = self.state.clone();
        if scene.npcs.insert(npc.id.spawn, npc.clone()).is_some() {
            return Err(NativeError::AuthorityBinding);
        }
        let witness =
            interactions::witness(self.profile_digest, &scene, self.interaction_moment())?;
        let callback = EventAiCallback {
            world: self.state.world,
            tick: self.state.tick,
            now_ms: self.state.now_ms,
            moment: witness.moment,
            catalog: catalog.digest(),
            npc: npc.id,
            invoker: self.state.player,
            kind,
            speaker_pose: npc.source_motion.as_ref().unwrap().motion.pose(),
            player_pose: m::Pose {
                position: self.state.position,
                orientation: ai::Real::from_bits(self.state.orientation.bits())?,
            },
            draft: witness.draft,
        };
        let mut continuation = self
            .state
            .event_ai
            .take()
            .ok_or(NativeError::InvalidState)?;
        continuation.reserve_history()?;
        let owner = continuation
            .owners
            .get_mut(&npc.id.spawn)
            .ok_or(NativeError::InvalidState)?;
        if owner.id != npc.id {
            return Err(NativeError::AuthorityBinding);
        }
        if events.len() != owner.holders.len() {
            return Err(NativeError::InvalidState);
        }
        let occurrence = owner.next_occurrence;
        let mut evaluation = EventAiEvaluation {
            callback: callback.clone(),
            occurrence,
            actions: Vec::new(),
        };
        for (event, snapshot) in events.iter().zip(&mut owner.holders) {
            let mut holder = source::AggroTextHolder::restore(*event, *snapshot)
                .map_err(NativeError::EventAi)?;
            if holder.begin_enter_combat() != Some(source::DrawRequest::Chance) {
                return Err(NativeError::InvalidState);
            }
            let chance_counter = self.state.rng_counter;
            let chance = self.state.draw_range(
                0,
                DrawPurpose::EventAiChance {
                    npc: npc.id,
                    event_id: event.event_id(),
                    occurrence,
                },
                0,
                i64::from(u32::MAX),
                &mut self.draws,
            )? as u32;
            let mut receipt = EventAiActionReceipt {
                event_id: event.event_id(),
                chance_counter,
                action_counter: None,
                broadcast_id: None,
                text_slot: None,
                audience: None,
                speech: None,
            };
            if holder
                .consume_chance(chance)
                .map_err(NativeError::EventAi)?
                == source::ChanceResult::ActionRequired
            {
                receipt.action_counter = Some(self.state.rng_counter);
                let action = self.state.draw_range(
                    0,
                    DrawPurpose::EventAiAction {
                        npc: npc.id,
                        event_id: event.event_id(),
                        occurrence,
                    },
                    0,
                    i64::from(u32::MAX),
                    &mut self.draws,
                )? as u32;
                let selection = holder
                    .consume_action(action)
                    .map_err(NativeError::EventAi)?;
                receipt.broadcast_id = Some(selection.text_id);
                receipt.text_slot = Some(selection.text_slot);
                let projected = npc.effective_profile(self.profile)?;
                let request =
                    EventAiAudienceRequest::from_callback(self.profile, &callback, &projected)?;
                let Facts::EventAiAudience(facts) = self.ask(
                    Some(npc.id),
                    Query::EventAiAudience(Box::new(request.clone())),
                )?
                else {
                    return Err(NativeError::AuthorityBinding);
                };
                if request.validate(&facts)? {
                    let speech = EventAiState::speech(
                        catalog,
                        &callback,
                        occurrence,
                        selection,
                        npc.initialized_creature()
                            .map(|selected| selected.model.gender),
                    )?;
                    self.events
                        .push(Event::EventAiSpeech(Box::new(speech.clone())));
                    receipt.speech = Some(speech);
                }
                receipt.audience = Some(EventAiAudienceReceipt { request, facts });
            }
            *snapshot = holder.snapshot().map_err(NativeError::EventAi)?;
            evaluation.actions.push(receipt);
        }
        owner.next_occurrence = owner
            .next_occurrence
            .checked_add(1)
            .ok_or(NativeError::Limit)?;
        npc.events
            .event_ai_anchor
            .as_mut()
            .ok_or(NativeError::InvalidState)?
            .append(&evaluation)?;
        continuation
            .history
            .push(crate::native::event_ai::EventAiHistory::EnterCombat(
                Box::new(evaluation.clone()),
            ));
        self.state.event_ai = Some(continuation);
        self.events
            .push(Event::EventAiEvaluation(Box::new(evaluation)));
        Ok(())
    }
}
