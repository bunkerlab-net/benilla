// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-bound point observations for constructor filters, never a dry region.
use super::super::{
    products::{
        navigation,
        source_height::{self, PointNoWaterReply, PointNoWaterRequest, PreparedSourceHeight},
        source_query::SourceMapQuerySnapshot,
    },
    source_pathfinder_query::SourcePathfinderQueryProvider,
};
use super::*;
use std::rc::Rc;

#[path = "chase_los.rs"]
pub(crate) mod chase_los;
#[path = "cold.rs"]
pub(crate) mod cold;
#[path = "dry_near_point.rs"]
pub(crate) mod dry_near_point;
#[path = "near_point.rs"]
pub(crate) mod near_point;

const POLICY: &[u8] = b"benilla-resident-query-ground-primary-v2";
pub(crate) fn geometry_revision(
    height: source_height::Binding,
    navigation: navigation::SnapshotBinding,
) -> [u8; 32] {
    let mut hash = Sha256::new();
    hash.update(POLICY);
    hash.update([1, 1, 1]); // Fixed collision detection, VMap height and LOS enabled.
    hash.update(10.0f32.to_bits().to_le_bytes()); // Ground Map::GetHeight search.
    hash.update(b"independent-empty-dynamic-participants-v1");
    hash.update(b"cmangos-8ec338a1-map-height-ground-search10-allowed-z-v1");
    // Pinned Unit.cpp, including the Ground UpdateAllowedPositionZ branch.
    hash.update(b"03b67a4b6226f1f463e71be3694d3c950fc9b915a6e921407a98e4ff82fc21d0");
    for policy in [
        benilla_sim::near_point::primary::POLICY,
        benilla_sim::near_point::POLICY,
        benilla_sim::source_trig::SOFT_POLICY,
        benilla_sim::facing::POLICY,
    ] {
        hash.update((policy.len() as u64).to_le_bytes());
        hash.update(policy.as_bytes());
    }
    for source in benilla_sim::near_point::primary::SOURCE_SHA256
        .iter()
        .chain(benilla_sim::near_point::SOURCE_SHA256.iter())
        .chain(benilla_sim::source_trig::SOURCE_SHA256.iter())
        .chain(benilla_sim::facing::SOURCE_SHA256.iter())
    {
        hash.update(source.as_bytes());
    }
    hash.update(height.map.to_le_bytes());
    hash.update(height.revision);
    hash.update(super::super::source_pathfinder_query::geometry_revision(
        navigation,
    ));
    hash.finalize().into()
}
#[derive(Debug)]
pub(crate) enum ResidentFilterError {
    Context(Failure),
    Query(super::super::source_pathfinder_query::ConstructorQueryError),
    Source(source_height::CompositionError),
    Terminal(benilla_native_nav::SourceSamePolygonError),
}
impl std::fmt::Display for ResidentFilterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Context(error) => write!(f, "resident filter context: {error:?}"),
            Self::Query(error) => write!(f, "resident filter query: {error:?}"),
            Self::Source(error) => write!(f, "resident filter source: {error:?}"),
            Self::Terminal(error) => write!(f, "resident conditional terminal: {error:?}"),
        }
    }
}
impl From<Failure> for ResidentFilterError {
    fn from(e: Failure) -> Self {
        Self::Context(e)
    }
}

pub(crate) struct ResidentFilterProvider<'a, 'terrain, 'bundle, 'source> {
    identity: Rc<()>,
    session: Session,
    profile: Profile,
    height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
    query: SourcePathfinderQueryProvider,
}
/// Move-only, exact-dispatch point observations. No public fields or Route conversion.
pub(crate) struct ResidentFilterDependency {
    identity: Rc<()>,
    context: AuthorityRequest,
    expires: Instant,
    observations: [PointNoWaterReply; 2],
    filter: benilla_native_nav::SourceOwnerFilter,
}
impl ResidentFilterDependency {
    #[cfg(test)]
    pub(crate) fn expire_for_test(&mut self) {
        self.expires = Instant::now();
    }
    pub(crate) fn observations(&self) -> &[PointNoWaterReply; 2] {
        &self.observations
    }
}
impl<'a, 'terrain, 'bundle, 'source> ResidentFilterProvider<'a, 'terrain, 'bundle, 'source> {
    pub(crate) fn bind(
        session: &Session,
        profile: &Profile,
        height: &'a PreparedSourceHeight<'terrain, 'bundle, 'source>,
        navigation: SourceMapQuerySnapshot,
    ) -> Result<Self, ResidentFilterError> {
        if height.binding().map != navigation.binding().map
            || height.generated_query_source_epoch() != navigation.binding().source_epoch
        {
            return Err(Failure::Binding.into());
        }
        require_participants(session)?;
        height
            .verify_unchanged(|| false)
            .map_err(ResidentFilterError::Source)?;
        let query = SourcePathfinderQueryProvider::bind_with_resident_filter(
            session,
            profile,
            navigation,
            height.binding(),
        )
        .map_err(ResidentFilterError::Query)?;
        Ok(Self {
            identity: Rc::new(()),
            session: session.clone(),
            profile: profile.clone(),
            height,
            query,
        })
    }
    pub(crate) fn observe(
        &mut self,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentFilterDependency, ResidentFilterError> {
        if !context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?
            .first_construction()
        {
            return Err(Failure::Binding.into());
        }
        self.observe_inner(context, first_ticket, deadline, cancelled, None)
    }
    pub(crate) fn observe_update(
        &mut self,
        prior: &ResidentFilterDependency,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<ResidentFilterDependency, ResidentFilterError> {
        // Expired point evidence cannot answer a query, but its private retained
        // filter provenance still permits fresh source observations on this owner.
        if !Rc::ptr_eq(&self.identity, &prior.identity) {
            return Err(Failure::Binding.into());
        }
        self.check_context(&prior.context, deadline, &cancelled)?;
        let retained = prior.filter;
        self.check_context(context, deadline, &cancelled)?;
        let saved = prior
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let Query::SourceMotion(MotionQuery::Route(old_route)) = &prior.context.query else {
            return Err(Failure::Binding.into());
        };
        let Query::SourceMotion(MotionQuery::Route(new_route)) = &context.query else {
            return Err(Failure::Binding.into());
        };
        if saved.construction_binding() != current.construction_binding()
            || old_route.owner != new_route.owner
        {
            return Err(Failure::Binding.into());
        }
        if prior.observations[1].request().ticket >= first_ticket {
            return Err(Failure::Binding.into());
        }
        self.observe_inner(context, first_ticket, deadline, cancelled, Some(retained))
    }
    fn observe_inner(
        &mut self,
        context: &AuthorityRequest,
        first_ticket: u64,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        retained: Option<benilla_native_nav::SourceOwnerFilter>,
    ) -> Result<ResidentFilterDependency, ResidentFilterError> {
        self.check_context(context, deadline, &cancelled)?;
        if first_ticket == 0 || first_ticket.checked_add(1).is_none() {
            return Err(Failure::Invalid.into());
        }
        // Consumes the real live constructor event, retaining its original query.
        self.query
            .retain_constructor(context)
            .map_err(ResidentFilterError::Query)?;
        let (observations, filter) = point_observations(
            self.height,
            context,
            first_ticket,
            deadline,
            &cancelled,
            retained,
            &mut |_| {},
        )?;
        self.check_context(context, deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(ResidentFilterDependency {
            identity: Rc::clone(&self.identity),
            context: context.clone(),
            expires: deadline,
            observations,
            filter,
        })
    }
    /// Reuse is limited to the identical dispatch before expiry. Movement, another
    /// dispatch or a changed capability witness requires a fresh observation.
    pub(crate) fn filter_for(
        &self,
        dependency: &ResidentFilterDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<benilla_native_nav::SourceOwnerFilter, ResidentFilterError> {
        if !Rc::ptr_eq(&self.identity, &dependency.identity) || dependency.context != *context {
            return Err(Failure::Binding.into());
        }
        let saved = dependency
            .context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        let current = context
            .require_pathfinder_dispatch(&self.profile)
            .map_err(|_| Failure::Binding)?;
        if !saved.same_live_dispatch(current) {
            return Err(Failure::Binding.into());
        }
        let deadline = deadline.min(dependency.expires);
        self.check_context(context, deadline, &cancelled)?;
        self.height
            .verify_unchanged(&cancelled)
            .map_err(ResidentFilterError::Source)?;
        self.check_context(context, deadline, &cancelled)?;
        Ok(dependency.filter)
    }
    fn check_context(
        &self,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: &impl Fn() -> bool,
    ) -> Result<(), ResidentFilterError> {
        if cancelled() {
            return Err(ResidentFilterError::Source(
                source_height::CompositionError::Cancelled,
            ));
        }
        if Instant::now() >= deadline {
            return Err(Failure::Deadline.into());
        }
        validate_route_context(&self.session, &self.profile, context)?;
        Ok(())
    }
}

fn point_observations(
    height: &PreparedSourceHeight<'_, '_, '_>,
    context: &AuthorityRequest,
    first_ticket: u64,
    deadline: Instant,
    cancelled: &impl Fn() -> bool,
    retained: Option<benilla_native_nav::SourceOwnerFilter>,
    observe: &mut impl FnMut(u8),
) -> Result<
    (
        [PointNoWaterReply; 2],
        benilla_native_nav::SourceOwnerFilter,
    ),
    ResidentFilterError,
> {
    if first_ticket == 0 || first_ticket.checked_add(1).is_none() {
        return Err(Failure::Invalid.into());
    }
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        return Err(Failure::UnsupportedQuery.into());
    };
    let capabilities = route.owner.capabilities().map_err(|_| Failure::Binding)?;
    let position = [
        route.owner_position.x.bits(),
        route.owner_position.y.bits(),
        route.owner_position.z.bits(),
    ];
    let mut replies = [None, None];
    let mut next = 0usize;
    let mut liquid_query = |mask| {
        let required = match next {
            0 => route_filter::LiquidMask::AllLiquids,
            1 => route_filter::LiquidMask::WaterOrOcean,
            _ => return Err(Failure::Binding.into()),
        };
        if mask != required {
            return Err(Failure::Binding.into());
        }
        observe(next as u8);
        let reply = height
            .point_no_water(
                PointNoWaterRequest {
                    binding: height.binding(),
                    ticket: first_ticket + next as u64,
                    position,
                    mask: if next == 0 { 15 } else { 10 },
                },
                deadline,
                cancelled,
            )
            .map_err(ResidentFilterError::Source)?;
        replies[next] = Some(reply);
        next += 1;
        route_filter::NoWater::try_from_status(0).map_err(|_| Failure::Water.into())
    };
    let filter = if let Some(retained) = retained {
        liquid_query(route_filter::LiquidMask::AllLiquids)?;
        liquid_query(route_filter::LiquidMask::WaterOrOcean)?;
        // Both predicates are false, so updateFilter performs no read/write
        // of include flags and never calls getNavTerrain.
        retained
    } else {
        let created = route_filter::create_dry_filter(capabilities, liquid_query).map_err(
            |error| match error {
                route_filter::Error::UnsupportedCapabilities => {
                    ResidentFilterError::Context(Failure::UnsupportedQuery)
                }
                route_filter::Error::Liquid(error) => error,
            },
        )?;
        benilla_native_nav::SourceOwnerFilter {
            include_flags: created.include_flags(),
            exclude_flags: created.exclude_flags(),
        }
    };

    Ok((
        [
            replies[0].ok_or(Failure::Binding)?,
            replies[1].ok_or(Failure::Binding)?,
        ],
        filter,
    ))
}
