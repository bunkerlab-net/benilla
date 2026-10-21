// SPDX-License-Identifier: GPL-3.0-or-later
//! Historical ACK remains bookkeeping after a controlled authored source change.
use super::{
    products::{source_height::CompositionError, vmap::installed_source_fixture as input},
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::ResidentLedger,
};
use benilla_world_runtime::native::AuthorityRequest;
use std::{collections::BTreeMap, time::Instant};

/// Restore this authored fixture only after the worker's no-replay measurements
/// and direct fresh-source refusal, or during unwinding. Never touches a player install.
pub(super) struct SourceChange<'a> {
    install: &'a input::Install,
    original: Vec<(String, Vec<u8>)>,
    before: BTreeMap<String, Vec<u8>>,
}
impl<'a> SourceChange<'a> {
    pub(super) fn begin(
        install: &'a input::Install,
        ledger: &ResidentLedger,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
        deadline: Instant,
    ) -> Self {
        let before = ledger.diagnostic();
        ledger
            .check_fresh_candidate(provider, context, deadline)
            .expect("the original candidate is source-valid before mutation");
        assert!(Instant::now() < deadline);
        let result = Self {
            install,
            original: input::navigation_entries(),
            before: install.bytes(),
        };
        let mut changed = result.original.clone();
        changed
            .iter_mut()
            .find(|(name, _)| name == input::WDT)
            .unwrap()
            .1[20] ^= 1;
        install.write(&changed);
        assert_ne!(install.bytes(), result.before);
        assert!(matches!(
            ledger.check_fresh_candidate(provider, context, deadline),
            Err(ResidentFilterError::Source(CompositionError::TerrainSource))
        ));
        assert!(
            Instant::now() < deadline,
            "expiry cannot mask source mutation"
        );
        assert_eq!(ledger.diagnostic(), before);
        result
    }

    pub(super) fn refuse_fresh_source(
        &self,
        ledger: &ResidentLedger,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        context: &AuthorityRequest,
    ) {
        assert_ne!(self.install.bytes(), self.before);
        let before = ledger.diagnostic();
        let constructors = provider.retained_constructor_count();
        // Direct source-only negative on the genuine saved first request, NOT a
        // new runtime dispatch or permission to re-emit the historical reply.
        assert!(matches!(
            provider.observe(context, 201, runtime::until(), || false),
            Err(ResidentFilterError::Source(CompositionError::TerrainSource))
        ));
        assert_eq!(provider.retained_constructor_count(), constructors);
        assert_eq!(ledger.diagnostic(), before);
        eprintln!("resident historical ACK: changed source still refuses fresh observations after actual lost-ACK retry");
    }
}
impl Drop for SourceChange<'_> {
    fn drop(&mut self) {
        self.install.write(&self.original);
        assert_eq!(self.install.bytes(), self.before);
    }
}

#[test]
fn resident_controller_historical_ack_survives_changed_source_without_replay() {
    super::source_route_resident_controller::changed_source_ack_probe();
}
