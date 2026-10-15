// SPDX-License-Identifier: GPL-3.0-or-later
//! Compile the app geometry source once under its production module path.
#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/core.rs"]
pub(crate) mod core;
#[allow(dead_code)]
#[path = "../../benilla-world/src/terrain_stream/native_coverage/planar.rs"]
mod planar;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/products/mod.rs"]
mod products;
#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_ground_height.rs"]
mod source_ground_height;
#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_navigation.rs"]
mod source_navigation;
#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_near_point.rs"]
mod source_near_point;
#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_vmap.rs"]
mod source_vmap;
#[allow(dead_code)]
#[path = "../../benilla-world/src/terrain_stream/native_coverage/static_mesh.rs"]
mod static_mesh;

#[cfg(test)]
pub(crate) use products::{authored_ground_height, authored_vmap};
#[cfg(test)]
pub(crate) use source_near_point::{
    geometry_revision as authored_near_point_revision, SourceNearPointConfiguration,
    SourceNearPointProvider,
};
#[cfg(test)]
pub(crate) use source_vmap::{geometry_revision as authored_vmap_revision, SourceVmapProvider};

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_near_point_empty.rs"]
mod source_near_point_empty;

#[cfg(test)]
pub(crate) use source_near_point_empty::{
    geometry_revision as authored_empty_near_point_revision, SourceEmptySelectorNearPointProvider,
};

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_filter.rs"]
mod source_route_filter;

#[cfg(test)]
pub(crate) use source_route_filter::{
    geometry_revision as authored_route_filter_revision, RouteFilterRequest,
    SourceRouteFilterProvider,
};
