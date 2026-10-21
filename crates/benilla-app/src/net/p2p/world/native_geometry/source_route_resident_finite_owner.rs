// SPDX-License-Identifier: GPL-3.0-or-later
//! Owner-thread diagnostics for a finite component loop. Only actual cloned
//! requests and notices cross the controller bridge; source/native/scene owners do not.
use super::{
    core::Failure,
    products::{
        source_height::{
            inventory::InventoryCompatibility, CompositionError, PreparedSourceHeight,
        },
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    route_commit::{authored_conflicts as wrong, RouteCommitBoundary, RouteCommitNotice},
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_finite_loop::{self as fixture, Mode},
    source_route_resident_history::SourceChange,
    source_route_resident_local::ledger::{
        Diagnostic, ProposalLiveness, ResidentLedger, WarmLiveness,
    },
};
use benilla_world_runtime::native::{
    AuthorityRequest, Facts, MotionQuery, NativeError, Profile, Query,
};
use std::time::{Duration, Instant};

pub(super) struct Owner<'a> {
    mode: Mode,
    accepted: Vec<RouteCommitNotice>,
    attempts: Vec<(AuthorityRequest, Instant)>,
    root: Option<Diagnostic>,
    prior: Option<WarmLiveness>,
    proposals: Vec<ProposalLiveness>,
    retired: Vec<ProposalLiveness>,
    last_deadline: Option<Instant>,
    deliveries: usize,
    expired: usize,
    lost: bool,
    old_boundary_refused: bool,
    change: Option<SourceChange<'a>>,
}
impl<'a> Owner<'a> {
    pub(super) fn new(mode: Mode) -> Self {
        Self {
            mode,
            accepted: vec![],
            attempts: vec![],
            root: None,
            prior: None,
            proposals: vec![],
            retired: vec![],
            last_deadline: None,
            deliveries: 0,
            expired: 0,
            lost: false,
            old_boundary_refused: false,
            change: None,
        }
    }
    pub(super) fn failure_tick(&self) -> u64 {
        self.attempts.last().unwrap().0.binding.tick
    }
    pub(super) fn accepted_epochs(&self) -> usize {
        1 + self.accepted.len()
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn route(
        &mut self,
        ledger: &mut ResidentLedger,
        geometry: &mut InstalledFixture,
        install: &'a input::Install,
        height: &PreparedSourceHeight<'_, '_, '_>,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        profile: &Profile,
        cold: &RouteCommitNotice,
        cold_deadline: Instant,
        request: &AuthorityRequest,
        late_failures: usize,
    ) -> Result<(Facts, bool), NativeError> {
        let epoch = self.accepted_epochs() as u64 + 1;
        assert!(
            (2..=8).contains(&epoch),
            "finite loop cannot request a ninth Route"
        );
        let previous = self.accepted.last().unwrap_or(cold);
        fixture::assert_request(profile, previous, request, epoch);
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
            unreachable!()
        };
        assert_eq!(
            route.request.prior_cache.as_ref().unwrap().revision + 1,
            epoch
        );
        let prior_cache = route.request.prior_cache.as_ref().unwrap();
        eprintln!("resident finite actual callback: epoch={epoch} tick={} query={} now-ms={} prior-revision={} prior-storage={} origin={:?} goal={:?}",
            request.binding.tick, route.request.binding.query_id, route.request.binding.now_ms,
            prior_cache.revision, prior_cache.poly_refs.len(), route.request.origin, route.request.goal);
        let retry = self
            .attempts
            .last()
            .is_some_and(|(old, _)| old.binding.tick == request.binding.tick);
        if !retry {
            assert_eq!(ledger.counts(), (1, 1));
            assert!(self
                .retired
                .iter()
                .all(|old| !old.is_alive() && !old.head_is_alive()));
            self.retired.append(&mut self.proposals);
            self.attempts.clear();
            self.root = Some(ledger.diagnostic());
            self.prior = (epoch > 2).then(|| ledger.committed_warm_liveness_for_test().unwrap());
            if let Some(prior) = &self.prior {
                assert!(prior.is_alive() && prior.cold_is_alive());
            }
            if self.mode == Mode::Recovery && matches!(epoch, 4 | 6) {
                let previous_deadline = self.last_deadline.unwrap();
                ledger
                    .check_fresh_candidate(
                        provider,
                        &previous.routes()[0].request,
                        previous_deadline,
                    )
                    .unwrap();
                if let Some(wait) = previous_deadline.checked_duration_since(Instant::now()) {
                    std::thread::sleep(wait + Duration::from_millis(1));
                }
                assert!(Instant::now() >= previous_deadline && Instant::now() >= cold_deadline);
                assert!(matches!(
                    ledger.check_fresh_candidate(
                        provider,
                        &previous.routes()[0].request,
                        runtime::until()
                    ),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert_eq!(ledger.diagnostic(), *self.root.as_ref().unwrap());
                self.expired += 1;
                eprintln!("resident finite expired predecessor: next-epoch={epoch} cold-expired=true previous-expired=true renewed=false");
            }
        } else {
            assert_eq!(
                (self.mode, epoch, self.attempts.len(), late_failures),
                (Mode::Recovery, 4, 1, 1)
            );
            let (failed, deadline) = &self.attempts[0];
            assert_eq!(
                serde_json::to_vec(failed).unwrap(),
                serde_json::to_vec(request).unwrap()
            );
            assert!(!failed
                .require_pathfinder_dispatch(profile)
                .unwrap()
                .same_live_dispatch(request.require_pathfinder_dispatch(profile).unwrap()));
            assert_eq!(ledger.counts(), (2, 1));
            assert!(ledger
                .diagnostic()
                .same_committed_root(self.root.as_ref().unwrap()));
            assert!(self.prior.as_ref().unwrap().is_alive());
            ledger
                .check_fresh_candidate(provider, failed, *deadline)
                .unwrap();
            assert!(matches!(
                ledger.check_fresh_candidate(provider, request, *deadline),
                Err(ResidentFilterError::Context(Failure::Binding))
            ));
        }
        if self.mode != Mode::OldPolicyBoundary {
            let index = geometry
                .app
                .world()
                .resource::<benilla_world::terrain_stream::NativeStaticMapIndex>();
            ledger
                .check_straight_dispatch_for_test(
                    provider,
                    index.source_provenance().unwrap(),
                    request,
                )
                .unwrap();
        } else if epoch == 2 {
            ledger
                .check_retained_dispatch_for_test(provider, request)
                .unwrap();
        } else {
            let index = geometry
                .app
                .world()
                .resource::<benilla_world::terrain_stream::NativeStaticMapIndex>();
            ledger
                .check_repeated_dispatch_for_test(
                    provider,
                    index.source_provenance().unwrap(),
                    request,
                )
                .unwrap();
        }
        let ticket = epoch.checked_mul(100).unwrap().checked_add(1).unwrap();
        if self.mode == Mode::OldPolicyBoundary && epoch == 4 {
            assert!(!retry && !self.old_boundary_refused);
            let error = super::source_route_resident_straight_refusals::refuse_actual_fourth(
                ledger,
                geometry,
                compatibility,
                provider,
                request,
            );
            assert_eq!(ledger.diagnostic(), *self.root.as_ref().unwrap());
            self.old_boundary_refused = true;
            return Err(error);
        }
        if self.mode == Mode::RefusalsHistory && epoch == 5 && !retry {
            fixture::assert_source_order(ledger, provider, request, ticket);
            assert_eq!(ledger.diagnostic(), *self.root.as_ref().unwrap());
        }
        if self.mode == Mode::RefusalsHistory && epoch == 4 && !retry {
            super::source_route_resident_straight_clearance::probe(
                ledger,
                geometry,
                install,
                height,
                compatibility,
                provider,
                request,
            );
            assert_eq!(ledger.diagnostic(), *self.root.as_ref().unwrap());
        }
        let deadline = Instant::now() + Duration::from_secs(30);
        geometry.advance_physics();
        let facts = if self.mode != Mode::OldPolicyBoundary {
            ledger.certify_straight(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                ticket,
                deadline,
                || false,
            )
        } else if epoch == 2 {
            ledger.certify_retained(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                ticket,
                deadline,
                || false,
            )
        } else {
            ledger.certify_repeated(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                ticket,
                deadline,
                || false,
            )
        }
        .unwrap();
        if self.mode != Mode::OldPolicyBoundary {
            ledger
                .check_straight_result_for_test(request, &facts)
                .unwrap();
        }
        fixture::assert_facts(request, &facts, epoch);
        let staged = ledger.diagnostic();
        let repeated = if self.mode != Mode::OldPolicyBoundary {
            ledger.certify_straight(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                ticket,
                deadline,
                || false,
            )
        } else if epoch == 2 {
            ledger.certify_retained(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                ticket,
                deadline,
                || false,
            )
        } else {
            ledger.certify_repeated(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                ticket,
                deadline,
                || false,
            )
        };
        assert!(matches!(
            repeated,
            Err(ResidentFilterError::Context(Failure::Binding))
        ));
        assert_eq!(ledger.diagnostic(), staged);
        assert!(staged.same_committed_root(self.root.as_ref().unwrap()));
        ledger
            .check_fresh_candidate(provider, request, deadline)
            .unwrap();
        self.proposals
            .push(ledger.staged_warm_liveness_for_test(request).unwrap());
        assert!(self
            .proposals
            .iter()
            .all(|p| p.is_alive() && p.head_is_alive()));
        let owned = ledger.flat_ownership_for_test().unwrap();
        assert_eq!(owned.staged_count, self.proposals.len());
        assert_eq!(
            owned.terminal_count,
            if epoch == 2 { 1 } else { 2 } + owned.staged_count
        );
        assert!(owned.staged_count <= 2 && owned.storage_bytes <= owned.upper_bound_bytes);
        self.attempts.push((request.clone(), deadline));
        assert!(self.attempts.len() <= 2);
        eprintln!("resident finite staged: epoch={epoch} attempt={} terminals={} staged={} logical-storage-bytes={} bound-bytes={}", self.attempts.len(), owned.terminal_count, owned.staged_count, owned.storage_bytes, owned.upper_bound_bytes);
        Ok((facts, self.mode == Mode::Recovery && epoch == 4 && !retry))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn notice(
        &mut self,
        ledger: &mut ResidentLedger,
        provider: &ResidentFilterProvider<'_, '_, '_, '_>,
        install: &'a input::Install,
        profile: &Profile,
        foreign: &RouteCommitBoundary,
        cold: &RouteCommitNotice,
        notice: &RouteCommitNotice,
        late_failures: usize,
    ) -> Result<(), NativeError> {
        self.deliveries += 1;
        assert_eq!(notice.routes().len(), 1);
        assert_eq!(notice.boundary().heads().len(), 1);
        let epoch = notice.boundary().heads()[0].cache().unwrap().revision;
        assert!((2..=8).contains(&epoch));
        let duplicate = self
            .accepted
            .last()
            .is_some_and(|old| old.boundary().tick() == notice.boundary().tick());
        let previous = if duplicate {
            self.accepted.iter().rev().nth(1).unwrap_or(cold)
        } else {
            self.accepted.last().unwrap_or(cold)
        };
        let actual = &notice.routes()[0].request;
        let latest = &self.attempts.last().unwrap().0;
        assert_eq!(actual, latest);
        assert!(actual
            .require_pathfinder_dispatch(profile)
            .unwrap()
            .same_live_dispatch(latest.require_pathfinder_dispatch(profile).unwrap()));
        fixture::assert_request(profile, previous, actual, epoch);
        if self.mode != Mode::OldPolicyBoundary {
            ledger
                .check_straight_result_for_test(actual, &notice.routes()[0].facts)
                .unwrap();
        }
        fixture::assert_facts(actual, &notice.routes()[0].facts, epoch);
        if duplicate {
            let old = self.accepted.last().unwrap();
            assert_eq!(notice, old);
            assert!(actual
                .require_pathfinder_dispatch(profile)
                .unwrap()
                .same_live_dispatch(
                    old.routes()[0]
                        .request
                        .require_pathfinder_dispatch(profile)
                        .unwrap()
                ));
        } else {
            assert_eq!(epoch, self.accepted_epochs() as u64 + 1);
            assert!(ledger
                .diagnostic()
                .same_committed_root(self.root.as_ref().unwrap()));
            assert!(self
                .proposals
                .iter()
                .all(|p| p.is_alive() && p.head_is_alive()));
        }
        for altered in [
            wrong::foreign_session(notice, foreign),
            wrong::skipped_tick(notice),
            wrong::wrong_current_cache(notice),
            wrong::without_routes(notice),
            wrong::duplicate_route(notice),
            wrong::stale_generation(notice),
            wrong::omitted_head(notice),
            wrong::extra_head(notice),
            wrong::decoded_dispatch(notice),
            wrong::uncertified_reply(notice),
            wrong::substitute_earlier_dispatch(notice, previous),
        ] {
            refuse(ledger, &altered);
        }
        if self.mode == Mode::Recovery && epoch == 4 {
            assert_eq!(late_failures, 1);
            refuse(
                ledger,
                &wrong::substitute_dispatch(notice, &self.attempts[0].0),
            );
        }
        let before = ledger.diagnostic();
        if self.mode == Mode::RefusalsHistory && epoch == 8 && !duplicate {
            assert!(self.change.is_none());
            let (context, deadline) = self.attempts.last().unwrap();
            self.change = Some(SourceChange::begin(
                install, ledger, provider, context, *deadline,
            ));
            assert_eq!(ledger.diagnostic(), before);
        }
        ledger.acknowledge(notice).unwrap();
        assert_eq!(ledger.counts(), (1, 1));
        ledger.check_committed_warm_for_test().unwrap();
        if let Some(prior) = &self.prior {
            assert!(!prior.is_alive() && prior.cold_is_alive());
        }
        assert!(self
            .retired
            .iter()
            .all(|p| !p.is_alive() && !p.head_is_alive() && p.cold_is_alive()));
        assert!(self
            .proposals
            .iter()
            .all(|p| !p.is_alive() && p.cold_is_alive()));
        assert!(self.proposals.last().unwrap().head_is_alive());
        assert!(self.proposals[..self.proposals.len() - 1]
            .iter()
            .all(|p| !p.head_is_alive()));
        let owned = ledger.flat_ownership_for_test().unwrap();
        assert_eq!((owned.terminal_count, owned.staged_count), (2, 0));
        assert!(owned.storage_bytes <= owned.upper_bound_bytes);
        if duplicate {
            assert_eq!(ledger.diagnostic(), before);
        }
        let accepted = ledger.diagnostic();
        ledger.acknowledge(notice).unwrap();
        assert_eq!(ledger.diagnostic(), accepted);
        refuse(
            ledger,
            &wrong::substitute_earlier_dispatch(notice, previous),
        );
        if self.mode == Mode::Recovery && epoch == 4 {
            refuse(
                ledger,
                &wrong::substitute_dispatch(notice, &self.attempts[0].0),
            );
        }
        if !duplicate {
            self.last_deadline = Some(self.attempts.last().unwrap().1);
            self.accepted.push(notice.clone());
            assert!(self.accepted.len() <= 7);
        }
        eprintln!("resident finite ACK: epoch={epoch} duplicate={duplicate} terminals=2 staged=0 old-warm-released=true proposal-wrappers-released={} logical-storage-bytes={} bound-bytes={}", self.proposals.len(), owned.storage_bytes, owned.upper_bound_bytes);
        let drop_epoch = if self.mode == Mode::Recovery { 6 } else { 8 };
        if epoch == drop_epoch && !self.lost {
            self.lost = true;
            Err(NativeError::AuthorityUnavailable(
                "deliberately lost finite-loop ACK after application",
            ))
        } else {
            Ok(())
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn finish(
        &self,
        proofs: &[fixture::CommittedTick],
        ledger: &mut ResidentLedger,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        geometry: &mut InstalledFixture,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        height: &PreparedSourceHeight<'_, '_, '_>,
        routes: usize,
        late_failures: usize,
    ) {
        let expected_warm = if self.mode == Mode::OldPolicyBoundary {
            2
        } else {
            7
        };
        assert_eq!(self.accepted.len(), expected_warm);
        assert_eq!(proofs.len(), expected_warm);
        for (proof, notice) in proofs.iter().zip(&self.accepted) {
            fixture::assert_commit_proof(proof, notice);
        }
        if self.mode == Mode::OldPolicyBoundary {
            assert!(self.old_boundary_refused);
            assert_eq!(self.deliveries, 2);
            assert_eq!(routes, 4);
            assert_eq!(late_failures, 0);
            assert_eq!(self.expired, 0);
            assert!(!self.lost && self.change.is_none());
            assert_eq!(ledger.diagnostic(), *self.root.as_ref().unwrap());
            ledger.check_committed_warm_for_test().unwrap();
            let prior = self.prior.as_ref().unwrap();
            assert!(prior.is_alive() && prior.cold_is_alive());
            assert!(self.proposals.is_empty());
            let owned = ledger.flat_ownership_for_test().unwrap();
            assert_eq!((owned.terminal_count, owned.staged_count), (2, 0));
            assert!(owned.storage_bytes <= owned.upper_bound_bytes);
            eprintln!("resident old-policy boundary: accepted=3 attempted=4 exact-terminal=NonSamePolygonUnsupported ledger-unchanged=true no-facts=true");
            return;
        }
        assert!(!self.old_boundary_refused);
        assert_eq!(self.deliveries, 8);
        assert!(self.lost);
        assert_eq!(routes, if self.mode == Mode::Recovery { 9 } else { 8 });
        assert_eq!(late_failures, usize::from(self.mode == Mode::Recovery));
        assert_eq!(
            self.expired,
            if self.mode == Mode::Recovery { 2 } else { 0 }
        );
        let (context, deadline) = self.attempts.last().unwrap();
        let before = ledger.diagnostic();
        assert!(Instant::now() < *deadline);
        let result = ledger.check_fresh_candidate(provider, context, *deadline);
        if self.mode == Mode::RefusalsHistory {
            assert!(matches!(
                result,
                Err(ResidentFilterError::Source(CompositionError::TerrainSource))
            ));
            self.change
                .as_ref()
                .unwrap()
                .refuse_fresh_finite_point_source(ledger, provider, height, context);
        } else {
            result.unwrap();
            assert!(self.change.is_none());
        }
        assert_eq!(ledger.diagnostic(), before);
        // This is a direct exhaustion probe on the actual final receipt, NOT a
        // ninth runtime dispatch. The observer must not enter fresh scene work.
        let reached = std::cell::Cell::new(0);
        assert!(matches!(
            ledger.certify_straight_observed(
                geometry.app.world_mut(),
                compatibility,
                provider,
                context,
                901,
                runtime::until(),
                || false,
                || reached.set(reached.get() + 1)
            ),
            Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
        ));
        assert_eq!(reached.get(), 0);
        let mut phases = vec![];
        assert!(matches!(
            ledger.probe_straight_source_for_test(
                provider,
                context,
                901,
                runtime::until(),
                || false,
                |phase| phases.push(phase)
            ),
            Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
        ));
        assert!(
            phases.is_empty(),
            "exhaustion never enters a new source stage"
        );
        assert_eq!(ledger.diagnostic(), before);
        eprintln!("resident finite loop complete: accepted=8 route-attempts={routes} warm-deliveries={} expired-predecessors={} last-revision=8 direct-ninth-refusal=true", self.deliveries, self.expired);
    }
}
fn refuse(ledger: &mut ResidentLedger, notice: &RouteCommitNotice) {
    let before = ledger.diagnostic();
    assert_eq!(ledger.acknowledge(notice), Err(Failure::Binding));
    assert_eq!(ledger.diagnostic(), before);
}
