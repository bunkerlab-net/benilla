// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded actual-byte VMapManager2::getAreaInfo and its independent output writes.
//! No TerrainInfo terrain occlusion, area DBC resolution or gameplay admission.
use super::super::VmapLiquidProducts;
use super::{to_internal, Error, QueryBudget, SnapshotBinding, Vector, VmapSnapshot};
use sha2::{Digest, Sha256};
use std::time::Instant;

const POLICY: &[u8] = b"cmangos-8ec338a1-static-vmap-area-v1-independent-metadata-floor-writes";

/// Raw area selection is bound separately from location, liquid, height and ray queries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AreaRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    /// Exact original world-coordinate binary32 bits.
    pub position: [u32; 3],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AreaResult {
    /// Source returns false and leaves caller z/flags/ADT/root/group untouched.
    None,
    Area {
        /// Highest successful world floor, binary32 bits. Equal floors retain ADT.
        floor: u32,
        adt_id: u16,
        /// Last successful model's selected group, independent of floor/ADT.
        /// IDs preserve all source uint32-to-int32 bits; no DBC interpretation.
        root_id: u32,
        group_id: u32,
        flags: u32,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AreaReply {
    pub request: AreaRequest,
    pub result: AreaResult,
}

#[derive(Debug)]
pub(crate) struct AreaSnapshot {
    binding: SnapshotBinding,
    // Wet and dry geometry stays private; never convert to a conservative reader.
    geometry: VmapSnapshot,
}

impl VmapLiquidProducts {
    pub(crate) fn load_vmap_area(&self) -> Result<AreaSnapshot, Error> {
        let geometry = VmapSnapshot::load_profile(&self.inventory, true)?;
        if geometry.spawns.iter().any(Option::is_none) {
            return Err(Error::Missing);
        }
        let mut hash = Sha256::new();
        hash.update(POLICY);
        hash.update(geometry.binding.inventory);
        hash.update(geometry.binding.policy);
        // The shared local-group/world-floor arithmetic has a separate policy.
        hash.update(geometry.location_binding().policy);
        Ok(AreaSnapshot {
            binding: SnapshotBinding {
                policy: hash.finalize().into(),
                ..geometry.binding
            },
            geometry,
        })
    }
}

impl AreaSnapshot {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }

    pub(crate) fn query(
        &self,
        request: AreaRequest,
        deadline: Instant,
    ) -> Result<AreaReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let mut budget = QueryBudget::new(deadline);
        let result = self.area(request.position.map(f32::from_bits), &mut budget)?;
        budget.complete()?;
        Ok(AreaReply { request, result })
    }

    /// The composite area reader shares this exact traversal and cumulative budget;
    /// the wet geometry itself remains private to this reader.
    pub(in super::super) fn area(
        &self,
        position: Vector,
        budget: &mut QueryBudget,
    ) -> Result<AreaResult, Error> {
        if position.iter().any(|value| !value.is_finite()) {
            return Err(Error::Format);
        }
        if position.iter().any(|value| value.abs() > 100_000.0) {
            return Err(Error::Unsupported);
        }
        budget.take()?;
        // Even an outside-map miss requires every declared static-tree slot loaded.
        for spawn in &self.geometry.spawns {
            budget.take()?;
            if spawn.is_none() {
                return Err(Error::Missing);
            }
        }
        let position = to_internal(position);
        let mut floor = f32::NEG_INFINITY; // AreaInfo constructor, MapTree.h.
        let mut adt_id = 0;
        let mut metadata = None;
        self.geometry
            .tree
            .intersect_point(position, budget, |id, budget| {
                let spawn = self
                    .geometry
                    .spawns
                    .get(id as usize)
                    .and_then(Option::as_ref)
                    .ok_or(Error::Missing)?;
                if let Some((candidate, _)) = spawn.location_candidate(
                    &self.geometry.models[spawn.model],
                    position,
                    budget,
                )? {
                    // WorldModel::IntersectPoint writes these on EVERY success.
                    // A later lower/equal floor may therefore replace metadata only.
                    metadata = Some((candidate.root_id, candidate.group_id, candidate.group_flags));
                    let world_z = f32::from_bits(candidate.floor);
                    // ModelInstance::intersectPoint updates only floor and ADT here.
                    if floor < world_z {
                        floor = world_z;
                        adt_id = candidate.adt_id;
                    }
                }
                Ok(())
            })?;
        budget.complete()?;
        Ok(match metadata {
            Some((root_id, group_id, flags)) => AreaResult::Area {
                floor: floor.to_bits(),
                adt_id,
                root_id,
                group_id,
                flags,
            },
            None => AreaResult::None,
        })
    }
}

#[cfg(test)]
mod tests;
