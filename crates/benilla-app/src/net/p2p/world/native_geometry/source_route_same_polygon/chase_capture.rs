// SPDX-License-Identifier: GPL-3.0-or-later
//! ACK-owned original Chase inputs; no decoder or loaded-owner installation seam.
use super::*;
use benilla_native_nav::{SourceChaseFixedCacheRecipe, SourceChaseFixedCacheWitness};
use serde::{Deserialize, Serialize};

const MAX_CAPTURE_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Default)]
pub(super) struct ChaseCapture {
    pub(super) native: SourceChaseFixedCacheWitness,
    pub(super) constructor: Option<AuthorityRequest>,
    pub(super) operations: Vec<ChaseCaptureOperation>,
}

/// Inert inspection of the actual accepted cold/final requests and original count.
/// It cannot import cached refs or grant authority to replay historical dispatches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in super::super) struct ChaseCaptureView {
    pub native: SourceChaseFixedCacheRecipe,
    pub constructor: AuthorityRequest,
    pub operations: Vec<ChaseCaptureOperation>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(in super::super) struct ChaseCaptureOperation {
    pub ordinal: u64,
    pub calculation: AuthorityRequest,
}

impl ChaseCapture {
    pub(super) fn accept(capture: &mut Option<Self>, candidate: &SamePolygonCandidate) {
        if let Some(selected) = capture {
            if selected.record(candidate).is_err() {
                // Only an accepted successor may leave the fixed-cache capture
                // domain. A later retained dispatch cannot restart this owner.
                *capture = None;
            }
        }
    }

    /// Called on an ACK-local clone after the candidate's certified predecessor
    /// has matched. The provider publishes it only after every owner validates.
    pub(super) fn record(&mut self, candidate: &SamePolygonCandidate) -> Result<(), Failure> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &candidate.context.query else {
            return Err(Failure::Binding);
        };
        let terminal = candidate.terminal.chase().ok_or(Failure::Binding)?;
        let ordinal = self.native.operation_count();
        if !matches!(
            route.request.kind,
            motion::RouteKind::ChaseInitial | motion::RouteKind::ChaseReplan
        ) || candidate.dispatch.constructor_kind()
            != motion::PathfinderConstructorKind::ChaseLazy
            || route.request.target.is_none()
            || route.request.wander.is_some()
            || route.request.launch_origin != route.request.origin
            || route
                .request
                .prior_cache
                .as_ref()
                .map_or(0, |cache| cache.revision)
                != ordinal
            || ordinal.checked_add(1) != Some(candidate.after_cache.revision)
            || terminal.compatibility().from() != bits(route.request.origin)
            || terminal.compatibility().to() != bits(route.request.goal)
        {
            return Err(Failure::Binding);
        }
        match &self.constructor {
            None if ordinal == 0
                && candidate.dispatch.first_construction()
                && route.request.prior_cache.is_none() => {}
            Some(constructor) if ordinal != 0 && !candidate.dispatch.first_construction() => {
                let Query::SourceMotion(MotionQuery::Route(first)) = &constructor.query else {
                    return Err(Failure::Binding);
                };
                if constructor.binding.npc != candidate.context.binding.npc
                    || first
                        .request
                        .pathfinder_dispatch
                        .ok_or(Failure::Binding)?
                        .construction_binding()
                        != candidate.dispatch.construction_binding()
                {
                    return Err(Failure::Binding);
                }
            }
            _ => return Err(Failure::Binding),
        }
        self.native.record(terminal).map_err(|_| Failure::Binding)?;
        let recipe = self.native.recipe().map_err(|_| Failure::Binding)?;
        self.constructor
            .get_or_insert_with(|| candidate.context.clone());
        self.operations.push(ChaseCaptureOperation {
            ordinal,
            calculation: candidate.context.clone(),
        });
        self.operations.retain(|operation| {
            recipe
                .operations
                .iter()
                .any(|input| input.ordinal == operation.ordinal)
        });
        if serde_json::to_vec(&(&self.constructor, &self.operations))
            .map_err(|_| Failure::Invalid)?
            .len()
            > MAX_CAPTURE_BYTES
        {
            return Err(Failure::Invalid);
        }
        Ok(())
    }

    fn view(&self) -> Option<ChaseCaptureView> {
        Some(ChaseCaptureView {
            native: self.native.recipe().ok()?,
            constructor: self.constructor.clone()?,
            operations: self.operations.clone(),
        })
    }
}

#[cfg(not(benilla_headless_geometry))]
#[path = "chase_capture_tests.rs"]
pub(in super::super) mod tests;

impl SourceSamePolygonRouteProvider {
    /// Test-only capture inspection grants neither encoding nor reopen authority.
    pub(in super::super) fn chase_reconstruction_capture_for(
        &self,
        npc: NpcId,
        construction: motion::QueryBinding,
    ) -> Option<ChaseCaptureView> {
        self.candidates
            .iter()
            .find(|entry| entry.npc == npc && entry.construction == construction)?
            .chase_reconstruction
            .as_ref()?
            .view()
    }
}
