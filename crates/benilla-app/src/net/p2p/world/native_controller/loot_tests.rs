// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual app loot packets and durable publication against authored source-shaped rows.
use super::*;
use benilla_sim::{inventory as inv, starter_init as init};
use benilla_world_runtime::native::synthetic_loot_fixture as loot;

struct LootCatalog;
impl ClientCatalog for LootCatalog {
    fn build(&self) -> u32 {
        Catalog.build()
    }
    fn manifest_digest(&self) -> runtime::Digest {
        Catalog.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        Catalog.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        Catalog.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        matches!(id, 70002 | 900002)
    }
    fn contains_spell(&self, id: u32) -> bool {
        Catalog.contains_spell(id)
    }
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        Catalog.player_power_type(player)
    }
    fn player_bytes_2(&self, player: PlayerId) -> Option<u32> {
        Catalog.player_bytes_2(player)
    }
    fn item_metadata(&self, entry: u32) -> Option<benilla_protocol::messages::ItemInfo> {
        if !matches!(entry, 70001 | 900001) {
            return None;
        }
        Some(benilla_protocol::messages::ItemInfo {
            class: if entry == 900001 { 4 } else { 15 },
            subclass: 0,
            name: format!("Authored item {entry}"),
            display_info_id: if entry == 900001 { 900002 } else { 70002 },
            quality: 1,
            flags: 0,
            buy_price: 0,
            sell_price: 0,
            inventory_type: if entry == 900001 { 5 } else { 0 },
            allowable_class: -1,
            allowable_race: -1,
            item_level: 0,
            required_level: u32::from(entry == 900001),
            required_skill: 0,
            required_skill_rank: 0,
            required_spell: 0,
            required_honor_rank: 0,
            required_city_rank: 0,
            required_rep_faction: 0,
            required_rep_rank: 0,
            max_count: 0,
            stackable: if entry == 900001 { 1 } else { 5 },
            container_slots: 0,
            stats: if entry == 900001 {
                vec![(5, 3), (6, 2)]
            } else {
                vec![]
            },
            damages: Vec::new(),
            dmg_min: 0.0,
            dmg_max: 0.0,
            dmg_type: 0,
            armor: if entry == 900001 { 7 } else { 0 },
            resistances: [0; 6],
            delay_ms: 0,
            ammo_type: 0,
            ranged_mod_range: 0.0,
            spells: Vec::new(),
            spell_charges_0: 0,
            use_spell: None,
            bonding: 0,
            description: "An original test token.".into(),
            page_text: 0,
            language_id: 0,
            page_material: 0,
            start_quest: 0,
            lock_id: 0,
            material: 0,
            sheath: 0,
            random_property: 0,
            block: 0,
            item_set: 0,
            max_durability: u32::from(entry == 900001),
            area: 0,
            map: 0,
            bag_family: 0,
        })
    }
    fn quest_metadata(&self, id: u32) -> Option<benilla_world_client::QuestMetadata> {
        Catalog.quest_metadata(id)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        Catalog.quest_greeting(id)
    }
    fn native_quest_greeting(&self, id: u32) -> Option<benilla_world_client::NativeQuestGreeting> {
        Catalog.native_quest_greeting(id)
    }
    fn native_quest_presentation(
        &self,
        id: u32,
    ) -> Option<benilla_world_client::NativeQuestPresentation> {
        Catalog.native_quest_presentation(id)
    }
}

fn setup(carried: u32) -> (n::Profile, init::StarterInput, n::QuestProgressionInput) {
    let (mut p, mut input, mut q) = loot::setup();
    p.npcs[0].ai.corpse_decay_ms = 60_000;
    p.npcs[0].ai.respawn_delay_seconds = 120;
    let row = &mut q.loot.as_mut().unwrap().captures[0].rows[0];
    row.chance_or_quest_chance_bits = 100f32.to_bits();
    row.min_count_or_reference = 7;
    row.max_count = 7;
    if carried > 0 {
        let mut item = input.items[0].clone();
        item.id = 70001;
        item.class = 15;
        item.inventory_type = 0;
        item.required_level = 0;
        item.stackable = 5;
        item.max_durability = 0;
        item.stats = [init::ItemStat { kind: 0, value: 0 }; 10];
        item.armor = 0;
        input.items.push(item);
        let init::Rows::Complete(grants) = &mut input.item_grants else {
            panic!("complete authored grants")
        };
        grants.push(init::ItemGrant {
            source: init::GrantSource::Custom,
            source_order: 1,
            item: 70001,
            count: carried,
        });
        p.initialize_player(&input, fixture::float(100.0), 1)
            .unwrap();
    }
    (p, input, q)
}
fn profile(carried: u32) -> n::Profile {
    let (mut p, input, q) = setup(carried);
    p.initialize_quest_progression(&input, q).unwrap();
    p
}
fn enter(h: &Harness, data: n::Profile, open: bool) -> NativeController {
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
        data,
        &LootCatalog,
        loot::LootAuthority::default(),
    )
    .unwrap()
}
fn corpse(c: &NativeController) -> u64 {
    c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap()
}
fn step(c: &mut NativeController) -> Pump {
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    assert_eq!(out.saved.as_ref().unwrap().tick, c.tick());
    out
}
fn kill(c: &mut NativeController) {
    c.receive(cast(c)).unwrap();
    for _ in 0..25 {
        step(c);
        if c.world.state().npcs[&2].health == 0 {
            return;
        }
    }
    panic!("authored creature did not die");
}
fn ready(h: &Harness, carried: u32) -> NativeController {
    let mut c = enter(h, profile(carried), false);
    c.initial_events().unwrap();
    kill(&mut c);
    c
}
fn open(c: &mut NativeController) -> Pump {
    c.receive(ClientCommand::Loot { guid: corpse(c) }).unwrap();
    let out = step(c);
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootResponse { .. })));
    out
}
fn progress(c: &NativeController) -> &n::QuestProgressionState {
    c.world.state().quest_progression.as_ref().unwrap()
}
fn inventory(c: &NativeController) -> &inv::State {
    progress(c).inventory.as_ref().unwrap()
}
fn inject(h: &Harness) {
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored loot save fault",
    )
    .unwrap();
}
fn repair(h: &Harness) {
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
}
fn no_loot_delivery(events: &[SessionEvent]) {
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::LootResponse { .. }
            | SessionEvent::LootRemoved { .. }
            | SessionEvent::LootMoneyNotify { .. }
            | SessionEvent::LootClearMoney
            | SessionEvent::ItemPushResult(_)
    )));
}
fn paused(out: &Pump) {
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.as_ref().unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
}

#[test]
fn stock_loot_panel_binds_only_after_durable_publication_and_item_query_uses_catalog() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    let guid = corpse(&c);
    assert!(c
        .receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .is_err());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    let before = c.world.state_digest();
    let query = c
        .receive(ClientCommand::ItemQuery {
            entry: 70001,
            guid: 0,
        })
        .unwrap();
    assert!(
        matches!(&query.events[..], [SessionEvent::ItemTemplate { entry: 70001, info: Some(info) }] if **info == LootCatalog.item_metadata(70001).unwrap())
    );
    assert_eq!(c.world.state_digest(), before);
    assert!(c.actions.is_empty());
    c.receive(ClientCommand::Loot { guid }).unwrap();
    assert!(matches!(
        c.receive(ClientCommand::Loot { guid }),
        Err(Error::Backpressure)
    ));
    assert!(matches!(
        c.receive(ClientCommand::LootMoney),
        Err(Error::Backpressure)
    ));
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert!(c.loot_window.is_none());
    assert_eq!(progress(&c).loot_target, Some(c.world.state().npcs[&2].id));
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    repair(&h);
    c.retry_checkpoint().unwrap();
    assert!(c.loot_window.is_none());
    assert!(matches!(
        c.receive(ClientCommand::LootMoney),
        Err(Error::Paused)
    ));
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(c.loot_window.unwrap().guid, guid);
    assert!(out.events.iter().any(|e| matches!(e, SessionEvent::LootResponse { guid: source, items, .. } if *source == guid && items.len() == 1 && items[0].count == 7)));
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().next_sequence, sequence);
    assert!(c.pump(Duration::ZERO).events.is_empty());
}

#[test]
fn double_click_item_and_money_commit_once_with_source_ordered_stock_results() {
    let h = Harness::new();
    let mut c = ready(&h, 4);
    open(&mut c);
    let money = c.world.state().npcs[&2]
        .corpse_loot
        .as_ref()
        .unwrap()
        .money();
    let old_money = progress(&c).money;
    for _ in 0..2 {
        c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
            .unwrap();
    }
    let mut out = step(&mut c);
    assert!(c.loot_window.is_none()); // Source duplicate-slot handling closes this panel.
    open(&mut c);
    for _ in 0..2 {
        c.receive(ClientCommand::LootMoney).unwrap();
    }
    out.events.extend(step(&mut c).events);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 11);
    assert_eq!(
        inventory(&c)
            .slots
            .values()
            .map(|i| i.count)
            .collect::<Vec<_>>(),
        [5, 5, 1]
    );
    assert_eq!(progress(&c).money, old_money + money);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootRemoved { slot: 0 }))
            .count(),
        1
    );
    assert_eq!(out.events.iter().filter(|e| matches!(e, SessionEvent::ItemPushResult(p) if p.item_entry == 70001 && p.count == 7)).count(), 1);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootMoneyNotify { .. })));
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootClearMoney))
            .count(),
        1
    );
    let removed = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::LootRemoved { .. }))
        .unwrap();
    let item = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    let coin = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::LootClearMoney))
        .unwrap();
    assert!(removed < item && item < coin);
    let digest = c.world.state_digest();
    drop(c);
    let mut c = enter(&h, profile(4), true);
    assert_eq!(c.world.state_digest(), digest);
    let initial = c.initial_events().unwrap();
    no_loot_delivery(&initial);
    assert_eq!(
        initial
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemCreate { .. }))
            .count(),
        4
    ); // Three carried instances and one equipped robe.
    assert!(c.loot_window.is_none());
    assert!(c
        .receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .is_err());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 11);
}

#[test]
fn collected_item_and_money_wait_for_save_retry_without_reexecution_or_duplicate_delivery() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    let money = c.world.state().npcs[&2]
        .corpse_loot
        .as_ref()
        .unwrap()
        .money();
    let before_money = progress(&c).money;
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    c.receive(ClientCommand::LootMoney).unwrap();
    inject(&h);
    let before_generation = c.store.generation();
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert_eq!(c.store.generation(), before_generation);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
    assert_eq!(progress(&c).money, before_money + money);
    assert!(c.pending.is_none());
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    let tick = c.tick();
    assert!(c.retry_tick().is_err());
    assert!(c.retry_checkpoint().is_err());
    assert!(c.pump(Duration::from_secs(600)).events.is_empty());
    assert_eq!(c.tick(), tick);
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemPushResult(_)))
            .count(),
        1
    );
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootClearMoney))
            .count(),
        1
    );
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootMoneyNotify { .. })));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let mut c = enter(&h, profile(0), true);
    assert_eq!(c.world.state_digest(), digest);
    no_loot_delivery(&c.initial_events().unwrap());
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
}

#[test]
fn full_backpack_refuses_whole_loot_row_without_partial_merge_or_removal() {
    for carried in [79, 80] {
        let h = Harness::new();
        let mut c = ready(&h, carried);
        open(&mut c);
        let before = inventory(&c).clone();
        c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
            .unwrap();
        let out = step(&mut c);
        assert_eq!(inventory(&c), &before);
        assert!(out
            .events
            .iter()
            .any(|e| matches!(e, SessionEvent::InventoryFailure { reason: 50, .. })));
        no_loot_delivery(&out.events);
        assert_eq!(
            c.world.state().npcs[&2]
                .corpse_loot
                .as_ref()
                .unwrap()
                .items()
                .count(),
            1
        );
        let guid = corpse(&c);
        c.receive(ClientCommand::LootRelease { guid }).unwrap();
        step(&mut c);
        let out = open(&mut c);
        assert!(out.events.iter().any(|e| matches!(e, SessionEvent::LootResponse { items, .. } if items.len() == 1 && items[0].count == 7)));
        c.shutdown().unwrap();
        drop(c);
        let c = enter(&h, profile(carried), true);
        assert_eq!(inventory(&c), &before);
    }
}

#[test]
fn close_reopen_and_new_pending_panel_cannot_retarget_guidless_clicks_or_release() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    let guid = corpse(&c);
    let captured = c.loot_window.unwrap();
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    c.receive(ClientCommand::Loot { guid }).unwrap();
    for command in [
        ClientCommand::AutostoreLootItem { slot: 0 },
        ClientCommand::LootMoney,
        ClientCommand::LootRelease { guid },
    ] {
        assert!(matches!(c.receive(command), Err(Error::Backpressure)));
    }
    assert_eq!(c.loot_window, Some(captured));
    assert!(
        matches!(&c.actions[0], Intent::Command { command: n::Command::LootItem { corpse, slot: 0 }, .. } if *corpse == captured.corpse.id)
    );
    step(&mut c);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
    assert!(c
        .receive(ClientCommand::LootRelease { guid: guid + 1 })
        .is_err());
    assert_eq!(c.loot_window, Some(captured));
    c.receive(ClientCommand::LootRelease { guid }).unwrap();
    assert!(c.loot_window.is_none());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    c.receive(ClientCommand::Loot { guid }).unwrap();
    assert!(matches!(
        c.receive(ClientCommand::LootMoney),
        Err(Error::Backpressure)
    ));
    let out = step(&mut c);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootResponse { .. })));
    assert_eq!(c.loot_window, Some(captured));
    c.receive(ClientCommand::LootMoney).unwrap();
    step(&mut c);
}

#[test]
fn failed_final_save_keeps_committed_loot_and_flushes_once_on_shutdown_retry() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    inject(&h);
    paused(&c.pump(c.tick_duration()));
    assert!(c.shutdown().is_err());
    assert!(c.closing());
    assert!(!c.closed());
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
    repair(&h);
    let (_, cancelled, events) = c.shutdown().unwrap();
    assert_eq!(cancelled, 0);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemPushResult(_)))
            .count(),
        1
    );
    assert!(c.closed());
    assert!(c.shutdown().is_err());
    drop(c);
    let mut c = enter(&h, profile(0), true);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
    assert!(c.loot_window.is_none());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    let initial = c.initial_events().unwrap();
    assert!(initial
        .iter()
        .any(|e| matches!(e, SessionEvent::LootResponse { .. })));
    assert!(!initial
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemPushResult(_))));
    assert!(c.loot_window.is_some());
    c.receive(ClientCommand::LootMoney).unwrap();
    step(&mut c);
}

#[test]
fn corpse_despawn_and_respawn_retire_published_guid_and_old_rights() {
    let h = Harness::new();
    let (mut p, input, mut q) = setup(0);
    q.loot
        .as_mut()
        .unwrap()
        .creatures
        .iter_mut()
        .find(|row| row.entry == 60001)
        .unwrap()
        .corpse_acceleration_decay_ms = 500;
    p.npcs[0].ai.corpse_decay_ms = 500;
    p.npcs[0].ai.respawn_delay_seconds = 2;
    p.initialize_quest_progression(&input, q).unwrap();
    let mut c = enter(&h, p, false);
    c.initial_events().unwrap();
    kill(&mut c);
    let old = c.world.state().npcs[&2].id;
    let old_guid = corpse(&c);
    open(&mut c);
    let before = inventory(&c).clone();
    let money = progress(&c).money;
    // Inspecting extends the authored corpse timer to 500 ms; respawn is two seconds.
    for _ in 0..40 {
        step(&mut c);
        if c.world.state().npcs[&2].id != old {
            break;
        }
    }
    assert_ne!(c.world.state().npcs[&2].id, old);
    assert_ne!(corpse(&c), old_guid);
    assert!(c.loot_window.is_none());
    assert!(progress(&c).loot_target.is_none());
    assert!(c.receive(ClientCommand::Loot { guid: old_guid }).is_err());
    assert!(c
        .receive(ClientCommand::LootRelease { guid: old_guid })
        .is_err());
    assert!(c
        .receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .is_err());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    assert_eq!(inventory(&c), &before);
    assert_eq!(progress(&c).money, money);
}

#[test]
fn death_can_close_existing_panel_and_later_guidless_clicks_cannot_retarget() {
    let h = Harness::new();
    let (mut p, input, q) = setup(0);
    p.npcs[1].ai.reaction = benilla_sim::creature_ai::Reaction::Defensive;
    let combat = p.npcs[1].combat.as_mut().unwrap();
    combat.minimum_damage = fixture::float(10000.0);
    combat.maximum_damage = fixture::float(10000.0);
    p.configure_player_death(
        benilla_sim::player_death::DeathConfig {
            map_instanceable: false,
            pve_reclaim_delay_enabled: true,
        },
        benilla_sim::player_death::DeathSupport::checked(
            &benilla_sim::player_death::REQUIRED_ABSENCES.map(|d| (d, Some(false))),
        )
        .unwrap(),
    )
    .unwrap();
    p.initialize_quest_progression(&input, q).unwrap();
    let mut c = enter(&h, p, false);
    c.initial_events().unwrap();
    kill(&mut c);
    open(&mut c);
    let target = c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 60001,
        target: Some(target),
    })
    .unwrap();
    for _ in 0..100 {
        step(&mut c);
        if c.world.state().health == 0 {
            break;
        }
    }
    assert_eq!(c.world.state().health, 0);
    let guid = corpse(&c);
    c.receive(ClientCommand::LootRelease { guid }).unwrap();
    assert!(c.loot_window.is_none());
    let before = inventory(&c).clone();
    let money = progress(&c).money;
    assert!(c
        .receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .is_err());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    assert_eq!(inventory(&c), &before);
    assert_eq!(progress(&c).money, money);
}

#[test]
fn queue_pressure_does_not_close_a_panel_when_release_was_not_admitted() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    let before = c.loot_window;
    let guid = corpse(&c);
    for _ in 0..MAX_ACTIONS {
        c.receive(ClientCommand::CastSpell {
            spell_id: 60002,
            target: Some(guid),
        })
        .unwrap();
    }
    assert!(matches!(
        c.receive(ClientCommand::LootRelease { guid }),
        Err(Error::Backpressure)
    ));
    assert_eq!(c.loot_window, before);
    assert_eq!(c.actions.len(), MAX_ACTIONS);
}

#[test]
fn switching_corpses_preserves_queued_identity_and_rejects_old_panel_release() {
    let h = Harness::new();
    let (mut p, input, q) = setup(0);
    p.npcs[1].max_health = 10;
    p.npcs[1].ai.corpse_decay_ms = 60_000;
    p.initialize_quest_progression(&input, q).unwrap();
    let mut c = enter(&h, p, false);
    c.initial_events().unwrap();
    kill(&mut c);
    let first = c.world.state().npcs[&2].id;
    let first_guid = corpse(&c);
    let second = c.world.state().npcs[&3].id;
    let second_guid = c.projection.npc_guid(second).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 60001,
        target: Some(second_guid),
    })
    .unwrap();
    for _ in 0..25 {
        step(&mut c);
        if c.world.state().npcs[&3].health == 0 {
            break;
        }
    }
    assert_eq!(c.world.state().npcs[&3].health, 0);
    open(&mut c);
    let first_money = c.world.state().npcs[&2]
        .corpse_loot
        .as_ref()
        .unwrap()
        .money();
    let second_money = c.world.state().npcs[&3]
        .corpse_loot
        .as_ref()
        .unwrap()
        .money();
    let before_money = progress(&c).money;
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    c.receive(ClientCommand::LootMoney).unwrap();
    c.receive(ClientCommand::Loot { guid: second_guid })
        .unwrap();
    assert!(matches!(
        c.receive(ClientCommand::LootMoney),
        Err(Error::Backpressure)
    ));
    assert!(matches!(
        c.receive(ClientCommand::LootRelease { guid: first_guid }),
        Err(Error::Backpressure)
    ));
    assert!(
        matches!(&c.actions[0], Intent::Command { command: n::Command::LootItem { corpse, .. }, .. } if *corpse == first)
    );
    assert!(
        matches!(&c.actions[1], Intent::Command { command: n::Command::LootMoney { corpse }, .. } if *corpse == first)
    );
    step(&mut c);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
    assert_eq!(progress(&c).money, before_money + first_money);
    assert_eq!(progress(&c).loot_target, Some(second));
    assert_eq!(c.loot_window.unwrap().corpse.id, second);
    assert!(c
        .receive(ClientCommand::LootRelease { guid: first_guid })
        .is_err());
    assert_eq!(c.loot_window.unwrap().corpse.id, second);
    c.receive(ClientCommand::LootMoney).unwrap();
    step(&mut c);
    assert_eq!(
        progress(&c).money,
        before_money + first_money + second_money
    );
    assert!(c.loot_window.is_none());
}

#[test]
fn initial_player_overlay_keeps_complete_state_with_carried_items() {
    let h = Harness::new();
    let mut c = enter(&h, profile(4), false);
    let fields = c.initial_player_fields().unwrap();
    assert_eq!(fields.unit_health(), Some(c.world.state().health));
    assert_eq!(fields.unit_level(), Some(u32::from(c.world.player_level())));
    assert_eq!(fields.player_money(), Some(progress(&c).money));
    assert!(fields.player_inv_slot(4).is_some_and(|guid| guid != 0));
    let carried = fields
        .player_pack_slot(0)
        .filter(|guid| *guid != 0)
        .unwrap();
    assert_eq!(fields.player_pack_slot(1), Some(0));
    let events = c.initial_events().unwrap();
    assert!(events.iter().any(|e| matches!(e, SessionEvent::ItemCreate { guid, fields, .. } if *guid == carried && fields.item_stack_count() == Some(4))));
    let first_player = events
        .iter()
        .find_map(|e| match e {
            SessionEvent::ObjectValues { guid, fields } if *guid == c.player_guid() => Some(fields),
            _ => None,
        })
        .unwrap();
    assert_eq!(first_player.unit_health(), fields.unit_health());
    assert_eq!(first_player.player_money(), fields.player_money());
    assert_eq!(first_player.player_pack_slot(0), Some(carried));
}

#[test]
fn looting_flag_is_durable_on_open_restore_and_explicit_release_retry() {
    fn flags(events: &[SessionEvent], player: u64) -> u32 {
        let mut out = benilla_protocol::ObjectFields::from_pairs(&[]);
        for event in events {
            if let SessionEvent::ObjectValues { guid, fields } = event {
                if *guid == player {
                    out.merge(fields.clone());
                }
            }
        }
        out.unit_flags()
    }
    let h = Harness::new();
    let mut c = ready(&h, 0);
    assert_eq!(c.initial_player_fields().unwrap().unit_flags() & 0x400, 0);
    let opened = open(&mut c);
    assert_eq!(flags(&opened.events, c.player_guid()) & 0x400, 0x400);
    c.shutdown().unwrap();
    drop(c);
    let mut c = enter(&h, profile(0), true);
    assert_eq!(
        c.initial_player_fields().unwrap().unit_flags() & 0x400,
        0x400
    );
    let initial = c.initial_events().unwrap();
    assert_eq!(flags(&initial, c.player_guid()) & 0x400, 0x400);
    // The fresh stock latch releases this unsolicited corpse response.
    let guid = corpse(&c);
    c.receive(ClientCommand::LootRelease { guid }).unwrap();
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert!(out.events.is_empty());
    assert!(progress(&c).loot_target.is_none());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let released = c.pump(Duration::ZERO);
    assert_eq!(flags(&released.events, c.player_guid()) & 0x400, 0);
    assert!(!released
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    let reopened = open(&mut c);
    assert_eq!(flags(&reopened.events, c.player_guid()) & 0x400, 0x400);
}

#[path = "quest_item_reward_tests.rs"]
mod quest_item_reward_tests;

#[test]
fn missing_slot_release_waits_for_save_then_retires_rights_without_release_packet() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    let window = c.loot_window;
    c.receive(ClientCommand::AutostoreLootItem { slot: 255 })
        .unwrap();
    inject(&h);
    paused(&c.pump(c.tick_duration()));
    assert_eq!(c.loot_window, window);
    assert!(progress(&c).loot_target.is_none());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.events.iter().filter(|e| matches!(e, SessionEvent::InventoryFailure { reason, bag_slot: 0, .. } if *reason == inv::InventoryResult::ItemNotFound.protocol())).count(), 1);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
    assert!(c.loot_window.is_none());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 0);
}

#[test]
fn item_last_automatic_release_publishes_once_after_save_retry() {
    let h = Harness::new();
    let mut c = ready(&h, 0);
    open(&mut c);
    c.receive(ClientCommand::LootMoney).unwrap();
    let money = step(&mut c);
    assert!(!money
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
    assert!(c.loot_window.is_some());
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    inject(&h);
    paused(&c.pump(c.tick_duration()));
    assert!(c.loot_window.is_some());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    let pushed = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    let released = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::LootReleaseResponse { result: 1, .. }))
        .unwrap();
    assert!(pushed < released);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootReleaseResponse { .. }))
            .count(),
        1
    );
    assert!(c.loot_window.is_none());
    assert!(c
        .receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .is_err());
    assert!(c.pump(Duration::ZERO).events.is_empty());
}

#[test]
fn queued_item_last_and_explicit_release_reduce_lifetime_then_exhausted_reopen_has_no_rights() {
    let h = Harness::new();
    let (mut p, input, mut q) = setup(0);
    p.npcs[0].ai.corpse_decay_ms = 20_000;
    for row in &mut q.loot.as_mut().unwrap().creatures {
        row.minimum_money = 0;
        row.maximum_money = 0;
        row.corpse_acceleration_decay_ms = 500;
    }
    p.initialize_quest_progression(&input, q).unwrap();
    let mut c = enter(&h, p, false);
    c.initial_events().unwrap();
    kill(&mut c);
    open(&mut c);
    let guid = corpse(&c);
    c.receive(ClientCommand::AutostoreLootItem { slot: 0 })
        .unwrap();
    c.receive(ClientCommand::LootRelease { guid }).unwrap();
    assert!(c.loot_window.is_none());
    assert!(c.receive(ClientCommand::LootRelease { guid }).is_err());
    inject(&h);
    paused(&c.pump(c.tick_duration()));
    let npc = &c.world.state().npcs[&2];
    assert!(npc.corpse_loot.as_ref().unwrap().looted);
    let deadline = npc.ai.corpse_expiration_ms();
    assert_eq!(deadline, c.world.state().now_ms + 500);
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemPushResult(_)))
            .count(),
        1
    );
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootReleaseResponse { result: 1, .. }))
            .count(),
        1
    );
    assert!(c.loot_window.is_none());
    assert!(c.pump(Duration::ZERO).events.is_empty());

    // ShowContentTo on an originally nonempty, now exhausted list releases
    // immediately. There is no new panel, inspection extension or collection right.
    c.receive(ClientCommand::Loot { guid }).unwrap();
    assert!(matches!(
        c.receive(ClientCommand::LootMoney),
        Err(Error::Backpressure)
    ));
    let out = step(&mut c);
    assert_eq!(out.events.iter().filter(|e|matches!(e,SessionEvent::LootReleaseResponse {guid:source,result:1} if *source==guid)).count(),1);
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::LootResponse { .. } | SessionEvent::ItemPushResult(_)
    )));
    assert_eq!(c.world.state().npcs[&2].ai.corpse_expiration_ms(), deadline);
    assert!(c.loot_window.is_none());
    assert!(progress(&c).loot_target.is_none());
    assert!(c.receive(ClientCommand::LootMoney).is_err());
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 7);
}
