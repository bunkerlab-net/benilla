// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::{
    products::source_height::{PreparedSourceHeight, ResidentInitializationConfiguration},
    source_creature_initialization::resident::ResidentCreatureInitializationProvider,
    source_pathfinder_query::resident_joined_tests as fixture,
};
use super::*;
use benilla_world_runtime::native::{
    synthetic_fixture as f, synthetic_motion_fixture::SyntheticMotionAuthority, Authority,
    AuthorityReply, AuthorityRequest, NativeError, NativeWorld, Profile, Query,
};

fn anchor(
    profile: &Profile,
    session: &Session,
    height: &PreparedSourceHeight<'_, '_, '_>,
) -> RouteCommitBoundary {
    struct Initializer<F> {
        source: F,
        fallback: SyntheticMotionAuthority,
    }
    impl<F: FnMut(&AuthorityRequest) -> Result<AuthorityReply, NativeError>> Authority
        for Initializer<F>
    {
        fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
            if matches!(request.query, Query::CreatureInitialization(_)) {
                (self.source)(request)
            } else {
                self.fallback.resolve(request)
            }
        }
    }
    let source = ResidentCreatureInitializationProvider::bind(
        session.world,
        profile,
        height,
        Some(ResidentInitializationConfiguration::VmapHeightEnabled),
        fixture::until(),
        || false,
    )
    .unwrap();
    let mut initializer = Initializer {
        source: |request: &AuthorityRequest| {
            source
                .resolve(request, fixture::until(), || false)
                .map(|facts| AuthorityReply {
                    request: request.clone(),
                    facts,
                })
                .map_err(|_| NativeError::AuthorityBinding)
        },
        fallback: Default::default(),
    };
    let world = NativeWorld::synthetic_component_test_with_initialization(
        profile.clone(),
        session.world,
        f::player(),
        1234,
        None,
        &mut initializer,
    )
    .unwrap();
    RouteCommitBoundary::capture_initial(&world).unwrap()
}

#[test]
fn resident_reconstruction_requires_exact_fresh_owner_and_single_attachment() {
    fixture::with_route_fixture(
        |_, height, compatibility, _, policy, profile, query, other| {
            let session = fixture::session(&profile);
            let provider = ResidentFilterProvider::bind_with_policy(
                &session,
                &profile,
                height,
                query,
                policy.clone(),
            )
            .unwrap();
            let foreign =
                ResidentFilterProvider::bind_with_policy(&session, &profile, height, other, policy)
                    .unwrap();
            let boundary = anchor(&profile, &session, height);
            let bind = |provider: &ResidentFilterProvider<'_, '_, '_, '_>| {
                ResidentLedger::bind(
                    &session,
                    &profile,
                    f::player(),
                    provider,
                    compatibility,
                    fixture::until(),
                )
                .unwrap()
            };
            let mut ledger = bind(&provider);
            let before = ledger.diagnostic();
            assert_eq!(
                ledger.initialize_reconstruction(&session, &foreign, &boundary),
                Err(Failure::Binding)
            );
            assert_eq!(ledger.diagnostic(), before);
            let mut wrong = session.clone();
            wrong.profile[0] ^= 1;
            assert_eq!(
                ledger.initialize_reconstruction(&wrong, &provider, &boundary),
                Err(Failure::Binding)
            );
            assert_eq!(ledger.diagnostic(), before);
            let mut wrong_world = session.clone();
            wrong_world.world[0] ^= 1;
            let foreign_boundary = anchor(&profile, &wrong_world, height);
            assert_eq!(
                ledger.initialize_reconstruction(&session, &provider, &foreign_boundary),
                Err(Failure::Binding)
            );
            assert_eq!(ledger.diagnostic(), before);

            let mut used = bind(&provider);
            used.initialize(&boundary).unwrap();
            used.initialize(&boundary).unwrap(); // Ordinary attachment remains idempotent.
            let before = used.diagnostic();
            assert_eq!(
                used.initialize_reconstruction(&session, &provider, &boundary),
                Err(Failure::Binding)
            );
            assert_eq!(used.diagnostic(), before);

            let mut owner = ResidentReconstructionOwner::new(provider, ledger);
            owner
                .initialize_reconstruction(&session, &boundary)
                .unwrap();
            assert_eq!(owner.provider.retained_constructor_count(), 0);
            let before = owner.ledger.diagnostic();
            assert_eq!(
                owner.initialize_reconstruction(&session, &boundary),
                Err(Failure::Binding)
            );
            assert_eq!(owner.ledger.diagnostic(), before);
        },
    );
}

#[test]
fn resident_reconstruction_rejects_used_query_even_with_new_empty_ledger() {
    fixture::with_route_fixture(|_, height, compatibility, _, policy, profile, query, _| {
        let session = fixture::session(&profile);
        let boundary = anchor(&profile, &session, height);
        let mut provider =
            ResidentFilterProvider::bind_with_policy(&session, &profile, height, query, policy)
                .unwrap();
        fixture::runtime_with_route(
            &profile,
            &session,
            height,
            compatibility,
            &mut provider,
            |provider, request| {
                provider
                    .observe(request, 100, fixture::until(), || false)
                    .unwrap();
            },
        );
        assert_eq!(provider.retained_constructor_count(), 1);
        // The actual runtime batch rolled back, but the native constructor did not disappear.
        let ledger = ResidentLedger::bind(
            &session,
            &profile,
            f::player(),
            &provider,
            compatibility,
            fixture::until(),
        )
        .unwrap();
        let before = ledger.diagnostic();
        let mut owner = ResidentReconstructionOwner::new(provider, ledger);
        assert_eq!(
            owner.initialize_reconstruction(&session, &boundary),
            Err(Failure::Binding)
        );
        assert_eq!(owner.ledger.diagnostic(), before);
        assert!(owner.session.is_none());
        assert_eq!(owner.provider.retained_constructor_count(), 1);
    });
}
