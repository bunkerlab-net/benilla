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
    let policy = ResidentGeometryPolicy::installed_dry_corridor(&compatibility);
    let (start, end) = endpoints;
    let binding = height
        .initialization_binding(
            f::initialized_profile().content_digest,
            INITIALIZATION_CONFIG,
            until(),
            || false,
        )
        .unwrap();
    let maps = benilla_formats::load_map_catalog(
        &mut benilla_formats::Chain::open(install.path()).unwrap(),
    )
    .unwrap();
    let mut p = initialized_profile(binding, start, end, &maps);
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
        assert_ne!(
            p.geometry_revision,
            super::super::source_route_filter::resident::dry_near_point::geometry_revision(
                height.binding()
            )
        );
        assert_ne!(
            p.geometry_revision,
            ResidentGeometryPolicy::installed_corridor(&compatibility)
                .revision(height.binding(), query.binding())
                .unwrap()
        );
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
    maps: &benilla_formats::MapCatalog,
) -> Profile {
    let mut p = initialization::profile_with_faction(binding, [77; 32], 2);
    p.npcs[0].ai.spawn = ai::Point::new(start[0], start[1], start[2]).unwrap();
    p.player.start = ai::Point::new(end[0], end[1], end[2]).unwrap();
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
    // Player height selection requires the real starter initializer's identity;
    // the creature-generation fixture intentionally has no initialized player.
    let starter = f::initialized_starter_input();
    if let benilla_world_runtime::native::AbilityAdmission::Available(spec) =
        &mut p.abilities[0].admission
    {
        spec.spell_rank = 1;
    }
    // Select the fixture's native melee component using its authored generation
    // exemplar; runtime initialization still projects the actual selected stats.
    let generation = p.creature_generations.as_ref().unwrap().source(2).unwrap();
    let (selected, _) = benilla_sim::creature_init::prepare(&generation.input)
        .unwrap()
        .resolve_with_model_receipt(benilla_sim::creature_init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: Some(1),
        })
        .unwrap();
    p.npcs[0].combat = Some(benilla_world_runtime::native::CreatureCombatProfile {
        minimum_damage: selected.stats.damage.minimum,
        maximum_damage: selected.stats.damage.maximum,
        no_crushing_blows: selected.no_crushing_blows,
        creature_is_critter: selected.creature_is_critter,
    });
    p.initialize_player(&starter, f::float(100.0), 1).unwrap();
    p.initialize_player_collision_height(
        &player_models::catalog(),
        &models::catalog_with_model_heights(2.875, 4.125),
    )
    .unwrap();
    let player_model = p.player_collision_height.as_ref().unwrap();
    assert_eq!(
        (
            player_model.race(),
            player_model.class(),
            player_model.gender()
        ),
        (starter.race, starter.class, starter.gender),
    );
    assert_eq!(
        p.player.start,
        ai::Point::new(end[0], end[1], end[2]).unwrap()
    );
    benilla_world_runtime::native::synthetic_combat_fixture::install(&mut p);
    // Retain configuration only after this profile's complete entry/capability
    // and interaction owners exist; another fixture's retained source is invalid.
    p.initialize_pathfinder_configuration(
        benilla_world_runtime::native::SelectedMMapConfiguration {
            enabled: true,
            effective_disabled_maps:
                benilla_world_runtime::native::EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        maps,
    )
    .unwrap();
    p.pathfinder_configuration
        .as_ref()
        .unwrap()
        .constructor_binding(&p, 2)
        .unwrap();
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
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    on_route: impl FnOnce(&mut ResidentFilterProvider<'_, '_, '_, '_>, &AuthorityRequest) -> T,
) -> (AuthorityRequest, T) {
    runtime_with_route_input(
        p,
        s,
        height,
        compatibility,
        provider,
        None,
        |_, _| {},
        on_route,
    )
}

/// The optional far goal is explicit adversarial test input, not source geometry.
#[allow(clippy::too_many_arguments)]
fn runtime_with_route_input<T>(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
    adversarial_offset: Option<[f32; 3]>,
    mut on_near: impl FnMut(&ResidentFilterProvider<'_, '_, '_, '_>, &AuthorityRequest),
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
            Query::SourceMotion(MotionQuery::NearPoint(near_request)) => {
                on_near(provider, request);
                let facts = if let Some(offset) = adversarial_offset {
                    let owner = near_request.request.owner.position;
                    // Adversarial consumed input: runtime, not this test, seals Route.
                    Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
                        request: motion::Query::NearPoint(near_request.request.clone()),
                        fact: motion::Fact::NearPoint(ai::Point::new(
                            owner.x.as_f32() + offset[0],
                            owner.y.as_f32() + offset[1],
                            owner.z.as_f32() + offset[2],
                        )?),
                    })))
                } else {
                    provider
                        .resolve_dry_primary_near_point(
                            s,
                            request,
                            f::player(),
                            compatibility,
                            INITIALIZATION_CONFIG,
                            until(),
                            || false,
                        )
                        .unwrap()
                };
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
        let before_records = world.records().to_vec();
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
                assert_eq!(world.records(), before_records);
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

#[allow(clippy::too_many_arguments)]
fn assert_composite_refusals(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    foreign: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &ResidentFilterProvider<'_, '_, '_, '_>,
    request: &AuthorityRequest,
) {
    use super::super::source_route_filter::resident::dry_near_point::ResidentDryNearPointProvider;
    assert!(ResidentDryNearPointProvider::bind(
        s,
        p,
        f::player(),
        height,
        INITIALIZATION_CONFIG,
        until(),
        || false
    )
    .is_err());
    assert!(matches!(
        provider.resolve_primary_near_point(s, request, until(), || false),
        Err(
            super::super::source_route_filter::resident::ResidentFilterError::Context(
                super::super::core::Failure::Binding
            )
        )
    ));
    for config in [
        None,
        Some(ResidentInitializationConfiguration::VmapHeightDisabled),
    ] {
        assert!(provider
            .resolve_dry_primary_near_point(
                s,
                request,
                f::player(),
                compatibility,
                config,
                until(),
                || false
            )
            .is_err());
    }
    assert_eq!(compatibility.revision(), foreign.revision());
    assert!(provider
        .resolve_dry_primary_near_point(
            s,
            request,
            f::player(),
            foreign,
            INITIALIZATION_CONFIG,
            until(),
            || false
        )
        .is_err());
    for field in 0..3 {
        let mut wrong = s.clone();
        match field {
            0 => wrong.profile[0] ^= 1,
            1 => wrong.geometry[0] ^= 1,
            _ => wrong.world[0] ^= 1,
        }
        assert!(provider
            .resolve_dry_primary_near_point(
                &wrong,
                request,
                f::player(),
                compatibility,
                INITIALIZATION_CONFIG,
                until(),
                || false
            )
            .is_err());
    }
    for field in 0..3 {
        let mut wrong = request.clone();
        match field {
            0 => wrong.binding.profile[0] ^= 1,
            1 => wrong.binding.world[0] ^= 1,
            _ => wrong.binding.npc.as_mut().unwrap().generation += 1,
        }
        assert!(provider
            .resolve_dry_primary_near_point(
                s,
                &wrong,
                f::player(),
                compatibility,
                INITIALIZATION_CONFIG,
                until(),
                || false
            )
            .is_err());
    }
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
            let replacement = input::raw_inventory(install);
            let replacement_compatibility = height
                .match_raw_inventory(&replacement, until(), || false)
                .unwrap();
            let original_deadline = until();
            let (context, (joined, observed)) = runtime_with_route_input(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                None,
                |provider, near| {
                    assert_composite_refusals(
                        &p,
                        &s,
                        height,
                        compatibility,
                        &replacement_compatibility,
                        provider,
                        near,
                    )
                },
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
    with_fixture(|_, height, compatibility, _, policy, p, query, _| {
        let s = session(&p);
        let mut provider =
            ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
        same_tick_later_use(&p, &s, height, compatibility, &mut provider);
    });
}

/// Exactly one TEST-AUTHORED NoPath permits a second real SAME-TICK dispatch.
/// It proves no source NoPath, accepted movement, cache or cross-tick persistence.
fn same_tick_later_use(
    p: &Profile,
    s: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
    compatibility: &InventoryCompatibility<'_, '_, '_, '_>,
    provider: &mut ResidentFilterProvider<'_, '_, '_, '_>,
) {
    let initializer = ResidentCreatureInitializationProvider::bind(
        s.world,
        p,
        height,
        INITIALIZATION_CONFIG,
        until(),
        || false,
    )
    .unwrap();
    let environment = provider
        .player_environment(f::player(), until(), || false)
        .unwrap();
    let mut routes = 0;
    let route_progress = std::cell::Cell::new(0);
    let mut authored_no_path = 0;
    let mut first_context: Option<AuthorityRequest> = None;
    let mut retained = None;
    let mut original_allocation = None;
    let mut authority = RuntimeAuthority {
        authored: Default::default(),
        source: |request: &AuthorityRequest| match &request.query {
            Query::CreatureInitialization(_) => Ok(Some(
                initializer.resolve(request, until(), || false).unwrap(),
            )),
            Query::PlayerEnvironment(_) => Ok(Some(
                environment.resolve(s, request, until(), || false).unwrap(),
            )),
            Query::SourceMotion(MotionQuery::NearPoint(_)) => Ok(Some(
                provider
                    .resolve_dry_primary_near_point(
                        s,
                        request,
                        f::player(),
                        compatibility,
                        INITIALIZATION_CONFIG,
                        until(),
                        || false,
                    )
                    .unwrap(),
            )),
            Query::SourceMotion(MotionQuery::Route(route)) => {
                routes += 1;
                route_progress.set(routes);
                let event = request.require_pathfinder_dispatch(p).unwrap();
                assert!(route.request.prior_cache.is_none());
                match routes {
                    1 => {
                        assert!(event.first_construction());
                        retained = Some(prerequisites(provider, request, 701, until()));
                        let allocation = provider.retained_constructor_for_test(request).unwrap();
                        assert_eq!(allocation.2, 1);
                        original_allocation = Some(allocation);
                        first_context = Some(request.clone());
                        authored_no_path += 1;
                        // Deliberately authored scaffolding, never joined-result conversion.
                        Ok(Some(Facts::SourceMotion(MotionFacts::Spatial(Box::new(
                            motion::Reply {
                                request: motion::Query::Route(route.request.clone()),
                                fact: motion::Fact::Route(motion::RouteOutcome::NoPath),
                            },
                        )))))
                    }
                    2 => {
                        assert!(!event.first_construction());
                        let first = first_context.as_ref().unwrap();
                        let first_event = first.require_pathfinder_dispatch(p).unwrap();
                        assert_eq!(
                            event.construction_binding(),
                            first_event.construction_binding()
                        );
                        assert_eq!(request.binding, first.binding);

                        let Query::SourceMotion(MotionQuery::Route(first_route)) = &first.query
                        else {
                            unreachable!()
                        };
                        assert!(
                            route.request.binding.query_id > first_route.request.binding.query_id
                        );
                        assert_eq!(
                            route.request.binding.now_ms,
                            first_route.request.binding.now_ms
                        );
                        assert_eq!(route.owner, first_route.owner);
                        assert_eq!(route.owner_position, first_route.owner_position);
                        // Independent allocation check precedes joined's early later-use refusal.
                        assert_eq!(
                            provider.retained_constructor_for_test(request).unwrap(),
                            original_allocation.unwrap()
                        );
                        let (constructor, water) = retained.take().unwrap();
                        assert!(matches!(provider.observe_joined_cold(
                            constructor, water, request, 705,
                            benilla_native_nav::SourceSamePolygonBudget::default(), until(), || false,
                        ), Err(super::super::source_route_filter::resident::ResidentFilterError::Context(
                            super::super::core::Failure::UnsupportedQuery))));
                        Err(NativeError::AuthorityUnavailable(
                            "authored NoPath later-use rollback",
                        ))
                    }
                    _ => panic!("more than two Route callbacks"),
                }
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
        let before_records = world.records().to_vec();
        match world.step(tick, None, &mut authority) {
            Ok(_) => assert_eq!(
                route_progress.get(),
                0,
                "first NoPath must reach the second callback in the same tick"
            ),
            Err(error) => {
                assert!(
                    matches!(
                        error,
                        NativeError::AuthorityUnavailable("authored NoPath later-use rollback")
                    ),
                    "{error:?}"
                );
                assert_eq!(
                    world.checkpoint().unwrap(),
                    before,
                    "whole runtime checkpoint/journal rollback, not provider registry reset"
                );
                assert_eq!(world.records(), before_records);
                drop(authority);
                assert_eq!(routes, 2);
                assert_eq!(authored_no_path, 1);
                return;
            }
        }
    }
    panic!("no real same-tick later-use callback")
}

#[test]
fn resident_joined_missing_generated_endpoint_refuses_calculation() {
    with_fixture_at(
        (
            [-4265., -4276., input::HEIGHT],
            [-4272., -4284., input::HEIGHT],
        ),
        |_install, height, compatibility, _inventory, policy, p, query, _| {
            let s = session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            runtime_with_route(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                |provider, context| {
                    let (constructor, water) = prerequisites(provider, context, 101, until());
                    assert!(matches!(
                provider.observe_joined_cold(
                    constructor,
                    water,
                    context,
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
        },
    );
}

#[test]
fn resident_joined_rejected_chase_prefix_never_calculates() {
    with_fixture(
        |_install, height, compatibility, _inventory, policy, p, query, _| {
            let s = session(&p);
            let mut provider =
                ResidentFilterProvider::bind_with_policy(&s, &p, height, query, policy).unwrap();
            runtime_with_route_input(
                &p,
                &s,
                height,
                compatibility,
                &mut provider,
                Some([256., 0., 0.]),
                |_, _| {},
                |provider, context| {
                    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                        unreachable!()
                    };
                    let selection = route
                        .require_collision_width(&p)
                        .unwrap()
                        .validated_model_selection(
                            &p,
                            route.owner.npc(),
                            route.owner.initialization(),
                        )
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
                    let constructor = provider.observe(context, 101, until(), || false).unwrap();
                    let prefix = provider
                        .observe_chase_los(&constructor, context, f::player(), 103, until(), || {
                            false
                        })
                        .unwrap();
                    assert_eq!(provider.los_for(&prefix,context,until(),||false).unwrap().decision,
            super::super::source_route_filter::resident::chase_los::ResidentChaseLosDecision::Distance);
                    assert!(provider
                        .los_for(&prefix, context, until(), || false)
                        .unwrap()
                        .ray
                        .is_none());
                    let water = provider
                        .observe_chase_water(&prefix, context, 104, until(), || false)
                        .unwrap();
                    assert!(provider
                        .water_for(&water, context, until(), || false)
                        .unwrap()
                        .owner
                        .is_none());
                    assert!(matches!(
                provider.observe_joined_cold(
                    constructor,
                    water,
                    context,
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
        },
    );
}
