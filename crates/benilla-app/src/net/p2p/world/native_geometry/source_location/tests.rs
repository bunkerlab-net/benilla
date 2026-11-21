// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual owned authored terrain/VMap/DBC bytes through runtime-issued requests.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::authored_source_location,
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    synthetic_fixture as fixture, synthetic_spell_fixture as spells, Authority, AuthorityReply,
    NativeError, NativeWorld,
};
use std::{collections::BTreeMap, time::Duration};

pub(crate) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn session(profile: &Profile) -> Session {
    Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: BTreeMap::new(),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: profile
                    .source_motion
                    .as_ref()
                    .unwrap()
                    .player_collision_height,
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            profile.content_digest,
            profile.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        same_polygon_route: None,
        player_environment: None,
        source_location: Some(
            SourceLocationPolicy::from_profile(profile, fixture::player()).unwrap(),
        ),
    }
}

pub(crate) fn setup_at(
    x: f32,
    row: Option<(u32, u32)>,
) -> (Profile, Session, AuthorityRequest, SourceLocationProvider) {
    let mut profile = spells::profile();
    profile.player.start = ai::Point::new(x, 0., 7.).unwrap();
    let snapshot = authored_source_location(profile.map_id, profile.content_digest, row);
    profile
        .initialize_source_location(
            snapshot.location_binding(),
            snapshot.location_catalog().clone(),
        )
        .unwrap();
    let session = session(&profile);
    let provider =
        SourceLocationProvider::bind(&session, &profile, fixture::player(), snapshot).unwrap();
    let mut world = NativeWorld::synthetic_component_test(
        profile.clone(),
        session.world,
        fixture::player(),
        1234,
    )
    .unwrap();
    #[derive(Default)]
    struct Capture {
        request: Option<AuthorityRequest>,
        other: spells::SpatialOnly,
    }
    impl Authority for Capture {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(request.query, Query::SourceLocation(_)) {
                self.request = Some(request.clone());
                return Err(NativeError::AuthorityUnavailable(
                    "capture selected location request",
                ));
            }
            self.other.resolve(request)
        }
    }
    let input = fixture::cast(&world, 133);
    let mut capture = Capture::default();
    assert!(world.step(1, Some(input), &mut capture).is_err());
    (
        profile,
        session,
        capture.request.expect("source-owned frame location query"),
        provider,
    )
}

pub(crate) fn setup() -> (Profile, Session, AuthorityRequest, SourceLocationProvider) {
    setup_at(0., Some((9, 12)))
}

#[test]
fn actual_reader_resolves_zone_and_area_and_echoes_exact_frame_pose() {
    let (profile, session, request, provider) = setup();
    let Facts::SourceLocation(facts) = provider.resolve(&session, &request, deadline()).unwrap()
    else {
        panic!()
    };
    let Query::SourceLocation(query) = &request.query else {
        panic!()
    };
    query
        .validate_for(&profile, session.world, fixture::player())
        .unwrap();
    assert_eq!(facts.query(), query.as_ref());
    assert_eq!((facts.zone_id(), facts.area_id()), (12, 9));
    assert_eq!(facts.area().unwrap().zone, 12);
    assert_eq!(query.pose().position, profile.player.start);
    assert_eq!(query.tick(), request.binding.tick);
    assert_eq!(
        query.snapshot_binding(),
        provider.snapshot.location_binding()
    );
    assert_eq!(
        facts.area(),
        Some(SourceLocationArea {
            id: 9,
            map_id: profile.map_id,
            zone: 12,
            explore_flag: 55,
            liquid_override: 0,
        })
    );
}

#[test]
fn present_zero_and_catalog_miss_remain_distinct_owned_observations() {
    for (row, expected_zone, expected_area) in [
        (None, 0, 0),
        (Some((0, 0)), 0, 0),
        (Some((0, 12)), 12, 0),
        (Some((9, 0)), 9, 9),
    ] {
        let (_, session, request, provider) = setup_at(0., row);
        let Facts::SourceLocation(facts) =
            provider.resolve(&session, &request, deadline()).unwrap()
        else {
            panic!()
        };
        assert_eq!(facts.area().map(|a| (a.id, a.zone)), row);
        assert_eq!(
            (facts.zone_id(), facts.area_id()),
            (expected_zone, expected_area)
        );
    }
}

#[test]
fn unavailable_neighbor_or_expired_deadline_is_never_an_empty_area() {
    let (_, session, request, provider) = setup();
    assert_eq!(
        provider.resolve(&session, &request, Instant::now()),
        Err(Failure::Deadline)
    );
    let (_, session, request, provider) = setup_at(0.125, Some((9, 12)));
    assert_eq!(
        provider.resolve(&session, &request, deadline()),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn selection_requires_exact_player_session_catalog_and_actual_products() {
    let (profile, mut selected, request, provider) = setup();
    selected.source_location = None;
    assert!(matches!(
        SourceLocationProvider::bind(
            &selected,
            &profile,
            fixture::player(),
            authored_source_location(profile.map_id, profile.content_digest, Some((9, 12))),
        ),
        Err(Failure::UnsupportedQuery)
    ));
    assert_eq!(
        provider.resolve(&selected, &request, deadline()),
        Err(Failure::Binding)
    );
    let (profile, selected, _, _) = setup();
    let mut other_player = fixture::player();
    other_player.character[0] ^= 1;
    assert!(matches!(
        SourceLocationProvider::bind(
            &selected,
            &profile,
            other_player,
            authored_source_location(profile.map_id, profile.content_digest, Some((9, 12))),
        ),
        Err(Failure::UnsupportedQuery)
    ));
    for (map, content, row) in [
        (profile.map_id + 1, profile.content_digest, Some((9, 12))),
        (profile.map_id, [99; 32], Some((9, 12))),
        (profile.map_id, profile.content_digest, Some((9, 13))),
        (profile.map_id, profile.content_digest, None),
    ] {
        assert!(matches!(
            SourceLocationProvider::bind(
                &selected,
                &profile,
                fixture::player(),
                authored_source_location(map, content, row),
            ),
            Err(Failure::Binding)
        ));
    }
    assert_eq!(
        SourceLocationPolicy::from_profile(&spells::profile(), fixture::player()),
        Err(Failure::UnsupportedQuery)
    );
}

#[test]
fn forged_source_identity_cannot_replace_loaded_snapshot() {
    for field in 0..3 {
        let mut profile = spells::profile();
        let snapshot =
            authored_source_location(profile.map_id, profile.content_digest, Some((9, 12)));
        let mut binding = snapshot.location_binding();
        match field {
            0 => binding.source_epoch[0] ^= 1,
            1 => binding.inventory[0] ^= 1,
            _ => binding.policy[0] ^= 1,
        }
        profile
            .initialize_source_location(binding, snapshot.location_catalog().clone())
            .unwrap();
        let selected = session(&profile);
        assert!(matches!(
            SourceLocationProvider::bind(&selected, &profile, fixture::player(), snapshot,),
            Err(Failure::Binding)
        ));
    }
}

#[test]
fn provider_rejects_any_session_replacement_and_wrong_outer_binding() {
    let (_, session, request, provider) = setup();
    for field in 0..9 {
        let mut changed = session.clone();
        match field {
            0 => changed.world[0] ^= 1,
            1 => changed.profile[0] ^= 1,
            2 => changed.policy[0] ^= 1,
            3 => changed.content[0] ^= 1,
            4 => changed.geometry[0] ^= 1,
            5 => changed.map += 1,
            6 => changed.player.as_mut().unwrap().speed = ai::Real::ONE,
            7 => changed.source_location = None,
            _ => changed.participants = None,
        }
        assert_eq!(
            provider.resolve(&changed, &request, deadline()),
            Err(Failure::Binding)
        );
    }
    for field in 0..6 {
        let mut changed = request.clone();
        match field {
            0 => changed.binding.world[0] ^= 1,
            1 => changed.binding.profile[0] ^= 1,
            2 => changed.binding.authority_policy[0] ^= 1,
            3 => changed.binding.geometry[0] ^= 1,
            4 => changed.binding.tick += 1,
            _ => {
                changed.binding.npc = Some(benilla_world_runtime::NpcId {
                    spawn: 2,
                    generation: 0,
                })
            }
        }
        assert_eq!(
            provider.resolve(&session, &changed, deadline()),
            Err(Failure::Binding)
        );
    }
}

#[test]
fn same_area_at_another_pose_is_not_the_same_query() {
    let (_, session, request, provider) = setup();
    let Facts::SourceLocation(facts) = provider.resolve(&session, &request, deadline()).unwrap()
    else {
        panic!()
    };
    let (_, other_session, other_request, other_provider) = setup_at(-0.125, Some((9, 12)));
    let Facts::SourceLocation(other_facts) = other_provider
        .resolve(&other_session, &other_request, deadline())
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(
        (facts.zone_id(), facts.area_id()),
        (other_facts.zone_id(), other_facts.area_id())
    );
    let Query::SourceLocation(other_query) = &other_request.query else {
        panic!()
    };
    assert_eq!(
        facts.validate_for(other_query),
        Err(NativeError::AuthorityBinding)
    );
}

#[test]
fn actual_location_provider_runs_nonempty_source_table_scope_and_rejects_matching_zone() {
    use benilla_world_runtime::native::{SpellAreaSourceRow, SpellAreaSourceRows};
    // Pinned classic-db ec4f596146be6467ea93c57397858e329e2db852 fresh-install
    // source-audited five raw rows. Spell/DBC/terrain inputs here remain authored.
    let mut rows = vec![SpellAreaSourceRow {
        spell: 6298,
        area: 148,
        quest_start: 0,
        quest_start_active: 0,
        quest_end: 0,
        condition_id: 0,
        aura_spell: 0,
        racemask: 0,
        gender: 2,
        autocast: 0,
    }];
    for area in [139, 2057, 2017, 3456] {
        rows.push(SpellAreaSourceRow {
            spell: 31906,
            area,
            quest_start: 0,
            quest_start_active: 0,
            quest_end: 0,
            condition_id: 1399,
            aura_spell: 30238,
            racemask: 0,
            gender: 0,
            autocast: 1,
        });
    }
    assert_eq!(rows.len(), 5);
    struct Loaded<'a> {
        session: &'a Session,
        provider: &'a SourceLocationProvider,
        other: spells::SpatialOnly,
        locations: Vec<SourceLocationFacts>,
    }
    impl Authority for Loaded<'_> {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if !matches!(request.query, Query::SourceLocation(_)) {
                return self.other.resolve(request);
            }
            let facts = self
                .provider
                .resolve(self.session, request, deadline())
                .map_err(|_| NativeError::AuthorityUnavailable("loaded source location"))?;
            let Facts::SourceLocation(location) = &facts else {
                panic!()
            };
            self.locations.push(location.clone());
            Ok(AuthorityReply {
                request: request.clone(),
                facts,
            })
        }
        fn observe_derived(&mut self, reply: &AuthorityReply) -> Result<(), NativeError> {
            self.other.observe_derived(reply)
        }
    }
    for zone in [12, 148] {
        let (mut profile, initial, mut source) = spells::inputs();
        profile.player.start = ai::Point::new(0., 0., 7.).unwrap();
        profile
            .initialize_spell_authority(&initial, source.clone())
            .unwrap();
        let snapshot =
            authored_source_location(profile.map_id, profile.content_digest, Some((9, zone)));
        profile
            .initialize_source_location(
                snapshot.location_binding(),
                snapshot.location_catalog().clone(),
            )
            .unwrap();
        let complete = SpellAreaSourceRows::CompleteTable(rows.clone());
        source.modifier_hooks.spell_area = complete.clone();
        for spell in &mut source.spells {
            spell.hooks.spell_area = complete.clone();
        }
        profile
            .initialize_spell_authority(&initial, source)
            .unwrap();
        let selected = session(&profile);
        let provider =
            SourceLocationProvider::bind(&selected, &profile, fixture::player(), snapshot).unwrap();
        let mut world =
            NativeWorld::synthetic_component_test(profile, selected.world, fixture::player(), 1234)
                .unwrap();
        let before = world.checkpoint().unwrap();
        let mut authority = Loaded {
            session: &selected,
            provider: &provider,
            other: spells::spatial_only(),
            locations: vec![],
        };
        let cast = fixture::cast(&world, 133);
        let outcome = world.step(1, Some(cast), &mut authority);
        assert_eq!(authority.locations.len(), 1);
        assert_eq!(
            (
                authority.locations[0].zone_id(),
                authority.locations[0].area_id()
            ),
            (zone, 9)
        );
        if zone == 12 {
            outcome.unwrap();
            assert_eq!(world.state().tick, 1);
            assert!(matches!(
                world.last_tick().unwrap().replies[0].request.query,
                Query::SourceLocation(_)
            ));
            world.checkpoint().unwrap();
        } else {
            assert_eq!(
                outcome.err(),
                Some(NativeError::UnsupportedMechanic(
                    "applicable spell_area source row"
                ))
            );
            assert_eq!(world.checkpoint().unwrap(), before);
        }
    }
}

#[test]
fn first_entry_uses_owned_reader_with_full_nonempty_area_scope_and_exact_pose() {
    use benilla_world_runtime::native::{
        synthetic_initial_cast_fixture, InitialCastEntryLocationQuery,
        InitialCastEntryLocationSource, SpellAreaSourceRow, SpellAreaSourceRows,
    };
    let (mut profile, input, prepared, mut source) = synthetic_initial_cast_fixture::inputs();
    profile.player.start = ai::Point::new(0., 0., 7.).unwrap();
    let snapshot = authored_source_location(profile.map_id, profile.content_digest, Some((9, 12)));
    source.entry_location = Some(
        InitialCastEntryLocationSource::new(
            snapshot.location_binding(),
            snapshot.location_catalog().clone(),
        )
        .unwrap(),
    );
    source.hooks.spell_area = SpellAreaSourceRows::CompleteTable(vec![SpellAreaSourceRow {
        spell: 6298,
        area: 148,
        quest_start: 0,
        quest_start_active: 0,
        quest_end: 0,
        condition_id: 0,
        aura_spell: 0,
        racemask: 0,
        gender: 2,
        autocast: 0,
    }]);
    let mut numeric = prepared.dependencies().clone();
    numeric.hook_scope_identity = source.hook_scope_identity();
    let prepared =
        benilla_sim::starter_init::prepare_starter(&input, &numeric, prepared.order()).unwrap();
    profile
        .initialize_player_prepared(&input, &prepared, fixture::float(100.), 1)
        .unwrap();
    profile
        .initialize_initial_cast_source(&input, source)
        .unwrap();
    let session = Session {
        world: [8; 16],
        profile: profile.digest(),
        policy: profile.authority_policy,
        content: profile.content_digest,
        geometry: profile.geometry_revision,
        map: profile.map_id,
        bodies: BTreeMap::new(),
        player: Some(PlayerMotion {
            body: Body {
                radius: profile.player.bounding_radius,
                height: ai::Real::new(2.).unwrap(),
            },
            speed: ai::Real::new(7.).unwrap(),
        }),
        participants: Some(NonRenderParticipants::authored_fixture(
            profile.content_digest,
            profile.map_id,
        )),
        static_los: None,
        primary_near_point: None,
        empty_selector_near_point: None,
        same_polygon_route: None,
        player_environment: None,
        source_location: None,
    };
    let query =
        InitialCastEntryLocationQuery::new(&profile, session.world, fixture::player()).unwrap();
    let provider =
        InitialCastEntryLocationProvider::bind(&session, &profile, fixture::player(), snapshot)
            .unwrap();
    let facts = provider.resolve(&session, &query, deadline()).unwrap();
    assert_eq!((facts.zone_id(), facts.area_id()), (12, 9));
    let world = NativeWorld::synthetic_component_test_with_initial_location(
        profile.clone(),
        session.world,
        fixture::player(),
        7,
        Some(facts),
    )
    .unwrap();
    assert_eq!(world.state().rng_counter, 1);
    let restored =
        NativeWorld::restore_synthetic_component_test(profile.clone(), world.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.checkpoint().unwrap(), world.checkpoint().unwrap());
    let mut wrong = session.clone();
    wrong.world[0] ^= 1;
    assert_eq!(
        provider.resolve(&wrong, &query, deadline()),
        Err(Failure::Binding)
    );
    let mut shifted = profile.clone();
    shifted.player.start.x = ai::Real::new(1.).unwrap();
    let shifted_query =
        InitialCastEntryLocationQuery::new(&shifted, session.world, fixture::player()).unwrap();
    assert_eq!(
        provider.resolve(&session, &shifted_query, deadline()),
        Err(Failure::Binding)
    );
    assert_eq!(
        provider.resolve(&session, &query, Instant::now()),
        Err(Failure::Deadline)
    );
    let wrong_snapshot =
        authored_source_location(profile.map_id, profile.content_digest, Some((9, 13)));
    assert!(matches!(
        InitialCastEntryLocationProvider::bind(
            &session,
            &profile,
            fixture::player(),
            wrong_snapshot
        ),
        Err(Failure::Binding)
    ));
}
