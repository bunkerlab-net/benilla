// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded owner-filter -> fresh corridor -> findSmoothPath dependency.
//! This stops before OptimizePath/NormalizeZ and grants no route fact or admission.
use super::{
    core::{Failure, Session},
    products::{
        liquid::{DryLiquidSnapshot, SnapshotBinding as LiquidBinding},
        navigation::{
            NavigationSnapshot, SnapshotBinding as NavigationBinding, SourceSmoothReply,
            SourceSmoothRequest,
        },
    },
    source_route_filter::{
        self, BoundRouteFilterQuery, RouteFilterDependency, RouteFilterRequest,
        SourceRouteFilterProvider,
    },
};
use benilla_native_nav::{
    SourceOwnerFilter, SourceSmoothingBudget, SOURCE_PATHFINDER_CPP_SHA256,
    SOURCE_PATHFINDER_H_SHA256, SOURCE_SMOOTHING_MAX_CORRIDOR_EXPANSIONS,
    SOURCE_SMOOTHING_MAX_POINTS, SOURCE_SMOOTHING_MAX_PRIMITIVES, SOURCE_SMOOTHING_POLICY,
};
use benilla_world_runtime::native::{AuthorityRequest, MotionQuery, Profile, Query};
use sha2::{Digest, Sha256};
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceRouteSmoothingStage {
    PreOptimizePathPreNormalizeZ,
}

/// Explicit source-stage configuration, not a deduction from dungeon metadata or
/// the runtime route capacity. The selected limit may not exceed that capacity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SourceRouteSmoothingConfiguration {
    pub stage: SourceRouteSmoothingStage,
    pub point_limit: usize,
    pub budget: SourceSmoothingBudget,
}
impl SourceRouteSmoothingConfiguration {
    fn validate(self) -> Result<(), Failure> {
        if !(2..=SOURCE_SMOOTHING_MAX_POINTS).contains(&self.point_limit)
            || self.budget.corridor_expansions > SOURCE_SMOOTHING_MAX_CORRIDOR_EXPANSIONS
            || self.budget.smoothing_primitives > SOURCE_SMOOTHING_MAX_PRIMITIVES
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RouteSmoothingBinding {
    pub liquid: LiquidBinding,
    pub navigation: NavigationBinding,
    pub configuration: SourceRouteSmoothingConfiguration,
}

/// One frozen complete source request. Tickets identify two liquid observations
/// followed by the actual loaded navigation query, under one absolute deadline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct RouteSmoothingRequest {
    pub snapshot: RouteSmoothingBinding,
    pub context: AuthorityRequest,
    pub first_ticket: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) struct RouteSmoothingDependency {
    request: RouteSmoothingRequest,
    filter: RouteFilterDependency,
    smoothing: SourceSmoothReply,
}
impl RouteSmoothingDependency {
    pub(crate) fn request(&self) -> &RouteSmoothingRequest {
        &self.request
    }
    pub(crate) fn filter_dependency(&self) -> &RouteFilterDependency {
        &self.filter
    }
    pub(crate) fn smoothing_reply(&self) -> &SourceSmoothReply {
        &self.smoothing
    }
}

pub(crate) struct SourceRouteSmoothingProvider {
    filter: SourceRouteFilterProvider,
    navigation: NavigationSnapshot,
    configuration: SourceRouteSmoothingConfiguration,
}
pub(crate) struct BoundRouteSmoothingQuery<'a> {
    provider: &'a SourceRouteSmoothingProvider,
    filter: BoundRouteFilterQuery<'a>,
    context: AuthorityRequest,
}

impl SourceRouteSmoothingProvider {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        liquid: DryLiquidSnapshot,
        navigation: NavigationSnapshot,
        configuration: SourceRouteSmoothingConfiguration,
    ) -> Result<Self, Failure> {
        let filter = SourceRouteFilterProvider::bind_with_smoothing(
            session,
            profile,
            liquid,
            navigation.binding(),
            configuration,
        )?;
        Ok(Self {
            filter,
            navigation,
            configuration,
        })
    }
    pub(crate) fn binding(&self) -> RouteSmoothingBinding {
        RouteSmoothingBinding {
            liquid: self.filter.binding(),
            navigation: self.navigation.binding(),
            configuration: self.configuration,
        }
    }
    pub(crate) fn begin(
        &self,
        session: &Session,
        context: &AuthorityRequest,
    ) -> Result<BoundRouteSmoothingQuery<'_>, Failure> {
        // Retain every participant, profile, source capability, generation,
        // clock, owner position and body guard from the loaded filter provider.
        let filter = self.filter.begin(session, context)?;
        let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        if self.configuration.point_limit > usize::from(route.request.point_limit) {
            return Err(Failure::Invalid);
        }
        Ok(BoundRouteSmoothingQuery {
            provider: self,
            filter,
            context: context.clone(),
        })
    }
}

enum QueryObservation<'a> {
    Filter(&'a RouteFilterDependency),
    Smoothing(&'a SourceSmoothReply),
}

impl BoundRouteSmoothingQuery<'_> {
    pub(crate) fn query(
        &self,
        request: RouteSmoothingRequest,
        deadline: Instant,
    ) -> Result<RouteSmoothingDependency, Failure> {
        self.query_observed(request, deadline, |_| {})
    }

    /// Observers see actual immutable results; they cannot replace loaded answers.
    fn query_observed(
        &self,
        request: RouteSmoothingRequest,
        deadline: Instant,
        mut observe: impl FnMut(QueryObservation<'_>),
    ) -> Result<RouteSmoothingDependency, Failure> {
        if request.snapshot != self.provider.binding()
            || request.context != self.context
            || request.first_ticket == 0
        {
            return Err(Failure::Binding);
        }
        let navigation_ticket = request
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
        observe(QueryObservation::Filter(&filter));
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.context.query else {
            return Err(Failure::UnsupportedQuery);
        };
        let bits = |point: benilla_sim::creature_ai::Point| {
            [point.x, point.y, point.z].map(|value| value.as_f32().to_bits())
        };
        let navigation_request = SourceSmoothRequest {
            snapshot: request.snapshot.navigation,
            ticket: navigation_ticket,
            // BuildPolyPath/findSmoothPath receive original endpoints. A nearest
            // point, launch origin or liquid query position cannot replace these.
            from: bits(route.request.origin),
            to: bits(route.request.goal),
            filter: SourceOwnerFilter {
                include_flags: filter.filter().include_flags(),
                exclude_flags: filter.filter().exclude_flags(),
            },
            point_limit: request.snapshot.configuration.point_limit,
            budget: request.snapshot.configuration.budget,
        };
        let smoothing = self
            .provider
            .navigation
            .source_smooth(navigation_request, deadline)
            .map_err(|_| Failure::MissingGeometry)?;
        if smoothing.request != navigation_request {
            return Err(Failure::Binding);
        }
        observe(QueryObservation::Smoothing(&smoothing));
        if Instant::now() >= deadline {
            return Err(Failure::MissingGeometry);
        }
        Ok(RouteSmoothingDependency {
            request,
            filter,
            smoothing,
        })
    }
}

pub(crate) fn geometry_revision(
    liquid: LiquidBinding,
    navigation: NavigationBinding,
    configuration: SourceRouteSmoothingConfiguration,
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
    hash.update(b"benilla-source-ground-owner-loaded-smoothing-dependency-v1");
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
    hash.update(SOURCE_SMOOTHING_POLICY.to_le_bytes());
    hash.update(SOURCE_PATHFINDER_CPP_SHA256.as_bytes());
    hash.update(SOURCE_PATHFINDER_H_SHA256.as_bytes());
    match configuration.stage {
        SourceRouteSmoothingStage::PreOptimizePathPreNormalizeZ => {
            hash.update(b"pre-OptimizePath-pre-NormalizeZ");
        }
    }
    for value in [
        configuration.point_limit,
        configuration.budget.corridor_expansions,
        configuration.budget.smoothing_primitives,
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
#[path = "source_route_smoothing/tests.rs"]
mod tests;
