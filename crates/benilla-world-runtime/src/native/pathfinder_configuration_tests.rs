// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::{creature_init::ExternalFeature, creature_motion::Evidence};
use std::sync::atomic::{AtomicU64, Ordering};

fn profile() -> Profile {
    let mut p = synthetic_combat_fixture::profile();
    p.initialize_creature_capabilities(synthetic_motion_fixture::creature_capability_sources(&p))
        .unwrap();
    p
}
fn selected(enabled: bool, maps: &[u32], source_instance_id: u32) -> SelectedMMapConfiguration {
    SelectedMMapConfiguration {
        enabled,
        effective_disabled_maps: EffectiveDisabledMMapSet::Complete(maps.iter().copied().collect()),
        source_instance_id,
    }
}
fn catalog(rows: &[(u32, u32)]) -> MapCatalog {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "benilla-authored-pathfinder-map-{}-{}.MPQ",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let mut dbc = b"WDBC".to_vec();
    for value in [rows.len() as u32, 42, 168, 9] {
        dbc.extend(value.to_le_bytes());
    }
    for &(id, raw_type) in rows {
        let mut row = [0_u32; 42];
        row[0] = id;
        row[1] = 1;
        row[2] = raw_type;
        for value in row {
            dbc.extend(value.to_le_bytes());
        }
    }
    dbc.extend(b"\0Fixture\0");
    std::fs::write(
        &path,
        synthetic_creature_model_fixture::archive(&[("DBFilesClient\\Map.dbc", dbc)]),
    )
    .unwrap();
    let mut chain = benilla_formats::Chain::open(&path).unwrap();
    let result = benilla_formats::load_map_catalog(&mut chain).unwrap();
    drop(chain);
    std::fs::remove_file(path).unwrap();
    result
}
fn owned() -> Profile {
    let mut p = profile();
    p.initialize_pathfinder_configuration(selected(true, &[], 0), &catalog(&[(p.map_id, 0)]))
        .unwrap();
    p
}
fn flags(p: &Profile, extra_flags: u32) -> Vec<NpcCombatAuthoritySource> {
    vec![NpcCombatAuthoritySource {
        entry: p.npcs[0].entry,
        extra_flags,
        static_flags1: 0,
        static_flags2: 0,
        static_flags3: 0,
        static_flags4: 0,
    }]
}

#[test]
fn selected_effective_truth_table_preserves_source_precedence() {
    for enabled in [false, true] {
        for disabled in [false, true] {
            let settings = selected(enabled, if disabled { &[7] } else { &[] }, 0);
            for (forcing, expected) in [
                (MMapForcingStatus::Default, enabled && !disabled),
                (MMapForcingStatus::Forced, enabled),
                (MMapForcingStatus::Ignored, false),
            ] {
                assert_eq!(
                    settings.pathfinding_enabled(7, forcing).unwrap(),
                    expected,
                    "enabled={enabled}, disabled={disabled}, forcing={forcing:?}"
                );
            }
            assert_eq!(
                settings
                    .pathfinding_enabled(8, MMapForcingStatus::Default)
                    .unwrap(),
                enabled
            );
        }
    }
}

#[test]
fn missing_effective_capture_is_never_an_empty_disabled_set() {
    for enabled in [false, true] {
        let settings = SelectedMMapConfiguration {
            enabled,
            effective_disabled_maps: EffectiveDisabledMMapSet::Missing,
            source_instance_id: 0,
        };
        for forcing in [
            MMapForcingStatus::Default,
            MMapForcingStatus::Forced,
            MMapForcingStatus::Ignored,
        ] {
            assert!(matches!(
                settings.pathfinding_enabled(0, forcing),
                Err(NativeError::AuthorityUnavailable(_))
            ));
        }
        let mut p = profile();
        let before = p.clone();
        assert!(matches!(
            p.initialize_pathfinder_configuration(settings, &catalog(&[(p.map_id, 0)])),
            Err(NativeError::AuthorityUnavailable(_))
        ));
        assert_eq!(p, before);
    }
}

#[test]
fn every_selected_input_is_required_including_explicit_instance_slot() {
    let settings = selected(true, &[], 0);
    let value = serde_json::to_value(&settings).unwrap();
    assert_eq!(
        serde_json::from_value::<SelectedMMapConfiguration>(value.clone()).unwrap(),
        settings
    );
    for field in ["enabled", "effective_disabled_maps", "source_instance_id"] {
        let mut missing = value.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<SelectedMMapConfiguration>(missing).is_err(),
            "{field}"
        );
    }
    for slot in [1, u32::MAX] {
        let mut p = profile();
        let before = p.clone();
        assert!(p
            .initialize_pathfinder_configuration(
                selected(true, &[], slot),
                &catalog(&[(p.map_id, 0)])
            )
            .is_err());
        assert_eq!(p, before);
    }
}

#[test]
fn actual_map_catalog_preserves_raw_type_and_server_dungeon_predicate() {
    let maps = catalog(&[(10, 0), (11, 1), (12, 2), (13, 3), (14, u32::MAX)]);
    for (map, raw, dungeon) in [
        (10, 0, false),
        (11, 1, true),
        (12, 2, true),
        (13, 3, false),
        (14, u32::MAX, false),
    ] {
        let source = SelectedMapSource::from_catalog(map, &maps).unwrap();
        assert_eq!(source.map_id, map);
        assert_eq!(source.raw_instance_type, raw);
        assert_eq!(source.is_dungeon(), dungeon);
    }
    assert!(!maps.is_party_dungeon(12));
    assert!(SelectedMapSource::from_catalog(12, &maps)
        .unwrap()
        .is_dungeon());
    assert!(matches!(
        SelectedMapSource::from_catalog(999, &maps),
        Err(NativeError::AuthorityUnavailable(_))
    ));
}

#[test]
fn only_actual_outdoor_rows_are_selectable_and_missing_never_becomes_zero() {
    for raw in [0, 1, 2, 3, u32::MAX] {
        let mut p = profile();
        let before = p.clone();
        let result = p.initialize_pathfinder_configuration(
            selected(true, &[], 0),
            &catalog(&[(p.map_id, raw)]),
        );
        assert_eq!(result.is_ok(), raw == 0, "raw type {raw}");
        if raw != 0 {
            assert_eq!(p, before);
        }
    }
    let mut p = profile();
    let before = p.clone();
    assert!(matches!(
        p.initialize_pathfinder_configuration(selected(true, &[], 0), &catalog(&[])),
        Err(NativeError::AuthorityUnavailable(_))
    ));
    assert_eq!(p, before);
}

#[test]
fn creature_ignore_beats_force_and_otherwise_delegates_to_unit_forcing() {
    for unit in [
        MMapForcingStatus::Default,
        MMapForcingStatus::Forced,
        MMapForcingStatus::Ignored,
    ] {
        assert_eq!(creature_forcing(0, unit), unit);
        assert_eq!(creature_forcing(0x80000, unit), unit);
        assert_eq!(
            creature_forcing(MMAP_FORCE_ENABLE, unit),
            MMapForcingStatus::Forced
        );
        assert_eq!(
            creature_forcing(MMAP_FORCE_DISABLE, unit),
            MMapForcingStatus::Ignored
        );
        assert_eq!(
            creature_forcing(MMAP_FORCE_ENABLE | MMAP_FORCE_DISABLE, unit),
            MMapForcingStatus::Ignored
        );
    }
}

#[test]
fn constructor_binding_uses_retained_complete_raw_flags_and_default_closure() {
    for extra in [0, 0x80000] {
        let mut p = profile();
        p.initialize_combat_authority(flags(&p, extra)).unwrap();
        p.initialize_pathfinder_configuration(
            selected(true, &[p.map_id + 1], 0),
            &catalog(&[(p.map_id, 0)]),
        )
        .unwrap();
        let source = p.pathfinder_configuration().unwrap();
        let witness = source
            .constructor_binding(&p, p.npcs[0].ai.creature_id)
            .unwrap();
        assert_eq!(witness.raw_extra_flags(), extra);
        assert_eq!(witness.forcing(), MMapForcingStatus::Default);
        assert_eq!(witness.map_id(), p.map_id);
        assert_eq!(witness.raw_instance_type(), 0);
        assert!(!witness.source_is_dungeon());
        assert_eq!(witness.source_instance_id(), 0);
        assert_eq!(witness.source_binding(), source.binding());
        witness.validate_for(&p, witness.spawn_id()).unwrap();
        assert!(matches!(
            source.constructor_binding(&p, 999),
            Err(NativeError::UnknownTarget)
        ));
    }
    for extra in [
        MMAP_FORCE_ENABLE,
        MMAP_FORCE_DISABLE,
        MMAP_FORCE_ENABLE | MMAP_FORCE_DISABLE,
    ] {
        let mut p = profile();
        let before = p.clone();
        assert!(p.initialize_combat_authority(flags(&p, extra)).is_err());
        assert_eq!(p, before);
    }
}

#[test]
fn disabled_settings_are_retained_but_cannot_claim_a_constructor_binding() {
    for settings in [selected(false, &[], 0), selected(true, &[7], 0)] {
        let mut p = profile();
        assert_eq!(p.map_id, 7);
        p.initialize_pathfinder_configuration(settings.clone(), &catalog(&[(p.map_id, 0)]))
            .unwrap();
        let source = p.pathfinder_configuration().unwrap();
        assert_eq!(source.selected(), &settings);
        assert!(matches!(
            source.constructor_binding(&p, 2),
            Err(NativeError::UnsupportedMechanic(_))
        ));
        let before = p.clone();
        assert!(p
            .initialize_pathfinder_configuration(selected(true, &[], 0), &catalog(&[(p.map_id, 0)]))
            .is_err());
        assert_eq!(
            p, before,
            "same-map re-enable cannot initialize a source query retroactively"
        );
    }
}

#[test]
fn repeated_identical_selection_is_stable_and_all_changes_roll_back() {
    let mut p = owned();
    let before = p.clone();
    let digest = p.digest();
    p.initialize_pathfinder_configuration(selected(true, &[], 0), &catalog(&[(p.map_id, 0)]))
        .unwrap();
    assert_eq!(p, before);
    assert_eq!(p.digest(), digest);
    for settings in [
        selected(false, &[], 0),
        selected(true, &[99], 0),
        selected(true, &[], 1),
    ] {
        assert!(p
            .initialize_pathfinder_configuration(settings, &catalog(&[(p.map_id, 0)]))
            .is_err());
        assert_eq!(p, before);
        assert_eq!(p.digest(), digest);
    }
    for raw in [1, 2] {
        assert!(p
            .initialize_pathfinder_configuration(
                selected(true, &[], 0),
                &catalog(&[(p.map_id, raw)])
            )
            .is_err());
        assert_eq!(p, before);
    }
    assert!(p.initialize_combat_authority(flags(&p, 0x80000)).is_err());
    assert_eq!(
        p, before,
        "changed retained operands cannot silently reseed the frozen source"
    );
}

#[test]
fn missing_owner_catalogs_and_invalid_nontransport_domain_cannot_be_selected() {
    for kind in 0..4 {
        let mut p = profile();
        match kind {
            0 => p.creature_capabilities = None,
            1 => p.interactions = None,
            2 => {
                p.interactions = None;
                p.initialize_interactions(synthetic_combat_fixture::source_rows(&p))
                    .unwrap();
            }
            3 => {
                p.source_motion
                    .as_mut()
                    .unwrap()
                    .npcs
                    .get_mut(&2)
                    .unwrap()
                    .npc_policy
                    .audit
                    .flight_swimming_transport = Evidence::Present
            }
            _ => unreachable!(),
        }
        let before = p.clone();
        assert!(
            p.initialize_pathfinder_configuration(
                selected(true, &[], 0),
                &catalog(&[(p.map_id, 0)])
            )
            .is_err(),
            "missing domain {kind}"
        );
        assert_eq!(p, before);
    }
}

#[test]
fn every_capability_external_feature_and_unknown_capture_fail_closed() {
    for feature in [
        ExternalFeature::PetSummonOrControl,
        ExternalFeature::InstanceOrBattleground,
        ExternalFeature::FlightSwimmingTransportOrMount,
        ExternalFeature::ExistingStateOrForcedLevel,
        ExternalFeature::GameEventOrPoolSubstitution,
        ExternalFeature::ConditionalOrGroupedSpawn,
        ExternalFeature::LinkingOrAssistance,
        ExternalFeature::Addon,
        ExternalFeature::InitialAuraOrUnitModifier,
        ExternalFeature::ScriptOrAiSubstitution,
        ExternalFeature::SpellList,
        ExternalFeature::MapOrOutdoorPvpCreationHook,
        ExternalFeature::EquipmentOrLootWeapon,
        ExternalFeature::EncounterCorpseOverride,
    ] {
        let mut p = owned();
        let before = p.clone();
        let CreatureCapabilityRows::Complete(mut rows) =
            synthetic_motion_fixture::creature_capability_sources(&p)
        else {
            unreachable!()
        };
        rows[0].external_features = CreatureCapabilityRows::Complete(vec![feature]);
        assert!(
            p.initialize_creature_capabilities(CreatureCapabilityRows::Complete(rows))
                .is_err(),
            "{feature:?}"
        );
        assert_eq!(p, before);
    }
    let mut p = profile();
    let before = p.clone();
    let CreatureCapabilityRows::Complete(mut rows) =
        synthetic_motion_fixture::creature_capability_sources(&p)
    else {
        unreachable!()
    };
    rows[0].external_features = CreatureCapabilityRows::Missing;
    assert!(p
        .initialize_creature_capabilities(CreatureCapabilityRows::Complete(rows))
        .is_err());
    assert_eq!(p, before);
}

#[test]
fn configuration_is_part_of_profile_digest_and_consistency_is_rechecked() {
    let original = profile();
    let p = owned();
    assert_ne!(p.digest(), original.digest());
    let source = p.pathfinder_configuration().unwrap();
    let witness = source.constructor_binding(&p, 2).unwrap();
    for field in 0..6 {
        let mut altered = p.clone();
        match field {
            0 => altered.content_digest = [13; 32],
            1 => altered.authority_policy = [14; 32],
            2 => altered.map_id += 1,
            3 => altered.creature_capabilities = None,
            4 => altered.pathfinder_configuration = None,
            5 => altered.geometry_revision = [15; 32],
            _ => unreachable!(),
        }
        assert!(witness.validate_for(&altered, 2).is_err(), "field {field}");
    }
    let mut altered = witness.clone();
    altered.owner.raw_extra_flags ^= 0x80000;
    assert!(altered.validate_for(&p, 2).is_err());
    assert!(witness.validate_for(&p, 999).is_err());
}

#[test]
fn raw_source_operands_are_serialized_without_a_query_or_liveness_claim() {
    let p = owned();
    let value = serde_json::to_value(p.pathfinder_configuration().unwrap()).unwrap();
    assert_eq!(value["selected"]["enabled"], true);
    assert_eq!(value["selected"]["source_instance_id"], 0);
    assert_eq!(
        value["selected"]["effective_disabled_maps"]["Complete"],
        serde_json::json!([])
    );
    assert_eq!(value["map"]["map_id"], p.map_id);
    assert_eq!(value["map"]["raw_instance_type"], 0);
    assert_eq!(value["owners"]["2"]["raw_extra_flags"], 0);
    assert_eq!(value["owners"]["2"]["forcing"], "Default");
    assert_eq!(
        value["source_policy"]["revision"],
        "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f"
    );
    let witness = serde_json::to_value(
        p.pathfinder_configuration()
            .unwrap()
            .constructor_binding(&p, 2)
            .unwrap(),
    )
    .unwrap();
    for absent in [
        "query",
        "query_lease",
        "generation",
        "generator",
        "unit_state",
    ] {
        assert!(witness.get(absent).is_none());
    }
}

#[test]
fn invalid_draft_does_not_partially_install_a_configuration() {
    let mut p = profile();
    p.tick_ms = 0;
    let before = p.clone();
    assert!(p
        .initialize_pathfinder_configuration(selected(true, &[], 0), &catalog(&[(p.map_id, 0)]))
        .is_err());
    assert_eq!(p, before);
    assert!(p.pathfinder_configuration().is_none());
}

#[test]
fn replacing_a_draft_selection_changes_its_digest_and_invalidates_old_evidence() {
    let original = owned();
    let witness = original
        .pathfinder_configuration()
        .unwrap()
        .constructor_binding(&original, 2)
        .unwrap();
    let mut replacement = original.clone();
    replacement.pathfinder_configuration = None;
    replacement
        .initialize_pathfinder_configuration(
            selected(true, &[replacement.map_id + 1], 0),
            &catalog(&[(replacement.map_id, 0)]),
        )
        .unwrap();
    assert_ne!(replacement.digest(), original.digest());
    assert!(matches!(
        witness.validate_for(&replacement, 2),
        Err(NativeError::AuthorityBinding)
    ));
    assert!(matches!(
        original
            .pathfinder_configuration()
            .unwrap()
            .constructor_binding(&replacement, 2),
        Err(NativeError::AuthorityBinding)
    ));
    witness.validate_for(&original, 2).unwrap();
}

#[test]
fn frozen_constructor_configuration_keeps_selection_and_error_order() {
    use std::sync::Arc;
    for enabled in [false, true] {
        let mut p = profile();
        p.initialize_pathfinder_configuration(
            selected(enabled, &[], 0),
            &catalog(&[(p.map_id, 0)]),
        )
        .unwrap();
        let frozen = ValidatedRouteModelProfile::new(Arc::new(p.clone())).unwrap();
        let source = p.pathfinder_configuration().unwrap();
        for spawn in [2, u64::MAX] {
            assert_eq!(
                source.constructor_binding_for_validated_profile(&frozen, spawn),
                source.constructor_binding(&p, spawn),
            );
        }
        let mut foreign = source.clone();
        foreign.selected.enabled = !enabled;
        assert_eq!(
            foreign.constructor_binding_for_validated_profile(&frozen, u64::MAX),
            Err(NativeError::AuthorityBinding),
        );
        assert_eq!(
            foreign.constructor_binding(&p, u64::MAX),
            Err(NativeError::AuthorityBinding),
        );
        for invalid in 0..3 {
            let mut changed = p.clone();
            let source = changed.pathfinder_configuration.as_mut().unwrap();
            match invalid {
                0 => source.selected.effective_disabled_maps = EffectiveDisabledMMapSet::Missing,
                1 => source.selected.source_instance_id = 1,
                2 => source.policy = "invalid-configuration",
                _ => unreachable!(),
            }
            assert_eq!(
                ValidatedRouteModelProfile::new(Arc::new(changed.clone())).unwrap_err(),
                changed
                    .pathfinder_configuration()
                    .unwrap()
                    .constructor_binding(&changed, u64::MAX)
                    .unwrap_err(),
            );
        }
    }
}
