// SPDX-License-Identifier: GPL-3.0-or-later
//! Real source-frame evidence. Full FIFO/scene fault coverage is separate.
use super::super::super::super::super::{
    core::mailbox,
    products::{
        source_height::{
            freshness_timings::trace_sources,
            owned::freshness::{CaptureFreshnessWorker, FreshnessPoll},
        },
        vmap::installed_source_fixture as input,
        Error,
    },
    source_pathfinder_query::resident_joined_tests as fixture,
    source_route_same_polygon::tests::captured_route_request,
};
use super::*;
use std::cell::Cell;

fn with_scope(
    test: impl FnOnce(
        &input::Install,
        Arc<OwnedSourceProducts>,
        Rc<OwnedInventoryCompatibility>,
        Rc<OwnedInventoryCompatibility>,
        Rc<F05Guard>,
    ),
) {
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, _, compatibility, _, _, _, _, _| {
            let terrain =
                benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles())
                    .unwrap();
            let collision = benilla_native_vmap_source::prepare(install.path(), 0, || false)
                .unwrap()
                .into_map_bundle(input::profile(), || false)
                .unwrap()
                .unwrap();
            let products =
                Arc::new(OwnedSourceProducts::new(terrain, collision, || false).unwrap());
            let inventory = Rc::new(
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap(),
            );
            let foreign_inventory = Rc::new(
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap(),
            );
            let (session, request) = captured_route_request();
            let (_, mailbox) = mailbox(session).unwrap();
            let (lease, _reply) = mailbox
                .enqueue_service_test_request_until(705, request, fixture::until())
                .unwrap();
            let mut worker = CaptureFreshnessWorker::new().unwrap();
            worker
                .try_enqueue(Arc::clone(&products), lease.clone())
                .unwrap();
            let observation = loop {
                match worker.try_poll() {
                    FreshnessPoll::Pending => {
                        assert!(Instant::now() < lease.deadline());
                        std::thread::yield_now();
                    }
                    FreshnessPoll::Ready(result) => break result.unwrap(),
                    FreshnessPoll::Idle | FreshnessPoll::Stopped => {
                        panic!("original source job lost")
                    }
                }
            };
            let guard = Rc::new(
                observation
                    .into_f05_guard(&products, &lease, fixture::until())
                    .unwrap(),
            );
            test(install, products, inventory, foreign_inventory, guard);
            drop(worker);
        },
    );
}

#[test]
fn f05_source_frames_preserve_scope_exit_debt_and_exact_inventory_custody() {
    // A second applicable impl makes inference ambiguous if a capability becomes Clone.
    trait AmbiguousIfClone<A> {
        fn check() {}
    }
    impl<T: ?Sized> AmbiguousIfClone<()> for T {}
    impl<T: ?Sized + Clone> AmbiguousIfClone<u8> for T {}
    let _ = <f05_scope::F05HeightFrame as AmbiguousIfClone<_>>::check;
    let _ = <f05_scope::F05InventoryFrame as AmbiguousIfClone<_>>::check;
    let _ = <F05ResumePermit as AmbiguousIfClone<_>>::check;
    let _ = <InactiveF05Proof as AmbiguousIfClone<_>>::check;
    let _ = <F05Guard as AmbiguousIfClone<_>>::check;
    with_scope(|_, products, inventory, foreign_inventory, guard| {
        let (original, original_trace) = trace_sources(|| {
            products.with_height(
                || guard.cancelled() || Instant::now() >= guard.callback_deadline(),
                |height| {
                    let inner = inventory
                        .reborrow(height, guard.callback_deadline(), || guard.cancelled())
                        .map(|_| ());
                    Ok(inner)
                },
            )?
        });
        original.unwrap();
        assert_eq!(original_trace.len(), 3);
        let custody = Rc::new(());
        let invocation = Rc::new(());
        let (begun, trace) = trace_sources(|| {
            f05_scope::begin(
                Arc::clone(&products),
                Rc::clone(&inventory),
                Rc::clone(&custody),
                Rc::clone(&invocation),
                Rc::clone(&guard),
                |height, compatibility| {
                    assert_eq!(height.binding(), compatibility.source_binding().0);
                    Ok::<_, CompositionError>(())
                },
            )
        });
        let (height, captured_inventory, ()) = begun.unwrap();
        assert_eq!(trace.len(), 2, "original F01 and F02 only before park");
        let permit = F05ResumePermit {
            products: Arc::clone(&products),
            custody: Rc::clone(&custody),
            invocation: Rc::clone(&invocation),
            guard: Rc::clone(&guard),
        };
        for foreign_invocation in [false, true] {
            let foreign = F05ResumePermit {
                products: Arc::clone(&products),
                custody: if foreign_invocation {
                    Rc::clone(&custody)
                } else {
                    Rc::new(())
                },
                invocation: if foreign_invocation {
                    Rc::new(())
                } else {
                    Rc::clone(&invocation)
                },
                guard: Rc::clone(&guard),
            };
            let called = Cell::new(false);
            let (result, trace) = trace_sources(|| {
                f05_scope::with_f05_views(
                    &height,
                    &captured_inventory,
                    &inventory,
                    &foreign,
                    |_, _| {
                        called.set(true);
                        Ok::<_, CompositionError>(())
                    },
                )
            });
            assert!(matches!(
                result,
                Err(CompositionError::Validation(Error::Identity))
            ));
            assert!(!called.get());
            assert!(trace.is_empty());
        }
        let entered = Cell::new(false);
        let (result, trace) = trace_sources(|| {
            f05_scope::with_f05_views(
                &height,
                &captured_inventory,
                &foreign_inventory,
                &permit,
                |_, _| {
                    entered.set(true);
                    Ok::<_, CompositionError>(())
                },
            )
        });
        assert!(matches!(
            result,
            Err(CompositionError::Validation(Error::Identity))
        ));
        assert!(!entered.get());
        assert!(trace.is_empty());
        let (result, trace) = trace_sources(|| {
            f05_scope::with_f05_views(
                &height,
                &captured_inventory,
                &inventory,
                &permit,
                |view, compatibility| {
                    assert_eq!(view.binding(), compatibility.source_binding().0);
                    Ok::<_, CompositionError>(())
                },
            )
        });
        result.unwrap();
        assert!(
            trace.is_empty(),
            "view restoration adds no source observation"
        );
        let (result, trace) = trace_sources(|| height.finish(Ok::<_, CompositionError>(())));
        result.unwrap();
        assert_eq!(trace.len(), 1, "original owed F59");
        guard.check_success().unwrap();
    });
}

#[test]
fn f05_source_frames_inner_error_and_cancelled_exit_match_original_wrappers() {
    // Independent actual captures for each arm: cancellation is never reset.
    for fault in 0..3 {
        for original in [true, false] {
            with_scope(|install, products, inventory, _, guard| {
                let inner = || {
                    match fault {
                        0 => {}
                        1 => guard.cancel(),
                        2 => {
                            let mut changed = input::navigation_entries();
                            changed
                                .iter_mut()
                                .find(|(name, _)| name == input::WDT)
                                .unwrap()
                                .1[20] ^= 1;
                            install.write(&changed);
                        }
                        _ => unreachable!(),
                    }
                    Err::<(), _>(CompositionError::Validation(Error::Identity))
                };
                let (result, trace) = trace_sources(|| {
                    if original {
                        // Literal existing nested-Result calculate_with_job rule:
                        // F02/inner failure is saved while with_height pays F59.
                        products.with_height(
                            || guard.cancelled() || Instant::now() >= guard.callback_deadline(),
                            |height| {
                                let result = (|| {
                                    let _compatibility = inventory.reborrow(
                                        height,
                                        guard.callback_deadline(),
                                        || guard.cancelled(),
                                    )?;
                                    inner()
                                })();
                                Ok(result)
                            },
                        )?
                    } else {
                        f05_scope::begin(
                            Arc::clone(&products),
                            Rc::clone(&inventory),
                            Rc::new(()),
                            Rc::new(()),
                            Rc::clone(&guard),
                            |_, _| inner(),
                        )
                        .map(|_| ())
                    }
                });
                match fault {
                    0 => assert!(matches!(
                        result,
                        Err(CompositionError::Validation(Error::Identity))
                    )),
                    1 => assert!(matches!(result, Err(CompositionError::Cancelled))),
                    2 => assert!(matches!(result, Err(CompositionError::TerrainSource))),
                    _ => unreachable!(),
                }
                assert_eq!(
                    trace.len(),
                    3,
                    "F01/F02 then F59; original={original} fault={fault}"
                );
            });
        }
    }
}

#[test]
fn f05_source_frames_failed_entry_skips_reader_and_exit() {
    with_scope(|_, products, inventory, _, guard| {
        guard.cancel();
        let entered = Cell::new(false);
        let (result, trace) = trace_sources(|| {
            f05_scope::begin(
                products,
                inventory,
                Rc::new(()),
                Rc::new(()),
                guard,
                |_, _| {
                    entered.set(true);
                    Ok::<_, CompositionError>(())
                },
            )
        });
        assert!(matches!(result, Err(CompositionError::Cancelled)));
        assert!(!entered.get());
        assert_eq!(trace.len(), 1, "failed F01 never creates F59 debt");
    });
}

#[test]
fn f05_actual_fifo_dropped_receiver_retires_only_new_unacknowledged_stage() {
    use super::super::super::super::super::{
        core::resident_dispatch::transport::paired_controller_proof_mailbox,
        products::source_height::ResidentInitializationConfiguration,
        real_corridor_fixture::installed_source::InstalledFixture,
        source_creature_initialization::resident::ResidentCreatureInitializationProvider,
    };
    use benilla_sim::{creature_ai as ai, creature_motion as motion};
    use benilla_world::terrain_stream::NativeStaticMapIndex;
    use benilla_world_runtime::native::{
        synthetic_fixture as f, synthetic_motion_fixture::SyntheticMotionAuthority, Authority,
        AuthorityReply, Command, Input, MotionFacts, MotionQuery, NativeError, NativeWorld,
        Orientation, OwnedInput, Query,
    };
    use std::time::Duration;

    const CONFIG: Option<ResidentInitializationConfiguration> =
        Some(ResidentInitializationConfiguration::VmapHeightEnabled);
    struct Capture {
        owner: OwnedResident,
        geometry: InstalledFixture,
        session: Session,
        profile: Profile,
        authored: SyntheticMotionAuthority,
        cold: bool,
        retained: Option<AuthorityRequest>,
    }
    impl Authority for Capture {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            let installed = self
                .geometry
                .app
                .world()
                .resource::<NativeStaticMapIndex>()
                .source_provenance()
                .unwrap()
                .clone();
            let deadline = fixture::until();
            let facts = match &request.query {
                Query::CreatureInitialization(_) => self.owner.inspect(
                    &installed,
                    deadline,
                    || false,
                    |provider, _, _| {
                        ResidentCreatureInitializationProvider::bind(
                            self.session.world,
                            &self.profile,
                            provider.height,
                            CONFIG,
                            deadline,
                            || false,
                        )
                        .map_err(|_| Failure::Binding)?
                        .resolve(request, deadline, || false)
                        .map_err(|_| Failure::Binding.into())
                    },
                ),
                Query::PlayerEnvironment(_) => self.owner.inspect(
                    &installed,
                    deadline,
                    || false,
                    |provider, _, _| {
                        provider
                            .player_environment(f::player(), deadline, || false)
                            .map_err(|_| Failure::Binding)?
                            .resolve(&self.session, request, deadline, || false)
                            .map_err(|_| Failure::Binding.into())
                    },
                ),
                Query::SourceMotion(MotionQuery::NearPoint(_)) => self.owner.inspect(
                    &installed,
                    deadline,
                    || false,
                    |provider, compatibility, _| {
                        provider.resolve_dry_primary_near_point(
                            &self.session,
                            request,
                            f::player(),
                            compatibility,
                            CONFIG,
                            deadline,
                            || false,
                        )
                    },
                ),
                Query::SourceMotion(MotionQuery::Route(route)) => {
                    if route.request.prior_cache.is_some() {
                        assert!(self.cold);
                        assert!(self.retained.replace(request.clone()).is_none());
                        return Err(NativeError::AuthorityUnavailable(
                            "capture actual retained Work",
                        ));
                    }
                    assert!(!self.cold);
                    self.geometry.advance_physics();
                    let facts = self.owner.certify_cold(
                        self.geometry.app.world_mut(),
                        &installed,
                        request,
                        f::player(),
                        deadline,
                        || false,
                    );
                    self.cold = facts.is_ok();
                    facts
                }
                Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(bearing))) => {
                    let angle = benilla_sim::facing::bearing_bits(
                        [bearing.origin.x.bits(), bearing.origin.y.bits()],
                        [bearing.toward.x.bits(), bearing.toward.y.bits()],
                    )
                    .map_err(|_| NativeError::AuthorityBinding)?;
                    Ok(Facts::SourceMotion(MotionFacts::Spatial(Box::new(
                        motion::Reply {
                            request: motion::Query::Bearing(bearing.clone()),
                            fact: motion::Fact::Bearing(ai::Real::from_bits(angle).unwrap()),
                        },
                    ))))
                }
                Query::SourceMotion(MotionQuery::Spatial(_)) => {
                    return Err(NativeError::AuthorityUnavailable(
                        "unsupported fixture spatial query",
                    ));
                }
                _ => return self.authored.resolve(request),
            }
            .map_err(|_| NativeError::AuthorityBinding)?;
            Ok(AuthorityReply {
                request: request.clone(),
                facts,
            })
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.authored.observe_derived(reply)
        }
    }
    fn movement(world: &NativeWorld, sequence: u64, destination: ai::Point) -> OwnedInput {
        OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                player: world.state().player,
                tick: world.state().tick + 1,
                sequence,
                command: Command::MovementPose {
                    destination,
                    orientation: Orientation::new(0.5).unwrap(),
                    movement_flags: motion::GroundMovementFlags::STOPPED,
                },
            },
        }
    }
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, _, compatibility, inventory, policy, profile, query, _| {
            let session = fixture::session(&profile);
            let geometry = InstalledFixture::new(install.path(), inventory);
            let installed = geometry
                .app
                .world()
                .resource::<NativeStaticMapIndex>()
                .source_provenance()
                .unwrap()
                .clone();
            let terrain =
                benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles())
                    .unwrap();
            let collision = benilla_native_vmap_source::prepare(install.path(), 0, || false)
                .unwrap()
                .into_map_bundle(input::profile(), || false)
                .unwrap()
                .unwrap();
            let products = OwnedSourceProducts::new(terrain, collision, || false).unwrap();
            let inventory =
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap();
            let owner = OwnedResident::bind(
                products,
                inventory,
                &session,
                &profile,
                f::player(),
                query,
                policy,
                &installed,
                fixture::until(),
                || false,
            )
            .unwrap();
            let mut capture = Capture {
                owner,
                geometry,
                session,
                profile,
                authored: Default::default(),
                cold: false,
                retained: None,
            };
            let mut world = NativeWorld::synthetic_component_test_with_initialization(
                capture.profile.clone(),
                capture.session.world,
                f::player(),
                1234,
                None,
                &mut capture,
            )
            .unwrap();
            let mut boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
            capture.owner.initialize(&boundary).unwrap();
            for tick in 1..=76 {
                let before = world.checkpoint().unwrap();
                let position = world.state().position;
                let inputs = if tick == 1 {
                    (1..=2)
                        .map(|offset| {
                            movement(
                                &world,
                                world.state().next_sequence + offset - 1,
                                ai::Point::new(
                                    position.x.as_f32(),
                                    position.y.as_f32() - offset as f32,
                                    position.z.as_f32(),
                                )
                                .unwrap(),
                            )
                        })
                        .collect()
                } else if capture.cold {
                    vec![movement(
                        &world,
                        world.state().next_sequence,
                        ai::Point::new(
                            position.x.as_f32() - 0.125,
                            position.y.as_f32(),
                            position.z.as_f32(),
                        )
                        .unwrap(),
                    )]
                } else {
                    vec![]
                };
                let result = world.step_batch(tick, inputs, &mut capture);
                if capture.retained.is_some() {
                    assert!(result.is_err());
                    assert_eq!(world.checkpoint().unwrap(), before);
                    break;
                }
                result.unwrap();
                let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
                capture.owner.acknowledge(&notice).unwrap();
                boundary = notice.boundary().clone();
            }
            let request = capture
                .retained
                .take()
                .expect("genuine first retained dispatch");
            request
                .require_pathfinder_dispatch(&capture.profile)
                .unwrap();
            assert_eq!(capture.owner.ledger.counts(), (1, 1));
            let (_authority, mailbox) = paired_controller_proof_mailbox(&capture.owner).unwrap();
            let mut driver = ResidentF05Driver::new(capture.owner, &mailbox).unwrap();
            let before = driver.diagnostic();
            let (counts_before, next_id_before) = driver.ledger_counts();
            let deadline = fixture::until();
            let (lease, reply) = mailbox
                .enqueue_resident_test_request(&driver.owner, 805, request, deadline)
                .unwrap();
            drop(reply);
            assert!(
                lease.is_active(),
                "receiver disconnect is not a fabricated cancellation"
            );
            let wake = driver.completion_wake();
            while driver.ledger_counts().1 == next_id_before || driver.pending() {
                assert!(
                    Instant::now() < deadline,
                    "actual F05 dropped-receiver path stalled"
                );
                driver.serve(&mailbox, &mut capture.geometry, 1, deadline, |_, _| {
                    panic!("only original retained Work is queued")
                });
                let _ = wake.recv_timeout(Duration::from_millis(1));
            }
            assert_eq!(driver.f05_counts(), (1, 1));
            assert_eq!(driver.f05_raw_outcome(), Some("Ok(())"));
            assert_eq!(driver.f05_reentry_jobs(), 1);
            assert_eq!(driver.f05_outcome(), (true, None));
            assert!(driver.diagnostic().same_committed_root(&before));
            assert_eq!(driver.ledger_counts(), (counts_before, next_id_before + 1));
            assert_eq!(driver.ledger_counts().0, (1, 1));
        },
    );
}
