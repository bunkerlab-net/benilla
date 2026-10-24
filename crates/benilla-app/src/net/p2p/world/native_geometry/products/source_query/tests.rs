// SPDX-License-Identifier: GPL-3.0-or-later
//! Inventory/fresh-allocation proof only; acceptance here grants no world authority.
use super::super::{
    dry_route::tests as fixtures,
    liquid::{LiquidRequest, LiquidStatus},
    same_polygon_fixture::{self, RawMap, TerrainKind},
};
use super::*;
use benilla_native_nav::{
    SourceChaseFixedCacheWitness, SourceOwnerFilter, SourceSamePolygonBudget,
    SourceSamePolygonConfig, SourceWanderBudget, SourceWanderWitness,
    SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION, SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
    SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES, SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
};
use std::{cell::Cell, sync::Arc, time::Duration};

fn until() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn point(x: f32, y: f32, z: f32) -> [u32; 3] {
    [x, y, z].map(f32::to_bits)
}
fn setup() -> (Arc<VmapLiquidProducts>, Profile) {
    let raw = RawMap::new(TerrainKind::Flat);
    let (spec, files) = same_polygon_fixture::fixture(&raw);
    let products = fixtures::load(spec, &files);
    let environment = fixtures::environment(Arc::clone(&products));
    let profile = super::super::super::source_route_same_polygon::tests::source_profile(
        &environment,
        &raw.maps(),
    );
    (products, profile)
}
fn recipes(
    query: &SourceMapQuery,
) -> (
    SourceWanderReconstructionRecipe,
    SourceChaseFixedCacheRecipe,
) {
    let config = SourceSamePolygonConfig {
        normalize_z: false,
        ignore_normalization: false,
    };
    let mut wander_owner = query.retain_default();
    let mut prefix = wander_owner
        .prepare_wander_same_polygon(
            None,
            SOURCE_WANDER_ENDPOINT_COMPATIBILITY_VERSION,
            SourceOwnerFilter::GROUND,
            config,
            SourceWanderBudget {
                endpoint_lookup_primitives: SOURCE_WANDER_MAX_ENDPOINT_LOOKUP_PRIMITIVES,
                build_lookup_primitives: SOURCE_WANDER_MAX_BUILD_LOOKUP_PRIMITIVES,
                terminal_primitives: 1,
            },
            until(),
        )
        .unwrap();
    wander_owner
        .select_wander_prefix(&mut prefix, until())
        .unwrap();
    let selected = wander_owner
        .continue_wander_prefix(&prefix, point(5., 5., 0.), point(6., 5., 0.25))
        .unwrap();
    let terminal = selected
        .loaded_prepared_wander_same_polygon(&prefix, until())
        .unwrap();
    let mut wander = SourceWanderWitness::default();
    wander.record(&terminal).unwrap();
    let mut chase_owner = query.retain_default();
    let selected = chase_owner
        .select_for_calculate(query.slot(), point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let terminal = selected
        .loaded_cold_same_polygon_with_filter(
            SOURCE_COLD_DISTANCE_COMPATIBILITY_VERSION,
            SourceOwnerFilter::GROUND,
            config,
            SourceSamePolygonBudget::default(),
            until(),
        )
        .unwrap();
    let mut chase = SourceChaseFixedCacheWitness::default();
    chase.record(&terminal).unwrap();
    let selected = chase_owner
        .select_for_calculate(query.slot(), point(5., 5., 0.), point(5., 6., 0.))
        .unwrap();
    let terminal = selected
        .loaded_retained_same_polygon(
            &terminal,
            config,
            SourceSamePolygonBudget::default(),
            until(),
        )
        .unwrap();
    chase.record(&terminal).unwrap();
    (wander.recipe().unwrap(), chase.recipe().unwrap())
}

#[test]
fn staging_exposes_exact_immutable_geometry_without_loading_navigation() {
    let (products, profile) = setup();
    let stage = products.stage_source_query_projection(&profile).unwrap();
    let liquid = stage.filter_liquid().unwrap();
    assert_eq!(liquid.binding(), stage.liquid().binding());
    assert_eq!(
        stage.navigation_binding().inventory,
        products.inventory.revision()
    );
    assert_eq!(
        stage.navigation_binding().source_epoch,
        stage.static_vmap().binding().source_epoch
    );
    // Independent prior-loader policy sequence, including literal source capacity.
    let selected = profile.pathfinder_configuration().unwrap();
    let mut policy = Sha256::new();
    policy.update(SOURCE_MAP_QUERY_POLICY);
    policy.update(super::super::SOURCE_PIN);
    policy.update(super::super::DETOUR_TREE);
    policy.update(products.inventory.revision());
    policy.update(liquid.binding().policy);
    policy.update(selected.binding());
    policy.update(profile.map_id.to_le_bytes());
    policy.update(selected.selected().source_instance_id.to_le_bytes());
    policy.update(1024_u32.to_le_bytes());
    assert_eq!(
        stage.navigation_binding(),
        SnapshotBinding {
            map: profile.map_id,
            content: profile.content_digest,
            source_epoch: products.inventory.source_epoch,
            inventory: products.inventory.revision(),
            policy: policy.finalize().into(),
        }
    );
    let request = LiquidRequest {
        snapshot: liquid.binding(),
        ticket: 1,
        position: point(5., 5., 0.),
    };
    assert_eq!(
        liquid.query(request, until()).unwrap().status,
        LiquidStatus::NoWater
    );
    if benilla_native_nav::SUPPORTED_ABI {
        let ordinary = products.load_source_query_projection(&profile).unwrap();
        assert_eq!(stage.navigation_binding(), ordinary.navigation().binding());
        assert_eq!(
            stage.configuration_binding(),
            ordinary.navigation().configuration_binding()
        );
    }
    let mut wrong = profile;
    wrong.content_digest = [77; 32];
    assert!(products.stage_source_query_projection(&wrong).is_err());
}

#[test]
fn dual_loader_publishes_only_the_exact_accepted_fresh_pair_allocation() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let (products, profile) = setup();
    let old = products.load_source_query_projection(&profile).unwrap();
    let old_guard = old.navigation().query().retain_default();
    let (wander, chase) = recipes(old.navigation().query());
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let stage = products.stage_source_query_projection(&profile).unwrap();
    let expected = stage.navigation_binding();
    let wander_calls = Cell::new(0);
    let chase_calls = Cell::new(0);
    let accepted = Cell::new(false);
    let (marker, projection, reconstructed_wander, reconstructed_chase) = stage
        .reconstruct_wander_and_chase_observed(
            &wander,
            &wander_challenge,
            &chase,
            &chase_challenge,
            until(),
            |geometry, _| {
                assert_eq!(geometry.navigation_binding(), expected);
                wander_calls.set(wander_calls.get() + 1);
                Ok(())
            },
            |geometry, operation| {
                assert_eq!(geometry.navigation_binding(), expected);
                if let SourceChaseFixedCacheStage::PreSelection { .. } = operation {
                    chase_calls.set(chase_calls.get() + 1);
                }
                Ok(())
            },
            |geometry, pair| {
                assert_eq!(geometry.navigation_binding(), expected);
                assert_eq!(
                    old_guard.check_query(pair.query()),
                    Err(benilla_native_nav::Error::Identity)
                );
                assert!(pair.matches_challenges(&wander_challenge, &chase_challenge));
                assert_eq!(pair.check_query(pair.query()), Ok(()));
                accepted.set(true);
                Ok::<_, ()>((17, pair))
            },
        )
        .unwrap();
    assert_eq!(marker, 17);
    assert!(accepted.get());
    assert!(wander_calls.get() > 0);
    assert_eq!(chase_calls.get(), 2);
    assert_eq!(projection.navigation().binding(), expected);
    assert_eq!(reconstructed_wander.witness().recipe().unwrap(), wander);
    assert_eq!(reconstructed_chase.witness().recipe().unwrap(), chase);
    assert_eq!(
        reconstructed_wander
            .retained_query()
            .check_query(projection.navigation().query()),
        Ok(())
    );
    assert_eq!(
        reconstructed_chase
            .retained_query()
            .check_query(projection.navigation().query()),
        Ok(())
    );
    assert_eq!(old_guard.check_query(old.navigation().query()), Ok(()));
    assert_eq!(
        old_guard.check_query(projection.navigation().query()),
        Err(benilla_native_nav::Error::Identity)
    );
}

#[test]
fn observer_or_acceptance_failure_never_returns_a_projection() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let (products, profile) = setup();
    let old = products.load_source_query_projection(&profile).unwrap();
    let (wander, chase) = recipes(old.navigation().query());
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let accepted = Cell::new(false);
    let failed = products.stage_source_query_projection(&profile).unwrap()
        .reconstruct_wander_and_chase_observed::<(), ()>(
            &wander, &wander_challenge, &chase, &chase_challenge, until(),
            |_, _| Ok(()),
            |_, stage| {
                if matches!(stage, SourceChaseFixedCacheStage::PreSelection { operation, .. } if operation.ordinal == 1) {
                    return Err(SourceChaseFixedCacheError::UnsupportedLineage);
                }
                Ok(())
            },
            |_, pair| { accepted.set(true); Ok(((), pair)) },
        );
    assert!(matches!(
        failed,
        Err(SourceQueryReconstructionError::Native(
            SourceWanderAndChaseReconstructionError::Chase(
                SourceChaseFixedCacheError::UnsupportedLineage
            )
        ))
    ));
    assert!(!accepted.get());
    let failed = products
        .stage_source_query_projection(&profile)
        .unwrap()
        .reconstruct_wander_and_chase_observed::<(), &str>(
            &wander,
            &wander_challenge,
            &chase,
            &chase_challenge,
            until(),
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_, _| Err("owner rejection"),
        );
    assert!(matches!(
        failed,
        Err(SourceQueryReconstructionError::Acceptance(
            "owner rejection"
        ))
    ));
    let failed = products
        .stage_source_query_projection(&profile)
        .unwrap()
        .reconstruct_wander_and_chase_observed::<(), ()>(
            &wander,
            &wander_challenge,
            &chase,
            &chase_challenge,
            Instant::now(),
            |_, _| panic!("expired loader must not replay"),
            |_, _| panic!("expired loader must not replay"),
            |_, _| panic!("expired loader must not accept"),
        );
    assert!(matches!(
        failed,
        Err(SourceQueryReconstructionError::Native(_))
    ));
}

#[test]
fn same_bytes_and_challenges_cannot_replace_the_factory_pair_during_acceptance() {
    if !benilla_native_nav::SUPPORTED_ABI {
        return;
    }
    let (products, profile) = setup();
    let old = products.load_source_query_projection(&profile).unwrap();
    let (wander, chase) = recipes(old.navigation().query());
    let wander_challenge = SourceWanderReconstructionChallenge::new();
    let chase_challenge = SourceChaseFixedCacheChallenge::new();
    let replacement = products.stage_source_query_projection(&profile).unwrap();
    let replacement = SourceMapQuery::reconstruct_wander_and_chase(
        replacement.geometry.slot,
        replacement.params,
        &replacement.tiles,
        &wander,
        &wander_challenge,
        &chase,
        &chase_challenge,
        until(),
    )
    .unwrap();
    let rejected = products
        .stage_source_query_projection(&profile)
        .unwrap()
        .reconstruct_wander_and_chase_observed::<(), ()>(
            &wander,
            &wander_challenge,
            &chase,
            &chase_challenge,
            until(),
            |_, _| Ok(()),
            |_, _| Ok(()),
            |_, actual| {
                assert!(replacement.matches_challenges(&wander_challenge, &chase_challenge));
                assert_eq!(
                    actual.check_query(replacement.query()),
                    Err(benilla_native_nav::Error::Identity)
                );
                Ok(((), replacement))
            },
        );
    assert!(matches!(
        rejected,
        Err(SourceQueryReconstructionError::Product(Error::Identity))
    ));
}
