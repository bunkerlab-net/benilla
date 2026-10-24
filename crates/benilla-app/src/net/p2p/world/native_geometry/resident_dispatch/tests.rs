// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::{
    products::{
        source_height::{
            inventory::OwnedInventoryCompatibility,
            owned::{
                freshness::{CaptureFreshnessWorker, FreshnessObservation, FreshnessPoll},
                OwnedSourceProducts,
            },
            CompositionError,
        },
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    source_pathfinder_query::resident_joined_tests as fixture,
    source_route_filter::resident::{ResidentFilterError, ResidentFilterProvider},
};
use super::*;
use benilla_world::terrain_stream::NativeStaticMapIndex;
use benilla_world_runtime::native::synthetic_fixture as f;

fn products(install: &input::Install) -> OwnedSourceProducts {
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let collision = benilla_native_vmap_source::prepare(install.path(), 0, || false)
        .unwrap()
        .into_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    OwnedSourceProducts::new(terrain, collision, || false).unwrap()
}
pub(super) fn with_request(
    test: impl FnOnce(
        &input::Install,
        &mut OwnedResident,
        &mut InstalledFixture,
        &Mailbox,
        &Profile,
        &AuthorityRequest,
    ),
) {
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, height, compatibility, inventory, policy, profile, query, other| {
            let session = fixture::session(&profile);
            let provenance =
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap();
            let mut geometry = InstalledFixture::new(install.path(), inventory);
            let installed = geometry
                .app
                .world()
                .resource::<NativeStaticMapIndex>()
                .source_provenance()
                .unwrap();
            let mut owner = OwnedResident::bind(
                products(install),
                provenance,
                &session,
                &profile,
                f::player(),
                query,
                policy.clone(),
                installed,
                fixture::until(),
                || false,
            )
            .unwrap();
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&session, &profile, height, other, policy)
                    .unwrap();
            let (_, mailbox) = mailbox(session.clone()).unwrap();
            fixture::runtime_with_route(
                &profile,
                &session,
                height,
                compatibility,
                &mut provider,
                |_, request| {
                    test(
                        install,
                        &mut owner,
                        &mut geometry,
                        &mailbox,
                        &profile,
                        request,
                    )
                },
            );
        },
    );
}
fn capture(
    mailbox: &Mailbox,
    owner: &OwnedResident,
    profile: &Profile,
    request: &AuthorityRequest,
) -> ResidentDispatchWork {
    ResidentDispatchWork::capture(
        mailbox,
        owner,
        profile,
        f::player(),
        701,
        request.clone(),
        fixture::until(),
    )
    .unwrap()
}
fn ready(worker: &mut CaptureFreshnessWorker) -> FreshnessObservation {
    let watchdog = fixture::until();
    loop {
        match worker.try_poll() {
            FreshnessPoll::Pending => {
                assert!(Instant::now() < watchdog);
                std::thread::yield_now();
            }
            FreshnessPoll::Ready(result) => return result.unwrap(),
            _ => panic!("resident observation lost its slot"),
        }
    }
}
pub(super) fn no_native_constructors(owner: &mut OwnedResident, geometry: &InstalledFixture) {
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap();
    owner
        .inspect(
            installed,
            fixture::until(),
            || false,
            |provider, _, ledger| {
                assert_eq!(provider.retained_constructor_count(), 0);
                assert_eq!(ledger.counts(), (0, 0));
                Ok(())
            },
        )
        .unwrap();
}

#[test]
fn resident_dispatch_preserves_original_work_selection_without_queue_admission() {
    fn send<T: Send>() {}
    send::<Work>();
    with_request(|_, owner, geometry, mailbox, profile, request| {
        assert!(mailbox.session.same_polygon_route.is_none());
        assert!(matches!(
            mailbox.enqueue_service_test_request(701, request.clone()),
            Err(Failure::UnsupportedQuery)
        ));
        let work = capture(mailbox, owner, profile, request);
        let lease = work.work.lease();
        for deadline in [lease.deadline() + WAIT, Instant::now() + WAIT] {
            let job = work.job(mailbox, owner, deadline).unwrap();
            assert!(job.lease().same_job(&lease));
            assert!(std::ptr::eq(job.request(), lease.request()));
            assert_eq!(job.profile(), profile);
            assert_eq!(job.player(), f::player());
            assert_eq!(job.deadline(), deadline.min(lease.deadline()));
        }
        assert!(matches!(
            work.job(mailbox, owner, Instant::now()),
            Err(Failure::Deadline)
        ));
        assert!(lease.is_active());
        let mut worker = CaptureFreshnessWorker::new().unwrap();
        owner
            .enqueue_observation(
                work.job(mailbox, owner, fixture::until()).unwrap(),
                &mut worker,
            )
            .unwrap();
        let other = capture(mailbox, owner, profile, request);
        assert!(matches!(
            owner.enqueue_observation(
                other.job(mailbox, owner, fixture::until()).unwrap(),
                &mut worker
            ),
            Err(ResidentFilterError::Context(Failure::QueueCapacity))
        ));
        drop(ready(&mut worker));
        assert!(mailbox.rx.is_empty() && mailbox.retry.is_empty());
        no_native_constructors(owner, geometry);
        drop(work);
        assert!(!lease.is_active());
        assert!(mailbox.rx.is_empty() && mailbox.retry.is_empty());
    });
}

#[test]
fn resident_dispatch_refuses_changed_context_body_player_and_branch_tampering() {
    with_request(|_, owner, geometry, mailbox, profile, request| {
        let mut worker = CaptureFreshnessWorker::new().unwrap();
        // These mutations must fail before enqueue. Altered branch fields may
        // fail live-dispatch authentication before the supported-shape guard;
        // they do not manufacture a new runtime-authorized unsupported branch.
        for field in 0..12 {
            let mut changed = request.clone();
            match field {
                0 => changed.binding.geometry[0] ^= 1,
                1 => changed.binding.profile[0] ^= 1,
                2 => changed.binding.world[0] ^= 1,
                3 => changed.binding.npc.as_mut().unwrap().generation += 1,
                _ => {
                    let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query else {
                        unreachable!()
                    };
                    match field {
                        4 => route.owner_ignore_pathfinding = None,
                        5 => route.pathfinder_dispatch = None,
                        6 => route.request.owner_collision_height = ai::Real::ONE,
                        7 => route.request.launch_origin.x = ai::Real::ONE,
                        8 => route.request.point_limit = 2,
                        9 => route.request.kind = motion::RouteKind::HomeForceDestination,
                        10 => {
                            route.request.target.as_mut().unwrap().collision_height = ai::Real::ONE
                        }
                        11 => route.request.binding.query_id += 1,
                        _ => unreachable!(),
                    }
                }
            }
            assert!(
                ResidentDispatchWork::capture(
                    mailbox,
                    owner,
                    profile,
                    f::player(),
                    701,
                    changed,
                    fixture::until()
                )
                .is_err(),
                "field {field}"
            );
            assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
        }
        let decoded = serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
        assert!(ResidentDispatchWork::capture(
            mailbox,
            owner,
            profile,
            f::player(),
            701,
            decoded,
            fixture::until()
        )
        .is_err());
        let mut changed_profile = profile.clone();
        changed_profile.tick_ms += 1;
        assert!(ResidentDispatchWork::capture(
            mailbox,
            owner,
            &changed_profile,
            f::player(),
            701,
            request.clone(),
            fixture::until()
        )
        .is_err());
        let mut player = f::player();
        player.owner[0] ^= 1;
        assert!(ResidentDispatchWork::capture(
            mailbox,
            owner,
            profile,
            player,
            701,
            request.clone(),
            fixture::until()
        )
        .is_err());
        for field in 0..5 {
            let mut session = mailbox.session.clone();
            match field {
                0 => session.geometry[0] ^= 1,
                1 => session.map += 1,
                2 => {
                    session
                        .bodies
                        .get_mut(&request.binding.npc.unwrap().spawn)
                        .unwrap()
                        .height = ai::Real::ONE
                }
                3 => session.player.as_mut().unwrap().body.radius = ai::Real::ONE,
                4 => session.participants = None,
                _ => unreachable!(),
            }
            let (_, mut changed) = super::super::mailbox(mailbox.session.clone()).unwrap();
            changed.session = session;
            assert!(ResidentDispatchWork::capture(
                &changed,
                owner,
                profile,
                f::player(),
                701,
                request.clone(),
                fixture::until()
            )
            .is_err());
            assert!(changed.rx.is_empty() && changed.retry.is_empty());
        }
        assert!(mailbox.rx.is_empty() && mailbox.retry.is_empty());
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_dispatch_refuses_foreign_work_attachment_dispatch_and_resident_allocation() {
    with_request(|_, owner, geometry, mailbox, profile, request| {
        let mut work = capture(mailbox, owner, profile, request);
        let other = capture(mailbox, owner, profile, request);
        assert_eq!(work.work.request, other.work.request);
        let original_lease = work.validation.lease.clone();
        work.validation.lease = other.work.lease();
        assert!(work.job(mailbox, owner, fixture::until()).is_err());
        work.validation.lease = original_lease;
        let (_, foreign_mailbox) = super::super::mailbox(mailbox.session.clone()).unwrap();
        assert!(work.job(&foreign_mailbox, owner, fixture::until()).is_err());
        with_request(|_, foreign, _, _, other_profile, other_request| {
            assert_eq!(profile, other_profile);
            assert_eq!(request, other_request);
            let (identity, session, _, player) = owner.dispatch_selection();
            let (foreign_identity, foreign_session, _, foreign_player) =
                foreign.dispatch_selection();
            assert!(session == foreign_session);
            assert_eq!(player, foreign_player);
            assert!(!identity.same_allocation(foreign_identity));
            assert!(work.job(mailbox, foreign, fixture::until()).is_err());
            let Query::SourceMotion(MotionQuery::Route(current)) = &other_request.query else {
                unreachable!()
            };
            let original = {
                let Query::SourceMotion(MotionQuery::Route(saved)) =
                    &mut work.validation.request.query
                else {
                    unreachable!()
                };
                let original = saved
                    .pathfinder_dispatch
                    .replace(current.pathfinder_dispatch.clone().unwrap());
                assert!(!original
                    .as_ref()
                    .unwrap()
                    .same_live_dispatch(saved.pathfinder_dispatch.as_ref().unwrap()));
                original
            };
            assert!(work.job(mailbox, owner, fixture::until()).is_err());
            let Query::SourceMotion(MotionQuery::Route(saved)) = &mut work.validation.request.query
            else {
                unreachable!()
            };
            saved.pathfinder_dispatch = original;
        });
        assert!(work.job(mailbox, owner, fixture::until()).is_ok());
        assert!(mailbox.rx.is_empty() && mailbox.retry.is_empty());
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_dispatch_revocation_and_original_expiry_prevent_certification() {
    with_request(|_, owner, geometry, mailbox, profile, request| {
        let installed = geometry
            .app
            .world()
            .resource::<NativeStaticMapIndex>()
            .source_provenance()
            .unwrap()
            .clone();
        for mode in 0..5 {
            let (_, attachment) = super::super::mailbox(mailbox.session.clone()).unwrap();
            let work = capture(&attachment, owner, profile, request);
            let mut worker = CaptureFreshnessWorker::new().unwrap();
            owner
                .enqueue_observation(
                    work.job(&attachment, owner, fixture::until()).unwrap(),
                    &mut worker,
                )
                .unwrap();
            let observation = ready(&mut worker);
            let deadline = if mode == 4 {
                Instant::now() + Duration::from_millis(50)
            } else {
                fixture::until()
            };
            let job = work.job(&attachment, owner, deadline).unwrap();
            match mode {
                0 => worker.cancel(),
                1 => drop(worker),
                2 => {
                    let other = capture(&attachment, owner, profile, request);
                    owner
                        .enqueue_observation(
                            other.job(&attachment, owner, fixture::until()).unwrap(),
                            &mut worker,
                        )
                        .unwrap();
                }
                3 => drop(attachment),
                4 => std::thread::sleep(job.deadline().saturating_duration_since(Instant::now())),
                _ => unreachable!(),
            }
            let result =
                owner.certify_observed(observation, job, geometry.app.world_mut(), &installed);
            if mode >= 3 {
                assert!(matches!(
                    result,
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
            } else {
                assert!(matches!(
                    result,
                    Err(ResidentFilterError::Source(CompositionError::Cancelled))
                ));
            }
            no_native_constructors(owner, geometry);
        }
        let work = ResidentDispatchWork::capture(
            mailbox,
            owner,
            profile,
            f::player(),
            701,
            request.clone(),
            Instant::now() + Duration::from_millis(50),
        )
        .unwrap();
        std::thread::sleep(work.work.deadline.saturating_duration_since(Instant::now()));
        assert!(matches!(
            work.job(mailbox, owner, fixture::until()),
            Err(Failure::Deadline)
        ));
        assert!(mailbox.rx.is_empty() && mailbox.retry.is_empty());
        no_native_constructors(owner, geometry);
    });
}

#[test]
fn resident_dispatch_refuses_foreign_capture_and_dropped_work_before_resume() {
    with_request(|install, owner, geometry, mailbox, profile, request| {
        let installed = geometry
            .app
            .world()
            .resource::<NativeStaticMapIndex>()
            .source_provenance()
            .unwrap()
            .clone();
        let work = capture(mailbox, owner, profile, request);
        let job = work.job(mailbox, owner, fixture::until()).unwrap();
        let mut worker = CaptureFreshnessWorker::new().unwrap();
        // Same original Work, but a separately prepared equal-byte capture. The
        // genuine resident allocation cannot adopt another capture's completion.
        worker
            .try_enqueue(Arc::new(products(install)), job.lease())
            .unwrap();
        let observation = ready(&mut worker);
        assert!(matches!(
            owner.certify_observed(observation, job, geometry.app.world_mut(), &installed),
            Err(ResidentFilterError::Source(CompositionError::Validation(
                super::super::super::products::Error::Identity
            )))
        ));
        no_native_constructors(owner, geometry);
        let other = capture(mailbox, owner, profile, request);
        owner
            .enqueue_observation(
                other.job(mailbox, owner, fixture::until()).unwrap(),
                &mut worker,
            )
            .unwrap();
        let observation = ready(&mut worker);
        let job = work.job(mailbox, owner, fixture::until()).unwrap();
        assert!(matches!(
            owner.certify_observed(observation, job, geometry.app.world_mut(), &installed),
            Err(ResidentFilterError::Source(CompositionError::Validation(
                super::super::super::products::Error::Identity
            )))
        ));
        no_native_constructors(owner, geometry);
        let work = capture(mailbox, owner, profile, request);
        let job = work.job(mailbox, owner, fixture::until()).unwrap();
        let lease = job.lease();
        let deadline = lease.deadline();
        // Retire before verification: cancellation during collision verification
        // can retain its nested source error rather than top-level Cancelled.
        let (entered, observed) = bounded(1);
        let (release, resume) = bounded(1);
        let mut worker = CaptureFreshnessWorker::with_test_gate(entered, resume, None).unwrap();
        owner.enqueue_observation(job, &mut worker).unwrap();
        observed
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .unwrap();
        assert!(lease.is_active(), "work expired before explicit retirement");
        drop(work);
        assert!(!lease.is_active());
        assert_eq!(lease.deadline(), deadline);
        assert!(matches!(worker.try_poll(), FreshnessPoll::Pending));
        release.try_send(()).unwrap();
        let watchdog = deadline;
        loop {
            match worker.try_poll() {
                FreshnessPoll::Pending => {
                    assert!(Instant::now() < watchdog);
                    std::thread::yield_now();
                }
                FreshnessPoll::Ready(result) => {
                    assert!(
                        matches!(result, Err(CompositionError::Cancelled)),
                        "retired work returned {:?}",
                        result.map(|_| "unexpected observation")
                    );
                    break;
                }
                _ => panic!("retired work lost its occupied slot"),
            }
        }
        assert!(matches!(worker.try_poll(), FreshnessPoll::Idle));
        assert_eq!(lease.deadline(), deadline);
        no_native_constructors(owner, geometry);
        assert!(mailbox.rx.is_empty() && mailbox.retry.is_empty());
    });
}
