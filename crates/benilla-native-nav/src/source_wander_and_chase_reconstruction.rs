// SPDX-License-Identifier: GPL-3.0-or-later
//! Two independent native constructor replays on one freshly loaded map query.
//!
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f PathFinder.cpp:37-55:
//! each constructor owns its own zero cache/current-query state while retaining
//! the map-instance query. ComputePathToRandomPoint and calculate keep their
//! respective operation order; this composition does not replay owner dispatch.
use super::*;
use crate::{
    ReconstructedSourceWanderTerminal, SourceWanderReconstructionChallenge,
    SourceWanderReconstructionError, SourceWanderReconstructionRecipe,
    SourceWanderReconstructionStage,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceWanderAndChaseReconstructionError {
    Native(Error),
    Wander(SourceWanderReconstructionError),
    Chase(SourceChaseFixedCacheError),
}
impl From<Error> for SourceWanderAndChaseReconstructionError {
    fn from(value: Error) -> Self {
        Self::Native(value)
    }
}
impl From<SourceWanderReconstructionError> for SourceWanderAndChaseReconstructionError {
    fn from(value: SourceWanderReconstructionError) -> Self {
        Self::Wander(value)
    }
}
impl From<SourceChaseFixedCacheError> for SourceWanderAndChaseReconstructionError {
    fn from(value: SourceChaseFixedCacheError) -> Self {
        Self::Chase(value)
    }
}

/// Opaque native composition: two typed certificates, independent constructor
/// caches and challenges, and the single fresh query that both actually used.
/// No stored references or preexisting certificate can construct this object.
///
/// The recipes remain inert inputs. Actual creature/construction identity,
/// accepted revisions, dispatch, inventory and expected complete semantics still
/// require a separate owner boundary. Native composition grants no app restore,
/// gameplay Route, RNG, timer or ACK authority.
///
/// ```compile_fail
/// use benilla_native_nav::{
///     ReconstructedSourceWanderAndChase, ReconstructedSourceWanderTerminal,
///     ReconstructedSourceChaseFixedCache, SourceMapQuery,
/// };
/// fn forge(query: SourceMapQuery, wander: ReconstructedSourceWanderTerminal,
///          chase: ReconstructedSourceChaseFixedCache) -> ReconstructedSourceWanderAndChase {
///     ReconstructedSourceWanderAndChase { query, wander, chase }
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{ReconstructedSourceWanderAndChase, ReconstructedSourceWanderTerminal};
/// fn swap_owners(pair: &ReconstructedSourceWanderAndChase) -> &ReconstructedSourceWanderTerminal {
///     pair.chase()
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{ReconstructedSourceWanderAndChase, SourceChaseFixedCacheChallenge};
/// fn swap_challenges(pair: &ReconstructedSourceWanderAndChase,
///                    chase: &SourceChaseFixedCacheChallenge) -> bool {
///     pair.wander().matches_challenge(chase)
/// }
/// ```
///
/// ```compile_fail
/// use benilla_native_nav::{SourceMapQuery, SourceChaseFixedCacheRecipe,
///     SourceChaseFixedCacheChallenge};
/// fn reuse(query: &SourceMapQuery, recipe: &SourceChaseFixedCacheRecipe,
///          challenge: &SourceChaseFixedCacheChallenge, until: std::time::Instant) {
///     query.replay_chase_fixed_cache_observed(recipe, challenge, until, |_| Ok(()));
/// }
/// ```
#[derive(Debug)]
pub struct ReconstructedSourceWanderAndChase {
    query: SourceMapQuery,
    wander: ReconstructedSourceWanderTerminal,
    chase: ReconstructedSourceChaseFixedCache,
}
impl ReconstructedSourceWanderAndChase {
    pub fn query(&self) -> &SourceMapQuery {
        &self.query
    }
    pub fn wander(&self) -> &ReconstructedSourceWanderTerminal {
        &self.wander
    }
    pub fn chase(&self) -> &ReconstructedSourceChaseFixedCache {
        &self.chase
    }
    /// Both exact allocation identities must match, even for byte-identical maps.
    pub fn check_query(&self, query: &SourceMapQuery) -> Result<(), Error> {
        self.wander.retained_query().check_query(query)?;
        self.chase.retained_query().check_query(query)
    }
    pub fn matches_challenges(
        &self,
        wander: &SourceWanderReconstructionChallenge,
        chase: &SourceChaseFixedCacheChallenge,
    ) -> bool {
        self.wander.matches_challenge(wander) && self.chase.matches_challenge(chase)
    }
    pub fn into_parts(
        self,
    ) -> (
        SourceMapQuery,
        ReconstructedSourceWanderTerminal,
        ReconstructedSourceChaseFixedCache,
    ) {
        (self.query, self.wander, self.chase)
    }
}

impl SourceMapQuery {
    /// Load exactly one fresh source1024 allocation, then replay separate Wander
    /// and fixed-cache NORMAL Chase constructors. Both recipe shapes are checked
    /// before loading. Each owner keeps its own filter, config, cache and count;
    /// neither terminal is used to seed the other owner's constructor.
    pub fn reconstruct_wander_and_chase(
        slot: SourceMapQuerySlot,
        params: &[u8],
        tiles: &[TileInput<'_>],
        wander_recipe: &SourceWanderReconstructionRecipe,
        wander_challenge: &SourceWanderReconstructionChallenge,
        chase_recipe: &SourceChaseFixedCacheRecipe,
        chase_challenge: &SourceChaseFixedCacheChallenge,
        until: Instant,
    ) -> Result<ReconstructedSourceWanderAndChase, SourceWanderAndChaseReconstructionError> {
        Self::reconstruct_wander_and_chase_observed(
            slot,
            params,
            tiles,
            wander_recipe,
            wander_challenge,
            chase_recipe,
            chase_challenge,
            until,
            |_| Ok(()),
            |_| Ok(()),
        )
    }
    /// Read-only rejection points preserve each existing replay's source order.
    /// Wander finishes first, then Chase starts from its own cold constructor.
    /// Failure drops the factory's private candidates; no partial pair is returned.
    pub fn reconstruct_wander_and_chase_observed(
        slot: SourceMapQuerySlot,
        params: &[u8],
        tiles: &[TileInput<'_>],
        wander_recipe: &SourceWanderReconstructionRecipe,
        wander_challenge: &SourceWanderReconstructionChallenge,
        chase_recipe: &SourceChaseFixedCacheRecipe,
        chase_challenge: &SourceChaseFixedCacheChallenge,
        until: Instant,
        wander_observer: impl FnMut(
            SourceWanderReconstructionStage<'_>,
        ) -> Result<(), SourceWanderReconstructionError>,
        chase_observer: impl FnMut(
            SourceChaseFixedCacheStage<'_>,
        ) -> Result<(), SourceChaseFixedCacheError>,
    ) -> Result<ReconstructedSourceWanderAndChase, SourceWanderAndChaseReconstructionError> {
        wander_recipe.validate()?;
        chase_recipe.validate()?;
        crate::deadline(until)?;
        if slot != wander_recipe.slot || slot != chase_recipe.slot {
            return Err(Error::Identity.into());
        }
        let query = Self::load(slot, params, tiles)?;
        crate::deadline(until)?;
        if query.query_capacity() != wander_recipe.query_capacity
            || query.query_capacity() != chase_recipe.query_capacity
        {
            return Err(Error::Identity.into());
        }
        let wander = query.reconstruct_wander_observed(
            wander_recipe,
            wander_challenge,
            until,
            wander_observer,
        )?;
        let chase = query.replay_chase_fixed_cache_observed(
            chase_recipe,
            chase_challenge,
            until,
            chase_observer,
        )?;
        let result = ReconstructedSourceWanderAndChase {
            query,
            wander,
            chase,
        };
        result.check_query(result.query())?;
        crate::deadline(until)?;
        Ok(result)
    }
}
