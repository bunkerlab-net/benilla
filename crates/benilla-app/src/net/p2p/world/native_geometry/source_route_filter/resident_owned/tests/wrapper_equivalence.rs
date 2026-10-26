// SPDX-License-Identifier: GPL-3.0-or-later
//! Wrapper-only differential oracle, not G1 predicate or suspended-runner coverage.
use super::super::super::super::super::products::source_height::freshness_timings;
use super::*;

// Literal pre-extraction wrapper control flow. Only cache storage/bind plumbing differs.
fn old_literal_calculate_with_job<T>(
    owner: &mut OwnedResident,
    installed: &NativeInventorySource,
    deadline: Instant,
    cancelled: impl Fn() -> bool,
    job: Option<&ResidentDispatchJob<'_>>,
    calculate: impl for<'view> FnOnce(
        &mut ResidentFilterProvider<'_, 'view, 'view, 'static>,
        &InventoryCompatibility<'_, 'view, 'view, 'static>,
        &mut ResidentLedger,
    ) -> Result<T, ResidentFilterError>,
) -> Result<T, ResidentFilterError> {
    let OwnedResident {
        products,
        inventory,
        state,
        ledger,
        ..
    } = owner;
    products
        .with_height(
            || cancelled() || Instant::now() >= deadline,
            |height| {
                let result: Result<_, ResidentFilterError> = (|| {
                    let compatibility = inventory
                        .reborrow(height, deadline, &cancelled)
                        .map_err(ResidentFilterError::Source)?;
                    let operands = InvocationOperands::default();
                    let mut provider = ResidentFilterProvider {
                        state: ResidentStateStorage::Borrowed(state),
                        height,
                        invocation: None,
                    };
                    provider.verify_installed_owner(
                        &compatibility,
                        installed,
                        deadline,
                        &cancelled,
                    )?;
                    if let Some(job) = job {
                        provider.invocation =
                            Some(InvocationContext::bind(job, &provider, &operands)?);
                    }
                    let result = calculate(&mut provider, &compatibility, ledger)?;
                    provider.verify_installed_owner(
                        &compatibility,
                        installed,
                        deadline,
                        &cancelled,
                    )?;
                    Ok(result)
                })();
                Ok(result)
            },
        )
        .map_err(ResidentFilterError::Source)?
}

#[derive(Clone, Copy, Debug)]
enum Fault {
    None,
    CancelAt(usize),
    SourceAt(usize),
    InnerError,
    InnerErrorSource,
    InnerErrorCancel,
    Panic,
}

#[derive(Debug, PartialEq, Eq)]
enum Event {
    CancelPoll(usize),
    Callback,
    Match,
    Model,
    Height,
    Return,
}

fn compare_wrappers(
    install: &input::Install,
    owner: &mut OwnedResident,
    installed: &NativeInventorySource,
    session: &Session,
    profile: &Profile,
    request: &AuthorityRequest,
) {
    let (_client, mailbox) = mailbox(session.clone()).unwrap();
    let before = owner.ledger.diagnostic();
    let mut faults = vec![
        Fault::None,
        Fault::InnerError,
        Fault::InnerErrorSource,
        Fault::InnerErrorCancel,
        Fault::Panic,
    ];
    // Wrapper-only ordinals: F01, reborrow F02, entry-owner F03,
    // success-only exit-owner F58, outer with_height exit F59.
    // This diagnostic callback deliberately does not execute the G1/scene body.
    for slot in 1..=5 {
        faults.push(Fault::CancelAt(slot));
        faults.push(Fault::SourceAt(slot));
    }
    let mut successful_trace = None;
    for fault in faults {
        // Independent fault cases; neither member of a pair renews this Work.
        let deadline = fixture::until();
        let work = ResidentDispatchWork::capture(
            &mailbox,
            owner,
            profile,
            f::player(),
            991,
            request.clone(),
            deadline,
        )
        .unwrap();
        let job = work.job(&mailbox, owner, deadline).unwrap();
        let mut evidence = Vec::new();
        for old in [true, false] {
            let restore = RestoreInstall::new(install);
            let events = RefCell::new(Vec::new());
            let mutated = Cell::new(false);
            let callback_ran = Cell::new(false);
            let mutate = || {
                if !mutated.replace(true) {
                    let mut entries = input::navigation_entries();
                    entries
                        .iter_mut()
                        .find(|(name, _)| name == input::WDT)
                        .unwrap()
                        .1[20] ^= 1;
                    install.write(&entries);
                }
            };
            let cancelled = || {
                let slot = freshness_timings::current_source_trace_len();
                events.borrow_mut().push(Event::CancelPoll(slot));
                if matches!(fault, Fault::SourceAt(at) if slot >= at) {
                    mutate();
                }
                matches!(fault, Fault::CancelAt(at) if slot >= at)
                    || (matches!(fault, Fault::InnerErrorCancel) && callback_ran.get())
            };
            let ((outcome, counts), trace) = freshness_timings::trace_sources(|| {
                freshness_timings::measure_mode(freshness_timings::Mode::CountOnly, || {
                    std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        let calculate = |provider: &mut ResidentFilterProvider<'_, '_, '_, 'static>,
                                         _: &InventoryCompatibility<'_, '_, '_, 'static>,
                                         ledger: &mut ResidentLedger| {
                            callback_ran.set(true);
                            events.borrow_mut().push(Event::Callback);
                            assert_eq!(ledger.diagnostic(), before);
                            let proof = provider.invocation.as_ref().unwrap();
                            assert!(proof.operands.model.borrow().is_none());
                            assert!(proof.operands.height.borrow().is_none());
                            events.borrow_mut().push(Event::Match);
                            assert!(proof.matches(provider, request)?);
                            events.borrow_mut().push(Event::Model);
                            let model =
                                provider.chase_model_for(request, f::player(), deadline, &|| false)?;
                            events.borrow_mut().push(Event::Height);
                            let height =
                                provider.initialized_height_for(request, deadline, &|| false)?;
                            assert_eq!(proof.operands.model.borrow().as_ref(), Some(&model));
                            assert_eq!(*proof.operands.height.borrow(), Some(height));
                            events.borrow_mut().push(Event::Return);
                            match fault {
                                Fault::Panic => panic!("wrapper oracle callback unwind"),
                                Fault::InnerErrorSource => {
                                    mutate();
                                    Err(Failure::UnsupportedQuery.into())
                                }
                                Fault::InnerError | Fault::InnerErrorCancel => {
                                    Err(Failure::UnsupportedQuery.into())
                                }
                                _ => Ok((model, height)),
                            }
                        };
                        if old {
                            old_literal_calculate_with_job(
                                owner,
                                installed,
                                deadline,
                                &cancelled,
                                Some(&job),
                                calculate,
                            )
                        } else {
                            owner.calculate_with_job(
                                installed,
                                deadline,
                                &cancelled,
                                Some(&job),
                                calculate,
                            )
                        }
                    }))
                })
            });
            assert_eq!(owner.ledger.diagnostic(), before);
            assert_eq!(owner.ledger.counts(), (0, 0));
            assert_eq!(
                mutated.get(),
                matches!(fault, Fault::SourceAt(_) | Fault::InnerErrorSource)
            );
            assert!(job.lease().is_active());
            let expected_slots = match fault {
                Fault::None | Fault::CancelAt(4..=5) | Fault::SourceAt(4..=5) => 5,
                Fault::InnerError
                | Fault::InnerErrorSource
                | Fault::InnerErrorCancel
                | Fault::CancelAt(3)
                | Fault::SourceAt(3) => 4,
                Fault::Panic | Fault::CancelAt(2) | Fault::SourceAt(2) => 3,
                Fault::CancelAt(1) | Fault::SourceAt(1) => 1,
                _ => unreachable!(),
            };
            assert_eq!(trace.len(), expected_slots, "{fault:?}, old={old}");
            if matches!(fault, Fault::None) && old {
                successful_trace = Some(trace.clone());
            }
            let success = successful_trace.as_ref().unwrap();
            let expected_trace = match fault {
                Fault::None | Fault::CancelAt(4..=5) | Fault::SourceAt(4..=5) => success.clone(),
                Fault::InnerError
                | Fault::InnerErrorSource
                | Fault::InnerErrorCancel
                | Fault::CancelAt(3)
                | Fault::SourceAt(3) => [success[..3].to_vec(), success[4..].to_vec()].concat(),
                Fault::CancelAt(2) | Fault::SourceAt(2) => {
                    [success[..2].to_vec(), success[4..].to_vec()].concat()
                }
                Fault::Panic => success[..3].to_vec(),
                Fault::CancelAt(1) | Fault::SourceAt(1) => success[..1].to_vec(),
                _ => unreachable!(),
            };
            assert_eq!(trace, expected_trace, "original wrapper completion: {fault:?}");
            if callback_ran.get() {
                assert_eq!(counts.context_validation_calls(), 1);
                assert_eq!(counts.full_gate_model_calls(), 1);
                // Full bind validation plus the callback's first lazy-cache fill.
                assert_eq!(counts.initialized_owner_height_calls(), 2);
            }
            match fault {
                Fault::None => assert!(matches!(&outcome, Ok(Ok(_)))),
                Fault::Panic => assert!(outcome.is_err()),
                Fault::InnerError => assert!(matches!(
                    &outcome,
                    Ok(Err(ResidentFilterError::Context(Failure::UnsupportedQuery)))
                )),
                Fault::CancelAt(_) | Fault::InnerErrorCancel => assert!(matches!(
                    &outcome,
                    Ok(Err(ResidentFilterError::Source(CompositionError::Cancelled)))
                )),
                Fault::SourceAt(_) | Fault::InnerErrorSource => assert!(matches!(
                    &outcome,
                    Ok(Err(ResidentFilterError::Source(CompositionError::TerrainSource)))
                )),
            }
            assert_eq!(
                callback_ran.get(),
                matches!(
                    fault,
                    Fault::None
                        | Fault::InnerError
                        | Fault::InnerErrorSource
                        | Fault::InnerErrorCancel
                        | Fault::Panic
                        | Fault::CancelAt(4..=5)
                        | Fault::SourceAt(4..=5)
                )
            );
            // Errors lack PartialEq; compare complete variant/payload Debug on
            // the same fixture, not merely success/failure or Display text.
            let result = match outcome {
                Ok(result) => format!("{result:?}"),
                Err(_) => "callback panic".to_owned(),
            };
            evidence.push((result, trace, events.into_inner(), counts));
            drop(restore);
        }
        assert_eq!(evidence[0].0, evidence[1].0, "{fault:?}");
        assert_eq!(evidence[0].1, evidence[1].1, "{fault:?}");
        assert_eq!(evidence[0].2, evidence[1].2, "{fault:?}");
        assert!(evidence[0].3.same_counts(&evidence[1].3), "{fault:?}");
    }
}

struct WrapperProbe<'a> {
    consumer: Consumer<'a>,
    inspected: bool,
}
impl Authority for WrapperProbe<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
            assert!(!self.inspected);
            let installed = self.consumer.installed();
            compare_wrappers(
                self.consumer.install,
                &mut self.consumer.owner,
                &installed,
                &self.consumer.session,
                &self.consumer.profile,
                request,
            );
            self.inspected = true;
            return Err(NativeError::AuthorityUnavailable(
                "wrapper diagnostic never publishes",
            ));
        }
        self.consumer.resolve(request)
    }
}

#[test]
fn owned_wrapper_old_literal_and_operand_refactor_preserve_unwind_and_source_order() {
    with_owned(|install, owner, geometry, session, profile| {
        let mut probe = WrapperProbe {
            consumer: Consumer {
                install,
                phased_retained: false,
                owner,
                geometry,
                session,
                profile,
                authored: Default::default(),
                routes: vec![],
                constructor: None,
                foreign_index: None,
                fifo_probe: None,
                fifo_inspected: false,
                foreign_boundary: None,
            },
            inspected: false,
        };
        let mut world = NativeWorld::synthetic_component_test_with_initialization(
            probe.consumer.profile.clone(),
            probe.consumer.session.world,
            f::player(),
            1234,
            None,
            &mut probe,
        )
        .unwrap();
        let boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
        probe.consumer.owner.initialize(&boundary).unwrap();
        for tick in 1..=76 {
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
            } else {
                vec![]
            };
            let before = world.checkpoint().unwrap();
            let result = world.step_batch(tick, inputs, &mut probe);
            if probe.inspected {
                assert!(result.is_err());
                assert_eq!(world.checkpoint().unwrap(), before);
                break;
            }
            result.unwrap();
        }
        assert!(probe.inspected);
        assert!(probe.consumer.routes.is_empty());
        assert_eq!(probe.consumer.owner.ledger.counts(), (0, 0));
    });
}
