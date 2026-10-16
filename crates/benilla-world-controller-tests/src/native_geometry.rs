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

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_point_path.rs"]
mod source_route_point_path;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_smoothing.rs"]
mod source_route_smoothing;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_warm_cache.rs"]
mod source_route_warm_cache;

#[cfg(test)]
pub(crate) use products::authored_route_smoothing;
#[cfg(test)]
pub(crate) use source_route_smoothing::{
    geometry_revision as authored_route_smoothing_revision, RouteSmoothingRequest,
    SourceRouteSmoothingConfiguration, SourceRouteSmoothingProvider, SourceRouteSmoothingStage,
};

#[cfg(test)]
pub(crate) use products::navigation::{
    SourceSmoothRequest, SourceTileReply, SourceTileRequest, SourceWarmRequest,
};
#[cfg(test)]
pub(crate) use source_route_warm_cache::{
    geometry_revision as authored_warm_cache_revision, SourceRouteWarmCacheProvider,
    WarmCacheConfiguration, WarmCacheRequest,
};

#[cfg(test)]
pub(crate) use source_route_point_path::{
    geometry_revision as authored_point_path_revision, PointPathConfiguration, PointPathRequest,
    SourceRoutePointPathProvider,
};

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_straight_path.rs"]
mod source_route_straight_path;

#[cfg(test)]
pub(crate) use source_route_straight_path::{
    geometry_revision as authored_straight_path_revision, SourceRouteStraightPathProvider,
    StraightPathConfiguration, StraightPathDependency, StraightPathRequest,
};

#[cfg(test)]
pub(crate) use products::navigation::tests::straight_ground_products as authored_route_straight_ground;

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_player_environment.rs"]
mod source_player_environment;

#[cfg(test)]
pub(crate) use products::authored_player_environment;
#[cfg(test)]
pub(crate) use source_player_environment::SourcePlayerEnvironmentProvider;

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_chase_gate.rs"]
mod source_route_chase_gate;

#[cfg(test)]
pub(crate) use products::dry_route::tests::{
    environment as authored_dry_chase_environment,
    snake_products as authored_dry_chase_snake_products,
};
#[cfg(test)]
pub(crate) use source_route_chase_gate::{
    geometry_revision as authored_chase_gate_revision, ChaseGateConfiguration, ChaseGateRejection,
    ChaseGateRequest, ChaseGateSelection, SourceRouteChaseGateProvider,
};

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_pathfinder_query.rs"]
mod source_pathfinder_query;

#[allow(dead_code)] // Certification and native-object inspection also serve app-only fixture tests.
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_same_polygon.rs"]
mod source_route_same_polygon;

#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/route_commit.rs"]
pub(crate) mod route_commit;
