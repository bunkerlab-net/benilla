// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual spatial facts for the independent environment component, never admission.
use super::{Error, GridLiquidStatus, TerrainLiquidRequest, TerrainLiquidSnapshot};
use benilla_world_runtime::native::environment::{
    BoundLiquidResult, EnvironmentQuery, EnvironmentSnapshotBinding, LiquidCatalog, LiquidData,
    LiquidStatus,
};
use std::time::Instant;

impl TerrainLiquidSnapshot<'_> {
    pub(crate) fn environment_binding(&self) -> EnvironmentSnapshotBinding {
        let source = self.binding();
        EnvironmentSnapshotBinding {
            map: source.map,
            content: source.content,
            source_epoch: source.source_epoch,
            inventory: source.inventory,
            policy: source.policy,
            liquid_catalog: self.types.digest(),
        }
    }

    /// The same immutable byte-derived catalog used by the spatial query, including SpellId.
    pub(crate) fn environment_catalog(&self) -> &LiquidCatalog {
        self.types.environment_catalog()
    }

    /// A direct adapter over the existing query, not a boolean provider or runtime route.
    pub(crate) fn query_environment(
        &self,
        request: EnvironmentQuery,
        deadline: Instant,
    ) -> Result<BoundLiquidResult, Error> {
        if request.snapshot != self.environment_binding() || request.required_type != 0x0f {
            return Err(Error::Identity);
        }
        let reply = self.query(
            TerrainLiquidRequest {
                snapshot: self.binding(),
                ticket: request.ticket,
                position: request.position,
                required_type: request.required_type,
                collision_height: request.collision_height,
            },
            deadline,
        )?;
        Ok(BoundLiquidResult {
            query: request,
            status: match reply.result.status {
                GridLiquidStatus::NoWater => LiquidStatus::NoWater,
                GridLiquidStatus::AboveWater => LiquidStatus::AboveWater,
                GridLiquidStatus::WaterWalk => LiquidStatus::WaterWalk,
                GridLiquidStatus::InWater => LiquidStatus::InWater,
                GridLiquidStatus::UnderWater => LiquidStatus::UnderWater,
            },
            data: reply.result.data.map(|data| LiquidData {
                entry: data.entry,
                type_flags: data.type_flags,
                level: data.level,
                depth_level: data.depth_level,
            }),
        })
    }
}

#[cfg(test)]
mod tests;
