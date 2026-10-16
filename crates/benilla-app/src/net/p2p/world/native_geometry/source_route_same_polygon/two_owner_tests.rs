// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete two-owner scene with actual loaded source products. Headless
//! tests stop at terminal proposals; only app LocalCorridor may certify a Route.
use super::super::{
    core::{self, NonRenderParticipants},
    products::{
        dry_route::tests as fixtures,
        same_polygon_fixture::{self, RawMap, TerrainKind},
        VmapLiquidProducts,
    },
};
pub(crate) use super::tests::source_session;
use super::*;
use benilla_sim::{creature_ai as ai, creature_init};
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_player_model_fixture as player_models, synthetic_two_owner_fixture as scene,
    Authority, CreatureCollisionWidthRows, CreatureCollisionWidthSource, EffectiveDisabledMMapSet,
    NativeError, NativeWorld, SelectedMMapConfiguration, SourceVisitorCatalog,
};
use std::time::Duration;

pub(crate) fn composed_inputs(
    products: Arc<VmapLiquidProducts>,
    raw_geometry: [u8; 32],
    maps: &benilla_formats::MapCatalog,
    hostile: u64,
    neutral: u64,
    order: [u64; 2],
) -> (Profile, Session, SamePolygonProducts) {
    let environment = fixtures::environment(Arc::clone(&products));
    let mut profile = scene::profile_with_spawns(hostile, neutral, order);
    profile.map_id = environment.binding().map;
    profile.content_digest = environment.binding().content;
    profile.player.start = Point::new(5., 9.25, 0.).unwrap();
    for npc in &mut profile.npcs {
        npc.ai.map_id = profile.map_id;
        npc.ai.content_revision = ai::movement::Revision(profile.content_digest);
        npc.ai.spawn = Point::new(
            if npc.ai.creature_id == hostile {
                2.
            } else {
                8.
            },
            2.,
            0.,
        )
        .unwrap();
        npc.display_id = if npc.ai.creature_id == hostile {
            31
        } else {
            34
        };
        npc.combat.as_mut().unwrap().minimum_damage = f::float(0.);
        npc.combat.as_mut().unwrap().maximum_damage = f::float(0.);
    }
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(scene::visitor_input(&profile, order.into()))
            .unwrap(),
    );
    let catalog = models::catalog_with_height_fields(1.375, 1.6, 2.5, 1.75, false);
    let sources = profile
        .npcs
        .iter()
        .map(|npc| {
            let mut input = models::input();
            input.template.entry = npc.entry;
            input.template.scale = f::float(if npc.ai.creature_id == hostile {
                1.25
            } else {
                2.1875
            });
            let creature_init::Rows::Complete(rows) = &mut input.models else {
                unreachable!()
            };
            for row in rows {
                if row.id == 34 {
                    row.bounding_radius = f::float(0.75);
                    row.combat_reach = f::float(2.);
                }
            }
            let (_, selection) = creature_init::prepare(&input)
                .unwrap()
                .resolve_with_model_receipt(creature_init::SelectionRolls {
                    model: Some(0),
                    gender: Some(u32::from(npc.ai.creature_id == hostile)),
                    level: None,
                })
                .unwrap();
            CreatureCollisionWidthSource::from_initial_model(
                npc.ai.creature_id,
                selection,
                &catalog,
            )
            .unwrap()
        })
        .collect();
    profile
        .initialize_creature_collision_widths(CreatureCollisionWidthRows::Complete(sources))
        .unwrap();
    profile.initialize_creature_collision_heights().unwrap();
    profile
        .initialize_player_collision_height(&player_models::catalog(), &catalog)
        .unwrap();
    profile
        .initialize_player_environment(
            environment.environment_binding(),
            environment.environment_catalog().clone(),
        )
        .unwrap();
    profile
        .initialize_pathfinder_configuration(
            SelectedMMapConfiguration {
                enabled: true,
                effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
                source_instance_id: 0,
            },
            maps,
        )
        .unwrap();
    let participants =
        NonRenderParticipants::authored_fixture(profile.content_digest, profile.map_id);
    let height_configuration = same_polygon_fixture::height_configuration(&products);
    let height = Arc::clone(&products)
        .load_dry_map_height_owned(height_configuration, Some(&participants))
        .unwrap();
    let inputs = SamePolygonProducts {
        projection: products.load_source_query_projection(&profile).unwrap(),
        environment,
        height,
        raw_geometry,
        configuration: super::tests::config(),
        near_configuration: SourceNearPointConfiguration {
            height: height_configuration,
            detect_position_collision: true,
            vmap_line_of_sight_enabled: true,
        },
    };
    let binding = inputs.binding().unwrap();
    profile.geometry_revision = binding.geometry_revision().unwrap();
    let mut session = source_session(&profile);
    session.primary_near_point =
        Some(core::PrimaryNearPointPolicy::from_profile(&profile).unwrap());
    session.same_polygon_route =
        Some(core::SamePolygonRoutePolicy::from_profile(&profile, f::player(), binding).unwrap());
    (profile, session, inputs)
}

fn setup(hostile: u64, neutral: u64, order: [u64; 2]) -> (Profile, Session, SamePolygonProviders) {
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, files) = same_polygon_fixture::fixture(&raw);
    let products = fixtures::load(spec, &files);
    let inventory = raw.inventory();
    let tiles: Vec<_> = (30..=32)
        .flat_map(|y| (30..=32).map(move |x| (x, y)))
        .map(|tile| (tile, inventory.tile_digest(tile).unwrap()))
        .collect();
    let revision = core::geometry_revision(
        0,
        inventory.wdt_digest(),
        &tiles.try_into().unwrap(),
        &[],
        inventory.source_digest(),
    );
    let (profile, session, inputs) =
        composed_inputs(products, revision, &raw.maps(), hostile, neutral, order);
    let providers = inputs.bind(&session, &profile, f::player()).unwrap();
    (profile, session, providers)
}

struct Capture<'a> {
    session: &'a Session,
    providers: &'a mut SamePolygonProviders,
    other: scene::CompleteAuthority,
    near: Vec<AuthorityReply>,
    proposals: Vec<SamePolygonProposal>,
    observations: Vec<Observation>,
}
impl Authority for Capture<'_> {
    fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
        self.other.observe_derived(reply)
    }
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let deadline = Instant::now() + Duration::from_secs(10);
        let facts = match &request.query {
            Query::PlayerEnvironment(_) => self
                .providers
                .environment
                .resolve(self.session, request, deadline)
                .unwrap(),
            Query::SourceMotion(MotionQuery::NearPoint(_)) => self
                .providers
                .near_point
                .resolve(self.session, request, deadline)
                .unwrap(),
            Query::SourceMotion(MotionQuery::Route(_)) => {
                self.proposals.push(
                    self.providers
                        .route
                        .calculate_observed(self.session, request, 1, deadline, |stage| {
                            self.observations.push(stage)
                        })
                        .unwrap(),
                );
                return Err(NativeError::AuthorityUnavailable(
                    "concrete LocalCorridor required",
                ));
            }
            _ => return self.other.resolve(request),
        };
        core::validate_facts(self.session, request, &facts).unwrap();
        let reply = AuthorityReply {
            request: request.clone(),
            facts,
        };
        if matches!(
            request.query,
            Query::SourceMotion(MotionQuery::NearPoint(_))
        ) {
            self.near.push(reply.clone());
        }
        Ok(reply)
    }
}

fn capture_owner(
    profile: &Profile,
    session: &Session,
    providers: &mut SamePolygonProviders,
    owner: u64,
) -> (AuthorityReply, SamePolygonProposal, Vec<Observation>) {
    use ai::movement::Check;
    use benilla_world_runtime::native::{Command, VisitorParticipant};
    let mut world =
        NativeWorld::synthetic_component_test(profile.clone(), session.world, f::player(), 1234)
            .unwrap();
    let mut capture = Capture {
        session,
        providers,
        other: Default::default(),
        near: vec![],
        proposals: vec![],
        observations: vec![],
    };
    capture.other.base.base.critical_chance = Some(0);
    capture.other.base.base.resist_chance = Some(0);
    for npc in &profile.npcs {
        if npc.ai.creature_id != owner {
            capture.other.visibility_overrides.push((
                VisitorParticipant::Creature(world.state().npcs[&npc.ai.creature_id].id),
                VisitorParticipant::Player(world.state().player),
                Check::Failed,
            ));
        }
    }
    let mut input = f::cast(&world, 60001);
    let Command::Cast { target, .. } = &mut input.input.command else {
        unreachable!()
    };
    *target = world.state().npcs[&owner].id;
    for _ in 0..50 {
        let before = world.checkpoint().unwrap();
        let tick = world.state().tick + 1;
        let result = world.step(tick, (tick == 1).then(|| input.clone()), &mut capture);
        if let Some(proposal) = capture.proposals.pop() {
            assert_eq!(
                result,
                Err(NativeError::AuthorityUnavailable(
                    "concrete LocalCorridor required"
                ))
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            return (capture.near.pop().unwrap(), proposal, capture.observations);
        }
        result.unwrap();
    }
    panic!("owner {owner} did not request genuine geometry");
}

#[test]
fn exact_two_owner_primary_roster_and_loaded_source_order_are_bound() {
    for (hostile, neutral) in [(2, 3), (3, 2)] {
        for order in [[2, 3], [3, 2]] {
            let (profile, session, mut providers) = setup(hostile, neutral, order);
            assert_ne!(session.bodies[&hostile], session.bodies[&neutral]);
            assert_eq!(
                core::EmptySelectorNearPointPolicy::from_profile(&profile),
                Err(Failure::UnsupportedQuery)
            );
            assert_eq!(
                core::StaticLosPolicy::from_profile(&profile, true),
                Err(Failure::UnsupportedQuery)
            );
            for mutation in 0..3 {
                let mut incomplete = profile.clone();
                match mutation {
                    0 => {
                        incomplete
                            .source_motion
                            .as_mut()
                            .unwrap()
                            .npcs
                            .remove(&neutral);
                    }
                    1 => {
                        incomplete.creature_capabilities = None;
                    }
                    _ => {
                        incomplete.source_visitors = None;
                    }
                }
                assert!(
                    core::PrimaryNearPointPolicy::from_profile(&incomplete).is_err(),
                    "incomplete source roster {mutation}"
                );
            }

            let (near_a, a, oa) = capture_owner(&profile, &session, &mut providers, hostile);
            let (near_b, b, ob) = capture_owner(&profile, &session, &mut providers, neutral);
            assert_ne!(Rc::as_ptr(&a.candidate), Rc::as_ptr(&b.candidate));
            assert_ne!(
                a.after_cache().binding.identity.creature,
                b.after_cache().binding.identity.creature
            );
            assert!(a.dispatch.first_construction() && b.dispatch.first_construction());
            assert_eq!(providers.route.queries.constructor_count(), 2);
            for (near, route, observations, owner) in
                [(&near_a, &a, oa, hostile), (&near_b, &b, ob, neutral)]
            {
                assert_eq!(near.request.binding.npc.unwrap().spawn, owner);
                assert_eq!(route.context.binding.npc.unwrap().spawn, owner);
                assert_eq!(session.body(&near.request), Ok(session.bodies[&owner]));
                assert_eq!(session.body(&route.context), Ok(session.bodies[&owner]));
                assert_eq!(
                    observations,
                    vec![
                        Observation::ConstructorQuery,
                        Observation::ConstructorFilter,
                        Observation::Gate(GateObservation::LineOfSight(
                            route.gate.line_of_sight.unwrap()
                        )),
                        Observation::Gate(GateObservation::OwnerWater(
                            route.gate.owner_water.unwrap()
                        )),
                        Observation::Gate(GateObservation::TargetWater(false)),
                        Observation::SelectedQuery,
                        Observation::OwnerIgnore,
                        Observation::HaveTile,
                        Observation::CalculateFilter,
                        Observation::Terminal,
                    ]
                );
            }
            for mutation in 0..5 {
                let mut changed = session.clone();
                match mutation {
                    0 => {
                        changed.bodies.remove(&neutral);
                    }
                    1 => {
                        changed.bodies.insert(99, session.bodies[&neutral]);
                    }
                    2 => {
                        changed.bodies.insert(neutral, session.bodies[&hostile]);
                    }
                    3 => {
                        changed.bodies.insert(hostile, session.bodies[&neutral]);
                    }
                    _ => {
                        changed.participants = None;
                    }
                }
                assert!(
                    changed.body(&near_a.request).is_err(),
                    "whole-roster mutation {mutation}"
                );
                assert!(core::mailbox(changed).is_err());
            }
            for mutation in 0..5 {
                let mut changed = near_a.request.clone();
                let Query::SourceMotion(MotionQuery::NearPoint(near)) = &mut changed.query else {
                    unreachable!()
                };
                let Query::SourceMotion(MotionQuery::NearPoint(peer)) = &near_b.request.query
                else {
                    unreachable!()
                };
                match mutation {
                    0 => {
                        near.searcher = peer.searcher.clone();
                    }
                    1 => {
                        changed.binding.npc.as_mut().unwrap().generation += 1;
                    }
                    2 => {
                        near.request.owner_bounding_radius = peer.request.owner_bounding_radius;
                    }
                    3 => {
                        changed.binding.npc = near_b.request.binding.npc;
                        near.request.binding.identity.creature = neutral;
                    }
                    _ => {
                        // Equal apparent owner/generation still cannot borrow the peer catalog row.
                        let mut witness = serde_json::to_value(&peer.searcher).unwrap();
                        witness["npc"] = serde_json::to_value(near.searcher.npc()).unwrap();
                        near.searcher = serde_json::from_value(witness).unwrap();
                    }
                }
                assert!(
                    session.body(&changed).is_err(),
                    "crossed owner mutation {mutation}"
                );
            }
            assert!(providers
                .route
                .committed_candidate_for(NpcId {
                    spawn: hostile,
                    generation: 0
                })
                .is_none());
            assert!(providers
                .route
                .committed_candidate_for(NpcId {
                    spawn: neutral,
                    generation: 0
                })
                .is_none());
        }
    }
}

#[test]
fn two_owner_private_prepared_handles_cannot_cross_requests() {
    let (profile, session, mut providers) = setup(2, 3, [2, 3]);
    let (_, a, _) = capture_owner(&profile, &session, &mut providers, 2);
    let (_, b, _) = capture_owner(&profile, &session, &mut providers, 3);
    let until = || Instant::now() + Duration::from_secs(10);
    let mut a_prepared = providers
        .route
        .prepare_calculation(&session, &a.context, 1, until(), |_| {})
        .unwrap();
    let mut b_prepared = providers
        .route
        .prepare_calculation(&session, &b.context, 1, until(), |_| {})
        .unwrap();
    providers.route.check_prepared(&a_prepared).unwrap();
    providers.route.check_prepared(&b_prepared).unwrap();
    // Adversarial private-state invariant tests, not a publicly mutable capability.
    std::mem::swap(&mut a_prepared.constructor, &mut b_prepared.constructor);
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    assert!(providers.route.check_prepared(&b_prepared).is_err());
    std::mem::swap(&mut a_prepared.constructor, &mut b_prepared.constructor);
    std::mem::swap(
        &mut a_prepared.filter_context,
        &mut b_prepared.filter_context,
    );
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    assert!(providers.route.check_prepared(&b_prepared).is_err());
    std::mem::swap(
        &mut a_prepared.filter_context,
        &mut b_prepared.filter_context,
    );
    std::mem::swap(&mut a_prepared.dispatch, &mut b_prepared.dispatch);
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    assert!(providers.route.check_prepared(&b_prepared).is_err());
    std::mem::swap(&mut a_prepared.dispatch, &mut b_prepared.dispatch);
    a_prepared.prior = Some(Rc::clone(&b.candidate));
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    a_prepared.prior = None;
    providers.route.check_prepared(&a_prepared).unwrap();
    providers.route.check_prepared(&b_prepared).unwrap();
    assert_eq!(providers.route.retained_candidate_count(), 2);
    assert_eq!(providers.route.certified_candidate_count(), 0);
}

#[test]
fn pending_two_owner_cancellation_and_replacement_preserve_peer_job_and_expiry() {
    use core::{ServiceQuery, ServiceStep};
    use crossbeam_channel::TryRecvError;
    for cancelled in [2, 3] {
        let (profile, session, mut providers) = setup(2, 3, [2, 3]);
        let (_, a, _) = capture_owner(&profile, &session, &mut providers, 2);
        let (_, b, _) = capture_owner(&profile, &session, &mut providers, 3);
        let (cancel_request, peer_request) = if cancelled == 2 {
            (a.context, b.context)
        } else {
            (b.context, a.context)
        };
        // Independent attachment fences exercise owner-local cleanup in the
        // shared provider without inventing cancellation by a read-only lease.
        let (_cancel_authority, cancel_mailbox) = core::mailbox(session.clone()).unwrap();
        let (_peer_authority, peer_mailbox) = core::mailbox(session.clone()).unwrap();
        let (cancel_lease, cancel_reply) = cancel_mailbox
            .enqueue_service_test_request(1, cancel_request.clone())
            .unwrap();
        let (peer_lease, peer_reply) = peer_mailbox
            .enqueue_service_test_request(2, peer_request)
            .unwrap();
        let peer_deadline = peer_lease.deadline();
        let advance = |mailbox: &core::Mailbox, routes: &mut SourceSamePolygonRouteProvider| {
            mailbox.serve_service_test_slice(
                1,
                true,
                Instant::now() + Duration::from_secs(10),
                |query, deadline| {
                    let ServiceQuery::Route(job) = query else {
                        panic!("Route job")
                    };
                    if routes
                        .advance_job(&job, 1, deadline)
                        .map_err(SamePolygonError::failure)?
                    {
                        Err(Failure::UnsupportedQuery) // Headless cannot certify a Route.
                    } else {
                        Ok(ServiceStep::Pending)
                    }
                },
            )
        };
        for _ in 0..2 {
            assert_eq!(advance(&cancel_mailbox, &mut providers.route), 1);
            assert_eq!(advance(&peer_mailbox, &mut providers.route), 1);
        }
        assert_eq!(providers.route.pending_job_count(), 2);
        assert!(cancel_lease.is_active() && peer_lease.is_active());
        drop(cancel_mailbox); // Sole Work owner is retired by real uninstall.
        assert!(!cancel_lease.is_active());
        assert!(matches!(
            cancel_reply.try_result(),
            Err(TryRecvError::Disconnected)
        ));
        providers.route.reap_jobs();
        assert_eq!(providers.route.pending_job_count(), 1);
        assert!(peer_lease.is_active());
        assert_eq!(peer_lease.deadline(), peer_deadline);
        for _ in 0..3 {
            assert_eq!(advance(&peer_mailbox, &mut providers.route), 1);
        }
        providers.route.reap_jobs();
        assert_eq!(providers.route.pending_job_count(), 0);
        assert!(matches!(
            peer_reply.try_result(),
            Ok(Err(Failure::UnsupportedQuery))
        ));
        assert!(!peer_lease.is_active());
        let (_replacement_authority, replacement_mailbox) = core::mailbox(session).unwrap();
        let (replacement, reply) = replacement_mailbox
            .enqueue_service_test_request(3, cancel_request)
            .unwrap();
        assert!(!replacement.same_job(&cancel_lease));
        for _ in 0..5 {
            assert_eq!(advance(&replacement_mailbox, &mut providers.route), 1);
        }
        providers.route.reap_jobs();
        assert_eq!(providers.route.pending_job_count(), 0);
        assert!(matches!(
            reply.try_result(),
            Ok(Err(Failure::UnsupportedQuery))
        ));
        assert!(matches!(
            reply.try_result(),
            Err(TryRecvError::Disconnected)
        ));
        assert_eq!(providers.route.retained_candidate_count(), 2);
        assert_eq!(providers.route.certified_candidate_count(), 0);
    }
}
