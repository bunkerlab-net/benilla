// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded warm straight calculation and typed retry dependency. This does not
//! implement the source Chase gate, publish Route facts or enable native admission.
use super::{
    core::{Failure, Session},
    products::{
        liquid::{DryLiquidSnapshot, SnapshotBinding as LiquidBinding},
        navigation::{
            NavigationSnapshot, SnapshotBinding as NavigationBinding, SourcePointReply,
            SourcePointRequest, SourceStraightReply, SourceStraightRequest,
            SourceStraightWarmReply, SourceTileReply, SourceTileRequest, SourceWarmReply,
            SourceWarmRequest, WarmCacheLineage,
        },
    },
    source_route_filter::{
        BoundRouteFilterQuery, RouteFilterRequest, RouteFilterUpdateDependency,
        SourceRouteFilterProvider,
    },
    source_route_point_path::{self, PointPathConfiguration},
    source_route_warm_cache::WarmCacheConfiguration,
};
use benilla_native_nav::{
    SourcePointPathBudget, SourcePointPathConfig, SourceStraightPathBudget,
    SOURCE_STRAIGHT_PATH_MAX_POINTS, SOURCE_STRAIGHT_PATH_MAX_PRIMITIVES,
    SOURCE_STRAIGHT_PATH_MAX_RAY_POLYGONS, SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES,
    SOURCE_WARM_STRAIGHT_PATH_POLICY,
};
use benilla_sim::creature_motion::RouteKind;
use benilla_world_runtime::native::{
    AuthorityRequest, MotionQuery, Profile, Query, CREATURE_COLLISION_WIDTH_POLICY,
};
use sha2::{Digest, Sha256};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StraightPathConfiguration {
    pub warm: WarmCacheConfiguration,
    pub optimize_path: bool,
    pub normalize_z: bool,
    pub ignore_normalization: bool,
    pub budget: SourceStraightPathBudget,
    pub fallback_budget: SourcePointPathBudget,
}
impl StraightPathConfiguration {
    fn point_configuration(self) -> PointPathConfiguration {
        PointPathConfiguration {
            warm: self.warm,
            optimize_path: self.optimize_path,
            normalize_z: self.normalize_z,
            ignore_normalization: self.ignore_normalization,
            budget: self.fallback_budget,
        }
    }
    fn validate(self) -> Result<(), Failure> {
        if self.normalize_z || self.ignore_normalization {
            return Err(Failure::UnsupportedQuery);
        }
        if self.budget.lookup_primitives > SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES
            || self.budget.raycast_polygons > SOURCE_STRAIGHT_PATH_MAX_RAY_POLYGONS
            || self.budget.sampled_points > SOURCE_STRAIGHT_PATH_MAX_POINTS
            || self.budget.native_primitives > SOURCE_STRAIGHT_PATH_MAX_PRIMITIVES
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StraightPathBinding {
    pub liquid: LiquidBinding,
    pub navigation: NavigationBinding,
    pub configuration: StraightPathConfiguration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct StraightPathRequest {
    pub snapshot: StraightPathBinding,
    pub context: AuthorityRequest,
    pub first_ticket: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StraightFallbackDependency {
    tiles: SourceTileReply,
    filter: RouteFilterUpdateDependency,
    polygons: SourceStraightWarmReply,
    points: SourcePointReply,
}
impl StraightFallbackDependency {
    pub(crate) fn tile_dependency(&self) -> &SourceTileReply {
        &self.tiles
    }
    pub(crate) fn before_cache(&self) -> &benilla_native_nav::WarmCache {
        self.polygons.before()
    }
    pub(crate) fn filter_dependency(&self) -> &RouteFilterUpdateDependency {
        &self.filter
    }
    pub(crate) fn polygon_dependency(&self) -> &SourceWarmReply {
        self.polygons.warm()
    }
    pub(crate) fn point_dependency(&self) -> &SourcePointReply {
        &self.points
    }
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct StraightPathDependency {
    request: StraightPathRequest,
    tiles: SourceTileReply,
    filter: RouteFilterUpdateDependency,
    straight: SourceStraightReply,
    fallback: Option<StraightFallbackDependency>,
}
impl StraightPathDependency {
    pub(crate) fn tile_dependency(&self) -> &SourceTileReply {
        &self.tiles
    }
    pub(crate) fn request(&self) -> &StraightPathRequest {
        &self.request
    }
    pub(crate) fn filter_dependency(&self) -> &RouteFilterUpdateDependency {
        &self.filter
    }
    pub(crate) fn straight_dependency(&self) -> &SourceStraightReply {
        &self.straight
    }
    pub(crate) fn fallback_dependency(&self) -> Option<&StraightFallbackDependency> {
        self.fallback.as_ref()
    }
}
pub(crate) struct SourceRouteStraightPathProvider {
    filter: SourceRouteFilterProvider,
    profile: Profile,
    navigation: NavigationSnapshot,
    lineage: WarmCacheLineage,
    configuration: StraightPathConfiguration,
}
pub(crate) struct BoundStraightPathQuery<'a> {
    provider: &'a SourceRouteStraightPathProvider,
    filter: BoundRouteFilterQuery<'a>,
    context: AuthorityRequest,
    collision_width_bits: u32,
}
impl SourceRouteStraightPathProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        liquid: DryLiquidSnapshot,
        navigation: NavigationSnapshot,
        lineage: WarmCacheLineage,
        configuration: StraightPathConfiguration,
    ) -> Result<Self, Failure> {
        configuration.validate()?;
        navigation
            .check_warm_lineage(&lineage)
            .map_err(|_| Failure::Binding)?;
        let filter = SourceRouteFilterProvider::bind_with_straight_path(
            session,
            profile,
            liquid,
            navigation.binding(),
            configuration,
        )?;
        Ok(Self {
            filter,
            profile: profile.clone(),
            navigation,
            lineage,
            configuration,
        })
    }
    /// Only the typed loaded Chase gate may select its combined geometry policy.
    pub(super) fn bind_with_chase_gate(
        session: &Session,
        profile: &Profile,
        liquid: DryLiquidSnapshot,
        navigation: NavigationSnapshot,
        vmap: super::products::vmap::SnapshotBinding,
        lineage: WarmCacheLineage,
        configuration: super::source_route_chase_gate::ChaseGateConfiguration,
    ) -> Result<Self, Failure> {
        configuration.straight.validate()?;
        navigation
            .check_warm_lineage(&lineage)
            .map_err(|_| Failure::Binding)?;
        let filter = SourceRouteFilterProvider::bind_with_chase_gate(
            session,
            profile,
            liquid,
            navigation.binding(),
            vmap,
            configuration,
        )?;
        Ok(Self {
            filter,
            profile: profile.clone(),
            navigation,
            lineage,
            configuration: configuration.straight,
        })
    }

    pub(super) fn dry_liquid(&self) -> &DryLiquidSnapshot {
        self.filter.dry_liquid()
    }

    pub(crate) fn binding(&self) -> StraightPathBinding {
        StraightPathBinding {
            liquid: self.filter.binding(),
            navigation: self.navigation.binding(),
            configuration: self.configuration,
        }
    }
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundStraightPathQuery<'_>, Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        // Home constructs a fresh stack-local PathFinder, never this warm owner.
        if route.request.kind == RouteKind::HomeForceDestination {
            return Err(Failure::UnsupportedQuery);
        }
        let filter = self.filter.begin(session, context)?;
        let prior = route
            .request
            .prior_cache
            .as_ref()
            .ok_or(Failure::MissingGeometry)?;
        prior
            .validate_for(route.request.binding.identity)
            .map_err(|_| Failure::Binding)?;
        let b = self.navigation.binding();
        let cache = self.lineage.cache();
        if prior.binding.owner_generation != route.owner.npc().generation
            || prior.binding.inventory != b.inventory
            || prior.binding.source_epoch != b.source_epoch
            || prior.binding.navigation_policy != b.policy
            || prior.poly_refs != cache.storage()
            || usize::from(prior.active_len) != cache.active_len()
            || usize::from(prior.point_limit) != cache.point_limit()
            || prior.include_flags != cache.filter().include_flags
            || prior.exclude_flags != cache.filter().exclude_flags
            || route.request.point_limit != prior.point_limit
        {
            return Err(Failure::Binding);
        }
        let width = route
            .require_collision_width(&self.profile)
            .map_err(|error| match error {
                benilla_world_runtime::native::NativeError::AuthorityUnavailable(_) => {
                    Failure::MissingGeometry
                }
                _ => Failure::Binding,
            })?;
        Ok(BoundStraightPathQuery {
            provider: self,
            filter,
            context: context.clone(),
            collision_width_bits: width.width().bits(),
        })
    }
}
/// Observations follow real queries and cannot replace their answers.
#[derive(Debug, PartialEq, Eq)]
enum Observation<'a> {
    Tiles(&'a SourceTileReply),
    Filter(&'a RouteFilterUpdateDependency),
    Straight(&'a SourceStraightReply),
    Warm(&'a SourceWarmReply),
    Points(&'a SourcePointReply),
}
impl BoundStraightPathQuery<'_> {
    pub(crate) fn query(
        &self,
        request: StraightPathRequest,
        deadline: Instant,
    ) -> Result<StraightPathDependency, Failure> {
        self.query_observed(request, deadline, |_| {})
    }
    fn query_observed(
        &self,
        request: StraightPathRequest,
        deadline: Instant,
        mut observe: impl FnMut(Observation<'_>),
    ) -> Result<StraightPathDependency, Failure> {
        if request.snapshot != self.provider.binding()
            || request.context != self.context
            || request.first_ticket == 0
        {
            return Err(Failure::Binding);
        }
        // Reserve every possible query before observing anything. Source errors are
        // unavailable dependencies, never a fabricated NOPATH or retry instruction.
        request
            .first_ticket
            .checked_add(8)
            .ok_or(Failure::Invalid)?;
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let prior = route
            .request
            .prior_cache
            .as_ref()
            .ok_or(Failure::MissingGeometry)?;
        let bits = |p: benilla_sim::creature_ai::Point| [p.x.bits(), p.y.bits(), p.z.bits()];
        benilla_native_nav::source_map_coord_pair(
            bits(route.request.origin),
            bits(route.request.goal),
        )
        .map_err(|_| Failure::Invalid)?;
        let tiles = self.observe_tiles(
            SourceTileRequest {
                snapshot: request.snapshot.navigation,
                ticket: request.first_ticket,
                from: bits(route.request.origin),
                to: bits(route.request.goal),
            },
            deadline,
            &mut observe,
        )?;
        let filter = self.filter.query_retained_update(
            RouteFilterRequest {
                snapshot: request.snapshot.liquid,
                context: request.context.clone(),
                first_ticket: request.first_ticket + 1,
            },
            self.provider.lineage.cache().filter(),
            None,
            deadline,
        )?;
        observe(Observation::Filter(&filter));
        require_time(deadline)?;
        let config = self.provider.configuration;
        let query = SourceStraightRequest {
            snapshot: request.snapshot.navigation,
            ticket: request.first_ticket + 3,
            from: bits(route.request.origin),
            to: bits(route.request.goal),
            collision_width_bits: self.collision_width_bits,
            configuration: SourcePointPathConfig {
                optimize_path: config.optimize_path,
                normalize_z: config.normalize_z,
                ignore_normalization: config.ignore_normalization,
                force_destination: false,
                previous_path_type: prior.path_type,
            },
            budget: config.budget,
        };
        let straight = self
            .provider
            .navigation
            .source_straight_path(&self.provider.lineage, query, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if straight.request() != query {
            return Err(Failure::Binding);
        }
        observe(Observation::Straight(&straight));
        require_time(deadline)?;
        // TargetedMovementGenerator.cpp:536. SHORT alone must not enter this retry.
        let fallback = if straight.result().path_type() & (8 | 4) != 0 {
            // calculate runs HaveTile again before its own updateFilter, even
            // with immutable loaded tiles and the same original requested goal.
            let tiles = self.observe_tiles(
                SourceTileRequest {
                    ticket: request.first_ticket + 4,
                    ..tiles.request()
                },
                deadline,
                &mut observe,
            )?;
            let updated = self.filter.query_retained_update(
                RouteFilterRequest {
                    snapshot: request.snapshot.liquid,
                    context: request.context.clone(),
                    first_ticket: request.first_ticket + 5,
                },
                straight.result().after().filter(),
                Some(&filter),
                deadline,
            )?;
            observe(Observation::Filter(&updated));
            require_time(deadline)?;
            let warm_query = SourceWarmRequest {
                snapshot: query.snapshot,
                ticket: request.first_ticket + 7,
                from: query.from,
                to: query.to,
                budget: config.warm.budget,
            };
            let polygons = self
                .provider
                .navigation
                .source_straight_warm(&straight, warm_query, deadline)
                .map_err(|_| Failure::MissingGeometry)?;
            if polygons.warm().request() != warm_query {
                return Err(Failure::Binding);
            }
            observe(Observation::Warm(polygons.warm()));
            require_time(deadline)?;
            let point_query = SourcePointRequest {
                warm: warm_query,
                ticket: request.first_ticket + 8,
                configuration: SourcePointPathConfig {
                    previous_path_type: straight.result().path_type(),
                    ..query.configuration
                },
                budget: config.fallback_budget,
            };
            let points = self
                .provider
                .navigation
                .source_straight_point_path(&polygons, point_query, deadline)
                .map_err(|_| Failure::MissingGeometry)?;
            if points.request() != point_query {
                return Err(Failure::Binding);
            }
            observe(Observation::Points(&points));
            Some(StraightFallbackDependency {
                tiles,
                filter: updated,
                polygons,
                points,
            })
        } else {
            None
        };
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        Ok(StraightPathDependency {
            request,
            tiles,
            filter,
            straight,
            fallback,
        })
    }
    fn observe_tiles(
        &self,
        request: SourceTileRequest,
        deadline: Instant,
        observe: &mut impl FnMut(Observation<'_>),
    ) -> Result<SourceTileReply, Failure> {
        require_time(deadline)?;
        let tiles = self
            .provider
            .navigation
            .source_tile_presence(request, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if tiles.request() != request {
            return Err(Failure::Binding);
        }
        observe(Observation::Tiles(&tiles));
        require_time(deadline)?;
        if !tiles.result().both_present() {
            // The source shortcut branch is known, but remains unavailable here:
            // no guessed NOPATH/NORMAL, synthetic route, or nearest-poly substitute.
            return Err(Failure::MissingGeometry);
        }
        Ok(tiles)
    }
}
fn require_time(deadline: Instant) -> Result<(), Failure> {
    if Instant::now() >= deadline {
        Err(Failure::MissingGeometry)
    } else {
        Ok(())
    }
}
pub(crate) fn geometry_revision(
    liquid: LiquidBinding,
    navigation: NavigationBinding,
    configuration: StraightPathConfiguration,
) -> Result<[u8; 32], Failure> {
    configuration.validate()?;
    let point = source_route_point_path::geometry_revision(
        liquid,
        navigation,
        configuration.point_configuration(),
    )?;
    let mut hash = Sha256::new();
    hash.update(b"benilla-source-warm-straight-loaded-tile-retry-dependency-v2");
    hash.update(point);
    hash.update(SOURCE_WARM_STRAIGHT_PATH_POLICY.to_le_bytes());
    hash.update(benilla_native_nav::SOURCE_TILE_PRESENCE_POLICY.to_le_bytes());
    hash.update(CREATURE_COLLISION_WIDTH_POLICY.as_bytes());
    hash.update(benilla_sim::creature_init::CREATURE_COLLISION_WIDTH_POLICY_ID.as_bytes());
    hash.update(
        b"loaded-start-then-end-HaveTile-before-retained-ground-dry-updateFilter-each-calculate-original-goal-terminal-cache-type",
    );
    for value in [
        configuration.budget.lookup_primitives,
        configuration.budget.raycast_polygons,
        configuration.budget.sampled_points,
        configuration.budget.native_primitives,
    ] {
        hash.update(
            u64::try_from(value)
                .map_err(|_| Failure::Invalid)?
                .to_le_bytes(),
        );
    }
    Ok(hash.finalize().into())
}
#[cfg(test)]
#[path = "source_route_straight_path/tests.rs"]
mod tests;
