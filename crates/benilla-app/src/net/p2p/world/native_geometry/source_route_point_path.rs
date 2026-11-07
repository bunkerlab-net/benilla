// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded terminal smooth-calculation dependency; not source Chase dispatch or a Route fact.
use super::{
    core::{Failure, Session},
    products::{
        liquid::{DryLiquidSnapshot, SnapshotBinding as LiquidBinding},
        navigation::{
            NavigationSnapshot, SnapshotBinding as NavigationBinding, SourcePointReply,
            SourcePointRequest, WarmCacheLineage,
        },
    },
    source_route_warm_cache::{
        self, BoundWarmCacheQuery, SourceRouteWarmCacheProvider, WarmCacheBinding,
        WarmCacheConfiguration, WarmCacheDependency, WarmCacheRequest,
    },
};
use benilla_native_nav::{
    SourcePointPathBudget, SourcePointPathConfig, SOURCE_POINT_PATH_MAX_POSTPROCESS_PRIMITIVES,
    SOURCE_SMOOTHING_MAX_PRIMITIVES, SOURCE_WARM_POINT_PATH_POLICY,
};
use benilla_sim::creature_motion::RouteKind;
use benilla_world_runtime::native::{AuthorityRequest, MotionQuery, Profile, Query};
use sha2::{Digest, Sha256};
use std::time::Instant;

/// Explicit query configuration; no inferred server settings or world-data defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PointPathConfiguration {
    pub warm: WarmCacheConfiguration,
    pub optimize_path: bool,
    pub normalize_z: bool,
    pub ignore_normalization: bool,
    pub budget: SourcePointPathBudget,
}
impl PointPathConfiguration {
    fn validate(self) -> Result<(), Failure> {
        // Ordinary Chase constructs PathFinder(owner), whose ignoreNormalization is false.
        // Active normalization needs its own actual allowed-Z/transport observations.
        if self.normalize_z || self.ignore_normalization {
            return Err(Failure::UnsupportedQuery);
        }
        if self.budget.smoothing_primitives > SOURCE_SMOOTHING_MAX_PRIMITIVES
            || self.budget.postprocess_primitives > SOURCE_POINT_PATH_MAX_POSTPROCESS_PRIMITIVES
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PointPathBinding {
    pub warm: WarmCacheBinding,
    pub configuration: PointPathConfiguration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PointPathRequest {
    pub snapshot: PointPathBinding,
    pub context: AuthorityRequest,
    pub first_ticket: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct PointPathDependency {
    request: PointPathRequest,
    warm: WarmCacheDependency,
    points: SourcePointReply,
}
impl PointPathDependency {
    pub(crate) fn request(&self) -> &PointPathRequest {
        &self.request
    }
    pub(crate) fn warm_dependency(&self) -> &WarmCacheDependency {
        &self.warm
    }
    pub(crate) fn point_dependency(&self) -> &SourcePointReply {
        &self.points
    }
}
pub(crate) struct SourceRoutePointPathProvider {
    warm: SourceRouteWarmCacheProvider,
    configuration: PointPathConfiguration,
}
pub(crate) struct BoundPointPathQuery<'a> {
    provider: &'a SourceRoutePointPathProvider,
    warm: BoundWarmCacheQuery<'a>,
    context: AuthorityRequest,
}
impl SourceRoutePointPathProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        liquid: DryLiquidSnapshot,
        navigation: NavigationSnapshot,
        lineage: WarmCacheLineage,
        configuration: PointPathConfiguration,
    ) -> Result<Self, Failure> {
        configuration.validate()?;
        let warm = SourceRouteWarmCacheProvider::bind_with_point_path(
            session,
            profile,
            liquid,
            navigation,
            lineage,
            configuration,
        )?;
        Ok(Self {
            warm,
            configuration,
        })
    }
    pub(crate) fn binding(&self) -> PointPathBinding {
        PointPathBinding {
            warm: self.warm.binding(),
            configuration: self.configuration,
        }
    }
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundPointPathQuery<'_>, Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if route.request.kind == RouteKind::HomeForceDestination {
            // Home constructs a fresh stack-local PathFinder. A warm handle cannot prove it.
            return Err(Failure::UnsupportedQuery);
        }
        let warm = self.warm.begin(session, context)?;
        Ok(BoundPointPathQuery {
            provider: self,
            warm,
            context: context.clone(),
        })
    }
}
impl BoundPointPathQuery<'_> {
    pub(crate) fn query(
        &self,
        request: PointPathRequest,
        deadline: Instant,
    ) -> Result<PointPathDependency, Failure> {
        if request.snapshot != self.provider.binding()
            || request.context != self.context
            || request.first_ticket == 0
        {
            return Err(Failure::Binding);
        }
        // Allocate the entire two-liquid/warm/terminal sequence before any query.
        let ticket = request
            .first_ticket
            .checked_add(3)
            .ok_or(Failure::Invalid)?;
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        let warm = self.warm.query(
            WarmCacheRequest {
                snapshot: request.snapshot.warm,
                context: request.context.clone(),
                first_ticket: request.first_ticket,
            },
            deadline,
        )?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let prior = route
            .request
            .prior_cache
            .as_ref()
            .ok_or(Failure::MissingGeometry)?;
        let configuration = self.provider.configuration;
        let query = SourcePointRequest {
            warm: warm.polygon_dependency().request(),
            ticket,
            configuration: SourcePointPathConfig {
                optimize_path: configuration.optimize_path,
                normalize_z: configuration.normalize_z,
                ignore_normalization: configuration.ignore_normalization,
                force_destination: false,
                previous_path_type: prior.path_type,
            },
            budget: configuration.budget,
        };
        let points = self
            .provider
            .warm
            .navigation()
            .source_point_path(warm.polygon_dependency(), query, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if points.request() != query {
            return Err(Failure::Binding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        Ok(PointPathDependency {
            request,
            warm,
            points,
        })
    }
}
pub(crate) fn geometry_revision(
    liquid: LiquidBinding,
    navigation: NavigationBinding,
    configuration: PointPathConfiguration,
) -> Result<[u8; 32], Failure> {
    configuration.validate()?;
    let warm = source_route_warm_cache::geometry_revision(liquid, navigation, configuration.warm)?;
    let mut hash = Sha256::new();
    hash.update(b"benilla-source-warm-terminal-smooth-calculation-v1");
    hash.update(warm);
    hash.update(SOURCE_WARM_POINT_PATH_POLICY.to_le_bytes());
    hash.update([
        u8::from(configuration.optimize_path),
        u8::from(configuration.normalize_z),
        u8::from(configuration.ignore_normalization),
    ]);
    hash.update(b"nontransport-fixed74-smooth-only-Chase-dependency-forceDest-false");
    for value in [
        configuration.budget.smoothing_primitives,
        configuration.budget.postprocess_primitives,
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
#[path = "source_route_point_path/tests.rs"]
mod tests;
