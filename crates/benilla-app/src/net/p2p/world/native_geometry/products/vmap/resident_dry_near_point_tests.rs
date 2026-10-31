// SPDX-License-Identifier: GPL-3.0-or-later
//! Real habitat-3 initialization and resident dry Swimming NearPoint consumption.
//! Route deliberately fails: no displacement, continuation or startup is certified.
use super::super::super::super::super::{
    core::{PrimaryNearPointPolicy, Session},
    source_pathfinder_query::resident_los_tests,
    source_route_filter::resident::{
        dry_near_point::{self, Observation, ResidentDryNearPointProvider},
        ResidentFilterError,
    },
};
use super::*;
use benilla_sim::{creature_motion, near_point};
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as models, synthetic_player_model_fixture as player_models,
};

fn fixture(test: impl FnOnce(&Install, &PreparedSourceHeight<'_, '_, '_>, Profile, Session)) {
    fixture_entries(shared_entries(), test);
}

fn fixture_entries(
    entries: Vec<Entry>,
    test: impl FnOnce(&Install, &PreparedSourceHeight<'_, '_, '_>, Profile, Session),
) {
    fixture_at(
        &entries,
        ai::Point::new(440.0, 940.0, 8.0).unwrap(),
        ai::Point::new(448.0, 940.0, 8.0).unwrap(),
        test,
    );
}

fn fixture_at(
    entries: &[Entry],
    spawn: ai::Point,
    player: ai::Point,
    test: impl FnOnce(&Install, &PreparedSourceHeight<'_, '_, '_>, Profile, Session),
) {
    let install = Install::new(entries);
    let before = install.bytes();
    let terrain = terrain(&install);
    let capture = source::prepare(&install.0, 0, || false).unwrap();
    let bundle = capture
        .assemble_map_bundle(profile(), || false)
        .unwrap()
        .unwrap();
    let height = PreparedSourceHeight::from_source(&terrain, &bundle, || false).unwrap();
    let binding = height
        .initialization_binding(CONTENT, CONFIG, deadline(), || false)
        .unwrap();
    let mut p = initial::profile_with_faction(
        binding,
        dry_near_point::geometry_revision(height.binding()),
        2,
    );
    // Stay within the source tile, away from the authored collision hull.
    p.npcs[0].ai.spawn = spawn;
    p.player.start = player;
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
    let starter = f::initialized_starter_input();
    if let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission {
        spec.spell_rank = 1;
    }
    let generation = p.creature_generations.as_ref().unwrap().source(2).unwrap();
    let (selected, _) = benilla_sim::creature_init::prepare(&generation.input)
        .unwrap()
        .resolve_with_model_receipt(benilla_sim::creature_init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: Some(1),
        })
        .unwrap();
    p.npcs[0].combat = Some(CreatureCombatProfile {
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
    synthetic_combat_fixture::install(&mut p);
    let maps =
        benilla_formats::load_map_catalog(&mut benilla_formats::Chain::open(&install.0).unwrap())
            .unwrap();
    // The final source catalogs own this configuration, not a foreign fixture.
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled: true,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        &maps,
    )
    .unwrap();
    p.pathfinder_configuration
        .as_ref()
        .unwrap()
        .constructor_binding(&p, 2)
        .unwrap();
    p.validate().unwrap();
    let mut s = resident_los_tests::session(&p);
    s.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(&p).unwrap());
    test(&install, &height, p, s);
    assert_eq!(install.bytes(), before);
}

struct NearAuthority<F> {
    source: F,
    authored: motion::SyntheticMotionAuthority,
}
impl<F: FnMut(&AuthorityRequest) -> Result<Option<Facts>, NativeError>> Authority
    for NearAuthority<F>
{
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        match (self.source)(request)? {
            Some(facts) => Ok(AuthorityReply {
                request: request.clone(),
                facts,
            }),
            None => self.authored.resolve(request),
        }
    }
}
fn point(facts: &Facts) -> ai::Point {
    let Facts::SourceMotion(MotionFacts::Spatial(reply)) = facts else {
        panic!("spatial facts")
    };
    let creature_motion::Fact::NearPoint(point) = reply.fact else {
        panic!("NearPoint")
    };
    point
}
fn bits(point: ai::Point) -> [u32; 3] {
    [point.x.bits(), point.y.bits(), point.z.bits()]
}

#[test]
fn resident_dry_swimming_near_point_reaches_route_then_rolls_back() {
    fixture(|install, height, p, s| {
        let provider = ResidentDryNearPointProvider::bind(
            &s,
            &p,
            f::player(),
            height,
            CONFIG,
            deadline(),
            || false,
        )
        .unwrap();
        let mut initialization = authority(&p, height);
        let mut world = initialize(p.clone(), &mut initialization).unwrap();
        assert_order(
            &world,
            &initialization.initializations[0],
            &initial_draws(&world),
        );
        let mut context = None;
        let mut accepted = None;
        let mut observations = vec![];
        let mut route = None;
        let mut responder = NearAuthority {
            authored: Default::default(),
            source: |request: &AuthorityRequest| match &request.query {
                Query::SourceMotion(MotionQuery::NearPoint(q)) => {
                    assert_eq!(
                        q.searcher.allowed_position_z_branch().unwrap(),
                        AllowedPositionZBranch::Swimming
                    );
                    let facts = provider
                        .resolve_observed(
                            &s,
                            request,
                            deadline(),
                            || false,
                            usize::MAX,
                            |o| observations.push(o.clone()),
                        )
                        .unwrap();
                    context = Some(request.clone());
                    accepted = Some(facts.clone());
                    Ok(Some(facts))
                }
                Query::SourceMotion(MotionQuery::Route(q)) => {
                    assert_eq!(q.request.goal, point(accepted.as_ref().unwrap()));
                    assert!(request
                        .require_pathfinder_dispatch(&p)
                        .unwrap()
                        .first_construction());
                    assert!(q.request.prior_cache.is_none());
                    route = Some(request.clone());
                    Err(NativeError::AuthorityUnavailable(
                        "dry NearPoint has no Route",
                    ))
                }
                _ => Ok(None),
            },
        };
        let mut refused = false;
        for tick in 1..=12 {
            let before = world.checkpoint().unwrap();
            if let Err(error) = world.step(tick, None, &mut responder) {
                assert!(
                    matches!(
                        error,
                        NativeError::AuthorityUnavailable("dry NearPoint has no Route")
                    ),
                    "{error:?}"
                );
                assert_eq!(world.checkpoint().unwrap(), before);
                refused = true;
                break;
            }
        }
        drop(responder);
        assert!(refused, "genuine runtime never reached Route refusal");
        assert!(route.is_some());
        let context = context.unwrap();
        let accepted = accepted.unwrap();
        let Query::SourceMotion(MotionQuery::NearPoint(q)) = &context.query else {
            unreachable!()
        };
        let [Observation::Ground {
            request,
            height: ground,
        }, Observation::Liquid(liquid), Observation::AllowedZ(z), Observation::Ray(ray)] =
            observations.as_slice()
        else {
            panic!("source order: {observations:?}")
        };
        assert_eq!(request.binding, height.binding());
        assert_eq!(request.max_search, 50.0f32.to_bits());
        assert_eq!(request.ticket, 1);
        assert_eq!(liquid.request().binding, height.binding());
        assert_eq!(liquid.request().mask, 15);
        assert_eq!(
            liquid.request().position,
            [request.position[0], request.position[1], *ground]
        );
        assert_eq!(*z, *ground);
        assert_eq!(
            bits(point(&accepted)),
            [request.position[0], request.position[1], *z]
        );
        assert_eq!(
            ray.request.from,
            near_point::primary::target_eye_bits(
                bits(q.request.target.pose.position),
                q.request.target.collision_height.bits()
            )
            .unwrap()
        );
        assert_eq!(ray.request.to, bits(point(&accepted)));
        assert!(!ray.request.ignore_m2);
        assert_eq!(ray.result, StaticRay::Miss);
        assert_eq!(
            provider
                .resolve(&s, &context, deadline(), || false)
                .unwrap(),
            accepted
        );
        assert_rejected(&provider, &s, &p, &context);
        // A completed observation is not permission to publish after its source
        // changes. Restore the authored bytes before testing the next boundary.
        for stop in 0..4 {
            let mut count = 0;
            let result = provider.resolve_observed(
                &s,
                &context,
                deadline(),
                || false,
                usize::MAX,
                |_| {
                    if count == stop {
                        install.write(&super::super::resident_liquid_tests::liquid_group_entries());
                    }
                    count += 1;
                },
            );
            assert!(result.is_err(), "changed source at boundary {stop}");
            install.write(&shared_entries());
            assert_eq!(
                provider
                    .resolve(&s, &context, deadline(), || false)
                    .unwrap(),
                accepted
            );
        }
    });
}

fn assert_rejected(
    provider: &ResidentDryNearPointProvider<'_, '_, '_, '_>,
    s: &Session,
    p: &Profile,
    context: &AuthorityRequest,
) {
    assert!(matches!(
        provider.resolve(s, context, Instant::now(), || false),
        Err(ResidentFilterError::Context(Failure::Deadline))
    ));
    assert!(matches!(
        provider.resolve(s, context, deadline(), || true),
        Err(ResidentFilterError::Source(CompositionError::Cancelled))
    ));
    let mut observed = false;
    assert!(provider
        .resolve_observed(s, context, deadline(), || false, 0, |_| observed = true)
        .is_err());
    assert!(!observed);
    // Find the fixture's exact work cost without assuming BIH implementation
    // constants. One less must exhaust only after ground and liquid completed.
    let total = (1..=128)
        .find(|limit| {
            provider
                .resolve_observed(s, context, deadline(), || false, *limit, |_| {})
                .is_ok()
        })
        .expect("bounded authored fixture work");
    let mut phases = vec![];
    let short = provider.resolve_observed(
        s,
        context,
        deadline(),
        || false,
        total - 1,
        |o| phases.push(o.clone()),
    );
    assert!(short.is_err());
    assert!(
        matches!(
            phases.as_slice(),
            [
                Observation::Ground { .. },
                Observation::Liquid(_),
                Observation::AllowedZ(_)
            ]
        ),
        "shared-budget prefix: {phases:?}"
    );
    assert!(provider
        .resolve_observed(s, context, deadline(), || false, total, |_| {})
        .is_ok());
    for stop in 0..4 {
        let cancelled = Cell::new(false);
        let mut count = 0;
        let result = provider.resolve_observed(
            s,
            context,
            deadline(),
            || cancelled.get(),
            usize::MAX,
            |_| {
                if count == stop {
                    cancelled.set(true);
                }
                count += 1;
            },
        );
        assert!(
            matches!(
                result,
                Err(ResidentFilterError::Source(CompositionError::Cancelled))
            ),
            "boundary {stop}: {result:?}"
        );
        assert_eq!(count, stop + 1);
    }
    for stop in 0..4 {
        let expires = Instant::now() + Duration::from_secs(1);
        let mut count = 0;
        let result = provider.resolve_observed(
            s,
            context,
            expires,
            || false,
            usize::MAX,
            |_| {
                if count == stop {
                    std::thread::sleep(
                        expires.saturating_duration_since(Instant::now())
                            + Duration::from_millis(1),
                    );
                }
                count += 1;
            },
        );
        assert!(
            matches!(result, Err(ResidentFilterError::Context(Failure::Deadline))),
            "expired boundary {stop}: {result:?}"
        );
        assert_eq!(count, stop + 1);
    }
    for case in 0..5 {
        let mut wrong = s.clone();
        match case {
            0 => wrong.primary_near_point = None,
            1 => wrong.participants = None,
            2 => wrong.geometry[0] ^= 1,
            3 => wrong.world[0] ^= 1,
            _ => {
                let mut edited = p.clone();
                edited.geometry_revision[0] ^= 1;
                wrong.primary_near_point =
                    Some(PrimaryNearPointPolicy::from_profile(&edited).unwrap());
            }
        }
        let mut seen = false;
        assert!(provider
            .resolve_observed(
                &wrong,
                context,
                deadline(),
                || false,
                usize::MAX,
                |_| seen = true
            )
            .is_err());
        assert!(!seen);
    }
    for case in 0..3 {
        let mut wire = serde_json::to_value(context).unwrap();
        let target = &mut wire["query"]["SourceMotion"]["NearPoint"]["target_player_height"];
        match case {
            0 => *target = serde_json::Value::Null,
            1 => target["witness"]["raw_height"] = 0x4000_0000u32.into(),
            _ => {
                target["witness"]["source_binding"][0] =
                    (target["witness"]["source_binding"][0].as_u64().unwrap() ^ 1).into()
            }
        }
        let wrong = serde_json::from_value(wire).unwrap();
        let mut seen = false;
        assert!(
            provider
                .resolve_observed(s, &wrong, deadline(), || false, usize::MAX, |_| seen = true)
                .is_err(),
            "edited target {case}"
        );
        assert!(!seen);
    }
    for case in 0..10 {
        let mut wire = serde_json::to_value(context).unwrap();
        let searcher = &mut wire["query"]["SourceMotion"]["NearPoint"]["searcher"];
        match case {
            0 => searcher["inhabit_type"] = 1.into(),
            1 => searcher["capability_movement_flags"] = 0x400u32.into(),
            2 => {
                searcher["npc"]["generation"] = (context.binding.npc.unwrap().generation + 1).into()
            }
            3 => searcher["initialization"]["request"]["collision_height"] = 0x4000_0000u32.into(),
            4 => {
                let inventory =
                    &mut searcher["initialization"]["request"]["snapshot"]["inventory"][0];
                *inventory = (inventory.as_u64().unwrap() ^ 1).into();
            }
            5 => searcher["initialization"] = serde_json::Value::Null,
            6 => {
                let model = &mut searcher["initialization"]["request"]["model_id"];
                *model = (model.as_u64().unwrap() + 1).into();
            }
            7 => {
                searcher["initialization"]["request"]["classification_height"] =
                    0x4000_0000u32.into()
            }
            8 => searcher["initialization"]["request"]["raw_height"] = 0x4000_0000u32.into(),
            _ => {
                searcher["source_binding"][0] =
                    (searcher["source_binding"][0].as_u64().unwrap() ^ 1).into()
            }
        }
        let wrong = serde_json::from_value(wire).unwrap();
        let mut seen = false;
        assert!(
            provider
                .resolve_observed(s, &wrong, deadline(), || false, usize::MAX, |_| seen = true)
                .is_err(),
            "edited searcher {case}"
        );
        assert!(!seen);
    }
}

#[test]
fn resident_dry_swimming_near_point_requires_exact_source_configuration() {
    fixture(|_, height, p, s| {
        for config in [None, Some(Configuration::VmapHeightDisabled)] {
            assert!(ResidentDryNearPointProvider::bind(
                &s,
                &p,
                f::player(),
                height,
                config,
                deadline(),
                || false
            )
            .is_err());
        }
        assert!(ResidentDryNearPointProvider::bind(
            &s,
            &p,
            f::player(),
            height,
            CONFIG,
            deadline(),
            || true
        )
        .is_err());
        assert!(ResidentDryNearPointProvider::bind(
            &s,
            &p,
            f::player(),
            height,
            CONFIG,
            Instant::now(),
            || false
        )
        .is_err());
        let mut wrong = p.clone();
        wrong.geometry_revision[0] ^= 1;
        let mut other = resident_los_tests::session(&wrong);
        other.primary_near_point = Some(PrimaryNearPointPolicy::from_profile(&wrong).unwrap());
        assert!(ResidentDryNearPointProvider::bind(
            &other,
            &wrong,
            f::player(),
            height,
            CONFIG,
            deadline(),
            || false
        )
        .is_err());
    });
}

#[test]
fn resident_dry_swimming_near_point_keeps_both_selected_generation_models() {
    fixture(|_, height, p, s| {
        let provider = ResidentDryNearPointProvider::bind(
            &s,
            &p,
            f::player(),
            height,
            CONFIG,
            deadline(),
            || false,
        )
        .unwrap();
        let mut selected_models = std::collections::BTreeSet::new();
        for seed in 0..32 {
            let mut initialization = authority(&p, height);
            let mut world = NativeWorld::synthetic_component_test_with_initialization(
                p.clone(),
                WORLD,
                f::player(),
                seed,
                None,
                &mut initialization,
            )
            .unwrap();
            let Query::CreatureInitialization(initial) = &initialization.initializations[0].query
            else {
                unreachable!()
            };
            if !selected_models.insert(initial.model_id) {
                continue;
            }
            assert_order(
                &world,
                &initialization.initializations[0],
                &initial_draws(&world),
            );
            let selected_height = initial.collision_height;
            let selected_model = initial.model_id;
            let mut goal = None;
            let mut reached_route = false;
            let mut responder = NearAuthority {
                authored: Default::default(),
                source: |request: &AuthorityRequest| match &request.query {
                    Query::SourceMotion(MotionQuery::NearPoint(q)) => {
                        let retained = q.searcher.initialization().unwrap();
                        assert_eq!(retained.request.model_id, selected_model);
                        assert_eq!(retained.request.collision_height, selected_height);
                        retained.request.validate_for(&p).unwrap();
                        assert_ne!(selected_height, retained.request.classification_height);
                        let facts = provider.resolve(&s, request, deadline(), || false).unwrap();
                        goal = Some(point(&facts));
                        Ok(Some(facts))
                    }
                    Query::SourceMotion(MotionQuery::Route(q)) => {
                        assert_eq!(Some(q.request.goal), goal);
                        reached_route = true;
                        Err(NativeError::AuthorityUnavailable(
                            "selected model has no Route",
                        ))
                    }
                    _ => Ok(None),
                },
            };
            let mut refused = false;
            for tick in 1..=12 {
                let before = world.checkpoint().unwrap();
                if let Err(error) = world.step(tick, None, &mut responder) {
                    assert!(
                        matches!(
                            error,
                            NativeError::AuthorityUnavailable("selected model has no Route")
                        ),
                        "seed {seed}: {error:?}"
                    );
                    assert_eq!(world.checkpoint().unwrap(), before);
                    refused = true;
                    break;
                }
            }
            drop(responder);
            assert!(
                refused && reached_route,
                "selected model {selected_model} did not reach Route refusal"
            );
            if selected_models.len() == 2 {
                break;
            }
        }
        assert_eq!(selected_models, std::collections::BTreeSet::from([7, 9]));
    });
}

#[test]
fn resident_dry_swimming_near_point_refuses_invalid_ground_and_vertical_fallback() {
    for base in [-200_000.0f32, -100_000.0, 100_001.0, 100.0] {
        let mut entries = shared_entries();
        let adt = &mut entries
            .iter_mut()
            .find(|entry| entry.name == ADT)
            .unwrap()
            .bytes;
        let mut offset = 0;
        let mut cells = 0;
        while offset + 8 <= adt.len() {
            let size = u32::from_le_bytes(adt[offset + 4..offset + 8].try_into().unwrap()) as usize;
            if &adt[offset..offset + 4] == b"KNCM" {
                let cell_base = if base == -200_000.0 {
                    // Flat GridMap height bypasses holes. Genuine nonflat input
                    // selects the array reader, which returns the source sentinel.
                    adt[offset + 8 + 0x3c..offset + 8 + 0x3e]
                        .copy_from_slice(&u16::MAX.to_le_bytes());
                    if cells == 0 {
                        8.25f32
                    } else {
                        7.25f32
                    }
                } else {
                    base
                };
                adt[offset + 8 + 0x70..offset + 8 + 0x74].copy_from_slice(&cell_base.to_le_bytes());
                cells += 1;
            }
            offset += 8 + size;
        }
        assert_eq!(cells, 256);
        fixture_entries(entries, |_, height, p, s| {
            let provider = ResidentDryNearPointProvider::bind(
                &s,
                &p,
                f::player(),
                height,
                CONFIG,
                deadline(),
                || false,
            )
            .unwrap();
            let mut initialization = authority(&p, height);
            if base == 100_001.0 {
                // The real initialization point already rejects the upward VMap
                // retry above its 100000-coordinate domain. No NearPoint request
                // exists at this bound, so do not claim its adapter guard ran.
                assert!(initialize(p, &mut initialization).is_err());
                assert_eq!(initialization.initializations.len(), 1);
                return;
            }
            let mut world = initialize(p, &mut initialization).unwrap();
            assert_order(
                &world,
                &initialization.initializations[0],
                &initial_draws(&world),
            );
            let mut observations = vec![];
            let mut near_count = 0;
            let mut failure = None;
            let mut responder = NearAuthority {
                authored: Default::default(),
                source: |request: &AuthorityRequest| match &request.query {
                    Query::SourceMotion(MotionQuery::NearPoint(_)) => {
                        near_count += 1;
                        let result = provider.resolve_observed(
                            &s,
                            request,
                            deadline(),
                            || false,
                            usize::MAX,
                            |o| observations.push(o.clone()),
                        );
                        failure = Some(
                            result.expect_err("unsupported source boundary cannot publish a point"),
                        );
                        Err(NativeError::AuthorityUnavailable(
                            "unsupported dry NearPoint",
                        ))
                    }
                    Query::SourceMotion(MotionQuery::Route(_)) => {
                        panic!("unsupported NearPoint reached Route")
                    }
                    _ => Ok(None),
                },
            };
            let mut refused = false;
            for tick in 1..=12 {
                let before = world.checkpoint().unwrap();
                if let Err(error) = world.step(tick, None, &mut responder) {
                    assert!(
                        matches!(
                            error,
                            NativeError::AuthorityUnavailable("unsupported dry NearPoint")
                        ),
                        "base {base}: {error:?}"
                    );
                    assert_eq!(world.checkpoint().unwrap(), before);
                    refused = true;
                    break;
                }
            }
            drop(responder);
            assert!(refused);
            assert_eq!(near_count, 1);
            if matches!(base, 100.0 | -100_000.0) {
                // The source writer clamps raw -100000 terrain to -500; this
                // exercises vertical refusal, NOT an exact -100000 query result.
                let expected = if base == 100.0 { 100.0f32 } else { -500.0f32 };
                assert!(
                    matches!(observations.first(), Some(Observation::Ground { height, .. }) if *height == expected.to_bits())
                );
                assert!(matches!(
                    failure,
                    Some(ResidentFilterError::Context(
                        Failure::UnsupportedNearPointFallback
                    ))
                ));
                assert!(
                    matches!(
                        observations.as_slice(),
                        [
                            Observation::Ground { .. },
                            Observation::Liquid(_),
                            Observation::AllowedZ(_)
                        ]
                    ),
                    "vertical: {observations:?}"
                );
            } else {
                assert!(
                    matches!(
                        failure,
                        Some(ResidentFilterError::Context(Failure::UnsupportedQuery))
                    ),
                    "base {base}: {failure:?}; {observations:?}"
                );
                assert!(
                    matches!(observations.as_slice(), [Observation::Ground { height, .. }] if *height == (-200_000.0f32).to_bits()),
                    "ground boundary {base}: {observations:?}"
                );
            }
        });
    }
}
