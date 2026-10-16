// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use environment::{EnvironmentPhase, EnvironmentPose};
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
        let receipt = cache.apply(
            self.profile,
            self.state.tick,
            EnvironmentPhase::Position {
                pose: EnvironmentPose {
                    position: [
                        pose.position.x.bits(),
                        pose.position.y.bits(),
                        pose.position.z.bits(),
                    ],
                    collision_height: height,
                },
                result: result.map(Box::new),
            },
            PlayerEnvironmentPhase::Position {
                input_sequence: self.state.next_sequence,
                pose,
                refreshed: query.is_some(),
            },
        )?;
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
