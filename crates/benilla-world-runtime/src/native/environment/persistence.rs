// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent component checkpoint/journal. These are not NativeWorld save files,
//! admission receipts, authenticated histories, or proof that external effects ran.
use super::*;
const VERSION: u32 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) enum EnvironmentPhase {
    Update {
        diff_ms: u32,
    },
    Position {
        pose: EnvironmentPose,
        result: Option<Box<BoundLiquidResult>>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct StoredState {
    flags: u8,
    timer_ms: u32,
    last_liquid: Option<u32>,
    last_position: Option<EnvironmentPose>,
    cached_query: Option<EnvironmentQuery>,
}
impl From<&EnvironmentState> for StoredState {
    fn from(s: &EnvironmentState) -> Self {
        Self {
            flags: s.flags,
            timer_ms: s.timer_ms,
            last_liquid: s.last_liquid,
            last_position: s.last_position,
            cached_query: s.cached_query,
        }
    }
}
impl StoredState {
    fn restore(
        self,
        machine: &EnvironmentMachine,
        sequence: u64,
    ) -> Result<EnvironmentState, EnvironmentError> {
        // No production callback executor exists: only NONE/LIQUID flags and
        // zero-spell remembered rows can arise on this component's committed path.
        if !matches!(self.flags, 0 | LIQUID)
            || self.timer_ms > REFRESH_MS
            || self.last_liquid.is_some_and(|id| {
                id == 0 || machine.catalog.get(id).is_none_or(|r| r.spell_id != 0)
            })
        {
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
                if q.snapshot == machine.snapshot
                    && q.ticket != 0
                    && q.ticket <= sequence
                    && q.required_type == ALL_LIQUIDS
                    && self.last_position.is_some() =>
            {
                EnvironmentPose {
                    position: q.position,
                    collision_height: q.collision_height,
                }
                .validate()
                .map_err(|_| EnvironmentError::InvalidCheckpoint)?;
                if self.flags == 0 && self.last_liquid.is_some() {
                    return Err(EnvironmentError::InvalidCheckpoint);
                }
            }
            _ => return Err(EnvironmentError::InvalidCheckpoint),
        }
        Ok(EnvironmentState {
            flags: self.flags,
            timer_ms: self.timer_ms,
            last_liquid: self.last_liquid,
            last_position: self.last_position,
            cached_query: self.cached_query,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentRecord {
    version: u32,
    policy: String,
    world: crate::WorldId,
    player: crate::PlayerId,
    snapshot: EnvironmentSnapshotBinding,
    sequence: u64,
    phase: EnvironmentPhase,
    before: crate::Digest,
    after: crate::Digest,
    previous: crate::Digest,
}
impl EnvironmentRecord {
    pub(super) fn new(
        machine: &EnvironmentMachine,
        sequence: u64,
        phase: EnvironmentPhase,
        before: crate::Digest,
        next: &EnvironmentState,
    ) -> Self {
        Self {
            version: VERSION,
            policy: ENVIRONMENT_POLICY.to_owned(),
            world: machine.world,
            player: machine.player,
            snapshot: machine.snapshot,
            sequence,
            phase,
            before,
            after: machine.digest_state(sequence, next),
            previous: machine.tip,
        }
    }
    pub fn digest(&self) -> crate::Digest {
        crate::digest(b"benilla-environment-record-v1\0", self)
    }
    pub fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EnvironmentCheckpoint {
    version: u32,
    policy: String,
    world: crate::WorldId,
    player: crate::PlayerId,
    snapshot: EnvironmentSnapshotBinding,
    sequence: u64,
    state: StoredState,
    state_digest: crate::Digest,
    history_tip: crate::Digest,
    checksum: crate::Digest,
}
impl EnvironmentCheckpoint {
    fn checksum(&self) -> crate::Digest {
        let mut body = self.clone();
        body.checksum = [0; 32];
        crate::digest(b"benilla-environment-checkpoint-v1\0", &body)
    }
    pub fn to_bytes(&self) -> Result<Vec<u8>, EnvironmentError> {
        let bytes = serde_json::to_vec(self).map_err(|_| EnvironmentError::InvalidCheckpoint)?;
        if bytes.len() > MAX_ENVIRONMENT_BYTES {
            return Err(EnvironmentError::TooLarge);
        }
        Ok(bytes)
    }
}
impl EnvironmentMachine {
    fn digest_state(&self, sequence: u64, state: &EnvironmentState) -> crate::Digest {
        crate::digest(
            b"benilla-environment-state-v1\0",
            &(
                ENVIRONMENT_POLICY,
                self.world,
                self.player,
                self.snapshot,
                sequence,
                StoredState::from(state),
            ),
        )
    }
    pub fn state_digest(&self) -> crate::Digest {
        self.digest_state(self.sequence, &self.state)
    }
    pub fn checkpoint(&self) -> EnvironmentCheckpoint {
        let mut checkpoint = EnvironmentCheckpoint {
            version: VERSION,
            policy: ENVIRONMENT_POLICY.to_owned(),
            world: self.world,
            player: self.player,
            snapshot: self.snapshot,
            sequence: self.sequence,
            state: StoredState::from(&self.state),
            state_digest: self.state_digest(),
            history_tip: self.tip,
            checksum: [0; 32],
        };
        checkpoint.checksum = checkpoint.checksum();
        checkpoint
    }
    /// Structural/hash checks against independently selected identity and catalog.
    /// Hashes detect corruption, not an adversary able to replace/re-hash the whole file.
    /// Trusted storage or a separately pinned state digest is still required.
    pub fn restore(
        snapshot: EnvironmentSnapshotBinding,
        world: crate::WorldId,
        player: crate::PlayerId,
        catalog: LiquidCatalog,
        bytes: &[u8],
    ) -> Result<Self, EnvironmentError> {
        if bytes.len() > MAX_ENVIRONMENT_BYTES {
            return Err(EnvironmentError::TooLarge);
        }
        let checkpoint: EnvironmentCheckpoint =
            serde_json::from_slice(bytes).map_err(|_| EnvironmentError::InvalidCheckpoint)?;
        if checkpoint.to_bytes()? != bytes
            || checkpoint.version != VERSION
            || checkpoint.policy != ENVIRONMENT_POLICY
            || checkpoint.checksum != checkpoint.checksum()
            || checkpoint.snapshot != snapshot
            || checkpoint.world != world
            || checkpoint.player != player
            || (checkpoint.sequence == 0) != (checkpoint.history_tip == [0; 32])
        {
            return Err(EnvironmentError::InvalidCheckpoint);
        }
        let mut machine = Self::new(snapshot, world, player, catalog)?;
        machine.state = checkpoint.state.restore(&machine, checkpoint.sequence)?;
        machine.sequence = checkpoint.sequence;
        machine.tip = checkpoint.history_tip;
        if machine.state_digest() != checkpoint.state_digest {
            return Err(EnvironmentError::InvalidCheckpoint);
        }
        Ok(machine)
    }
    /// Call only after durably retaining the independently created checkpoint.
    /// Exact current checkpoint equality prevents discarding a different suffix.
    /// The caller owns durable I/O; this component cannot verify a filesystem write.
    pub fn discard_records(
        &mut self,
        retained: &EnvironmentCheckpoint,
    ) -> Result<(), EnvironmentError> {
        if *retained != self.checkpoint() {
            return Err(EnvironmentError::InvalidCheckpoint);
        }
        self.records.clear();
        Ok(())
    }
    pub fn records_to_bytes(&self) -> Result<Vec<u8>, EnvironmentError> {
        let bytes =
            serde_json::to_vec(&self.records).map_err(|_| EnvironmentError::InvalidCheckpoint)?;
        if bytes.len() > MAX_ENVIRONMENT_BYTES {
            return Err(EnvironmentError::TooLarge);
        }
        Ok(bytes)
    }
    /// Re-executes each source phase with the same closed production callback boundary.
    /// Whole-batch transactional: malformed/changed/unsupported records commit nothing.
    pub fn replay_bytes(&mut self, bytes: &[u8]) -> Result<(), EnvironmentError> {
        if bytes.len() > MAX_ENVIRONMENT_BYTES {
            return Err(EnvironmentError::TooLarge);
        }
        let records: Vec<EnvironmentRecord> =
            serde_json::from_slice(bytes).map_err(|_| EnvironmentError::InvalidCheckpoint)?;
        if records.len() > MAX_ENVIRONMENT_RECORDS
            || serde_json::to_vec(&records).map_err(|_| EnvironmentError::InvalidCheckpoint)?
                != bytes
        {
            return Err(EnvironmentError::InvalidCheckpoint);
        }
        let mut staged = self.clone();
        for expected in records {
            if expected.version != VERSION
                || expected.policy != ENVIRONMENT_POLICY
                || expected.world != staged.world
                || expected.player != staged.player
                || expected.snapshot != staged.snapshot
                || expected.sequence
                    != staged
                        .sequence
                        .checked_add(1)
                        .ok_or(EnvironmentError::Limit)?
                || expected.before != staged.state_digest()
                || expected.previous != staged.tip
            {
                return Err(EnvironmentError::ReplayMismatch);
            }
            staged.transact(expected.phase.clone())?;
            if staged.records.last() != Some(&expected) {
                return Err(EnvironmentError::ReplayMismatch);
            }
        }
        *self = staged;
        Ok(())
    }
}
