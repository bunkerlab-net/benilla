// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_protocol::messages::{parse_server_with_tail, Object, ObjectType, ServerPacket};
use std::io::{BufRead, BufReader, Seek, SeekFrom};

const ITEM_TRACE_START: u64 = 4_371_397_127;
// Northshire-01 A trace.tsv:68134600-68134687.
const IDENTITIES: [usize; 16] = [
    68_134_604, 68_134_610, 68_134_613, 68_134_618, 68_134_623, 68_134_632, 68_134_635, 68_134_643,
    68_134_646, 68_134_651, 68_134_654, 68_134_659, 68_134_665, 68_134_670, 68_134_673, 68_134_687,
];
const DESTROYS: [usize; 8] = [
    68_134_606, 68_134_614, 68_134_625, 68_134_636, 68_134_647, 68_134_655, 68_134_666, 68_134_683,
];

struct TraceItemEvent {
    guid: SourceGuid,
    destroyed: bool,
    line: usize,
}

fn item_identities(trace: &Path) -> (Vec<SourceGuid>, Vec<TraceItemEvent>) {
    let mut file = BufReader::new(std::fs::File::open(trace).unwrap());
    file.seek(SeekFrom::Start(ITEM_TRACE_START)).unwrap();
    let mut out = Vec::new();
    let mut events = Vec::new();
    let mut dead = 0;
    let mut last_probe = None;
    let mut last_birth = None;
    let mut last_construct = None;
    for (line, row) in (68_134_600..=68_134_687).zip(file.lines()) {
        let row = row.unwrap();
        let mut cols = row.splitn(4, '\t');
        if cols.next() != Some("12") {
            continue;
        }
        let _seq = cols.next().unwrap();
        let event = cols.next().unwrap();
        let args: Vec<_> = cols.next().unwrap().split_ascii_whitespace().collect();
        match event {
            "logical_birth" => {
                let [logical, "object", "12", ordinal, pointer] = args.as_slice() else {
                    panic!("trace {line}: expected an item birth");
                };
                assert_eq!(ordinal.parse::<usize>().unwrap(), 27 + out.len());
                assert!(last_probe.is_none());
                assert!(last_birth
                    .replace((pointer.to_string(), logical.to_string()))
                    .is_none());
            }
            "object_construct" => {
                let [pointer] = args.as_slice() else {
                    panic!("trace {line}: constructor operands")
                };
                assert_eq!(last_birth.as_ref().unwrap().0, *pointer);
                assert!(last_construct.replace(pointer.to_string()).is_none());
            }
            "object_identity" => {
                let [pointer, low, counter, "0", "16384", logical] = args.as_slice() else {
                    panic!("trace {line}: expected an entry-less item GUID");
                };
                let (born, id) = last_birth.take().unwrap();
                assert_eq!((born.as_str(), id.as_str()), (*pointer, *logical));
                assert_eq!(last_construct.take().as_deref(), Some(*pointer));
                assert_eq!(line, IDENTITIES[out.len()]);
                assert_eq!(low, counter);
                let counter: u32 = counter.parse().unwrap();
                assert_eq!(counter as usize, out.len() + 1);
                out.push(SourceGuid::from_raw(
                    0x4000_0000_0000_0000 | u64::from(counter),
                ));
                events.push(TraceItemEvent {
                    guid: out[counter as usize - 1],
                    destroyed: false,
                    line,
                });
                if counter % 2 == 1 {
                    last_probe = Some((born, id));
                }
            }
            "object_destroy" => {
                let [pointer] = args.as_slice() else {
                    panic!("trace {line}: destruction operands")
                };
                assert_eq!(line, DESTROYS[dead]);
                assert_eq!(last_probe.as_ref().unwrap().0, *pointer);
                events.push(TraceItemEvent {
                    guid: *out.last().unwrap(),
                    destroyed: true,
                    line,
                });
            }
            "logical_death" => {
                let [logical] = args.as_slice() else {
                    panic!("trace {line}: death operands")
                };
                assert_eq!(line, DESTROYS[dead] + 1);
                assert_eq!(last_probe.take().unwrap().1, *logical);
                dead += 1;
            }
            _ => panic!("trace {line}: unexpected creation item event {event}"),
        }
    }
    assert_eq!((out.len(), dead), (16, 8));
    (out, events)
}

#[derive(Debug, PartialEq, Eq)]
struct ItemRow {
    guid: u64,
    entry: u32,
    slot: usize,
    count: u32,
    owner: u64,
    contained: u64,
    durability: u32,
    max_durability: u32,
    charges: [i32; 5],
    flags: u32,
}

fn login_items(trace: &Path) -> Vec<ItemRow> {
    // Northshire-01 A client-packets.jsonl:31 / trace.tsv:68751755.
    let path = trace
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("client-packets.jsonl");
    let packet: serde_json::Value = serde_json::from_str(
        &BufReader::new(std::fs::File::open(path).unwrap())
            .lines()
            .nth(30)
            .unwrap()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        (packet["opcode"].as_u64(), packet["bytes"].as_u64()),
        (Some(502), Some(470))
    );
    let hex = packet["hex"].as_str().unwrap();
    let body: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    let (
        ServerPacket::UpdateObject {
            first_pass,
            objects,
        },
        tail,
    ) = parse_server_with_tail(502, &body).unwrap()
    else {
        panic!("expected the compressed player and item create blocks");
    };
    assert!(!first_pass);
    assert_eq!(tail, 0);
    let mut all = objects.iter();
    let Some(Object::Create {
        guid: 1,
        object_type: ObjectType::Player,
        mask: player,
        ..
    }) = all.next_back()
    else {
        panic!("final update block is the player");
    };
    let player: BTreeMap<_, _> = player.raw_fields().collect();
    let inventory: BTreeMap<_, _> = (0..39)
        .filter_map(|slot| {
            let guid = guid_word(&player, 0x1e6 + 2 * slot as u16);
            (guid != 0).then_some((guid, slot))
        })
        .collect();
    assert_eq!(inventory.len(), 8);
    let mut rows: Vec<_> = all
        .map(|object| {
            let Object::Create {
                guid,
                object_type: ObjectType::Item,
                mask: item,
                ..
            } = object
            else {
                panic!("expected an Item create block");
            };
            let item: BTreeMap<_, _> = item.raw_fields().collect();
            let word = |field| item.get(&field).copied().unwrap_or(0);
            assert_eq!(guid_word(&item, 0), *guid);
            let slot = inventory[guid];
            let entry = word(3);
            if slot < 19 {
                assert_eq!(player[&(0x104 + 12 * slot as u16)], entry);
            }
            ItemRow {
                guid: *guid,
                entry,
                slot,
                count: word(14),
                owner: guid_word(&item, 6),
                contained: guid_word(&item, 8),
                durability: word(46),
                max_durability: word(47),
                charges: std::array::from_fn(|i| word(16 + i as u16) as i32),
                flags: word(21),
            }
        })
        .collect();
    assert_eq!(rows.len(), 8);
    rows.sort_unstable_by_key(|row| row.guid);
    rows
}

fn guid_word(fields: &BTreeMap<u16, u32>, field: u16) -> u64 {
    let word = |at| u64::from(fields.get(&at).copied().unwrap_or(0));
    word(field) | (word(field + 1) << 32)
}

pub(super) fn assert_item_prefix(
    trace: &Path,
    objects: &SourceObjects,
    player: &SourcePlayer,
    items: &SourceItems,
    first: u64,
    (created, probes): (usize, usize),
    actual: &[ItemLifetimeObservation],
) {
    let (traced, events) = item_identities(trace);
    let login = login_items(trace);
    assert_eq!((login[0].entry, login[0].slot, login[0].count), (56, 4, 1));
    assert_eq!(login[0].guid, traced[1].raw());
    assert_eq!(actual.len(), created + 2 * probes);
    for (
        &TraceItemEvent {
            guid,
            destroyed,
            line,
        },
        &actual,
    ) in events.iter().zip(actual)
    {
        let expected = if destroyed {
            ItemLifetimeObservation::Destroyed { guid }
        } else {
            let entry = login[(guid.counter() as usize - 1) / 2].entry;
            ItemLifetimeObservation::Assigned { guid, entry }
        };
        assert_eq!(actual, expected, "trace.tsv:{line}");
    }
    assert_eq!(
        objects.construction_count(12),
        first + (created + probes) as u64 - 1
    );
    let mut live = Vec::new();
    for (slot, handle) in player.items.iter().enumerate() {
        let Some(handle) = *handle else { continue };
        let item = items.get(handle).unwrap();
        let object = objects.get(handle).unwrap();
        assert_eq!(object.kind, SourceObjectKind::Item);
        assert!(object.map.is_none() && !object.in_world);
        let guid = object.guid.unwrap();
        let ordinal = (handle.construction - first) as usize;
        assert_eq!(guid, traced[ordinal]);
        assert_eq!(usize::from(item.slot), slot);
        live.push(ItemRow {
            guid: guid.raw(),
            entry: item.entry,
            slot,
            count: item.count,
            owner: item.owner.raw(),
            contained: item.contained.raw(),
            durability: item.durability,
            max_durability: item.max_durability,
            charges: item.charges,
            flags: item.flags,
        });
    }
    live.sort_unstable_by_key(|row| row.guid);
    assert_eq!(live, login[..created]);
    for ordinal in (0..created + probes).step_by(2) {
        let handle = ObjectHandle {
            thread: 12,
            construction: first + ordinal as u64,
        };
        assert!(objects.get(handle).is_err());
        assert!(items.get(handle).is_err());
    }
}

pub(super) fn guid_allocators() -> GlobalGuidAllocators {
    GlobalGuidAllocators::from_seed_rows(GuidSeedRows {
        player: GuidMaximum::NoResult,
        // Northshire-01 A proxy-requests.jsonl:168,181 returns NULL MAX(guid).
        item: GuidMaximum::NoResult,
        corpse: GuidMaximum::NoResult,
        pet_number: GuidMaximum::NoResult,
        creature: GuidMaximum::NoResult,
        gameobject: GuidMaximum::NoResult,
        creature_reserve: 0,
        gameobject_reserve: 0,
    })
}
