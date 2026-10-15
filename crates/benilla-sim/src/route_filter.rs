// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// PathFinder.cpp:1030-1123, Unit.cpp:4467-4480, GridMap.cpp:1130-1179.
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later.
//! Bounded non-PLAYERBOTS creature filter prerequisite, not a route result.
//! The host must own the capabilities and prove loaded terrain at the owner
//! position before either callback. Scalar markers alone establish no geometry.

use crate::creature_capabilities::MovementCapabilities;

pub const POLICY: &str = "cmangos-8ec338a1-ground-dry-route-filter-v1";
/// Complete LF-normalized PathFinder.cpp, MoveMapSharedDefines.h, Unit.cpp,
/// ObjectGuid.h, GridMap.cpp and GridMapDefines.h, in manifest order.
pub const SOURCE_SHA256: [&str; 6] = [
    "0fc2684275621af341e104d7593c7f66e5baa6251e088548deb5c547625294ef",
    "7994c9cbb53c81bc30b92b71201e17081040e5988c0e54d9425e0746dec7fd16",
    "03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0",
    "0f89e1aaf212822d048ba55eb637db483e6e04fb629d7032255188100f354dbe",
    "52c6d22fbe9f275196a44bf4f9b39da527071e3c81f7460a4ac03a3c9107b85d",
    "3a901f22b6d65edae74a93b5cd2be793ebc5c3884eb2c0aa06a37265ef53e54e",
];

/// Original NavTerrainFlag values, not an application-specific bit allocation.
pub const NAV_GROUND: u16 = 1;
pub const NAV_GROUND_STEEP: u16 = 2;
pub const NAV_WATER: u16 = 4;
pub const NAV_MAGMA_SLIME: u16 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LiquidMask {
    AllLiquids,
    WaterOrOcean,
}

impl LiquidMask {
    pub const fn bits(self) -> u8 {
        match self {
            Self::AllLiquids => 0x0f,
            Self::WaterOrOcean => 0x0a,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct UnsupportedLiquidStatus {
    pub status: u8,
}

/// A source NO_WATER observation. This is not a loaded-product proof; the host
/// must obtain the status using the requested mask at the actual owner position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NoWater(());

impl NoWater {
    pub const fn try_from_status(status: u8) -> Result<Self, UnsupportedLiquidStatus> {
        if status == 0 {
            Ok(Self(()))
        } else {
            Err(UnsupportedLiquidStatus { status })
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Branch {
    GroundDry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    include_flags: u16,
    exclude_flags: u16,
}

impl Filter {
    pub const fn include_flags(self) -> u16 {
        self.include_flags
    }

    pub const fn exclude_flags(self) -> u16 {
        self.exclude_flags
    }

    pub const fn branch(self) -> Branch {
        Branch::GroundDry
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error<E> {
    UnsupportedCapabilities,
    Liquid(E),
}

/// Original createFilter/updateFilter decision for the owned ground-only domain.
/// Both TerrainInfo::CanCheckLiquidLevel results must already be justified by
/// loaded coverage. IsInWater requests all liquids first; its dry result permits
/// IsUnderwater's water/ocean query. Nonzero statuses are unsupported, including
/// ABOVE_WATER and WATER_WALK: source IsInWater treats either as true.
pub fn create_ground_filter<E>(
    capabilities: MovementCapabilities,
    mut query: impl FnMut(LiquidMask) -> Result<NoWater, E>,
) -> Result<Filter, Error<E>> {
    if !capabilities.can_walk() || capabilities.can_swim() || capabilities.can_fly() {
        return Err(Error::UnsupportedCapabilities);
    }
    let filter = Filter {
        include_flags: NAV_GROUND,
        exclude_flags: 0,
    };
    query(LiquidMask::AllLiquids).map_err(Error::Liquid)?;
    query(LiquidMask::WaterOrOcean).map_err(Error::Liquid)?;
    // Neither source predicate is true, so getNavTerrain is not called.
    Ok(filter)
}

#[cfg(test)]
#[path = "route_filter/tests.rs"]
mod tests;
