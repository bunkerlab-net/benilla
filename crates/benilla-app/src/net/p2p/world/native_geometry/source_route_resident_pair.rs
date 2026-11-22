// SPDX-License-Identifier: GPL-3.0-or-later
//! Two genuine co-located fixture owners, one retained revision each. This is not
//! NPC avoidance or populated-world coverage; all source/timer inputs stay unchanged.
use super::{
    core::Failure,
    products::source_height::inventory::InventoryCompatibility,
    real_corridor_fixture::installed_source::InstalledFixture,
    route_commit::{authored_conflicts as wrong, RouteCommitNotice},
    source_pathfinder_query::resident_joined_tests as runtime,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
    source_route_resident_local::ledger::{Diagnostic, ProposalLiveness, ResidentLedger},
    source_route_resident_retained as retained,
};
use crate::net::p2p::world::{resident_fixture::Driver, PauseKind, SavedBoundary};
use benilla_sim::{creature_ai::Point, creature_motion as motion};
use benilla_world_runtime::native::{
    AuthorityRequest, CheckpointIdentity, Facts, MotionQuery, NativeError, Profile, Query,
};
use std::{
    cell::Cell,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Mode {
    Accept,
    Recovery,
    ThirdRevisionBoundary,
}
pub(super) const SECOND_FAILURE: &str = "two-owner deliberate second retained callback failure";
pub(super) const THIRD_REFUSAL: &str = "two-owner third revision refused before source work";
pub(super) struct Owner {
    mode: Mode,
    profile: Profile,
    cold: Option<RouteCommitNotice>,
    warm: Option<RouteCommitNotice>,
    root: Option<Diagnostic>,
    attempts: Vec<AuthorityRequest>,
    proposals: Vec<ProposalLiveness>,
    failed_second: bool,
    dropped_ack: bool,
    third_refused: bool,
}
impl Owner {
    pub(super) fn new(mode: Mode, profile: &Profile) -> Self {
        assert_eq!(profile.npcs.len(), 2);
        Self {
            mode,
            profile: profile.clone(),
            cold: None,
            warm: None,
            root: None,
            attempts: vec![],
            proposals: vec![],
            failed_second: false,
            dropped_ack: false,
            third_refused: false,
        }
    }
    pub(super) fn observe_cold(&mut self, notice: &RouteCommitNotice, ledger: &ResidentLedger) {
        assert!(self.cold.is_none());
        assert_eq!(notice.routes().len(), 2);
        assert_eq!(ledger.counts(), (2, 2));
        self.cold = Some(notice.clone());
    }
    pub(super) fn route(
        &mut self,
        ledger: &mut ResidentLedger,
        geometry: &mut InstalledFixture,
        compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
        provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
        request: &AuthorityRequest,
    ) -> Result<Facts, NativeError> {
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
            unreachable!()
        };
        let before = ledger.diagnostic();
        let constructors = provider.retained_constructor_count();
        assert_eq!(constructors, 2);
        if route.request.prior_cache.as_ref().unwrap().revision == 2 {
            assert_eq!(self.mode, Mode::ThirdRevisionBoundary);
            assert!(self.warm.is_some());
            let queries = Cell::new(0);
            let result = ledger.certify_two_owner_retained_observed(
                geometry.app.world_mut(),
                compatibility,
                provider,
                request,
                901,
                runtime::until(),
                || {
                    queries.set(queries.get() + 1);
                    false
                },
                || panic!("closed revision reached concrete completion"),
            );
            assert!(matches!(
                result,
                Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
            ));
            assert_eq!(
                queries.get(),
                0,
                "revision guard precedes source freshness work"
            );
            assert_eq!(ledger.diagnostic(), before);
            assert_eq!(provider.retained_constructor_count(), constructors);
            self.third_refused = true;
            return Err(NativeError::AuthorityUnavailable(THIRD_REFUSAL));
        }
        assert!(self.warm.is_none());
        if self.attempts.is_empty() {
            self.root = Some(before.clone());
        }
        let old_work = Cell::new(0);
        let old = ledger.certify_retained(
            geometry.app.world_mut(),
            compatibility,
            provider,
            request,
            801,
            runtime::until(),
            || {
                old_work.set(old_work.get() + 1);
                false
            },
        );
        assert!(matches!(
            old,
            Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
        ));
        assert_eq!(old_work.get(), 0);
        assert_eq!(ledger.diagnostic(), before);
        ledger.check_pair_isolation_for_test(request).unwrap();
        let cold = self.cold.as_ref().unwrap();
        let old = cold
            .routes()
            .iter()
            .find(|reply| reply.request.binding.npc == request.binding.npc)
            .unwrap();
        let head = cold
            .boundary()
            .heads()
            .iter()
            .find(|head| Some(head.npc()) == request.binding.npc)
            .unwrap();
        let old_dispatch = old
            .request
            .require_pathfinder_dispatch(&self.profile)
            .unwrap();
        let dispatch = request.require_pathfinder_dispatch(&self.profile).unwrap();
        assert_eq!(
            dispatch.construction_binding(),
            old_dispatch.construction_binding()
        );
        assert!(!dispatch.first_construction());
        assert!(!dispatch.same_live_dispatch(old_dispatch));
        assert_eq!(route.request.prior_cache.as_deref(), head.cache());
        assert_eq!(head.cache().unwrap().revision, 1);
        assert_eq!(route.request.kind, motion::RouteKind::ChaseReplan);
        self.attempts.push(request.clone());
        if self.mode == Mode::Recovery && self.attempts.len() == 2 {
            assert_eq!(self.proposals.len(), 1);
            assert!(before.same_committed_root(self.root.as_ref().unwrap()));
            self.failed_second = true;
            return Err(NativeError::AuthorityUnavailable(SECOND_FAILURE));
        }
        geometry.advance_physics();
        let facts = ledger.certify_two_owner_retained(
            geometry.app.world_mut(), compatibility, provider, request,
            201 + self.attempts.len() as u64 * 10, runtime::until(), || false,
        ).unwrap_or_else(|error| panic!("actual two-owner first retained geometry refused: owner={:?} tick={} error={error:?}", request.binding.npc, request.binding.tick));
        retained::assert_facts(request, &facts);
        assert!(ledger
            .diagnostic()
            .same_committed_root(self.root.as_ref().unwrap()));
        assert_eq!(provider.retained_constructor_count(), constructors);
        self.proposals
            .push(ledger.staged_warm_liveness_for_test(request).unwrap());
        Ok(facts)
    }
    pub(super) fn notice(
        &mut self,
        ledger: &mut ResidentLedger,
        notice: &RouteCommitNotice,
    ) -> Result<(), NativeError> {
        assert_eq!(
            notice.routes().len(),
            2,
            "both natural retained callbacks must share one saved tick"
        );
        assert_eq!(notice.boundary().heads().len(), 2);
        if let Some(old) = &self.warm {
            assert_eq!(notice, old);
            let before = ledger.diagnostic();
            ledger.acknowledge(notice).unwrap();
            assert_eq!(ledger.diagnostic(), before);
            return Ok(());
        }
        for bad in [
            wrong::wrong_last_cache(notice),
            wrong::crossed_owner_heads(notice),
            wrong::reordered_routes(notice),
            wrong::without_routes(notice),
            wrong::duplicate_route(notice),
            wrong::decoded_dispatch(notice),
            wrong::stale_generation(notice),
            wrong::omitted_head(notice),
            wrong::uncertified_reply(notice),
        ] {
            reject(ledger, &bad);
        }
        if self.mode == Mode::Recovery {
            assert!(self.failed_second);
            assert_eq!(self.attempts.len(), 4);
            reject(
                ledger,
                &wrong::substitute_dispatch(notice, &self.attempts[0]),
            );
            assert!(!self.attempts[0]
                .require_pathfinder_dispatch(&self.profile)
                .unwrap()
                .same_live_dispatch(
                    self.attempts[2]
                        .require_pathfinder_dispatch(&self.profile)
                        .unwrap()
                ));
        }
        ledger.acknowledge(notice).unwrap();
        ledger.check_committed_pair_for_test().unwrap();
        let ownership = ledger.pair_ownership_for_test().unwrap();
        assert_eq!((ownership.terminal_count, ownership.staged_count), (4, 0));
        assert_eq!(ledger.counts(), (2, 2));
        for (index, proposal) in self.proposals.iter().enumerate() {
            assert!(!proposal.is_alive(), "ACK retires all proposal wrappers");
            assert_eq!(
                proposal.head_is_alive(),
                !(self.mode == Mode::Recovery && index == 0)
            );
            assert!(
                proposal.cold_is_alive(),
                "both immutable cold anchors remain owned"
            );
        }
        self.warm = Some(notice.clone());
        if self.mode == Mode::Recovery {
            self.dropped_ack = true;
            return Err(NativeError::AuthorityUnavailable(
                "two-owner applied ACK response lost",
            ));
        }
        Ok(())
    }
    pub(super) fn finish(&self, proof: &Proof, ledger: &ResidentLedger) {
        let notice = self
            .warm
            .as_ref()
            .expect("both actual retained owners acknowledged");
        assert_eq!(notice.before(), proof.before.state_digest);
        assert_eq!(notice.previous_record(), proof.before.history_tip);
        assert_eq!(notice.boundary().state(), proof.after.state_digest);
        assert_eq!(notice.boundary().record(), proof.after.history_tip);
        assert_eq!(proof.saved.tick, proof.after.tick);
        assert_eq!(notice.boundary().tick(), proof.after.tick);
        assert_eq!(notice.boundary().world(), proof.after.world);
        assert_eq!(notice.boundary().player(), proof.after.player);
        assert_eq!(notice.boundary().profile(), proof.after.profile);
        assert_eq!(proof.before.tick + 1, proof.after.tick);
        assert_eq!(self.failed_second, self.mode == Mode::Recovery);
        assert_eq!(self.dropped_ack, self.mode == Mode::Recovery);
        assert_eq!(self.third_refused, self.mode == Mode::ThirdRevisionBoundary);
        ledger.check_committed_pair_for_test().unwrap();
        assert_eq!(ledger.pair_ownership_for_test().unwrap().staged_count, 0);
    }
}
fn reject(ledger: &mut ResidentLedger, notice: &RouteCommitNotice) {
    let before = ledger.diagnostic();
    assert_eq!(ledger.acknowledge(notice), Err(Failure::Binding));
    assert_eq!(ledger.diagnostic(), before, "no partial owner promotion");
}
pub(super) struct Proof {
    before: CheckpointIdentity,
    after: CheckpointIdentity,
    saved: SavedBoundary,
}

pub(super) fn after_cold_ack(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    cold_events: usize,
    mode: Mode,
) -> (u64, usize, usize, Proof) {
    assert_eq!(routes.load(Ordering::SeqCst), 2);
    let start = driver.state().position;
    let mut events = cold_events;
    for step in 1..=64 {
        let destination = Point::new(
            start.x.as_f32() - step as f32 * 0.125,
            start.y.as_f32(),
            start.z.as_f32(),
        )
        .unwrap();
        driver.queue_movement(destination, 0.5).unwrap();
        let before = driver.checkpoint();
        let records = driver.records().to_vec();
        let generation = driver.store_generation();
        let tick = driver.tick();
        let mut out = driver.pump_one();
        if routes.load(Ordering::SeqCst) == 2 {
            assert!(out.pause.is_none(), "{:?}", out.pause);
            assert_eq!(out.ticks, 1);
            events += out.events.len();
            continue;
        }
        assert_eq!(
            routes.load(Ordering::SeqCst),
            4,
            "both owners must naturally replan in the same tick"
        );
        if mode == Mode::Recovery {
            assert_eq!(
                out.pause.as_ref().map(|p| &p.kind),
                Some(&PauseKind::Authority)
            );
            assert!(out.pause.as_ref().unwrap().message.contains(SECOND_FAILURE));
            assert_eq!(out.ticks, 0);
            assert!(out.events.is_empty());
            assert_eq!(driver.checkpoint(), before);
            assert_eq!(driver.records(), records);
            assert_eq!(driver.store_generation(), generation);
            assert_eq!(driver.tick(), tick);
            assert!(!driver.route_pending());
            let inputs = driver.pending_inputs().unwrap().to_vec();
            let count = callbacks.load(Ordering::SeqCst);
            let blocked = driver.pump_zero();
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            assert_eq!(driver.checkpoint(), before);
            driver.retry_tick().unwrap();
            assert_eq!(driver.pending_inputs().unwrap(), inputs);
            out = driver.pump_zero();
            assert_eq!(routes.load(Ordering::SeqCst), 6);
        }
        assert_eq!(out.ticks, 1);
        assert_eq!(driver.tick(), tick + 1);
        let saved = out.saved.clone().unwrap();
        assert_eq!(saved.tick, tick + 1);
        assert_eq!(Some(saved.generation), generation.checked_add(1));
        assert!(driver.records().is_empty());
        let proof = Proof {
            before: before.identity(),
            after: driver.checkpoint().identity(),
            saved,
        };
        assert!(driver.pending_inputs().is_none());
        let checkpoint = driver.checkpoint();
        let saved_generation = driver.store_generation();
        let count = callbacks.load(Ordering::SeqCst);
        if mode == Mode::Recovery {
            assert_eq!(
                out.pause.as_ref().map(|p| &p.kind),
                Some(&PauseKind::CommittedAwaitingRouteAck)
            );
            assert!(driver.route_pending());
            let blocked = driver.pump_one();
            assert_eq!(blocked.ticks, 0);
            assert!(blocked.events.is_empty());
            assert_eq!(driver.checkpoint(), checkpoint);
            assert_eq!(callbacks.load(Ordering::SeqCst), count);
            driver.retry_route_commit().unwrap();
        } else {
            assert!(out.pause.is_none(), "{:?}", out.pause);
        }
        assert!(!driver.route_pending());
        assert!(driver.pause().is_none());
        assert_eq!(driver.checkpoint(), checkpoint);
        assert_eq!(driver.store_generation(), saved_generation);
        assert_eq!(callbacks.load(Ordering::SeqCst), count);
        let flushed = driver.pump_zero();
        assert_eq!(flushed.ticks, 0);
        assert!(flushed.events.is_empty());
        events += out.events.len();
        if mode == Mode::ThirdRevisionBoundary {
            return after_warm_boundary(driver, callbacks, routes, events, proof);
        }
        return (driver.tick(), events, count, proof);
    }
    panic!("unchanged ordinary movement did not produce both first retained callbacks");
}
fn after_warm_boundary(
    driver: &mut Driver,
    callbacks: &AtomicUsize,
    routes: &AtomicUsize,
    mut events: usize,
    proof: Proof,
) -> (u64, usize, usize, Proof) {
    let start = driver.state().position;
    for step in 1..=64 {
        driver
            .queue_movement(
                Point::new(
                    start.x.as_f32() - step as f32 * 0.125,
                    start.y.as_f32(),
                    start.z.as_f32(),
                )
                .unwrap(),
                0.5,
            )
            .unwrap();
        let before = driver.checkpoint();
        let records = driver.records().to_vec();
        let generation = driver.store_generation();
        let out = driver.pump_one();
        if routes.load(Ordering::SeqCst) == 4 {
            assert_eq!(out.ticks, 1);
            assert!(out.pause.is_none());
            events += out.events.len();
            continue;
        }
        assert_eq!(routes.load(Ordering::SeqCst), 5);
        assert_eq!(
            out.pause.as_ref().map(|p| &p.kind),
            Some(&PauseKind::Authority)
        );
        assert!(out.pause.as_ref().unwrap().message.contains(THIRD_REFUSAL));
        assert_eq!(out.ticks, 0);
        assert!(out.events.is_empty());
        assert_eq!(driver.checkpoint(), before);
        assert_eq!(driver.records(), records);
        assert_eq!(driver.store_generation(), generation);
        return (
            driver.tick(),
            events,
            callbacks.load(Ordering::SeqCst),
            proof,
        );
    }
    panic!("ordinary movement did not reach the actual third-revision boundary");
}
#[test]
fn two_owner_first_retained_commits_one_atomic_pair() {
    super::source_route_resident_controller::two_owner_retained_probe(Mode::Accept);
}
#[test]
fn two_owner_second_callback_rollback_and_lost_ack_do_not_publish_twice() {
    super::source_route_resident_controller::two_owner_retained_probe(Mode::Recovery);
}
#[test]
fn two_owner_third_revision_refuses_before_source_work() {
    super::source_route_resident_controller::two_owner_retained_probe(Mode::ThirdRevisionBoundary);
}
