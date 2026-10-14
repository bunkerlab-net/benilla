// SPDX-License-Identifier: GPL-3.0-or-later
//! Source motion emits launch/interrupt packets, not replacement paths for every
//! Unit::UpdateSplinePosition relocation or SetOrientation mutation.
use super::*;
use benilla_sim::creature_motion as source;
#[path = "native_motion_path.rs"]
mod path;

/// Rejoining a live source spline retains its original path clock instead of
/// restarting at the saved map pose or waiting for a future replan.
/// The existing presentation type carries total duration, not source per-segment
/// timestamps or final facing. Its interpolation never supplies authority facts.
pub(super) fn create_spline(npc: &n::Npc) -> Result<Option<p::CreateSpline>, Error> {
    let Some(source) = &npc.source_motion else {
        return Ok(None);
    };
    let Some(active) = source.motion.snapshot().spline else {
        return Ok(None);
    };
    if active.spline.finalized {
        return Ok(None);
    }
    let duration_ms = *active
        .spline
        .timestamps_ms
        .last()
        .ok_or(Error::Unrepresentable)?;
    Ok(Some(p::CreateSpline {
        path: active.spline.points.into_iter().map(point).collect(),
        id: u32::try_from(active.spline_id).map_err(|_| Error::Unrepresentable)?,
        time_passed_ms: active.spline.elapsed_ms,
        duration_ms,
        flying: false,
        cyclic: false,
        run_mode: !active.walking,
    }))
}

/// Source lifecycle packets follow the journal, preserving motion emitted before
/// same-tick death/removal rather than consulting only final alive/visible state.
pub(super) struct Frame {
    represented: BTreeMap<NpcId, u64>,
    previous_guids: BTreeMap<u64, NpcId>,
}
impl Frame {
    pub(super) fn new(
        represented: &BTreeMap<NpcId, u64>,
        previous: &n::State,
        world: &n::NativeWorld,
    ) -> Result<Self, Error> {
        let mut previous_guids = BTreeMap::new();
        for npc in previous.npcs.values() {
            if previous_guids
                .insert(npc_guid(world, npc.id)?, npc.id)
                .is_some()
            {
                return Err(Error::GuidCollision);
            }
        }
        Ok(Self {
            represented: represented.clone(),
            previous_guids,
        })
    }
    pub(super) fn represented(&self) -> &BTreeMap<NpcId, u64> {
        &self.represented
    }
    pub(super) fn guid(&self, id: &NpcId) -> Result<u64, Error> {
        self.represented
            .get(id)
            .copied()
            .ok_or(Error::StaleProjection)
    }
    pub(super) fn remove(&mut self, id: NpcId) -> Result<SessionEvent, Error> {
        Ok(SessionEvent::ObjectDestroyed(
            self.represented.remove(&id).ok_or(Error::StaleProjection)?,
        ))
    }
    pub(super) fn spawn(&mut self, id: NpcId, guid: u64) -> Result<(), Error> {
        let previous = self
            .previous_guids
            .values()
            .filter(|old| old.spawn == id.spawn)
            .max_by_key(|old| old.generation)
            .ok_or(Error::StaleProjection)?;
        if previous.generation.checked_add(1) != Some(id.generation)
            || self.represented.keys().any(|old| old.spawn == id.spawn)
        {
            return Err(Error::StaleProjection);
        }
        if self.previous_guids.get(&guid).is_some_and(|old| *old != id)
            || self.represented.values().any(|old| *old == guid)
        {
            return Err(Error::GuidCollision);
        }
        self.represented.insert(id, guid);
        self.previous_guids.insert(guid, id);
        Ok(())
    }
    pub(super) fn finish(&self, next: &BTreeMap<NpcId, u64>) -> Result<(), Error> {
        if &self.represented != next {
            return Err(Error::StaleProjection);
        }
        Ok(())
    }
}

pub(super) fn project(
    id: NpcId,
    event: &source::Event,
    player_guid: u64,
    represented: &BTreeMap<NpcId, u64>,
) -> Result<Option<SessionEvent>, Error> {
    let guid = *represented.get(&id).ok_or(Error::StaleProjection)?;
    match event {
        source::Event::SplineLaunched {
            path,
            spline_id,
            duration_ms,
            facing,
            ..
        } => {
            let start = point(*path.first().ok_or(Error::Unrepresentable)?);
            let wire_id = u32::try_from(*spline_id).map_err(|_| Error::Unrepresentable)?;
            let facing = match facing {
                source::Facing::Tangent => p::MonsterMoveFacing::None,
                source::Facing::Angle(angle) => p::MonsterMoveFacing::Angle(angle.as_f32()),
                source::Facing::Target(n::SOLO_UNIT) => p::MonsterMoveFacing::Target(player_guid),
                source::Facing::Target(target) => p::MonsterMoveFacing::Target(
                    represented
                        .iter()
                        .find_map(|(id, guid)| (id.spawn == *target).then_some(*guid))
                        .ok_or(Error::UnknownTarget)?,
                ),
            };
            Ok(Some(SessionEvent::MonsterMove {
                guid,
                transport: None,
                start,
                spline_id: wire_id,
                path: path::linear_path(path)?,
                facing,
                stop: false,
                duration_ms: *duration_ms,
                flying: false,
                // WriteCommonMonsterMovePart always adds Runmode, even for a
                // walking source spline; WriteCreate preserves the actual bit.
                run_mode: true,
            }))
        }
        source::Event::SplineInterrupted { pose, spline_id } => {
            Ok(Some(SessionEvent::MonsterMove {
                guid,
                transport: None,
                start: point(pose.position),
                spline_id: u32::try_from(*spline_id).map_err(|_| Error::Unrepresentable)?,
                path: vec![],
                facing: p::MonsterMoveFacing::None,
                stop: true,
                duration_ms: 0,
                flying: false,
                run_mode: true,
            }))
        }
        // Unit::SetInFront/SetOrientation mutates authoritative orientation
        // without a relocation packet or facing spline, including while moving.
        // Creation reads the current pose; a real launch retains its own facing.
        source::Event::FacingChanged { .. }
        | source::Event::Relocated { .. }
        | source::Event::SplineAdvanced { .. }
        | source::Event::ChaseReached
        | source::Event::ChaseUnreachable
        | source::Event::HomeStarted { .. }
        | source::Event::HomeReached { .. } => Ok(None),
    }
}

#[cfg(test)]
#[path = "native_motion_tests.rs"]
mod tests;
