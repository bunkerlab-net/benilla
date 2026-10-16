// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright CMaNGOS contributors; upstream GPL-2.0-or-later, Rust GPL-3.0-or-later.
// CMaNGOS 8ec338a1 TargetedMovementGenerator.cpp:482-500. Upstream GPL-2.0-or-later.
//! Modeled Chase constructor dispatch, not a native query allocation or cache proof.
use super::{MotionError, QueryBinding, RouteKind, RouteRequest};
use serde::{Deserialize, Deserializer, Serialize};

/// Transcript identity of the actual first dispatch and each retained use. Fields
/// are private: live construction originates in MotionState::dispatch. Restored
/// transcript data has no authority to rehydrate any loaded native query.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PathfinderDispatch {
    construction: QueryBinding,
    dispatch_query_id: u64,
    dispatch_now_ms: u64,
}
impl PathfinderDispatch {
    pub(super) fn new(
        construction: Option<QueryBinding>,
        dispatch: QueryBinding,
    ) -> Result<Self, MotionError> {
        let result = Self {
            construction: construction.unwrap_or(dispatch),
            dispatch_query_id: dispatch.query_id,
            dispatch_now_ms: dispatch.now_ms,
        };
        result.validate_for(dispatch)?;
        Ok(result)
    }
    pub fn construction_binding(self) -> QueryBinding {
        self.construction
    }
    pub fn dispatch_binding(self) -> QueryBinding {
        QueryBinding {
            identity: self.construction.identity,
            query_id: self.dispatch_query_id,
            now_ms: self.dispatch_now_ms,
        }
    }
    pub fn first_construction(self) -> bool {
        self.construction == self.dispatch_binding()
    }
    pub fn validate_for(self, binding: QueryBinding) -> Result<(), MotionError> {
        if self.dispatch_binding() != binding
            || self.construction.identity != binding.identity
            || self.construction.query_id == 0
            || self.construction.query_id > binding.query_id
            || self.construction.now_ms > binding.now_ms
            || (self.construction.query_id == binding.query_id && self.construction != binding)
        {
            return Err(MotionError::Invalid(
                "pathfinder constructor dispatch binding",
            ));
        }
        Ok(())
    }
}
impl RouteRequest {
    /// Source cache admission needs the explicit constructor event, not an
    /// inference from absent cache state. Native query ownership is checked by
    /// the provider; this only validates the modeled transcript transition.
    pub(super) fn validate_source_cache_dispatch(&self) -> Result<(), MotionError> {
        self.validate_pathfinder_dispatch()?;
        let dispatch = self.pathfinder_dispatch.ok_or(MotionError::Missing(
            "source pathfinder constructor dispatch",
        ))?;
        if dispatch.first_construction() != self.prior_cache.is_none() {
            return Err(MotionError::Invalid(
                "source pathfinder cache dispatch phase",
            ));
        }
        Ok(())
    }
    /// Required presence is distinct from source/native ownership. Authored direct
    /// requests may carry null, but actual Chase dispatch always supplies the event.
    pub fn validate_pathfinder_dispatch(&self) -> Result<(), MotionError> {
        match (&self.pathfinder_dispatch, self.kind) {
            (None, RouteKind::HomeForceDestination) => Ok(()),
            (Some(_), RouteKind::HomeForceDestination) => {
                Err(MotionError::Invalid("Home cannot retain Chase constructor"))
            }
            (Some(dispatch), _) => dispatch.validate_for(self.binding),
            (None, _) => Err(MotionError::Missing("Chase constructor dispatch")),
        }
    }
}
pub(super) fn required_option<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(deserializer)
}
