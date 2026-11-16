// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn row(id: u32, slots: [u32; 3]) -> LoadedEquipmentRow {
    SqlTemplateRow {
        ordinal: u64::from(id),
        words: [id, slots[0], slots[1], slots[2]],
        strings: [],
    }
}
fn item(inventory_type: u32) -> EquipmentItem {
    EquipmentItem {
        display: 1234,
        class: 2,
        subclass: 7,
        material: u32::MAX,
        inventory_type,
        sheath: 3,
    }
}
fn templates() -> LoadedEquipmentTemplates {
    LoadedEquipmentTemplates::load(
        vec![row(1, [10, 0, 0]), row(2, [0, 10, 0])],
        BTreeMap::from([(10, item(13))]),
    )
    .unwrap()
}
#[test]
fn equipment_loader_fixes_slots_without_skipping_rows_in_source_order() {
    let loaded = LoadedEquipmentTemplates::load(
        vec![row(0, [99, 11, 0]), row(3, [10, 12, 11])],
        BTreeMap::from([(10, item(13)), (11, item(1)), (12, item(29))]),
    )
    .unwrap();
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded.get(0), Some(&[0, 0, 0]));
    assert_eq!(loaded.get(3), Some(&[10, 0, 0]));
    assert_eq!(
        loaded.notices,
        [(0, 0, 1580), (0, 1, 1596), (3, 1, 1596), (3, 2, 1596)]
    );
    assert_eq!(loaded.item_fixups, [0, 0, 1, 0]);
}
#[test]
fn equipment_loader_accepts_every_source_hand_inventory_type() {
    let types = [13, 14, 15, 17, 21, 22, 23, 25, 26, 28];
    let items = types.into_iter().map(|id| (id, item(id))).collect();
    let rows = types.into_iter().map(|id| row(id, [id; 3])).collect();
    let loaded = LoadedEquipmentTemplates::load(rows, items).unwrap();
    assert!(loaded.notices.is_empty());
    assert_eq!(loaded.len(), 10);
    assert!(LoadedEquipmentTemplates::load(Vec::new(), BTreeMap::new())
        .unwrap()
        .is_empty());
}
#[test]
fn equipment_item_fixups_use_the_already_corrected_class_and_skip_id_zero() {
    let mut bad = item(29);
    bad.class = 16;
    bad.subclass = 6;
    bad.sheath = 8;
    let loaded =
        LoadedEquipmentTemplates::load(Vec::new(), BTreeMap::from([(0, bad), (1, bad)])).unwrap();
    assert_eq!(loaded.item_fixups, [1, 1, 1, 1]);
    assert_eq!(loaded.items[&0], bad);
    assert_eq!(
        loaded.items[&1],
        EquipmentItem {
            class: 15,
            subclass: 0,
            inventory_type: 0,
            sheath: 0,
            ..bad
        }
    );
    let mut fixed_class = bad;
    fixed_class.subclass = 5;
    let loaded =
        LoadedEquipmentTemplates::load(Vec::new(), BTreeMap::from([(1, fixed_class)])).unwrap();
    assert_eq!(loaded.items[&1].subclass, 5);
}
#[test]
fn load_equipment_zero_force_missing_and_present_preserve_source_writes() {
    let templates = templates();
    let mut state = SourceCreatureEquipment::default();
    state.load(1, false, &templates);
    assert_eq!(state.id, 1);
    assert_eq!(
        state.slots[0],
        SourceVirtualItem {
            display: 1234,
            info: [0x0dff0702, 3]
        }
    );
    let equipped = state.clone();
    state.load(0, false, &templates);
    assert_eq!(state, equipped);
    state.load(999, true, &templates);
    assert_eq!(state, equipped);
    state.load(0, true, &templates);
    assert_eq!(state, SourceCreatureEquipment::default());
}
#[test]
fn virtual_item_zero_clears_before_lookup_and_missing_item_leaves_slot_unchanged() {
    let templates = templates();
    let mut state = SourceCreatureEquipment::default();
    state.slots[0].info[1] = 0xaabbccdd;
    state.set_virtual_item(0, 10, &templates);
    assert_eq!(state.slots[0].info, [0x0dff0702, 0xaabbcc03]);
    let old = state.slots[0];
    state.set_virtual_item(0, 999, &templates);
    assert_eq!(state.slots[0], old);
    assert_eq!(state.missing_items, [999]);
    state.set_virtual_item(0, 0, &templates);
    assert_eq!(state.slots[0], SourceVirtualItem::default());
}
#[test]
fn init_entry_equipment_selects_every_event_spawn_and_default_branch() {
    use CreatureEquipmentBranch::*;
    let cases = [
        (
            Some((1, 9)),
            Some(2),
            2,
            2,
            EventEquipment,
            Some((1, false)),
            false,
        ),
        (
            Some((0, 9)),
            Some(2),
            1,
            2,
            EventEntry,
            Some((2, true)),
            false,
        ),
        (Some((0, 9)), None, 1, 0, EventEntry, Some((0, true)), false),
        (Some((0, 0)), Some(-1), 1, 2, EventUnchanged, None, false),
        (None, None, 1, 0, NormalTemplate, Some((1, true)), true),
        (None, Some(-1), 1, 0, NormalTemplate, Some((1, true)), true),
        (
            None,
            Some(-1),
            1,
            2,
            DifficultyTemplate,
            Some((2, false)),
            true,
        ),
        (None, Some(2), 1, 1, SpawnEquipment, Some((2, false)), false),
        (None, Some(0), 1, 1, SpawnNone, None, false),
        (
            None,
            Some(-2),
            1,
            1,
            SpawnEquipment,
            Some((u32::MAX - 1, false)),
            false,
        ),
    ];
    for (event, spawn, normal, difficulty, branch, call, loot) in cases {
        assert_eq!(
            CreatureEquipmentBranch::select(event, spawn, normal, difficulty),
            (branch, call, loot)
        );
        let mut state = SourceCreatureEquipment::default();
        assert_eq!(
            state
                .init_entry(event, spawn, normal, difficulty, false, &templates())
                .unwrap(),
            branch
        );
        assert_eq!(
            state.id,
            call.filter(|(id, _)| *id == 1 || *id == 2)
                .map_or(0, |c| c.0)
        );
    }
}
#[test]
fn can_wield_loot_refuses_at_prepare_after_loading_only_on_default_branch() {
    let templates = templates();
    let mut state = SourceCreatureEquipment::default();
    assert_eq!(
        state
            .init_entry(None, Some(-1), 0, 1, true, &templates)
            .unwrap_err()
            .id,
        "execution.equipment.Creature_405.PrepareBodyLootState_owner"
    );
    assert_eq!(state.id, 1);
    assert_eq!(state.slots[0].display, 1234);
    for (event, spawn) in [(Some((1, 0)), Some(-1)), (None, Some(1)), (None, Some(0))] {
        state
            .init_entry(event, spawn, 0, 1, true, &templates)
            .unwrap();
    }
}
#[test]
fn equipment_storage_refuses_undefined_maximum_index() {
    assert_eq!(
        LoadedEquipmentTemplates::load(vec![row(u32::MAX, [0; 3])], BTreeMap::new())
            .unwrap_err()
            .id,
        "execution.equipment.SQLStorageImpl_317.max_entry_index_undefined"
    );
    assert_eq!(
        LoadedEquipmentTemplates::load(Vec::new(), BTreeMap::from([(u32::MAX, item(13))]))
            .unwrap_err()
            .id,
        "execution.equipment.item_SQLStorageImpl_317.max_entry_index_undefined"
    );
}
#[test]
fn init_entry_keeps_existing_slots_on_unchanged_event_and_zero_spawn() {
    let templates = templates();
    let mut state = SourceCreatureEquipment::default();
    state.load(1, false, &templates);
    let old = state.clone();
    state
        .init_entry(Some((0, 0)), Some(-1), 0, 0, true, &templates)
        .unwrap();
    assert_eq!(state, old);
    state
        .init_entry(None, Some(0), 0, 0, true, &templates)
        .unwrap();
    assert_eq!(state, old);
    state
        .init_entry(None, None, 0, 0, false, &templates)
        .unwrap();
    assert_eq!(state, SourceCreatureEquipment::default());
}
#[test]
fn stock_equipment_catalog_census_or_explicit_absence() {
    let Ok(path) = std::env::var("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP stock equipment census: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&path), false).unwrap();
    let loaded = LoadedEquipmentTemplates::catalog_diagnostic(&mut catalog).unwrap();
    // pre-start-classicmangos.sql:161413-166730 contains the restored equipment rows.
    assert_eq!(loaded.len(), 5318);
    let mut reasons = BTreeMap::<u32, usize>::new();
    for &(_, _, line) in &loaded.notices {
        *reasons.entry(line).or_default() += 1;
    }
    println!(
        "F6_EQUIPMENT_STOCK loaded={} skipped_rows=0 slot_fixups={reasons:?} item_fixups={:?}",
        loaded.len(),
        loaded.item_fixups
    );
}
