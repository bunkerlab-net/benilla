// SPDX-License-Identifier: GPL-3.0-or-later
use super::super::super::{
    item_recording,
    player_save_tests::{payload, prepared_operands, unsigned, Operand},
    tests::construct,
};
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
use serde_json::Value;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom};
use std::path::Path;

const INVENTORY_INSERT: &str =
    "INSERT INTO character_inventory (guid,bag,slot,item,item_template) VALUES (?, ?, ?, ?, ?)";
const INVENTORY_UPDATE: &str =
    "UPDATE character_inventory SET guid = ?, bag = ?, slot = ?, item_template = ? WHERE item = ?";
const INSTANCE_REPLACE: &str = concat!(
    "REPLACE INTO item_instance (owner_guid, itemEntry, creatorGuid, giftCreatorGuid, count, ",
    "duration, charges, flags, enchantments, randomPropertyId, durability, itemTextId, guid) ",
    "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
);
const INSTANCE_UPDATE: &str = concat!(
    "UPDATE item_instance SET owner_guid = ?, itemEntry = ?, creatorGuid = ?, giftCreatorGuid = ?, ",
    "count = ?, duration = ?, charges = ?, flags = ?, enchantments = ?, randomPropertyId = ?, ",
    "durability = ?, itemTextId = ? WHERE guid = ?",
);

fn u32_bind(word: u32) -> Operand {
    unsigned(3, word.to_le_bytes())
}

fn text_bind(words: impl IntoIterator<Item = impl std::fmt::Display>) -> Operand {
    let mut text = String::new();
    for word in words {
        write!(text, "{word} ").unwrap();
    }
    Operand {
        tag: [254, 0],
        bytes: text.into_bytes(),
    }
}

fn inventory_binds(row: &InventoryRow, update: bool) -> [Operand; 5] {
    let item = u32_bind(row.item_guid);
    let template = u32_bind(row.item_template);
    let [fourth, fifth] = if update {
        [template, item]
    } else {
        [item, template]
    };
    [
        u32_bind(row.guid),
        u32_bind(row.bag_guid),
        unsigned(1, [row.slot]),
        fourth,
        fifth,
    ]
}

fn instance_binds(row: &ItemInstanceRow) -> [Operand; 13] {
    [
        u32_bind(row.owner_guid),
        u32_bind(row.entry),
        u32_bind(row.creator),
        u32_bind(row.gift_creator),
        u32_bind(row.count),
        u32_bind(row.duration),
        text_bind(row.charges),
        u32_bind(row.flags),
        text_bind(row.enchantments.into_iter().flatten()),
        Operand {
            tag: [2, 0],
            bytes: row.random_property.to_le_bytes().into(),
        },
        unsigned(2, row.durability.to_le_bytes()),
        u32_bind(row.text_id),
        u32_bind(row.guid),
    ]
}

fn json_rows(root: &Path, file: &str, count: usize) -> impl Iterator<Item = (usize, String)> {
    BufReader::new(File::open(root.join(file)).unwrap())
        .lines()
        .take(count)
        .enumerate()
        .map(|(n, row)| (n + 1, row.unwrap()))
}

fn proxy_rows(root: &Path, numbers: impl IntoIterator<Item = usize>) -> BTreeMap<usize, Value> {
    let numbers: std::collections::BTreeSet<_> = numbers.into_iter().collect();
    json_rows(root, "proxy-requests.jsonl", *numbers.last().unwrap())
        .filter(|(n, _)| numbers.contains(n))
        .map(|(n, row)| (n, serde_json::from_str(&row).unwrap()))
        .collect()
}

fn assert_bind(
    proxy: &BTreeMap<usize, Value>,
    (prepare, execute): (usize, usize),
    sql: &str,
    binds: &[Operand],
) {
    let request = &proxy[&prepare];
    let bytes = payload(request);
    assert_eq!(bytes[0], 22);
    assert_eq!(std::str::from_utf8(&bytes[1..]).unwrap(), sql, "p{prepare}");
    let identity = &proxy[&(prepare + 1)];
    assert_eq!(identity["kind"], "prepared_identity");
    assert_eq!(identity["connection"], 4);
    assert_eq!(identity["request"], request["request"]);
    let id = identity["native_statement"]
        .as_u64()
        .unwrap()
        .try_into()
        .unwrap();
    let words = prepared_operands(&payload(&proxy[&execute]), id, binds.len());
    assert_eq!(words, binds, "p{execute}");
}

fn assert_observed_binds<'a>(
    proxy: &BTreeMap<usize, Value>,
    effects: impl ExactSizeIterator<Item = &'a RowOperation>,
    lines: &[(usize, usize)],
) {
    assert_eq!(effects.len(), lines.len());
    for (effect, &at) in effects.zip(lines) {
        match effect {
            RowOperation::Insert(RowInsert::Inventory(row)) => {
                assert_bind(proxy, at, INVENTORY_INSERT, &inventory_binds(row, false));
            }
            RowOperation::ReplaceItemInstance(row) => {
                assert_bind(proxy, at, INSTANCE_REPLACE, &instance_binds(row));
            }
            RowOperation::Update(RowUpdate::Inventory(row)) => {
                assert_bind(proxy, at, INVENTORY_UPDATE, &inventory_binds(row, true));
            }
            RowOperation::Update(RowUpdate::ItemInstance(row)) => {
                assert_bind(proxy, at, INSTANCE_UPDATE, &instance_binds(row));
            }
            _ => panic!("unexpected inventory operation at p{}: {effect:?}", at.1),
        }
    }
}

pub(in super::super::super) fn assert_create_recording(root: &Path, effects: &[RowOperation]) {
    // C proxy-requests.jsonl:1516-1552, all eight placement/instance pairs in queue order.
    let lines = [
        (1516, 1519),
        (1521, 1524),
        (1516, 1526),
        (1521, 1528),
        (1516, 1530),
        (1521, 1532),
        (1516, 1534),
        (1521, 1536),
        (1516, 1538),
        (1521, 1540),
        (1516, 1542),
        (1521, 1544),
        (1516, 1546),
        (1521, 1548),
        (1516, 1550),
        (1521, 1552),
    ];
    let proxy = proxy_rows(root, lines.iter().flat_map(|&(p, e)| [p, p + 1, e]));
    assert_observed_binds(&proxy, effects.iter(), &lines);
}

fn fixture<T>(
    templates: &SourceStartupItems,
    run: impl FnOnce(&mut SourcePlayer, &mut Outfit<'_>) -> T,
) -> T {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = construct(&mut objects, &mut units);
    let owner = player.object();
    objects
        .assign_guid(owner, 1, SourceGuid::from_raw(1))
        .unwrap();
    let unit = units.get_mut(&objects, owner).unwrap();
    run(
        &mut player,
        &mut Outfit {
            objects: &mut objects,
            unit,
            items: &mut SourceItems::default(),
            templates,
            guids: &mut item_recording::guid_allocators(),
            spells: &SourceSpellStores::default(),
            observe: &mut |_| {},
            class: 8,
            logging_out: false,
        },
    )
}

fn make(player: &SourcePlayer, live: &mut Outfit<'_>, entry: u32, count: u32) -> ObjectHandle {
    live.items
        .create(
            live.objects,
            live.guids,
            player.object(),
            entry,
            count,
            live.templates,
            live.observe,
        )
        .unwrap()
        .expect("packet item must have a production factory owner")
}

fn place(player: &mut SourcePlayer, live: &mut Outfit<'_>, handle: ObjectHandle, slot: usize) {
    let item = live.items.get(handle).unwrap();
    let count = item.count;
    let proto = live.templates.item_prototype(item.entry).unwrap().unwrap();
    player.store_item(live, handle, slot, count, proto).unwrap();
}

fn set_state(
    player: &mut SourcePlayer,
    live: &mut Outfit<'_>,
    item: ObjectHandle,
    state: SourceItemState,
) {
    SourceItem::set_state(item, state, player, live.objects, live.items, live.observe).unwrap();
}

fn save(player: &SourcePlayer, live: &Outfit<'_>) -> Vec<SourcePlayerSaveObservation> {
    let mut rows = Vec::new();
    let guid = live
        .objects
        .get(player.object())
        .unwrap()
        .guid
        .unwrap()
        .counter();
    player
        .save_inventory(live.objects, live.items, guid, true, &mut |effect| {
            rows.push(effect);
            Ok(())
        })
        .unwrap();
    rows
}

#[test]
fn authored_a_packet_fixture_matches_all_six_logout_inventory_executes() {
    // Authored from A client-packets.jsonl:505,860,868,1869,2534,3675 until the source loot writer lands.
    let (Some(root), Some(trace)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_NORTHSHIRE_A_TRACE"),
    ) else {
        println!("SKIP A logout inventory: set BENILLA_F2_CATALOG and BENILLA_NORTHSHIRE_A_TRACE");
        return;
    };
    let a = Path::new(&trace)
        .ancestors()
        .nth(5)
        .unwrap()
        .join("northshire-03/source-choice/a");
    let (_, packet) = json_rows(&a, "client-packets.jsonl", 3675).last().unwrap();
    let mut packet: Value = serde_json::from_str(&packet).unwrap();
    assert_eq!(packet["kind"], "final_projection");
    assert_eq!(packet["guid"], 1);
    let inventory: Vec<[u64; 4]> = serde_json::from_value(packet["inventory"].take()).unwrap();
    let [water, candle, apple] = [159, 755, 4536].map(|entry| {
        let [field, raw_guid, entry, count] =
            *inventory.iter().find(|row| row[2] == entry).unwrap();
        // UpdateFields.h:201 stores bag-zero slots as adjacent GUID words.
        assert_eq!(field % 2, 0);
        (
            (field - 0x1e6) as usize / 2,
            SourceGuid::from_raw(raw_guid),
            entry as u32,
            count as u32,
        )
    });
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let templates = SourceStartupItems::catalog_prefix(
        &mut catalog,
        &mut SourceRandom::new(
            1,
            &mut Mt19937::from_seed(1),
            &mut DrawCursor::default(),
            &mut |_| {},
        )
        .unwrap(),
    )
    .unwrap();
    let effects = fixture(&templates, |player, live| {
        let mut next = 1;
        let [water_handle, candle_handle, apple_handle] =
            [water, candle, apple].map(|(_, guid, entry, count)| {
                // This fixture constructs only the three touched items. Consume the other counters.
                while next < guid.counter() {
                    assert_eq!(live.guids.allocate(HighGuid::Item).unwrap().counter, next);
                    next += 1;
                }
                let handle = make(player, live, entry, count);
                assert_eq!(live.objects.get(handle).unwrap().guid, Some(guid));
                next += 1;
                handle
            });
        set_state(player, live, water_handle, SourceItemState::Unchanged);
        for ((slot, _, _, _), handle) in [
            (candle, candle_handle),
            (apple, apple_handle),
            (water, water_handle),
        ] {
            place(player, live, handle, slot);
        }
        save(player, live)
    });
    // A p1835-1851, with the create statements prepared at p1522 and p1527.
    let lines = [
        (1522, 1835),
        (1527, 1837),
        (1522, 1839),
        (1527, 1841),
        (1843, 1846),
        (1848, 1851),
    ];
    let proxy = proxy_rows(
        &a,
        lines
            .iter()
            .flat_map(|&(p, e)| [p, p + 1, e])
            .chain([1552, 1847, 1852]),
    );
    assert_observed_binds(
        &proxy,
        effects.iter().map(|event| match event {
            SourcePlayerSaveObservation::Row(row) => row,
            SourcePlayerSaveObservation::Begin => panic!("unexpected inventory BEGIN"),
        }),
        &lines,
    );
    let SourcePlayerSaveObservation::Row(RowOperation::Update(RowUpdate::Inventory(row))) =
        &effects[4]
    else {
        panic!("A:p1846 must update inventory");
    };
    assert_bind(
        &proxy,
        (1522, 1552),
        INVENTORY_INSERT,
        &inventory_binds(row, false),
    );
    // A p1847/p1852 respond to the placement update and instance update respectively.
    let mut wire = File::open(a.join("connection-4-responses.bin")).unwrap();
    for (line, matched_request, affected) in [(1847, 1846, 0), (1852, 1851, 1)] {
        assert_eq!(proxy[&line]["kind"], "response");
        assert_eq!(proxy[&line]["request"], proxy[&matched_request]["request"]);
        wire.seek(SeekFrom::Start(
            proxy[&line]["wire_offset"].as_u64().unwrap(),
        ))
        .unwrap();
        let mut header = [0; 6];
        wire.read_exact(&mut header).unwrap();
        assert_eq!(
            header[4..],
            [0, affected],
            "A:p{line} OK packet and affected rows"
        );
    }
}

fn ordinary_templates() -> SourceStartupItems {
    let mut templates = SourceStartupItems::default();
    templates
        .validate(
            [(
                1,
                StartupItemPrototype {
                    stackable: 20,
                    ..Default::default()
                },
            )]
            .into(),
            &Default::default(),
            &mut SourceRandom::new(
                1,
                &mut Mt19937::from_seed(1),
                &mut DrawCursor::default(),
                &mut |_| {},
            )
            .unwrap(),
        )
        .unwrap();
    templates
}

fn three_slots(player: &mut SourcePlayer, live: &mut Outfit<'_>) -> [ObjectHandle; 3] {
    [23, 24, 25].map(|slot| {
        let handle = make(player, live, 1, 1);
        place(player, live, handle, slot);
        handle
    })
}

#[test]
fn refused_or_failed_inventory_projection_keeps_states_and_positions() {
    fixture(&ordinary_templates(), |player, live| {
        let handles = three_slots(player, live);
        let state = |items: &SourceItems| {
            handles.map(|handle| {
                let item = items.get(handle).unwrap();
                (item.state, item.queue_pos)
            })
        };
        let before = [
            (SourceItemState::New, Some(0)),
            (SourceItemState::New, Some(1)),
            (SourceItemState::New, Some(2)),
        ];
        let queue = handles.map(Some);
        let mut attempts = 0;
        assert_eq!(
            player
                .save_inventory(live.objects, live.items, 1, true, &mut |_| {
                    attempts += 1;
                    require(attempts != 4, "test.inventory_observer_failure")
                })
                .unwrap_err()
                .id,
            "test.inventory_observer_failure"
        );
        assert_eq!(attempts, 4);
        assert_eq!(state(live.items), before);
        assert_eq!(player.item_update_queue, queue);
        assert_eq!(save(player, live).len(), 6);

        player.remove_backpack_item(live, handles[2], 25).unwrap();
        assert_eq!(
            player
                .save_inventory(live.objects, live.items, 1, true, &mut |_| panic!(
                    "corrupt queue wrote a row"
                ))
                .unwrap_err()
                .id,
            "execution.player_save.Player_15984.invalid_item_queue_owner"
        );
        assert_eq!(state(live.items), before);
        assert_eq!(player.item_update_queue, queue);
    });
}

#[test]
fn removed_new_item_leaves_a_hole_without_shifting_save_order() {
    fixture(&ordinary_templates(), |player, live| {
        let [first, removed, last] = three_slots(player, live);
        player.remove_backpack_item(live, removed, 24).unwrap();
        set_state(player, live, removed, SourceItemState::Removed);
        assert!(live.items.get(removed).is_err());
        assert!(live.objects.get(removed).is_err());
        assert_eq!(player.item_update_queue, [Some(first), None, Some(last)]);
        assert_eq!(live.items.get(first).unwrap().queue_pos, Some(0));
        assert_eq!(live.items.get(last).unwrap().queue_pos, Some(2));

        use SourcePlayerSaveObservation::Row;
        let rows = save(player, live);
        let [Row(RowOperation::Insert(RowInsert::Inventory(first))), Row(RowOperation::ReplaceItemInstance(first_instance)), Row(RowOperation::Insert(RowInsert::Inventory(last))), Row(RowOperation::ReplaceItemInstance(last_instance))] =
            rows.as_slice()
        else {
            panic!("unexpected save rows: {rows:?}");
        };
        assert_eq!(
            (
                first.item_guid,
                first_instance.guid,
                last.item_guid,
                last_instance.guid
            ),
            (1, 1, 3, 3)
        );
    });
}

#[test]
fn removed_persisted_item_deletes_placement_then_instance() {
    fixture(&ordinary_templates(), |player, live| {
        let handle = make(player, live, 1, 1);
        set_state(player, live, handle, SourceItemState::Unchanged);
        place(player, live, handle, 23);
        assert_eq!(
            live.items.get(handle).unwrap().state,
            SourceItemState::Changed
        );
        player.remove_backpack_item(live, handle, 23).unwrap();
        set_state(player, live, handle, SourceItemState::Removed);
        let rows = save(player, live);
        assert_eq!(
            rows,
            [
                SourcePlayerSaveObservation::Row(RowOperation::Delete(RowDelete::Inventory {
                    item_guid: 1
                })),
                SourcePlayerSaveObservation::Row(RowOperation::Delete(RowDelete::ItemInstance(1))),
            ]
        );
        assert_eq!(
            live.items.get(handle).unwrap().state,
            SourceItemState::Removed
        );
        assert_eq!(live.items.get(handle).unwrap().queue_pos, Some(0));
    });
}

#[test]
fn blocked_queue_ignores_insertion_and_unchanged_item_writes_nothing() {
    fixture(&ordinary_templates(), |player, live| {
        let handle = make(player, live, 1, 1);
        player.item_update_queue_blocked = true;
        place(player, live, handle, 23);
        assert_eq!(live.items.get(handle).unwrap().state, SourceItemState::New);
        assert_eq!(live.items.get(handle).unwrap().queue_pos, None);
        assert!(player.item_update_queue.is_empty());

        player.item_update_queue_blocked = false;
        set_state(player, live, handle, SourceItemState::Changed);
        assert_eq!(player.item_update_queue, [Some(handle)]);
        assert_eq!(live.items.get(handle).unwrap().queue_pos, Some(0));

        set_state(player, live, handle, SourceItemState::Unchanged);
        assert_eq!(live.items.get(handle).unwrap().queue_pos, None);
        assert_eq!(player.item_update_queue, [Some(handle)]);
        assert!(save(player, live).is_empty());
    });
}
