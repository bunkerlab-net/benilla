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
fn unsupported_addon_payloads_fail_even_in_overridden_template_rows() {
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
            } else {
                template_addon(&mut i).addon = rejected.clone();
            }
            assert!(matches!(prepare(&i), Err(InitError::Unsupported(_))));
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
