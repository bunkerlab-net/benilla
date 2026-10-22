// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded reconstruction by actual source execution from a checked cache-free anchor.
//! Serialized replies are comparison outputs, never answers to a live query.
use super::{
    core::{Failure, Session},
    route_commit::{RouteCommitBoundary, RouteCommitNotice},
    source_route_same_polygon::SourceSamePolygonRouteProvider,
};
use benilla_world_runtime::native::{Authority, NativeError, NativeWorld, OwnedInput};
use benilla_world_store::native::reconstruction::{Recipe, RecipeStore};

/// Private owner seam, not a provider extension or saved-reply resolver.
pub(super) trait ReconstructionRouteOwner {
    fn initialize_reconstruction(
        &mut self,
        session: &Session,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure>;
    fn acknowledge(&mut self, session: &Session, notice: &RouteCommitNotice)
        -> Result<(), Failure>;
}
impl ReconstructionRouteOwner for SourceSamePolygonRouteProvider {
    fn initialize_reconstruction(
        &mut self,
        session: &Session,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        SourceSamePolygonRouteProvider::initialize_reconstruction(self, session, boundary)
    }
    fn acknowledge(
        &mut self,
        session: &Session,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        SourceSamePolygonRouteProvider::acknowledge(self, session, notice)
    }
}

#[cfg(all(test, not(benilla_headless_geometry)))]
#[path = "source_cache_reconstruction/resident.rs"]
pub(super) mod resident;

/// Implemented only by the local owner of the concrete loaded providers. It does
/// not grant raw-content admission or create a native cache from stored refs.
pub(super) trait ReconstructionAuthority: Authority {
    fn route_state(
        &mut self,
    ) -> (
        &Session,
        &mut dyn ReconstructionRouteOwner,
        &mut RouteCommitBoundary,
    );
}

/// Inputs are owned until every record and the final checkpoint match. On error
/// all fresh runtime/query objects and the store lock are dropped together.
pub(super) fn reconstruct<A: ReconstructionAuthority>(
    store: RecipeStore,
    recipe: &Recipe,
    mut world: NativeWorld,
    mut authority: A,
) -> Result<(RecipeStore, NativeWorld, A), NativeError> {
    if store.requires_reopen()
        || store.recipe() != recipe
        || world.state().tick != recipe.anchor().identity().tick
        || !world.records().is_empty()
        || world.checkpoint()? != *recipe.anchor()
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    world.validate_saved_session_reopen()?;
    {
        let (session, provider, boundary) = authority.route_state();
        boundary.validate_world(&world)?;
        provider
            .initialize_reconstruction(session, boundary)
            .map_err(|_| NativeError::AuthorityBinding)?;
    }
    for expected in recipe.records() {
        let inputs = expected
            .inputs
            .iter()
            .cloned()
            .map(|input| OwnedInput {
                authenticated_owner: input.player.owner,
                input,
            })
            .collect();
        // This calls the actual authority for Prepare, native calculations and
        // fresh collision observations. In particular it cannot use ReplayAuthority.
        let actual = world.step_batch(expected.tick, inputs, &mut authority)?;
        if actual != expected {
            return Err(NativeError::ReplayMismatch);
        }
        let (session, provider, boundary) = authority.route_state();
        let notice = RouteCommitNotice::capture(&world, boundary)?;
        provider
            .acknowledge(session, &notice)
            .map_err(|_| NativeError::AuthorityBinding)?;
        *boundary = notice.boundary().clone();
    }
    let checkpoint = world.checkpoint()?;
    if checkpoint != *recipe.expected_checkpoint() {
        return Err(NativeError::ReplayMismatch);
    }
    authority.route_state().2.validate_world(&world)?;
    world.checkpoint_committed(&checkpoint)?;
    Ok((store, world, authority))
}
