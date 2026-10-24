// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic source rows and arithmetic, not game data or a native admission token.
use super::*;

fn row() -> ItemTemplateRow {
    ItemTemplateRow {
        entry: 501,
        display_id: 701,
        class: 2,
        subclass: 7,
        material: -1,
        inventory_type: 13,
        sheath: 5,
        delay_ms: 0,
        damage: [ItemDamage {
            minimum: SpellFloat::ZERO,
            maximum: SpellFloat::ZERO,
            school: 0,
        }; 5],
    }
}
fn equipped() -> Input {
    let mut i = input();
    i.template.equipment_template_id = 17;
    i.equipment_templates = Rows::Complete(vec![EquipmentTemplateRow {
        entry: 17,
        item_entries: [501, 0, 0],
    }]);
    i.item_templates = Rows::Complete(vec![row()]);
    i
}
fn rows_mut<T>(rows: &mut Rows<T>) -> &mut Vec<T> {
    match rows {
        Rows::Complete(rows) => rows,
        Rows::Missing => panic!("expected complete test rows"),
    }
}

#[test]
fn raw_item_packing_and_item_damage_delay_do_not_replace_creature_stats() {
    let i = equipped();
    let out = initialize(&i);
    assert_eq!(
        out.equipment,
        InitialEquipment {
            equipment_template_id: 17,
            mainhand: Some(InitialVirtualItem {
                item_entry: 501,
                display_id: 701,
                info: [0x0dff_0702, 5],
                has_weapon: true
            }),
        }
    );
    assert_eq!(out.stats, initialize(&input()).stats);
    assert_eq!(out.mainhand_interval_ms, 2300);
    assert_eq!(out.unit_bytes_2, 0x1001);
    let mut changed = i.clone();
    let item = &mut rows_mut(&mut changed.item_templates)[0];
    item.delay_ms = u32::MAX;
    item.damage = [ItemDamage {
        minimum: f(42.),
        maximum: f(-3.),
        school: 6,
    }; 5];
    assert_eq!(initialize(&changed), out);
    assert_ne!(
        serde_json::to_vec(&i).unwrap(),
        serde_json::to_vec(&changed).unwrap()
    );
}

#[test]
fn spawn_selection_inherits_overrides_or_skips_fresh_equipment_exactly() {
    let mut i = equipped();
    assert!(initialize(&i).equipment.mainhand.is_some());
    spawn(&mut i).equipment_id = 0;
    assert_eq!(
        initialize(&i).equipment,
        InitialEquipment {
            equipment_template_id: 0,
            mainhand: None
        }
    );
    rows_mut(&mut i.equipment_templates).push(EquipmentTemplateRow {
        entry: 18,
        item_entries: [0; 3],
    });
    spawn(&mut i).equipment_id = 18;
    assert_eq!(
        initialize(&i).equipment,
        InitialEquipment {
            equipment_template_id: 18,
            mainhand: None
        }
    );
    i.template.equipment_template_id = 0;
    spawn(&mut i).equipment_id = -1;
    rows_mut(&mut i.equipment_templates).push(EquipmentTemplateRow {
        entry: 0,
        item_entries: [501, 0, 0],
    });
    assert_eq!(
        initialize(&i).equipment,
        InitialEquipment {
            equipment_template_id: 0,
            mainhand: None
        }
    );
    spawn(&mut i).equipment_id = 17;
    assert_eq!(initialize(&i).equipment.mainhand.unwrap().item_entry, 501);
}

#[test]
fn raw_template_reference_still_validates_under_a_zero_spawn_override() {
    let mut i = equipped();
    spawn(&mut i).equipment_id = 0;
    i.equipment_templates = Rows::Complete(vec![]);
    assert!(matches!(
        prepare(&i),
        Err(InitError::Missing {
            table: "creature_equip_template template reference",
            id: 17
        })
    ));
    i.template.equipment_template_id = 0;
    spawn(&mut i).equipment_id = 19;
    assert!(matches!(
        prepare(&i),
        Err(InitError::Missing {
            table: "creature_equip_template selected reference",
            id: 19
        })
    ));
}

#[test]
fn static_disarm_uses_effective_unit_flags_and_class_even_for_zero_display() {
    let mut i = equipped();
    let normal = initialize(&i);
    i.template.unit_flags = 0x0020_0000;
    let disarmed = initialize(&i);
    assert_eq!(
        disarmed.stats.damage.attack_power,
        normal.stats.damage.attack_power
    );
    assert_eq!(
        disarmed.stats.damage.minimum,
        f(f32::from_bits(normal.stats.damage.minimum.bits()) * 0.5)
    );
    assert_eq!(
        disarmed.stats.damage.maximum,
        f(f32::from_bits(normal.stats.damage.maximum.bits()) * 0.5)
    );
    assert_eq!(
        disarmed.stats.base_weapon_minimum,
        normal.stats.base_weapon_minimum
    );
    assert_eq!(disarmed.mainhand_interval_ms, normal.mainhand_interval_ms);
    rows_mut(&mut i.item_templates)[0].display_id = 0;
    assert_eq!(initialize(&i).stats.damage, disarmed.stats.damage);
    assert!(initialize(&i).equipment.mainhand.unwrap().has_weapon);
    spawn(&mut i).unit_flags = 0;
    assert_eq!(initialize(&i).stats.damage, normal.stats.damage);
    i.template.unit_flags = 0;
    spawn(&mut i).unit_flags = 0x0020_0000;
    assert_eq!(initialize(&i).stats.damage, disarmed.stats.damage);
    rows_mut(&mut i.item_templates)[0].class = 4;
    assert!(!initialize(&i).equipment.mainhand.unwrap().has_weapon);
    assert_eq!(initialize(&i).stats.damage, normal.stats.damage);
    spawn(&mut i).equipment_id = 0;
    assert_eq!(initialize(&i).stats.damage, normal.stats.damage);
}

#[test]
fn all_native_hand_inventory_types_and_signed_material_bytes_are_preserved() {
    for inventory_type in [13, 14, 15, 17, 21, 22, 23, 25, 26, 28] {
        let mut i = equipped();
        rows_mut(&mut i.item_templates)[0].inventory_type = inventory_type;
        assert_eq!(
            initialize(&i).equipment.mainhand.unwrap().info[0] >> 24,
            inventory_type
        );
    }
    for inventory_type in [0, 1, 12, 16, 18, 24, 27, 29, u32::MAX] {
        let mut i = equipped();
        rows_mut(&mut i.item_templates)[0].inventory_type = inventory_type;
        assert!(matches!(prepare(&i), Err(InitError::Unsupported(_))));
    }
    for material in [-128, -1, 0, 127] {
        let mut i = equipped();
        rows_mut(&mut i.item_templates)[0].material = material;
        assert_eq!(
            (initialize(&i).equipment.mainhand.unwrap().info[0] >> 16) & 255,
            material as u32 & 255
        );
    }
}

#[test]
fn missing_duplicate_and_loader_repair_rows_fail_before_draws() {
    let mut i = input();
    i.equipment_templates = Rows::Missing;
    assert!(matches!(prepare(&i), Err(InitError::Missing { .. })));
    let mut i = input();
    i.item_templates = Rows::Missing;
    assert!(matches!(prepare(&i), Err(InitError::Missing { .. })));
    let mut i = equipped();
    i.item_templates = Rows::Complete(vec![]);
    assert!(matches!(
        prepare(&i),
        Err(InitError::Missing { id: 501, .. })
    ));
    let mut i = equipped();
    rows_mut(&mut i.item_templates).push(row());
    assert!(matches!(
        prepare(&i),
        Err(InitError::Invalid("duplicate equipment item row"))
    ));
    let mut i = equipped();
    let duplicate = rows_mut(&mut i.equipment_templates)[0];
    rows_mut(&mut i.equipment_templates).push(duplicate);
    assert!(matches!(
        prepare(&i),
        Err(InitError::Invalid("duplicate equipment template row"))
    ));
    for mutation in 0..6 {
        let mut i = equipped();
        let item = &mut rows_mut(&mut i.item_templates)[0];
        match mutation {
            0 => item.class = 16,
            1 => item.subclass = 21,
            2 => item.sheath = 8,
            3 => item.material = -129,
            4 => item.material = 128,
            _ => item.damage[4].school = 7,
        }
        assert!(prepare(&i).is_err());
    }
}

#[test]
fn unselected_closure_is_validated_selected_offhand_ranged_and_forced_dual_wield_refuse() {
    let mut i = equipped();
    let baseline = initialize(&i);
    rows_mut(&mut i.equipment_templates).push(EquipmentTemplateRow {
        entry: 20,
        item_entries: [501; 3],
    });
    assert_eq!(initialize(&i), baseline);
    spawn(&mut i).equipment_id = 20;
    assert!(matches!(
        prepare(&i),
        Err(InitError::Unsupported(
            "offhand or ranged initial equipment"
        ))
    ));
    spawn(&mut i).equipment_id = 17;
    rows_mut(&mut i.equipment_templates)[1].item_entries[2] = 999;
    assert!(matches!(
        prepare(&i),
        Err(InitError::Missing { id: 999, .. })
    ));
    let mut i = equipped();
    i.template.extra_flags = 0x0080_0000;
    assert!(matches!(
        prepare(&i),
        Err(InitError::Unsupported(
            "forced dual-wield initial equipment"
        ))
    ));
    for slot in [1, 2] {
        let mut i = equipped();
        rows_mut(&mut i.equipment_templates)[0].item_entries[slot] = 501;
        assert!(matches!(
            prepare(&i),
            Err(InitError::Unsupported(
                "offhand or ranged initial equipment"
            ))
        ));
    }
}

#[test]
fn disarm_does_not_hide_an_out_of_domain_unhalved_transient() {
    let mut i = equipped();
    i.template.damage_multiplier = f(150_000.);
    if let Rows::Complete(rows) = &mut i.class_levels {
        for row in rows {
            row.base_melee_attack_power = SpellFloat::ZERO;
        }
    }
    assert!(matches!(prepare(&i), Err(InitError::Numeric(_))));
    i.template.unit_flags = 0x0020_0000;
    assert!(matches!(prepare(&i), Err(InitError::Numeric(_))));
}

#[test]
fn fixed_width_equipment_budgets_fail_before_other_validation() {
    let mut i = input();
    i.template.entry = 0;
    i.equipment_templates = Rows::Complete(vec![
        EquipmentTemplateRow {
            entry: 0,
            item_entries: [0; 3]
        };
        MAX_CREATURE_INIT_SOURCE_ROWS + 1
    ]);
    assert_eq!(
        i.validate_retention_bounds(),
        Err(InitError::Invalid("source retention row bound"))
    );
    assert!(matches!(
        prepare(&i),
        Err(InitError::Invalid("source retention row bound"))
    ));
    i.equipment_templates = Rows::Complete(vec![]);
    i.item_templates = Rows::Complete(vec![row(); 3000]);
    assert_eq!(
        i.validate_retention_bounds(),
        Err(InitError::Invalid("source retention byte bound"))
    );
    assert!(matches!(
        prepare(&i),
        Err(InitError::Invalid("source retention byte bound"))
    ));
}
