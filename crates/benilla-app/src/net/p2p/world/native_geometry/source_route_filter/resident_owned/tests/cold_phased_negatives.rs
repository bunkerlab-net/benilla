// SPDX-License-Identifier: GPL-3.0-or-later
//! Cold preparation is selected by the genuine FIFO, never a fabricated certificate.
use super::*;

#[test]
fn cold_fifo_prepared_refusals_preserve_original_work_and_uncommitted_ledger() {
    // Cancellation, selector drop and original Work expiry use the existing real
    // FIFO retirement hook; all other mutations occur between two owner calls.
    // MAX_CANDIDATES saturation is not manufactured here: under one-owner cold
    // admission, genuine competing dispatches fail earlier duplicate/conflict
    // checks. Mutation 18 exercises ID overflow, not vector capacity saturation.
    for mutation in [
        0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 21, 22, 23,
    ] {
        fifo_current_physics_probe_mode(false, FifoProbeMode::Phased(mutation));
    }
}

#[allow(clippy::too_many_arguments)]
pub(super) fn certify_cold_phased_probe(
    install: &input::Install,
    owner: &mut OwnedResident,
    geometry: &mut InstalledFixture,
    session: &Session,
    request: &AuthorityRequest,
    mutation: u8,
    mut foreign_index: Option<NativeStaticMapIndex>,
    foreign_boundary: Option<RouteCommitBoundary>,
) -> Result<Facts, ResidentFilterError> {
    assert!(request
        .require_pathfinder_dispatch(&owner.state.profile)
        .unwrap()
        .first_construction());
    assert_eq!(owner.ledger.counts(), (0, 0));
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap()
        .clone();
    let (_, mailbox) = super::super::super::super::super::core::resident_dispatch::transport::paired_controller_proof_mailbox(owner).unwrap();
    let mut selector = ResidentFifoSelection::new(&mailbox).unwrap();
    let deadline = fixture::until();
    let (lease, reply) = mailbox
        .enqueue_resident_test_request(owner, 904, request.clone(), deadline)
        .unwrap();
    let before = owner.ledger.diagnostic();
    let mut callbacks = 0;
    let mut first_deadline = if mutation == 5 {
        Instant::now() + Duration::from_secs(5)
    } else {
        deadline
    };
    while callbacks == 0 {
        assert!(Instant::now() < deadline);
        selector.serve_phased_results(
            &mailbox,
            owner,
            1,
            first_deadline,
            |owner, job, phase| {
                callbacks += 1;
                assert!(job.lease().same_job(&lease));
                assert!(std::ptr::eq(job.request(), lease.request()));
                assert!(matches!(&phase, ResidentPhase::Observation(_)));
                // A shorter invocation is installed using the SAME selected Work.
                // The selected job already clamps the outer invocation deadline.
                first_deadline = job.deadline();
                let result = owner
                    .certify_phase_result(phase, job, geometry.app.world_mut(), &installed)
                    .unwrap();
                assert!(matches!(&result, ResidentPhaseResult::Prepared(_)));
                Ok(result)
            },
            |_, _| panic!("cold Work must stay on resident FIFO"),
        );
        std::thread::yield_now();
    }
    assert_eq!(callbacks, 1);
    assert!(owner.ledger.diagnostic().same_semantic_state(&before));
    assert_eq!(owner.ledger.counts(), (0, 0));
    assert!(
        reply.try_result().is_err(),
        "prefix cannot return Facts or a seal"
    );
    assert!(lease.is_active());
    let constructor = owner
        .constructor_identity(&installed, request, deadline)
        .unwrap();
    let mut wrong = request.clone();
    wrong.binding.tick += 1;
    assert!(owner.state.query.retain_constructor(&wrong).is_err());
    assert!(owner.ledger.diagnostic().same_semantic_state(&before));
    let original_index = if mutation == 1 {
        let original = geometry
            .app
            .world_mut()
            .remove_resource::<NativeStaticMapIndex>()
            .unwrap();
        geometry.app.insert_resource(foreign_index.take().unwrap());
        Some(original)
    } else {
        None
    };
    let obstacle = if mutation == 2 {
        let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
            unreachable!()
        };
        let a = route.request.origin;
        let b = route.request.goal;
        let middle = [
            (a.x.as_f32() + b.x.as_f32()) * 0.5,
            (a.y.as_f32() + b.y.as_f32()) * 0.5,
            a.z.as_f32() + 1.,
        ];
        Some(
            geometry
                .app
                .world_mut()
                .spawn((
                    avian3d::prelude::Collider::cuboid(2., 3., 0.25),
                    avian3d::prelude::RigidBody::Static,
                    avian3d::prelude::CollisionLayers::DEFAULT,
                    bevy::prelude::Transform::from_translation(
                        benilla_assets::coords::wow_to_bevy(middle),
                    ),
                ))
                .id(),
        )
    } else {
        None
    };
    let restore = (11..=14)
        .contains(&mutation)
        .then(|| RestoreInstall::new(install));
    if (11..=14).contains(&mutation) {
        let mut entries = input::navigation_entries();
        match mutation {
            11 => entries = input::overlapping_entries(),
            12 => {
                entries
                    .iter_mut()
                    .find(|(name, _)| name == input::WDT)
                    .unwrap()
                    .1[20] ^= 1;
            }
            13 => entries.retain(|(name, _)| !name.ends_with("Selected_40_40.adt")),
            14 => {
                entries
                    .iter_mut()
                    .find(|(name, _)| name == input::EXTERIOR_ADT)
                    .unwrap()
                    .1[20] ^= 1;
            }
            _ => unreachable!(),
        }
        install.write(&entries);
    }

    let restore_ledger = (15..=19)
        .contains(&mutation)
        .then(|| owner.ledger.invalidate_prepared_cold_for_test(mutation));
    let restore_boundary = (mutation == 23).then(|| {
        let boundary = foreign_boundary.unwrap();
        assert_eq!(boundary.tick() + 1, request.binding.tick);
        owner.ledger.replace_cold_boundary_for_test(boundary)
    });
    let original_products = (mutation == 22).then(|| {
        let replacement = Arc::new(products(install));
        std::mem::replace(&mut owner.products, replacement)
    });
    let competing = if mutation == 21 {
        let profile = owner.state.profile.clone();
        let ready = observe_sources(owner, session, &profile, request);
        let first_id = owner.ledger.next_transport_candidate();
        let job = ready
            .work
            .job(&ready._mailbox, owner, fixture::until())
            .unwrap();
        let result = owner
            .certify_observed_result(ready.observation, job, geometry.app.world_mut(), &installed)
            .unwrap();
        Some((result, first_id))
    } else {
        None
    };
    if mutation == 5 {
        std::thread::sleep(first_deadline.saturating_duration_since(Instant::now()));
        assert!(
            lease.is_active(),
            "first invocation expiry must not be confused with Work expiry"
        );
    }
    let before_final = owner.ledger.diagnostic();
    geometry.advance_physics();
    let mut inspected = None;
    selector.serve_phased_results(
        &mailbox,
        owner,
        1,
        deadline,
        |owner, job, phase| {
            callbacks += 1;
            assert!(job.lease().same_job(&lease));
            assert!(matches!(&phase, ResidentPhase::Prepared(_)));
            let replacement = (mutation == 6).then(|| {
                ResidentDispatchWork::capture(
                    &mailbox,
                    owner,
                    &owner.state.profile,
                    f::player(),
                    905,
                    request.clone(),
                    deadline,
                )
                .unwrap()
            });
            let job = if let Some(replacement) = &replacement {
                replacement.job(&mailbox, owner, deadline).unwrap()
            } else {
                job
            };
            let resumed_installed = if mutation == 7 {
                foreign_index.as_ref().unwrap().source_provenance().unwrap()
            } else {
                &installed
            };
            let result =
                owner.certify_phase_result(phase, job, geometry.app.world_mut(), resumed_installed);
            inspected = Some(match result {
                Ok(ResidentPhaseResult::Complete(result)) => Ok(result.facts().clone()),
                Ok(ResidentPhaseResult::Prepared(_)) => {
                    panic!("cold final cannot yield or reconstruct")
                }
                Err(error) => Err(error),
            });
            // This component inspection never publishes the successful candidate.
            Err(Failure::UnsupportedQuery)
        },
        |_, _| panic!("cold Work cannot become ordinary"),
    );
    assert_eq!(callbacks, 2);
    assert!(matches!(
        reply.try_result(),
        Ok(Err(Failure::UnsupportedQuery))
    ));
    assert!(!lease.is_active());
    let result = inspected.expect("same selected Work must resume exactly once");
    if mutation != 0 {
        match mutation {
            1 | 7 | 15 | 16 | 19 | 21 | 23 => assert!(matches!(
                &result,
                Err(ResidentFilterError::Context(Failure::Binding))
            )),
            2 => assert!(matches!(
                &result,
                Err(ResidentFilterError::Context(Failure::UnsupportedNavigation))
            )),
            5 => assert!(matches!(
                &result,
                Err(ResidentFilterError::Source(CompositionError::Cancelled))
            )),
            6 | 22 => assert!(matches!(
                &result,
                Err(ResidentFilterError::Source(CompositionError::Validation(
                    super::super::super::super::super::products::Error::Identity
                )))
            )),
            11..=14 => assert!(matches!(&result, Err(ResidentFilterError::Source(_)))),
            17 => assert!(matches!(
                &result,
                Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
            )),
            18 => assert!(matches!(
                &result,
                Err(ResidentFilterError::Context(Failure::QueueCapacity))
            )),
            _ => panic!("unknown cold mutation {mutation}"),
        }
        assert!(owner.ledger.diagnostic().same_semantic_state(&before_final));
    }
    if let Some(restore) = restore_ledger {
        restore(&mut owner.ledger);
    }
    if let Some(restore) = restore_boundary {
        restore(&mut owner.ledger);
    }
    if let Some(products) = original_products {
        owner.products = products;
    }
    if let Some((result, first_id)) = competing {
        assert_eq!(owner.ledger.next_transport_candidate(), first_id + 1);
        owner
            .retire_unpublished_probe_result(result, first_id)
            .unwrap();
        assert_eq!(owner.ledger.next_transport_candidate(), first_id + 1);
        assert!(owner
            .ledger
            .diagnostic()
            .only_cold_candidate_retired_since(&before_final));
        assert!(owner.ledger.diagnostic().same_acknowledged_state(&before));
        assert_eq!(owner.ledger.counts(), (0, 0));
    } else if mutation != 0 {
        assert!(owner.ledger.diagnostic().same_semantic_state(&before));
    }
    if let Some(obstacle) = obstacle {
        geometry.app.world_mut().despawn(obstacle);
    }
    if let Some(original) = original_index {
        geometry.app.insert_resource(original);
    }
    drop(restore);
    assert_eq!(
        owner
            .constructor_identity(&installed, request, deadline)
            .unwrap(),
        constructor,
        "final/refusal must preserve the original native constructor allocation"
    );
    result
}

#[test]
fn cold_fifo_final_retry_drop_tail_full_and_post_mint_expiry_do_not_ack() {
    for mode in [
        FifoProbeMode::ColdTransport(false),
        FifoProbeMode::ColdTransport(true),
        FifoProbeMode::ColdTransportRetire(false),
        FifoProbeMode::ColdTransportRetire(true),
    ] {
        fifo_current_physics_probe_mode(false, mode);
    }
}
