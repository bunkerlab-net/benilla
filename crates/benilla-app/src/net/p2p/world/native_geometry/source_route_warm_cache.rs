// SPDX-License-Identifier: GPL-3.0-or-later
//! Bound loaded warm polygon proposals, not terminal owner calculations or Route facts.
use super::{
    core::{Failure, Session},
    products::{
        liquid::{DryLiquidSnapshot, SnapshotBinding as LiquidBinding},
        navigation::{
            NavigationSnapshot, SnapshotBinding as NavigationBinding, SourceWarmReply,
            SourceWarmRequest, WarmCacheLineage,
        },
    },
    source_route_filter::{
        self, BoundRouteFilterQuery, RouteFilterDependency, RouteFilterRequest,
        SourceRouteFilterProvider,
    },
};
use benilla_native_nav::{
    SourceWarmCacheBudget, SOURCE_PATHFINDER_CPP_SHA256, SOURCE_PATHFINDER_H_SHA256,
    SOURCE_SMOOTHING_POLICY, SOURCE_WARM_CACHE_MAX_CORRIDOR_EXPANSIONS,
    SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES, SOURCE_WARM_CACHE_POLICY,
};
use benilla_world_runtime::native::{AuthorityRequest, MotionQuery, Profile, Query};
use sha2::{Digest, Sha256};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WarmCacheConfiguration {
    pub budget: SourceWarmCacheBudget,
}
impl WarmCacheConfiguration {
    fn validate(self) -> Result<(), Failure> {
        if self.budget.lookup_primitives > SOURCE_WARM_CACHE_MAX_LOOKUP_PRIMITIVES
            || self.budget.corridor_expansions > SOURCE_WARM_CACHE_MAX_CORRIDOR_EXPANSIONS
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct WarmCacheBinding {
    pub liquid: LiquidBinding,
    pub navigation: NavigationBinding,
    pub configuration: WarmCacheConfiguration,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct WarmCacheRequest {
    pub snapshot: WarmCacheBinding,
    pub context: AuthorityRequest,
    pub first_ticket: u64,
}
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct WarmCacheDependency {
    request: WarmCacheRequest,
    filter: RouteFilterDependency,
    polygons: SourceWarmReply,
}
impl WarmCacheDependency {
    pub(crate) fn request(&self) -> &WarmCacheRequest {
        &self.request
    }
    pub(crate) fn filter_dependency(&self) -> &RouteFilterDependency {
        &self.filter
    }
    pub(crate) fn polygon_dependency(&self) -> &SourceWarmReply {
        &self.polygons
    }
}
pub(crate) struct SourceRouteWarmCacheProvider {
    filter: SourceRouteFilterProvider,
    navigation: NavigationSnapshot,
    lineage: WarmCacheLineage,
    configuration: WarmCacheConfiguration,
}
pub(crate) struct BoundWarmCacheQuery<'a> {
    provider: &'a SourceRouteWarmCacheProvider,
    filter: BoundRouteFilterQuery<'a>,
    context: AuthorityRequest,
}
impl SourceRouteWarmCacheProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        liquid: DryLiquidSnapshot,
        navigation: NavigationSnapshot,
        lineage: WarmCacheLineage,
        configuration: WarmCacheConfiguration,
    ) -> Result<Self, Failure> {
        if lineage.binding() != navigation.binding() {
            return Err(Failure::Binding);
        }
        let filter = SourceRouteFilterProvider::bind_with_warm_cache(
            session,
            profile,
            liquid,
            navigation.binding(),
            configuration,
        )?;
        Ok(Self {
            filter,
            navigation,
            lineage,
            configuration,
        })
    }
    pub(super) fn bind_with_point_path(
        session: &Session,
        profile: &Profile,
        liquid: DryLiquidSnapshot,
        navigation: NavigationSnapshot,
        lineage: WarmCacheLineage,
        configuration: super::source_route_point_path::PointPathConfiguration,
    ) -> Result<Self, Failure> {
        if lineage.binding() != navigation.binding() {
            return Err(Failure::Binding);
        }
        let filter = SourceRouteFilterProvider::bind_with_point_path(
            session,
            profile,
            liquid,
            navigation.binding(),
            configuration,
        )?;
        Ok(Self {
            filter,
            navigation,
            lineage,
            configuration: configuration.warm,
        })
    }
    pub(super) fn navigation(&self) -> &NavigationSnapshot {
        &self.navigation
    }

    pub(crate) fn binding(&self) -> WarmCacheBinding {
        WarmCacheBinding {
            liquid: self.filter.binding(),
            navigation: self.navigation.binding(),
            configuration: self.configuration,
        }
    }
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundWarmCacheQuery<'_>, Failure> {
        let filter = self.filter.begin(session, context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
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
        Ok(BoundWarmCacheQuery {
            provider: self,
            filter,
            context: context.clone(),
        })
    }
}
impl BoundWarmCacheQuery<'_> {
    pub(crate) fn query(
        &self,
        request: WarmCacheRequest,
        deadline: Instant,
    ) -> Result<WarmCacheDependency, Failure> {
        if request.snapshot != self.provider.binding()
            || request.context != self.context
            || request.first_ticket == 0
        {
            return Err(Failure::Binding);
        }
        let ticket = request
            .first_ticket
            .checked_add(2)
            .ok_or(Failure::Invalid)?;
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        let filter = self.filter.query(
            RouteFilterRequest {
                snapshot: request.snapshot.liquid,
                context: request.context.clone(),
                first_ticket: request.first_ticket,
            },
            deadline,
        )?;
        if filter.filter().include_flags() != self.provider.lineage.cache().filter().include_flags
            || filter.filter().exclude_flags()
                != self.provider.lineage.cache().filter().exclude_flags
        {
            return Err(Failure::Binding);
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let bits = |p: benilla_sim::creature_ai::Point| [p.x.bits(), p.y.bits(), p.z.bits()];
        let query = SourceWarmRequest {
            snapshot: request.snapshot.navigation,
            ticket,
            from: bits(route.request.origin),
            to: bits(route.request.goal),
            budget: request.snapshot.configuration.budget,
        };
        let polygons = self
            .provider
            .navigation
            .source_warm(&self.provider.lineage, query, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if polygons.request() != query {
            return Err(Failure::Binding);
        }
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        Ok(WarmCacheDependency {
            request,
            filter,
            polygons,
        })
    }
}
pub(crate) fn geometry_revision(
    liquid: LiquidBinding,
    navigation: NavigationBinding,
    configuration: WarmCacheConfiguration,
) -> Result<[u8; 32], Failure> {
    configuration.validate()?;
    if liquid.map != navigation.map
        || liquid.content != navigation.content
        || liquid.source_epoch != navigation.source_epoch
        || liquid.inventory != navigation.inventory
    {
        return Err(Failure::Binding);
    }
    let mut hash = Sha256::new();
    hash.update(b"benilla-source-warm-retained-polygon-dependency-v1");
    hash.update(source_route_filter::geometry_revision(liquid));
    hash.update(navigation.map.to_le_bytes());
    for value in [
        navigation.content,
        navigation.source_epoch,
        navigation.inventory,
        navigation.policy,
    ] {
        hash.update(value);
    }
    hash.update(SOURCE_WARM_CACHE_POLICY.to_le_bytes());
    hash.update(SOURCE_SMOOTHING_POLICY.to_le_bytes());
    hash.update(SOURCE_PATHFINDER_CPP_SHA256.as_bytes());
    hash.update(SOURCE_PATHFINDER_H_SHA256.as_bytes());
    hash.update(b"fixed-74-pre-BuildPointPath-no-cold-owner-producer");
    for value in [
        configuration.budget.lookup_primitives,
        configuration.budget.corridor_expansions,
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
#[path = "source_route_warm_cache/tests.rs"]
mod tests;
