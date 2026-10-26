// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual component runtime commits across source-view scopes, not durable controller saves.
use super::super::super::super::{
    core::{
        mailbox,
        resident_dispatch::{fifo::ResidentFifoSelection, ResidentDispatchWork},
    },
    products::{
        source_height::{
            owned::freshness::{CaptureFreshnessWorker, FreshnessObservation, FreshnessPoll},
            preparation_counts::{measure, Counts},
            CompositionError, ResidentInitializationConfiguration,
        },
        vmap::installed_source_fixture as input,
    },
    real_corridor_fixture::installed_source::InstalledFixture,
    source_creature_initialization::resident::ResidentCreatureInitializationProvider,
    source_pathfinder_query::resident_joined_tests as fixture,
};
use super::*;
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world::terrain_stream::NativeStaticMapIndex;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture::SyntheticMotionAuthority, Authority,
    AuthorityReply, AuthorityRequest, Command, Input, MotionFacts, MotionQuery, NativeError,
    NativeWorld, Orientation, OwnedInput, Profile, Query,
};
use std::{cell::Cell, collections::BTreeMap, path::PathBuf, time::Duration};

const CONFIGURATION: Option<ResidentInitializationConfiguration> =
    Some(ResidentInitializationConfiguration::VmapHeightEnabled);

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

fn moved_static<T: 'static>(value: T) -> T {
    value
}

fn with_owned(
    test: impl FnOnce(&input::Install, OwnedResident, InstalledFixture, Session, Profile),
) {
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, _, compatibility, inventory, policy, profile, query, _| {
            let session = fixture::session(&profile);
            let provenance =
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap();
            let geometry = InstalledFixture::new(install.path(), inventory);
            let installed = geometry
                .app
                .world()
                .resource::<NativeStaticMapIndex>()
                .source_provenance()
                .unwrap();
            let owner = OwnedResident::bind(
                products(install),
                provenance,
                &session,
                &profile,
                f::player(),
                query,
                policy,
                installed,
                fixture::until(),
                || false,
            )
            .unwrap();
            test(install, moved_static(owner), geometry, session, profile);
        },
    );
}

#[derive(Clone, Copy)]
enum FifoProbeMode {
    Clear,
    Phased(u8),
    Obstacle,
    Foreign,
    Transport(bool),
    ColdTransport(bool),
    TransportRefusal(bool),
    TransportRetire(bool),
    ColdTransportRetire(bool),
}
struct Consumer<'a> {
    install: &'a input::Install,
    phased_retained: bool,
    owner: OwnedResident,
    geometry: InstalledFixture,
    session: Session,
    profile: Profile,
    authored: SyntheticMotionAuthority,
    routes: Vec<AuthorityRequest>,
    constructor: Option<(usize, benilla_native_nav::SourceMapQuerySlot, usize)>,
    foreign_index: Option<NativeStaticMapIndex>,
    fifo_probe: Option<FifoProbeMode>,
    fifo_inspected: bool,
    foreign_boundary: Option<RouteCommitBoundary>,
}
impl Consumer<'_> {
    fn installed(&self) -> NativeInventorySource {
        self.geometry
            .app
            .world()
            .resource::<NativeStaticMapIndex>()
            .source_provenance()
            .unwrap()
            .clone()
    }
}
impl Authority for Consumer<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let installed = self.installed();
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
                        CONFIGURATION,
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
                        CONFIGURATION,
                        deadline,
                        || false,
                    )
                },
            ),
            Query::SourceMotion(MotionQuery::Route(route)) => {
                self.geometry.advance_physics();
                let before = self.owner.ledger.diagnostic();
                if let Some(mode) = self.fifo_probe {
                    let retained = !self.routes.is_empty();
                    if retained {
                        assert_eq!(self.routes.len(), 1);
                        assert_eq!(self.owner.ledger.counts(), (1, 1));
                        let original = self.routes[0]
                            .require_pathfinder_dispatch(&self.profile)
                            .unwrap();
                        let current = request.require_pathfinder_dispatch(&self.profile).unwrap();
                        assert!(!current.first_construction());
                        assert!(!original.same_live_dispatch(current));
                        assert_eq!(
                            current.construction_binding(),
                            original.construction_binding()
                        );
                        assert_eq!(route.request.prior_cache.as_ref().unwrap().revision, 1);
                    }
                    let facts = certify_fifo_probe(
                        self.install,
                        &mut self.owner,
                        &mut self.geometry,
                        &self.session,
                        request,
                        mode,
                        self.foreign_index.take(),
                        self.foreign_boundary.take(),
                    );
                    if matches!(
                        mode,
                        FifoProbeMode::Foreign
                            | FifoProbeMode::TransportRefusal(true)
                            | FifoProbeMode::Phased(1)
                    ) {
                        assert!(matches!(
                            facts,
                            Err(ResidentFilterError::Context(Failure::Binding))
                        ));
                    } else if matches!(
                        mode,
                        FifoProbeMode::Obstacle
                            | FifoProbeMode::TransportRefusal(false)
                            | FifoProbeMode::Phased(2)
                    ) {
                        assert!(matches!(
                            facts,
                            Err(ResidentFilterError::Context(Failure::UnsupportedNavigation))
                        ));
                    } else if matches!(mode, FifoProbeMode::Phased(3..)) {
                        assert!(
                            facts.is_err(),
                            "intervening revocation or identity mutation must refuse",
                        );
                    } else {
                        let Facts::SourceMotion(MotionFacts::Spatial(reply)) = facts.unwrap()
                        else {
                            panic!("spatial certificate");
                        };
                        let motion::Fact::RouteCalculation(calculation) = &reply.fact else {
                            panic!("route certificate");
                        };
                        calculation.validate_for(&route.request).unwrap();
                        assert_eq!(calculation.prior_cache, route.request.prior_cache);
                        assert_eq!(
                            calculation.after_cache.revision,
                            if retained { 2 } else { 1 }
                        );
                        assert!(matches!(
                            calculation.outcome,
                            motion::RouteOutcome::Complete(_)
                        ));
                    }
                    let after = self.owner.ledger.diagnostic();
                    if matches!(
                        mode,
                        FifoProbeMode::TransportRetire(_) | FifoProbeMode::ColdTransportRetire(_)
                    ) {
                        assert_eq!(
                            self.owner.ledger.counts(),
                            (usize::from(retained), usize::from(retained))
                        );
                    } else if let FifoProbeMode::Transport(drop_reply)
                    | FifoProbeMode::ColdTransport(drop_reply) = mode
                    {
                        assert_eq!(
                            self.owner.ledger.counts(),
                            (
                                usize::from(retained) + usize::from(!drop_reply),
                                usize::from(retained)
                            )
                        );
                    } else if matches!(mode, FifoProbeMode::Clear | FifoProbeMode::Phased(0)) {
                        if retained {
                            assert!(after.only_first_retained_staged_since(&before));
                            after.assert_first_retained_staging_rejects_mutations(&before);
                            assert_eq!(self.owner.ledger.counts(), (2, 1));
                        } else {
                            assert!(after.only_initial_cold_staged_since(&before));
                            after.assert_initial_cold_staging_rejects_mutations(&before);
                            assert_eq!(self.owner.ledger.counts(), (1, 0));
                        }
                    } else {
                        if matches!(mode, FifoProbeMode::Phased(21)) {
                            if retained {
                                assert!(after.same_committed_root(&before));
                            } else {
                                assert!(after.same_acknowledged_state(&before));
                            }
                        } else if retained || !matches!(mode, FifoProbeMode::Phased(_)) {
                            assert_eq!(after, before);
                        } else {
                            assert!(after.same_semantic_state(&before));
                        }
                        assert_eq!(
                            self.owner.ledger.counts(),
                            if retained { (1, 1) } else { (0, 0) }
                        );
                    }
                    if retained {
                        assert_eq!(
                            Some(
                                self.owner
                                    .constructor_identity(&installed, request, deadline)
                                    .unwrap()
                            ),
                            self.constructor,
                            "retained FIFO must use the original single native constructor"
                        );
                    }
                    self.fifo_inspected = true;
                    // Deliberately do not return inspected Facts to this runtime,
                    // the mailbox, or a RouteCommitNotice/ACK path.
                    return Err(NativeError::AuthorityUnavailable(
                        "FIFO component inspection is not publication",
                    ));
                }
                assert_invocation_context(
                    &mut self.owner,
                    &installed,
                    &self.session,
                    &self.profile,
                    request,
                    self.routes.first(),
                );
                let facts = if self.routes.is_empty() {
                    assert!(request
                        .require_pathfinder_dispatch(&self.profile)
                        .unwrap()
                        .first_construction());
                    assert!(route.request.prior_cache.is_none());
                    let ready = observe_sources(&self.owner, &self.session, &self.profile, request);
                    assert_eq!(self.owner.ledger.diagnostic(), before);
                    let foreign = self.foreign_index.take().unwrap();
                    let foreign_owner = foreign.source_provenance().unwrap();
                    assert_eq!(
                        installed.inventory_digest(),
                        foreign_owner.inventory_digest()
                    );
                    assert!(!installed.same_owner(foreign_owner));
                    let original = self
                        .geometry
                        .app
                        .world_mut()
                        .remove_resource::<NativeStaticMapIndex>()
                        .unwrap();
                    self.geometry.app.insert_resource(foreign);
                    // Pass the retained original provenance deliberately: certification
                    // must independently reject the actual replaced scene index.
                    let refused =
                        ready.certify(&mut self.owner, &mut self.geometry, &installed, deadline);
                    self.geometry.app.insert_resource(original);
                    self.geometry.advance_physics();
                    assert!(matches!(
                        refused,
                        Err(ResidentFilterError::Context(Failure::Binding))
                    ));
                    assert_eq!(self.owner.ledger.diagnostic(), before);
                    // An obstacle introduced after Ready must be seen by the
                    // current physics acquisition, not the worker's old success.
                    let ready = observe_sources(&self.owner, &self.session, &self.profile, request);
                    let a = route.request.origin;
                    let b = route.request.goal;
                    let middle = [
                        (a.x.as_f32() + b.x.as_f32()) * 0.5,
                        (a.y.as_f32() + b.y.as_f32()) * 0.5,
                        a.z.as_f32() + 1.,
                    ];
                    let obstacle = self
                        .geometry
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
                        .id();
                    let refused =
                        ready.certify(&mut self.owner, &mut self.geometry, &installed, deadline);
                    self.geometry.app.world_mut().despawn(obstacle);
                    assert!(matches!(
                        refused,
                        Err(ResidentFilterError::Context(Failure::UnsupportedNavigation))
                    ));
                    assert!(self.owner.ledger.diagnostic().same_committed_root(&before));
                    let ready = observe_sources(&self.owner, &self.session, &self.profile, request);
                    ready.certify(&mut self.owner, &mut self.geometry, &installed, deadline)
                } else {
                    assert_eq!(self.routes.len(), 1);
                    let original = self.routes[0]
                        .require_pathfinder_dispatch(&self.profile)
                        .unwrap();
                    let current = request.require_pathfinder_dispatch(&self.profile).unwrap();
                    assert!(!current.first_construction());
                    assert!(!original.same_live_dispatch(current));
                    assert_eq!(
                        current.construction_binding(),
                        original.construction_binding()
                    );
                    assert_eq!(route.request.prior_cache.as_ref().unwrap().revision, 1);
                    let ready = observe_sources(&self.owner, &self.session, &self.profile, request);
                    if self.phased_retained {
                        ready.certify_phased(
                            &mut self.owner,
                            &mut self.geometry,
                            &installed,
                            deadline,
                        )
                    } else {
                        ready.certify(&mut self.owner, &mut self.geometry, &installed, deadline)
                    }
                };
                let facts = facts.map_err(|error| {
                    eprintln!("scoped resident Route refused: {error:?}");
                    NativeError::AuthorityBinding
                })?;
                let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &facts else {
                    panic!("concrete spatial result")
                };
                let motion::Fact::RouteCalculation(calculation) = &spatial.fact else {
                    panic!("concrete Route calculation")
                };
                calculation.validate_for(&route.request).unwrap();
                assert_eq!(calculation.source, motion::RouteSource::OwnerPathFinder);
                assert!(matches!(
                    calculation.outcome,
                    motion::RouteOutcome::Complete(_)
                ));
                assert_eq!(calculation.prior_cache, route.request.prior_cache);
                assert_eq!(
                    calculation.after_cache.revision,
                    self.routes.len() as u64 + 1
                );
                if !self.routes.is_empty() {
                    assert!(self.owner.ledger.diagnostic().same_committed_root(&before));
                }
                let constructor = self
                    .owner
                    .constructor_identity(&installed, request, deadline)
                    .unwrap();
                assert_eq!(constructor.2, 1);
                if let Some(original) = self.constructor {
                    assert_eq!(constructor, original);
                } else {
                    self.constructor = Some(constructor);
                }
                self.routes.push(request.clone());
                Ok(facts)
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
                        fact: motion::Fact::Bearing(
                            ai::Real::from_bits(angle)
                                .map_err(|_| NativeError::AuthorityBinding)?,
                        ),
                    },
                ))))
            }
            Query::SourceMotion(MotionQuery::Spatial(_)) => {
                return Err(NativeError::AuthorityUnavailable(
                    "unsupported scoped spatial query",
                ));
            }
            _ => return self.authored.resolve(request),
        }
        .map_err(|error| {
            eprintln!("scoped resident observation refused: {error:?}");
            NativeError::AuthorityBinding
        })?;
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
}

fn assert_context_result(actual: Result<(), ResidentFilterError>, expected: Result<(), Failure>) {
    match actual {
        Ok(()) => assert_eq!(expected, Ok(())),
        Err(ResidentFilterError::Context(error)) => assert_eq!(expected, Err(error)),
        other => panic!("unexpected fallback {other:?}"),
    }
}

fn assert_dispatch_result(
    actual: Result<&benilla_world_runtime::native::PathfinderDispatchReceipt, ResidentFilterError>,
    expected: Result<&benilla_world_runtime::native::PathfinderDispatchReceipt, NativeError>,
) {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => {
            assert!(
                std::ptr::eq(actual, expected),
                "return the current request's receipt"
            );
            assert!(actual.same_live_dispatch(expected));
        }
        (Err(ResidentFilterError::Context(Failure::Binding)), Err(_)) => (),
        other => panic!("dispatch fallback differs from full validation: {other:?}"),
    }
}

fn assert_full_dispatch_fallback(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
    deadline: Instant,
) {
    use super::super::super::super::products::source_height::freshness_timings;
    let expected = request.require_pathfinder_dispatch(&provider.profile);
    let (actual, counts) =
        freshness_timings::measure(|| provider.dispatch_for(request, deadline, &|| false));
    assert_dispatch_result(actual, expected);
    assert_eq!(counts.full_resident_dispatch_calls(), 1);
}

fn assert_model_result(
    actual: Result<ChaseGateModel, ResidentFilterError>,
    expected: Result<ChaseGateModel, Failure>,
) {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
        (Err(ResidentFilterError::Context(actual)), Err(expected)) => assert_eq!(actual, expected),
        other => panic!("model fallback differs from full validation: {other:?}"),
    }
}

fn assert_full_model_fallback(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
    player: PlayerId,
    deadline: Instant,
) {
    use super::super::super::super::products::source_height::freshness_timings;
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        unreachable!()
    };
    let (expected, _) = freshness_timings::measure(|| {
        gate_model(&provider.profile, provider.session.world, player, route)
    });
    let (actual, counts) = freshness_timings::measure(|| {
        provider.chase_model_for(request, player, deadline, &|| false)
    });
    assert_model_result(actual, expected);
    assert_eq!(counts.full_gate_model_calls(), 1);
}

fn assert_height_result(
    actual: Result<Option<ai::Real>, ResidentFilterError>,
    expected: Result<Option<ai::Real>, Failure>,
) {
    match (actual, expected) {
        (Ok(actual), Ok(expected)) => assert_eq!(actual, expected),
        (Err(ResidentFilterError::Context(actual)), Err(expected)) => assert_eq!(actual, expected),
        other => panic!("height fallback differs from full validation: {other:?}"),
    }
}

fn assert_full_height_fallback(
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
    deadline: Instant,
) {
    use super::super::super::super::products::source_height::freshness_timings;
    let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
        unreachable!()
    };
    let (expected, _) = freshness_timings::measure(|| {
        super::super::super::initialized_owner_height(&provider.profile, route)
    });
    let (actual, counts) = freshness_timings::measure(|| {
        provider.initialized_height_for(request, deadline, &|| false)
    });
    assert_height_result(actual, expected);
    assert_eq!(counts.initialized_owner_height_calls(), 1);
}

// Exercise receipt reuse independently from route staging and publication.
fn assert_invocation_context(
    owner: &mut OwnedResident,
    installed: &NativeInventorySource,
    session: &Session,
    profile: &Profile,
    request: &AuthorityRequest,
    historical: Option<&AuthorityRequest>,
) {
    use super::super::super::super::products::source_height::freshness_timings;
    let (_client, mailbox) = mailbox(session.clone()).unwrap();
    let deadline = fixture::until();
    let capture = |ticket| {
        ResidentDispatchWork::capture(
            &mailbox,
            owner,
            profile,
            f::player(),
            ticket,
            request.clone(),
            deadline,
        )
        .unwrap()
    };
    let work = capture(801);
    let other_work = capture(802);
    let job = work.job(&mailbox, owner, deadline).unwrap();
    let other_job = other_work.job(&mailbox, owner, deadline).unwrap();
    let before = owner.ledger.diagnostic();
    let (result, counts) = freshness_timings::measure(|| {
        owner.calculate_with_job(
            installed,
            deadline,
            || false,
            Some(&job),
            |provider, _, ledger| {
                let proof = provider.invocation.as_ref().unwrap();
                assert!(proof.matches(provider, request)?);
                assert!(
                    proof.operands.model.borrow().is_none(),
                    "model gate stays lazy at bind"
                );
                let mut wrong_player = f::player();
                wrong_player.character[0] ^= 1;
                // A mismatched player must run the full validator on every call;
                // neither a refusal nor an unrelated success may fill this slot.
                for _ in 0..2 {
                    assert_full_model_fallback(provider, request, wrong_player, deadline);
                    assert!(proof.operands.model.borrow().is_none());
                }
                let Query::SourceMotion(MotionQuery::Route(route)) = &request.query else {
                    unreachable!()
                };
                let (expected, _) = freshness_timings::measure(|| {
                    gate_model(
                        &provider.profile,
                        provider.session.world,
                        f::player(),
                        route,
                    )
                });
                assert!(
                    proof.operands.height.borrow().is_none(),
                    "height stays lazy at bind"
                );
                let (expected_height, _) = freshness_timings::measure(|| {
                    super::super::super::initialized_owner_height(&provider.profile, route)
                });
                let expected_height = expected_height.unwrap();
                assert!(expected_height.is_some());
                let (first, counts) = freshness_timings::measure(|| {
                    provider.initialized_height_for(request, deadline, &|| false)
                });
                assert_eq!(first?, expected_height);
                assert_eq!(counts.initialized_owner_height_calls(), 1);
                let (repeated, counts) = freshness_timings::measure(|| {
                    provider.initialized_height_for(request, deadline, &|| false)
                });
                assert_eq!(repeated?, expected_height);
                assert_eq!(counts.initialized_owner_height_calls(), 0);
                assert_eq!(*proof.operands.height.borrow(), Some(expected_height));
                assert!(matches!(
                    provider.initialized_height_for(request, deadline, &|| true),
                    Err(ResidentFilterError::Source(CompositionError::Cancelled))
                ));
                assert!(matches!(
                    provider.initialized_height_for(request, Instant::now(), &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                // Storage/validator semantics only: these detached proofs do not
                // admit the altered route/profile as a live job.
                freshness_timings::measure(|| {
                    let empty_operands = InvocationOperands::default();
                    let empty = InvocationContext::bind(&job, provider, &empty_operands).unwrap();
                    let mut no_initialization = serde_json::to_value(route).unwrap();
                    no_initialization["owner"]["initialization"] = serde_json::Value::Null;
                    let no_initialization = serde_json::from_value(no_initialization).unwrap();
                    let (first, counts) = freshness_timings::measure(|| {
                        empty.initialized_height(&provider.profile, &no_initialization)
                    });
                    assert_eq!(first, Ok(None));
                    assert_eq!(counts.initialized_owner_height_calls(), 1);
                    assert_eq!(*empty.operands.height.borrow(), Some(None));
                    let (again, counts) = freshness_timings::measure(|| {
                        empty.initialized_height(&provider.profile, &no_initialization)
                    });
                    assert_eq!(again, Ok(None));
                    assert_eq!(counts.initialized_owner_height_calls(), 0);

                    let failed_operands = InvocationOperands::default();
                    let failed = InvocationContext::bind(&job, provider, &failed_operands).unwrap();
                    let mut invalid = provider.profile.clone();
                    invalid.creature_collision_heights = None;
                    for _ in 0..2 {
                        let (result, counts) = freshness_timings::measure(|| {
                            failed.initialized_height(&invalid, route)
                        });
                        assert!(result.is_err());
                        assert_eq!(counts.initialized_owner_height_calls(), 1);
                        assert!(failed.operands.height.borrow().is_none());
                    }
                });
                let expected_model = expected.unwrap();
                let (actual, first_model) = freshness_timings::measure(|| {
                    provider.chase_model_for(request, f::player(), deadline, &|| false)
                });
                assert_eq!(actual?, expected_model);
                assert_eq!(first_model.full_gate_model_calls(), 1);
                let (actual, reused_model) = freshness_timings::measure(|| {
                    provider.chase_model_for(request, f::player(), deadline, &|| false)
                });
                let returned = actual?;
                assert_eq!(returned, expected_model);
                assert_eq!(reused_model.full_gate_model_calls(), 0);
                assert!(!std::ptr::eq(
                    &returned,
                    proof.operands.model.borrow().as_ref().unwrap()
                ));
                drop(returned);
                assert_eq!(proof.operands.model.borrow().as_ref(), Some(&expected_model));
                assert_full_model_fallback(provider, request, wrong_player, deadline);
                assert_eq!(proof.operands.model.borrow().as_ref(), Some(&expected_model));
                assert!(matches!(
                    provider.chase_model_for(request, f::player(), deadline, &|| true),
                    Err(ResidentFilterError::Source(CompositionError::Cancelled))
                ));
                assert!(matches!(
                    provider.chase_model_for(request, f::player(), Instant::now(), &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                let (result, repeated) = freshness_timings::measure(|| {
                    for _ in 0..3 {
                        provider.check_context(request, deadline, &|| false)?;
                        assert_dispatch_result(
                            provider.dispatch_for(request, deadline, &|| false),
                            request.require_pathfinder_dispatch(profile),
                        );
                    }
                    Ok::<_, ResidentFilterError>(())
                });
                result?;
                assert_eq!(repeated.context_validation_calls(), 0);
                assert_eq!(repeated.full_resident_dispatch_calls(), 0);
                assert!(matches!(
                    provider.dispatch_for(request, deadline, &|| true),
                    Err(ResidentFilterError::Source(CompositionError::Cancelled))
                ));
                assert!(matches!(
                    provider.dispatch_for(request, Instant::now(), &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert!(matches!(
                    provider.check_context(request, deadline, &|| true),
                    Err(ResidentFilterError::Source(CompositionError::Cancelled))
                ));
                assert!(matches!(
                    provider.check_context(request, Instant::now(), &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));

                // Equal serialized bytes are not the live dispatch capability.
                let decoded: AuthorityRequest =
                    serde_json::from_value(serde_json::to_value(request).unwrap()).unwrap();
                assert_eq!(&decoded, request);
                assert!(decoded.require_pathfinder_dispatch(profile).is_err());
                assert!(!proof.matches(provider, &decoded)?);
                let expected =
                    validate_route_context(&provider.session, &provider.profile, &decoded);
                let (actual, fallback) = freshness_timings::measure(|| {
                    provider.check_context(&decoded, deadline, &|| false)
                });
                assert_context_result(actual, expected);
                assert_eq!(fallback.context_validation_calls(), 1);
                assert_full_dispatch_fallback(provider, &decoded, deadline);
                assert_full_model_fallback(provider, &decoded, f::player(), deadline);
                assert_full_height_fallback(provider, &decoded, deadline);

                for field in 0..6 {
                    let mut changed = request.clone();
                    match field {
                        0 => changed.binding.world[0] ^= 1,
                        1 => changed.binding.profile[0] ^= 1,
                        2 => changed.binding.tick += 1,
                        _ => {
                            let Query::SourceMotion(MotionQuery::Route(route)) = &mut changed.query
                            else {
                                unreachable!()
                            };
                            match field {
                                3 => {
                                    route.owner_position.x =
                                        ai::Real::new(route.owner_position.x.as_f32() + 1.).unwrap()
                                }
                                4 => {
                                    route.request.owner_collision_height = ai::Real::new(
                                        route.request.owner_collision_height.as_f32() + 1.,
                                    )
                                    .unwrap()
                                }
                                5 => {
                                    route.request.goal.x =
                                        ai::Real::new(route.request.goal.x.as_f32() + 1.).unwrap()
                                }
                                _ => unreachable!(),
                            }
                        }
                    }
                    assert_ne!(&changed, request);
                    assert!(!proof.matches(provider, &changed)?);
                    let expected =
                        validate_route_context(&provider.session, &provider.profile, &changed);
                    let (actual, measured) = freshness_timings::measure(|| {
                        provider.check_context(&changed, deadline, &|| false)
                    });
                    assert_context_result(actual, expected);
                    assert_eq!(measured.context_validation_calls(), 1);
                    assert_full_dispatch_fallback(provider, &changed, deadline);
                    assert_full_model_fallback(provider, &changed, f::player(), deadline);
                    assert_full_height_fallback(provider, &changed, deadline);
                }
                if let Some(historical) = historical {
                    assert!(!proof.matches(provider, historical)?);
                    let (actual, measured) = freshness_timings::measure(|| {
                        provider.check_context(historical, deadline, &|| false)
                    });
                    actual?;
                    assert_eq!(measured.context_validation_calls(), 1);
                    assert_full_dispatch_fallback(provider, historical, deadline);
                    assert_full_model_fallback(provider, historical, f::player(), deadline);
                    assert_full_height_fallback(provider, historical, deadline);
                }

                // Bindings are checked even if a test transplants the private proof.
                let mut proof = provider.invocation.take().unwrap();
                let saved_lease = proof.lease.clone();
                proof.lease = other_job.lease();
                assert!(matches!(
                    proof.matches(provider, request),
                    Err(ResidentFilterError::Context(Failure::Binding))
                ));
                provider.invocation = Some(proof);
                assert!(matches!(
                    provider.dispatch_for(request, deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Binding))
                ));
                assert!(matches!(
                    provider.chase_model_for(request, f::player(), deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Binding))
                ));
                assert!(matches!(
                    provider.initialized_height_for(request, deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Binding))
                ));
                let mut proof = provider.invocation.take().unwrap();
                proof.lease = saved_lease;
                provider.invocation = Some(proof);
                let saved_identity = provider.state.identity.clone();
                provider.state.identity = Rc::new(());
                assert!(!provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .matches(provider, request)?);
                assert_full_dispatch_fallback(provider, request, deadline);
                assert_full_model_fallback(provider, request, f::player(), deadline);
                assert_full_height_fallback(provider, request, deadline);
                let invalid_operands = InvocationOperands::default();
                assert!(InvocationContext::bind(&job, provider, &invalid_operands).is_err());
                provider.state.identity = saved_identity;
                provider.state.profile.tick_ms += 1;
                assert!(!provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .matches(provider, request)?);
                assert_full_dispatch_fallback(provider, request, deadline);
                assert_full_model_fallback(provider, request, f::player(), deadline);
                assert_full_height_fallback(provider, request, deadline);
                provider.state.profile.tick_ms -= 1;
                provider.state.session.world[0] ^= 1;
                assert!(!provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .matches(provider, request)?);
                assert_full_dispatch_fallback(provider, request, deadline);
                assert_full_model_fallback(provider, request, f::player(), deadline);
                assert_full_height_fallback(provider, request, deadline);
                provider.state.session.world[0] ^= 1;
                assert_eq!(
                    provider
                        .invocation
                        .as_ref()
                        .unwrap()
                        .operands
                        .model
                        .borrow()
                        .as_ref(),
                    Some(&expected_model)
                );
                assert_eq!(
                    *provider.invocation.as_ref().unwrap().operands.height.borrow(),
                    Some(expected_height)
                );
                assert_eq!(ledger.diagnostic(), before);
                Ok(())
            },
        )
    });
    result.unwrap();
    // Exactly one fresh full validation mints this invocation's receipt. The
    // explicit baseline validations in the mutation loop are counted separately.
    assert_eq!(counts.context_validation_calls(), 8);
    assert_eq!(
        counts.full_resident_dispatch_calls(),
        1,
        "fresh invocation mint remains fully validated"
    );
    assert_eq!(owner.ledger.diagnostic(), before);
    owner
        .inspect(
            installed,
            deadline,
            || false,
            |provider, _, _| {
                assert!(
                    provider.invocation.is_none(),
                    "receipt must not persist across views"
                );
                assert_full_dispatch_fallback(provider, request, deadline);
                assert_full_model_fallback(provider, request, f::player(), deadline);
                assert_full_height_fallback(provider, request, deadline);
                Ok(())
            },
        )
        .unwrap();
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = owner.calculate_with_job::<()>(
            installed,
            deadline,
            || false,
            Some(&job),
            |provider, _, _| {
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .model
                    .borrow()
                    .is_none());
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .height
                    .borrow()
                    .is_none());
                let (_, counts) = freshness_timings::measure(|| {
                    provider
                        .initialized_height_for(request, deadline, &|| false)
                        .unwrap()
                });
                assert_eq!(counts.initialized_owner_height_calls(), 1);
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .height
                    .borrow()
                    .is_some());
                provider.chase_model_for(request, f::player(), deadline, &|| false)?;
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .model
                    .borrow()
                    .is_some());
                panic!("temporary context receipt must unwind with its provider");
            },
        );
    }));
    assert!(unwound.is_err());
    owner
        .inspect(
            installed,
            deadline,
            || false,
            |provider, _, ledger| {
                assert!(provider.invocation.is_none());
                assert_eq!(ledger.diagnostic(), before);
                Ok(())
            },
        )
        .unwrap();
    owner
        .calculate_with_job(
            installed,
            deadline,
            || false,
            Some(&job),
            |provider, _, ledger| {
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .model
                    .borrow()
                    .is_none());
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .height
                    .borrow()
                    .is_none());
                let (_, counts) = freshness_timings::measure(|| {
                    provider
                        .initialized_height_for(request, deadline, &|| false)
                        .unwrap()
                });
                assert_eq!(counts.initialized_owner_height_calls(), 1);
                assert!(provider
                    .invocation
                    .as_ref()
                    .unwrap()
                    .operands
                    .height
                    .borrow()
                    .is_some());
                provider.chase_model_for(request, f::player(), deadline, &|| false)?;
                drop(mailbox);
                assert!(!job.lease().is_active());
                assert!(matches!(
                    provider.initialized_height_for(request, deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert!(matches!(
                    provider.chase_model_for(request, f::player(), deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert!(matches!(
                    provider.dispatch_for(request, deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert!(matches!(
                    provider.check_context(request, deadline, &|| false),
                    Err(ResidentFilterError::Context(Failure::Deadline))
                ));
                assert_eq!(ledger.diagnostic(), before);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(owner.ledger.diagnostic(), before);
}

// Holds the original Work and worker through one caller-thread attempt. No query
// or ECS view survives the polling boundary.
struct ReadySources {
    _worker: CaptureFreshnessWorker,
    _mailbox: super::super::super::super::core::Mailbox,
    work: ResidentDispatchWork,
    observation: FreshnessObservation,
}
impl ReadySources {
    fn certify_phased(
        self,
        owner: &mut OwnedResident,
        geometry: &mut InstalledFixture,
        installed: &NativeInventorySource,
        deadline: Instant,
    ) -> Result<Facts, ResidentFilterError> {
        let job = self.work.job(&self._mailbox, owner, deadline)?;
        let phase = owner.certify_phase_result_retained_only(
            ResidentPhase::Observation(Ok(self.observation)),
            job,
            geometry.app.world_mut(),
            installed,
        )?;
        let ResidentPhaseResult::Prepared(prepared) = phase else {
            panic!("retained prefix cannot return Facts");
        };
        geometry.advance_physics();
        let job = self.work.job(&self._mailbox, owner, deadline)?;
        match owner.certify_phase_result_retained_only(
            ResidentPhase::Prepared(prepared),
            job,
            geometry.app.world_mut(),
            installed,
        )? {
            ResidentPhaseResult::Complete(result) => Ok(result.facts().clone()),
            ResidentPhaseResult::Prepared(_) => panic!("final phase cannot yield again"),
        }
    }
    fn certify(
        self,
        owner: &mut OwnedResident,
        geometry: &mut InstalledFixture,
        installed: &NativeInventorySource,
        deadline: Instant,
    ) -> Result<Facts, ResidentFilterError> {
        // Physics is advanced after polling, not inferred fresh from a new borrow.
        geometry.advance_physics();
        let job = self.work.job(&self._mailbox, owner, deadline)?;
        owner.certify_observed(self.observation, job, geometry.app.world_mut(), installed)
    }
}

fn observe_sources(
    owner: &OwnedResident,
    session: &Session,
    profile: &Profile,
    request: &AuthorityRequest,
) -> ReadySources {
    let (_client, mailbox) = mailbox(session.clone()).unwrap();
    let deadline = fixture::until();
    let work = ResidentDispatchWork::capture(
        &mailbox,
        owner,
        profile,
        f::player(),
        701,
        request.clone(),
        deadline,
    )
    .unwrap();
    let job = work.job(&mailbox, owner, deadline).unwrap();
    let lease = job.lease();
    let mut worker = CaptureFreshnessWorker::new().unwrap();
    owner.enqueue_observation(job, &mut worker).unwrap();
    loop {
        match worker.try_poll() {
            FreshnessPoll::Pending => {
                assert!(Instant::now() < deadline, "source worker did not complete");
                std::thread::yield_now();
            }
            FreshnessPoll::Ready(result) => {
                assert_eq!(lease.deadline(), deadline);
                return ReadySources {
                    _worker: worker,
                    _mailbox: mailbox,
                    work,
                    observation: result.unwrap(),
                };
            }
            FreshnessPoll::Idle | FreshnessPoll::Stopped => panic!("source job disappeared"),
        }
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

#[test]
fn scoped_resident_keeps_cold_ack_and_retained_native_owner_across_views() {
    for phased_retained in [false, true] {
        scoped_retained_owner_across_views(phased_retained);
    }
}

fn scoped_retained_owner_across_views(phased_retained: bool) {
    with_owned(|install, mut owner, geometry, session, profile| {
        let ((), preparations) = measure(|| {
            let bytes = install.bytes();
            let installed = geometry
                .app
                .world()
                .resource::<NativeStaticMapIndex>()
                .source_provenance()
                .unwrap()
                .clone();
            let native = owner.state.query.nav_query().retain_default();
            let witnesses = owner
                .inspect(
                    &installed,
                    fixture::until(),
                    || false,
                    |provider, _, ledger| {
                        assert_eq!(ledger.counts(), (0, 0));
                        assert_eq!(provider.retained_constructor_count(), 0);
                        Ok(provider.liveness_for_reconstruction_test())
                    },
                )
                .unwrap();
            let mut consumer = Consumer {
                install,
                phased_retained,
                owner: moved_static(owner),
                geometry,
                session,
                profile,
                authored: Default::default(),
                routes: vec![],
                constructor: None,
                foreign_index: Some(
                    NativeStaticMapIndex::from_inventory(input::raw_inventory(install)).unwrap(),
                ),
                fifo_probe: None,
                fifo_inspected: false,
                foreign_boundary: None,
            };
            let mut world = NativeWorld::synthetic_component_test_with_initialization(
                consumer.profile.clone(),
                consumer.session.world,
                f::player(),
                1234,
                None,
                &mut consumer,
            )
            .unwrap();
            let mut boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
            consumer.owner.initialize(&boundary).unwrap();
            let mut cold_liveness = None;
            let mut cold_notice = None;
            for _ in 0..76 {
                let tick = world.state().tick + 1;
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
                } else if !consumer.routes.is_empty() {
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
                world.step_batch(tick, inputs, &mut consumer).unwrap();
                let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
                let installed = consumer.installed();
                if !notice.routes().is_empty() {
                    let before = consumer.owner.ledger.diagnostic();
                    consumer
                        .owner
                        .inspect(
                            &installed,
                            fixture::until(),
                            || false,
                            |provider, _, ledger| {
                                native.check_query(provider.query.nav_query()).unwrap();
                                assert_eq!(ledger.diagnostic(), before);
                                assert_eq!(provider.retained_constructor_count(), 1);
                                assert!(std::rc::Weak::ptr_eq(
                                    &witnesses.0,
                                    &provider.liveness_for_reconstruction_test().0
                                ));
                                assert!(std::rc::Weak::ptr_eq(
                                    &witnesses.1,
                                    &provider.liveness_for_reconstruction_test().1
                                ));
                                Ok(())
                            },
                        )
                        .unwrap();
                    assert_eq!(
                        consumer.owner.ledger.counts(),
                        (consumer.routes.len(), consumer.routes.len() - 1)
                    );
                    if consumer.routes.len() == 1 {
                        cold_liveness =
                            Some(consumer.owner.ledger.cold_liveness_for_test().unwrap());
                        assert!(cold_liveness.as_ref().unwrap().is_alive());
                    }
                }
                // A successful off-thread observation cannot substitute for current
                // installation or source validation, and it cannot promote the ledger.
                let restore = if !notice.routes().is_empty() {
                    let request = consumer.routes.last().unwrap().clone();
                    let before = consumer.owner.ledger.diagnostic();
                    let ready = observe_sources(
                        &consumer.owner,
                        &consumer.session,
                        &consumer.profile,
                        &request,
                    );
                    assert_eq!(consumer.owner.ledger.diagnostic(), before);
                    let restore = RestoreInstall::new(install);
                    install.write(&input::overlapping_entries());
                    assert!(matches!(
                        ready.certify(
                            &mut consumer.owner,
                            &mut consumer.geometry,
                            &installed,
                            fixture::until(),
                        ),
                        Err(ResidentFilterError::Source(_))
                    ));
                    assert_eq!(consumer.owner.ledger.diagnostic(), before);
                    Some(restore)
                } else {
                    None
                };
                // The genuine runtime already committed. ACK stays atomic and succeeds
                // even while source files have changed; no trailing freshness can undo it.
                consumer.owner.acknowledge(&notice).unwrap();
                drop(restore);
                boundary = notice.boundary().clone();
                let acknowledged = consumer.owner.ledger.diagnostic();
                consumer.owner.acknowledge(&notice).unwrap();
                assert_eq!(consumer.owner.ledger.diagnostic(), acknowledged);
                if !notice.routes().is_empty() && consumer.routes.len() == 1 {
                    assert_eq!(consumer.owner.ledger.counts(), (1, 1));
                    assert!(cold_liveness.as_ref().unwrap().is_alive());
                    cold_notice = Some(notice);
                    consumer.owner = moved_static(consumer.owner);
                    let root = consumer.owner.ledger.diagnostic();
                    let restore = RestoreInstall::new(install);
                    assert!(consumer
                        .owner
                        .inspect(
                            &installed,
                            fixture::until(),
                            || false,
                            |_, _, _| {
                                install.write(&input::overlapping_entries());
                                Ok(())
                            }
                        )
                        .is_err());
                    assert_eq!(consumer.owner.ledger.diagnostic(), root);
                    assert!(cold_liveness.as_ref().unwrap().is_alive());
                    restore.restore();
                    assert!(consumer
                        .owner
                        .inspect::<()>(
                            &installed,
                            fixture::until(),
                            || true,
                            |_, _, _| {
                                panic!("cancelled scope must not inspect the retained ledger")
                            }
                        )
                        .is_err());
                    assert_eq!(consumer.owner.ledger.diagnostic(), root);
                }
                if consumer.routes.len() == 2 {
                    assert_eq!(consumer.owner.ledger.counts(), (1, 1));
                    let cold = cold_notice.as_ref().unwrap();
                    assert_eq!(cold.boundary().heads()[0].cache().unwrap().revision, 1);
                    assert_eq!(boundary.heads()[0].cache().unwrap().revision, 2);
                    assert!(cold_liveness.as_ref().unwrap().is_alive());
                    break;
                }
            }
            assert_eq!(
                consumer.routes.len(),
                2,
                "genuine retained dispatch reached"
            );
            assert_eq!(install.bytes(), bytes);
            assert!(witnesses.0.strong_count() > 0 && witnesses.1.strong_count() > 0);
            drop(native); // This diagnostic lease must not keep native storage alive.
            drop(consumer);
            assert_eq!(witnesses.0.strong_count(), 0);
            assert_eq!(witnesses.1.strong_count(), 0);
            assert!(!cold_liveness.unwrap().is_alive());
            assert_eq!(install.bytes(), bytes);
        });
        assert_eq!(preparations, Counts::default());
    });
}

struct RestoreInstall {
    path: PathBuf,
    bytes: BTreeMap<String, Vec<u8>>,
}
impl RestoreInstall {
    fn new(install: &input::Install) -> Self {
        Self {
            path: install.path().to_path_buf(),
            bytes: install.bytes(),
        }
    }
    fn restore(&self) {
        for (name, bytes) in &self.bytes {
            std::fs::write(self.path.join(name), bytes).unwrap();
        }
    }
}
impl Drop for RestoreInstall {
    fn drop(&mut self) {
        self.restore();
    }
}

#[test]
fn scoped_resident_refuses_foreign_owner_expiry_cancel_and_changed_sources_without_state_loss() {
    with_owned(|install, mut owner, geometry, _, _| {
        let restore = RestoreInstall::new(install);
        let installed = geometry
            .app
            .world()
            .resource::<NativeStaticMapIndex>()
            .source_provenance()
            .unwrap()
            .clone();
        let foreign = input::raw_inventory(install);
        let foreign = foreign.source_provenance().unwrap();
        assert_eq!(installed.inventory_digest(), foreign.inventory_digest());
        assert!(!installed.same_owner(foreign));
        let before = owner.ledger.diagnostic();
        let called = Cell::new(false);
        for (source, deadline, cancelled) in [
            (foreign, fixture::until(), false),
            (&installed, Instant::now() - Duration::from_millis(1), false),
            (&installed, fixture::until(), true),
        ] {
            assert!(owner
                .inspect(
                    source,
                    deadline,
                    || cancelled,
                    |_, _, _| {
                        called.set(true);
                        Ok(())
                    }
                )
                .is_err());
            assert!(!called.get());
            assert_eq!(owner.ledger.diagnostic(), before);
        }
        let cancelled = Cell::new(false);
        assert!(matches!(
            owner.inspect(
                &installed,
                fixture::until(),
                || cancelled.get(),
                |_, _, _| {
                    cancelled.set(true);
                    Ok(())
                }
            ),
            Err(ResidentFilterError::Source(CompositionError::Cancelled))
        ));
        assert_eq!(owner.ledger.diagnostic(), before);

        assert!(
            owner
                .inspect(
                    &installed,
                    fixture::until(),
                    || false,
                    |_, _, _| {
                        install.write(&input::overlapping_entries());
                        Ok(())
                    }
                )
                .is_err(),
            "post-scope freshness failure must suppress success"
        );
        assert_eq!(owner.ledger.diagnostic(), before);
        assert!(owner
            .inspect::<()>(
                &installed,
                fixture::until(),
                || false,
                |_, _, _| { panic!("changed source must fail before reader") }
            )
            .is_err());
        restore.restore();
        std::fs::remove_file(install.path().join("model.MPQ")).unwrap();
        assert!(owner
            .inspect::<()>(
                &installed,
                fixture::until(),
                || false,
                |_, _, _| { panic!("missing source must fail before reader") }
            )
            .is_err());
        assert_eq!(owner.ledger.diagnostic(), before);
        restore.restore();
        owner
            .inspect(
                &installed,
                fixture::until(),
                || false,
                |provider, _, ledger| {
                    assert_eq!(ledger.diagnostic(), before);
                    assert_eq!(provider.retained_constructor_count(), 0);
                    Ok(())
                },
            )
            .unwrap();
    });
}

#[test]
fn scoped_resident_bind_rejects_equal_byte_foreign_inventory_and_different_source_products() {
    fixture::with_selected_controller_fixture(
        fixture::ControllerOwners::Single,
        true,
        false,
        false,
        false,
        fixture::GeneratedSupport::Disabled,
        |install, height, compatibility, inventory, policy, profile, query, other| {
            let session = fixture::session(&profile);
            let installed = inventory.source_provenance().unwrap();
            let foreign = input::raw_inventory(install);
            let foreign_compatibility = height
                .match_raw_inventory(&foreign, fixture::until(), || false)
                .unwrap();
            let foreign_owned = OwnedInventoryCompatibility::capture(
                &foreign_compatibility,
                fixture::until(),
                || false,
            )
            .unwrap();
            assert!(OwnedResident::bind(
                products(install),
                foreign_owned,
                &session,
                &profile,
                f::player(),
                query,
                policy.clone(),
                installed,
                fixture::until(),
                || false
            )
            .is_err());
            let original =
                OwnedInventoryCompatibility::capture(compatibility, fixture::until(), || false)
                    .unwrap();
            let different = input::install();
            assert!(OwnedResident::bind(
                products(&different),
                original,
                &session,
                &profile,
                f::player(),
                other,
                policy,
                installed,
                fixture::until(),
                || false
            )
            .is_err());
        },
    );
}

/// The callback inspects a genuine certificate, but the original FIFO reply is a
/// refusal and the caller's runtime deliberately rolls back instead of publishing.
#[allow(clippy::too_many_arguments)]
fn certify_fifo_probe(
    install: &input::Install,
    owner: &mut OwnedResident,
    geometry: &mut InstalledFixture,
    session: &Session,
    request: &AuthorityRequest,
    mode: FifoProbeMode,
    mut foreign_index: Option<NativeStaticMapIndex>,
    foreign_boundary: Option<RouteCommitBoundary>,
) -> Result<Facts, ResidentFilterError> {
    if let FifoProbeMode::Phased(mutation @ 8..=10) = mode {
        return super::super::super::super::core::resident_dispatch::fifo::exercise_prepared_retirement(owner, geometry, request, mutation);
    }
    if let FifoProbeMode::Phased(mutation) = mode {
        if request
            .require_pathfinder_dispatch(&owner.state.profile)
            .unwrap()
            .first_construction()
        {
            if matches!(mutation, 3 | 4) {
                return super::super::super::super::core::resident_dispatch::fifo::exercise_prepared_retirement(
                    owner, geometry, request, mutation,
                );
            }
            return cold_phased_negatives::certify_cold_phased_probe(
                install,
                owner,
                geometry,
                session,
                request,
                mutation,
                foreign_index,
                foreign_boundary,
            );
        }
        return certify_phased_probe(
            install,
            owner,
            geometry,
            session,
            request,
            mutation,
            foreign_index,
        );
    }
    if let FifoProbeMode::TransportRetire(expire) | FifoProbeMode::ColdTransportRetire(expire) =
        mode
    {
        return Ok(super::super::super::super::core::resident_dispatch::transport::exercise_result_terminal_retirement(
            owner, geometry, request, expire, matches!(mode, FifoProbeMode::ColdTransportRetire(_)),
        ));
    }
    if let FifoProbeMode::TransportRefusal(foreign) = mode {
        return super::super::super::super::core::resident_dispatch::transport::exercise_result_refusal(
            owner, geometry, request, foreign.then(|| foreign_index.take().unwrap()),
        );
    }
    if let FifoProbeMode::Transport(drop_reply) | FifoProbeMode::ColdTransport(drop_reply) = mode {
        return Ok(super::super::super::super::core::resident_dispatch::transport::exercise_result_transport(
            owner, geometry, request, drop_reply, matches!(mode, FifoProbeMode::ColdTransport(_)),
        ));
    }
    let (_, mailbox) = mailbox(session.clone()).unwrap();
    let mut selector = ResidentFifoSelection::new(&mailbox).unwrap();
    let deadline = fixture::until();
    let (lease, reply) = mailbox
        .enqueue_resident_test_request(owner, 901, request.clone(), deadline)
        .unwrap();
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap()
        .clone();
    let mut inspected = None;
    let mut callbacks = 0;
    while inspected.is_none() {
        assert!(Instant::now() < deadline);
        let attempts = selector.serve(
            &mailbox,
            owner,
            8,
            deadline,
            |owner, job, observation| {
                callbacks += 1;
                assert!(job.lease().same_job(&lease));
                assert!(std::ptr::eq(job.request(), lease.request()));
                let observation = observation.unwrap();
                // Mutation happens after the successful worker observation is polled.
                // No historical physics snapshot can authorize the current route.
                let original_index = if matches!(mode, FifoProbeMode::Foreign) {
                    let foreign = foreign_index.take().unwrap();
                    assert!(!installed.same_owner(foreign.source_provenance().unwrap()));
                    let original = geometry
                        .app
                        .world_mut()
                        .remove_resource::<NativeStaticMapIndex>()
                        .unwrap();
                    geometry.app.insert_resource(foreign);
                    Some(original)
                } else {
                    None
                };
                let obstacle = if matches!(mode, FifoProbeMode::Obstacle) {
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
                geometry.advance_physics();
                inspected = Some(owner.certify_observed(
                    observation,
                    job,
                    geometry.app.world_mut(),
                    &installed,
                ));
                if let Some(obstacle) = obstacle {
                    geometry.app.world_mut().despawn(obstacle);
                }
                if let Some(original) = original_index {
                    geometry.app.insert_resource(original);
                }
            },
            |_, _| panic!("resident cannot enter ordinary body/transport"),
        );
        assert_eq!(attempts, 1);
        std::thread::yield_now();
    }
    assert_eq!(callbacks, 1);
    assert!(matches!(
        reply.try_result(),
        Ok(Err(Failure::UnsupportedQuery))
    ));
    assert!(!lease.is_active());
    assert_eq!(
        selector.serve(
            &mailbox,
            owner,
            8,
            deadline,
            |_, _, _| panic!("single use"),
            |_, _| panic!("empty")
        ),
        0
    );
    inspected.unwrap()
}

#[test]
fn fifo_resident_ready_certifies_current_physics_without_transport_success_or_ack() {
    fifo_current_physics_probe(false);
}

#[test]
fn fifo_retained_ready_preserves_committed_cold_without_current_transport_or_ack() {
    fifo_current_physics_probe(true);
    for mutation in 0..22 {
        fifo_current_physics_probe_mode(true, FifoProbeMode::Phased(mutation));
    }
}

fn fifo_current_physics_probe(retained: bool) {
    for mode in [
        FifoProbeMode::Clear,
        FifoProbeMode::Obstacle,
        FifoProbeMode::Foreign,
    ] {
        fifo_current_physics_probe_mode(retained, mode);
    }
}
#[test]
fn resident_result_transport_requeues_exact_work_and_rejects_forged_receipts() {
    fifo_current_physics_probe_mode(false, FifoProbeMode::Transport(false));
    fifo_current_physics_probe_mode(true, FifoProbeMode::Transport(false));
}
#[test]
fn resident_result_transport_dropped_reply_cannot_promote_staging() {
    fifo_current_physics_probe_mode(false, FifoProbeMode::Transport(true));
    fifo_current_physics_probe_mode(true, FifoProbeMode::Transport(true));
}
#[test]
fn resident_result_transport_rechecks_geometry_after_worker_completion() {
    fifo_current_physics_probe_mode(false, FifoProbeMode::TransportRefusal(false));
    fifo_current_physics_probe_mode(false, FifoProbeMode::TransportRefusal(true));
}
#[test]
fn resident_result_transport_tail_full_and_post_mint_expiry_retire_only_unsent_candidate() {
    fifo_current_physics_probe_mode(false, FifoProbeMode::TransportRetire(false));
    fifo_current_physics_probe_mode(false, FifoProbeMode::TransportRetire(true));
    fifo_current_physics_probe_mode(true, FifoProbeMode::TransportRetire(false));
    fifo_current_physics_probe_mode(true, FifoProbeMode::TransportRetire(true));
}
fn fifo_current_physics_probe_mode(retained: bool, mode: FifoProbeMode) {
    {
        with_owned(|install, owner, geometry, session, profile| {
            let mut consumer = Consumer {
                install,
                phased_retained: false,
                owner,
                geometry,
                session,
                profile,
                authored: Default::default(),
                routes: vec![],
                constructor: None,
                foreign_index: Some(
                    NativeStaticMapIndex::from_inventory(input::raw_inventory(install)).unwrap(),
                ),
                fifo_probe: (!retained).then_some(mode),
                fifo_inspected: false,
                foreign_boundary: None,
            };
            let mut world = NativeWorld::synthetic_component_test_with_initialization(
                consumer.profile.clone(),
                consumer.session.world,
                f::player(),
                1234,
                None,
                &mut consumer,
            )
            .unwrap();
            let mut boundary = RouteCommitBoundary::capture_initial(&world).unwrap();
            consumer.owner.initialize(&boundary).unwrap();
            let mut committed = consumer.owner.ledger.diagnostic();
            let mut cold_notice = None;
            for tick in 1..=76 {
                let before = world.checkpoint().unwrap();
                let inputs = if tick == 1 {
                    let position = world.state().position;
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
                } else if retained && !consumer.routes.is_empty() {
                    let position = world.state().position;
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
                if matches!(mode, FifoProbeMode::Phased(23)) {
                    // Same actual state/tick, newly attached controller session.
                    consumer.foreign_boundary =
                        Some(RouteCommitBoundary::capture_initial(&world).unwrap());
                }
                let result = world.step_batch(tick, inputs, &mut consumer);
                if consumer.fifo_inspected {
                    assert!(result.is_err());
                    assert_eq!(world.checkpoint().unwrap(), before);
                    break;
                }
                result.unwrap();
                // Successful ticks, including the prior cold route, advance the genuine
                // component-runtime ACK boundary, not a durable-controller save.
                // The refused FIFO attempt above must never reach this path.
                let notice = RouteCommitNotice::capture(&world, &boundary).unwrap();
                if !notice.routes().is_empty() {
                    assert!(retained);
                    assert!(cold_notice.is_none());
                    assert_eq!(notice.routes().len(), 1);
                    assert_eq!(consumer.routes.len(), 1);
                    assert!(notice.routes()[0]
                        .request
                        .require_pathfinder_dispatch(&consumer.profile)
                        .unwrap()
                        .same_live_dispatch(
                            consumer.routes[0]
                                .require_pathfinder_dispatch(&consumer.profile)
                                .unwrap()
                        ));
                    cold_notice = Some(notice.clone());
                }
                consumer.owner.acknowledge(&notice).unwrap();
                boundary = notice.boundary().clone();
                committed = consumer.owner.ledger.diagnostic();
                if cold_notice.is_some() {
                    assert_eq!(consumer.owner.ledger.counts(), (1, 1));
                    consumer.fifo_probe = Some(mode);
                    consumer.foreign_index = Some(
                        NativeStaticMapIndex::from_inventory(input::raw_inventory(install))
                            .unwrap(),
                    );
                }
            }
            assert!(consumer.fifo_inspected);
            assert_eq!(consumer.routes.len(), usize::from(retained));
            assert_eq!(cold_notice.is_some(), retained);
            let after = consumer.owner.ledger.diagnostic();
            if matches!(
                mode,
                FifoProbeMode::TransportRetire(_) | FifoProbeMode::ColdTransportRetire(_)
            ) {
                assert_eq!(
                    consumer.owner.ledger.counts(),
                    (usize::from(retained), usize::from(retained))
                );
            } else if let FifoProbeMode::Transport(drop_reply)
            | FifoProbeMode::ColdTransport(drop_reply) = mode
            {
                assert_eq!(
                    consumer.owner.ledger.counts(),
                    (
                        usize::from(retained) + usize::from(!drop_reply),
                        usize::from(retained)
                    )
                );
            } else if matches!(mode, FifoProbeMode::Clear | FifoProbeMode::Phased(0)) {
                if retained {
                    assert!(after.only_first_retained_staged_since(&committed));
                    assert_eq!(consumer.owner.ledger.counts(), (2, 1));
                } else {
                    assert!(after.only_initial_cold_staged_since(&committed));
                    assert_eq!(consumer.owner.ledger.counts(), (1, 0));
                }
            } else {
                if matches!(mode, FifoProbeMode::Phased(21)) {
                    if retained {
                        assert!(after.same_committed_root(&committed));
                    } else {
                        assert!(after.same_acknowledged_state(&committed));
                    }
                } else if retained || !matches!(mode, FifoProbeMode::Phased(_)) {
                    assert_eq!(after, committed);
                } else {
                    assert!(after.same_semantic_state(&committed));
                }
                assert_eq!(
                    consumer.owner.ledger.counts(),
                    if retained { (1, 1) } else { (0, 0) }
                );
            }
        });
    }
}

/// The original Work, cancellation and stop tokens survive a completed prefix.
fn certify_phased_probe(
    install: &input::Install,
    owner: &mut OwnedResident,
    geometry: &mut InstalledFixture,
    session: &Session,
    request: &AuthorityRequest,
    mutation: u8,
    mut foreign_index: Option<NativeStaticMapIndex>,
) -> Result<Facts, ResidentFilterError> {
    let installed = geometry
        .app
        .world()
        .resource::<NativeStaticMapIndex>()
        .source_provenance()
        .unwrap()
        .clone();
    let profile = owner.state.profile.clone();
    let ReadySources {
        _worker: worker,
        _mailbox: mailbox,
        work,
        observation,
    } = observe_sources(owner, session, &profile, request);
    let mut worker = Some(worker);
    let before = owner.ledger.diagnostic();
    let first_deadline = if mutation == 5 {
        Instant::now() + Duration::from_secs(5)
    } else {
        fixture::until()
    };
    let job = work.job(&mailbox, owner, first_deadline).unwrap();
    let original_lease = job.lease();
    let result = owner
        .certify_phase_result_retained_only(
            ResidentPhase::Observation(Ok(observation)),
            job,
            geometry.app.world_mut(),
            &installed,
        )
        .unwrap();
    let ResidentPhaseResult::Prepared(prepared) = result else {
        panic!("retained prefix must yield");
    };
    assert_eq!(owner.ledger.diagnostic(), before);
    // An intervening forged dispatch cannot select or replace the retained native owner.
    let mut wrong = request.clone();
    wrong.binding.tick += 1;
    assert!(owner.state.query.retain_constructor(&wrong).is_err());
    assert_eq!(owner.ledger.diagnostic(), before);
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
    match mutation {
        3 => worker.as_ref().unwrap().cancel(),
        4 => drop(worker.take()),
        5 => std::thread::sleep(first_deadline.saturating_duration_since(Instant::now())),
        _ => {}
    }
    let restore_ledger = (15..=20)
        .contains(&mutation)
        .then(|| owner.ledger.invalidate_prepared_for_test(mutation));
    let competing = if mutation == 21 {
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
    let before_final = owner.ledger.diagnostic();
    geometry.advance_physics();
    let replacement = if mutation == 6 {
        Some(
            ResidentDispatchWork::capture(
                &mailbox,
                owner,
                &profile,
                f::player(),
                702,
                request.clone(),
                fixture::until(),
            )
            .unwrap(),
        )
    } else {
        None
    };
    let job = replacement
        .as_ref()
        .unwrap_or(&work)
        .job(&mailbox, owner, fixture::until())
        .unwrap();
    let foreign_owner = foreign_index
        .as_ref()
        .and_then(|index| index.source_provenance());
    let resumed_installed = if mutation == 7 {
        foreign_owner.unwrap()
    } else {
        &installed
    };
    let result = owner.certify_phase_result_retained_only(
        ResidentPhase::Prepared(prepared),
        job,
        geometry.app.world_mut(),
        resumed_installed,
    );
    assert!(
        original_lease.is_active(),
        "short BEGIN slice never expires the longer original Work",
    );
    match mutation {
        1 | 7 | 15..=17 | 21 => assert!(matches!(
            &result,
            Err(ResidentFilterError::Context(Failure::Binding))
        )),
        2 => assert!(matches!(
            &result,
            Err(ResidentFilterError::Context(Failure::UnsupportedNavigation))
        )),
        3..=5 => assert!(matches!(
            &result,
            Err(ResidentFilterError::Source(CompositionError::Cancelled))
        )),
        18 | 20 => assert!(matches!(
            &result,
            Err(ResidentFilterError::Context(Failure::QueueCapacity))
        )),
        19 => assert!(matches!(
            &result,
            Err(ResidentFilterError::Context(Failure::UnsupportedQuery))
        )),
        11..=14 => assert!(matches!(&result, Err(ResidentFilterError::Source(_)))),
        6 => assert!(matches!(
            &result,
            Err(ResidentFilterError::Source(CompositionError::Validation(
                super::super::super::super::products::Error::Identity
            )))
        )),
        _ => {}
    }
    if mutation != 0 {
        assert_eq!(owner.ledger.diagnostic(), before_final);
    }
    if let Some(restore) = restore_ledger {
        restore(&mut owner.ledger);
    }
    if let Some((result, first_id)) = competing {
        owner
            .retire_unpublished_probe_result(result, first_id)
            .unwrap();
        assert!(owner.ledger.diagnostic().same_committed_root(&before));
        assert_eq!(owner.ledger.counts(), (1, 1));
    } else if mutation != 0 {
        assert_eq!(owner.ledger.diagnostic(), before);
    }
    if let Some(obstacle) = obstacle {
        geometry.app.world_mut().despawn(obstacle);
    }
    if let Some(original) = original_index {
        geometry.app.insert_resource(original);
    }
    drop(restore);
    match result? {
        ResidentPhaseResult::Complete(result) => Ok(result.facts().clone()),
        ResidentPhaseResult::Prepared(_) => panic!("final cannot recreate native preparation"),
    }
}

#[path = "tests/cold_phased_negatives.rs"]
mod cold_phased_negatives;

#[path = "tests/wrapper_equivalence.rs"]
mod wrapper_equivalence;
