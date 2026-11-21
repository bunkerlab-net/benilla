// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::player_death as d;
use synthetic_map_classification_fixture as f;
fn support() -> d::DeathSupport {
    // Existing authored callback exclusions, not a claim of raw map classification.
    d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|dependency| (dependency, Some(false))))
        .unwrap()
}
fn config(instanceable: bool) -> d::DeathConfig {
    d::DeathConfig {
        map_instanceable: instanceable,
        pve_reclaim_delay_enabled: true,
    }
}
#[test]
fn actual_loaded_map_kinds_bind_profile_and_survive_tick_replay_reopen() {
    for (kind, expected) in [
        (0, MapKind::Common),
        (1, MapKind::Instance),
        (2, MapKind::Raid),
        (3, MapKind::Battleground),
    ] {
        let mut profile = synthetic_fixture::initialized_profile();
        let map = profile.map_id;
        profile
            .initialize_map_classification(&f::catalog(map, kind))
            .unwrap();
        let source = profile.map_classification.as_ref().unwrap();
        assert_eq!(source.map_id(), map);
        assert_eq!(source.kind(), expected);
        assert_eq!(source.is_battleground(), kind == 3);
        assert_eq!(source.instanceable(), kind != 0);
        let binding = source.binding();
        let mut world = NativeWorld::synthetic_component_test(
            profile.clone(),
            [8; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        let start = world.checkpoint().unwrap();
        world
            .step(
                1,
                None,
                &mut synthetic_fixture::SyntheticAuthority::default(),
            )
            .unwrap();
        let checkpoint = world.checkpoint().unwrap();
        let restored = NativeWorld::restore_synthetic_component_test(
            profile.clone(),
            Checkpoint::decode(&checkpoint.encode().unwrap()).unwrap(),
        )
        .unwrap();
        assert_eq!(restored.checkpoint().unwrap(), checkpoint);
        assert_eq!(
            restored
                .profile
                .map_classification
                .as_ref()
                .unwrap()
                .binding(),
            binding
        );
        let mut replay =
            NativeWorld::restore_synthetic_component_test(profile.clone(), start).unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.state_digest(), world.state_digest());
        let mut other = profile;
        other.map_classification = None;
        assert!(NativeWorld::restore_synthetic_component_test(other, checkpoint).is_err());
    }
}
#[test]
fn missing_unknown_duplicate_or_changed_map_source_refuses_atomically() {
    let mut profile = synthetic_fixture::initialized_profile();
    let map = profile.map_id;
    let before = profile.clone();
    for rows in [
        vec![],
        vec![f::row(map + 1, 0)],
        vec![f::row(map, 4)],
        vec![f::row(map, 3), f::row(map, 0)],
    ] {
        assert!(matches!(
            profile.initialize_map_classification(&f::catalog_rows(&rows)),
            Err(NativeError::AuthorityUnavailable(_))
        ));
        assert_eq!(profile, before);
    }
    profile
        .initialize_map_classification(&f::catalog(map, 0))
        .unwrap();
    let bound = profile.clone();
    profile
        .initialize_map_classification(&f::catalog(map, 0))
        .unwrap();
    assert_eq!(profile, bound);
    let mut changed = f::row(map, 0);
    changed[3] = 123;
    assert!(profile
        .initialize_map_classification(&f::catalog_rows(&[changed]))
        .is_err());
    assert_eq!(profile, bound);
    profile.map_id += 1;
    assert!(profile.validate().is_err());
}
#[test]
fn source_classification_checks_real_death_configuration_in_either_order() {
    for kind in 0..=3 {
        let mut profile = synthetic_fixture::initialized_profile();
        profile
            .initialize_map_classification(&f::catalog(profile.map_id, kind))
            .unwrap();
        let before = profile.clone();
        if kind == 3 {
            for instanceable in [false, true] {
                assert!(matches!(
                    profile.configure_player_death(config(instanceable), support()),
                    Err(NativeError::UnsupportedMechanic(_))
                ));
                assert_eq!(profile, before);
            }
        } else {
            assert!(matches!(
                profile.configure_player_death(config(kind == 0), support()),
                Err(NativeError::AuthorityBinding)
            ));
            assert_eq!(profile, before);
            profile
                .configure_player_death(config(kind != 0), support())
                .unwrap();
            profile.validate().unwrap();
        }
    }
    let mut existing = synthetic_fixture::initialized_profile();
    existing
        .configure_player_death(config(false), support())
        .unwrap();
    let before = existing.clone();
    for kind in [1, 2, 3] {
        assert!(existing
            .initialize_map_classification(&f::catalog(existing.map_id, kind))
            .is_err());
        assert_eq!(existing, before);
    }
    existing
        .initialize_map_classification(&f::catalog(existing.map_id, 0))
        .unwrap();
    existing.validate().unwrap();
}
#[test]
fn absent_loaded_map_remains_unknown_not_an_inferred_non_battleground() {
    let mut profile = synthetic_fixture::initialized_profile();
    assert!(profile.map_classification.is_none());
    // Preserve legacy declared-domain behavior without promoting it to raw evidence.
    profile
        .configure_player_death(config(false), support())
        .unwrap();
    assert!(profile.map_classification.is_none());
    let world =
        NativeWorld::synthetic_component_test(profile, [8; 16], synthetic_fixture::player(), 42)
            .unwrap();
    assert!(world.profile.map_classification.is_none());
    assert!(world.checkpoint().is_ok());
}
