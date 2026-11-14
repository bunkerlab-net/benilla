// SPDX-License-Identifier: GPL-3.0-or-later
//! Component-only owner attachment. No store or controller installation is provided.
use super::super::{
    source_route_filter::resident::ResidentFilterProvider,
    source_route_resident_local::ledger::ResidentLedger,
};
use super::{Failure, ReconstructionRouteOwner, RouteCommitBoundary, RouteCommitNotice, Session};

/// The native provider and ledger cannot be replaced independently after attachment.
/// Height/source borrows remain live for the entire reconstruction owner lifetime.
pub(in super::super) struct ResidentReconstructionOwner<'a, 'terrain, 'bundle, 'source> {
    provider: ResidentFilterProvider<'a, 'terrain, 'bundle, 'source>,
    ledger: ResidentLedger,
    session: Option<Session>,
}
impl<'a, 'terrain, 'bundle, 'source> ResidentReconstructionOwner<'a, 'terrain, 'bundle, 'source> {
    pub(in super::super) fn new(
        provider: ResidentFilterProvider<'a, 'terrain, 'bundle, 'source>,
        ledger: ResidentLedger,
    ) -> Self {
        Self {
            provider,
            ledger,
            session: None,
        }
    }
}
impl ReconstructionRouteOwner for ResidentReconstructionOwner<'_, '_, '_, '_> {
    fn initialize_reconstruction(
        &mut self,
        session: &Session,
        boundary: &RouteCommitBoundary,
    ) -> Result<(), Failure> {
        if self.session.is_some() {
            return Err(Failure::Binding);
        }
        self.ledger
            .initialize_reconstruction(session, &self.provider, boundary)?;
        self.session = Some(session.clone());
        Ok(())
    }
    fn acknowledge(
        &mut self,
        session: &Session,
        notice: &RouteCommitNotice,
    ) -> Result<(), Failure> {
        if self.session.as_ref() != Some(session) {
            return Err(Failure::Binding);
        }
        self.ledger.acknowledge(notice)
    }
}

#[path = "resident/live.rs"]
mod live;

#[cfg(test)]
#[path = "resident/tests.rs"]
mod tests;
