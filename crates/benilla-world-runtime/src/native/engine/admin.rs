// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
impl<A: Authority> Frame<'_, A> {
    pub(super) fn admin_command(
        &mut self,
        request: &AdminRequest,
    ) -> Result<Option<Rejection>, NativeError> {
        // OwnedInput authenticated_owner has already been checked against this selected owner
        // before the frame was staged. Never use a serialized request as an authenticated actor.
        if let Err(reason) = crate::native::admin::admin_request_gate(
            self.profile,
            self.state,
            self.state.player.owner,
            request,
        ) {
            self.events.push(Event::AdminRejected {
                request: request.clone(),
                reason: reason.clone(),
            });
            return Ok(Some(Rejection::Admin(reason)));
        }
        if let Some(receipt) = self.state.admin.receipt(&request.key) {
            self.events.push(Event::AdminCommitted(receipt.clone()));
            return Ok(None);
        }
        self.sync_quest_progression()?;
        let g = self
            .profile
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let q = self
            .state
            .quest_progression
            .as_ref()
            .ok_or(NativeError::InvalidState)?;
        let mut next = q.clone();
        let effect = match request.operation {
            AdminOperation::GainOneLevel => {
                let before = q.player.view().level;
                let max = g.admin_supported_maximum();
                let target = before.saturating_add(1).min(max);
                let (updated, events) = q.admin_give_level(
                    g,
                    self.state
                        .combat
                        .as_ref()
                        .ok_or(NativeError::InvalidState)?,
                    target,
                )?;
                next = updated;
                self.events
                    .extend(events.into_iter().map(Event::QuestProgression));
                AdminEffect::Level {
                    before,
                    after: target,
                    xp: next.player.view().xp,
                    supported_maximum: max,
                }
            }
            AdminOperation::SetXpGain { enabled } => {
                let before = next.xp_enabled;
                next.xp_enabled = enabled;
                AdminEffect::XpGain {
                    before,
                    enabled,
                    xp: next.player.view().xp,
                    level: next.player.view().level,
                }
            }
        };
        self.commit_quest_progression(next)?;
        let receipt = AdminReceipt {
            request: request.clone(),
            tick: self.state.tick,
            sequence: self.state.next_sequence,
            effect,
        };
        self.state.admin.insert(receipt.clone());
        self.events.push(Event::AdminCommitted(receipt));
        Ok(None)
    }
}
