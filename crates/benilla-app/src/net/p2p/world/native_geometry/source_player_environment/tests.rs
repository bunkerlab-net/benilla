// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual loaded authored products and source-initialized player model witnesses.
use super::super::{
    core::{Body, NonRenderParticipants, PlayerMotion},
    products::{authored_player_environment, authored_player_environment_with_spell},
};
use super::*;
use benilla_sim::creature_ai as ai;
use benilla_world_runtime::native::{
    environment::LiquidStatus, synthetic_fixture as fixture,
    synthetic_player_model_fixture as models, Authority, AuthorityReply, Command, Input,
    NativeError, NativeWorld, OwnedInput,
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
        source_location: None,
        player_environment: Some(
            PlayerEnvironmentPolicy::from_profile(profile, fixture::player()).unwrap(),
        ),
    }
}

pub(crate) fn setup_at(
    x: f32,
    z: f32,
) -> (
    Profile,
    Session,
    AuthorityRequest,
    SourcePlayerEnvironmentProvider,
) {
    let mut profile = models::profile_with_fields(1., 1., 2.);
    profile.player.start = ai::Point::new(x, 0., z).unwrap();
    let snapshot = authored_player_environment(profile.map_id, profile.content_digest);
    profile
        .initialize_player_environment(
            snapshot.environment_binding(),
            snapshot.environment_catalog().clone(),
        )
        .unwrap();
    let session = session(&profile);
    let provider =
        SourcePlayerEnvironmentProvider::bind(&session, &profile, fixture::player(), snapshot)
            .unwrap();
    let mut world = NativeWorld::synthetic_component_test(
        profile.clone(),
        session.world,
        fixture::player(),
        1234,
    )
    .unwrap();
    let input = OwnedInput {
        authenticated_owner: fixture::player().owner,
        input: Input {
            world: session.world,
            tick: 1,
            player: fixture::player(),
            sequence: world.state().next_sequence,
            command: Command::MovementPose {
                destination: world.state().position,
                orientation: world.state().orientation,
                movement_flags: profile
                    .source_motion
                    .as_ref()
                    .unwrap()
                    .player_movement_flags,
            },
        },
    };
    #[derive(Default)]
    struct Capture(Option<AuthorityRequest>);
    impl Authority for Capture {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            assert!(matches!(request.query, Query::PlayerEnvironment(_)));
            self.0 = Some(request.clone());
            Err(NativeError::AuthorityUnavailable(
                "capture selected environment request",
            ))
        }
    }
    let mut capture = Capture::default();
    assert!(world.step(1, Some(input), &mut capture).is_err());
    (profile, session, capture.0.unwrap(), provider)
}

pub(crate) fn setup() -> (
    Profile,
    Session,
    AuthorityRequest,
    SourcePlayerEnvironmentProvider,
) {
    setup_at(0., 31.)
}

#[test]
fn actual_loaded_water_observation_binds_complete_query_and_height_witness() {
    for (z, expected) in [
        (31., LiquidStatus::AboveWater),
        (29., LiquidStatus::InWater),
    ] {
        let (profile, session, request, provider) = setup_at(0., z);
        let Facts::PlayerEnvironment(facts) =
            provider.resolve(&session, &request, deadline()).unwrap()
        else {
            panic!()
        };
        let Query::PlayerEnvironment(query) = &request.query else {
            panic!()
        };
        assert_eq!(facts.query(), query.as_ref());
        assert_eq!(facts.result().query, query.liquid_query());
        assert_eq!(facts.result().status, expected);
        assert_eq!(facts.result().data.unwrap().level, 30f32.to_bits());
        assert_eq!(facts.result().data.unwrap().type_flags, 8);
        assert_eq!(query.height_witness().height().bits(), 2f32.to_bits());
        query
            .height_witness()
            .validate_for(&profile, session.world, fixture::player())
            .unwrap();
    }
}

#[test]
fn expired_deadline_or_unloaded_neighbor_is_unavailable_never_no_water() {
    let (_, session, request, provider) = setup();
    assert_eq!(
        provider.resolve(&session, &request, Instant::now() - Duration::from_secs(1)),
        Err(Failure::Deadline)
    );
    let (_, session, request, provider) = setup_at(0.125, 31.);
    assert_eq!(
        provider.resolve(&session, &request, deadline()),
        Err(Failure::MissingGeometry)
    );
}

#[test]
fn provider_requires_selection_and_exact_actual_snapshot_and_catalog() {
    let (profile, mut session, request, provider) = setup();
    session.player_environment = None;
    assert!(matches!(
        SourcePlayerEnvironmentProvider::bind(
            &session,
            &profile,
            fixture::player(),
            authored_player_environment(profile.map_id, profile.content_digest)
        ),
        Err(Failure::UnsupportedQuery)
    ));
    assert_eq!(
        provider.resolve(&session, &request, deadline()),
        Err(Failure::Binding)
    );
    let (profile, session, _, _) = setup();
    for (map, content) in [
        (profile.map_id + 1, profile.content_digest),
        (profile.map_id, [99; 32]),
    ] {
        assert!(matches!(
            SourcePlayerEnvironmentProvider::bind(
                &session,
                &profile,
                fixture::player(),
                authored_player_environment(map, content)
            ),
            Err(Failure::Binding)
        ));
    }
    let changed_catalog =
        authored_player_environment_with_spell(profile.map_id, profile.content_digest, 77);
    assert_ne!(
        changed_catalog.environment_catalog(),
        profile.player_environment.as_ref().unwrap().catalog()
    );
    assert!(matches!(
        SourcePlayerEnvironmentProvider::bind(
            &session,
            &profile,
            fixture::player(),
            changed_catalog
        ),
        Err(Failure::Binding)
    ));
    let mut player = fixture::player();
    player.character[0] ^= 1;
    assert!(matches!(
        SourcePlayerEnvironmentProvider::bind(
            &session,
            &profile,
            player,
            authored_player_environment(profile.map_id, profile.content_digest)
        ),
        Err(Failure::UnsupportedQuery)
    ));
    let mut profile = models::profile_with_fields(1., 1., 2.);
    assert_eq!(
        PlayerEnvironmentPolicy::from_profile(&profile, fixture::player()),
        Err(Failure::UnsupportedQuery)
    );
    profile.player_collision_height = None;
    assert!(profile
        .initialize_player_environment(
            provider.snapshot.environment_binding(),
            provider.snapshot.environment_catalog().clone()
        )
        .is_err());
}

#[test]
fn forged_nonspatial_source_binding_does_not_replace_loaded_product_identity() {
    let (profile, _, _, _) = setup();
    let snapshot = authored_player_environment(profile.map_id, profile.content_digest);
    for field in 0..3 {
        let mut profile = models::profile_with_fields(1., 1., 2.);
        let mut binding = snapshot.environment_binding();
        match field {
            0 => binding.source_epoch[0] ^= 1,
            1 => binding.inventory[0] ^= 1,
            _ => binding.policy[0] ^= 1,
        }
        profile
            .initialize_player_environment(binding, snapshot.environment_catalog().clone())
            .unwrap();
        let session = session(&profile);
        assert!(matches!(
            SourcePlayerEnvironmentProvider::bind(
                &session,
                &profile,
                fixture::player(),
                authored_player_environment(profile.map_id, profile.content_digest)
            ),
            Err(Failure::Binding)
        ));
    }
}
