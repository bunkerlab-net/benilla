// SPDX-License-Identifier: GPL-3.0-or-later
//! Same-install source/Generated navigation and genuine runtime Chase inputs.
//! Runtime deliberately refuses Route; no displacement or committed cache is claimed.
use super::super::{
    core::{PlayerEnvironmentPolicy, PrimaryNearPointPolicy, Session},
    products::{
        source_height::{
            inventory::InventoryCompatibility, PreparedSourceHeight,
            ResidentInitializationConfiguration,
        },
        source_query::SourceMapQuerySnapshot,
        vmap::installed_source_fixture as input,
    },
    source_creature_initialization::{
        resident::ResidentCreatureInitializationProvider, tests as initialization,
    },
    source_route_filter::resident::{
        chase_los::water::ResidentChaseWaterDependency, ResidentFilterDependency,
        ResidentFilterProvider, ResidentGeometryPolicy,
    },
};
use super::resident_los_tests as los;
use benilla_formats::NativeM2MapInventory;
use benilla_sim::{creature_ai as ai, creature_motion as motion};
use benilla_world_runtime::native::{
    environment::LiquidCatalog, synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as authored, synthetic_player_model_fixture as player_models,
    Authority, AuthorityReply, AuthorityRequest, Command, CreatureCapabilityCatalog,
    CreatureCapabilityRows, CreatureInitializationClosure, CreatureInitializationSnapshot, Facts,
    Input, MotionFacts, MotionQuery, NativeError, NativeWorld, Orientation, OwnedInput, Profile,
    Query,
};
use std::time::{Duration, Instant};

const INITIALIZATION_CONFIG: Option<ResidentInitializationConfiguration> =
    Some(ResidentInitializationConfiguration::VmapHeightEnabled);

pub(crate) fn until() -> Instant {
    Instant::now() + Duration::from_secs(60)
}
pub(crate) fn bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}
pub(crate) fn session(p: &Profile) -> Session {
    let mut s = los::session(p);
    s.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(p).unwrap());
    s.player_environment = Some(PlayerEnvironmentPolicy::from_profile(p, f::player()).unwrap());
    s
}
pub(crate) fn with_fixture<T>(
    test: impl FnOnce(
        &input::Install,
        &PreparedSourceHeight<'_, '_, '_>,
        &InventoryCompatibility<'_, '_, '_, '_>,
        NativeM2MapInventory,
        ResidentGeometryPolicy,
        Profile,
        SourceMapQuerySnapshot,
        SourceMapQuerySnapshot,
    ) -> T,
) -> T {
    let (start, _) = input::support();
    // Start beyond melee reach so the real runtime must issue Chase/NearPoint.
    with_fixture_at((start, [start[0] - 8., start[1], start[2]]), test)
}
fn with_fixture_at<T>(
    endpoints: ([f32; 3], [f32; 3]),
    test: impl FnOnce(
        &input::Install,
        &PreparedSourceHeight<'_, '_, '_>,
        &InventoryCompatibility<'_, '_, '_, '_>,
        NativeM2MapInventory,
        ResidentGeometryPolicy,
        Profile,
        SourceMapQuerySnapshot,
        SourceMapQuerySnapshot,
    ) -> T,
) -> T {
    if !benilla_native_nav::SUPPORTED_ABI {
        panic!("joined evidence needs native source query ABI");
    }
    let install = input::navigation_install();
    let before = install.bytes();
    let inventory = input::raw_inventory(&install);
    let terrain =
        benilla_native_terrain::prepare(install.path(), 0, &input::guard_source_tiles()).unwrap();
    let capture = benilla_native_vmap_source::prepare(install.path(), 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(input::profile(), || false)
        .unwrap()
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let compatibility = height
        .match_raw_inventory(&inventory, until(), || false)
        .unwrap();
    // Policy precedes the Profile, every runtime session and all owned receipts.
    let policy = ResidentGeometryPolicy::installed_corridor(&compatibility);
    let (start, end) = endpoints;
    let binding = height
        .initialization_binding(
            f::initialized_profile().content_digest,
            INITIALIZATION_CONFIG,
            until(),
            || false,
        )
        .unwrap();
    let mut p = initialized_profile(binding, start, end);
    p.tick_ms = 25;
    p.source_motion.as_mut().unwrap().relocation_aggro_delay_ms = 125;
    let catalog = LiquidCatalog::from_wdbc(
        &benilla_formats::Chain::open(install.path())
            .unwrap()
            .read_file("DBFilesClient\\LiquidType.dbc")
            .unwrap(),
    )
    .unwrap();
    let result = input::with_navigation(&terrain, &bundle, |prepared, generated| {
        let query =
            SourceMapQuerySnapshot::from_generated(&[(prepared, generated)], &p, until(), || false)
                .unwrap();
        let other =
            SourceMapQuerySnapshot::from_generated(&[(prepared, generated)], &p, until(), || false)
                .unwrap();
        p.geometry_revision = policy.revision(height.binding(), query.binding()).unwrap();
        p.initialize_player_environment(
            height
                .environment_binding(p.content_digest, &catalog, || false)
                .unwrap(),
            catalog,
        )
        .unwrap();
        // Policy cross-binding is rejected before any session emits a receipt.
        let wrong_policy_query =
            SourceMapQuerySnapshot::from_generated(&[(prepared, generated)], &p, until(), || false)
                .unwrap();
        assert!(
            ResidentFilterProvider::bind(&session(&p), &p, &height, wrong_policy_query).is_err()
        );
        let mut old = p.clone();
        old.geometry_revision = super::super::source_route_filter::resident::geometry_revision(
            height.binding(),
            query.binding(),
        );
        let old_query =
            SourceMapQuerySnapshot::from_generated(&[(prepared, generated)], &old, until(), || {
                false
            })
            .unwrap();
        assert!(ResidentFilterProvider::bind_with_policy(
            &session(&old),
            &old,
            &height,
            old_query,
            policy.clone()
        )
        .is_err());
        let mut wrong_height = height.binding();
        wrong_height.revision[0] ^= 1;
        assert!(policy.revision(wrong_height, query.binding()).is_err());
        let mut wrong_navigation = query.binding();
        wrong_navigation.source_epoch[0] ^= 1;
        assert!(policy.revision(height.binding(), wrong_navigation).is_err());

        test(
            &install,
            &height,
            &compatibility,
            inventory,
            policy,
            p,
            query,
            other,
        )
    });
    assert_eq!(
        install.bytes(),
        before,
        "authored install must remain unchanged"
    );
    result
}

/// Reuse the authored generation/model catalog fixture, but bind its habitat-3
/// initialization to this actual source owner before creating any session.
fn initialized_profile(
    binding: CreatureInitializationSnapshot,
    start: [f32; 3],
    end: [f32; 3],
) -> Profile {
    let configured = super::tests::generated_profile(start, end);
    let mut p = initialization::profile_with_faction(binding, [77; 32], 2);
    p.npcs[0].ai.spawn = ai::Point::new(start[0], start[1], start[2]).unwrap();
    p.player.start = ai::Point::new(end[0], end[1], end[2]).unwrap();
    p.pathfinder_configuration = configured.pathfinder_configuration;
    let mut source = p
        .creature_capabilities
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap()
        .clone();
    source.initialization = CreatureInitializationClosure::DryWater {
        map: binding.map,
        respawn_position: p.npcs[0].ai.spawn,
        snapshot: binding,
    };
    p.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(vec![source]))
            .unwrap(),
    );
    p.initialize_player_collision_height(
        &player_models::catalog(),
        &models::catalog_with_model_heights(2.875, 4.125),
    )
    .unwrap();
    benilla_world_runtime::native::synthetic_combat_fixture::install(&mut p);
    p.validate().unwrap();
    p
}

fn assert_initialized(world: &NativeWorld) {
    let npc = &world.state().npcs[&2];
    let capabilities = npc.creature_capabilities().unwrap().witness();
    assert_eq!(capabilities.inhabit_type(), 3);
    let initialized = capabilities.initialization().unwrap();
    initialized.request.validate_for(world.profile()).unwrap();
    assert_eq!(
        initialized.status,
        benilla_world_runtime::native::environment::LiquidStatus::NoWater
    );
    assert!(initialized.data.is_none());
    assert_eq!(initialized.request.required_type, 15);
    assert_eq!(
        initialized.request.classification_height.bits(),
        0x4002_007e
    );
    assert_ne!(
        initialized.request.classification_height,
        initialized.request.collision_height
    );
    assert_eq!(
        npc.creature_collision_height()
            .unwrap()
            .witness()
            .height()
            .bits(),
        initialized.request.collision_height.bits()
    );
    assert_eq!(initialized.request.model_gender_draws.len(), 2);
    let checkpoint = serde_json::to_value(world.checkpoint().unwrap()).unwrap();
    let draws: Vec<benilla_world_runtime::native::Draw> = serde_json::from_value(
        checkpoint["state"]["npcs"]["2"]["source_generation"]["generations"][0]["draws"].clone(),
    )
    .unwrap();
    let level = draws
        .iter()
        .find(|draw| {
            matches!(
                draw.purpose,
                benilla_world_runtime::native::DrawPurpose::CreatureLevel { npc }
                    if Some(npc) == initialized.request.binding.npc
            )
        })
        .unwrap();
    assert_eq!(
        initialized.request.before_level_counter,
        level.first_counter
    );
}

struct RuntimeAuthority<F> {
    authored: authored::SyntheticMotionAuthority,
    source: F,
}
impl<F> Authority for RuntimeAuthority<F>
where
    F: FnMut(&AuthorityRequest) -> Result<Option<Facts>, NativeError>,
{
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let Some(facts) = (self.source)(request)? else {
            return self.authored.resolve(request);
        };
        Ok(AuthorityReply {
            request: request.clone(),
            facts,
        })
    }
}
/// Perform evidence work inside the genuine live authority callback, then refuse
/// Route and verify the entire outer batch rolls back. Source owners stay borrowed.
pub(crate) fn runtime_with_route<T>(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    on_route: impl FnOnce(&mut ResidentFilterProvider<'_, '_, '_, '_>, &AuthorityRequest) -> T,
) -> (AuthorityRequest, T) {
    let initializer = ResidentCreatureInitializationProvider::bind(
        s.world,
        p,
        height,
        INITIALIZATION_CONFIG,
        until(),
        || false,
    )
    .unwrap();
    let mut initializations = 0;
    let environment = provider
        .player_environment(f::player(), until(), || false)
        .unwrap();
    let mut near = None;
    let mut captured = None;
    let mut environments = 0;
    let mut on_route = Some(on_route);
    let mut evidence = None;
    let mut authority = RuntimeAuthority {
        authored: Default::default(),
        source: |request: &AuthorityRequest| match &request.query {
            Query::CreatureInitialization(_) => {
                initializations += 1;
                Ok(Some(
                    initializer.resolve(request, until(), || false).unwrap(),
                ))
            }
            Query::SourceMotion(MotionQuery::NearPoint(_)) => {
                let facts = provider
                    .resolve_primary_near_point(s, request, until(), || false)
                    .unwrap();
                let Facts::SourceMotion(MotionFacts::Spatial(reply)) = &facts else {
                    panic!("spatial")
                };
                let motion::Fact::NearPoint(point) = reply.fact else {
                    panic!("NearPoint")
                };
                near = Some(point);
                Ok(Some(facts))
            }
            Query::PlayerEnvironment(_) => {
                environments += 1;
                Ok(Some(
                    environment.resolve(s, request, until(), || false).unwrap(),
                ))
            }
            Query::SourceMotion(MotionQuery::Route(route)) => {
                assert_eq!(Some(route.request.goal), near);
                assert!(request
                    .require_pathfinder_dispatch(p)
                    .unwrap()
                    .first_construction());
                assert!(route.request.prior_cache.is_none());
                evidence = Some(on_route.take().expect("one first dispatch")(
                    provider, request,
                ));
                captured = Some(request.clone());
                Err(NativeError::AuthorityUnavailable(
                    "joined evidence has no Route conversion",
                ))
            }
            _ => Ok(None),
        },
    };
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p.clone(),
        s.world,
        f::player(),
        1234,
        None,
        &mut authority,
    )
    .unwrap();
    assert_initialized(&world);
    for tick in 1..=12 {
        let before = world.checkpoint().unwrap();
        let inputs = if tick == 1 {
            (1..=2)
                .map(|offset| OwnedInput {
                    authenticated_owner: world.state().player.owner,
                    input: Input {
                        world: world.state().world,
                        player: world.state().player,
                        tick,
                        sequence: world.state().next_sequence + offset - 1,
                        command: Command::MovementPose {
                            destination: ai::Point::new(
                                p.player.start.x.as_f32(),
                                p.player.start.y.as_f32() - offset as f32,
                                p.player.start.z.as_f32(),
                            )
                            .unwrap(),
                            orientation: Orientation::new(0.5).unwrap(),
                            movement_flags: motion::GroundMovementFlags::STOPPED,
                        },
                    },
                })
                .collect()
        } else {
            vec![]
        };
        match world.step_batch(tick, inputs, &mut authority) {
            Ok(_) => (),
            Err(error) => {
                assert!(
                    matches!(
                        error,
                        NativeError::AuthorityUnavailable(
                            "joined evidence has no Route conversion"
                        )
                    ),
                    "{error:?}"
                );
                assert_eq!(
                    world.checkpoint().unwrap(),
                    before,
                    "unaccepted Route rolls back entire tick"
                );
                drop(authority);
                assert_eq!(initializations, 1, "real initialization precedes Chase");
                assert_eq!(
                    environments, 1,
                    "source cache remains the original observation"
                );
                let context = captured.unwrap();
                let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                    unreachable!()
                };
                assert!(route.owner.initialization().is_some());
                let target = route.require_target_water(p, s.world, f::player()).unwrap();
                assert!(!target.in_water());
                let value = serde_json::to_value(target).unwrap();
                let cached = ai::Point::new(
                    p.player.start.x.as_f32(),
                    p.player.start.y.as_f32() - 1.,
                    p.player.start.z.as_f32(),
                )
                .unwrap();
                assert_eq!(
                    value["snapshot"]["state"]["cached_query"]["position"],
                    serde_json::json!(bits(cached))
                );
                return (
                    context,
                    evidence.expect("join ran inside live Route callback"),
                );
            }
        }
    }
    panic!("genuine source NearPoint never produced first Chase")
}
/// Historical dispatch is used only by negative cases; positive joins use the
/// live callback above and never infer synchronous evidence from this capture.
fn runtime_context(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
) -> AuthorityRequest {
    runtime_with_route(p, s, height, provider, |_, _| ()).0
}
pub(crate) fn prerequisites(
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    context: &AuthorityRequest,
    ticket: u64,
    deadline: Instant,
) -> (ResidentFilterDependency, ResidentChaseWaterDependency) {
    let constructor = provider
        .observe(context, ticket, deadline, || false)
        .unwrap();
    let los = provider
        .observe_chase_los(
            &constructor,
            context,
            f::player(),
            ticket + 2,
            deadline,
            || false,
        )
        .unwrap();
    let water = provider
        .observe_chase_water(&los, context, ticket + 3, deadline, || false)
        .unwrap();
    (constructor, water)
}

#[test]
fn resident_joined_cold_preserves_runtime_source_ancestry_and_expiry() {
    use benilla_native_nav::SourceSamePolygonBudget;
    with_fixture(
        |install, height, compatibility, _inventory, policy, p, query, other| {
            let s = session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy.clone())
                    .unwrap();
            let other =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, other, policy).unwrap();
            let original_deadline = until();
            let (context, (joined, observed)) = runtime_with_route(
                &p,
                &s,
                height,
                &mut provider,
                |provider, context| {
                    let (constructor, water) =
                        prerequisites(provider, context, 101, original_deadline);
                    let joined = provider
                        .observe_joined_cold(
                            constructor,
                            water,
                            context,
                            105,
                            SourceSamePolygonBudget::default(),
                            until(),
                            || false,
                        )
                        .unwrap();
                    let observed = provider
                        .joined_for(&joined, context, until(), || false)
                        .unwrap();
                    let segment = provider
                        .joined_segment_los(&joined, context, 107, until(), || false)
                        .unwrap();
                    eprintln!("native114 live joined: points={:?} body={:?} liquid_tickets={:?} segment={:?} capacity={} active={}",
                        observed.terminal.points, observed.body, observed.terminal.liquid_tickets,
                        segment.request, observed.terminal.query_capacity, observed.terminal.active_len);
                    (joined, observed)
                },
            );
            provider
                .verify_installed_owner(compatibility, compatibility.owner(), until(), || false)
                .unwrap();
            let replacement = input::raw_inventory(install);
            let replacement_compatibility = height
                .match_raw_inventory(&replacement, until(), || false)
                .unwrap();
            assert_eq!(
                compatibility.revision(),
                replacement_compatibility.revision()
            );
            assert!(provider
                .verify_installed_owner(
                    &replacement_compatibility,
                    replacement_compatibility.owner(),
                    until(),
                    || false
                )
                .is_err());

            assert_eq!(joined.deadline(until()), original_deadline);
            let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                unreachable!()
            };
            assert_eq!(
                observed.terminal.points,
                [bits(route.request.origin), bits(route.request.goal)]
            );
            assert_ne!(observed.terminal.points[0], observed.terminal.points[1]);
            assert_eq!(observed.terminal.liquid_tickets, [105, 106]);
            assert_eq!(
                observed.body.height.bits(),
                route.require_collision_height(&p).unwrap().height().bits()
            );
            assert!(observed.terminal.before_storage_zero);
            let ray = provider
                .joined_segment_los(&joined, &context, 107, until(), || false)
                .unwrap();
            assert!(!ray.request.ignore_m2);
            for (point, eye) in observed
                .terminal
                .points
                .into_iter()
                .zip([ray.request.from, ray.request.to])
            {
                assert_eq!(
                    eye,
                    [
                        point[0],
                        point[1],
                        (f32::from_bits(point[2]) + observed.body.height.as_f32()).to_bits()
                    ]
                );
            }
            assert!(other
                .joined_for(&joined, &context, until(), || false)
                .is_err());
            assert!(provider
                .joined_for(&joined, &context, Instant::now(), || false)
                .is_err());
            assert!(provider
                .joined_for(&joined, &context, until(), || true)
                .is_err());
            for ticket in [0, 105, 106] {
                assert!(provider
                    .joined_segment_los(&joined, &context, ticket, until(), || false)
                    .is_err());
            }
            let decoded: AuthorityRequest =
                serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
            assert_eq!(decoded, context);
            assert!(provider
                .joined_for(&joined, &decoded, until(), || false)
                .is_err());
            for ticket in [0, 101, 102, 103, 104, u64::MAX] {
                let (constructor, water) = prerequisites(&mut provider, &context, 101, until());
                assert!(provider
                    .observe_joined_cold(
                        constructor,
                        water,
                        &context,
                        ticket,
                        SourceSamePolygonBudget::default(),
                        until(),
                        || false
                    )
                    .is_err());
            }
            let (old, water) = prerequisites(&mut provider, &context, 201, until());
            let different = provider.observe(&context, 211, until(), || false).unwrap();
            assert!(provider
                .observe_joined_cold(
                    different,
                    water,
                    &context,
                    215,
                    SourceSamePolygonBudget::default(),
                    until(),
                    || false
                )
                .is_err());
            drop(old);
            // Every prerequisite may independently be the shortest-lived one.
            for shortest in 0..3 {
                let short = Instant::now() + Duration::from_secs(30);
                let long = until();
                let cap = |stage| if stage == shortest { short } else { long };
                let constructor = provider.observe(&context, 401, cap(0), || false).unwrap();
                let los = provider
                    .observe_chase_los(&constructor, &context, f::player(), 403, cap(1), || false)
                    .unwrap();
                let water = provider
                    .observe_chase_water(&los, &context, 404, cap(2), || false)
                    .unwrap();
                let joined = provider
                    .observe_joined_cold(
                        constructor,
                        water,
                        &context,
                        405,
                        SourceSamePolygonBudget::default(),
                        long,
                        || false,
                    )
                    .unwrap();
                assert_eq!(joined.deadline(long), short);
            }
            let later = continuation_context(&p, &s, height);
            let (constructor, water) = prerequisites(&mut provider, &context, 501, until());
            assert!(matches!(
                provider.observe_joined_cold(
                    constructor,
                    water,
                    &later,
                    505,
                    SourceSamePolygonBudget::default(),
                    until(),
                    || false
                ),
                Err(
                    super::super::source_route_filter::resident::ResidentFilterError::Context(
                        super::super::core::Failure::UnsupportedQuery
                    )
                )
            ));
            let (mut expired, water) = prerequisites(&mut provider, &context, 301, until());
            expired.expire_for_test();
            assert!(provider
                .observe_joined_cold(
                    expired,
                    water,
                    &context,
                    305,
                    SourceSamePolygonBudget::default(),
                    until(),
                    || false
                )
                .is_err());
        },
    );
}

/// Negative-only authored motion, with the same genuine source initialization.
/// This does not provide joined evidence or admit a source Route.
fn first_dispatch(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
    offset: Option<[f32; 3]>,
) -> (NativeWorld, AuthorityRequest) {
    let initializer = ResidentCreatureInitializationProvider::bind(
        s.world,
        p,
        height,
        INITIALIZATION_CONFIG,
        until(),
        || false,
    )
    .unwrap();
    let mut initializations = 0;
    let mut authority = RuntimeAuthority {
        authored: Default::default(),
        source: |request: &AuthorityRequest| {
            if matches!(request.query, Query::CreatureInitialization(_)) {
                initializations += 1;
                return Ok(Some(
                    initializer.resolve(request, until(), || false).unwrap(),
                ));
            }
            Ok(None)
        },
    };
    let mut world = NativeWorld::synthetic_component_test_with_initialization(
        p.clone(),
        s.world,
        f::player(),
        1234,
        None,
        &mut authority,
    )
    .unwrap();
    drop(authority);
    assert_eq!(initializations, 1);
    assert_initialized(&world);
    let mut captured = None;
    let mut authority = RuntimeAuthority {
        authored: Default::default(),
        source: |request: &AuthorityRequest| {
            if let (Some(offset), Query::SourceMotion(MotionQuery::NearPoint(near))) =
                (offset, &request.query)
            {
                let owner = near.request.owner.position;
                let goal = ai::Point::new(
                    owner.x.as_f32() + offset[0],
                    owner.y.as_f32() + offset[1],
                    owner.z.as_f32() + offset[2],
                )?;
                let mut base = authored::SyntheticMotionAuthority::default();
                let mut reply = base.resolve(request)?;
                let Facts::SourceMotion(MotionFacts::Spatial(spatial)) = &mut reply.facts else {
                    unreachable!()
                };
                spatial.fact = motion::Fact::NearPoint(goal);
                return Ok(Some(reply.facts));
            }
            if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_)))
                && captured.is_none()
            {
                captured = Some(request.clone());
            }
            Ok(None)
        },
    };
    for tick in 1..=12 {
        world.step(tick, None, &mut authority).unwrap();
        if !authority
            .authored
            .queries
            .iter()
            .any(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        {
            continue;
        }
        drop(authority);
        let context = captured.unwrap();
        assert!(context
            .require_pathfinder_dispatch(p)
            .unwrap()
            .first_construction());
        return (world, context);
    }
    panic!("no initialized first Chase dispatch")
}

/// Synthetic Route replies only establish a genuine later runtime dispatch for
/// the refusal case. They are never joined evidence or an accepted source Route.
fn continuation_context(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
) -> AuthorityRequest {
    let (mut world, _) = first_dispatch(p, s, height, None);
    let first = world.state().tick;
    let mut authority = authored::SyntheticMotionAuthority::default();
    for tick in first + 1..=first + 10 {
        let input = OwnedInput {
            authenticated_owner: world.state().player.owner,
            input: Input {
                world: world.state().world,
                tick,
                player: world.state().player,
                sequence: world.state().next_sequence,
                command: Command::MovementPose {
                    destination: ai::Point::new(
                        p.player.start.x.as_f32(),
                        p.player.start.y.as_f32() - (tick - first) as f32 * 0.6,
                        p.player.start.z.as_f32(),
                    )
                    .unwrap(),
                    orientation: world.state().orientation,
                    movement_flags: motion::GroundMovementFlags::from_bits(1).unwrap(),
                },
            },
        };
        world.step(tick, Some(input), &mut authority).unwrap();
        for next in std::mem::take(&mut authority.queries) {
            if matches!(next.query, Query::SourceMotion(MotionQuery::Route(_)))
                && !next
                    .require_pathfinder_dispatch(p)
                    .unwrap()
                    .first_construction()
            {
                return next;
            }
        }
    }
    panic!("no genuine same-constructor continuation")
}

#[test]
fn resident_joined_missing_generated_endpoint_refuses_calculation() {
    with_fixture_at(
        (
            [-4265., -4276., input::HEIGHT],
            [-4272., -4284., input::HEIGHT],
        ),
        |_install, height, _compatibility, _inventory, policy, p, query, _| {
            let s = session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let context = runtime_context(&p, &s, height, &mut provider);
            let (constructor, water) = prerequisites(&mut provider, &context, 101, until());
            assert!(matches!(
                provider.observe_joined_cold(
                    constructor,
                    water,
                    &context,
                    105,
                    benilla_native_nav::SourceSamePolygonBudget::default(),
                    until(),
                    || false
                ),
                Err(
                    super::super::source_route_filter::resident::ResidentFilterError::Context(
                        super::super::core::Failure::MissingGeometry
                    )
                )
            ));
        },
    );
}

#[test]
fn resident_joined_rejected_chase_prefix_never_calculates() {
    with_fixture(
        |_install, height, _compatibility, _inventory, policy, p, query, _| {
            let s = session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            let (_, context) = first_dispatch(&p, &s, height, Some([256., 0., 0.]));
            let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                unreachable!()
            };
            let selection = route
                .require_collision_width(&p)
                .unwrap()
                .validated_model_selection(&p, route.owner.npc(), route.owner.initialization())
                .unwrap();
            let reach = selection.combat_reach().bits();
            assert!(
                route.request.goal.x.as_f32() - route.owner_position.x.as_f32()
                    > 200.0 + f32::from_bits(reach)
            );
            assert_eq!(
                benilla_sim::chase_gate::numeric_gate_bits(
                    bits(route.owner_position),
                    bits(route.request.goal),
                    reach,
                )
                .unwrap(),
                benilla_sim::chase_gate::NumericGate::Distance
            );
            let constructor = provider.observe(&context, 101, until(), || false).unwrap();
            let prefix = provider
                .observe_chase_los(&constructor, &context, f::player(), 103, until(), || false)
                .unwrap();
            assert_eq!(provider.los_for(&prefix,&context,until(),||false).unwrap().decision,
            super::super::source_route_filter::resident::chase_los::ResidentChaseLosDecision::Distance);
            let water = provider
                .observe_chase_water(&prefix, &context, 104, until(), || false)
                .unwrap();
            assert!(provider
                .water_for(&water, &context, until(), || false)
                .unwrap()
                .owner
                .is_none());
            assert!(matches!(
                provider.observe_joined_cold(
                    constructor,
                    water,
                    &context,
                    105,
                    benilla_native_nav::SourceSamePolygonBudget::default(),
                    until(),
                    || false
                ),
                Err(
                    super::super::source_route_filter::resident::ResidentFilterError::Context(
                        super::super::core::Failure::UnsupportedQuery
                    )
                )
            ));
        },
    );
}
