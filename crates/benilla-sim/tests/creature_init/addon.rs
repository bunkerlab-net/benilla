// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic addon-source identities and payloads, never native database rows.
use super::*;

fn addon(sheath_state: u32) -> Addon {
    Addon {
        mount: 0,
        stand_state: 0,
        sheath_state,
        emote: 0,
        move_flags: 0,
        auras: vec![],
    }
}
fn with_addons(spawn_sheath: Option<u32>, template_sheath: Option<u32>) -> Input {
    let mut i = input();
    i.spawn_addons = Rows::Complete(
        spawn_sheath
            .map(|sheath| SpawnAddonRow {
                guid: i.spawn_id,
                addon: addon(sheath),
            })
            .into_iter()
            .collect(),
    );
    i.template_addons = Rows::Complete(
        template_sheath
            .map(|sheath| TemplateAddonRow {
                entry: i.template.entry,
                addon: addon(sheath),
            })
            .into_iter()
            .collect(),
    );
    i
}
fn spawn_addon(i: &mut Input) -> &mut SpawnAddonRow {
    match &mut i.spawn_addons {
        Rows::Complete(rows) => &mut rows[0],
        Rows::Missing => unreachable!(),
    }
}
fn template_addon(i: &mut Input) -> &mut TemplateAddonRow {
    match &mut i.template_addons {
        Rows::Complete(rows) => &mut rows[0],
        Rows::Missing => unreachable!(),
    }
}

#[test]
fn complete_absence_and_all_sheaths_preserve_other_creation_fields_and_draws() {
    let baseline_input = input();
    let baseline = initialize(&baseline_input);
    assert_eq!(baseline.unit_bytes_2, 0x1001);
    for spawn_sheath in [None, Some(0), Some(1), Some(2)] {
        for template_sheath in [None, Some(0), Some(1), Some(2)] {
            let i = with_addons(spawn_sheath, template_sheath);
            let plan = prepare(&i).unwrap();
            assert_eq!(plan.model_roll_range(), Some((0, 5)));
            assert_eq!(plan.gender_roll_range(Some(0)).unwrap(), Some((0, 1)));
            assert_eq!(plan.level_roll_range(), Some((1, 20)));
            let mut expected = baseline.clone();
            expected.unit_bytes_2 = 0x1000 | spawn_sheath.or(template_sheath).unwrap_or(1);
            assert_eq!(initialize(&i), expected);
            assert_eq!(model_receipt(&i), model_receipt(&baseline_input));
        }
    }
}

#[test]
fn missing_addon_queries_cannot_be_replaced_by_empty_or_overriding_rows() {
    for source in [0, 1] {
        let mut i = with_addons(Some(0), Some(2));
        if source == 0 {
            i.spawn_addons = Rows::Missing;
        } else {
            i.template_addons = Rows::Missing;
        }
        assert!(matches!(prepare(&i), Err(InitError::Missing { .. })));
    }
}

#[test]
fn exact_addon_query_ownership_and_duplicate_keys_are_required() {
    for guid in [0, u64::from(u32::MAX) + 1, u64::MAX] {
        let mut i = input();
        i.spawn_id = guid;
        assert!(matches!(
            prepare(&i),
            Err(InitError::Invalid("addon spawn database GUID"))
        ));
    }
    for wrong in [0, 92, u64::MAX] {
        let mut i = with_addons(Some(0), Some(2));
        spawn_addon(&mut i).guid = wrong;
        assert!(matches!(
            prepare(&i),
            Err(InitError::Invalid("addon spawn row identity"))
        ));
    }
    for wrong in [0, 98, u32::MAX] {
        let mut i = with_addons(Some(0), Some(2));
        template_addon(&mut i).entry = wrong;
        assert!(matches!(
            prepare(&i),
            Err(InitError::Invalid("addon template row identity"))
        ));
    }
    for duplicate_spawn in [false, true] {
        let mut i = with_addons(Some(0), Some(2));
        if duplicate_spawn {
            let row = spawn_addon(&mut i).clone();
            i.spawn_addons = Rows::Complete(vec![row.clone(), row]);
        } else {
            let row = template_addon(&mut i).clone();
            i.template_addons = Rows::Complete(vec![row.clone(), row]);
        }
        assert!(matches!(
            prepare(&i),
            Err(InitError::Invalid("duplicate addon row"))
        ));
    }
}

#[test]
fn unsupported_effective_payloads_and_unknown_shadowed_lookups_refuse() {
    let mut rejected = vec![];
    for value in [1, 256, u32::MAX] {
        rejected.push(Addon {
            mount: value,
            ..addon(1)
        });
        rejected.push(Addon {
            stand_state: value,
            ..addon(1)
        });
        rejected.push(Addon {
            emote: value,
            ..addon(1)
        });
    }
    for sheath in [3, 255, 256, u32::MAX] {
        rejected.push(addon(sheath));
    }
    for auras in [vec![0], vec![17], vec![17, 17], vec![u32::MAX]] {
        rejected.push(Addon { auras, ..addon(1) });
    }
    for rejected in rejected {
        for is_spawn in [false, true] {
            let mut i = with_addons(Some(0), Some(2));
            if is_spawn {
                spawn_addon(&mut i).addon = rejected.clone();
                assert!(matches!(prepare(&i), Err(InitError::Unsupported(_))));
            } else {
                template_addon(&mut i).addon = rejected.clone();
                if rejected.stand_state != 0 {
                    assert_eq!(initialize(&i).unit_bytes_2, 0x1000);
                } else {
                    assert!(matches!(prepare(&i), Err(InitError::Unsupported(_))));
                }
            }
        }
    }
}

#[test]
fn unretained_addon_writers_remain_unsupported() {
    let mut i = with_addons(Some(0), Some(2));
    i.external_features = Rows::Complete(vec![ExternalFeature::Addon]);
    assert!(matches!(
        prepare(&i),
        Err(InitError::Unsupported(
            "applicable external initialization feature"
        ))
    ));
}

#[test]
fn ignored_move_flags_are_retained_without_movement_or_rng_effects() {
    let mut i = with_addons(Some(0), Some(2));
    let expected = initialize(&i);
    let before = serde_json::to_vec(&i).unwrap();
    spawn_addon(&mut i).addon.move_flags = u32::MAX;
    template_addon(&mut i).addon.move_flags = 0xaabb_ccdd;
    assert_ne!(serde_json::to_vec(&i).unwrap(), before);
    assert_eq!(initialize(&i), expected);
}

#[test]
fn authored_peasant_and_guard_whole_row_overrides_validate_before_selection() {
    for (entry, guid, emote, auras) in [(11260, 80119, 234, vec![]), (1642, 79927, 0, vec![18950])]
    {
        let mut i = with_addons(Some(0), Some(2));
        i.spawn_id = guid;
        i.template.entry = entry;
        spawn_addon(&mut i).guid = guid;
        template_addon(&mut i).entry = entry;
        template_addon(&mut i).addon.emote = emote;
        template_addon(&mut i).addon.auras = auras;
        i.addon_loader = Some(AddonLoaderInputs {
            mount_displays: vec![],
            emotes: vec![234],
            spells: vec![AddonSpellLookup {
                id: 18950,
                effects: [6, 6, 0],
                targets_a: [1, 1, 0],
                targets_b: [0; 3],
                cast_time_ms: Some(0),
            }],
        });
        let result = initialize(&i);
        assert_eq!(result.unit_bytes_2, 0x1000);
        let mut effective = i.clone();
        effective.spawn_addons = Rows::Complete(vec![]);
        assert!(matches!(
            prepare(&effective),
            Err(InitError::Unsupported(_))
        ));
        let mut invalid = i.clone();
        invalid.addon_loader.as_mut().unwrap().emotes.clear();
        invalid.addon_loader.as_mut().unwrap().spells.clear();
        assert!(prepare(&invalid).is_err());
        assert_eq!(initialize(&i), result);
    }
}

#[test]
fn loader_invalid_shadowed_auras_and_unknown_strict_outcomes_refuse() {
    let mut i = with_addons(Some(0), Some(2));
    template_addon(&mut i).addon.auras = vec![18950];
    i.addon_loader = Some(AddonLoaderInputs {
        mount_displays: vec![],
        emotes: vec![],
        spells: vec![AddonSpellLookup {
            id: 18950,
            effects: [6, 0, 0],
            targets_a: [1, 0, 0],
            targets_b: [0; 3],
            cast_time_ms: None,
        }],
    });
    assert!(prepare(&i).is_ok());
    for case in 0..6 {
        let mut invalid = i.clone();
        let spell = &mut invalid.addon_loader.as_mut().unwrap().spells[0];
        match case {
            0 => spell.effects = [2, 0, 0],
            1 => spell.cast_time_ms = Some(1),
            2 => spell.targets_a[0] = 6,
            3 => spell.targets_b[0] = 6,
            4 => template_addon(&mut invalid).addon.auras.push(18950),
            _ => template_addon(&mut invalid).addon.auras = vec![0],
        }
        assert!(prepare(&invalid).is_err(), "case {case}");
    }
    let mut effective = i.clone();
    spawn_addon(&mut effective).addon.auras = vec![18950];
    assert!(matches!(
        prepare(&effective),
        Err(InitError::Unsupported("initial addon auras"))
    ));
}
