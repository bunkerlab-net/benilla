// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS SpellStart, SpellEvent and Unit::_UpdateSpells, pinned 8ec338a1.
use super::*;
use benilla_sim::event_processor::{CallbackKind, EventKind};
use benilla_sim::spell_event::{
    plan_after_delayed, plan_after_update, SpellEventPlan, SpellEventState,
};

impl<A: Authority> Frame<'_, A> {
    /// Source InterruptSpell cancels now and clears the current slot; it does not
    /// remove the owning event. The due callback later destroys the finished spell.
    pub(super) fn interrupt_scheduled_cast(
        &mut self,
        id: u64,
        failure: spell::CastFailure,
    ) -> Result<(), NativeError> {
        let mut cast = self
            .state
            .casts
            .remove(&id)
            .ok_or(NativeError::InvalidState)?;
        let events = cast
            .execution
            .cancel(&mut self.state.resources.caster, failure);
        self.spell_events(id, cast.ability, cast.target, events)?;
        self.state
            .player_events
            .finish(id, self.state.now_ms, FinishedSpellReason::Interrupted)?;
        if self.state.player_events.current_spell == Some(id) {
            self.state.player_events.current_spell = None;
        }
        Ok(())
    }
    pub(super) fn player_event_update(&mut self) -> Result<(), NativeError> {
        // The enclosing Frame already stages world, RNG, outputs and lifetimes.
        // Holding the queue separately allows callbacks to mutate that staged world.
        let mut queue = std::mem::take(&mut self.state.player_events.queue);
        let mut transaction = queue.begin_update(self.profile.tick_ms)?;
        while let Some(due) = transaction.pop_due()? {
            match due.kind {
                EventKind::ReenableHelp { .. } | EventKind::AggroLinking { .. } => {
                    return Err(NativeError::InvalidState)
                }
                EventKind::AiNotify => {
                    if self.state.player_events.notify != Some(due.event_id) {
                        return Err(NativeError::InvalidState);
                    }
                    if due.callback == CallbackKind::Execute {
                        self.source_player_notify_callback(&transaction.callback_snapshot()?)?;
                        transaction.finish_execute(true)?;
                    } else {
                        transaction.finish_abort(true)?;
                    }
                    // FinalizeAINotifyEvent happens after the source visitor.
                    self.state.player_events.notify = None;
                }
                EventKind::SpellUpdate { cast_id } => {
                    let lifetime = self
                        .state
                        .player_events
                        .spells
                        .get(&cast_id)
                        .ok_or(NativeError::InvalidState)?;
                    if lifetime.event_id != due.event_id {
                        return Err(NativeError::InvalidState);
                    }
                    if due.callback == CallbackKind::Abort {
                        // Source due-Abort deletes regardless of IsDeletable. A live
                        // current reference would leak its Spell; that dependency is
                        // outside this bounded host, rather than silently cleared.
                        if self.state.player_events.current_spell == Some(cast_id) {
                            return Err(NativeError::UnsupportedMechanic(
                                "aborted nondeletable current SpellEvent",
                            ));
                        }
                        if self.state.casts.contains_key(&cast_id) {
                            self.interrupt_scheduled_cast(
                                cast_id,
                                spell::CastFailure::Interrupted,
                            )?;
                        }
                        transaction.finish_abort(
                            self.state.player_events.current_spell != Some(cast_id),
                        )?;
                        self.state.player_events.spells.remove(&cast_id);
                        continue;
                    }
                    let mut handled_delayed = false;
                    let mut after_update = spell::CastState::Finished;
                    if lifetime.finished.is_none() {
                        let delay_started = lifetime.delay_start_ms.is_some();
                        (after_update, handled_delayed) = self.casts_one(
                            cast_id,
                            due.diff_ms.ok_or(NativeError::InvalidState)?,
                            delay_started,
                            &transaction.callback_snapshot()?,
                        )?;
                        let lifetime = self.state.player_events.spells.get_mut(&cast_id).unwrap();
                        lifetime.last_callback_ms = Some(self.state.now_ms);
                        lifetime.last_dispatch = Some(benilla_sim::event_processor::QueueEntry {
                            deadline_ms: due.scheduled_ms,
                            insertion_id: due.insertion_id,
                        });
                        if !self.state.casts.contains_key(&cast_id) {
                            lifetime.finished = Some((
                                self.state.now_ms,
                                if handled_delayed {
                                    FinishedSpellReason::DelayedFinished
                                } else {
                                    FinishedSpellReason::UpdateFinished
                                },
                            ));
                        }
                    }
                    let lifetime = self.state.player_events.spells.get_mut(&cast_id).unwrap();
                    let source_state = match after_update {
                        spell::CastState::Casting { .. } => SpellEventState::Casting,
                        spell::CastState::Traveling { impact_at_ms } => {
                            SpellEventState::Traveling {
                                delay_start_ms: lifetime.delay_start_ms,
                                // Only consulted by the source on first travel callback.
                                delay_moment_ms: if lifetime.delay_start_ms.is_none() {
                                    impact_at_ms
                                        .checked_sub(self.state.now_ms)
                                        .ok_or(NativeError::InvalidState)?
                                } else {
                                    0
                                },
                            }
                        }
                        spell::CastState::Finished | spell::CastState::Failed(_) => {
                            SpellEventState::Finished {
                                deletable: self.state.player_events.current_spell != Some(cast_id),
                            }
                        }
                    };
                    let mut plan = plan_after_update(due.now_ms, source_state)?;
                    if let SpellEventPlan::HandleDelayed { .. } = plan {
                        if !handled_delayed {
                            return Err(NativeError::InvalidState);
                        }
                        plan = plan_after_delayed(
                            due.now_ms,
                            lifetime.delay_start_ms.ok_or(NativeError::InvalidState)?,
                            0,
                        )?;
                    } else if handled_delayed {
                        return Err(NativeError::InvalidState);
                    }
                    match plan {
                        SpellEventPlan::Delete => {
                            transaction.finish_execute(true)?;
                            self.state.player_events.spells.remove(&cast_id);
                        }
                        SpellEventPlan::ReAdd {
                            deadline_ms,
                            set_delay_start_ms,
                        } => {
                            if let Some(start) = set_delay_start_ms {
                                lifetime.delay_start_ms = Some(start);
                                lifetime.travel_deadline_ms = Some(deadline_ms);
                            }
                            transaction.re_add(due.event_id, deadline_ms, false)?;
                            transaction.finish_execute(false)?;
                        }
                        SpellEventPlan::HandleDelayed { .. } => {
                            return Err(NativeError::InvalidState)
                        }
                    }
                }
            }
        }
        transaction.commit()?;
        self.state.player_events.queue = queue;
        // Unit::_UpdateSpells follows m_events.Update, never before it.
        if self
            .state
            .player_events
            .current_spell
            .is_some_and(|id| !self.state.casts.contains_key(&id))
        {
            self.state.player_events.current_spell = None;
        }
        Ok(())
    }
}
