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
    Obstacle,
    Foreign,
    Transport(bool),
    TransportRefusal(bool),
    TransportRetire(bool),
}
struct Consumer {
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
}
impl Consumer {
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
impl Authority for Consumer {
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
                        &mut self.owner,
                        &mut self.geometry,
                        &self.session,
                        request,
                        mode,
                        self.foreign_index.take(),
                    );
                    if matches!(
                        mode,
                        FifoProbeMode::Foreign | FifoProbeMode::TransportRefusal(true)
                    ) {
                        assert!(matches!(
                            facts,
                            Err(ResidentFilterError::Context(Failure::Binding))
                        ));
                    } else if matches!(
                        mode,
                        FifoProbeMode::Obstacle | FifoProbeMode::TransportRefusal(false)
                    ) {
                        assert!(matches!(
                            facts,
                            Err(ResidentFilterError::Context(Failure::UnsupportedNavigation))
                        ));
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
                    if matches!(mode, FifoProbeMode::TransportRetire(_)) {
                        assert_eq!(self.owner.ledger.counts(), (0, 0));
                    } else if let FifoProbeMode::Transport(drop_reply) = mode {
                        assert_eq!(self.owner.ledger.counts(), (usize::from(!drop_reply), 0));
                    } else if matches!(mode, FifoProbeMode::Clear) {
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
                        assert_eq!(after, before);
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
                    ready.certify(&mut self.owner, &mut self.geometry, &installed, deadline)
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

// Holds the original Work and worker through one caller-thread attempt. No query
// or ECS view survives the polling boundary.
struct ReadySources {
    _worker: CaptureFreshnessWorker,
    _mailbox: super::super::super::super::core::Mailbox,
    work: ResidentDispatchWork,
    observation: FreshnessObservation,
}
impl ReadySources {
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
                let restore = if !notice.routes().is_empty() && consumer.routes.len() == 1 {
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
fn certify_fifo_probe(
    owner: &mut OwnedResident,
    geometry: &mut InstalledFixture,
    session: &Session,
    request: &AuthorityRequest,
    mode: FifoProbeMode,
    mut foreign_index: Option<NativeStaticMapIndex>,
) -> Result<Facts, ResidentFilterError> {
    if let FifoProbeMode::TransportRetire(expire) = mode {
        return Ok(super::super::super::super::core::resident_dispatch::transport::exercise_result_terminal_retirement(
            owner, geometry, request, expire,
        ));
    }
    if let FifoProbeMode::TransportRefusal(foreign) = mode {
        return super::super::super::super::core::resident_dispatch::transport::exercise_result_refusal(
            owner, geometry, request, foreign.then(|| foreign_index.take().unwrap()),
        );
    }
    if let FifoProbeMode::Transport(drop_reply) = mode {
        return Ok(super::super::super::super::core::resident_dispatch::transport::exercise_result_transport(
            owner, geometry, request, drop_reply,
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
}
#[test]
fn resident_result_transport_dropped_reply_cannot_promote_staging() {
    fifo_current_physics_probe_mode(false, FifoProbeMode::Transport(true));
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
}
fn fifo_current_physics_probe_mode(retained: bool, mode: FifoProbeMode) {
    {
        with_owned(|install, owner, geometry, session, profile| {
            let mut consumer = Consumer {
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
            if matches!(mode, FifoProbeMode::TransportRetire(_)) {
                assert_eq!(consumer.owner.ledger.counts(), (0, 0));
            } else if let FifoProbeMode::Transport(drop_reply) = mode {
                assert_eq!(
                    consumer.owner.ledger.counts(),
                    (usize::from(!drop_reply), 0)
                );
            } else if matches!(mode, FifoProbeMode::Clear) {
                if retained {
                    assert!(after.only_first_retained_staged_since(&committed));
                    assert_eq!(consumer.owner.ledger.counts(), (2, 1));
                } else {
                    assert!(after.only_initial_cold_staged_since(&committed));
                    assert_eq!(consumer.owner.ledger.counts(), (1, 0));
                }
            } else {
                assert_eq!(after, committed);
                assert_eq!(
                    consumer.owner.ledger.counts(),
                    if retained { (1, 1) } else { (0, 0) }
                );
            }
        });
    }
}
