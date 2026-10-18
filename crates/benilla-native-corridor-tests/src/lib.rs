#![cfg(test)]
#![allow(dead_code)] // Shared app service entry points are intentionally not installed here.
//! Executes the exact app LocalCorridor and source-loaded readers in a small Bevy host.
//! Authored data and component runtime only; production source admission stays closed.

#[path = "../../benilla-app/src/net/p2p/world/native_geometry/core.rs"]
mod core;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/local_corridor.rs"]
mod local_corridor;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/products/mod.rs"]
mod products;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/real_corridor_fixture.rs"]
mod real_corridor_fixture;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/route_commit.rs"]
mod route_commit;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_cache_reconstruction.rs"]
mod source_cache_reconstruction;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_creature_initialization.rs"]
mod source_creature_initialization;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_ground_height.rs"]
mod source_ground_height;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_location.rs"]
mod source_location;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_navigation.rs"]
mod source_navigation;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_near_point.rs"]
mod source_near_point;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_near_point_empty.rs"]
mod source_near_point_empty;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_pathfinder_query.rs"]
mod source_pathfinder_query;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_player_environment.rs"]
mod source_player_environment;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_chase_gate.rs"]
mod source_route_chase_gate;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_filter.rs"]
mod source_route_filter;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_point_path.rs"]
mod source_route_point_path;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_same_polygon.rs"]
mod source_route_same_polygon;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_same_polygon_local_core.rs"]
mod source_route_same_polygon_local;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_smoothing.rs"]
mod source_route_smoothing;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_straight_path.rs"]
mod source_route_straight_path;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_route_warm_cache.rs"]
mod source_route_warm_cache;
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_vmap.rs"]
mod source_vmap;

use core::{Body, Corridor, Failure, Session};
use local_corridor::LocalCorridor;

mod wander_component_tests;

mod home_component_tests;
mod home_mailbox_tests;
mod suspended_owner_component_tests;

mod reconstruction_component_tests;
mod staged_wander_tests;
mod wander_reopen_component_tests;
