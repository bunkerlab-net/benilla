// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_world_runtime::native::source_execution::source_rows::{
    RespawnKind, RowDelete, RowInsert, RowOperation, RowUpdate,
};
use RowOperation::{Delete, Insert, ReplaceItemInstance, Update};

fn inventory(slot: u8, item_guid: u32, item_template: u32) -> InventoryRow {
    InventoryRow {
        guid: 1,
        bag_guid: 0,
        slot,
        item_guid,
        item_template,
    }
}

fn item(guid: u32, entry: u32, count: u32) -> ItemInstanceRow {
    ItemInstanceRow {
        guid,
        owner_guid: 1,
        entry,
        count,
        creator: 0,
        gift_creator: 0,
        duration: 0,
        charges: [0; 5],
        flags: 0,
        enchantments: [[0; 3]; 7],
        random_property: 0,
        durability: 0,
        text_id: 0,
    }
}

fn create_batch() -> Vec<RowOperation> {
    let mut operations = vec![
        Delete(RowDelete::Character(1)),
        Insert(RowInsert::Character(character())),
    ];
    // Player.cpp:3819-3864 and Item.cpp:250-312: queue order, not slot order.
    for (slot, guid, entry, count) in [
        (4, 2, 56, 1),
        (6, 4, 1395, 1),
        (7, 6, 55, 1),
        (15, 8, 35, 1),
        (3, 10, 6096, 1),
        (23, 12, 2070, 5),
        (24, 14, 159, 5),
        (25, 16, 6948, 1),
    ] {
        operations.push(Insert(RowInsert::Inventory(inventory(slot, guid, entry))));
        operations.push(ReplaceItemInstance(item(guid, entry, count)));
    }
    operations.push(Delete(RowDelete::CooldownsByGuid(1)));
    for (button, action, kind) in [
        (0, 6603, 0),
        (1, 133, 0),
        (2, 168, 0),
        (10, 159, 128),
        (11, 2070, 128),
    ] {
        operations.push(Insert(RowInsert::Action(ActionRow {
            guid: 1,
            button,
            action,
            kind,
        })));
    }
    operations.push(Delete(RowDelete::AurasByGuid(1)));
    for (skill, value, max) in [
        (415, 1, 1),
        (754, 5, 5),
        (237, 5, 5),
        (98, 300, 300),
        (228, 1, 5),
        (8, 5, 5),
        (136, 1, 5),
        (6, 5, 5),
        (162, 1, 5),
        (95, 1, 5),
    ] {
        operations.push(Insert(RowInsert::Skill(SkillRow {
            guid: 1,
            skill,
            value,
            max,
        })));
    }
    operations.push(Delete(RowDelete::EnteredInstancesByAccount(1)));
    operations.push(Insert(RowInsert::Tutorial(TutorialRow {
        account: 1,
        words: [0; 8],
    })));
    operations
}

#[test]
fn create_commits_37_ordered_results_and_exact_row_keys() {
    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let prepared = store.prepare(create_batch()).unwrap();
    assert!(store.read_rows().unwrap().character().is_none());
    let receipt = store.commit(prepared).unwrap();
    let mut counts = [OperationCount {
        matched: 0,
        affected: 1,
    }; 37];
    for index in [0, 18, 24, 35] {
        counts[index] = OperationCount {
            matched: 0,
            affected: 0,
        };
    }
    assert_eq!(receipt.operation_counts(), counts);
    drop(store);

    let (_store, rows) = temp.open().unwrap();
    assert_eq!(rows.character(), Some(&*character()));
    assert_eq!(
        rows.inventory()
            .iter()
            .map(|r| (r.guid, r.bag_guid, r.slot, r.item_guid, r.item_template))
            .collect::<Vec<_>>(),
        [
            (1, 0, 4, 2, 56),
            (1, 0, 6, 4, 1395),
            (1, 0, 7, 6, 55),
            (1, 0, 15, 8, 35),
            (1, 0, 3, 10, 6096),
            (1, 0, 23, 12, 2070),
            (1, 0, 24, 14, 159),
            (1, 0, 25, 16, 6948),
        ]
    );
    assert_eq!(
        rows.items()
            .iter()
            .map(|r| (r.guid, r.owner_guid, r.entry, r.count))
            .collect::<Vec<_>>(),
        [
            (2, 1, 56, 1),
            (4, 1, 1395, 1),
            (6, 1, 55, 1),
            (8, 1, 35, 1),
            (10, 1, 6096, 1),
            (12, 1, 2070, 5),
            (14, 1, 159, 5),
            (16, 1, 6948, 1),
        ]
    );
    assert_eq!(
        rows.actions()
            .iter()
            .map(|r| (r.guid, r.button, r.action, r.kind))
            .collect::<Vec<_>>(),
        [
            (1, 0, 6603, 0),
            (1, 1, 133, 0),
            (1, 2, 168, 0),
            (1, 10, 159, 128),
            (1, 11, 2070, 128),
        ]
    );
    assert_eq!(
        rows.skills()
            .iter()
            .map(|r| (r.guid, r.skill, r.value, r.max))
            .collect::<Vec<_>>(),
        [
            (1, 6, 5, 5),
            (1, 8, 5, 5),
            (1, 95, 1, 5),
            (1, 98, 300, 300),
            (1, 136, 1, 5),
            (1, 162, 1, 5),
            (1, 228, 1, 5),
            (1, 237, 5, 5),
            (1, 415, 1, 1),
            (1, 754, 5, 5),
        ]
    );
    assert_eq!(
        rows.tutorial(),
        Some(&TutorialRow {
            account: 1,
            words: [0; 8]
        })
    );
    assert!(
        rows.cooldowns.is_empty() && rows.auras.is_empty() && rows.entered_instances.is_empty()
    );
    assert!(rows.spells.is_empty());
}

#[test]
fn updates_distinguish_unchanged_rows_from_changed_rows_and_key_order() {
    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let prepared = store.prepare(create_batch()).unwrap();
    store.commit(prepared).unwrap();
    let prepared = store
        .prepare([
            Update(RowUpdate::Inventory(inventory(24, 14, 159))),
            Update(RowUpdate::ItemInstance(item(14, 159, 7))),
            Update(RowUpdate::OnlineByGuid {
                guid: 1,
                online: true,
            }),
            Update(RowUpdate::ClearOnline),
            Update(RowUpdate::ClearOnline),
            Delete(RowDelete::Inventory { item_guid: 2 }),
            Delete(RowDelete::ItemInstance(2)),
        ])
        .unwrap();
    let receipt = store.commit(prepared).unwrap();
    assert_eq!(
        receipt
            .operation_counts()
            .iter()
            .map(|r| (r.matched, r.affected))
            .collect::<Vec<_>>(),
        [(1, 0), (1, 1), (1, 1), (1, 1), (0, 0), (1, 1), (1, 1),]
    );
    let rows = store.read_rows().unwrap();
    assert_eq!(rows.items().iter().find(|r| r.guid == 14).unwrap().count, 7);
    assert!(!rows.character().unwrap().online);
    assert_eq!(rows.inventory()[0].item_guid, 4);
    assert_eq!(rows.items()[0].guid, 4);
}

#[test]
fn invalid_late_operation_keeps_both_files_and_store_retryable() {
    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let prepared = store.prepare(create_batch()).unwrap();
    store.commit(prepared).unwrap();
    let current = fs::read(temp.file(CURRENT)).unwrap();
    let previous = fs::read(temp.file(PREVIOUS)).unwrap();
    let mut bad_character = character();
    bad_character.class = 1;
    for (invalid, expected) in [
        (
            vec![Insert(RowInsert::Inventory(inventory(26, 17, 755)))],
            "source inventory and item rows do not join",
        ),
        (
            vec![
                Insert(RowInsert::Inventory(inventory(24, 17, 755))),
                ReplaceItemInstance(item(17, 755, 2)),
            ],
            "duplicate source inventory slot",
        ),
        (
            vec![
                Delete(RowDelete::Character(1)),
                Insert(RowInsert::Character(bad_character)),
            ],
            "character is outside the source store domain",
        ),
        (
            vec![Update(RowUpdate::Skill(SkillRow {
                guid: 1,
                skill: 999,
                value: 1,
                max: 5,
            }))],
            "source UPDATE target does not exist",
        ),
        (
            vec![Insert(RowInsert::Inventory(inventory(26, 14, 159)))],
            "source INSERT target already exists",
        ),
        (
            vec![Insert(RowInsert::Spell(SpellRow {
                guid: 1,
                spell: 133,
                active: true,
                disabled: false,
            }))],
            "unsupported nonempty source table",
        ),
    ] {
        let login = Update(RowUpdate::OnlineByGuid {
            guid: 1,
            online: true,
        });
        let result = store.prepare(std::iter::once(login).chain(invalid));
        assert!(matches!(result, Err(Error::Invalid(reason)) if reason == expected));
        assert_eq!(store.generation(), 2);
        assert!(!store.requires_reopen());
        assert_eq!(fs::read(temp.file(CURRENT)).unwrap(), current);
        assert_eq!(fs::read(temp.file(PREVIOUS)).unwrap(), previous);
        assert!(!store.read_rows().unwrap().character().unwrap().online);
    }
    let valid = store
        .prepare([Update(RowUpdate::OnlineByGuid {
            guid: 1,
            online: true,
        })])
        .unwrap();
    assert_eq!(store.commit(valid).unwrap().generation(), 3);
}

#[test]
fn row_cap_refuses_an_oversized_batch_before_publish() {
    let temp = Temp::new();
    let (mut store, initial) = temp.create();
    let rows = (0..=120).map(|button| {
        Insert(RowInsert::Action(ActionRow {
            guid: 1,
            button,
            action: 6603,
            kind: 0,
        }))
    });
    let operations = std::iter::once(Insert(RowInsert::Character(character()))).chain(rows);
    assert!(matches!(
        store.prepare(operations),
        Err(Error::Invalid("source row limit exceeded"))
    ));
    assert_eq!(
        hash(&fs::read(temp.file(CURRENT)).unwrap()),
        initial.digest()
    );
    assert!(!temp.file(PREVIOUS).exists());
    assert!(!store.requires_reopen());
}

#[test]
fn strict_reopen_rejects_unsorted_keys_and_unsupported_nonempty_payload() {
    let temp = Temp::new();
    let (mut store, initial) = temp.create();
    let prepared = store.prepare(create_batch()).unwrap();
    store.commit(prepared).unwrap();
    let mut rows = store.read_rows().unwrap();
    drop(store);
    let head = Header {
        generation: 2,
        parent: initial.digest(),
    };
    rows.inventory.swap(0, 1);
    fs::write(temp.file(CURRENT), head.encode(&identity(), &rows).unwrap()).unwrap();
    assert!(matches!(
        temp.open(),
        Err(Error::Invalid("invalid source table rows or keys"))
    ));
    rows.inventory.swap(0, 1);
    rows.spells.push(SpellRow {
        guid: 1,
        spell: 133,
        active: true,
        disabled: false,
    });
    fs::write(temp.file(CURRENT), head.encode(&identity(), &rows).unwrap()).unwrap();
    assert!(matches!(
        temp.open(),
        Err(Error::Invalid("unsupported nonempty source table"))
    ));
}

#[test]
fn world_rows_use_separate_kind_keys_and_persist_without_a_character() {
    let temp = Temp::new();
    let (mut store, _) = temp.create();
    let creature = RespawnRow {
        kind: RespawnKind::Creature,
        guid: 12,
        instance: 0,
        deadline: 10_000,
    };
    let object = RespawnRow {
        kind: RespawnKind::GameObject,
        ..creature
    };
    let dragons = DragonStateRow {
        state: 0,
        deadline: 0,
        positions: [14890, 14889, 14888, 14887],
    };
    let prepared = store
        .prepare([
            Insert(RowInsert::Respawn(object)),
            Insert(RowInsert::Respawn(creature)),
            Insert(RowInsert::DragonState(dragons)),
            Delete(RowDelete::Respawn {
                kind: RespawnKind::GameObject,
                guid: 12,
                instance: 0,
            }),
        ])
        .unwrap();
    let receipt = store.commit(prepared).unwrap();
    assert_eq!(
        receipt.operation_counts()[3],
        OperationCount {
            matched: 1,
            affected: 1
        }
    );
    drop(store);
    let (_store, rows) = temp.open().unwrap();
    assert!(rows.character().is_none());
    assert_eq!(rows.respawns(), [creature]);
    assert_eq!(rows.dragons(), Some(&dragons));
}
