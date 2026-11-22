include!("source_route_same_polygon_local_core.rs");

#[cfg(test)]
#[path = "source_route_same_polygon_local/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "source_route_same_polygon_local/two_owner_tests.rs"]
mod two_owner_tests;

#[cfg(test)]
#[path = "source_route_same_polygon_local/quest_loop_tests.rs"]
mod quest_loop_tests;
