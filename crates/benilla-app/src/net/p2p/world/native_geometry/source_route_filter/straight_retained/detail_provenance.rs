// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only attribution of this attempted terminal, never a second calculation.
use super::super::super::super::super::products::nav_generation::GeneratedQueryProvenance;
use super::*;
use benilla_native_nav::LoadedSourceRetainedStraightDetail;

impl ResidentFilterProvider<'_, '_, '_, '_> {
    /// Inspect a sealed native witness and historical generated-loader receipt.
    /// The private inspector must only build its return value, without emitting
    /// it; that value is returned only after the same attempt remains fresh.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn inspect_straight_detail_for_test<T>(
        &mut self,
        dependency: &ResidentJoinedStraightDependency,
        origin: &ResidentJoinedColdDependency,
        context: &AuthorityRequest,
        deadline: Instant,
        cancelled: impl Fn() -> bool,
        inspect: impl FnOnce(
            &LoadedSourceRetainedStraightDetail,
            &GeneratedQueryProvenance,
        ) -> Result<T, ResidentFilterError>,
    ) -> Result<T, ResidentFilterError> {
        let deadline = dependency.deadline(deadline);
        self.straight_for(dependency, origin, context, deadline, &cancelled)?;
        let witness = {
            // This later-use constructor is already retained and selected. Reborrowing
            // its exact query/endpoints neither allocates a constructor nor calculates
            // another terminal or changes the original native path/backing storage.
            let mut constructor = self
                .query
                .retain_constructor(context)
                .map_err(ResidentFilterError::Query)?;
            let selected = constructor
                .select_for_calculate(deadline)
                .map_err(ResidentFilterError::Query)?;
            let witness = dependency
                .terminal
                .detail_height_witness(&selected, Default::default(), deadline)
                .map_err(ResidentFilterError::StraightDetail)?;
            witness
                .check_terminal(&dependency.terminal)
                .map_err(native_error)?;
            witness.check_query(&selected).map_err(native_error)?;
            witness
        };
        let generated = self
            .query
            .generated_provenance_for_test()
            .map_err(ResidentFilterError::Query)?
            .ok_or(Failure::MissingGeometry)?;
        let report = inspect(&witness, generated)?;
        self.straight_for(dependency, origin, context, deadline, &cancelled)?;
        Ok(report)
    }
}
