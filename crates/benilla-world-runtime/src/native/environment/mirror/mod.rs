// SPDX-License-Identifier: GPL-3.0-or-later
//! Owned, standalone CMaNGOS MirrorTimer mechanics; see ../../MIRROR_TIMER.md.
//! CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
//! Player.cpp:154–286, Player.h:410–474 and shared/Util/Timer.h:105–128.
//! Copyright CMaNGOS contributors, upstream GPL-2.0-or-later.
//! No player update driver, spell/health effects, packets, Engine or admission.
mod persistence;
pub(crate) use persistence::MirrorTimerSnapshot;
pub use persistence::{MirrorTimerCheckpoint, MirrorTimerRecord};
use serde::{Deserialize, Serialize};

pub const MIRROR_TIMER_POLICY: &str = "cmangos-8ec338a1-owned-mirror-timer-v1";
pub const MAX_MIRROR_TIMER_RECORDS: usize = 2048;
pub const MAX_MIRROR_TIMER_BYTES: usize = 4 * 1024 * 1024;
const PULSE_INTERVAL_MS: u32 = 2000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MirrorTimerType {
    Fatigue = 0,
    Breath = 1,
    FeignDeath = 2,
    Environmental = 3,
}
impl MirrorTimerType {
    pub const ALL: [Self; 4] = [
        Self::Fatigue,
        Self::Breath,
        Self::FeignDeath,
        Self::Environmental,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MirrorTimerStatus {
    Unchanged = 0,
    FullUpdate = 1,
    StatusUpdate = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MirrorTimerError {
    /// Source std::abs(INT32_MIN) is undefined when Update reaches it.
    UndefinedScaleMagnitude,
    InvalidIdentity,
    InvalidCheckpoint,
    ReplayMismatch,
    TooLarge,
    JournalFull,
    Limit,
}
impl std::fmt::Display for MirrorTimerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "mirror timer component: {self:?}")
    }
}
impl std::error::Error for MirrorTimerError {}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ShortIntervalTimer {
    interval: u32,
    current: u32,
}
impl ShortIntervalTimer {
    fn new() -> Self {
        Self {
            interval: 0,
            current: 0,
        }
    }
    fn update(&mut self, diff: u32) {
        self.current = self.current.wrapping_add(diff);
    }
    fn passed(&self) -> bool {
        self.current >= self.interval
    }
    fn reset(&mut self) {
        if self.passed() {
            self.current = self.current.wrapping_sub(self.interval);
        }
    }
}

/// Fully owned source state. Direct methods perform only timer mutations; a false
/// Update result is the source signal to a caller, never an executed damage tick.
/// There is no public Deserialize or field mutation bypassing these methods.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MirrorTimer {
    timer_type: MirrorTimerType,
    scale: i32,
    spell_id: u32,
    tracker: ShortIntervalTimer,
    pulse: ShortIntervalTimer,
    status: MirrorTimerStatus,
    active: bool,
    frozen: bool,
}
impl MirrorTimer {
    pub fn new(timer_type: MirrorTimerType) -> Self {
        Self {
            timer_type,
            scale: -1,
            spell_id: 0,
            tracker: ShortIntervalTimer::new(),
            pulse: ShortIntervalTimer::new(),
            status: MirrorTimerStatus::Unchanged,
            active: false,
            frozen: false,
        }
    }
    pub fn timer_type(&self) -> MirrorTimerType {
        self.timer_type
    }
    pub fn scale(&self) -> i32 {
        self.scale
    }
    pub fn spell_id(&self) -> u32 {
        self.spell_id
    }
    pub fn is_active(&self) -> bool {
        self.active
    }
    pub fn is_regenerating(&self) -> bool {
        self.scale > 0
    }
    pub fn is_frozen(&self) -> bool {
        self.frozen && !self.is_regenerating()
    }
    /// The stored bit, distinct from IsFrozen while regenerating.
    pub fn frozen_requested(&self) -> bool {
        self.frozen
    }
    pub fn duration(&self) -> u32 {
        self.tracker.interval
    }
    pub fn remaining(&self) -> u32 {
        self.tracker.interval.wrapping_sub(self.tracker.current)
    }
    pub fn tracker_current(&self) -> u32 {
        self.tracker.current
    }
    pub fn pulse_interval(&self) -> u32 {
        self.pulse.interval
    }
    pub fn pulse_current(&self) -> u32 {
        self.pulse.current
    }
    /// Non-consuming inspection for persistence and diagnostics.
    pub fn status(&self) -> MirrorTimerStatus {
        self.status
    }

    pub fn fetch_status(&mut self) -> MirrorTimerStatus {
        let status = self.status;
        self.status = MirrorTimerStatus::Unchanged;
        status
    }
    pub fn stop(&mut self) {
        if self.active {
            self.active = false;
            self.pulse.current = 0;
            self.tracker.current = 0;
            self.status = MirrorTimerStatus::StatusUpdate;
        }
    }
    pub fn start(&mut self, interval: u32, spell_id: u32) {
        if self.scale < 0 {
            self.active = true;
            self.pulse.current = 0;
            self.pulse.interval = PULSE_INTERVAL_MS;
            self.tracker.current = 0;
            self.tracker.interval = interval;
            self.spell_id = spell_id;
            self.status = MirrorTimerStatus::FullUpdate;
        } else {
            self.stop();
        }
    }
    /// Source Start(current, max, spellId): `current` means remaining duration.
    pub fn start_remaining(&mut self, current: u32, max: u32, spell_id: u32) {
        self.start(max, spell_id);
        if self.active {
            self.tracker.current = max.wrapping_sub(current);
            self.set_frozen(false);
        }
    }
    pub fn set_remaining(&mut self, duration: u32) {
        if duration == 0 {
            self.stop();
            return;
        }
        if self.active && duration != self.remaining() {
            self.status = MirrorTimerStatus::FullUpdate;
        }
        self.tracker.current = self.duration().wrapping_sub(duration);
    }
    pub fn set_duration(&mut self, duration: u32) {
        if duration == 0 {
            self.stop();
            return;
        }
        if self.active && duration != self.duration() {
            self.status = MirrorTimerStatus::FullUpdate;
        }
        self.tracker.interval = duration;
    }
    pub fn set_frozen(&mut self, state: bool) {
        if self.active && state != self.is_frozen() {
            self.status = MirrorTimerStatus::StatusUpdate;
        }
        self.frozen = state;
    }
    pub fn set_scale(&mut self, scale: i32) {
        if scale == 0 {
            self.set_frozen(true);
            return;
        }
        if self.active && scale != self.scale {
            self.status = MirrorTimerStatus::FullUpdate;
        }
        self.scale = scale;
    }
    /// Return the exact source bool: false on expiration/pulse, true otherwise.
    /// Unsigned source arithmetic wraps. Undefined abs(INT32_MIN) is rejected only
    /// if reached; rejection leaves the complete state untouched.
    pub fn update(&mut self, diff: u32) -> Result<bool, MirrorTimerError> {
        if !self.active || self.is_frozen() {
            return Ok(true);
        }
        let magnitude = self
            .scale
            .checked_abs()
            .ok_or(MirrorTimerError::UndefinedScaleMagnitude)? as u32;
        let diff = diff.wrapping_mul(magnitude);
        if self.scale < 0 {
            self.tracker.update(diff);
            if !self.tracker.passed() {
                return Ok(true);
            }
            let interval = self.tracker.interval;
            let overflow = self.tracker.current.wrapping_sub(interval);
            self.tracker.current = interval;
            if overflow == diff {
                self.pulse.update(overflow);
                if !self.pulse.passed() {
                    return Ok(true);
                }
                // Source resets once, not modulo: a long tick retains backlog.
                self.pulse.reset();
            }
            Ok(false)
        } else {
            let current = self.tracker.current;
            if current > diff {
                self.tracker.current = current.wrapping_sub(diff);
                self.pulse.current = 0;
            } else {
                self.stop();
            }
            Ok(true)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum MirrorTimerOperation {
    Start {
        interval: u32,
        spell_id: u32,
    },
    StartRemaining {
        current: u32,
        max: u32,
        spell_id: u32,
    },
    Stop,
    SetRemaining {
        duration: u32,
    },
    SetDuration {
        duration: u32,
    },
    SetFrozen {
        frozen: bool,
    },
    SetScale {
        scale: i32,
    },
    FetchStatus,
    Update {
        diff: u32,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirrorTimerOutcome {
    None,
    Status(MirrorTimerStatus),
    Update(bool),
}
impl MirrorTimer {
    fn apply(
        &mut self,
        operation: &MirrorTimerOperation,
    ) -> Result<MirrorTimerOutcome, MirrorTimerError> {
        use MirrorTimerOperation as Op;
        match *operation {
            Op::Start { interval, spell_id } => self.start(interval, spell_id),
            Op::StartRemaining {
                current,
                max,
                spell_id,
            } => self.start_remaining(current, max, spell_id),
            Op::Stop => self.stop(),
            Op::SetRemaining { duration } => self.set_remaining(duration),
            Op::SetDuration { duration } => self.set_duration(duration),
            Op::SetFrozen { frozen } => self.set_frozen(frozen),
            Op::SetScale { scale } => self.set_scale(scale),
            Op::FetchStatus => return Ok(MirrorTimerOutcome::Status(self.fetch_status())),
            Op::Update { diff } => return self.update(diff).map(MirrorTimerOutcome::Update),
        }
        Ok(MirrorTimerOutcome::None)
    }
}

/// Four source timers with independent, bounded checkpoint/journal ownership.
/// Neither construction nor replay starts a world update loop or applies effects.
#[derive(Clone, Debug)]
pub struct MirrorTimerMachine {
    world: crate::WorldId,
    player: crate::PlayerId,
    timers: [MirrorTimer; 4],
    sequence: u64,
    tip: crate::Digest,
    records: Vec<MirrorTimerRecord>,
    journal_bytes: usize,
}
impl MirrorTimerMachine {
    pub fn new(world: crate::WorldId, player: crate::PlayerId) -> Result<Self, MirrorTimerError> {
        if world == [0; 16] || player.owner == [0; 16] || player.character == [0; 16] {
            return Err(MirrorTimerError::InvalidIdentity);
        }
        Ok(Self {
            world,
            player,
            timers: MirrorTimerType::ALL.map(MirrorTimer::new),
            sequence: 0,
            tip: [0; 32],
            records: vec![],
            journal_bytes: 2,
        })
    }
    pub fn timer(&self, timer_type: MirrorTimerType) -> &MirrorTimer {
        &self.timers[timer_type as usize]
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn records(&self) -> &[MirrorTimerRecord] {
        &self.records
    }
    pub fn apply(
        &mut self,
        timer_type: MirrorTimerType,
        operation: MirrorTimerOperation,
    ) -> Result<MirrorTimerOutcome, MirrorTimerError> {
        if self.records.len() >= MAX_MIRROR_TIMER_RECORDS {
            return Err(MirrorTimerError::JournalFull);
        }
        let sequence = self
            .sequence
            .checked_add(1)
            .ok_or(MirrorTimerError::Limit)?;
        let mut timers = self.timers.clone();
        let outcome = timers[timer_type as usize].apply(&operation)?;
        let record =
            MirrorTimerRecord::new(self, sequence, timer_type, operation, outcome, &timers);
        let record_len = serde_json::to_vec(&record)
            .map_err(|_| MirrorTimerError::InvalidCheckpoint)?
            .len();
        let journal_bytes = self
            .journal_bytes
            .checked_add(record_len)
            .and_then(|n| n.checked_add(usize::from(!self.records.is_empty())))
            .ok_or(MirrorTimerError::TooLarge)?;
        if journal_bytes > MAX_MIRROR_TIMER_BYTES {
            return Err(MirrorTimerError::JournalFull);
        }
        self.timers = timers;
        self.sequence = sequence;
        self.tip = record.digest();
        self.records.push(record);
        self.journal_bytes = journal_bytes;
        Ok(outcome)
    }
}

#[cfg(test)]
mod tests;
