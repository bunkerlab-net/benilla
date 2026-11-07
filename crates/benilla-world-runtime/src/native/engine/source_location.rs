// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
impl<A: Authority> Frame<'_, A> {
    /// A fail-closed scope check before effects, not a synthetic UpdateZone callback.
    pub(super) fn source_location(
        &mut self,
        position: ai::Point,
        orientation: Orientation,
    ) -> Result<(), NativeError> {
        if !super::super::source_location::selected(self.profile) {
            return Ok(());
        }
        let query = SourceLocationQuery::new(
            self.profile,
            self.state,
            self.replies.len() as u64 + 1,
            benilla_sim::creature_motion::Pose {
                position,
                orientation: ai::Real::from_bits(orientation.bits())?,
            },
        )?;
        let Facts::SourceLocation(facts) =
            self.ask(None, Query::SourceLocation(Box::new(query.clone())))?
        else {
            return Err(NativeError::AuthorityBinding);
        };
        facts.validate_for(&query)?;
        facts.validate(self.profile, self.state.world, self.state.player)?;
        self.state.source_location = Some(facts);
        Ok(())
    }
}
