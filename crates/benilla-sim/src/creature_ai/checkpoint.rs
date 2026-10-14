// SPDX-License-Identifier: GPL-3.0-or-later
//! Checked AI checkpoints, separate from live non-deserializable authority state.
use super::*;
use serde::{Deserialize, Serialize};
pub const AI_CHECKPOINT_VERSION: u32 = 4;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AiCheckpoint {
    version: u32,
    phase: Phase,
    position: Point,
    combat_start: Point,
    last_refresh: Point,
    target: Option<u64>,
    victim_detached_at_ms: Option<u64>,
    pending_receipts: Option<(u64, u64)>,
    threat: Real,
    now_ms: u64,
    sequence: u64,
    receipt_id: u64,
    movement_request_id: u64,
    command_id: u64,
    motion: Option<MotionCommand>,
    mainhand_ms: u32,
    pursuit_ms: u32,
    combat_tick_ms: u32,
    unreachable_ms: u32,
    corpse_at_ms: u64,
    corpse_timer_previous_ms: u64,
    corpse_timer_last_ms: u64,
    corpse_timer_calls: Vec<CorpseTimerCall>,
    respawn_at_ms: u64,
    death_at_ms: Option<u64>,
    death_deferred: bool,
    despawned_at_ms: Option<u64>,
}
impl SoloCreatureAi {
    pub fn checkpoint(&self) -> AiCheckpoint {
        AiCheckpoint {
            version: AI_CHECKPOINT_VERSION,
            phase: self.phase,
            position: self.position,
            combat_start: self.combat_start,
            last_refresh: self.last_refresh,
            target: self.target,
            victim_detached_at_ms: self.victim_detached_at_ms,
            pending_receipts: self.pending_receipts,
            threat: self.threat,
            now_ms: self.now_ms,
            sequence: self.sequence,
            receipt_id: self.receipt_id,
            movement_request_id: self.movement_request_id,
            command_id: self.command_id,
            motion: self.motion.clone(),
            mainhand_ms: self.mainhand_ms,
            pursuit_ms: self.pursuit_ms,
            combat_tick_ms: self.combat_tick_ms,
            unreachable_ms: self.unreachable_ms,
            corpse_at_ms: self.corpse_at_ms,
            corpse_timer_previous_ms: self.corpse_timer_previous_ms,
            corpse_timer_last_ms: self.corpse_timer_last_ms,
            corpse_timer_calls: self.corpse_timer_calls.clone(),
            respawn_at_ms: self.respawn_at_ms,
            death_at_ms: self.death_at_ms,
            death_deferred: self.death_deferred,
            despawned_at_ms: self.despawned_at_ms,
        }
    }
    pub fn restore(
        s: AiCheckpoint,
        profile: Profile,
        now_ms: u64,
        sequence: u64,
        solo_target: u64,
    ) -> Result<Self, AiError> {
        profile.validate()?;
        for point in [s.position, s.combat_start, s.last_refresh] {
            point.validate()?;
        }
        s.threat.nonnegative("checkpoint threat")?;
        if s.pending_receipts.is_some()
            || s.victim_detached_at_ms.is_some_and(|at| {
                at != now_ms || s.phase != Phase::Combat || s.threat != Real::ZERO
            })
            || s.version != AI_CHECKPOINT_VERSION
            || s.now_ms != now_ms
            || s.sequence != sequence
            || now_ms < profile.clock_origin_ms
            || solo_target == 0
            || s.target.is_some_and(|t| t != solo_target)
            || s.mainhand_ms > profile.mainhand_interval_ms.max(100)
            || s.pursuit_ms > profile.pursuit_ms
            || s.unreachable_ms > UNREACHABLE_MS
            || s.combat_tick_ms > 60_000u32.saturating_add(COMBAT_TICK_MS)
            || s.movement_request_id > s.sequence
            || s.command_id > s.receipt_id.saturating_add(s.sequence).saturating_mul(2)
        {
            return Err(AiError::Invalid("AI checkpoint bounds/clock/version"));
        }
        if let Some(m) = &s.motion {
            if m.command_id == 0 || m.command_id != s.command_id {
                return Err(AiError::Invalid("AI checkpoint motion ID"));
            }
            if let Some(home) = m.home {
                home.validate()?;
            }
        }
        if let Some(death) = s.death_at_ms {
            if death > now_ms
                || death < profile.clock_origin_ms
                || !s.valid_loot_timer(death, &profile)
                || (death / 1000)
                    .checked_add(u64::from(profile.respawn_delay_seconds))
                    .and_then(|x| x.checked_mul(1000))
                    != Some(s.respawn_at_ms)
                || !matches!(s.phase, Phase::Corpse | Phase::Despawned)
            {
                return Err(AiError::Invalid("AI checkpoint death timers"));
            }
        } else if !s.death_deferred
            || matches!(s.phase, Phase::Corpse | Phase::Despawned)
            || !s.corpse_timer_calls.is_empty()
            || s.corpse_timer_last_ms != 0
            || s.corpse_timer_previous_ms != 0
        {
            return Err(AiError::Invalid("AI checkpoint missing death instant"));
        }
        if let Some(at) = s.despawned_at_ms {
            if s.phase != Phase::Despawned || at < s.corpse_at_ms || at > now_ms {
                return Err(AiError::Invalid("AI checkpoint corpse removal time"));
            }
        } else if s.phase == Phase::Despawned {
            return Err(AiError::Invalid(
                "AI checkpoint missing corpse removal time",
            ));
        }
        let phase_ok = match s.phase {
            Phase::Idle => {
                s.target.is_none()
                    && s.threat == Real::ZERO
                    && s.motion.is_none()
                    && s.unreachable_ms == 0
            }
            Phase::Combat => {
                s.target == Some(solo_target)
                    && s.motion.as_ref().is_some_and(|m| {
                        m.purpose
                            == MovementPurpose::Chase {
                                target: solo_target,
                            }
                            && m.home.is_none()
                    })
            }
            Phase::ReturningHome => {
                s.target.is_none()
                    && s.threat == Real::ZERO
                    && s.unreachable_ms == 0
                    && s.motion.as_ref().is_some_and(|m| {
                        m.purpose == MovementPurpose::Home && m.home == Some(s.combat_start)
                    })
            }
            Phase::Corpse | Phase::Despawned => {
                s.target.is_none()
                    && s.threat == Real::ZERO
                    && s.motion.is_none()
                    && s.unreachable_ms == 0
                    && s.corpse_at_ms >= profile.clock_origin_ms
                    && s.respawn_at_ms >= profile.clock_origin_ms
                    && (s.phase != Phase::Despawned
                        || (s.position == profile.spawn && now_ms >= s.corpse_at_ms))
            }
        };
        if !phase_ok {
            return Err(AiError::Invalid("AI checkpoint phase invariants"));
        }
        Ok(Self {
            profile,
            phase: s.phase,
            position: s.position,
            combat_start: s.combat_start,
            last_refresh: s.last_refresh,
            target: s.target,
            victim_detached_at_ms: s.victim_detached_at_ms,
            pending_receipts: None,
            threat: s.threat,
            now_ms: s.now_ms,
            sequence: s.sequence,
            receipt_id: s.receipt_id,
            movement_request_id: s.movement_request_id,
            command_id: s.command_id,
            motion: s.motion,
            mainhand_ms: s.mainhand_ms,
            pursuit_ms: s.pursuit_ms,
            combat_tick_ms: s.combat_tick_ms,
            unreachable_ms: s.unreachable_ms,
            corpse_at_ms: s.corpse_at_ms,
            corpse_timer_previous_ms: s.corpse_timer_previous_ms,
            corpse_timer_last_ms: s.corpse_timer_last_ms,
            corpse_timer_calls: s.corpse_timer_calls,
            respawn_at_ms: s.respawn_at_ms,
            death_at_ms: s.death_at_ms,
            death_deferred: s.death_deferred,
            despawned_at_ms: s.despawned_at_ms,
        })
    }
    /// Additional invariants of a continuously updated fixed-step world adapter.
    pub fn validate_fixed_step(&self, tick_ms: u32) -> Result<(), AiError> {
        if tick_ms == 0
            || !(self.now_ms - self.profile.clock_origin_ms).is_multiple_of(u64::from(tick_ms))
        {
            return Err(AiError::Invalid("AI fixed step clock"));
        }
        let aligned = |t: u64| {
            t >= self.profile.clock_origin_ms
                && (t - self.profile.clock_origin_ms).is_multiple_of(u64::from(tick_ms))
        };
        if self.death_at_ms.is_some_and(|t| !aligned(t))
            || self.despawned_at_ms.is_some_and(|t| !aligned(t))
        {
            return Err(AiError::Invalid("AI lifecycle alignment"));
        }
        if let Some(removed) = self.despawned_at_ms {
            let death = self
                .death_at_ms
                .ok_or(AiError::Invalid("fixed-step corpse death"))?;
            let relative = self
                .corpse_at_ms
                .checked_sub(self.profile.clock_origin_ms)
                .ok_or(AiError::Invalid("fixed-step corpse origin"))?;
            let tick = u64::from(tick_ms);
            let ceiling = relative
                .checked_add(tick - 1)
                .and_then(|t| (t / tick).checked_mul(tick))
                .and_then(|t| t.checked_add(self.profile.clock_origin_ms))
                .ok_or(AiError::Invalid("fixed-step corpse time overflow"))?;
            let earliest = ceiling.max(if self.death_deferred {
                death
                    .checked_add(tick)
                    .ok_or(AiError::Invalid("fixed-step death overflow"))?
            } else {
                death
            });
            if removed != earliest {
                return Err(AiError::Invalid("delayed fixed-step corpse removal"));
            }
        }
        match self.phase {
            Phase::Corpse
                if (self.death_at_ms != Some(self.now_ms) || !self.death_deferred)
                    && self.now_ms >= self.corpse_at_ms =>
            {
                Err(AiError::Invalid("overdue fixed-step corpse"))
            }
            Phase::Despawned
                if self.despawned_at_ms != Some(self.now_ms)
                    && self.now_ms >= self.respawn_at_ms =>
            {
                Err(AiError::Invalid("overdue fixed-step respawn"))
            }
            _ => Ok(()),
        }
    }
    pub fn last_receipt_id(&self) -> u64 {
        self.receipt_id
    }
    pub fn last_movement_request_id(&self) -> u64 {
        self.movement_request_id
    }
}

impl AiCheckpoint {
    fn valid_loot_timer(&self, death: u64, profile: &Profile) -> bool {
        if self.corpse_timer_calls.len() > 1
            || self.corpse_timer_last_ms < death
            || self.corpse_timer_last_ms > self.now_ms
            || self
                .despawned_at_ms
                .is_some_and(|at| self.corpse_timer_last_ms > at)
        {
            return false;
        }
        let Some(call) = self.corpse_timer_calls.first() else {
            return self.corpse_timer_previous_ms == 0
                && death.checked_add(u64::from(profile.corpse_decay_ms))
                    == Some(self.corpse_at_ms);
        };
        if call.delay_ms == 0
            || call.at_ms < death
            || call.at_ms > self.corpse_timer_last_ms
            || self.corpse_timer_previous_ms < death
        {
            return false;
        }
        let Some(until) = call.at_ms.checked_add(u64::from(call.delay_ms)) else {
            return false;
        };
        let expected = if call.inspecting {
            self.corpse_timer_previous_ms.max(until)
        } else {
            self.corpse_timer_previous_ms.min(until)
        };
        expected == self.corpse_at_ms && expected != self.corpse_timer_previous_ms
    }
}
