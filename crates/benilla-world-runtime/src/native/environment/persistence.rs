// SPDX-License-Identifier: GPL-3.0-or-later
//! Independent component checkpoint/journal. These are not NativeWorld save files,
//! admission receipts, authenticated histories, or proof that external effects ran.
use super::*;
const VERSION: u32 = 2;

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
        crate::digest(b"benilla-environment-record-v2\0", self)
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
    state: EnvironmentStateSnapshot,
    state_digest: crate::Digest,
    history_tip: crate::Digest,
    checksum: crate::Digest,
}
impl EnvironmentCheckpoint {
    fn checksum(&self) -> crate::Digest {
        let mut body = self.clone();
        body.checksum = [0; 32];
        crate::digest(b"benilla-environment-checkpoint-v2\0", &body)
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
            b"benilla-environment-state-v2\0",
            &(
                ENVIRONMENT_POLICY,
                self.world,
                self.player,
                self.snapshot,
                sequence,
                state.snapshot(),
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
            state: self.state.snapshot(),
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
        machine.state =
            checkpoint
                .state
                .restore(snapshot, &machine.catalog, checkpoint.sequence)?;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_restore_cannot_import_started_or_unrelated_timer_state() {
        let m = super::super::tests::machine();
        for timer in 0..4 {
            let mut changed = m.clone();
            changed.state.mirrors[timer].start(1000, 7);
            let bytes = changed.checkpoint().to_bytes().unwrap();
            assert!(EnvironmentMachine::restore(
                m.snapshot,
                m.world,
                m.player,
                m.catalog.clone(),
                &bytes
            )
            .is_err());
        }
        let mut changed = m.clone();
        changed.state.mirrors[0].set_scale(10);
        // Scale 10 alone is a valid source timer state, but cannot be the initial
        // environment component state before any HIGH_SEA transition.
        assert!(EnvironmentMachine::restore(
            m.snapshot,
            m.world,
            m.player,
            m.catalog.clone(),
            &changed.checkpoint().to_bytes().unwrap()
        )
        .is_err());
    }

    #[test]
    fn rehashed_latest_phase_pose_or_timer_inconsistency_is_rejected() {
        let mut m = super::super::tests::machine();
        let pose = super::super::tests::pose();
        let sample = super::super::tests::reply(&m, LiquidStatus::NoWater, 0, 0);
        m.set_position_phase(pose, Some(sample)).unwrap();
        for change_pose in [false, true] {
            let mut checkpoint = m.checkpoint();
            if change_pose {
                checkpoint.state.last_position.as_mut().unwrap().position[0] = 1f32.to_bits();
            } else {
                checkpoint.state.timer_ms = 99;
            }
            // Recompute all integrity hashes: rejection must be semantic, not checksum-only.
            checkpoint.state_digest = crate::digest(
                b"benilla-environment-state-v2\0",
                &(
                    ENVIRONMENT_POLICY,
                    m.world,
                    m.player,
                    m.snapshot,
                    m.sequence,
                    &checkpoint.state,
                ),
            );
            checkpoint.checksum = checkpoint.checksum();
            assert!(EnvironmentMachine::restore(
                m.snapshot,
                m.world,
                m.player,
                m.catalog.clone(),
                &checkpoint.to_bytes().unwrap()
            )
            .is_err());
        }
    }
}
