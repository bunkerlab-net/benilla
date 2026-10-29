// SPDX-License-Identifier: GPL-3.0-or-later
//! Real Lua mainhand cursor commands with durable same-instance publication.
use super::*;
use benilla_world_runtime::native::synthetic_equipment_fixture as armor;

struct StaffCatalog;
impl ClientCatalog for StaffCatalog {
    fn build(&self) -> u32 {
        LootCatalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        LootCatalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        LootCatalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        // Both gender alternatives for each of the two authored source owners.
        matches!(id, 34 | 131 | 134) || LootCatalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        id == 900003 || LootCatalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        LootCatalog.contains_spell(id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        LootCatalog.player_power_type(player)
    }
    fn player_bytes_2(&self, player: PlayerId) -> Option<u32> {
        LootCatalog.player_bytes_2(player)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        if entry != 900002 {
            return LootCatalog.item_metadata(entry);
        }
        let mut info = LootCatalog.item_metadata(900001)?;
        info.class = 2;
        info.subclass = 10;
        info.name = "Authored original staff".into();
        info.display_info_id = 900003;
        info.inventory_type = 17;
        info.stats = vec![(3, 3)];
        info.damages = vec![benilla_protocol::messages::ItemDamage {
            min: 2.0,
            max: 4.0,
            school: 0,
        }];
        info.dmg_min = 2.0;
        info.dmg_max = 4.0;
        info.armor = 0;
        info.delay_ms = 2500;
        info.max_durability = 1000;
        Some(info)
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        LootCatalog.quest_metadata(id)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        LootCatalog.quest_greeting(id)
    }
    fn native_quest_greeting(&self, id: u32) -> Option<benilla_world_client::NativeQuestGreeting> {
        LootCatalog.native_quest_greeting(id)
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        LootCatalog.native_quest_presentation(id)
    }
}

fn enter_staff(h: &Harness, open: bool) -> NativeController {
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        armor::weapon_profile(),
        &StaffCatalog,
        armor::EquipmentAuthority::default(),
    )
    .unwrap()
}
fn values(events: &[SessionEvent], player: u64) -> benilla_protocol::ObjectFields {
    let mut result = benilla_protocol::ObjectFields::default();
    for e in events {
        if let SessionEvent::ObjectValues { guid, fields } = e {
            if *guid == player {
                result.merge(fields.clone());
            }
        }
    }
    result
}
#[test]
fn mainhand_cursor_move_withholds_same_object_until_save_then_reopens_and_reequips() {
    use benilla_ui::script::{ContainerState, InvSlotView, InventorySlots, UiScript};
    let h = Harness::new();
    let mut c = enter_staff(&h, false);
    let initial = c.initial_events().unwrap();
    let mut fields = values(&initial, c.player_guid());
    let guid = fields.player_inv_slot(15).unwrap();
    assert_ne!(guid, 0);
    let frontier = inventory(&c).next_item;
    let mana = c.world.state().resources.caster.mana;
    let maximum = c.world.state().combat.as_ref().unwrap().stats.max_mana;
    let mut ui = UiScript::new().unwrap();
    let mut doll: InventorySlots = Default::default();
    doll[16] = Some(InvSlotView {
        item_id: 900002,
        guid,
        count: 1,
        equip_slots: vec![16],
        ..Default::default()
    });
    ui.set_inventory_slots(doll);
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            ..Default::default()
        }),
    );
    ui.run("PickupInventoryItem(16); PickupContainerItem(0, 1)")
        .unwrap();
    let moves = ui.take_container_moves();
    assert_eq!(moves.len(), 1);
    let m = &moves[0];
    let (src_bag, src_slot) = crate::ui_items::wire_pos(m.src_bag, m.src_slot).unwrap();
    let (dst_bag, dst_slot) = crate::ui_items::wire_pos(m.dst_bag, m.dst_slot).unwrap();
    assert_eq!((src_bag, src_slot, dst_bag, dst_slot), (255, 15, 255, 23));
    c.receive(ClientCommand::SwapInvItem { src_slot, dst_slot })
        .unwrap();
    inject(&h);
    let failed = c.pump(c.tick_duration());
    paused(&failed);
    assert!(failed.events.is_empty());
    assert_eq!(fields.player_inv_slot(15), Some(guid));
    let digest = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let committed = c.pump(Duration::ZERO);
    assert!(!committed.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. }
            | SessionEvent::ObjectDestroyed(_)
            | SessionEvent::ItemPushResult(_)
    )));
    fields.merge(values(&committed.events, c.player_guid()));
    assert_eq!(fields.player_inv_slot(15).unwrap_or(0), 0);
    assert_eq!(fields.player_pack_slot(0), Some(guid));
    assert_eq!(fields.player_visible_item_entry(15), None);
    assert_eq!(inventory(&c).next_item, frontier);
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().stats.max_mana,
        maximum
    );
    assert!(c.world.state().resources.caster.mana <= mana);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter_staff(&h, true);
    assert_eq!(c.world.state_digest(), digest);
    let initial = c.initial_events().unwrap();
    assert_eq!(
        initial
            .iter()
            .filter(|e| matches!(e,SessionEvent::ItemCreate { guid:g,.. } if *g==guid))
            .count(),
        1
    );
    assert_eq!(
        values(&initial, c.player_guid()).player_pack_slot(0),
        Some(guid)
    );
    let before_mana = c.world.state().resources.caster.mana;
    let mut ui = UiScript::new().unwrap();
    ui.set_container(
        0,
        Some(ContainerState {
            num_slots: 16,
            slots: [(
                1,
                benilla_ui::script::ContainerSlot {
                    guid,
                    item_id: 900002,
                    count: 1,
                    inventory_type: 17,
                    item_class: Some(2),
                    equip_slots: vec![16],
                    ..Default::default()
                },
            )]
            .into(),
            ..Default::default()
        }),
    );
    ui.set_inventory_slots(Default::default());
    ui.run("PickupContainerItem(0, 1); PickupInventoryItem(16)")
        .unwrap();
    let moves = ui.take_container_moves();
    assert_eq!(moves.len(), 1);
    let m = &moves[0];
    let src = crate::ui_items::wire_pos(m.src_bag, m.src_slot).unwrap();
    let dst = crate::ui_items::wire_pos(m.dst_bag, m.dst_slot).unwrap();
    assert_eq!((src, dst), ((255, 23), (255, 15)));
    c.receive(ClientCommand::SwapInvItem {
        src_slot: src.1,
        dst_slot: dst.1,
    })
    .unwrap();
    let equipped = step(&mut c);
    let fields = values(&equipped.events, c.player_guid());
    assert_eq!(fields.player_inv_slot(15), Some(guid));
    assert_eq!(fields.player_visible_item_entry(15), Some(900002));
    assert_eq!(
        c.world.state().combat.as_ref().unwrap().stats.max_mana,
        maximum
    );
    assert_eq!(c.world.state().resources.caster.mana, before_mana);
    assert_eq!(inventory(&c).next_item, frontier);
    assert!(!equipped.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemCreate { .. } | SessionEvent::ObjectDestroyed(_)
    )));
    c.shutdown().unwrap();
    let final_digest = c.world.state_digest();
    drop(c);
    assert_eq!(enter_staff(&h, true).world.state_digest(), final_digest);
}

#[path = "mainhand_auto_equip_tests.rs"]
mod mainhand_auto_equip_tests;

#[path = "mainhand_custody_tests.rs"]
mod mainhand_custody_tests;
