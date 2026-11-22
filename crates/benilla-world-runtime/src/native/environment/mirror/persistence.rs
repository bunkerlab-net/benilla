// SPDX-License-Identifier: GPL-3.0-or-later
//! Component integrity and replay, not authenticated history or NativeWorld saves.
use super::*;
const VERSION: u32 = 1;

/// Crate-private transfer DTO. Restore validates it before it becomes timer state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct MirrorTimerSnapshot {
    timer_type: MirrorTimerType,
    scale: i32,
    spell_id: u32,
    status: MirrorTimerStatus,
    active: bool,
    frozen: bool,
    tracker_interval: u32,
    tracker_current: u32,
    pulse_interval: u32,
    pulse_current: u32,
}
impl MirrorTimer {
    pub(crate) fn snapshot(&self) -> MirrorTimerSnapshot {
        MirrorTimerSnapshot {
            timer_type: self.timer_type,
            scale: self.scale,
            spell_id: self.spell_id,
            status: self.status,
            active: self.active,
            frozen: self.frozen,
            tracker_interval: self.tracker.interval,
            tracker_current: self.tracker.current,
            pulse_interval: self.pulse.interval,
            pulse_current: self.pulse.current,
        }
    }
    pub(crate) fn restore(
        stored: MirrorTimerSnapshot,
        expected_type: MirrorTimerType,
    ) -> Result<Self, MirrorTimerError> {
        if stored.timer_type != expected_type
            || stored.scale == 0
            || !matches!(stored.pulse_interval, 0 | PULSE_INTERVAL_MS)
            || stored.pulse_current > u32::MAX - PULSE_INTERVAL_MS
            || (!stored.active && stored.status == MirrorTimerStatus::FullUpdate)
            || (stored.pulse_interval == 0
                && (stored.active
                    || stored.pulse_current != 0
                    || stored.spell_id != 0
                    || stored.status != MirrorTimerStatus::Unchanged))
            || (!stored.active && stored.pulse_current != 0)
        {
            return Err(MirrorTimerError::InvalidCheckpoint);
        }
        Ok(Self {
            timer_type: stored.timer_type,
            scale: stored.scale,
            spell_id: stored.spell_id,
            status: stored.status,
            active: stored.active,
            frozen: stored.frozen,
            tracker: ShortIntervalTimer {
                interval: stored.tracker_interval,
                current: stored.tracker_current,
            },
            pulse: ShortIntervalTimer {
                interval: stored.pulse_interval,
                current: stored.pulse_current,
            },
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorTimerRecord {
    version: u32,
    policy: String,
    world: crate::WorldId,
    player: crate::PlayerId,
    sequence: u64,
    timer_type: MirrorTimerType,
    operation: MirrorTimerOperation,
    outcome: MirrorTimerOutcome,
    before: crate::Digest,
    after: crate::Digest,
    previous: crate::Digest,
}
impl MirrorTimerRecord {
    pub(super) fn new(
        machine: &MirrorTimerMachine,
        sequence: u64,
        timer_type: MirrorTimerType,
        operation: MirrorTimerOperation,
        outcome: MirrorTimerOutcome,
        timers: &[MirrorTimer; 4],
    ) -> Self {
        Self {
            version: VERSION,
            policy: MIRROR_TIMER_POLICY.to_owned(),
            world: machine.world,
            player: machine.player,
            sequence,
            timer_type,
            operation,
            outcome,
            before: machine.state_digest(),
            after: machine.digest_state(sequence, timers),
            previous: machine.tip,
        }
    }
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-mirror-timer-record-v1\0", self)
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
    pub fn timer_type(&self) -> MirrorTimerType {
        self.timer_type
    }
    pub fn operation(&self) -> &MirrorTimerOperation {
        &self.operation
    }
    pub fn outcome(&self) -> MirrorTimerOutcome {
        self.outcome
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MirrorTimerCheckpoint {
    version: u32,
    policy: String,
    world: crate::WorldId,
    player: crate::PlayerId,
    sequence: u64,
    timers: [MirrorTimerSnapshot; 4],
    state_digest: crate::Digest,
    history_tip: crate::Digest,
    checksum: crate::Digest,
}
impl MirrorTimerCheckpoint {
    fn checksum(&self) -> crate::Digest {
        let mut body = self.clone();
        body.checksum = [0; 32];
        crate::digest(b"benilla-mirror-timer-checkpoint-v1\0", &body)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, MirrorTimerError> {
        let bytes = serde_json::to_vec(self).map_err(|_| MirrorTimerError::InvalidCheckpoint)?;
        if bytes.len() > MAX_MIRROR_TIMER_BYTES {
            return Err(MirrorTimerError::TooLarge);
        }
        Ok(bytes)
    }
}
impl MirrorTimerMachine {
    fn digest_state(&self, sequence: u64, timers: &[MirrorTimer; 4]) -> crate::Digest {
        crate::digest(
            b"benilla-mirror-timer-state-v1\0",
            &(
                MIRROR_TIMER_POLICY,
                self.world,
                self.player,
                sequence,
                timers.each_ref().map(MirrorTimer::snapshot),
            ),
        )
    }
    pub fn state_digest(&self) -> crate::Digest {
        self.digest_state(self.sequence, &self.timers)
    }
    pub fn checkpoint(&self) -> MirrorTimerCheckpoint {
        let mut checkpoint = MirrorTimerCheckpoint {
            version: VERSION,
            policy: MIRROR_TIMER_POLICY.to_owned(),
            world: self.world,
            player: self.player,
            sequence: self.sequence,
            timers: self.timers.each_ref().map(MirrorTimer::snapshot),
            state_digest: self.state_digest(),
            history_tip: self.tip,
            checksum: [0; 32],
        };
        checkpoint.checksum = checkpoint.checksum();
        checkpoint
    }
    /// Select world/player independently. Hashes detect corruption, not replacement
    /// by someone who can rewrite/re-hash the complete checkpoint.
    pub fn restore(
        world: crate::WorldId,
        player: crate::PlayerId,
        bytes: &[u8],
    ) -> Result<Self, MirrorTimerError> {
        if bytes.len() > MAX_MIRROR_TIMER_BYTES {
            return Err(MirrorTimerError::TooLarge);
        }
        let checkpoint: MirrorTimerCheckpoint =
            serde_json::from_slice(bytes).map_err(|_| MirrorTimerError::InvalidCheckpoint)?;
        if checkpoint.to_bytes()? != bytes
            || checkpoint.version != VERSION
            || checkpoint.policy != MIRROR_TIMER_POLICY
            || checkpoint.world != world
            || checkpoint.player != player
            || checkpoint.checksum != checkpoint.checksum()
            || (checkpoint.sequence == 0) != (checkpoint.history_tip == [0; 32])
        {
            return Err(MirrorTimerError::InvalidCheckpoint);
        }
        let mut machine = Self::new(world, player)?;
        for (timer_type, snapshot) in MirrorTimerType::ALL.into_iter().zip(checkpoint.timers) {
            machine.timers[timer_type as usize] = MirrorTimer::restore(snapshot, timer_type)?;
        }
        machine.sequence = checkpoint.sequence;
        machine.tip = checkpoint.history_tip;
        if machine.state_digest() != checkpoint.state_digest
            || (machine.sequence == 0
                && machine.timers != MirrorTimerType::ALL.map(MirrorTimer::new))
        {
            return Err(MirrorTimerError::InvalidCheckpoint);
        }
        Ok(machine)
    }
    /// Caller must durably retain this exact checkpoint before discarding history.
    /// This component does no filesystem I/O and cannot certify that retention.
    pub fn discard_records(
        &mut self,
        retained: &MirrorTimerCheckpoint,
    ) -> Result<(), MirrorTimerError> {
        if *retained != self.checkpoint() {
            return Err(MirrorTimerError::InvalidCheckpoint);
        }
        self.records.clear();
        self.journal_bytes = 2;
        Ok(())
    }
    pub fn records_to_bytes(&self) -> Result<Vec<u8>, MirrorTimerError> {
        let bytes =
            serde_json::to_vec(&self.records).map_err(|_| MirrorTimerError::InvalidCheckpoint)?;
        if bytes.len() > MAX_MIRROR_TIMER_BYTES {
            return Err(MirrorTimerError::TooLarge);
        }
        Ok(bytes)
    }
    /// Re-execute and compare every operation/result/hash on a fully owned staged
    /// machine. Any changed tail, unsupported operation or capacity failure rolls
    /// back the entire batch, including pending status consumption and pulse state.
    pub fn replay_bytes(&mut self, bytes: &[u8]) -> Result<(), MirrorTimerError> {
        if bytes.len() > MAX_MIRROR_TIMER_BYTES {
            return Err(MirrorTimerError::TooLarge);
        }
        let records: Vec<MirrorTimerRecord> =
            serde_json::from_slice(bytes).map_err(|_| MirrorTimerError::InvalidCheckpoint)?;
        if records.len() > MAX_MIRROR_TIMER_RECORDS
            || serde_json::to_vec(&records).map_err(|_| MirrorTimerError::InvalidCheckpoint)?
                != bytes
        {
            return Err(MirrorTimerError::InvalidCheckpoint);
        }
        let mut staged = self.clone();
        for expected in records {
            if expected.version != VERSION
                || expected.policy != MIRROR_TIMER_POLICY
                || expected.world != staged.world
                || expected.player != staged.player
                || expected.sequence
                    != staged
                        .sequence
                        .checked_add(1)
                        .ok_or(MirrorTimerError::Limit)?
                || expected.before != staged.state_digest()
                || expected.previous != staged.tip
            {
                return Err(MirrorTimerError::ReplayMismatch);
            }
            staged.apply(expected.timer_type, expected.operation.clone())?;
            if staged.records.last() != Some(&expected) {
                return Err(MirrorTimerError::ReplayMismatch);
            }
        }
        *self = staged;
        Ok(())
    }
}
