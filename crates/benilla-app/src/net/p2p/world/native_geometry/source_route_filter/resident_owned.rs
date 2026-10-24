// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only retained query owner. No Service, publication callback or replacement API.
use super::super::super::{
    core::resident_dispatch::ResidentDispatchJob,
    products::source_height::{
        inventory::{InventoryCompatibility, OwnedInventoryCompatibility},
        owned::{
            freshness::{CaptureFreshnessWorker, EnqueueError, FreshnessObservation},
            OwnedSourceProducts,
        },
    },
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_route_resident_local::ledger::ResidentLedger,
};
use super::*;
use benilla_formats::NativeInventorySource;
use benilla_world_runtime::{native::Facts, PlayerId};
use bevy::prelude::World;
use std::sync::Arc;

/// Borrowed only by one temporary provider view. It proves immutable context
/// validation, never source freshness, physics clearance or publication authority.
pub(super) struct InvocationContext<'call> {
    job: &'call ResidentDispatchJob<'call>,
    lease: super::super::super::core::JobLease,
}
impl<'call> InvocationContext<'call> {
    fn bind(
        job: &'call ResidentDispatchJob<'call>,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    ) -> Result<Self, ResidentFilterError> {
        job.require_provider(provider, &provider.session, &provider.profile)?;
        validate_route_context(&provider.session, &provider.profile, job.request())?;
        job.request()
            .require_pathfinder_dispatch(&provider.profile)
            .map_err(|_| Failure::Binding)?;
        job.require_provider(provider, &provider.session, &provider.profile)?;
        Ok(Self {
            job,
            lease: job.lease(),
        })
    }

    pub(super) fn matches(
        &self,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) -> Result<bool, ResidentFilterError> {
        // A miss may use the original validator; a revoked job must never do so.
        if !self.lease.is_active() || Instant::now() >= self.job.deadline() {
            return Err(Failure::Deadline.into());
        }
        if !self.lease.same_job(&self.job.lease()) {
            return Err(Failure::Binding.into());
        }
        match self
            .job
            .require_provider(provider, &provider.session, &provider.profile)
        {
            Ok(()) => (),
            Err(Failure::Binding) => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        if self.job.request() != context {
            return Ok(false);
        }
        let Query::SourceMotion(MotionQuery::Route(saved)) = &self.job.request().query else {
            return Ok(false);
        };
        let Query::SourceMotion(MotionQuery::Route(current)) = &context.query else {
            return Ok(false);
        };
        Ok(saved
            .pathfinder_dispatch
            .as_ref()
            .zip(current.pathfinder_dispatch.as_ref())
            .is_some_and(|(saved, current)| saved.same_live_dispatch(current)))
    }
}

pub(crate) struct OwnedResident {
    products: Arc<OwnedSourceProducts>,
    inventory: OwnedInventoryCompatibility,
    state: ResidentFilterState,
    ledger: ResidentLedger,
    identity: ResidentProviderIdentity,
    player: PlayerId,
    transport: Arc<transport::ResidentTransportSelection>,
}
impl OwnedResident {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn bind(
        products: OwnedSourceProducts,
        inventory: OwnedInventoryCompatibility,
        session: &Session,
        profile: &Profile,
        player: PlayerId,
        navigation: SourceMapQuerySnapshot,
        policy: ResidentGeometryPolicy,
        installed: &NativeInventorySource,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Self, ResidentFilterError> {
        let (state, ledger, identity) = products
            .with_height(
                || cancelled() || Instant::now() >= deadline,
                |height| {
                    let result: Result<_, ResidentFilterError> = (|| {
                        let compatibility = inventory
                            .reborrow(height, deadline, &cancelled)
                            .map_err(ResidentFilterError::Source)?;
                        let provider = ResidentFilterProvider::bind_with_policy(
                            session, profile, height, navigation, policy,
                        )?;
                        provider.verify_installed_owner(
                            &compatibility,
                            installed,
                            deadline,
                            &cancelled,
                        )?;
                        let ledger = ResidentLedger::bind(
                            session,
                            profile,
                            player,
                            &provider,
                            &compatibility,
                            deadline,
                        )?;
                        let identity = provider.ledger_identity(session, profile)?;
                        let ResidentStateStorage::Owned(state, _) = provider.state else {
                            unreachable!("bind creates owned state")
                        };
                        Ok((state, ledger, identity))
                    })();
                    Ok(result)
                },
            )
            .map_err(ResidentFilterError::Source)??;
        let transport = Arc::new(transport::ResidentTransportSelection::new(
            session,
            profile,
            player,
            &identity,
            ledger.transport_identity(),
        ));
        Ok(Self {
            transport,
            products: Arc::new(products),
            inventory,
            state,
            ledger,
            identity,
            player,
        })
    }

    /// Read-only source observations. No mutable ledger or query escapes.
    pub(in super::super::super) fn inspect<T>(
        &mut self,
        installed: &NativeInventorySource,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        read: impl for<'view> FnOnce(
            &ResidentFilterProvider<'_, 'view, 'view, 'static>,
            &InventoryCompatibility<'_, 'view, 'view, 'static>,
            &ResidentLedger,
        ) -> Result<T, ResidentFilterError>,
    ) -> Result<T, ResidentFilterError> {
        self.calculate(
            installed,
            deadline,
            cancelled,
            |provider, compatibility, ledger| read(provider, compatibility, ledger),
        )
    }

    /// Only private bounded operations use the mutable view. A failed calculation
    /// may retain constructor work/staging, but cannot promote a ledger head.
    fn calculate<T>(
        &mut self,
        installed: &NativeInventorySource,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        calculate: impl for<'view> FnOnce(
            &mut ResidentFilterProvider<'_, 'view, 'view, 'static>,
            &InventoryCompatibility<'_, 'view, 'view, 'static>,
            &mut ResidentLedger,
        ) -> Result<T, ResidentFilterError>,
    ) -> Result<T, ResidentFilterError> {
        self.calculate_with_job(installed, deadline, cancelled, None, calculate)
    }

    fn calculate_with_job<T>(
        &mut self,
        installed: &NativeInventorySource,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        job: Option<&ResidentDispatchJob<'_>>,
        calculate: impl for<'view> FnOnce(
            &mut ResidentFilterProvider<'_, 'view, 'view, 'static>,
            &InventoryCompatibility<'_, 'view, 'view, 'static>,
            &mut ResidentLedger,
        ) -> Result<T, ResidentFilterError>,
    ) -> Result<T, ResidentFilterError> {
        let Self {
            products,
            inventory,
            state,
            ledger,
            ..
        } = self;
        products
            .with_height(
                || cancelled() || Instant::now() >= deadline,
                |height| {
                    let result: Result<_, ResidentFilterError> = (|| {
                        let compatibility = inventory
                            .reborrow(height, deadline, &cancelled)
                            .map_err(ResidentFilterError::Source)?;
                        let mut provider = ResidentFilterProvider {
                            state: ResidentStateStorage::Borrowed(state),
                            height,
                            invocation: None,
                        };
                        provider.verify_installed_owner(
                            &compatibility,
                            installed,
                            deadline,
                            &cancelled,
                        )?;
                        if let Some(job) = job {
                            provider.invocation = Some(InvocationContext::bind(job, &provider)?);
                        }
                        let result = calculate(&mut provider, &compatibility, ledger)?;
                        provider.verify_installed_owner(
                            &compatibility,
                            installed,
                            deadline,
                            &cancelled,
                        )?;
                        Ok(result)
                    })();
                    Ok(result)
                },
            )
            .map_err(ResidentFilterError::Source)?
    }

    pub(crate) fn constructor_identity(
        &mut self,
        installed: &NativeInventorySource,
        context: &AuthorityRequest,
        deadline: Instant,
    ) -> Result<(usize, benilla_native_nav::SourceMapQuerySlot, usize), ResidentFilterError> {
        self.calculate(
            installed,
            deadline,
            || false,
            |provider, _, _| provider.retained_constructor_for_test(context),
        )
    }

    /// Concrete cold certification stages only; an ordered actual commit ACK promotes it.
    pub(crate) fn certify_cold(
        &mut self,
        world: &mut World,
        installed: &NativeInventorySource,
        context: &AuthorityRequest,
        player: PlayerId,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.calculate(
            installed,
            deadline,
            &cancelled,
            |provider, compatibility, ledger| {
                Self::cold_in_view(
                    provider,
                    compatibility,
                    ledger,
                    world,
                    context,
                    player,
                    deadline,
                    &cancelled,
                )
            },
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn cold_in_view(
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        ledger: &mut ResidentLedger,
        world: &mut World,
        context: &AuthorityRequest,
        player: PlayerId,
        deadline: Instant,
        cancelled: &impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        let constructor = provider.observe(context, 101, deadline, cancelled)?;
        let los =
            provider.observe_chase_los(&constructor, context, player, 103, deadline, cancelled)?;
        let water = provider.observe_chase_water(&los, context, 104, deadline, cancelled)?;
        ledger.certify(
            world,
            compatibility,
            provider,
            context,
            constructor,
            water,
            105,
            deadline,
            cancelled,
        )
    }

    pub(crate) fn certify_retained(
        &mut self,
        world: &mut World,
        installed: &NativeInventorySource,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
    ) -> Result<Facts, ResidentFilterError> {
        self.calculate(
            installed,
            deadline,
            &cancelled,
            |provider, compatibility, ledger| {
                ledger.certify_retained(
                    world,
                    compatibility,
                    provider,
                    context,
                    201,
                    deadline,
                    &cancelled,
                )
            },
        )
    }

    /// A drained worker observation permits one synchronous attempt, not a cached
    /// certificate. Callers advance physics before this call; every ECS resource
    /// is reacquired by the existing certifier, and no borrow crosses the poll.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn certify_observed(
        &mut self,
        observation: FreshnessObservation,
        job: ResidentDispatchJob<'_>,
        world: &mut World,
        installed: &NativeInventorySource,
    ) -> Result<Facts, ResidentFilterError> {
        job.require_resident(self)?;
        let player = job.player();
        // bind creates this unique allocation; moving the resident preserves it.
        let products = Arc::clone(&self.products);
        observation.with_current(
            &products,
            &job.lease(),
            job.deadline(),
            |context, deadline, cancelled| {
                job.require_resident(self)?;
                if !std::ptr::eq(context, job.request()) || job.profile() != &self.state.profile {
                    return Err(Failure::Binding.into());
                }
                let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                    return Err(Failure::Binding.into());
                };
                self.calculate_with_job(
                    installed,
                    deadline,
                    cancelled,
                    Some(&job),
                    |provider, compatibility, ledger| {
                        if route.request.prior_cache.is_none() {
                            Self::cold_in_view(
                                provider,
                                compatibility,
                                ledger,
                                world,
                                context,
                                player,
                                deadline,
                                &cancelled,
                            )
                        } else {
                            ledger.certify_retained(
                                world,
                                compatibility,
                                provider,
                                context,
                                201,
                                deadline,
                                cancelled,
                            )
                        }
                    },
                )
            },
        )
    }

    /// Only the provider minted this identity during bind; no source I/O or
    /// synthetic allocation token is introduced when capturing a dispatch.
    pub(crate) fn dispatch_selection(
        &self,
    ) -> (&ResidentProviderIdentity, &Session, &Profile, PlayerId) {
        (
            &self.identity,
            &self.state.session,
            &self.state.profile,
            self.player,
        )
    }

    pub(crate) fn enqueue_observation(
        &self,
        job: ResidentDispatchJob<'_>,
        worker: &mut CaptureFreshnessWorker,
    ) -> Result<(), ResidentFilterError> {
        job.require_resident(self)?;
        worker
            .try_enqueue(Arc::clone(&self.products), job.lease())
            .map_err(|error| {
                match error {
                    EnqueueError::Busy => Failure::QueueCapacity,
                    EnqueueError::Inactive => Failure::Deadline,
                    EnqueueError::Stopped => Failure::Binding,
                }
                .into()
            })
    }

    pub(crate) fn initialize(&mut self, boundary: &RouteCommitBoundary) -> Result<(), Failure> {
        self.ledger.initialize(boundary)
    }
    /// Historical ACK is the existing atomic promotion. No fallible freshness check
    /// follows it; changed source cannot turn a successful publication into an error.
    pub(crate) fn acknowledge(&mut self, notice: &RouteCommitNotice) -> Result<(), Failure> {
        self.ledger.acknowledge(notice)
    }
}

#[path = "resident_owned/tests.rs"]
#[cfg(test)]
mod tests;

#[path = "resident_owned/transport.rs"]
pub(crate) mod transport;
