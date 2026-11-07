// SPDX-License-Identifier: GPL-3.0-or-later
//! Unit::UpdateAllowedPositionZ's swimming-capable branch over complete-dry bytes.
//! CanSwim stays true; the real liquid query still follows the 50-yard height query.
use super::super::liquid::{LiquidReply, LiquidRequest, LiquidStatus};
use super::*;

const ALL_LIQUIDS: u8 = 0x0f;
const DEFAULT_CLASSIFICATION_HEIGHT: f32 = 2.03128;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SwimmingAllowedZRequest {
    pub height: MapHeightRequest,
    /// Selected source collision height, not getLiquidStatus's default height.
    pub collision_height: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WaterOrGroundObservation {
    pub liquid: LiquidReply,
    pub required_type: u8,
    pub classification_height: u32,
    pub min_water_depth: u32,
    /// !HasAuraType(WATER_WALK); this bounded provider only admits the no-aura arm.
    pub swim: bool,
    pub max_z: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SwimmingAllowedZReply {
    pub request: SwimmingAllowedZRequest,
    pub ground: MapHeightReply,
    pub water_or_ground: WaterOrGroundObservation,
    pub allowed_z: u32,
}

impl MapHeightSnapshot<'_> {
    pub(crate) fn query_swimming_allowed_z_with_budget(
        &self,
        request: SwimmingAllowedZRequest,
        budget: &mut vmap::QueryBudget,
    ) -> Result<SwimmingAllowedZReply, Error> {
        if !request.height.swim {
            return Err(Error::Identity);
        }
        let collision_height = f32::from_bits(request.collision_height);
        if !collision_height.is_finite() || collision_height <= 0.0 || collision_height > 100.0 {
            return Err(Error::Unsupported);
        }
        // Map::GetHeight must finish before GetWaterOrGroundLevel; even an invalid
        // ground sentinel does not skip the source liquid query.
        let ground = self.query_with_budget(request.height, budget)?;
        if ground.request != request.height {
            return Err(Error::Identity);
        }
        budget.take()?;
        let dry = self.dry_liquid();
        let liquid_request = LiquidRequest {
            snapshot: dry.binding(),
            ticket: request.height.ticket,
            position: [
                request.height.position[0],
                request.height.position[1],
                ground.height,
            ],
        };
        let liquid = dry.query_at_ground(liquid_request, budget.deadline())?;
        if liquid.request != liquid_request || liquid.status != LiquidStatus::NoWater {
            return Err(Error::Identity);
        }
        // Enabled VMap-height configuration establishes CanCheckLiquidLevel. The
        // decoded complete-dry branch returns NO_WATER before DBC/height use.
        let max_z = water_or_ground_level(
            true,
            f32::from_bits(ground.height),
            true,
            collision_height,
            None,
        );
        let allowed_z = allowed_position_z(
            f32::from_bits(request.height.position[2]),
            f32::from_bits(ground.height),
            max_z,
        )
        .to_bits();
        budget.complete()?;
        Ok(SwimmingAllowedZReply {
            request,
            ground,
            water_or_ground: WaterOrGroundObservation {
                liquid,
                required_type: ALL_LIQUIDS,
                classification_height: DEFAULT_CLASSIFICATION_HEIGHT.to_bits(),
                min_water_depth: request.collision_height,
                swim: true,
                max_z: max_z.to_bits(),
            },
            allowed_z,
        })
    }
}

/// GridMap.cpp:1194-1219. Private scalar oracle seam; only NO_WATER is exposed
/// by the production reader, never caller-authored or unproved wet observations.
fn water_or_ground_level(
    can_check: bool,
    ground_z: f32,
    swim: bool,
    min_water_depth: f32,
    liquid_level: Option<f32>,
) -> f32 {
    if can_check {
        if let Some(level) = liquid_level {
            if swim {
                if level - ground_z > min_water_depth {
                    return level - min_water_depth;
                }
                return ground_z;
            }
            return level;
        }
        return ground_z;
    }
    -200_000.0
}

#[cfg(test)]
#[path = "swimming_tests.rs"]
mod tests;
