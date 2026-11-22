// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use environment::{
    EnvironmentCallback, EnvironmentError, EnvironmentPhase, EnvironmentPose,
    NativeEnvironmentEffects,
};
impl<A: Authority> Frame<'_, A> {
    // Player::SetPosition:6069–6075 runs after relocation, even when unchanged.
    pub(super) fn player_environment_position(&mut self) -> Result<(), NativeError> {
        let Some(cache) = self.state.source_player_environment.as_ref() else {
            return if self.profile.player_environment.is_none() {
                Ok(())
            } else {
                Err(NativeError::InvalidState)
            };
        };
        let pose = benilla_sim::creature_motion::Pose {
            position: self.state.position,
            orientation: ai::Real::from_bits(self.state.orientation.bits())?,
        };
        let query = cache.query_for_position(self.profile, pose)?;
        let result = if let Some(query) = &query {
            query.validate_for(self.profile, self.state.world, self.state.player)?;
            let Facts::PlayerEnvironment(facts) =
                self.ask(None, Query::PlayerEnvironment(Box::new(query.clone())))?
            else {
                return Err(NativeError::AuthorityBinding);
            };
            facts.validate_for(query)?;
            Some(facts.result())
        } else {
            None
        };
        let cache = self
            .state
            .source_player_environment
            .as_mut()
            .ok_or(NativeError::InvalidState)?;
        let height = self
            .state
            .source_player_collision_height
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .witness()
            .height()
            .bits();
        let phase = EnvironmentPhase::Position {
            pose: EnvironmentPose {
                position: [
                    pose.position.x.bits(),
                    pose.position.y.bits(),
                    pose.position.z.bits(),
                ],
                collision_height: height,
            },
            result: result.map(Box::new),
        };
        let kind = PlayerEnvironmentPhase::Position {
            input_sequence: self.state.next_sequence,
            pose,
            refreshed: query.is_some(),
        };
        let receipt = if self
            .profile
            .player_environment
            .as_ref()
            .is_some_and(PlayerEnvironmentSource::surface_callbacks_selected)
        {
            PlayerEnvironmentSource::validate_surface_sources(self.profile)?;
            let mut effects = SurfaceEffects {
                profile: self.profile,
                npcs: &mut self.state.npcs,
                slow: self
                    .state
                    .player_slow
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?,
                events: &mut self.events,
                failure: None,
            };
            let result =
                cache.apply_surface(self.profile, self.state.tick, phase, kind, &mut effects);
            if let Some(error) = effects.failure {
                return Err(error);
            }
            result?
        } else {
            cache.apply(self.profile, self.state.tick, phase, kind)?
        };
        self.events.push(Event::PlayerEnvironment(receipt));
        Ok(())
    }
    // Player::Update:1483–1489 decrements once, after player reactives and before NPCs.
    pub(super) fn player_environment_update(&mut self) -> Result<(), NativeError> {
        let Some(cache) = self.state.source_player_environment.as_mut() else {
            return if self.profile.player_environment.is_none() {
                Ok(())
            } else {
                Err(NativeError::InvalidState)
            };
        };
        let diff_ms = self.profile.tick_ms;
        let receipt = cache.apply(
            self.profile,
            self.state.tick,
            EnvironmentPhase::Update { diff_ms },
            PlayerEnvironmentPhase::Update { diff_ms },
        )?;
        self.events.push(Event::PlayerEnvironment(receipt));
        Ok(())
    }
}

/// Borrows only this Frame's staged owned state. The outer frame discards all
/// NPC/holder/speed/slot/event changes if this or any later operation fails.
struct SurfaceEffects<'a> {
    profile: &'a Profile,
    npcs: &'a mut BTreeMap<u64, Npc>,
    slow: &'a mut PlayerSlowState,
    events: &'a mut Vec<Event>,
    failure: Option<NativeError>,
}
impl SurfaceEffects<'_> {
    fn apply(&mut self, intent: EnvironmentCallback) -> Result<(), NativeError> {
        match intent {
            EnvironmentCallback::UpdateThreatTables => {
                // Real single-target references are owned by the NPC AI, not inferred
                // from its selected target. This closed callback has no scripts,
                // target mutation, selection or shared threat: independent reference
                // updates commute. We do not claim addHatedBy insertion-order parity
                // for future observable callbacks or emit an invented callback trace.
                let offline_targets = self.profile.source_player_offline_targets()?;
                for npc in self.npcs.values_mut() {
                    if npc.ai.hostile_reference().is_some() {
                        let offline = *offline_targets
                            .get(&npc.id.spawn)
                            .ok_or(NativeError::InvalidState)?;
                        npc.ai
                            .update_online_status(SOLO_UNIT, Some(SOLO_UNIT), offline)?;
                    }
                }
                Ok(())
            }
            EnvironmentCallback::RemoveInterruptAuras { mask } => {
                self.slow
                    .remove_interrupt_auras(mask, self.profile, self.npcs, self.events)
            }
            EnvironmentCallback::RemoveSpellAuras { spell_id } => {
                // The first selected surface domain still rejects spell-bearing liquid;
                // this operation itself nevertheless uses real source-default removal.
                self.slow
                    .remove_spell_auras(spell_id, self.profile, self.npcs, self.events)
            }
            _ => Err(NativeError::PlayerEnvironment(
                EnvironmentError::CallbacksUnavailable(intent),
            )),
        }
    }
}
impl NativeEnvironmentEffects for SurfaceEffects<'_> {
    fn effect(&mut self, intent: EnvironmentCallback, _flags: u8) -> Result<(), EnvironmentError> {
        if self.failure.is_some() {
            return Err(EnvironmentError::CallbackFailed(intent));
        }
        self.apply(intent).map_err(|error| {
            self.failure = Some(error);
            EnvironmentError::CallbackFailed(intent)
        })
    }
}
