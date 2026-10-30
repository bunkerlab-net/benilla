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
    synthetic_creature_model_fixture as models, synthetic_distinct_quest_fixture as quest,
    synthetic_fixture as f, synthetic_player_model_fixture as player_models,
    synthetic_two_owner_fixture as scene, Authority, CreatureCollisionWidthRows,
    CreatureCollisionWidthSource, EffectiveDisabledMMapSet, NativeError, NativeWorld,
    SelectedMMapConfiguration, SourceVisitorCatalog,
};
use std::time::Duration;

#[path = "two_owner_lifecycle_tests.rs"]
pub(crate) mod lifecycle_tests;

pub(crate) fn composed_inputs(
    products: Arc<VmapLiquidProducts>,
    raw_geometry: [u8; 32],
    maps: &benilla_formats::MapCatalog,
    hostile: u64,
    neutral: u64,
    order: [u64; 2],
) -> (Profile, Session, SamePolygonProducts) {
    let mut profile = scene::profile_with_spawns(hostile, neutral, order);
    profile.player.start = Point::new(5., 9.25, 0.).unwrap();
    profile_inputs(profile, products, raw_geometry, maps, hostile, order)
}

/// Preserve the selected quest/combat/spell/facing hosts while loading the same
/// geometry and model witnesses used by the complete two-owner fixture.
pub(crate) fn quest_inputs(
    products: Arc<VmapLiquidProducts>,
    raw_geometry: [u8; 32],
    maps: &benilla_formats::MapCatalog,
    hostile: u64,
    giver: u64,
    order: [u64; 2],
) -> (Profile, Session, SamePolygonProducts) {
    let mut profile = quest::profile_with_spawns(hostile, giver, order);
    // A starts outside melee range; B remains within source quest interaction range.
    profile.player.start = Point::new(5., 6.5, 0.).unwrap();
    // The selected melee host does not admit rear hits. Author the initial
    // player looking at A before its chase can enter melee range.
    let spawn = owner_spawn(true);
    profile.player.orientation = benilla_world_runtime::native::Orientation::new(
        (spawn.y.as_f32() - profile.player.start.y.as_f32())
            .atan2(spawn.x.as_f32() - profile.player.start.x.as_f32())
            .rem_euclid(std::f32::consts::TAU),
    )
    .unwrap();
    profile_inputs(profile, products, raw_geometry, maps, hostile, order)
}

fn owner_spawn(hostile: bool) -> Point {
    Point::new(if hostile { 2. } else { 8. }, 2., 0.).unwrap()
}

fn profile_inputs(
    mut profile: Profile,
    products: Arc<VmapLiquidProducts>,
    raw_geometry: [u8; 32],
    maps: &benilla_formats::MapCatalog,
    hostile: u64,
    order: [u64; 2],
) -> (Profile, Session, SamePolygonProducts) {
    let environment = fixtures::environment(Arc::clone(&products));
    profile.map_id = environment.binding().map;
    profile.content_digest = environment.binding().content;
    for npc in &mut profile.npcs {
        npc.ai.map_id = profile.map_id;
        npc.ai.content_revision = ai::movement::Revision(profile.content_digest);
        npc.ai.spawn = owner_spawn(npc.ai.creature_id == hostile);
        npc.display_id = if npc.ai.creature_id == hostile {
            31
        } else {
            34
        };
        npc.combat.as_mut().unwrap().minimum_damage = f::float(0.);
        npc.combat.as_mut().unwrap().maximum_damage = f::float(0.);
    }
    // Remap the complete visitor catalog only after both owners have their
    // loaded map/content and positions. Gameplay catalogs remain selected.
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

fn loaded_fixture() -> (
    Arc<VmapLiquidProducts>,
    [u8; 32],
    benilla_formats::MapCatalog,
) {
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
    (products, revision, raw.maps())
}

fn setup(hostile: u64, neutral: u64, order: [u64; 2]) -> (Profile, Session, SamePolygonProviders) {
    let (products, revision, maps) = loaded_fixture();
    let (profile, session, inputs) =
        composed_inputs(products, revision, &maps, hostile, neutral, order);
    let providers = inputs.bind(&session, &profile, f::player()).unwrap();
    (profile, session, providers)
}

struct Capture<'a, A = scene::CompleteAuthority> {
    session: &'a Session,
    providers: &'a mut SamePolygonProviders,
    other: A,
    near: Vec<AuthorityReply>,
    proposals: Vec<SamePolygonProposal>,
    observations: Vec<Observation>,
}
impl<A: Authority> Authority for Capture<'_, A> {
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
    let mut capture = Capture::<scene::CompleteAuthority> {
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
fn loaded_quest_profile_preserves_selected_semantics_and_exact_body_witnesses() {
    use benilla_world_runtime::native::{AbilityAdmission, Rejection, SourceVisitorRows};
    let (products, revision, maps) = loaded_fixture();
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[2, 3], [3, 2]] {
            let selected = quest::profile_with_spawns(hostile, giver, order);
            let (profile, session, inputs) = quest_inputs(
                Arc::clone(&products),
                revision,
                &maps,
                hostile,
                giver,
                order,
            );
            profile.validate().unwrap();
            assert_eq!(profile.interactions, selected.interactions);
            assert_eq!(profile.combat, selected.combat);
            assert_eq!(profile.quest_progression, selected.quest_progression);
            assert_eq!(profile.abilities, selected.abilities);
            assert_eq!(
                profile.source_motion.as_ref().unwrap().spell_hits,
                selected.source_motion.as_ref().unwrap().spell_hits
            );
            assert!(profile.has_source_spell_authority());
            assert_eq!(
                profile
                    .abilities
                    .iter()
                    .filter(|ability| matches!(ability.admission, AbilityAdmission::Available(_)))
                    .map(|ability| ability.id)
                    .collect::<Vec<_>>(),
                vec![quest::SPELL]
            );
            let progression = profile.quest_progression.as_ref().unwrap();
            assert!(progression.loot.is_some());
            assert_eq!(progression.npc_flags(quest::HOSTILE_ENTRY), Some(0));
            assert_eq!(progression.npc_flags(quest::GIVER_ENTRY), Some(2));
            assert_eq!(profile.npc(hostile).unwrap().max_health, 10);
            assert_eq!(profile.player.start, Point::new(5., 6.5, 0.).unwrap());
            assert_eq!(
                profile.npc(hostile).unwrap().ai.spawn,
                Point::new(2., 2., 0.).unwrap()
            );
            assert_eq!(
                profile.npc(giver).unwrap().ai.spawn,
                Point::new(8., 2., 0.).unwrap()
            );
            let neutral = profile.npc(giver).unwrap();
            assert!(neutral.ai.neutral_to_all);
            assert_eq!(neutral.ai.reaction, ai::Reaction::Aggressive);
            assert!(profile.npc_motion(giver).unwrap().npc_can_enter_combat);
            assert!(!neutral.combat.as_ref().unwrap().creature_is_critter);
            let visitors = profile.source_visitors.as_ref().unwrap();
            assert_eq!(visitors.owner_order(), order);
            let owner = visitors.owner(giver).unwrap();
            assert!(owner.ai_name.is_empty() && owner.script_name.is_empty());
            assert_eq!(owner.script_id, 0);
            assert_eq!(owner.spell_list_id, 0);
            assert_eq!(owner.entry_events, SourceVisitorRows::Complete(vec![]));
            assert_eq!(owner.guid_events, SourceVisitorRows::Complete(vec![]));
            assert_eq!(owner.overrides, SourceVisitorRows::Complete(vec![]));

            assert_eq!(profile.map_id, inputs.environment.binding().map);
            assert_eq!(profile.content_digest, inputs.environment.binding().content);
            assert_eq!(
                profile.geometry_revision,
                inputs.binding().unwrap().geometry_revision().unwrap()
            );
            assert_eq!(session.profile, profile.digest());
            let configuration = profile.pathfinder_configuration().unwrap();
            assert_eq!(configuration.map_id(), profile.map_id);
            assert!(configuration.selected().enabled);
            assert_eq!(configuration.raw_instance_type(), 0);
            assert!(!configuration.source_is_dungeon());
            let providers = inputs.bind(&session, &profile, f::player()).unwrap();
            assert_eq!(providers.route.certified_candidate_count(), 0);
            let world = NativeWorld::synthetic_component_test(
                profile.clone(),
                session.world,
                f::player(),
                1234,
            )
            .unwrap();
            for npc in &profile.npcs {
                let live = &world.state().npcs[&npc.ai.creature_id];
                let width = live.creature_collision_width().unwrap().witness();
                let height = live.creature_collision_height().unwrap().witness();
                width.validate_for(&profile, live.id).unwrap();
                height.validate_for(&profile, live.id).unwrap();
                assert_eq!(width.native_display_id(), npc.display_id);
                assert_eq!(height.native_display_id(), width.native_display_id());
                assert_eq!(height.width_source_binding(), width.source_binding());
                assert_eq!(
                    session.bodies[&live.id.spawn].radius,
                    npc.ai.bounding_radius
                );
                assert_eq!(
                    session.bodies[&live.id.spawn].height.bits(),
                    height.height().bits()
                );
                assert_eq!(npc.combat.as_ref().unwrap().minimum_damage, f::float(0.));
                assert_eq!(npc.combat.as_ref().unwrap().maximum_damage, f::float(0.));
            }
            assert_ne!(session.bodies[&hostile], session.bodies[&giver]);
            let player_height = world.state().player_collision_height().unwrap().witness();
            player_height
                .validate_for(&profile, session.world, f::player())
                .unwrap();
            assert_eq!(
                session.player.unwrap().body.height.bits(),
                player_height.height().bits()
            );
            let mut authority = quest::spatial_only();
            let before = world.checkpoint().unwrap();
            let giver_id = world.state().npcs[&giver].id;
            let hostile_id = world.state().npcs[&hostile].id;
            assert!(world
                .preview_quest_status(giver_id, &mut authority)
                .unwrap());
            assert_eq!(
                world
                    .preview_quest_interaction(giver_id, quest::QUEST, false, &mut authority)
                    .unwrap(),
                None
            );
            assert_eq!(
                world
                    .preview_quest_interaction(hostile_id, quest::QUEST, false, &mut authority)
                    .unwrap(),
                Some(Rejection::QuestInteraction)
            );
            assert_eq!(world.checkpoint().unwrap(), before);
            assert!(
                authority.queries.is_empty(),
                "selected quest semantics must stay runtime-owned"
            );

            let mut missing_hit = profile.clone();
            missing_hit
                .source_motion
                .as_mut()
                .unwrap()
                .spell_hits
                .remove(&quest::SPELL);
            assert!(missing_hit.validate().is_err());
            let mut passive = profile;
            passive
                .npcs
                .iter_mut()
                .find(|npc| npc.ai.creature_id == giver)
                .unwrap()
                .ai
                .reaction = ai::Reaction::Passive;
            assert!(passive.validate().is_err());
        }
    }
}

#[test]
fn loaded_quest_initial_hostile_chase_reaches_real_terminal_without_a_route_certificate() {
    let (products, revision, maps) = loaded_fixture();
    for (hostile, giver) in [(2, 3), (3, 2)] {
        for order in [[2, 3], [3, 2]] {
            let (profile, session, inputs) = quest_inputs(
                Arc::clone(&products),
                revision,
                &maps,
                hostile,
                giver,
                order,
            );
            let npc = profile.npc(hostile).unwrap();
            assert!(!ai::in_melee_range(
                npc.ai.spawn,
                profile.player.start,
                ai::combat_reach(npc.ai.combat_reach, profile.player.combat_reach).unwrap(),
                f::real(0.),
            )
            .unwrap());
            let mut providers = inputs.bind(&session, &profile, f::player()).unwrap();
            let mut world = NativeWorld::synthetic_component_test(
                profile.clone(),
                session.world,
                f::player(),
                1234,
            )
            .unwrap();
            let mut capture = Capture {
                session: &session,
                providers: &mut providers,
                other: quest::spatial_only(),
                near: vec![],
                proposals: vec![],
                observations: vec![],
            };
            let mut reached_terminal = false;
            for _ in 0..50 {
                let before = world.checkpoint().unwrap();
                let result = world.step(world.state().tick + 1, None, &mut capture);
                if let Some(proposal) = capture.proposals.last() {
                    assert_eq!(
                        result,
                        Err(NativeError::AuthorityUnavailable(
                            "concrete LocalCorridor required"
                        ))
                    );
                    assert_eq!(world.checkpoint().unwrap(), before);
                    core::assert_loaded_quest_body_real_slice_progress(&session, &proposal.context);
                    // Exercise the same loaded quest body through the real 2ms service slices.
                    // Only the existing source provider may propose a native route, and only
                    // concrete app physics may certify one; this callback supplies no facts.
                    let (_authority, mailbox) = core::mailbox(session.clone()).unwrap();
                    let (lease, reply) = mailbox
                        .enqueue_service_test_request(17, proposal.context.clone())
                        .unwrap();
                    let original_deadline = lease.deadline();
                    let mut calls = 0;
                    let mut attempts = 0;
                    loop {
                        attempts += 1;
                        assert_eq!(
                            mailbox.serve_service_queries_until(1, true, |query, _| {
                                let core::ServiceQuery::Route(job) = query else {
                                    panic!("unstaged loaded quest Route")
                                };
                                calls += 1;
                                assert!(job.lease().same_job(&lease));
                                assert_eq!(job.lease().deadline(), original_deadline);
                                assert_eq!(job.request(), &proposal.context);
                                assert_eq!(job.body(), session.bodies[&hostile]);
                                Err(Failure::MissingGeometry)
                            }),
                            1
                        );
                        match reply.try_result() {
                            Err(crossbeam_channel::TryRecvError::Empty) => {
                                assert!(lease.is_active())
                            }
                            Ok(result) => {
                                assert_eq!(result, Err(Failure::MissingGeometry));
                                break;
                            }
                            other => panic!(
                                "loaded quest body expired before the provider: {other:?}; attempts={attempts}"
                            ),
                        }
                    }
                    assert_eq!(calls, 1);
                    assert!(!lease.is_active());
                    eprintln!(
                        "loaded quest body completed within original lease: attempts={attempts}"
                    );
                    // Continue the same exact loaded context through every provider stage,
                    // still without inventing a concrete physics certificate.
                    let (_authority, mailbox) = core::mailbox(session.clone()).unwrap();
                    let (lease, reply) = mailbox
                        .enqueue_service_test_request(18, proposal.context.clone())
                        .unwrap();
                    let original_deadline = lease.deadline();
                    let mut provider_attempts = 0;
                    let mut observed = vec![];
                    loop {
                        provider_attempts += 1;
                        assert_eq!(
                            mailbox.serve_service_queries_until(1, true, |query, deadline| {
                                let core::ServiceQuery::Route(job) = query else {
                                    panic!("loaded Route job")
                                };
                                assert!(job.lease().same_job(&lease));
                                assert_eq!(job.lease().deadline(), original_deadline);
                                let mut attempt = vec![];
                                let advanced = capture.providers.route.advance_job_observed(
                                    &job, 1, deadline, |stage| attempt.push(stage));
                                let complete = match advanced {
                                    Ok(complete) => { observed.extend(attempt); complete },
                                    Err(error) => {
                                        eprintln!("loaded quest incomplete provider attempt: error={error:?}, observations={attempt:?}");
                                        return Err(error.failure());
                                    }
                                };
                                if complete {
                                    let staged = capture
                                        .providers
                                        .route
                                        .terminal_for_job(&job)
                                        .map_err(SamePolygonError::failure)?;
                                    assert!(Rc::ptr_eq(&staged.candidate, &proposal.candidate));
                                    assert_eq!(staged.model, proposal.model);
                                    assert_eq!(staged.owner_ignore, proposal.owner_ignore);
                                    Err(Failure::MissingGeometry)
                                } else {
                                    Ok(core::ServiceStep::Pending)
                                }
                            }),
                            1
                        );
                        match reply.try_result() {
                            Err(crossbeam_channel::TryRecvError::Empty) => {
                                assert!(lease.is_active())
                            }
                            Ok(result) => {
                                assert_eq!(result, Err(Failure::MissingGeometry));
                                break;
                            }
                            other => panic!(
                                "loaded quest staged provider expired: {other:?}; attempts={provider_attempts}"
                            ),
                        }
                    }
                    assert_eq!(observed, capture.observations);
                    assert!(!lease.is_active());
                    capture.providers.route.reap_jobs();
                    assert_eq!(capture.providers.route.pending_job_count(), 0);
                    eprintln!(
                        "loaded quest provider completed within original lease: attempts={provider_attempts}"
                    );
                    assert_eq!(proposal.context.binding.npc.unwrap().spawn, hostile);
                    assert!(proposal.dispatch.first_construction());
                    assert_eq!(
                        proposal.after_cache().source,
                        motion::RouteSource::OwnerPathFinder
                    );
                    assert_eq!(proposal.after_cache().revision, 1);
                    let end = proposal.terminal().actual_end().map(f32::from_bits);
                    let endpoint = Point::new(end[0], end[1], end[2]).unwrap();
                    eprintln!(
                        "loaded quest terminal: hostile={hostile}, giver={giver}, order={order:?}, tick={}, endpoint={end:?}, player_distance={}, giver_distance={}",
                        world.state().tick + 1,
                        endpoint
                            .distance_squared(profile.player.start)
                            .unwrap()
                            .as_f32()
                            .sqrt(),
                        endpoint
                            .distance_squared(profile.npc(giver).unwrap().ai.spawn)
                            .unwrap()
                            .as_f32()
                            .sqrt(),
                    );
                    assert_eq!(
                        capture
                            .near
                            .last()
                            .unwrap()
                            .request
                            .binding
                            .npc
                            .unwrap()
                            .spawn,
                        hostile
                    );
                    assert_eq!(capture.observations.last(), Some(&Observation::Terminal));
                    assert_eq!(capture.providers.route.certified_candidate_count(), 0);
                    assert!(capture
                        .providers
                        .route
                        .committed_candidate_for(world.state().npcs[&hostile].id)
                        .is_none());
                    assert!(capture
                        .providers
                        .route
                        .committed_candidate_for(world.state().npcs[&giver].id)
                        .is_none());
                    assert!(world
                        .state()
                        .quest_progression
                        .as_ref()
                        .unwrap()
                        .quests
                        .progress(quest::QUEST)
                        .is_none());
                    reached_terminal = true;
                    break;
                }
                result.unwrap();
            }
            assert!(
                reached_terminal,
                "initial hostile chase never reached the loaded terminal"
            );
        }
    }
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
                            route.gate.as_ref().unwrap().line_of_sight.unwrap()
                        )),
                        Observation::Gate(GateObservation::OwnerWater(
                            route.gate.as_ref().unwrap().owner_water.unwrap()
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
    std::mem::swap(
        &mut a_prepared.dependencies.constructor,
        &mut b_prepared.dependencies.constructor,
    );
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    assert!(providers.route.check_prepared(&b_prepared).is_err());
    std::mem::swap(
        &mut a_prepared.dependencies.constructor,
        &mut b_prepared.dependencies.constructor,
    );
    std::mem::swap(
        &mut a_prepared.validation.filter_context,
        &mut b_prepared.validation.filter_context,
    );
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    assert!(providers.route.check_prepared(&b_prepared).is_err());
    std::mem::swap(
        &mut a_prepared.validation.filter_context,
        &mut b_prepared.validation.filter_context,
    );
    std::mem::swap(
        &mut a_prepared.validation.dispatch,
        &mut b_prepared.validation.dispatch,
    );
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    assert!(providers.route.check_prepared(&b_prepared).is_err());
    std::mem::swap(
        &mut a_prepared.validation.dispatch,
        &mut b_prepared.validation.dispatch,
    );
    a_prepared.dependencies.prior = Some(Rc::clone(&b.candidate));
    assert!(providers.route.check_prepared(&a_prepared).is_err());
    a_prepared.dependencies.prior = None;
    providers.route.check_prepared(&a_prepared).unwrap();
    providers.route.check_prepared(&b_prepared).unwrap();
    assert_eq!(providers.route.retained_candidate_count(), 2);
    assert_eq!(providers.route.certified_candidate_count(), 0);
}

#[test]
fn two_owner_validated_contexts_cannot_cross_requests_before_constructor() {
    let (profile, session, mut providers) = setup(2, 3, [2, 3]);
    let (_, a, _) = capture_owner(&profile, &session, &mut providers, 2);
    let (_, b, _) = capture_owner(&profile, &session, &mut providers, 3);
    let until = || Instant::now() + Duration::from_secs(10);
    let mut a = providers
        .route
        .validate_calculation(&session, &a.context, 1, until())
        .unwrap();
    let mut b = providers
        .route
        .validate_calculation(&session, &b.context, 1, until())
        .unwrap();
    providers.route.check_validated(&a).unwrap();
    providers.route.check_validated(&b).unwrap();
    std::mem::swap(&mut a.filter_context, &mut b.filter_context);
    assert!(providers.route.check_validated(&a).is_err());
    assert!(providers.route.check_validated(&b).is_err());
    std::mem::swap(&mut a.filter_context, &mut b.filter_context);
    std::mem::swap(&mut a.dispatch, &mut b.dispatch);
    assert!(providers.route.check_validated(&a).is_err());
    assert!(providers.route.check_validated(&b).is_err());
    std::mem::swap(&mut a.dispatch, &mut b.dispatch);
    providers.route.check_validated(&a).unwrap();
    providers.route.check_validated(&b).unwrap();
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
        for _ in 0..4 {
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
        for _ in 0..6 {
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
