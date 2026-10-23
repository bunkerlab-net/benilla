// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only retained query owner. No Service, publication callback or replacement API.
use super::super::super::{
    core::JobLease,
    products::source_height::{
        inventory::{InventoryCompatibility, OwnedInventoryCompatibility},
        owned::{freshness::FreshnessObservation, OwnedSourceProducts},
    },
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_route_resident_local::ledger::ResidentLedger,
};
use super::*;
use benilla_formats::NativeInventorySource;
use benilla_world_runtime::{native::Facts, PlayerId};
use bevy::prelude::World;
use std::sync::Arc;

pub(crate) struct OwnedResident {
    products: Arc<OwnedSourceProducts>,
    inventory: OwnedInventoryCompatibility,
    state: ResidentFilterState,
    ledger: ResidentLedger,
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
        let (state, ledger) = products
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
                        let ResidentStateStorage::Owned(state, _) = provider.state else {
                            unreachable!("bind creates owned state")
                        };
                        Ok((state, ledger))
                    })();
                    Ok(result)
                },
            )
            .map_err(ResidentFilterError::Source)??;
        Ok(Self {
            products: Arc::new(products),
            inventory,
            state,
            ledger,
        })
    }

    /// Read-only source observations. No mutable ledger or query escapes.
    pub(crate) fn inspect<T>(
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
        let Self {
            products,
            inventory,
            state,
            ledger,
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
                        };
                        provider.verify_installed_owner(
                            &compatibility,
                            installed,
                            deadline,
                            &cancelled,
                        )?;
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
                let constructor = provider.observe(context, 101, deadline, &cancelled)?;
                let los = provider.observe_chase_los(
                    &constructor,
                    context,
                    player,
                    103,
                    deadline,
                    &cancelled,
                )?;
                let water =
                    provider.observe_chase_water(&los, context, 104, deadline, &cancelled)?;
                ledger.certify(
                    world,
                    compatibility,
                    provider,
                    context,
                    constructor,
                    water,
                    105,
                    deadline,
                    &cancelled,
                )
            },
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
        lease: &JobLease,
        world: &mut World,
        installed: &NativeInventorySource,
        player: PlayerId,
        invocation_deadline: Instant,
    ) -> Result<Facts, ResidentFilterError> {
        // bind creates this unique allocation; moving the resident preserves it.
        let products = Arc::clone(&self.products);
        observation.with_current(
            &products,
            lease,
            invocation_deadline,
            |context, deadline, cancelled| {
                let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                    return Err(Failure::Binding.into());
                };
                if route.request.prior_cache.is_none() {
                    self.certify_cold(world, installed, context, player, deadline, cancelled)
                } else {
                    self.certify_retained(world, installed, context, deadline, cancelled)
                }
            },
        )
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
