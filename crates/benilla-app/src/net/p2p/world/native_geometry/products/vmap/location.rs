// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded dry StaticMapTree::GetLocationInfo and its selected WMO floor.
//! This is neither a height ray nor an area/liquid-status or gameplay admission proof.
use super::{
    apply, scale, sub, to_internal, triangle, Error, Group, LocationFloor, LocationReply,
    LocationRequest, Model, QueryBudget, Ray, SnapshotBinding, Spawn, StaticLocation, Vector,
    VmapSnapshot,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

const POLICY: &[u8] = b"benilla-dry-static-vmap-location-v1-glibc-2.41-bounded-trig";

pub(super) struct SelectedLocation {
    pub floor: LocationFloor,
    pub spawn: usize,
    pub group: usize,
}

impl VmapSnapshot {
    /// Location selection has its own policy identity; ray/height replies cannot
    /// be substituted even when all inventoried bytes and input position agree.
    pub(crate) fn location_binding(&self) -> SnapshotBinding {
        let mut hash = Sha256::new();
        hash.update(POLICY);
        hash.update(self.binding.inventory);
        SnapshotBinding {
            policy: hash.finalize().into(),
            ..self.binding
        }
    }

    pub(crate) fn query_location(
        &self,
        request: LocationRequest,
        deadline: Instant,
    ) -> Result<LocationReply, Error> {
        if request.snapshot != self.location_binding() || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let mut budget = QueryBudget::new(deadline);
        let result = self.location(request.position.map(f32::from_bits), &mut budget)?;
        budget.complete()?;
        Ok(LocationReply { request, result })
    }

    /// Composite consumers must share one absolute deadline and work allowance.
    /// Only a complete, liquid-free loaded map can establish this scoped result.
    pub(in super::super) fn location(
        &self,
        position: Vector,
        budget: &mut QueryBudget,
    ) -> Result<StaticLocation, Error> {
        Ok(match self.select_location(position, budget)? {
            Some(selected) => StaticLocation::Floor(selected.floor),
            None => StaticLocation::None,
        })
    }

    /// Private selected indices keep liquid lookup tied to the exact winning group.
    /// Wet snapshots do not expose the conservative location or ray APIs.
    pub(super) fn select_location(
        &self,
        position: Vector,
        budget: &mut QueryBudget,
    ) -> Result<Option<SelectedLocation>, Error> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err(Error::Format);
        }
        if position.iter().any(|value| value.abs() > 100_000.0) {
            return Err(Error::Unsupported);
        }
        budget.take()?;
        // Charge every slot, including unrelated ones, before any absence proof.
        for spawn in &self.spawns {
            budget.take()?;
            if spawn.is_none() {
                return Err(Error::Missing);
            }
        }
        let position = to_internal(position);
        let mut floor = f32::NEG_INFINITY; // LocationInfo constructor, MapTree.h.
        let mut result = None;
        self.tree.intersect_point(position, budget, |id, budget| {
            let spawn = self
                .spawns
                .get(id as usize)
                .and_then(Option::as_ref)
                .ok_or(Error::Missing)?;
            if let Some((candidate, group)) =
                spawn.location(&self.models[spawn.model], position, floor, budget)?
            {
                floor = f32::from_bits(candidate.floor);
                result = Some(SelectedLocation {
                    floor: candidate,
                    spawn: id as usize,
                    group,
                });
            }
            Ok(())
        })?;
        budget.complete()?;
        Ok(result)
    }
}

impl Spawn {
    /// ModelInstance::GetLocationInfo: M2 exclusion, point bounds, local group
    /// selection, then strict highest-world-floor replacement across instances.
    fn location(
        &self,
        model: &Model,
        position: Vector,
        floor: f32,
        budget: &mut QueryBudget,
    ) -> Result<Option<(LocationFloor, usize)>, Error> {
        budget.take()?;
        if self.flags & 1 != 0 || !self.bounds.contains(position) {
            return Ok(None);
        }
        let local = scale(
            apply(self.inverse, sub(position, self.position)),
            self.inverse_scale,
        );
        let down = apply(self.inverse, [0.0, 0.0, -1.0]);
        let Some((group, distance)) = model.location(local, down, budget)? else {
            return Ok(None);
        };
        let ground = scale(down, distance);
        let ground: Vector = std::array::from_fn(|axis| local[axis] + ground[axis]);
        // G3D's vec * matrix uses columns of the ORIGINAL inverse matrix. Do not
        // substitute a freshly computed rotation or an inverse-of-inverse here.
        let world_z = (ground[0] * self.inverse[0][2]
            + ground[1] * self.inverse[1][2]
            + ground[2] * self.inverse[2][2])
            * self.scale
            + self.position[2];
        if !world_z.is_finite() {
            return Err(Error::Format);
        }
        if floor < world_z {
            let group_index = group;
            let group = &model.groups[group];
            Ok(Some((
                LocationFloor {
                    floor: world_z.to_bits(),
                    root_id: model.root_id,
                    group_id: group.id,
                    group_flags: group.flags,
                    spawn_id: self.id,
                    adt_id: self.adt,
                },
                group_index,
            )))
        } else {
            Ok(None)
        }
    }
}

impl Model {
    /// WorldModel::GetLocationInfo uses point traversal even for a single group;
    /// equal distances retain the first group visited, with no volume heuristic.
    fn location(
        &self,
        position: Vector,
        down: Vector,
        budget: &mut QueryBudget,
    ) -> Result<Option<(usize, f32)>, Error> {
        budget.take()?;
        if self.groups.is_empty() {
            return Ok(None);
        }
        let mut nearest = f32::INFINITY;
        let mut selected = None;
        self.tree.as_ref().ok_or(Error::Missing)?.intersect_point(
            position,
            budget,
            |id, budget| {
                if let Some(distance) =
                    self.groups[id as usize].inside_distance(position, down, budget)?
                {
                    if distance < nearest {
                        nearest = distance;
                        selected = Some((id as usize, distance));
                    }
                }
                Ok(())
            },
        )?;
        Ok(selected)
    }
}

impl Group {
    /// GroupModel::IsInsideObject offsets upward by 0.1 LOCAL units, searches to
    /// positive infinity along the unnormalized transformed down vector, then
    /// subtracts the offset. The original point must be inside this group bound.
    fn inside_distance(
        &self,
        position: Vector,
        down: Vector,
        budget: &mut QueryBudget,
    ) -> Result<Option<f32>, Error> {
        budget.take()?;
        if self.triangles.is_empty() || !self.bounds.contains(position) {
            return Ok(None);
        }
        let ray = Ray {
            origin: sub(position, scale(down, 0.1)),
            direction: down,
        };
        let mut distance = f32::INFINITY;
        let hit = self
            .tree
            .as_ref()
            .ok_or(Error::Missing)?
            .intersect_with_limit(ray, &mut distance, budget, |id, distance, budget| {
                budget.take()?;
                Ok(triangle(
                    self.triangles[id as usize].map(|index| self.vertices[index as usize]),
                    ray,
                    distance,
                ))
            })?;
        if hit {
            if !distance.is_finite() {
                return Err(Error::Format);
            }
            Ok(Some(distance - 0.1))
        } else {
            Ok(None)
        }
    }
}

#[cfg(test)]
pub(crate) mod tests;
