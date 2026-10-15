// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned Map::GetHeight over actual static products and a proven empty dynamic tree.
//! Height is a scalar source answer, including invalid sentinels, never body support.
use super::super::core::NonRenderParticipants;
use super::{height, Error, Hash, InventoriedProducts};
use sha2::{Digest, Sha256};
use std::time::Instant;

pub(super) const POLICY: &[u8] =
    b"cmangos-8ec338a1-map-height-enabled-complete-dry-static-empty-dynamic-v1";

/// Explicit recorded effective configuration, independently supplied by the source
/// owner. Equality binds it to this inventory; it does not authenticate the capture.
/// Missing configuration and the source's default-disabled setting are not enabled.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceHeightConfiguration {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub effective_config: Hash,
    pub vmap_height_enabled: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapHeightRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub position: [u32; 3],
    pub swim: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct MapHeightReply {
    pub request: MapHeightRequest,
    pub height: u32,
}
pub(crate) struct MapHeightSnapshot<'a> {
    binding: SnapshotBinding,
    height: height::StaticHeightSnapshot<'a>,
    participants: NonRenderParticipants,
}
impl InventoriedProducts {
    pub(crate) fn load_map_height(
        &self,
        config: SourceHeightConfiguration,
        participants: Option<&NonRenderParticipants>,
    ) -> Result<MapHeightSnapshot<'_>, Error> {
        if config.map != self.map
            || config.content != self.content
            || config.source_epoch != self.source_epoch
            || config.effective_config != self.effective_config
        {
            return Err(Error::Identity);
        }
        if !config.vmap_height_enabled {
            return Err(Error::Unsupported);
        }
        let participants = participants
            .filter(|proof| proof.matches(self.content, self.map))
            .ok_or(Error::Profile)?;
        let height = self.load_static_height()?;
        let b = height.binding();
        let mut policy = Sha256::new();
        policy.update(POLICY);
        policy.update(b.policy);
        policy.update(config.effective_config);
        policy.update([u8::from(config.vmap_height_enabled)]);
        Ok(MapHeightSnapshot {
            binding: SnapshotBinding {
                map: b.map,
                content: b.content,
                source_epoch: b.source_epoch,
                inventory: b.inventory,
                policy: policy.finalize().into(),
            },
            height,
            participants: participants.clone(),
        })
    }
}
impl MapHeightSnapshot<'_> {
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }
    pub(crate) fn query(
        &self,
        request: MapHeightRequest,
        deadline: Instant,
    ) -> Result<MapHeightReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        if !self
            .participants
            .matches(self.binding.content, self.binding.map)
        {
            return Err(Error::Profile);
        }
        let height = map_height(
            request.position.map(f32::from_bits),
            request.swim,
            |position, use_vmap, search| {
                if !use_vmap {
                    return Err(Error::Unsupported);
                }
                let query = height::HeightRequest {
                    snapshot: self.height.binding(),
                    ticket: request.ticket,
                    position: position.map(f32::to_bits),
                    max_search: search.to_bits(),
                };
                let reply = self.height.query(query, deadline)?;
                if reply.request != query {
                    return Err(Error::Identity);
                }
                Ok(f32::from_bits(reply.height))
            },
            |_, _| {
                // DynamicMapTree::getHeight returns -G3D::inf() after an empty
                // tree traversal. Only the independent participant token permits it.
                if Instant::now() >= deadline {
                    return Err(Error::Budget);
                }
                Ok(f32::NEG_INFINITY)
            },
        )?;
        if Instant::now() >= deadline {
            return Err(Error::Budget);
        }
        Ok(MapHeightReply {
            request,
            height: height.to_bits(),
        })
    }
}

/// Map.cpp:2758-2765. Callbacks are private source-oracle seams, not providers.
fn map_height(
    p: [f32; 3],
    swim: bool,
    mut static_height: impl FnMut([f32; 3], bool, f32) -> Result<f32, Error>,
    mut dynamic_height: impl FnMut([f32; 3], f32) -> Result<f32, Error>,
) -> Result<f32, Error> {
    if p.iter().any(|v| !v.is_finite()) {
        return Err(Error::Format);
    }
    let static_height = static_height(p, true, if swim { 50.0 } else { 10.0 })?;
    if !static_height.is_finite() {
        return Err(Error::Unsupported);
    }
    let selected = if p[2] < static_height {
        static_height
    } else {
        p[2]
    };
    let dyn_start = 2.0f32 + selected;
    let dyn_range = dyn_start - static_height;
    if !dyn_start.is_finite() || !dyn_range.is_finite() {
        return Err(Error::Unsupported);
    }
    let dynamic_height = dynamic_height([p[0], p[1], dyn_start], dyn_range)?;
    // std::max preserves the first operand on equality, unlike f32::max.
    Ok(if static_height < dynamic_height {
        dynamic_height
    } else {
        static_height
    })
}

/// Unit.cpp:11835-11857, the proven non-flying/non-swimming branch only.
pub(crate) fn allowed_ground_z(mut z: f32, ground_z: f32) -> f32 {
    if ground_z > -100_000.0 {
        if z > ground_z {
            return ground_z;
        }
        if z < ground_z {
            z = ground_z;
        }
    }
    z
}

#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
pub(super) mod tests;
