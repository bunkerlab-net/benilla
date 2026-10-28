// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored operands only; no client/database capture or corpse-world admission.
use benilla_protocol::{messages as protocol, ObjectFields};
use benilla_sim::corpse_appearance::{capture_ordinary, EquippedItem, OwnerAppearance};
use benilla_world_client::corpse_appearance::{field_pairs, fields};

fn raw(fields: &ObjectFields, index: u16) -> Option<u32> {
    fields
        .raw_fields()
        .find_map(|(key, value)| (key == index).then_some(value))
}

fn owner() -> OwnerAppearance {
    OwnerAppearance {
        race: 6,
        gender: 1,
        skin: 3,
        face: 7,
        hair_style: 2,
        hair_color: 4,
        facial_hair: 5,
        native_display_id: 49,
        guild_id: 1234,
        hide_helm: false,
        hide_cloak: false,
        equipment: std::array::from_fn(|i| {
            Some(EquippedItem {
                display_info_id: 900 + i as u32,
                inventory_type: 1 + i as u32,
            })
        }),
    }
}

/// Test-only Values packet envelope through the real opcode/mask decoder, not a mock parser.
fn wire_roundtrip(pairs: &[(u16, u32)]) -> ObjectFields {
    let mut body = 1u32.to_le_bytes().to_vec();
    body.extend([0, 0, 1, 42]); // no transport, VALUES, packed authored GUID42
    body.push(2); // two mask words cover the appearance descriptor
    let mut masks = [0u32; 2];
    for &(index, _) in pairs {
        masks[index as usize / 32] |= 1 << (index % 32);
    }
    for mask in masks {
        body.extend(mask.to_le_bytes());
    }
    for &(_, value) in pairs {
        body.extend(value.to_le_bytes());
    }
    let (packet, tail) =
        protocol::parse_server_with_tail(protocol::opcode::SMSG_UPDATE_OBJECT, &body).unwrap();
    assert_eq!(tail, 0);
    let protocol::ServerPacket::UpdateObject { mut objects } = packet else {
        panic!("wrong packet")
    };
    assert_eq!(objects.len(), 1);
    let protocol::Object::Values { guid, mask } = objects.remove(0) else {
        panic!("wrong object")
    };
    assert_eq!(guid, 42);
    mask
}

#[test]
fn complete_source_snapshot_reaches_existing_wire_parser_and_appearance_accessors() {
    for hide_helm in [false, true] {
        for hide_cloak in [false, true] {
            let mut input = owner();
            input.hide_helm = hide_helm;
            input.hide_cloak = hide_cloak;
            let snapshot = capture_ordinary(&input);
            let pairs = field_pairs(&snapshot);
            let direct = fields(&snapshot);
            let decoded = wire_roundtrip(&pairs);
            for (index, word) in pairs {
                assert_eq!(raw(&direct, index), Some(word));
                assert_eq!(raw(&decoded, index), Some(word));
            }
            assert_eq!(
                decoded.corpse_look(),
                Some(benilla_protocol::CorpseLook {
                    race: 6,
                    sex: 1,
                    skin: 3,
                    face: 7,
                    hair_style: 2,
                    hair_color: 4,
                    facial_hair: 5,
                })
            );
            assert_eq!(decoded.corpse_display_id(), Some(49));
            assert_eq!(decoded.corpse_guild(), 1234);
            assert_eq!(decoded.corpse_hides_helm(), hide_helm);
            assert_eq!(decoded.corpse_hides_cloak(), hide_cloak);
            assert!(!decoded.corpse_is_bones());
            for i in 0..19 {
                assert_eq!(decoded.corpse_item(i), Some((900 + u32::from(i), 1 + i)));
            }
            // Appearance alone never supplies corpse ownership, world pose or dynamic loot flags.
            assert_eq!(decoded.corpse_owner(), None);
            assert_eq!(raw(&decoded, 36), None);
        }
    }
}

#[test]
fn known_empty_slots_stay_explicit_and_hidden_equipment_is_not_removed() {
    let mut input = owner();
    input.hide_helm = true;
    input.hide_cloak = true;
    input.equipment[4] = None;
    let snapshot = capture_ordinary(&input);
    let decoded = wire_roundtrip(&field_pairs(&snapshot));
    assert_eq!(raw(&decoded, 17), Some(0));
    assert_eq!(decoded.corpse_item(4), None);
    assert_eq!(decoded.corpse_item(0), Some((900, 1)));
    assert_eq!(decoded.corpse_item(14), Some((914, 15)));
    // Snapshot owns its old words; subsequent owner equipment/appearance mutations cannot alter it.
    input.equipment[0] = None;
    input.native_display_id = 999;
    let changed = capture_ordinary(&input);
    assert_eq!(changed.equipment[0], 0);
    assert_eq!(changed.native_display_id, 999);
    assert_eq!(snapshot.equipment[0], 900 | (1 << 24));
    assert_eq!(snapshot.native_display_id, 49);
}

#[test]
fn raw_uint32_item_or_and_all_byte_bits_are_preserved_without_extra_masking() {
    let mut input = owner();
    input.race = 255;
    input.gender = 254;
    input.skin = 253;
    input.face = 252;
    input.hair_style = 251;
    input.hair_color = 250;
    input.facial_hair = 249;
    input.equipment[0] = Some(EquippedItem {
        display_info_id: 0xa512_3456,
        inventory_type: 0x102,
    });
    let snapshot = capture_ordinary(&input);
    assert_eq!(snapshot.bytes_1, 0xfdfe_ff00);
    assert_eq!(snapshot.bytes_2, 0xf9fa_fbfc);
    assert_eq!(snapshot.equipment[0], 0xa712_3456);
    let decoded = wire_roundtrip(&field_pairs(&snapshot));
    assert_eq!(decoded.corpse_item(0), Some((0x123456, 0xa7)));
    let look = decoded.corpse_look().unwrap();
    assert_eq!(
        (
            look.face,
            look.hair_style,
            look.hair_color,
            look.facial_hair
        ),
        (252, 251, 250, 249)
    );
}
