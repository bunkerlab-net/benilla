// SPDX-License-Identifier: GPL-3.0-or-later
use super::captured::*;
use super::*;
use crate::starter_init::{ItemDamage, ItemRow, ItemSpell, ItemStat};

fn row(class: u32, subclass: u32, kind: u32) -> CapturedItem {
    let inventory = Item {
        entry: 800001,
        class,
        inventory_type: kind,
        display_id: 800002,
        quality: 1,
        flags: 0,
        extra_flags: 0,
        stackable: 1,
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
        spells: [Spell {
            id: 0,
            trigger: 0,
            charges: 0,
            ppm_rate_bits: 0,
            cooldown: -1,
            category: 0,
            category_cooldown: -1,
        }; 5],
        script_id: 0,
    };
    let placement = ItemRow {
        id: inventory.entry,
        class,
        subclass,
        inventory_type: kind,
        allowable_class: u32::MAX,
        allowable_race: u32::MAX,
        required_level: 1,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_reputation_faction: 0,
        flags: 0,
        unique_max_count: 0,
        stackable: 1,
        bag_family: 0,
        container_slots: 0,
        item_set: 0,
        random_property: 0,
        duration: 0,
        scripted: false,
        stats: [ItemStat { kind: 0, value: 0 }; 10],
        armor: 0,
        resistances: [0; 6],
        block: 0,
        spells: [ItemSpell { id: 0, trigger: 0 }; 5],
        damage: [ItemDamage {
            min_bits: 0,
            max_bits: 0,
            school: 0,
        }; 5],
        delay_ms: 0,
        max_durability: 0,
    };
    CapturedItem {
        inventory,
        placement,
        buy_count: 1,
    }
}
fn audit() -> Audit {
    Audit {
        item_creation_and_loot_scripts: Evidence::Absent,
        equipped_bags: Evidence::Absent,
        bank_and_keyring_items: Evidence::Absent,
        trade_or_temporary_item_loot: Evidence::Absent,
        additional_item_mutations: Evidence::Absent,
    }
}

#[test]
fn authored_classes_admit_storage_without_granting_equipment_or_use() {
    for (class, sub, kind) in [
        (0, 0, 0),
        (2, 10, 17),
        (4, 1, 5),
        (7, 0, 0),
        (12, 0, 0),
        (15, 0, 0),
        (2, 2, 15),
        (4, 6, 14),
    ] {
        let catalog = compile_catalog(vec![row(class, sub, kind)], audit()).unwrap();
        catalog.corpse_carried_ready(800001).unwrap();
        assert_eq!(
            catalog.equip_ready(800001),
            Err(Error::Unsupported("carried_item_equipment_not_implemented"))
        );
        assert_eq!(
            catalog.use_ready(800001),
            Err(Error::Unsupported("carried_item_use_not_implemented"))
        );
        let mut state = State::new(BTreeMap::new(), 1).unwrap();
        state
            .store_corpse_item(&catalog, 800001, 1, |_, _, _| Ok(()))
            .unwrap();
        state.validate(&catalog).unwrap();
        assert_eq!(state.slots[&23].charges, [0; 5]);
        assert_eq!(
            catalog.item(800001).unwrap().spells[4].category_cooldown,
            -1
        );
    }
}

#[test]
fn empty_bag_food_binding_and_unique_counts_use_existing_storage() {
    let mut bag = row(1, 0, 18);
    bag.inventory.container_slots = 6;
    bag.placement.container_slots = 6;
    let catalog = compile_catalog(vec![bag], audit()).unwrap();
    let mut state = State::new(BTreeMap::new(), 1).unwrap();
    state
        .store_corpse_item(&catalog, 800001, 1, |_, _, _| Ok(()))
        .unwrap();
    assert_eq!(state.slots[&23].kind, InstanceKind::EmptyBag { slots: 6 });
    let mut food = row(0, 0, 0);
    food.inventory.spells[0].id = 800003;
    food.inventory.spells[0].charges = -2;
    food.placement.spells[0].id = 800003;
    food.inventory.bonding = 4;
    food.inventory.maximum_count = 1;
    food.placement.unique_max_count = 1;
    let catalog = compile_catalog(vec![food], audit()).unwrap();
    let mut state = State::new(BTreeMap::new(), 1).unwrap();
    state
        .store_corpse_item(&catalog, 800001, 1, |_, _, _| Ok(()))
        .unwrap();
    assert!(state.slots[&23].bound);
    assert_eq!(state.slots[&23].charges[0], -2);
    assert_eq!(
        state
            .can_store_corpse_item(&catalog, 800001, 1)
            .unwrap()
            .result,
        InventoryResult::CantCarryMoreOfThis
    );
}

#[test]
fn precise_refusals_preserve_loader_and_lifecycle_guards() {
    let mut quest = row(12, 0, 0);
    quest.inventory.start_quest = 800004;
    assert_eq!(quest.compile_carried().unwrap_err().field, "startquest");
    let mut bad = row(2, 21, 17);
    assert_eq!(
        bad.clone().compile_carried().unwrap_err().reason,
        "prototype_loader_repair_required"
    );
    bad.placement.subclass = 10;
    bad.placement.stackable = 20;
    assert_eq!(
        bad.compile_carried().unwrap_err().reason,
        "inventory_placement_mismatch"
    );
    let mut proc = row(2, 10, 17);
    proc.inventory.spells[0].id = 800003;
    proc.inventory.spells[0].trigger = 2;
    proc.placement.spells[0] = ItemSpell {
        id: 800003,
        trigger: 2,
    };
    assert_eq!(
        proc.compile_carried().unwrap_err().reason,
        "carried_equip_proc_spell_unsupported"
    );
    let mut unknown = audit();
    unknown.item_creation_and_loot_scripts = Evidence::Unknown;
    assert_eq!(
        compile_catalog(vec![row(15, 0, 0)], unknown).unwrap_err()[0].field,
        "audit"
    );
    assert_eq!(
        compile_catalog(vec![row(15, 0, 0), row(15, 0, 0)], audit()).unwrap_err()[0].reason,
        "duplicate_item"
    );
}

#[test]
fn starter_projection_retains_actual_eligibility_and_rejects_unsupported_classes() {
    let mut armor = row(4, 1, 5);
    armor.placement.required_level = 50;
    armor.placement.allowable_class = 128;
    armor.placement.required_skill = 415;
    armor.placement.required_skill_rank = 200;
    let starter = armor.compile_starting().unwrap();
    assert_eq!(starter.item.required_level, 50);
    assert_eq!(starter.item.required_skill_rank, 200);
    assert_eq!(starter.first_spell_category, 0);
    assert_eq!(
        row(12, 0, 0).compile_starting().unwrap_err().reason,
        "starting_placement_unsupported"
    );
}

#[test]
fn playercast_storage_preserves_flags_without_authorizing_use() {
    let mut hearthstone = row(15, 0, 0);
    hearthstone.inventory.entry = 6948;
    hearthstone.placement.id = 6948;
    hearthstone.inventory.flags = 0x40;
    hearthstone.placement.flags = 0x40;
    hearthstone.inventory.spells[0].id = 8690;
    hearthstone.placement.spells[0].id = 8690;
    let starter = hearthstone.clone().compile_starting().unwrap();
    assert_eq!(starter.item.flags, 0x40);
    let catalog = compile_catalog(vec![hearthstone.clone()], audit()).unwrap();
    assert_eq!(catalog.grant_ready(6948).unwrap().flags, 0x40);
    assert_eq!(
        catalog.use_ready(6948),
        Err(Error::Unsupported("carried_item_use_not_implemented"))
    );
    hearthstone.inventory.flags |= 1;
    hearthstone.placement.flags |= 1;
    assert_eq!(hearthstone.compile_starting().unwrap_err().field, "Flags");
}

#[test]
fn captured_catalog_rejects_more_than_the_source_item_bound() {
    let rows = vec![row(15, 0, 0); 2049];
    assert_eq!(
        compile_catalog(rows, audit()).unwrap_err()[0].reason,
        "item_row_limit"
    );
}
