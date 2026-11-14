// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored loader witnesses and items, not copied world rows.
use super::*;
use benilla_sim::{inventory as inv, loot::captured::*};

fn item(entry: u32) -> inv::Item {
    inv::Item {
        entry,
        class: 15,
        inventory_type: 0,
        display_id: entry + 100,
        quality: 1,
        flags: 0,
        extra_flags: 0,
        stackable: 20,
        maximum_count: 0,
        bonding: 0,
        maximum_durability: 0,
        bag_family: 0,
        container_slots: 0,
        random_property: 0,
        start_quest: 0,
        duration: 0,
        item_set: 0,
        area: 0,
        map: 0,
        lock_id: 0,
        spells: [inv::Spell {
            id: 0,
            trigger: 0,
            charges: 0,
            ppm_rate_bits: 0,
            cooldown: 0,
            category: 0,
            category_cooldown: 0,
        }; 5],
        script_id: 0,
    }
}
fn inventory(items: Vec<inv::Item>) -> inv::Catalog {
    inv::Catalog::new(
        items,
        inv::Audit {
            item_creation_and_loot_scripts: inv::Evidence::Absent,
            equipped_bags: inv::Evidence::Absent,
            bank_and_keyring_items: inv::Evidence::Absent,
            trade_or_temporary_item_loot: inv::Evidence::Absent,
            additional_item_mutations: inv::Evidence::Absent,
        },
    )
    .unwrap()
}
fn table(source: Vec<(u32, Row)>) -> Table {
    let selected_positions = source
        .iter()
        .enumerate()
        .map(|(i, (entry, row))| SelectedPosition {
            position: i as u64 * 2,
            entry: *entry,
            item: row.item,
        })
        .collect();
    let result_row_count = source.len() as u64 * 2;
    let mut rows = source
        .into_iter()
        .map(|(entry, row)| TemplateRow { entry, row })
        .collect::<Vec<_>>();
    rows.sort_by_key(|r| (r.entry, r.row.item));
    Table {
        source_fingerprint: [1; 32],
        ordered_result_fingerprint: [2; 32],
        result_row_count,
        rows,
        selected_positions,
    }
}
fn reference_row(item: u32, chance: f32) -> Row {
    Row {
        item,
        ..reference(item as i32, chance, 1)
    }
}
fn input() -> CapturedLoot {
    CapturedLoot {
        body: Some(table(vec![
            (6, reference_row(60000, 9.)),
            (6, reference_row(60441, 0.66)),
        ])),
        references: Some(table(vec![
            (60000, ordinary(80002, 0., 1)),
            (60000, ordinary(80001, 0., 1)),
            (60441, ordinary(80004, 30., 1)),
            (60441, ordinary(80003, 20., 1)),
            (60441, ordinary(80005, 0., 1)),
        ])),
    }
}
fn replay(draws: Vec<String>) -> Replay {
    Replay {
        draws: draws.into(),
    }
}
fn chance(value: f64) -> String {
    format!("c:{}", value.to_bits())
}
#[test]
fn captured_reference_thresholds_groups_and_money_have_exact_draw_order() {
    let inv = inventory((80001..=80005).map(item).collect());
    let catalog = input().compile(&inv).unwrap();
    let mut rng = replay(vec![
        chance(8.999),
        "s:0,1:0,1".into(),
        "u:1:1:1".into(),
        chance(0.659),
        "s:0,1:0,1".into(),
        chance(35.),
        "u:1:1:1".into(),
        "u:2:5:5".into(),
    ]);
    let out = catalog
        .generate(6, 2, 5, &unit_rules(), &OwnerQuestItems::Unknown, &mut rng)
        .unwrap();
    assert_eq!(
        out.items.iter().map(|i| i.item).collect::<Vec<_>>(),
        [80002, 80003]
    );
    assert_eq!(out.money, 5);
    assert!(rng.draws.is_empty());
    let mut rng = replay(vec![
        chance(9.),
        chance(f64::from(0.66f32)),
        "u:2:5:2".into(),
    ]);
    let out = catalog
        .generate(6, 2, 5, &unit_rules(), &OwnerQuestItems::Unknown, &mut rng)
        .unwrap();
    assert!(out.items.is_empty());
    assert_eq!(out.money, 2);
    assert!(rng.draws.is_empty());
    let mut rng = replay(vec![
        chance(99.),
        chance(0.),
        "s:0,1:1,0".into(),
        chance(70.),
        "s:2:2".into(),
        "u:1:1:1".into(),
    ]);
    let out = catalog
        .generate(6, 0, 0, &unit_rules(), &OwnerQuestItems::Unknown, &mut rng)
        .unwrap();
    assert_eq!(out.items[0].item, 80005);
    assert!(rng.draws.is_empty());
}
#[test]
fn captured_order_not_item_key_order_determines_equal_group_result() {
    let inv = inventory((80001..=80005).map(item).collect());
    let first = input().compile(&inv).unwrap();
    let mut changed = input();
    changed
        .references
        .as_mut()
        .unwrap()
        .selected_positions
        .swap(0, 1);
    let positions = &mut changed.references.as_mut().unwrap().selected_positions;
    positions[0].position = 0;
    positions[1].position = 2;
    let second = changed.compile(&inv).unwrap();
    assert_ne!(first.identity(), second.identity());
    for (catalog, expected) in [(first, 80002), (second, 80001)] {
        let mut rng = replay(vec![
            chance(0.),
            "s:0,1:0,1".into(),
            "u:1:1:1".into(),
            chance(99.),
        ]);
        let out = catalog
            .generate(6, 0, 0, &unit_rules(), &OwnerQuestItems::Unknown, &mut rng)
            .unwrap();
        assert_eq!(out.items[0].item, expected);
        assert!(rng.draws.is_empty());
    }
}
#[test]
fn every_possible_leaf_requires_inventory_admission_even_if_reference_rarely_drops() {
    for field in 0..8 {
        let mut items = (80001..=80005).map(item).collect::<Vec<_>>();
        match field {
            0 => {
                items.pop();
            }
            1 => items[4].start_quest = 99,
            2 => items[4].random_property = 99,
            3 => items[4].script_id = 99,
            4 => items[4].bag_family = 1,
            5 => items[4].spells[0].id = 99,
            6 => items[4].duration = 99,
            _ => items[4].lock_id = 99,
        }
        if field == 5 {
            items[4].spells[0].trigger = 1;
        }
        let error = input().compile(&inventory(items)).unwrap_err();
        assert_eq!(
            (error.table, error.entry, error.item),
            ("reference_loot_template", 60441, 80005)
        );
    }
    let mut items = (80001..=80005).map(item).collect::<Vec<_>>();
    items[4].class = 1;
    items[4].inventory_type = 18;
    items[4].container_slots = 6;
    items[4].stackable = 1;
    assert!(input().compile(&inventory(items)).is_ok());
}
#[test]
fn unknown_duplicate_incomplete_and_nonmonotonic_loader_evidence_refuses() {
    let inv = inventory((80001..=80005).map(item).collect());
    for fault in 0..6 {
        let mut data = input();
        match fault {
            0 => data.body = None,
            1 => data.references.as_mut().unwrap().selected_positions.clear(),
            2 => data.body.as_mut().unwrap().selected_positions[1].position = 0,
            3 => data.body.as_mut().unwrap().ordered_result_fingerprint = [0; 32],
            4 => data.body.as_mut().unwrap().selected_positions[1].item = 33,
            _ => data.body.as_mut().unwrap().rows[1].row.condition_id = 1,
        }
        assert!(data.compile(&inv).is_err(), "fault {fault}");
    }
}
