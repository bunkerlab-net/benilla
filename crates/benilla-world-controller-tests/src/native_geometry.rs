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
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_navigation.rs"]
mod source_navigation;
#[allow(dead_code)]
#[path = "../../benilla-app/src/net/p2p/world/native_geometry/source_vmap.rs"]
mod source_vmap;
#[allow(dead_code)]
#[path = "../../benilla-world/src/terrain_stream/native_coverage/static_mesh.rs"]
mod static_mesh;
