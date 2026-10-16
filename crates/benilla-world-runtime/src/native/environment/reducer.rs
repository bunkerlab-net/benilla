// SPDX-License-Identifier: GPL-3.0-or-later
//! Journal-free owned environment reduction shared by the standalone component
//! and crate-private player ownership. Source behavior stays in EnvironmentState's
//! private refresh/set_flags methods; this layer owns binding and atomic staging.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) enum EnvironmentPhase {
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
pub(crate) struct EnvironmentStateSnapshot {
    pub(super) flags: u8,
    pub(super) timer_ms: u32,
    #[serde(deserialize_with = "super::super::creature_collision_width::required_option")]
    pub(super) last_liquid: Option<u32>,
    #[serde(deserialize_with = "super::super::creature_collision_width::required_option")]
    pub(super) last_position: Option<EnvironmentPose>,
    #[serde(deserialize_with = "super::super::creature_collision_width::required_option")]
    pub(super) cached_query: Option<EnvironmentQuery>,
    pub(super) mirrors: [mirror::MirrorTimerSnapshot; 4],
}
impl From<&EnvironmentState> for EnvironmentStateSnapshot {
    fn from(s: &EnvironmentState) -> Self {
        Self {
            flags: s.flags,
            timer_ms: s.timer_ms,
            last_liquid: s.last_liquid,
            last_position: s.last_position,
            cached_query: s.cached_query,
            mirrors: s.mirrors.each_ref().map(MirrorTimer::snapshot),
        }
    }
}
impl EnvironmentStateSnapshot {
    pub(crate) fn restore(
        self,
        snapshot: EnvironmentSnapshotBinding,
        catalog: &LiquidCatalog,
        sequence: u64,
    ) -> Result<EnvironmentState, EnvironmentError> {
        snapshot.validate(catalog)?;
        // Only owned mirror-scale effects execute. Wet/hazard/aura/threat branches
        // remain unavailable; no started/ticking mirror timers can be imported here.
        if ![0, LIQUID, LIQUID | HIGH_SEA].contains(&self.flags)
            || self.timer_ms > REFRESH_MS
            || self
                .last_liquid
                .is_some_and(|id| id == 0 || catalog.get(id).is_none_or(|r| r.spell_id != 0))
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
                if q.snapshot == snapshot
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
                if (self.flags == 0 && self.last_liquid.is_some())
                    || (q.ticket == sequence
                        && (self.timer_ms != REFRESH_MS
                            || self.last_position
                                != Some(EnvironmentPose {
                                    position: q.position,
                                    collision_height: q.collision_height,
                                })))
                {
                    return Err(EnvironmentError::InvalidCheckpoint);
                }
            }
            _ => return Err(EnvironmentError::InvalidCheckpoint),
        }
        let mut mirrors = initial_mirrors();
        if self.flags == LIQUID {
            let mut regenerated = initial_mirrors();
            regenerated[0].set_scale(10);
            if self.mirrors == regenerated.each_ref().map(MirrorTimer::snapshot) {
                if self.cached_query.is_none_or(|q| q.ticket < 3) {
                    return Err(EnvironmentError::InvalidCheckpoint);
                }
                mirrors = regenerated;
            }
        }
        if self.mirrors != mirrors.each_ref().map(MirrorTimer::snapshot) {
            return Err(EnvironmentError::InvalidCheckpoint);
        }
        Ok(EnvironmentState {
            mirrors,
            flags: self.flags,
            timer_ms: self.timer_ms,
            last_liquid: self.last_liquid,
            last_position: self.last_position,
            cached_query: self.cached_query,
        })
    }
}

impl EnvironmentState {
    /// Genuine Player constructor state, before any terrain query has occurred.
    pub(crate) fn new(
        snapshot: EnvironmentSnapshotBinding,
        catalog: &LiquidCatalog,
    ) -> Result<Self, EnvironmentError> {
        snapshot.validate(catalog)?;
        Ok(Self {
            flags: 0,
            timer_ms: 0,
            last_liquid: None,
            last_position: None,
            cached_query: None,
            mirrors: initial_mirrors(),
        })
    }

    pub(crate) fn snapshot(&self) -> EnvironmentStateSnapshot {
        self.into()
    }

    /// Use the same immutable binding/catalog and monotone sequence selected by the owner.
    pub(crate) fn query_for_position(
        &self,
        snapshot: EnvironmentSnapshotBinding,
        sequence: u64,
        pose: EnvironmentPose,
    ) -> Result<Option<EnvironmentQuery>, EnvironmentError> {
        pose.validate()?;
        if self.timer_ms != 0 {
            return Ok(None);
        }
        Ok(Some(EnvironmentQuery {
            snapshot,
            ticket: sequence.checked_add(1).ok_or(EnvironmentError::Limit)?,
            position: pose.position,
            required_type: ALL_LIQUIDS,
            collision_height: pose.collision_height,
        }))
    }

    /// Every accepted position (including cached calls) and update consumes one
    /// monotone phase number. The caller commits state and sequence together.
    /// A failure never mutates either input and has no external callback effects.
    pub(crate) fn reduce(
        &self,
        snapshot: EnvironmentSnapshotBinding,
        catalog: &LiquidCatalog,
        sequence: u64,
        phase: &EnvironmentPhase,
    ) -> Result<(Self, u64), EnvironmentError> {
        let next_sequence = sequence.checked_add(1).ok_or(EnvironmentError::Limit)?;
        let mut next = self.clone();
        match phase {
            EnvironmentPhase::Update { diff_ms } => {
                next.update_timer(*diff_ms);
            }
            EnvironmentPhase::Position { pose, result } => {
                let pose = *pose;
                match (
                    self.query_for_position(snapshot, sequence, pose)?,
                    result.as_deref().copied(),
                ) {
                    (None, None) => {}
                    (Some(expected), Some(reply)) => {
                        reply.validate(expected)?;
                        let mut mirrors = next.mirrors.clone();
                        next.refresh_if_due(
                            reply,
                            catalog,
                            &mut OwnedMirrorCallbacks {
                                timers: &mut mirrors,
                            },
                        )?;
                        next.mirrors = mirrors;
                    }
                    _ => return Err(EnvironmentError::Binding),
                }
                next.last_position = Some(pose);
            }
        }
        Ok((next, next_sequence))
    }

    /// NativeWorld has no player mirror activation/expiration lifecycle yet.
    /// This is a narrower executor boundary, not a source numeric restriction.
    pub(crate) fn validate_native_lifecycle(
        &self,
        catalog: &LiquidCatalog,
    ) -> Result<(), EnvironmentError> {
        if ![0, LIQUID].contains(&self.flags)
            || self.mirrors != initial_mirrors()
            || self
                .last_liquid
                .is_some_and(|id| id == 0 || catalog.get(id).is_none_or(|row| row.spell_id != 0))
        {
            return Err(EnvironmentError::InvalidInput);
        }
        Ok(())
    }
}

impl EnvironmentPhase {
    /// Player::UpdateMirrorTimers (Player.cpp:1213–1300, called at 1409)
    /// owns additional high-sea, breath, hazard and spell lifecycles. The native
    /// owner must refuse those inputs until their actual lifecycles are present.
    pub(crate) fn validate_native_lifecycle(
        &self,
        catalog: &LiquidCatalog,
    ) -> Result<(), EnvironmentError> {
        if let Self::Position {
            result: Some(result),
            ..
        } = self
        {
            if matches!(
                result.status,
                LiquidStatus::UnderWater | LiquidStatus::InWater
            ) || result.data.is_some_and(|data| {
                data.type_flags & (0x01 | 0x04 | 0x10) != 0
                    || (data.entry != 0
                        && catalog.get(data.entry).is_some_and(|row| row.spell_id != 0))
            }) {
                return Err(EnvironmentError::InvalidInput);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
