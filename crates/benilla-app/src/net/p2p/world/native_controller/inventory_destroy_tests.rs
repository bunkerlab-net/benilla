// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn destroy(slot: u8, count: u8) -> ClientCommand {
    ClientCommand::DestroyItem {
        bag_index: 255,
        slot,
        count,
    }
}
#[test]
fn stock_cursor_delete_selected_later_stack_is_checkpointed_once_and_reopened() {
    use benilla_ui::script::{ContainerSlot, ContainerState, UiScript};
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    let first = inventory(&c).slots[&23].clone();
    let selected = cache.guid(24);
    let allocator = inventory(&c).next_item;
    let mut ui = UiScript::new().unwrap();
    ui.keep_calls();
    ui.set_container(
        0,
        Some(ContainerState {
            name: Some("Backpack".into()),
            num_slots: 16,
            slots: [(
                2,
                ContainerSlot {
                    guid: selected,
                    item_id: 70001,
                    count: 2,
                    texture: Some("Authored icon".into()),
                    ..Default::default()
                },
            )]
            .into(),
        }),
    );
    ui.run("PickupContainerItem(0, 2); DeleteCursorItem()")
        .unwrap();
    let requests = ui.take_container_destroys();
    assert_eq!(requests, vec![(0, 2, 0)]);
    c.receive(destroy(22 + requests[0].1 as u8, requests[0].2 as u8))
        .unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert_eq!(cache.guid(24), selected);
    assert!(!inventory(&c).slots.contains_key(&24));
    assert_eq!(inventory(&c).slots[&23], first);
    assert_eq!(inventory(&c).next_item, allocator);
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    assert!(c.retry_checkpoint().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    cache.apply(&out.events, c.player_guid());
    assert_eq!(cache.guid(24), 0);
    assert_eq!(cache.count(23), 5);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e,SessionEvent::ObjectDestroyed(g) if *g==selected))
            .count(),
        1
    );
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemPushResult(_) | SessionEvent::QuestObjectiveItem { .. }
    )));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let reopened = enter(&h, profile(7), true);
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(inventory(&reopened).slots[&23], first);
    assert!(!inventory(&reopened).slots.contains_key(&24));
}
#[test]
fn partial_destroy_and_excess_count_stay_on_selected_stack_without_allocator_change() {
    let h = Harness::new();
    let (mut c, mut cache) = start(&h, 7);
    let first = inventory(&c).slots[&23].clone();
    let selected = cache.guid(24);
    let allocator = inventory(&c).next_item;
    c.receive(destroy(24, 1)).unwrap();
    let out = step(&mut c);
    cache.apply(&out.events, c.player_guid());
    assert_eq!(cache.guid(24), selected);
    assert_eq!(cache.count(24), 1);
    assert_eq!(inventory(&c).slots[&23], first);
    c.receive(destroy(24, 255)).unwrap();
    let out = step(&mut c);
    cache.apply(&out.events, c.player_guid());
    assert_eq!(cache.guid(24), 0);
    assert_eq!(inventory(&c).slots[&23], first);
    assert_eq!(inventory(&c).next_item, allocator);
    c.receive(destroy(24, 0)).unwrap();
    let out = step(&mut c);
    assert!(out.events.iter().any(|e| matches!(
        e,
        SessionEvent::InventoryFailure {
            reason: 23,
            item_guid: 0,
            item2_guid: 0,
            ..
        }
    )));
    assert!(c
        .receive(ClientCommand::DestroyItem {
            bag_index: 0,
            slot: 24,
            count: 0
        })
        .is_err());
}

struct ProtectedCatalog;
impl ClientCatalog for ProtectedCatalog {
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
        LootCatalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        LootCatalog.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        LootCatalog.contains_spell(id)
    }
    fn player_power_type(&self, p: PlayerId) -> Option<u8> {
        LootCatalog.player_power_type(p)
    }
    fn player_bytes_2(&self, p: PlayerId) -> Option<u32> {
        LootCatalog.player_bytes_2(p)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        let mut info = LootCatalog.item_metadata(if entry == 70003 { 70001 } else { entry })?;
        if entry == 70003 {
            info.flags = inv::NO_USER_DESTROY;
        }
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

#[test]
fn protected_destroy_refuses_with_null_guids_and_preserves_checkpoint_inventory() {
    let h = Harness::new();
    let (mut data, input, mut q) = setup(0);
    // Explicit authored prototype policy; source/metadata rows agree before initialization.
    let catalog = &mut q.loot.as_mut().unwrap().inventory;
    let mut json = serde_json::to_value(&*catalog).unwrap();
    let mut protected = json["rows"]["70001"].clone();
    protected["entry"] = serde_json::json!(70003);
    protected["flags"] = serde_json::json!(inv::NO_USER_DESTROY);
    json["rows"]["70003"] = protected;
    *catalog = serde_json::from_value(json).unwrap();
    data.initialize_quest_progression(&input, q).unwrap();
    data.admin = Some(n::AdminPolicy {
        world: [8; 16],
        world_owner: Selection::from_profile(&h.profile, &h.character)
            .unwrap()
            .player
            .owner,
        allowlist: Default::default(),
        revision: 7,
    });
    let mut c = NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        Entry::Create {
            world: [8; 16],
            seed: 7,
        },
        data,
        &ProtectedCatalog,
        loot::LootAuthority::default(),
    )
    .unwrap();
    use benilla_ui::script::admin::AdminBackend;
    let backend = c.admin_backend();
    backend.submit(item_request(&backend, 1, 70003, 7)).unwrap();
    step(&mut c);
    assert_eq!(inventory(&c).slots[&24].item, 70003);
    let before = inventory(&c).clone();
    c.receive(destroy(24, 0)).unwrap();
    let out = step(&mut c);
    assert_eq!(inventory(&c), &before);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::InventoryFailure {
                    reason: 24,
                    item_guid: 0,
                    item2_guid: 0,
                    ..
                }
            ))
            .count(),
        1
    );
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::ObjectDestroyed(_) | SessionEvent::ItemPushResult(_)
    )));
}
#[test]
fn backpack_destroy_remains_available_after_real_player_death() {
    use benilla_sim::player_death as d;
    let h = Harness::new();
    let mut data = profile(7);
    data.npcs[0].max_health = 1000;
    let npc = data.npcs[0].combat.as_mut().unwrap();
    npc.minimum_damage = fixture::float(10000.);
    npc.maximum_damage = fixture::float(10000.);
    data.configure_player_death(
        d::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        d::DeathSupport::checked(&d::REQUIRED_ABSENCES.map(|x| (x, Some(false)))).unwrap(),
    )
    .unwrap();
    let mut c = enter(&h, data, false);
    c.receive(cast(&c)).unwrap();
    for _ in 0..200 {
        step(&mut c);
        if !c.world.state().player_alive() {
            break;
        }
    }
    assert!(!c.world.state().player_alive());
    let first = inventory(&c).slots[&23].clone();
    c.receive(destroy(24, 0)).unwrap();
    let out = step(&mut c);
    assert!(!inventory(&c).slots.contains_key(&24));
    assert_eq!(inventory(&c).slots[&23], first);
    assert_eq!(c.world.state().health, 0);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::InventoryFailure { .. })));
}
