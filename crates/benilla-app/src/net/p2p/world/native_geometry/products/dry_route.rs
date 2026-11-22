// SPDX-License-Identifier: GPL-3.0-or-later
//! Complete-dry projection of the original opt-in liquid inventory. This combines
//! actual loaded dependencies only; it grants no Chase gate or gameplay admission.
use super::{
    liquid::DryLiquidSnapshot, navigation::NavigationSnapshot, vmap::VmapSnapshot, Error,
    VmapLiquidProducts,
};

/// Owned proof and readers over one immutable inventory, never caller-authored flags
/// or successful-host observations. The VMap is the exact one used to prove dryness.
pub(crate) struct DryRouteProjection {
    liquid: DryLiquidSnapshot,
    navigation: NavigationSnapshot,
    vmap: VmapSnapshot,
}

impl VmapLiquidProducts {
    pub(crate) fn load_dry_route_projection(&self) -> Result<DryRouteProjection, Error> {
        let (liquid, vmap) = self.inventory.load_dry_liquid_and_vmap()?;
        let navigation = self.inventory.load_dry_route_navigation(&liquid)?;
        Ok(DryRouteProjection {
            liquid,
            navigation,
            vmap,
        })
    }
}

impl DryRouteProjection {
    pub(crate) fn liquid(&self) -> &DryLiquidSnapshot {
        &self.liquid
    }
    pub(crate) fn navigation(&self) -> &NavigationSnapshot {
        &self.navigation
    }
    pub(crate) fn static_vmap(&self) -> &VmapSnapshot {
        &self.vmap
    }

    pub(crate) fn into_parts(self) -> (DryLiquidSnapshot, NavigationSnapshot, VmapSnapshot) {
        (self.liquid, self.navigation, self.vmap)
    }
}

#[cfg(test)]
pub(crate) mod tests;
