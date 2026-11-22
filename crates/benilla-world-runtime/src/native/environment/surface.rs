// SPDX-License-Identifier: GPL-3.0-or-later
//! Native surface-only execution. The selected NativeWorld frame owns rollback of
//! its actual callback state; this reducer stages only its own environment state.
use super::*;

/// Only the native frame's validated callback source may select this executor.
/// Implementations must mutate the actual staged native owners, never certify an
/// effect by recording an intent. No Clone bound can make external effects atomic.
pub(crate) trait NativeEnvironmentEffects {
    fn effect(&mut self, intent: EnvironmentCallback, flags: u8) -> Result<(), EnvironmentError>;
}

struct NativeSurfaceCallbacks<'a, H: ?Sized> {
    timers: &'a mut [MirrorTimer; 4],
    effects: &'a mut H,
}
impl<H: NativeEnvironmentEffects + ?Sized> Callbacks for NativeSurfaceCallbacks<'_, H> {
    fn effect(&mut self, intent: EnvironmentCallback, flags: u8) -> Result<(), EnvironmentError> {
        match intent {
            EnvironmentCallback::SetMirrorScale { timer, scale } => {
                self.timers[mirror_index(timer)].set_scale(scale);
                Ok(())
            }
            EnvironmentCallback::UpdateThreatTables
            | EnvironmentCallback::RemoveInterruptAuras { .. } => {
                self.effects.effect(intent, flags)
            }
            _ => Err(EnvironmentError::CallbacksUnavailable(intent)),
        }
    }
    fn has_aura(&mut self, spell_id: u32) -> Result<bool, EnvironmentError> {
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadHasAura { spell_id },
        ))
    }
    fn breathing_interval(&mut self) -> Result<u32, EnvironmentError> {
        Err(EnvironmentError::CallbacksUnavailable(
            EnvironmentCallback::ReadWaterBreathingInterval,
        ))
    }
}

fn cleared_mirrors() -> [MirrorTimer; 4] {
    let mut timers = initial_mirrors();
    for timer in [
        EnvironmentMirror::Fatigue,
        EnvironmentMirror::Breath,
        EnvironmentMirror::Environmental,
    ] {
        timers[mirror_index(timer)].set_scale(10);
    }
    timers
}

impl EnvironmentState {
    /// Explicitly selected surface domain. UpdateMirrorTimers (Player.cpp:1409)
    /// precedes the refresh countdown (1483); this phase never activates/ticks it.
    pub(crate) fn validate_native_surface(
        &self,
        catalog: &LiquidCatalog,
    ) -> Result<(), EnvironmentError> {
        if self.flags & !(LIQUID | HIGH_LIQUID | IN_WATER) != 0
            || (self.flags != 0 && self.flags & LIQUID == 0)
            || (self.mirrors != initial_mirrors() && self.mirrors != cleared_mirrors())
            || self
                .last_liquid
                .is_some_and(|id| id == 0 || catalog.get(id).is_none_or(|row| row.spell_id != 0))
        {
            return Err(EnvironmentError::InvalidInput);
        }
        Ok(())
    }

    /// The caller must select a validated native callback source and stage the
    /// complete owning frame. Errors discard this reducer's state, but may follow
    /// actual host mutations; only that outer frame can roll those mutations back.
    pub(crate) fn reduce_native_surface(
        &self,
        snapshot: EnvironmentSnapshotBinding,
        catalog: &LiquidCatalog,
        sequence: u64,
        phase: &EnvironmentPhase,
        effects: &mut (impl NativeEnvironmentEffects + ?Sized),
    ) -> Result<(Self, u64), EnvironmentError> {
        let next_sequence = sequence.checked_add(1).ok_or(EnvironmentError::Limit)?;
        self.snapshot()
            .restore_native_surface(snapshot, catalog, sequence)?;
        phase.validate_native_surface(catalog)?;
        let mut next = self.clone();
        match phase {
            EnvironmentPhase::Update { diff_ms } => next.update_timer(*diff_ms),
            EnvironmentPhase::Position { pose, result } => {
                match (
                    self.query_for_position(snapshot, sequence, *pose)?,
                    result.as_deref().copied(),
                ) {
                    (None, None) => {}
                    (Some(expected), Some(reply)) => {
                        reply.validate(expected)?;
                        let mut mirrors = next.mirrors.clone();
                        next.refresh_if_due(
                            reply,
                            catalog,
                            &mut NativeSurfaceCallbacks {
                                timers: &mut mirrors,
                                effects,
                            },
                        )?;
                        next.mirrors = mirrors;
                    }
                    _ => return Err(EnvironmentError::Binding),
                }
                next.last_position = Some(*pose);
            }
        }
        next.validate_native_surface(catalog)?;
        Ok((next, next_sequence))
    }
}

impl EnvironmentPhase {
    pub(crate) fn validate_native_surface(
        &self,
        catalog: &LiquidCatalog,
    ) -> Result<(), EnvironmentError> {
        if let Self::Position {
            result: Some(result),
            ..
        } = self
        {
            if result.status == LiquidStatus::UnderWater
                || result.data.is_some_and(|data| {
                    data.type_flags & (0x01 | 0x04 | 0x10) != 0
                        || (data.entry != 0
                            && catalog.get(data.entry).is_some_and(|row| row.spell_id != 0))
                })
            {
                return Err(EnvironmentError::InvalidInput);
            }
        }
        Ok(())
    }
}

impl EnvironmentStateSnapshot {
    /// Native surface snapshots have a different admission domain from the
    /// standalone component. Hashes establish consistency, never historical truth.
    pub(crate) fn restore_native_surface(
        self,
        snapshot: EnvironmentSnapshotBinding,
        catalog: &LiquidCatalog,
        sequence: u64,
    ) -> Result<EnvironmentState, EnvironmentError> {
        snapshot.validate(catalog)?;
        if self.timer_ms > REFRESH_MS {
            return Err(EnvironmentError::InvalidCheckpoint);
        }
        if let Some(pose) = self.last_position {
            pose.validate()
                .map_err(|_| EnvironmentError::InvalidCheckpoint)?;
        }
        match self.cached_query {
            None if self.flags == 0
                && self.timer_ms == 0
                && self.last_liquid.is_none()
                && self.last_position.is_none() => {}
            Some(q)
                if q.snapshot == snapshot
                    && q.ticket != 0
                    && q.ticket <= sequence
                    && q.required_type == ALL_LIQUIDS
                    && self.last_position.is_some() =>
            {
                let cached_pose = EnvironmentPose {
                    position: q.position,
                    collision_height: q.collision_height,
                };
                cached_pose
                    .validate()
                    .map_err(|_| EnvironmentError::InvalidCheckpoint)?;
                if (self.flags == 0 && self.last_liquid.is_some())
                    || (q.ticket == sequence
                        && (self.timer_ms != REFRESH_MS || self.last_position != Some(cached_pose)))
                {
                    return Err(EnvironmentError::InvalidCheckpoint);
                }
            }
            _ => return Err(EnvironmentError::InvalidCheckpoint),
        }
        let mirrors = if self.mirrors == initial_mirrors().each_ref().map(MirrorTimer::snapshot) {
            initial_mirrors()
        } else if self.mirrors == cleared_mirrors().each_ref().map(MirrorTimer::snapshot) {
            // At least observe liquid, expire its cache, then clear it. Re-entering
            // requires another expiry and observation; cached poses never refresh.
            let minimum_ticket = if self.flags == 0 { 3 } else { 5 };
            if self.cached_query.is_none_or(|q| q.ticket < minimum_ticket) {
                return Err(EnvironmentError::InvalidCheckpoint);
            }
            cleared_mirrors()
        } else {
            return Err(EnvironmentError::InvalidCheckpoint);
        };
        let state = EnvironmentState {
            flags: self.flags,
            timer_ms: self.timer_ms,
            last_liquid: self.last_liquid,
            last_position: self.last_position,
            cached_query: self.cached_query,
            mirrors,
        };
        state
            .validate_native_surface(catalog)
            .map_err(|_| EnvironmentError::InvalidCheckpoint)?;
        Ok(state)
    }
}

#[cfg(test)]
mod tests;
