// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored item/loot packets only, not installed-game data or production admission.
use super::*;
use benilla_sim::starter_init as init;
use n::synthetic_loot_fixture as fixture;
struct Catalog {
    quest: quests::tests::Catalog,
    items: BTreeMap<u32, p::ItemInfo>,
    display: bool,
    spell_queries: BTreeMap<u32, crate::NativeItemSpellQuery>,
}
fn metadata(item: &inv::Item) -> p::ItemInfo {
    p::ItemInfo {
        class: item.class,
        subclass: 0,
        name: format!("Authored item {}", item.entry),
        display_info_id: item.display_id,
        quality: item.quality,
        flags: item.flags,
        buy_price: 0,
        sell_price: 0,
        inventory_type: item.inventory_type,
        allowable_class: -1,
        allowable_race: -1,
        item_level: 0,
        required_level: 0,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_city_rank: 0,
        required_rep_faction: 0,
        required_rep_rank: 0,
        max_count: item.maximum_count as u32,
        stackable: item.stackable as u32,
        container_slots: item.container_slots,
        stats: vec![],
        damages: vec![],
        dmg_min: 0.0,
        dmg_max: 0.0,
        dmg_type: 0,
        armor: 0,
        resistances: [0; 6],
        delay_ms: 0,
        ammo_type: 0,
        ranged_mod_range: 0.0,
        spells: item
            .spells
            .iter()
            .enumerate()
            .filter(|(_, s)| s.id != 0)
            .map(|(index, s)| p::ItemSpellEntry {
                index: index as u8,
                spell_id: s.id,
                trigger: s.trigger,
                charges: s.charges,
                cooldown_ms: s.cooldown,
                category: s.category,
                category_cooldown_ms: s.category_cooldown,
            })
            .collect(),
        spell_charges_0: item.spells[0].charges,
        use_spell: item
            .spells
            .iter()
            .find(|s| s.id != 0 && s.trigger == 0)
            .map(|s| p::ItemUseSpell {
                spell_id: s.id,
                cooldown_ms: s.cooldown,
                category: s.category,
                category_cooldown_ms: s.category_cooldown,
            }),
        bonding: item.bonding,
        description: "Authored item description | only".into(),
        page_text: 0,
        language_id: 0,
        page_material: 0,
        start_quest: item.start_quest,
        lock_id: item.lock_id,
        material: 0,
        sheath: 0,
        random_property: item.random_property,
        block: 0,
        item_set: item.item_set,
        max_durability: item.maximum_durability,
        area: item.area,
        map: item.map,
        bag_family: item.bag_family,
    }
}
impl Catalog {
    fn new(w: &n::NativeWorld) -> Self {
        let genesis = w
            .profile()
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap();
        let items = genesis
            .inventory
            .items()
            .map(|item| {
                let mut info = metadata(item);
                if let Some(source) = genesis.starter_items.get(&item.entry) {
                    info.subclass = source.subclass;
                    info.allowable_class = source.allowable_class as i32;
                    info.allowable_race = source.allowable_race as i32;
                    info.required_level = source.required_level;
                    info.required_skill = u32::from(source.required_skill);
                    info.required_skill_rank = source.required_skill_rank;
                    info.required_spell = source.required_spell;
                    info.required_honor_rank = source.required_honor_rank;
                    info.required_rep_faction = source.required_reputation_faction;
                    info.stats = source
                        .stats
                        .iter()
                        .filter(|s| s.kind != 0 || s.value != 0)
                        .map(|s| (s.kind, s.value))
                        .collect();
                    info.damages = source
                        .damage
                        .iter()
                        .filter(|d| f32::from_bits(d.max_bits) > 0.0)
                        .map(|d| p::ItemDamage {
                            min: f32::from_bits(d.min_bits),
                            max: f32::from_bits(d.max_bits),
                            school: d.school,
                        })
                        .collect();
                    info.dmg_min = f32::from_bits(source.damage[0].min_bits);
                    info.dmg_max = f32::from_bits(source.damage[0].max_bits);
                    info.dmg_type = source.damage[0].school;
                    info.armor = source.armor;
                    info.resistances = source.resistances.map(|r| r as i32);
                    info.block = source.block;
                    info.delay_ms = source.delay_ms;
                }
                (item.entry, info)
            })
            .collect();
        Self {
            quest: quests::tests::Catalog::new(w),
            items,
            display: true,
            spell_queries: genesis
                .inventory
                .items()
                .flat_map(|i| i.spells)
                .filter(|s| s.id != 0)
                .map(|s| {
                    (
                        s.id,
                        crate::NativeItemSpellQuery::Present {
                            recovery_time: 9000,
                            category: 90,
                            category_recovery_time: 9100,
                        },
                    )
                })
                .collect(),
        }
    }
}
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        self.quest.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        self.quest.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        self.display && self.items.values().any(|i| i.display_info_id == id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        self.quest.contains_spell(id)
    }
    fn player_power_type(&self, id: PlayerId) -> Option<u8> {
        self.quest.player_power_type(id)
    }
    fn player_bytes_2(&self, id: PlayerId) -> Option<u32> {
        self.quest.player_bytes_2(id)
    }
    fn native_item_spell_query(&self, spell: u32) -> Option<crate::NativeItemSpellQuery> {
        self.spell_queries.get(&spell).copied()
    }
    fn item_metadata(&self, id: u32) -> Option<p::ItemInfo> {
        self.items.get(&id).cloned()
    }
    fn quest_metadata(&self, id: u32) -> Option<crate::QuestMetadata> {
        self.quest.quest_metadata(id)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        self.quest.quest_greeting(id)
    }
    fn native_quest_greeting(&self, id: u32) -> Option<crate::NativeQuestGreeting> {
        self.quest.native_quest_greeting(id)
    }
    fn native_quest_presentation(&self, id: u32) -> Option<crate::NativeQuestPresentation> {
        self.quest.native_quest_presentation(id)
    }
}
fn build(rows: &[u8], extra_equipped: u32) -> n::NativeWorld {
    build_collect(rows, extra_equipped, false)
}
fn build_collect(rows: &[u8], extra_equipped: u32, collect: bool) -> n::NativeWorld {
    let (mut profile, mut input, mut q) = fixture::setup();
    let init::Rows::Complete(grants) = &mut input.item_grants else {
        unreachable!()
    };
    grants[0].count += extra_equipped;
    profile
        .initialize_player(&input, n::synthetic_fixture::float(100.0), 1)
        .unwrap();
    let loot = q.loot.as_mut().unwrap();
    let capture = &mut loot.captures[0];
    let row = capture.rows[0].clone();
    capture.rows = rows
        .iter()
        .map(|count| {
            let mut row = row.clone();
            row.chance_or_quest_chance_bits = 100f32.to_bits();
            row.min_count_or_reference = i32::from(*count);
            row.max_count = u32::from(*count);
            row
        })
        .collect();
    capture.loader_order.result_row_count = rows.len() as u64;
    capture.loader_order.selected_row_positions = (0..rows.len() as u64).collect();
    profile.npcs[0].ai.corpse_decay_ms = 60_000;
    if collect {
        q.quests[0].special_flags |= benilla_sim::quest_lifecycle::DELIVER;
        q.quests[0].required_item[0] = 70001;
        q.quests[0].required_item_count[0] = 6;
    }
    profile.initialize_quest_progression(&input, q).unwrap();
    n::NativeWorld::synthetic_component_test(profile, [8; 16], n::synthetic_fixture::player(), 42)
        .unwrap()
}
fn setup(
    rows: &[u8],
    extra_equipped: u32,
) -> (n::NativeWorld, NativeProjection, fixture::LootAuthority) {
    let w = build(rows, extra_equipped);
    let p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    (w, p, fixture::LootAuthority::default())
}
fn step(
    w: &mut n::NativeWorld,
    p: &mut NativeProjection,
    a: &mut fixture::LootAuthority,
    requests: Vec<UiRequest>,
) -> Vec<SessionEvent> {
    let inputs = requests
        .into_iter()
        .enumerate()
        .map(|(index, r)| {
            let mut input = p.local(w, r).unwrap();
            input.input.sequence += index as u64;
            input
        })
        .collect();
    w.step_batch(w.state().tick + 1, inputs, a).unwrap();
    p.advance(w).unwrap()
}
fn corpse(w: &n::NativeWorld) -> NpcHandle {
    NpcHandle {
        id: w.state().npcs[&2].id,
    }
}
fn kill(
    w: &mut n::NativeWorld,
    p: &mut NativeProjection,
    a: &mut fixture::LootAuthority,
) -> Vec<SessionEvent> {
    let target = corpse(w);
    let mut events = step(
        w,
        p,
        a,
        vec![UiRequest::CastAtNpc {
            ability: 60001,
            target,
        }],
    );
    for _ in 0..25 {
        if w.state().npcs[&2].health == 0 {
            return events;
        }
        events.extend(step(w, p, a, vec![]));
    }
    panic!("authored corpse did not die")
}
fn player_fields(events: &[SessionEvent], guid: u64) -> ObjectFields {
    let mut out = ObjectFields::from_pairs(&[]);
    for e in events {
        if let SessionEvent::ObjectValues { guid: g, fields } = e {
            if *g == guid {
                out.merge(fields.clone());
            }
        }
    }
    out
}
#[test]
fn item_query_requires_bound_complete_metadata() {
    let w = build(&[2], 0);
    let catalog = Catalog::new(&w);
    let p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
    let before = w.state_digest();
    assert!(
        matches!(p.item_query(&w,70001).unwrap(),SessionEvent::ItemTemplate{entry:70001,info:Some(ref i)} if i.stackable==5&&i.description=="Authored item description || only")
    );
    assert!(matches!(
        p.item_query(&w, 999),
        Err(Error::MissingItemMetadata(999))
    ));
    assert_eq!(w.state_digest(), before);
    let mutations: Vec<fn(&mut p::ItemInfo)> = vec![
        |i| i.class += 1,
        |i| i.inventory_type += 1,
        |i| i.display_info_id += 1,
        |i| i.quality += 1,
        |i| i.flags += 1,
        |i| i.stackable += 1,
        |i| i.max_count += 1,
        |i| i.bonding += 1,
        |i| i.max_durability += 1,
        |i| i.bag_family += 1,
        |i| i.container_slots += 1,
        |i| i.random_property += 1,
        |i| i.start_quest += 1,
        |i| i.item_set += 1,
        |i| i.area += 1,
        |i| i.map += 1,
        |i| i.lock_id += 1,
        |i| i.spell_charges_0 += 1,
        |i| i.name = "\0".into(),
        |i| i.description = "\0".into(),
        |i| i.dmg_min = f32::NAN,
    ];
    for mutate in mutations {
        let mut c = Catalog::new(&w);
        mutate(c.items.get_mut(&70001).unwrap());
        assert!(matches!(
            NativeProjection::new(&w, w.state().player, &c),
            Err(Error::InvalidItemMetadata(70001))
        ));
    }
    let starter_mutations: Vec<fn(&mut p::ItemInfo)> = vec![
        |i| i.subclass += 1,
        |i| i.allowable_class = 1,
        |i| i.allowable_race = 1,
        |i| i.required_level += 1,
        |i| i.required_skill += 1,
        |i| i.required_skill_rank += 1,
        |i| i.required_spell += 1,
        |i| i.required_honor_rank += 1,
        |i| i.required_rep_faction += 1,
        |i| i.stats[0].1 += 1,
        |i| {
            i.damages.push(p::ItemDamage {
                min: 1.0,
                max: 2.0,
                school: 0,
            })
        },
        |i| i.dmg_min = 1.0,
        |i| i.dmg_max = 2.0,
        |i| i.dmg_type = 1,
        |i| i.armor += 1,
        |i| i.resistances[0] += 1,
        |i| i.block += 1,
        |i| i.delay_ms += 1,
    ];
    for mutate in starter_mutations {
        let mut c = Catalog::new(&w);
        mutate(c.items.get_mut(&900001).unwrap());
        assert!(matches!(
            NativeProjection::new(&w, w.state().player, &c),
            Err(Error::InvalidItemMetadata(900001))
        ));
    }
    let mut c = Catalog::new(&w);
    c.items.remove(&70001);
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &c),
        Err(Error::MissingItemMetadata(70001))
    ));
    let mut c = Catalog::new(&w);
    c.display = false;
    assert!(matches!(
        NativeProjection::new(&w, w.state().player, &c),
        Err(Error::UnboundItemDisplay(_))
    ));
}
#[test]
fn initial_equipment_and_carried_copies_have_distinct_stable_guids() {
    let (w, p, _) = setup(&[2], 2);
    let initial = p.initial_events(&w).unwrap();
    let items = initial
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ItemCreate { guid, fields, .. } => Some((*guid, fields)),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(items.len(), 3);
    assert_eq!(
        items.iter().map(|(g, _)| *g).collect::<BTreeSet<_>>().len(),
        3
    );
    let fields = player_fields(&initial, p.player_guid());
    assert_eq!(fields.player_inv_slot(4), Some(items[0].0));
    assert!(fields.player_pack_slot(0).is_some_and(|g| g != items[0].0));
    assert!(fields.player_pack_slot(1).is_some_and(|g| g != items[0].0));
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let projection =
        NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
            .unwrap();
    let restored_events = projection.initial_events(&restored).unwrap();
    assert_eq!(
        items.iter().map(|(g, _)| *g).collect::<Vec<_>>(),
        restored_events
            .iter()
            .filter_map(|e| if let SessionEvent::ItemCreate { guid, .. } = e {
                Some(*guid)
            } else {
                None
            })
            .collect::<Vec<_>>()
    );
}
#[test]
fn loot_windows_money_and_split_stack_pushes_use_stock_packets_and_source_order() {
    let (mut w, mut p, mut a) = setup(&[4, 4], 0);
    let killed = kill(&mut w, &mut p, &mut a);
    assert!(killed
        .iter()
        .any(|e| matches!(e,SessionEvent::ObjectValues{fields,..} if fields.unit_lootable())));
    let target = corpse(&w);
    let money = w.state().npcs[&2].corpse_loot.as_ref().unwrap().money();
    let events = step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    assert!(
        matches!(events.iter().find(|e|matches!(e,SessionEvent::LootResponse{..})),Some(SessionEvent::LootResponse{gold,items,..}) if *gold==money&&items.len()==2)
    );
    let events = step(
        &mut w,
        &mut p,
        &mut a,
        vec![
            UiRequest::TakeLootItem { target, slot: 0 },
            UiRequest::TakeLootItem { target, slot: 1 },
            UiRequest::TakeLootMoney(target),
        ],
    );
    let pushes = events
        .iter()
        .filter_map(|e| {
            if let SessionEvent::ItemPushResult(p) = e {
                Some(p)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(pushes.len(), 2);
    assert_eq!(pushes[0].item_slot, 23);
    assert_eq!(pushes[1].item_slot, u32::MAX);
    assert!(pushes
        .iter()
        .all(|p| p.count == 4 && p.bag_slot == 255 && !p.from_npc && !p.created));
    let removed = events
        .iter()
        .position(|e| matches!(e, SessionEvent::LootRemoved { slot: 0 }))
        .unwrap();
    let pushed = events
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    assert!(removed < pushed);
    let first = events
        .iter()
        .find_map(|e| {
            if let SessionEvent::ItemCreate { guid, fields, .. } = e {
                if fields.item_stack_count() == Some(4) {
                    Some(*guid)
                } else {
                    None
                }
            } else {
                None
            }
        })
        .unwrap();
    assert!(events.iter().any(|e|matches!(e,SessionEvent::ObjectValues{guid,fields} if *guid==first&&fields.item_stack_count()==Some(5))));
    assert!(events.iter().any(
        |e| matches!(e,SessionEvent::ItemCreate{fields,..} if fields.item_stack_count()==Some(3))
    ));
    assert_eq!(
        w.state()
            .quest_progression
            .as_ref()
            .unwrap()
            .inventory
            .as_ref()
            .unwrap()
            .item_count(70001)
            .unwrap(),
        8
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootClearMoney)));
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootMoneyNotify { .. })));
    assert_eq!(
        player_fields(&events, p.player_guid()).player_money(),
        Some(11 + money)
    );
    // SendGold calls Release after unregistering the looter: no release response.
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
}
#[test]
fn full_backpack_failure_never_removes_loot_or_announces_an_item() {
    let (mut w, mut p, mut a) = setup(&[2], 16);
    kill(&mut w, &mut p, &mut a);
    let target = corpse(&w);
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let events = step(
        &mut w,
        &mut p,
        &mut a,
        vec![UiRequest::TakeLootItem { target, slot: 0 }],
    );
    assert!(events.iter().any(|e|matches!(e,SessionEvent::InventoryFailure { reason: 50, bag_slot, .. } if *bag_slot == 70001u32 as u8)));
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::LootRemoved { .. } | SessionEvent::ItemPushResult(_)
    )));
    assert_eq!(
        w.state().npcs[&2]
            .corpse_loot
            .as_ref()
            .unwrap()
            .items()
            .count(),
        1
    );
}

#[test]
fn collection_quest_credit_precedes_storage_and_reward_removes_exact_instances() {
    let mut w = build_collect(&[8], 0, true);
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let mut a = fixture::LootAuthority::default();
    let giver = NpcHandle {
        id: w.state().npcs[&3].id,
    };
    step(
        &mut w,
        &mut p,
        &mut a,
        vec![UiRequest::AcceptQuest {
            quest: 70001,
            giver,
        }],
    );
    kill(&mut w, &mut p, &mut a);
    let target = corpse(&w);
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let collected = step(
        &mut w,
        &mut p,
        &mut a,
        vec![UiRequest::TakeLootItem { target, slot: 0 }],
    );
    let credit = collected
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::QuestObjectiveItem {
                    item_id: 70001,
                    count: 6
                }
            )
        })
        .unwrap();
    let stored = collected
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemCreate { .. }))
        .unwrap();
    assert!(credit < stored);
    assert!(
        matches!(p.quest_progress(&w, 70001, giver).unwrap(), SessionEvent::QuestProgress(q) if q.is_complete && q.required_items == vec![p::QuestRequiredItem { item_id: 70001, count: 6, display_id: 70002 }])
    );
    let slots = w
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .inventory
        .as_ref()
        .unwrap()
        .slots
        .clone();
    let removed = guid(&w, slots[&23].id);
    let remaining = guid(&w, slots[&24].id);
    let rewarded = step(
        &mut w,
        &mut p,
        &mut a,
        vec![UiRequest::TurnInQuest {
            quest: 70001,
            giver,
        }],
    );
    assert!(rewarded
        .iter()
        .any(|e| matches!(e, SessionEvent::ObjectDestroyed(g) if *g == removed)));
    assert!(rewarded.iter().any(|e| matches!(e, SessionEvent::ObjectValues { guid, fields } if *guid == remaining && fields.item_stack_count() == Some(2))));
    let removed_at = rewarded
        .iter()
        .position(|e| matches!(e, SessionEvent::ObjectDestroyed(g) if *g == removed))
        .unwrap();
    let shrunk_at = rewarded.iter().position(|e| matches!(e, SessionEvent::ObjectValues { guid, fields } if *guid == remaining && fields.item_stack_count() == Some(2))).unwrap();
    let reward_at = rewarded
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::QuestComplete(_) | SessionEvent::XpGain(_) | SessionEvent::LevelUp(_)
            )
        })
        .unwrap();
    assert!(removed_at < shrunk_at && shrunk_at < reward_at);
    let fields = player_fields(&rewarded, p.player_guid());
    assert_eq!(fields.player_pack_slot(0), Some(0));
    assert_eq!(fields.player_pack_slot(1), Some(remaining));
    assert!(!rewarded
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemPushResult(_))));
}

#[test]
fn player_looting_flag_tracks_open_release_restore_and_exhaustion() {
    let (mut w, mut p, mut a) = setup(&[2], 0);
    assert_eq!(
        player_fields(&p.initial_events(&w).unwrap(), p.player_guid()).unit_flags() & 0x400,
        0
    );
    kill(&mut w, &mut p, &mut a);
    let target = corpse(&w);
    let opened = step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let flags = player_fields(&opened, p.player_guid()).unit_flags();
    assert_eq!(flags & 0x400, 0x400);
    assert_eq!(flags & 8, 8);
    assert_eq!(
        flags & 0x80000 != 0,
        w.state().combat.as_ref().unwrap().in_combat
    );
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let projection =
        NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
            .unwrap();
    assert_eq!(
        player_fields(
            &projection.initial_events(&restored).unwrap(),
            projection.player_guid()
        )
        .unit_flags()
            & 0x400,
        0x400
    );
    let released = step(&mut w, &mut p, &mut a, vec![UiRequest::ReleaseLoot(target)]);
    assert_eq!(
        player_fields(&released, p.player_guid()).unit_flags() & 0x400,
        0
    );
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let exhausted = step(
        &mut w,
        &mut p,
        &mut a,
        vec![
            UiRequest::TakeLootItem { target, slot: 0 },
            UiRequest::TakeLootMoney(target),
        ],
    );
    assert!(!exhausted
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
    assert_eq!(
        player_fields(&exhausted, p.player_guid()).unit_flags() & 0x400,
        0
    );
}

#[test]
fn bind_on_equip_marks_equipped_copy_but_not_carried_copy_bound() {
    let (mut profile, mut input, mut q) = fixture::setup();
    let init::Rows::Complete(grants) = &mut input.item_grants else {
        unreachable!()
    };
    grants[0].count = 2;
    profile
        .initialize_player(&input, n::synthetic_fixture::float(100.0), 1)
        .unwrap();
    let loot = q.loot.as_mut().unwrap();
    let items = loot
        .inventory
        .items()
        .cloned()
        .map(|mut item| {
            if item.entry == 900001 {
                item.bonding = 2;
            }
            item
        })
        .collect();
    loot.inventory = fixture::inventory_catalog(items);
    profile.initialize_quest_progression(&input, q).unwrap();
    let w = n::NativeWorld::synthetic_component_test(
        profile,
        [8; 16],
        n::synthetic_fixture::player(),
        42,
    )
    .unwrap();
    let p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let events = p.initial_events(&w).unwrap();
    let player = player_fields(&events, p.player_guid());
    let equipped = player.player_inv_slot(4).unwrap();
    let carried = player.player_pack_slot(0).unwrap();
    assert_ne!(equipped, carried);
    assert!(events.iter().any(|event| matches!(event, SessionEvent::ItemCreate { guid, fields, .. } if *guid == equipped && fields.item_flags() == Some(1))));
    assert!(events.iter().any(|event| matches!(event, SessionEvent::ItemCreate { guid, fields, .. } if *guid == carried && fields.item_flags() == Some(0))));
    assert!(
        matches!(p.item_query(&w, 900001).unwrap(), SessionEvent::ItemTemplate { info: Some(info), .. } if info.bonding == 2)
    );
}

#[test]
fn explicit_and_missing_slot_release_unregister_without_a_release_response() {
    let (mut w, mut p, mut a) = setup(&[2], 0);
    kill(&mut w, &mut p, &mut a);
    let target = corpse(&w);
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let explicit = step(
        &mut w,
        &mut p,
        &mut a,
        vec![
            UiRequest::ReleaseLoot(target),
            UiRequest::ReleaseLoot(target),
        ],
    );
    assert!(!explicit
        .iter()
        .any(|e| matches!(e, SessionEvent::LootReleaseResponse { .. })));
    assert!(w
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot_target
        .is_none());
    assert_eq!(
        player_fields(&explicit, p.player_guid()).unit_flags() & 0x400,
        0
    );
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let missing = step(
        &mut w,
        &mut p,
        &mut a,
        vec![UiRequest::TakeLootItem { target, slot: 255 }],
    );
    assert_eq!(missing.iter().filter(|e| matches!(e, SessionEvent::InventoryFailure { reason, bag_slot: 0, .. } if *reason == inv::InventoryResult::ItemNotFound.protocol())).count(), 1);
    assert!(!missing.iter().any(|e| matches!(
        e,
        SessionEvent::LootReleaseResponse { .. } | SessionEvent::ItemPushResult(_)
    )));
    assert!(w
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot_target
        .is_none());
}

#[test]
fn item_last_exhaustion_sends_one_release_but_already_looted_slot_only_unregisters() {
    let (mut w, mut p, mut a) = setup(&[2], 0);
    kill(&mut w, &mut p, &mut a);
    let target = corpse(&w);
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    let duplicate = step(
        &mut w,
        &mut p,
        &mut a,
        vec![
            UiRequest::TakeLootItem { target, slot: 0 },
            UiRequest::TakeLootItem { target, slot: 0 },
        ],
    );
    assert_eq!(
        duplicate
            .iter()
            .filter(|e| matches!(e, SessionEvent::ItemPushResult(_)))
            .count(),
        1
    );
    assert!(!duplicate.iter().any(|e| matches!(
        e,
        SessionEvent::LootReleaseResponse { .. } | SessionEvent::InventoryFailure { .. }
    )));
    assert!(w
        .state()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot_target
        .is_none());

    let (mut w, mut p, mut a) = setup(&[2], 0);
    kill(&mut w, &mut p, &mut a);
    let target = corpse(&w);
    step(&mut w, &mut p, &mut a, vec![UiRequest::OpenLoot(target)]);
    step(
        &mut w,
        &mut p,
        &mut a,
        vec![UiRequest::TakeLootMoney(target)],
    );
    let exhausted = step(
        &mut w,
        &mut p,
        &mut a,
        vec![
            UiRequest::TakeLootItem { target, slot: 0 },
            UiRequest::ReleaseLoot(target),
        ],
    );
    let pushed = exhausted
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    let released = exhausted
        .iter()
        .position(|e| matches!(e, SessionEvent::LootReleaseResponse { result: 1, .. }))
        .unwrap();
    assert!(pushed < released);
    assert_eq!(
        exhausted
            .iter()
            .filter(|e| matches!(e, SessionEvent::LootReleaseResponse { .. }))
            .count(),
        1
    );
    assert_eq!(
        player_fields(&exhausted, p.player_guid()).unit_flags() & 0x400,
        0
    );
}

fn grant_world() -> n::NativeWorld {
    let mut p = fixture::item_grant_profile();
    p.admin = Some(n::AdminPolicy {
        world: [1; 16],
        world_owner: n::synthetic_fixture::player().owner,
        allowlist: Default::default(),
        revision: 1,
    });
    n::NativeWorld::synthetic_component_test(p, [1; 16], n::synthetic_fixture::player(), 42)
        .unwrap()
}
fn grant(
    w: &mut n::NativeWorld,
    p: &mut NativeProjection,
    key: u8,
    item: u32,
    count: u32,
) -> Vec<SessionEvent> {
    let r = n::AdminRequest {
        key: [key; 16],
        world: w.state().world,
        player: w.state().player,
        policy_revision: 1,
        operation: n::AdminOperation::GrantItem { item, count },
    };
    let input = n::synthetic_quest_fixture::command(w, n::Command::Admin(r));
    w.step(
        w.state().tick + 1,
        Some(input),
        &mut fixture::LootAuthority::default(),
    )
    .unwrap();
    p.advance(w).unwrap()
}
#[test]
fn admin_empty_container_creation_and_acquisition_are_independent_of_corpse_removal() {
    let mut w = grant_world();
    let mut p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    p.initial_events(&w).unwrap();
    let events = grant(&mut w, &mut p, 1, 70003, 2);
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::LootRemoved { .. } | SessionEvent::LootResponse { .. }
    )));
    let containers: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ItemCreate {
                guid,
                container,
                fields,
            } => Some((*guid, *container, fields)),
            _ => None,
        })
        .collect();
    assert_eq!(containers.len(), 2);
    for (_, container, fields) in &containers {
        assert!(*container);
        assert_eq!(fields.container_num_slots(), Some(16));
        for slot in 0..36 {
            assert_eq!(fields.container_slot(slot), Some(0));
        }
        assert_eq!(fields.item_stack_count(), Some(1));
        assert_eq!(fields.item_random_properties_id(), 0);
        assert_eq!(fields.item_spell_charges(0), Some(0));
        assert_eq!(fields.item_flags(), Some(0));
        assert_eq!(fields.item_owner(), Some(p.player_guid));
        assert_eq!(fields.item_contained(), Some(p.player_guid));
    }
    let pushes: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::ItemPushResult(p) => Some(p),
            _ => None,
        })
        .collect();
    assert_eq!(pushes.len(), 1);
    assert_eq!(pushes[0].count, 2);
    assert!(pushes[0].created);
    assert!(!pushes[0].from_npc);
    assert!(pushes[0].show_in_chat);
    assert_eq!(pushes[0].item_slot, u32::MAX);
    let duplicate = grant(&mut w, &mut p, 1, 70003, 2);
    assert!(!duplicate.iter().any(|e| matches!(
        e,
        SessionEvent::ItemPushResult(_) | SessionEvent::ItemCreate { .. }
    )));
    let restored = n::NativeWorld::restore_synthetic_component_test(
        w.profile().clone(),
        w.checkpoint().unwrap(),
    )
    .unwrap();
    let projection =
        NativeProjection::new(&restored, restored.state().player, &Catalog::new(&restored))
            .unwrap();
    let restored_events = projection.initial_events(&restored).unwrap();
    assert_eq!(
        restored_events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::ItemCreate {
                    container: true,
                    ..
                }
            ))
            .count(),
        2
    );
    assert!(!restored_events
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemPushResult(_))));
}
#[test]
fn grant_metadata_retains_full_spell_records_and_start_quest_without_using_them() {
    let mut w = grant_world();
    let catalog = Catalog::new(&w);
    let mut p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
    let events = grant(&mut w, &mut p, 1, 70004, 2);
    assert!(events.iter().any(|e|matches!(e,SessionEvent::ItemCreate {container:false,fields,..} if fields.item_spell_charges(0)==Some(-5))));
    let SessionEvent::ItemTemplate {
        info: Some(info), ..
    } = p.item_query(&w, 70004).unwrap()
    else {
        panic!("item metadata")
    };
    assert_eq!(info.spells[0].charges, -5);
    assert_eq!(info.spells[0].cooldown_ms, 10);
    assert_eq!(info.spells[0].category, 4);
    assert_eq!(info.spells[0].category_cooldown_ms, 20);
    assert_eq!(info.start_quest, 70002);
    assert_eq!(info.item_set, 80002);
    let mut bad = Catalog::new(&w);
    bad.items.get_mut(&70004).unwrap().spells[0].category_cooldown_ms += 1;
    assert!(NativeProjection::new(&w, w.state().player, &bad).is_err());
}

fn grant_spell_world(spell: inv::Spell) -> n::NativeWorld {
    let (mut profile, input, mut q) = fixture::item_grant_setup();
    let loot = q.loot.as_mut().unwrap();
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    items.iter_mut().find(|i| i.entry == 70004).unwrap().spells[0] = spell;
    loot.inventory = fixture::inventory_catalog(items);
    profile.initialize_quest_progression(&input, q).unwrap();
    profile.admin = Some(n::AdminPolicy {
        world: [1; 16],
        world_owner: n::synthetic_fixture::player().owner,
        allowlist: Default::default(),
        revision: 1,
    });
    n::NativeWorld::synthetic_component_test(profile, [1; 16], n::synthetic_fixture::player(), 42)
        .unwrap()
}
#[test]
fn source_item_query_resolves_both_negative_cooldowns_but_keeps_mixed_sign_override() {
    for (raw_cooldown, raw_category_cooldown, expected) in [
        (-1, -1, (9000, 90, 9100)),
        (-1, 20, (-1, 4, 20)),
        (10, -1, (10, 4, -1)),
    ] {
        let w = grant_spell_world(inv::Spell {
            id: 80001,
            trigger: 0,
            charges: -5,
            ppm_rate_bits: 0,
            cooldown: raw_cooldown,
            category: 4,
            category_cooldown: raw_category_cooldown,
        });
        let mut catalog = Catalog::new(&w);
        let info = catalog.items.get_mut(&70004).unwrap();
        let spell = &mut info.spells[0];
        spell.cooldown_ms = expected.0;
        spell.category = expected.1;
        spell.category_cooldown_ms = expected.2;
        let spell = info.use_spell.as_mut().unwrap();
        spell.cooldown_ms = expected.0;
        spell.category = expected.1;
        spell.category_cooldown_ms = expected.2;
        let p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
        let SessionEvent::ItemTemplate {
            info: Some(info), ..
        } = p.item_query(&w, 70004).unwrap()
        else {
            panic!("metadata")
        };
        assert_eq!(
            (
                info.spells[0].cooldown_ms,
                info.spells[0].category,
                info.spells[0].category_cooldown_ms
            ),
            expected
        );
        catalog.spell_queries.remove(&80001);
        assert!(NativeProjection::new(&w, w.state().player, &catalog).is_err());
    }
}
#[test]
fn missing_query_spell_zeroes_wire_block_without_zeroing_created_charges() {
    for id in [0, 80001] {
        let mut w = grant_spell_world(inv::Spell {
            id,
            trigger: 0,
            charges: -5,
            ppm_rate_bits: 0,
            cooldown: -1,
            category: 4,
            category_cooldown: -1,
        });
        let mut catalog = Catalog::new(&w);
        catalog
            .spell_queries
            .insert(id, crate::NativeItemSpellQuery::Missing);
        // The shared Bag row also names 80001: its query block is empty too.
        for entry in if id == 0 {
            vec![70004]
        } else {
            vec![70003, 70004]
        } {
            let info = catalog.items.get_mut(&entry).unwrap();
            info.spells.clear();
            info.use_spell = None;
            info.spell_charges_0 = 0;
        }
        let mut p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
        let events = grant(&mut w, &mut p, 1, 70004, 1);
        assert!(events.iter().any(|e|matches!(e,SessionEvent::ItemCreate {fields,..} if fields.item_spell_charges(0)==Some(-5))));
        let SessionEvent::ItemTemplate {
            info: Some(info), ..
        } = p.item_query(&w, 70004).unwrap()
        else {
            panic!("metadata")
        };
        assert_eq!(info.spell_charges_0, 0);
        assert!(info.spells.is_empty());
        assert!(info.use_spell.is_none());
    }
}
#[test]
fn admin_catalog_label_escapes_imported_pipe_markup() {
    let w = grant_world();
    let mut catalog = Catalog::new(&w);
    catalog.items.get_mut(&70004).unwrap().name = "Item |cff00ff00color|r".into();
    let p = NativeProjection::new(&w, w.state().player, &catalog).unwrap();
    assert_eq!(
        p.admin_item_label(70004),
        Some("Item ||cff00ff00color||r".into())
    );
}

#[test]
fn movement_receipt_rejects_stale_before_and_tampered_after_without_changing_frame() {
    let w = build(&[2], 0);
    let p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let catalog = &w
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .inventory;
    let mut before = inventory(w.state()).unwrap().clone();
    before
        .store_new(catalog, 70001, 2, |_, _, _| Ok(()))
        .unwrap();
    let mut after = before.clone();
    let moved = after.swap_backpack(catalog, 23, 38, true).unwrap();
    let mut frame = Frame {
        next_item: Some(before.next_item),
        slots: before.slots.clone(),
        pending_push: None,
    };
    let mut tampered = moved.clone();
    tampered.after.slots.get_mut(&38).unwrap().count += 1;
    assert!(matches!(
        p.inventory_moved(&w, &tampered, &mut frame),
        Err(Error::StaleProjection)
    ));
    assert_eq!(frame.slots, before.slots);
    assert!(frame.pending_push.is_none());
    let events = p.inventory_moved(&w, &moved, &mut frame).unwrap();
    assert_eq!(frame.slots, moved.after.slots);
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::ItemPushResult(_) | SessionEvent::ItemCreate { .. }
    )));
    assert!(matches!(
        p.inventory_moved(&w, &moved, &mut frame),
        Err(Error::StaleProjection)
    ));
    assert_eq!(frame.slots, moved.after.slots);
}

#[test]
fn pickup_and_quest_bonding_move_project_same_instance_flags_without_item_push() {
    for bonding in [1, 4] {
        let (mut profile, input, mut data) = fixture::setup();
        let loot = data.loot.as_mut().unwrap();
        let items = loot
            .inventory
            .items()
            .cloned()
            .map(|mut item| {
                if item.entry == 70001 {
                    item.bonding = bonding;
                }
                item
            })
            .collect();
        loot.inventory = fixture::inventory_catalog(items);
        profile.initialize_quest_progression(&input, data).unwrap();
        let w = n::NativeWorld::synthetic_component_test(
            profile,
            [8; 16],
            n::synthetic_fixture::player(),
            42,
        )
        .unwrap();
        let p = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
        let catalog = &w
            .profile()
            .quest_progression
            .as_ref()
            .unwrap()
            .loot
            .as_ref()
            .unwrap()
            .inventory;
        let mut state = inventory(w.state()).unwrap().clone();
        state
            .grant_item(
                catalog,
                70001,
                2,
                inv::GrantTarget::SelfPlayer,
                |_, _, _| Ok(()),
            )
            .unwrap();
        let id = state.slots[&23].id;
        assert!(!state.slots[&23].bound);
        let mut frame = Frame {
            next_item: Some(state.next_item),
            slots: state.slots.clone(),
            pending_push: None,
        };
        let receipt = state.swap_backpack(catalog, 23, 38, true).unwrap();
        let events = p.inventory_moved(&w, &receipt, &mut frame).unwrap();
        assert!(frame.slots[&38].bound);
        assert_eq!(frame.slots[&38].id, id);
        assert!(events.iter().any(|e| matches!(e,
            SessionEvent::ObjectValues { guid: changed, fields }
            if *changed == guid(&w, id) && fields.item_flags() == Some(1))));
        assert!(!events.iter().any(|e| matches!(
            e,
            SessionEvent::ItemPushResult(_)
                | SessionEvent::ItemCreate { .. }
                | SessionEvent::QuestObjectiveItem { .. }
        )));
    }
}

#[test]
fn split_projection_rejects_locally_valid_wrong_allocator_and_tampered_views_atomically() {
    let w = build(&[2], 0);
    let projection = NativeProjection::new(&w, w.state().player, &Catalog::new(&w)).unwrap();
    let catalog = &w
        .profile()
        .quest_progression
        .as_ref()
        .unwrap()
        .loot
        .as_ref()
        .unwrap()
        .inventory;
    let mut before = inventory(w.state()).unwrap().clone();
    before
        .store_new(catalog, 70001, 4, |_, _, _| Ok(()))
        .unwrap();
    let mut after = before.clone();
    let receipt = after.split_backpack(catalog, 23, 38, 1).unwrap();
    let mut frame = Frame {
        next_item: Some(before.next_item),
        slots: before.slots.clone(),
        pending_push: None,
    };
    let mut other = before.clone();
    other.next_item += 10;
    let other = other.split_backpack(catalog, 23, 38, 1).unwrap();
    other.validate(catalog).unwrap();
    assert!(matches!(
        projection.inventory_split(&w, &other, &mut frame),
        Err(Error::StaleProjection)
    ));
    assert_eq!(frame.next_item, Some(before.next_item));
    assert_eq!(frame.slots, before.slots);
    let mut bad = receipt.clone();
    bad.views[0].slots.get_mut(&23).unwrap().count += 1;
    assert!(matches!(
        projection.inventory_split(&w, &bad, &mut frame),
        Err(Error::StaleProjection)
    ));
    assert_eq!(frame.next_item, Some(before.next_item));
    assert_eq!(frame.slots, before.slots);
    let events = projection
        .inventory_split(&w, &receipt, &mut frame)
        .unwrap();
    assert_eq!(frame.next_item, Some(receipt.after.next_item));
    assert_eq!(frame.slots, receipt.after.slots);
    assert!(!events
        .iter()
        .any(|event| matches!(event, SessionEvent::ItemPushResult(_))));
}
