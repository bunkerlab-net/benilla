// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 TargetedMovementGenerator.cpp:1210-1347, WaypointMovementGenerator.cpp:617-646.
use super::*;
use benilla_sim::source_formations::motion::{self, FollowPath, PathError, PathOwner};

/// A live F5 spline owner also binds every F6 path/collision call to these actual Units.
pub trait FormationPathOwner: PathOwner<Error = Refusal> {
    fn formation_path_units(&self) -> (ObjectHandle, ObjectHandle);
}
impl FormationMotion {
    pub fn build_path(
        self,
        follower: ObjectHandle,
        formations: &SourceFormations,
        mirroring: bool,
        last_angle: &mut u32,
        owner: &mut impl FormationPathOwner,
    ) -> Result<FollowPath> {
        let Self::Follow { slot, master } = self else {
            return Err(refuse("execution.formation.not_follow_motion"));
        };
        require(
            formations.binding(follower) == Some(slot)
                && formations.master(slot.group)? == Some(master)
                && owner.formation_path_units() == (follower, master),
            "execution.formation.follow_path_binding",
        )?;
        let offsets = formations.slot_offset(slot, mirroring)?;
        motion::build_path(
            owner,
            offsets,
            last_angle,
            formations.state(slot.group)?.shape == 0,
            formations.can_use_mmap(slot.group)?,
        )
        .map_err(|error| match error {
            PathError::Owner(error) => error,
            PathError::Numeric => refuse("execution.formation.path_numeric_domain"),
            PathError::Undefined(reason) => {
                refuse(&format!("execution.formation.{reason}_undefined"))
            }
        })
    }
}
impl SourceFormations {
    /// Called by the real linear waypoint transition before the next movement launch.
    pub fn linear_waypoint_next<T>(
        &mut self,
        unit: ObjectHandle,
        path: &BTreeMap<u32, T>,
        current: &mut u32,
        returning: &mut bool,
    ) -> Result<()> {
        use std::ops::Bound::{Excluded, Unbounded};
        require(
            path.contains_key(current),
            "execution.formation.waypoint_iterator",
        )?;
        if !*returning {
            if let Some((&next, _)) = path.range((Excluded(*current), Unbounded)).next() {
                *current = next;
            } else {
                // --begin is undefined before the mirror callback; WaypointMovementGenerator.cpp:624.
                *current = *path
                    .range(..*current)
                    .next_back()
                    .map(|(key, _)| key)
                    .ok_or_else(|| {
                        refuse("execution.formation.waypoint_previous_begin_undefined")
                    })?;
                *returning = true;
                if let Some(slot) = self.binding(unit) {
                    self.state_mut(slot.group)?.mirror = true;
                }
            }
        } else if path.first_key_value().map(|(key, _)| key) == Some(current) {
            // ++begin may become end, then the caller dereferences it; WaypointMovementGenerator.cpp:639.
            *returning = false;
            let next = path
                .range((Excluded(*current), Unbounded))
                .next()
                .map(|(key, _)| *key);
            if let Some(slot) = self.binding(unit) {
                self.state_mut(slot.group)?.mirror = false;
            }
            *current =
                next.ok_or_else(|| refuse("execution.formation.waypoint_next_end_undefined"))?;
        } else {
            *current = *path.range(..*current).next_back().unwrap().0;
        }
        Ok(())
    }
}
