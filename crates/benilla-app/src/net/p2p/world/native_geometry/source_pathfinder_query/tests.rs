// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored products/configuration with actual runtime dispatch and native1024 query.
//! Synthetic route replies only advance the test runtime; no Route admission is implied.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::dry_route::tests as fixtures,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_combat_fixture as combat, synthetic_fixture as f,
    synthetic_motion_fixture as motion_fixture, Authority, AuthorityReply, Command,
    EffectiveDisabledMMapSet, Input, NativeError, NativeWorld, OwnedInput,
    SelectedMMapConfiguration,
};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use super::test_archive as archive;

fn point(x: f32, y: f32, z: f32) -> ai::Point {
    ai::Point::new(x, y, z).unwrap()
}
fn maps(map_id: u32) -> benilla_formats::MapCatalog {
    maps_kind(map_id, 0)
}
fn maps_kind(map_id: u32, instance_type: u32) -> benilla_formats::MapCatalog {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-query-map-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut bytes = b"WDBC".to_vec();
    for value in [1_u32, 42, 168, 9] {
        bytes.extend(value.to_le_bytes());
    }
    let mut row = [0_u32; 42];
    row[0] = map_id;
    row[1] = 1;
    row[2] = instance_type;
    for value in row {
        bytes.extend(value.to_le_bytes());
    }
    bytes.extend(b"\0Fixture\0");
    std::fs::write(
        &path,
        archive::archive(&[("DBFilesClient\\Map.dbc", bytes)]),
    )
    .unwrap();
    let mut chain = benilla_formats::Chain::open(&path).unwrap();
    let result = benilla_formats::load_map_catalog(&mut chain).unwrap();
    drop(chain);
    std::fs::remove_file(path).unwrap();
    result
}
fn profile() -> Profile {
    let products = fixtures::products();
    let legacy = products.load_dry_route_projection().unwrap();
    let b = legacy.navigation().binding();
    configured_profile(
        b.map,
        b.content,
        point(-400., -400., 0.),
        point(-400., -390., 0.),
    )
}
fn configured_profile(map: u32, content: [u8; 32], spawn: ai::Point, player: ai::Point) -> Profile {
    let mut p = f::initialized_profile();
    p.map_id = map;
    p.content_digest = content;
    p.npcs[0].ai.map_id = p.map_id;
    p.npcs[0].ai.content_revision = ai::movement::Revision(p.content_digest);
    p.npcs[0].ai.spawn = spawn;
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.npcs[0].faction_template = 2;
    p.player.start = player;
    p.source_motion = Some(motion_fixture::genesis(&p, 2));
    combat::install(&mut p);
    p.initialize_creature_capabilities(motion_fixture::creature_capability_sources(&p))
        .unwrap();
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled: true,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        &maps(p.map_id),
    )
    .unwrap();
    p
}
fn session(p: &Profile) -> Session {
    let settings = p.source_motion.as_ref().unwrap();
    Session {
        world: [8; 16],
        profile: p.digest(),
        policy: p.authority_policy,
        content: p.content_digest,
        geometry: p.geometry_revision,
        map: p.map_id,
        bodies: BTreeMap::from([(
            2,
            Body {
                radius: p.npcs[0].ai.bounding_radius,
                height: settings
                    .npc(2)
                    .unwrap()
                    .npc_policy
                    .settings
                    .collision_height,
            },
        )]),
        player: Some(PlayerMotion {
            body: Body {
                radius: p.player.bounding_radius,
                height: settings.player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            p.content_digest,
            p.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        source_location: None,
        player_environment: None,
        same_polygon_route: None,
    }
}
fn setup() -> (Profile, Session, SourcePathfinderQueryProvider) {
    let mut p = profile();
    let projection = fixtures::products()
        .load_source_query_projection(&p)
        .unwrap();
    p.geometry_revision = geometry_revision(projection.navigation().binding());
    let s = session(&p);
    let (_, query, _) = projection.into_parts();
    let provider = SourcePathfinderQueryProvider::bind(&s, &p, query).unwrap();
    (p, s, provider)
}
fn replacement(p: &Profile, s: &Session) -> SourcePathfinderQueryProvider {
    let (_, query, _) = fixtures::products()
        .load_source_query_projection(p)
        .unwrap()
        .into_parts();
    SourcePathfinderQueryProvider::bind(s, p, query).unwrap()
}
fn world(p: &Profile) -> NativeWorld {
    NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 1234).unwrap()
}
fn first_unconsumed(p: &Profile) -> AuthorityRequest {
    let mut w = world(p);
    let mut base = motion_fixture::SyntheticMotionAuthority::default();
    for tick in 1..=8 {
        w.step(tick, None, &mut base).unwrap();
        if let Some(context) = base
            .queries
            .iter()
            .find(|q| matches!(q.query, Query::SourceMotion(MotionQuery::Route(_))))
        {
            return context.clone();
        }
    }
    panic!("no genuine constructor dispatch");
}
fn route(
    context: &mut AuthorityRequest,
) -> &mut benilla_world_runtime::native::CreatureRouteRequest {
    let Query::SourceMotion(MotionQuery::Route(route)) = &mut context.query else {
        panic!("Route")
    };
    route
}
fn until() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

#[test]
fn source_product_binds_real1024_configuration_and_separate_policy() {
    let p = profile();
    let products = fixtures::products();
    let legacy = products.load_dry_route_projection().unwrap();
    let source = products.load_source_query_projection(&p).unwrap();
    let other = products.load_source_query_projection(&p).unwrap();
    assert_ne!(
        source.navigation().binding().policy,
        legacy.navigation().binding().policy
    );
    assert_eq!(source.navigation().binding(), other.navigation().binding());
    assert_eq!(
        source.navigation().configuration_binding(),
        p.pathfinder_configuration().unwrap().binding()
    );
    assert_eq!(source.navigation().query().query_capacity(), 1024);
    assert_eq!(
        source.navigation().query().slot(),
        SourceMapQuerySlot {
            map_id: p.map_id,
            instance_id: 0
        }
    );
    assert_eq!(
        source
            .navigation()
            .query()
            .retain_default()
            .check_query(other.navigation().query()),
        Err(benilla_native_nav::Error::Identity)
    );
    assert_eq!(
        source.liquid().binding().inventory,
        source.navigation().binding().inventory
    );
    assert_eq!(
        source.static_vmap().binding().inventory,
        source.navigation().binding().inventory
    );
}

#[test]
fn actual_dispatch_retains_preloaded_query_before_selected_tile_check() {
    let (p, _, mut provider) = setup();
    struct Consumer<'a> {
        provider: &'a mut SourcePathfinderQueryProvider,
        base: motion_fixture::SyntheticMotionAuthority,
        observed: bool,
    }
    impl Authority for Consumer<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
                let mut constructor = self.provider.retain_constructor(request).unwrap();
                assert!(constructor.dispatch().first_construction());
                assert!(!constructor.current_selected());
                assert_eq!(constructor.slot().instance_id, 0);
                assert!(constructor
                    .select_and_check_tiles(until())
                    .unwrap()
                    .both_present());
                assert!(constructor.current_selected());
                self.observed = true;
            }
            self.base.resolve(request)
        }
    }
    let mut w = world(&p);
    let mut authority = Consumer {
        provider: &mut provider,
        base: Default::default(),
        observed: false,
    };
    for tick in 1..=8 {
        w.step(tick, None, &mut authority).unwrap();
        if authority.observed {
            break;
        }
    }
    assert!(authority.observed);
    assert_eq!(provider.constructor_count(), 1);
}

#[test]
fn repeated_live_receipt_reuses_query_and_equal_byte_replacement_cannot_claim() {
    let (p, s, mut provider) = setup();
    let context = first_unconsumed(&p);
    {
        let mut constructor = provider.retain_constructor(&context).unwrap();
        assert!(!constructor.current_selected());
        assert_eq!(constructor.snapshot(), provider_binding(&p));
        constructor.select_and_check_tiles(until()).unwrap();
    }
    assert!(provider
        .retain_constructor(&context.clone())
        .unwrap()
        .current_selected());
    let mut other = replacement(&p, &s);
    assert_eq!(other.binding(), provider.binding());
    assert_eq!(
        other.retain_constructor(&context).err(),
        Some(ConstructorQueryError::Binding)
    );
    assert_eq!(other.constructor_count(), 0);
    assert_eq!(provider.constructor_count(), 1);
}
fn provider_binding(p: &Profile) -> SnapshotBinding {
    fixtures::products()
        .load_source_query_projection(p)
        .unwrap()
        .navigation()
        .binding()
}

#[test]
fn unclaimed_event_does_not_prove_query_was_loaded_before_dispatch() {
    let (p, s, original) = setup();
    let expected_binding = original.binding();
    drop(original);
    let context = first_unconsumed(&p);
    // This distinct allocation is intentionally loaded after the actual event.
    // The registry can prove its ownership now; the event cannot attest timing.
    // Future production installation must independently enforce preload order.
    let mut late = replacement(&p, &s);
    assert_eq!(late.binding(), expected_binding);
    assert!(!late
        .retain_constructor(&context)
        .unwrap()
        .current_selected());
    assert_eq!(late.constructor_count(), 1);
}

#[test]
fn transcript_hashes_missing_receipts_and_changed_generation_cannot_construct() {
    let (p, _, mut provider) = setup();
    let context = first_unconsumed(&p);
    let restored: AuthorityRequest =
        serde_json::from_value(serde_json::to_value(&context).unwrap()).unwrap();
    assert_eq!(restored, context);
    assert_eq!(
        provider.retain_constructor(&restored).err(),
        Some(ConstructorQueryError::Binding)
    );
    let mut missing = context.clone();
    route(&mut missing).pathfinder_dispatch = None;
    assert_eq!(
        provider.retain_constructor(&missing).err(),
        Some(ConstructorQueryError::Binding)
    );
    let mut changed = context.clone();
    changed.binding.npc.as_mut().unwrap().generation += 1;
    assert_eq!(
        provider.retain_constructor(&changed).err(),
        Some(ConstructorQueryError::Binding)
    );
    let mut wrong_profile = context.clone();
    wrong_profile.binding.profile[0] ^= 1;
    assert_eq!(
        provider.retain_constructor(&wrong_profile).err(),
        Some(ConstructorQueryError::Binding)
    );
    assert_eq!(provider.constructor_count(), 0);
    assert!(!provider
        .retain_constructor(&context)
        .unwrap()
        .current_selected());
}

#[test]
fn changed_world_or_tick_cannot_transplant_an_unclaimed_live_event() {
    let (p, s, mut provider) = setup();
    let context = first_unconsumed(&p);
    let mut foreign_session = s.clone();
    foreign_session.world = [9; 16];
    let mut foreign_provider = replacement(&p, &foreign_session);
    let mut transplanted = context.clone();
    transplanted.binding.world = foreign_session.world;
    assert_eq!(
        foreign_provider.retain_constructor(&transplanted).err(),
        Some(ConstructorQueryError::Binding)
    );
    assert_eq!(foreign_provider.constructor_count(), 0);
    let mut wrong_tick = context.clone();
    wrong_tick.binding.tick += 1;
    assert_eq!(
        provider.retain_constructor(&wrong_tick).err(),
        Some(ConstructorQueryError::Binding)
    );
    assert_eq!(provider.constructor_count(), 0);
    // Neither rejected context consumed the actual event's shared live claim.
    assert!(!provider
        .retain_constructor(&context)
        .unwrap()
        .current_selected());
}

#[test]
fn failing_after_constructor_then_runtime_rollback_reuses_retained_query_safely() {
    let (p, s, mut provider) = setup();
    struct Fail<'a> {
        provider: &'a mut SourcePathfinderQueryProvider,
        base: motion_fixture::SyntheticMotionAuthority,
        fail: bool,
        seen: Vec<AuthorityRequest>,
    }
    impl Authority for Fail<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(request.query, Query::SourceMotion(MotionQuery::Route(_))) {
                let mut constructor = self.provider.retain_constructor(request).unwrap();
                constructor.select_and_check_tiles(until()).unwrap();
                self.seen.push(request.clone());
                if self.fail {
                    return Err(NativeError::AuthorityUnavailable(
                        "authored post-constructor failure",
                    ));
                }
            }
            self.base.resolve(request)
        }
    }
    let mut w = world(&p);
    let mut authority = Fail {
        provider: &mut provider,
        base: Default::default(),
        fail: true,
        seen: vec![],
    };
    loop {
        let before = w.checkpoint().unwrap();
        let result = w.step(w.state().tick + 1, None, &mut authority);
        if !authority.seen.is_empty() {
            assert!(result.is_err());
            assert_eq!(before, w.checkpoint().unwrap());
            break;
        }
        result.unwrap();
        assert!(w.state().tick < 8);
    }
    authority.fail = false;
    w.step(w.state().tick + 1, None, &mut authority).unwrap();
    assert_eq!(authority.seen.len(), 2);
    assert_eq!(authority.seen[0], authority.seen[1]);
    let contexts = authority.seen.clone();
    assert_eq!(provider.constructor_count(), 1);
    assert!(provider
        .retain_constructor(&contexts[1])
        .unwrap()
        .current_selected());
    for context in contexts {
        let mut other = replacement(&p, &s);
        assert_eq!(
            other.retain_constructor(&context).err(),
            Some(ConstructorQueryError::Binding)
        );
    }
}

#[test]
fn changed_selected_configuration_or_profile_cannot_rebind_same_loaded_query() {
    let (p, s, _) = setup();
    let (_, query, _) = fixtures::products()
        .load_source_query_projection(&p)
        .unwrap()
        .into_parts();
    let mut changed = p.clone();
    changed.clock_origin_ms += 100;
    changed.npcs[0].ai.clock_origin_ms += 100;
    let changed_session = session(&changed);
    assert!(SourcePathfinderQueryProvider::bind(&s, &changed, query).is_err());
    let (_, query, _) = fixtures::products()
        .load_source_query_projection(&p)
        .unwrap()
        .into_parts();
    let mut changed = p.clone();
    changed.pathfinder_configuration = None;
    changed
        .initialize_pathfinder_configuration(
            SelectedMMapConfiguration {
                enabled: true,
                effective_disabled_maps: EffectiveDisabledMMapSet::Complete([99].into()),
                source_instance_id: 0,
            },
            &maps(changed.map_id),
        )
        .unwrap();
    assert!(SourcePathfinderQueryProvider::bind(&session(&changed), &changed, query).is_err());
    assert_ne!(s.profile, changed_session.profile);
}

#[test]
fn disabled_or_missing_selected_configuration_never_loads_constructor_query() {
    let mut p = profile();
    p.pathfinder_configuration = None;
    assert!(fixtures::products()
        .load_source_query_projection(&p)
        .is_err());
    p.initialize_pathfinder_configuration(
        SelectedMMapConfiguration {
            enabled: false,
            effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
            source_instance_id: 0,
        },
        &maps(p.map_id),
    )
    .unwrap();
    assert!(fixtures::products()
        .load_source_query_projection(&p)
        .is_err());
}

#[test]
fn retained_dispatch_requires_the_existing_constructor_registry() {
    let (p, s, mut provider) = setup();
    let mut w = world(&p);
    let mut base = motion_fixture::SyntheticMotionAuthority::default();
    let mut first = None;
    for tick in 1..=10 {
        let input = (tick >= 3).then(|| OwnedInput {
            authenticated_owner: w.state().player.owner,
            input: Input {
                world: w.state().world,
                tick,
                player: w.state().player,
                sequence: w.state().next_sequence,
                command: Command::MovementPose {
                    destination: point(-400., -390. + (tick - 2) as f32 * 0.6, 0.),
                    orientation: w.state().orientation,
                    movement_flags: benilla_sim::creature_motion::GroundMovementFlags::from_bits(1)
                        .unwrap(),
                },
            },
        });
        w.step(tick, input, &mut base).unwrap();
        for context in std::mem::take(&mut base.queries) {
            let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
                continue;
            };
            let receipt = route.require_pathfinder_dispatch(&p).unwrap();
            if receipt.first_construction() {
                first = Some(receipt.construction_binding());
                provider
                    .retain_constructor(&context)
                    .unwrap()
                    .select_and_check_tiles(until())
                    .unwrap();
            } else {
                assert_eq!(Some(receipt.construction_binding()), first);
                let mut absent = replacement(&p, &s);
                assert_eq!(
                    absent.retain_constructor(&context).err(),
                    Some(ConstructorQueryError::MissingConstructor)
                );
                assert_eq!(absent.constructor_count(), 0);
                assert!(provider
                    .retain_constructor(&context)
                    .unwrap()
                    .current_selected());
                assert_eq!(provider.constructor_count(), 1);
                return;
            }
        }
    }
    panic!("no genuine retained constructor dispatch");
}

#[test]
fn registry_budget_failure_does_not_consume_the_live_constructor_claim() {
    let (p, s, mut provider) = setup();
    let context = first_unconsumed(&p);
    let Query::SourceMotion(MotionQuery::Route(route)) = &context.query else {
        unreachable!()
    };
    let event = route.require_pathfinder_dispatch(&p).unwrap();
    // Authored full-registry control only. No fabricated entry is consumed.
    for offset in 1..=MAX_CONSTRUCTORS {
        let mut construction = event.construction_binding();
        construction.query_id += offset as u64;
        provider.constructors.push(RegisteredConstructor {
            npc: event.npc(),
            construction,
            first_event: event.clone(),
            reopened: None,
            query: provider.navigation.query().retain_default(),
        });
    }
    assert_eq!(
        provider.retain_constructor(&context).err(),
        Some(ConstructorQueryError::Budget)
    );
    let mut available = replacement(&p, &s);
    assert!(!available
        .retain_constructor(&context)
        .unwrap()
        .current_selected());
    assert_eq!(available.constructor_count(), 1);
}

#[test]
fn expired_calculate_budget_keeps_constructor_available_for_retry() {
    let (p, _, mut provider) = setup();
    let context = first_unconsumed(&p);
    let mut constructor = provider.retain_constructor(&context).unwrap();
    assert_eq!(
        constructor.select_and_check_tiles(Instant::now()),
        Err(ConstructorQueryError::Native(
            benilla_native_nav::Error::Deadline
        ))
    );
    assert!(!constructor.current_selected());
    assert!(constructor
        .select_and_check_tiles(until())
        .unwrap()
        .both_present());
    assert!(provider
        .retain_constructor(&context)
        .unwrap()
        .current_selected());
}

#[test]
fn actual_source_invalid_coordinate_dispatch_constructs_but_never_selects() {
    let mut p = profile();
    p.npcs[0].ai.spawn = point(17067., -400., 0.);
    p.player.start = point(17067., -390., 0.);
    let projection = fixtures::products()
        .load_source_query_projection(&p)
        .unwrap();
    p.geometry_revision = geometry_revision(projection.navigation().binding());
    let s = session(&p);
    let (_, query, _) = projection.into_parts();
    let mut provider = SourcePathfinderQueryProvider::bind(&s, &p, query).unwrap();
    let context = first_unconsumed(&p);
    let mut constructor = provider.retain_constructor(&context).unwrap();
    assert!(!constructor.current_selected());
    assert_eq!(
        constructor.select_and_check_tiles(until()),
        Err(ConstructorQueryError::Coordinates(
            SourceMapCoordError::DestinationInvalid
        ))
    );
    assert!(!constructor.current_selected());
}

#[test]
fn registered_handle_reborrows_exact_query_and_rejects_foreign_or_replaced_seal() {
    let (profile, session, mut provider) = setup();
    let request = first_unconsumed(&profile);
    let handle = provider.prepare_constructor(&request).unwrap();
    assert_eq!(provider.constructor_count(), 1);
    let before = provider
        .registered_constructor_for(handle.npc, handle.construction)
        .unwrap();
    assert!(before.1.same_live_dispatch(&handle.first_event));
    assert!(!before.2);
    assert!(!provider.reborrow(&handle).unwrap().current_selected());
    provider
        .reborrow(&handle)
        .unwrap()
        .select_and_check_tiles(until())
        .unwrap();
    assert!(provider.reborrow(&handle).unwrap().current_selected());
    let selected = provider
        .registered_constructor_for(handle.npc, handle.construction)
        .unwrap();
    assert_eq!(before.0, selected.0);
    assert!(before.1.same_live_dispatch(&selected.1));
    assert!(selected.2);
    let mut foreign = replacement(&profile, &session);
    assert_eq!(
        foreign.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding)
    );
    assert_eq!(foreign.constructor_count(), 0);
    // Another actual runtime emits equal bytes with a different live allocation.
    let replacement = first_unconsumed(&profile);
    assert_eq!(request, replacement);
    assert!(!request
        .require_pathfinder_dispatch(&profile)
        .unwrap()
        .same_live_dispatch(replacement.require_pathfinder_dispatch(&profile).unwrap()));
    let fresh = provider.prepare_constructor(&replacement).unwrap();
    assert_eq!(provider.constructor_count(), 1);
    assert_eq!(
        provider.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding)
    );
    assert!(provider.reborrow(&fresh).unwrap().current_selected());
    let replaced = provider
        .registered_constructor_for(fresh.npc, fresh.construction)
        .unwrap();
    assert_eq!(selected.0, replaced.0);
    assert!(!selected.1.same_live_dispatch(&replaced.1));
    assert!(replaced.1.same_live_dispatch(&fresh.first_event));
    assert!(replaced.2);
    let mut missing = fresh.construction;
    missing.query_id += 1;
    assert!(provider
        .registered_constructor_for(fresh.npc, missing)
        .is_none());
}

#[test]
fn committed_constructor_retirement_keeps_cache_free_head_and_stable_handle() {
    let (p, _, mut provider) = setup();
    let mut w = world(&p);
    let initial = super::super::route_commit::RouteCommitBoundary::capture_initial(&w).unwrap();
    let mut base = motion_fixture::SyntheticMotionAuthority::default();
    let mut first = None;
    for tick in 1..=8 {
        w.step(tick, None, &mut base).unwrap();
        first = base
            .queries
            .iter()
            .find(|context| matches!(context.query, Query::SourceMotion(MotionQuery::Route(_))))
            .cloned();
        if first.is_some() {
            break;
        }
    }
    let context = first.expect("genuine first Chase dispatch");
    let handle = provider.prepare_constructor(&context).unwrap();
    let boundary = super::super::route_commit::RouteCommitBoundary::capture_initial(&w).unwrap();
    let head = boundary
        .heads()
        .iter()
        .find(|head| head.npc() == handle.npc)
        .unwrap();
    // Synthetic routes supply no cache, but the actual Chase keeps its constructor.
    assert_eq!(head.construction(), Some(handle.construction));
    assert!(head.cache().is_none());
    let mut obsolete = handle.construction;
    obsolete.query_id += 1;
    // Registry-shape control only; no fabricated constructor serves a query.
    provider.constructors.insert(
        0,
        RegisteredConstructor {
            npc: handle.npc,
            construction: obsolete,
            first_event: handle.first_event.clone(),
            reopened: None,
            query: provider.navigation.query().retain_default(),
        },
    );
    assert_eq!(provider.constructor_count(), 2);
    provider.retain_committed_generators(&boundary);
    assert_eq!(provider.constructor_count(), 1);
    provider
        .reborrow(&handle)
        .unwrap()
        .select_and_check_tiles(until())
        .unwrap();
    provider.retain_committed_generators(&boundary);
    assert_eq!(provider.constructor_count(), 1);
    // Absence removes the exact old key; a saved handle cannot adopt a new owner.
    provider.retain_committed_generators(&initial);
    assert_eq!(provider.constructor_count(), 0);
    assert_eq!(
        provider.reborrow(&handle).err(),
        Some(ConstructorQueryError::MissingConstructor)
    );
}

#[test]
fn reopened_handle_requires_exact_origin_and_never_uses_historical_live_seal() {
    let (profile, session, mut provider) = setup();
    let request = first_unconsumed(&profile);
    let mut handle = provider.prepare_constructor(&request).unwrap();
    let live = handle.first_event.clone();
    let historical: PathfinderDispatchReceipt =
        serde_json::from_value(serde_json::to_value(&live).unwrap()).unwrap();
    assert_eq!(historical, live);
    assert!(!historical.same_live_dispatch(&live));
    let origin = Rc::new(());
    // Registry identity controls only. The separate pending/certificate installer
    // is the sole non-test constructor of this shape; no restored Route is served.
    provider.constructors[0].reopened = Some(Rc::clone(&origin));
    provider.constructors[0].first_event = historical.clone();
    assert_eq!(
        provider.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding),
        "a normal handle cannot adopt a reopened entry"
    );
    handle.reopened = Some(Rc::clone(&origin));
    handle.first_event = historical;
    assert!(provider.reborrow(&handle).is_ok());
    let mut foreign = replacement(&profile, &session);
    assert_eq!(
        foreign.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding)
    );
    handle.reopened = Some(Rc::new(()));
    assert_eq!(
        provider.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding),
        "equal metadata cannot replace the fresh reopened origin"
    );
    handle.reopened = Some(origin);
    provider.constructors[0].reopened = None;
    provider.constructors[0].first_event = live.clone();
    assert_eq!(
        provider.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding),
        "a reopened handle cannot adopt a normal entry"
    );
    handle.reopened = None;
    assert_eq!(
        provider.reborrow(&handle).err(),
        Some(ConstructorQueryError::Binding),
        "normal entries still require the genuine shared live event"
    );
    handle.first_event = live;
    assert!(provider.reborrow(&handle).is_ok());
}

#[test]
fn reopened_entry_rejects_first_use_without_claiming_or_replacing_it() {
    let (profile, session, mut provider) = setup();
    let request = first_unconsumed(&profile);
    let event = request.require_pathfinder_dispatch(&profile).unwrap();
    let historical: PathfinderDispatchReceipt =
        serde_json::from_value(serde_json::to_value(event).unwrap()).unwrap();
    let origin = Rc::new(());
    // Registry rejection control only; the installer separately requires an
    // actual pending NORMAL Wander checkpoint and its reconstructed query.
    provider.constructors.push(RegisteredConstructor {
        npc: event.npc(),
        construction: event.construction_binding(),
        first_event: historical.clone(),
        reopened: Some(Rc::clone(&origin)),
        query: provider.navigation.query().retain_default(),
    });
    for attempted in [request, first_unconsumed(&profile)] {
        assert_eq!(
            provider.retain_constructor(&attempted).err(),
            Some(ConstructorQueryError::Binding)
        );
        assert_eq!(provider.constructor_count(), 1);
        assert_eq!(provider.constructors[0].first_event, historical);
        assert!(Rc::ptr_eq(
            provider.constructors[0].reopened.as_ref().unwrap(),
            &origin
        ));
        let mut normal = replacement(&profile, &session);
        assert!(normal.retain_constructor(&attempted).is_ok());
        assert_eq!(normal.constructor_count(), 1);
    }
}

/// Test-only runtime metadata. Geometry is supplied by genuine generation callers.
pub(crate) fn generated_profile(start: [f32; 3], end: [f32; 3]) -> Profile {
    configured_profile(
        0,
        f::initialized_profile().content_digest,
        point(start[0], start[1], start[2]),
        point(end[0], end[1], end[2]),
    )
}

/// Exercises the real owner constructor receipt, with synthetic replies only to
/// advance unrelated runtime stages. This does not certify a movement Route.
pub(crate) fn assert_generated_dispatch(
    mut p: Profile,
    query: SourceMapQuerySnapshot,
    replacement_query: Option<SourceMapQuerySnapshot>,
    present: bool,
) {
    p.geometry_revision = geometry_revision(query.binding());
    let s = session(&p);
    let mut provider = SourcePathfinderQueryProvider::bind(&s, &p, query).unwrap();
    // This is generated by NativeWorld::step and the real owner constructor.
    let request = first_unconsumed(&p);
    {
        let mut constructor = provider.retain_constructor(&request).unwrap();
        assert!(constructor.dispatch().first_construction());
        assert!(!constructor.current_selected());
        assert_eq!(
            constructor
                .select_and_check_tiles(until())
                .unwrap()
                .both_present(),
            present
        );
        assert!(constructor.current_selected());
    }
    assert!(provider
        .retain_constructor(&request)
        .unwrap()
        .current_selected());
    assert_eq!(provider.constructor_count(), 1);
    let mut foreign = request.clone();
    foreign.binding.world = [9; 16];
    assert!(provider.retain_constructor(&foreign).is_err());
    if let Some(query) = replacement_query {
        assert_eq!(provider.binding(), query.binding());
        let mut replacement = SourcePathfinderQueryProvider::bind(&s, &p, query).unwrap();
        assert!(replacement.retain_constructor(&request).is_err());
        assert_eq!(replacement.constructor_count(), 0);
    }
}

pub(crate) fn assert_generated_binding_rejection(mut p: Profile, query: SourceMapQuerySnapshot) {
    p.geometry_revision = [3; 32];
    assert!(SourcePathfinderQueryProvider::bind(&session(&p), &p, query).is_err());
}

pub(crate) fn generated_rejected_configurations(p: &Profile) -> Vec<Profile> {
    let mut missing = p.clone();
    missing.pathfinder_configuration = None;
    let mut variants = vec![missing.clone()];
    for (enabled, instance, kind) in [(false, 0, 0), (true, 1, 0), (true, 0, 1)] {
        let mut changed = missing.clone();
        let result = changed.initialize_pathfinder_configuration(
            SelectedMMapConfiguration {
                enabled,
                effective_disabled_maps: EffectiveDisabledMMapSet::Complete(Default::default()),
                source_instance_id: instance,
            },
            &maps_kind(p.map_id, kind),
        );
        if instance != 0 || kind != 0 {
            assert!(
                result.is_err(),
                "unsupported map instances cannot enter a validated profile"
            );
        } else {
            result.unwrap();
            variants.push(changed);
        }
    }
    variants
}
