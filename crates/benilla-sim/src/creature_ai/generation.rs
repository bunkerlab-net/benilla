// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Creature.cpp SetDeathState/Respawn/InitEntry, pinned 8ec338a1.
//! Checked per-life timing and the single-use InitEntry boundary.
use super::*;

pub(super) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    serde::Deserialize::deserialize(deserializer)
}

/// The authority's fresh respawn-delay selection for exactly one death receipt.
/// This changes no per-life corpse delay and advances no AI clock on its own.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DeathTiming {
    pub receipt_id: u64,
    pub at_ms: u64,
    pub respawn_delay_seconds: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct GenerationLifecycle {
    pub generation: u64,
    #[serde(deserialize_with = "required_option")]
    pub pending_respawn: Option<RespawnBoundary>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RespawnBoundary {
    sequence: u64,
    at_ms: u64,
    previous_profile: Profile,
}

impl SoloCreatureAi {
    /// Select source-owned generation timing at creation, before any observations.
    /// The enclosing owner authenticates the initial and every later projection.
    pub fn new_generation_owned(profile: Profile) -> Result<Self, AiError> {
        let mut ai = Self::new(profile)?;
        ai.generation_lifecycle = Some(GenerationLifecycle {
            generation: 0,
            pending_respawn: None,
        });
        Ok(ai)
    }

    pub fn effective_profile(&self) -> &Profile {
        &self.profile
    }

    pub fn generation(&self) -> Option<u64> {
        self.generation_lifecycle.as_ref().map(|s| s.generation)
    }

    /// Consume the actual Respawned boundary exactly once. Native generation
    /// identity and source selection must be checked by the enclosing transaction.
    /// All clocks, timers, receipts, commands and movement counters are retained.
    pub fn rebind_after_respawn(
        &mut self,
        projected: Profile,
        previous_generation: u64,
        next_generation: u64,
    ) -> Result<(), AiError> {
        projected.validate()?;
        let lifecycle = self
            .generation_lifecycle
            .as_ref()
            .ok_or(AiError::Invalid("generation ownership not selected"))?;
        let boundary = lifecycle
            .pending_respawn
            .as_ref()
            .ok_or(AiError::Invalid("missing respawn rebind boundary"))?;
        if lifecycle.generation != previous_generation
            || previous_generation.checked_add(1) != Some(next_generation)
            || boundary.sequence != self.sequence
            || boundary.at_ms != self.now_ms
            || boundary.previous_profile != self.profile
            || self.phase != Phase::Idle
            || self.pending_receipts.is_some()
            || self.position != self.profile.spawn
            || self.target.is_some()
            || self.victim_detached_at_ms.is_some()
            || self.hostile_references != HostileReferences::new()
            || self.motion.is_some()
            || self.unreachable_ms != 0
            || self.death_at_ms.is_some()
            || !self.death_deferred
            || self.despawned_at_ms.is_some()
            || !self.corpse_timer_calls.is_empty()
            || self.corpse_timer_previous_ms != 0
            || self.corpse_timer_last_ms != 0
        {
            return Err(AiError::Invalid("respawn rebind boundary binding"));
        }
        // InitEntry/SelectLevel owns only these fields; reaction, identity,
        // clock origin, aggro and leash policy are not mutable generation inputs.
        let mut permitted = self.profile.clone();
        permitted.level = projected.level;
        permitted.combat_reach = projected.combat_reach;
        permitted.bounding_radius = projected.bounding_radius;
        permitted.run_speed = projected.run_speed;
        permitted.home_speed = projected.home_speed;
        permitted.mainhand_interval_ms = projected.mainhand_interval_ms;
        permitted.corpse_decay_ms = projected.corpse_decay_ms;
        if permitted != projected {
            return Err(AiError::Invalid("respawn changed stable AI profile"));
        }
        self.profile = projected;
        self.generation_lifecycle = Some(GenerationLifecycle {
            generation: next_generation,
            pending_respawn: None,
        });
        Ok(())
    }

    pub(super) fn require_finished_generation(&self) -> Result<(), AiError> {
        if self
            .generation_lifecycle
            .as_ref()
            .is_some_and(|s| s.pending_respawn.is_some())
        {
            Err(AiError::Invalid("unfinished respawn profile rebind"))
        } else {
            Ok(())
        }
    }

    pub(super) fn mark_respawn_boundary(&mut self) -> Result<(), AiError> {
        if let Some(lifecycle) = &mut self.generation_lifecycle {
            if lifecycle.pending_respawn.is_some() || lifecycle.generation == u64::MAX {
                return Err(AiError::Invalid("respawn generation overflow/boundary"));
            }
            lifecycle.pending_respawn = Some(RespawnBoundary {
                sequence: self.sequence,
                at_ms: self.now_ms,
                previous_profile: self.profile.clone(),
            });
        }
        Ok(())
    }

    pub(super) fn validate_death_timing(
        &self,
        observation: &Observation,
        now_ms: u64,
        timing: Option<DeathTiming>,
    ) -> Result<(), AiError> {
        let mut deaths = observation
            .receipts
            .iter()
            .filter(|receipt| receipt.event == CombatEvent::Died);
        let death = deaths.next();
        if deaths.next().is_some() {
            return Err(AiError::Invalid("multiple death receipts"));
        }
        match (self.generation_lifecycle.is_some(), death, timing) {
            (true, Some(receipt), Some(timing))
                if receipt.id == timing.receipt_id
                    && receipt.id > self.receipt_id
                    && timing.at_ms == now_ms
                    && !matches!(self.phase, Phase::Corpse | Phase::Despawned) =>
            {
                Ok(())
            }
            (_, None, None) | (false, Some(_), None) => Ok(()),
            _ => Err(AiError::Invalid("death timing receipt/time/owner binding")),
        }
    }
}

#[cfg(test)]
mod tests;
