// SPDX-License-Identifier: GPL-3.0-or-later
//! Home's explicit forceDestination=true, straightPath=false same-polygon branch.
//! CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f, HomeMovementGenerator.cpp:61–64
//! and PathFinder.cpp:506–519,964–980. The early NORMAL return precedes the forced
//! endpoint rewrite. Only the common cold BuildPolyPath operations execute here;
//! this grants no fallback, shortcut, retained Chase lineage or movement authority.
use crate::{
    Error, LoadedSourceColdDistanceCompatibility, LoadedSourceSamePolygon, SelectedSourceQuery,
    SourceMapQuerySlot, SourceOwnerFilter, SourceSamePolygonBudget, SourceSamePolygonConfig,
    SourceSamePolygonError,
};
use std::time::Instant;

pub const SOURCE_HOME_SAME_POLYGON_POLICY: &str =
    "cmangos-8ec338a1-home-local-force-true-straight-false-same-poly-early-normal-v1";

/// A fresh Home calculation under its distinct source call flags. There is no
/// conversion to a persistent Chase terminal or constructor accepting cache refs.
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceHomeSamePolygon, LoadedSourceSamePolygon};
/// fn retain_home(home: LoadedSourceHomeSamePolygon) -> LoadedSourceSamePolygon {
///     home.into()
/// }
/// ```
#[derive(Debug)]
pub struct LoadedSourceHomeSamePolygon {
    terminal: LoadedSourceSamePolygon,
    force_destination: bool,
    straight_path: bool,
}
impl LoadedSourceHomeSamePolygon {
    pub fn policy(&self) -> &'static str {
        SOURCE_HOME_SAME_POLYGON_POLICY
    }
    pub fn force_destination(&self) -> bool {
        self.force_destination
    }
    pub fn straight_path(&self) -> bool {
        self.straight_path
    }
    pub fn check_query(&self, selected: &SelectedSourceQuery<'_>) -> Result<(), Error> {
        self.terminal.check_query(selected)
    }
    pub fn compatibility(&self) -> &LoadedSourceColdDistanceCompatibility {
        self.terminal.compatibility()
    }
    pub fn config(&self) -> SourceSamePolygonConfig {
        self.terminal.config()
    }
    pub fn points(&self) -> &[[u32; 3]] {
        self.terminal.points()
    }
    pub fn actual_end(&self) -> [u32; 3] {
        self.terminal.actual_end()
    }
    pub fn path_type(&self) -> u32 {
        self.terminal.path_type()
    }
    pub fn slot(&self) -> SourceMapQuerySlot {
        self.terminal.slot()
    }
    pub fn query_capacity(&self) -> u32 {
        self.terminal.query_capacity()
    }
}
impl SelectedSourceQuery<'_> {
    /// Home passes forceDestination=true and leaves straightPath=false. With the
    /// selected same-polygon early return, neither flag changes BuildPolyPath's
    /// cold lookup or two-point NORMAL terminal. Cross-polygon/missing endpoints
    /// reject before any forced fallback could be inferred.
    pub fn loaded_home_cold_same_polygon_with_filter(
        &self,
        compatibility_version: u32,
        filter: SourceOwnerFilter,
        config: SourceSamePolygonConfig,
        budget: SourceSamePolygonBudget,
        until: Instant,
    ) -> Result<LoadedSourceHomeSamePolygon, SourceSamePolygonError> {
        let force_destination = true;
        let straight_path = false;
        let terminal = self.loaded_cold_same_polygon_with_filter(
            compatibility_version,
            filter,
            config,
            budget,
            until,
        )?;
        Ok(LoadedSourceHomeSamePolygon {
            terminal,
            force_destination,
            straight_path,
        })
    }
}
