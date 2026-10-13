// SPDX-License-Identifier: GPL-3.0-or-later
//! Test-only catalog values are authored bindings, not records from a WoW installation.
use benilla_protocol::{messages as protocol, SessionEvent};
use benilla_world_client::*;
use benilla_world_data::{load_json, Limits, WorldData};
use benilla_world_runtime as r;
use r::{Command, PlayerId, Rules, World};
const FIXTURE: &[u8] = include_bytes!("../../benilla-world-data/tests/fixtures/original-camp.json");
fn player(n: u8) -> PlayerId {
    PlayerId {
        owner: [n; 16],
        character: [n + 10; 16],
    }
}
struct TestCatalog {
    data: WorldData,
    missing_spell: bool,
}
impl ClientCatalog for TestCatalog {
    fn item_metadata(&self, entry: u32) -> Option<protocol::ItemInfo> {
        let item = self.data.item(entry)?;
        Some(protocol::ItemInfo {
            class: 15,
            subclass: 0,
            name: item.name.clone(),
            display_info_id: item.display_id,
            quality: 1,
            flags: 0,
            buy_price: 0,
            sell_price: 0,
            inventory_type: 0,
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
            max_count: 0,
            stackable: u32::from(item.max_stack),
            container_slots: 0,
            stats: Vec::new(),
            damages: Vec::new(),
            dmg_min: 0.0,
            dmg_max: 0.0,
            dmg_type: 0,
            armor: 0,
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
            max_durability: 0,
            area: 0,
            map: 0,
            bag_family: 0,
        })
    }
    fn quest_metadata(&self, entry: u32) -> Option<QuestMetadata> {
        let q = self.data.quest(entry)?;
        let mut rewards = [(0, 0); 4];
        for (i, item) in q.reward_items.iter().enumerate() {
            rewards[i] = (item.item_entry, u32::from(item.count));
        }
        Some(QuestMetadata {
            template: protocol::QuestTemplate {
                quest_id: entry,
                method: 2,
                level: 1,
                zone_or_sort: 0,
                quest_type: 0,
                rep_objective_faction: 0,
                rep_objective_value: 0,
                next_quest_in_chain: 0,
                money: q.reward_money as i32,
                money_max_level: 0,
                reward_spell: 0,
                src_item_id: 0,
                flags: 0,
                rewards,
                choices: [(0, 0); 6],
                point_map_id: 0,
                point_x: 0.0,
                point_y: 0.0,
                point_opt: 0,
                title: q.title.clone(),
                objectives_text: "Practice on two constructs.".into(),
                details: q.details.clone(),
                end_text: "Return to the steward.".into(),
                objectives: std::array::from_fn(|i| {
                    let (creature_or_go, required_count) = q
                        .objectives
                        .get(i)
                        .map_or((0, 0), |o| (o.creature_entry, u32::from(o.count)));
                    protocol::QuestObjective {
                        creature_or_go,
                        required_count,
                        item_id: 0,
                        item_count: 0,
                        text: String::new(),
                    }
                }),
            },
            request_text: "Have you finished the practice?".into(),
            offer_text: "Take this practice token.".into(),
        })
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        Some("Welcome to the original practice camp.".into())
    }

    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> r::Digest {
        [5; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 0
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        matches!(id, 770001 | 770002)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        id == 770003
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn contains_spell(&self, id: u32) -> bool {
        !self.missing_spell && matches!(id, 53001 | 53002)
    }
}
fn raw_data() -> WorldData {
    load_json(FIXTURE, Limits::default()).unwrap()
}
fn bound_data() -> WorldData {
    let mut export = raw_data().export().clone();
    export.creatures[0].display_id = 770001;
    export.creatures[1].display_id = 770002;
    export.items[0].display_id = 770003;
    export.abilities[0].entry = 53001;
    export.abilities[1].entry = 53002;
    load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap()
}
fn rules() -> Rules {
    let mut rules = Rules::original_camp();
    rules.starting_abilities = vec![53001, 53002];
    rules
}
fn world() -> World {
    World::new(bound_data(), rules(), [8; 16], 1, vec![player(1)]).unwrap()
}
fn projection(w: &World) -> Projection {
    Projection::new(
        w,
        player(1),
        &TestCatalog {
            data: w.data().clone(),
            missing_spell: false,
        },
    )
    .unwrap()
}
fn step(w: &mut World, p: &mut Projection, request: Option<UiRequest>) -> Vec<SessionEvent> {
    let inputs = request
        .map(|request| p.commands().local(w, request).unwrap())
        .into_iter()
        .collect();
    w.step(w.state().tick + 1, inputs).unwrap();
    p.tick(w).unwrap()
}
fn npc(p: &Projection, w: &World, spawn: u64) -> NpcHandle {
    p.capture_npc(p.npc_guid(w.npc(spawn).unwrap().id).unwrap())
        .unwrap()
}
#[test]
fn original_fixture_cannot_silently_become_a_classic_client_world() {
    let w = World::new(
        raw_data(),
        Rules::original_camp(),
        [8; 16],
        1,
        vec![player(1)],
    )
    .unwrap();
    assert!(matches!(
        Projection::new(
            &w,
            player(1),
            &TestCatalog {
                data: bound_data(),
                missing_spell: false
            }
        ),
        Err(Error::UnboundCreatureDisplay(900001))
    ));
    assert!(matches!(
        Projection::new(
            &world(),
            player(1),
            &TestCatalog {
                data: bound_data(),
                missing_spell: true
            }
        ),
        Err(Error::UnboundSpell(53001))
    ));
}
#[test]
fn projection_uses_exact_bound_ids_and_protocol_health_power_xp_fields() {
    let w = world();
    let p = projection(&w);
    let events = p.initial_events(&w).unwrap();
    let mut units = 0;
    let mut player_values = 0;
    let mut book = 0;
    for event in events {
        match event {
            SessionEvent::ObjectCreate {
                display_id,
                position,
                fields,
                ..
            } => {
                units += 1;
                assert!(matches!(display_id, Some(770001 | 770002)));
                assert!(position == [0.0, 0.0, 0.0] || position == [3.0, 0.0, 0.0]);
                assert!(matches!(fields.unit_health(), Some(80 | 30)));
            }
            SessionEvent::ObjectValues { guid, fields } => {
                player_values += 1;
                assert_eq!(Some(guid), p.player_guid(player(1)));
                assert_eq!(fields.unit_health(), Some(60));
                assert_eq!(fields.unit_max_health(), Some(60));
                assert_eq!(fields.player_xp(), Some(0));
                assert_eq!(fields.player_next_level_xp(), Some(200));
            }
            SessionEvent::SpellBook { spell_ids, .. } => {
                book += 1;
                assert_eq!(spell_ids, vec![53001, 53002]);
            }
            _ => {}
        }
    }
    assert_eq!((units, player_values, book), (2, 1, 1));
}
#[test]
fn local_translation_never_accepts_gui_owner_tick_or_sequence() {
    let w = world();
    let p = projection(&w);
    let target = npc(&p, &w, 2);
    let input = p
        .commands()
        .local(&w, UiRequest::MeleeOnce(target))
        .unwrap();
    assert_eq!(input.authenticated_owner, player(1).owner);
    assert_eq!(input.input.player, player(1));
    assert_eq!((input.input.tick, input.input.sequence), (1, 0));
    assert_eq!(input.input.command, Command::Melee { target: target.id });
}
#[test]
fn remote_envelopes_bind_authenticated_owner_character_content_rules_and_membership() {
    let w = World::new(
        bound_data(),
        rules(),
        [8; 16],
        1,
        vec![player(1), player(2)],
    )
    .unwrap();
    let p = projection(&w);
    let valid = r::Input {
        world: w.state().world,
        tick: 1,
        player: player(2),
        sequence: 0,
        command: Command::Revive,
    };
    let envelope = p.commands().envelope(&w, valid.clone()).unwrap();
    assert_eq!(
        p.commands()
            .remote(&w, player(2).owner, envelope.clone())
            .unwrap()
            .input,
        valid
    );
    assert!(matches!(
        p.commands().remote(&w, player(1).owner, envelope.clone()),
        Err(Error::WrongOwner)
    ));
    let mut bad = envelope.clone();
    bad.input.player.character = [99; 16];
    assert!(matches!(
        p.commands().remote(&w, player(2).owner, bad),
        Err(Error::WrongCharacter)
    ));
    let mut bad = envelope.clone();
    bad.compatibility.content_digest[0] ^= 1;
    assert!(matches!(
        p.commands().remote(&w, player(2).owner, bad),
        Err(Error::Incompatible)
    ));
    let mut bad = envelope.clone();
    bad.compatibility.rules_digest[0] ^= 1;
    assert!(matches!(
        p.commands().remote(&w, player(2).owner, bad),
        Err(Error::Incompatible)
    ));
    let mut bad = envelope.clone();
    bad.compatibility.membership_digest[0] ^= 1;
    assert!(matches!(
        p.commands().remote(&w, player(2).owner, bad),
        Err(Error::Incompatible)
    ));
    let mut bad = envelope.clone();
    bad.input.sequence = 2;
    assert!(matches!(
        p.commands().remote(&w, player(2).owner, bad),
        Err(Error::InvalidSequence)
    ));
    let mut bad = envelope;
    bad.input.tick = 2;
    assert!(matches!(
        p.commands().remote(&w, player(2).owner, bad),
        Err(Error::WrongTick)
    ));
}
#[test]
fn rejected_remote_data_cannot_cancel_an_independent_solo_tick() {
    let mut w = world();
    let mut p = projection(&w);
    let local = p
        .commands()
        .local(&w, UiRequest::MoveDelta([1000, 0, 0]))
        .unwrap();
    let fake = p.commands().envelope(&w, local.input.clone()).unwrap();
    assert!(p.commands().remote(&w, player(2).owner, fake).is_err());
    w.step(1, vec![local]).unwrap();
    p.tick(&w).unwrap();
    assert_eq!(w.player(player(1)).unwrap().position.xyz, [1000, 0, 0]);
}
#[test]
fn envelope_parser_rejects_unknown_fields_and_oversize_payloads() {
    let w = world();
    let p = projection(&w);
    let input = p.commands().local(&w, UiRequest::Revive).unwrap();
    let envelope = p.commands().envelope(&w, input.input).unwrap();
    let bytes = envelope.encode().unwrap();
    assert_eq!(Envelope::decode(&bytes).unwrap(), envelope);
    let mut json: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    json["fake_owner"] = true.into();
    assert_eq!(
        Envelope::decode(&serde_json::to_vec(&json).unwrap()),
        Err(Error::InvalidEnvelope)
    );
    assert_eq!(Envelope::decode(&vec![0; 4097]), Err(Error::TooLarge));
}
#[test]
fn npc_quest_cast_and_reward_results_reach_existing_session_events() {
    let mut w = world();
    let mut p = projection(&w);
    let giver = npc(&p, &w, 1);
    assert!(matches!(
        p.quest_detail(&w, 910001, giver).unwrap(),
        SessionEvent::QuestDetail(_)
    ));
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::AcceptQuest {
            quest: 910001,
            giver,
        }),
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::GossipComplete)));
    for _ in 0..2 {
        step(&mut w, &mut p, Some(UiRequest::MoveDelta([1000, 0, 0])));
    }
    let mut xp_awards = 0;
    let mut objective_updates = 0;
    let mut cast_starts = 0;
    let mut cast_goes = 0;
    for kill in 0..2 {
        if kill > 0 {
            let tick = w.npc(2).unwrap().respawn_at.unwrap();
            while w.state().tick < tick {
                step(&mut w, &mut p, None);
            }
        }
        let target = npc(&p, &w, 2);
        step(&mut w, &mut p, Some(UiRequest::MeleeOnce(target)));
        let attack = w.state().tick;
        let events = step(
            &mut w,
            &mut p,
            Some(UiRequest::CastAtNpc {
                ability: 53001,
                target,
            }),
        );
        cast_starts += events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    SessionEvent::SpellStart {
                        spell_id: 53001,
                        ..
                    }
                )
            })
            .count();
        while w.state().tick < attack + 9 {
            let events = step(&mut w, &mut p, None);
            cast_goes += events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        SessionEvent::SpellGo {
                            spell_id: 53001,
                            ..
                        }
                    )
                })
                .count();
        }
        step(&mut w, &mut p, Some(UiRequest::MeleeOnce(target)));
        let events = step(
            &mut w,
            &mut p,
            Some(UiRequest::CastAtNpc {
                ability: 53001,
                target,
            }),
        );
        cast_starts += events
            .iter()
            .filter(|e| {
                matches!(
                    e,
                    SessionEvent::SpellStart {
                        spell_id: 53001,
                        ..
                    }
                )
            })
            .count();
        while w.state().tick < attack + 16 {
            let events = step(&mut w, &mut p, None);
            cast_goes += events
                .iter()
                .filter(|e| {
                    matches!(
                        e,
                        SessionEvent::SpellGo {
                            spell_id: 53001,
                            ..
                        }
                    )
                })
                .count();
            xp_awards += events
                .iter()
                .filter(|e| matches!(e, SessionEvent::XpGain(_)))
                .count();
            objective_updates += events
                .iter()
                .filter(|e| matches!(e, SessionEvent::QuestObjectiveKill { .. }))
                .count();
        }
        step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
        step(
            &mut w,
            &mut p,
            Some(UiRequest::TakeLootItem { target, slot: 0 }),
        );
        step(&mut w, &mut p, Some(UiRequest::TakeLootMoney(target)));
    }
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::TurnInQuest {
            quest: 910001,
            giver,
        }),
    );
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::QuestComplete(q) if q.xp==25&&q.money==10)));
    assert_eq!(
        (xp_awards, objective_updates, cast_starts, cast_goes),
        (2, 2, 4, 4)
    );
    let fields = events
        .iter()
        .find_map(|e| match e {
            SessionEvent::ObjectValues { guid, fields }
                if Some(*guid) == p.player_guid(player(1)) =>
            {
                Some(fields)
            }
            _ => None,
        })
        .unwrap();
    assert_eq!(fields.player_xp(), Some(125));
    assert_eq!(fields.player_quest_log(0).unwrap().quest_id, 0);
}
#[test]
fn respawn_changes_guid_and_old_captured_commands_keep_the_old_generation() {
    let mut rules = rules();
    rules.melee_damage = 30;
    let mut w = World::new(bound_data(), rules, [8; 16], 1, vec![player(1)]).unwrap();
    let mut p = projection(&w);
    for _ in 0..2 {
        step(&mut w, &mut p, Some(UiRequest::MoveDelta([1000, 0, 0])));
    }
    let old = npc(&p, &w, 2);
    let guid = p.npc_guid(old.id).unwrap();
    step(&mut w, &mut p, Some(UiRequest::MeleeOnce(old)));
    let due = w.npc(2).unwrap().respawn_at.unwrap();
    while w.state().tick < due - 1 {
        step(&mut w, &mut p, None);
    }
    let events = step(&mut w, &mut p, None);
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::ObjectDestroyed(id) if *id==guid)));
    assert_ne!(p.npc_guid(w.npc(2).unwrap().id), Some(guid));
    assert_eq!(p.capture_npc(guid), Err(Error::UnknownTarget));
    let input = p.commands().local(&w, UiRequest::OpenLoot(old)).unwrap();
    let result = w.step(w.state().tick + 1, vec![input]).unwrap();
    assert_eq!(
        result.outcomes[0].rejection,
        Some(r::Rejection::StaleTarget)
    );
}
#[test]
fn skipped_projection_ticks_fail_instead_of_losing_events() {
    let mut w = world();
    let mut p = projection(&w);
    w.step(1, vec![]).unwrap();
    w.step(2, vec![]).unwrap();
    assert!(matches!(p.tick(&w), Err(Error::StaleProjection)));
}
#[test]
fn restored_cooldown_is_projected_instead_of_reset_in_client() {
    let mut w = world();
    let mut p = projection(&w);
    step(&mut w, &mut p, Some(UiRequest::CastSelf { ability: 53002 }));
    let restored = World::restore(bound_data(), rules(), &w.snapshot_bytes().unwrap()).unwrap();
    let p = projection(&restored);
    let events = p.initial_events(&restored).unwrap();
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellStart {
            spell_id: 53002,
            cast_time_ms: 500,
            ..
        }
    )));
    assert!(events.iter().any(
        |e| matches!(e,SessionEvent::SpellCooldowns{cooldowns,..} if cooldowns==&vec![(53002,1000)])
    ));
}

#[test]
fn imported_plain_text_cannot_activate_stock_ui_hyperlink_or_color_markup() {
    let mut export = bound_data().export().clone();
    export.creatures[0].name = "Steward |Hitem:123|hclick|h".into();
    export.quests[0].title = "|cffff0000A title|r".into();
    export.quests[0].details = "Text |Hurl:example|hlabel|h".into();
    let data = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let w = World::new(data, rules(), [8; 16], 1, vec![player(1)]).unwrap();
    let p = projection(&w);
    assert!(p.initial_events(&w).unwrap().iter().any(|e| matches!(e, SessionEvent::CreatureName { name: Some(name),.. } if name=="Steward ||Hitem:123||hclick||h")));
    let giver = npc(&p, &w, 1);
    let SessionEvent::QuestDetail(q) = p.quest_detail(&w, 910001, giver).unwrap() else {
        panic!("quest detail missing")
    };
    assert_eq!(q.title, "||cffff0000A title||r");
    assert_eq!(q.details, "Text ||Hurl:example||hlabel||h");
}

#[test]
fn equal_tick_independently_progressed_worlds_cannot_exchange_commands() {
    let mut a = world();
    let mut b = world();
    let mut pa = projection(&a);
    let mut pb = projection(&b);
    step(&mut a, &mut pa, Some(UiRequest::MoveDelta([1000, 0, 0])));
    step(&mut b, &mut pb, Some(UiRequest::MoveDelta([-1000, 0, 0])));
    assert_eq!(a.state().revision, b.state().revision);
    assert_ne!(a.state_digest(), b.state_digest());
    let input = pa.commands().local(&a, UiRequest::Revive).unwrap();
    let envelope = pa.commands().envelope(&a, input.input).unwrap();
    assert!(matches!(
        pb.commands().remote(&b, player(1).owner, envelope),
        Err(Error::Incompatible)
    ));
}

fn kill_construct(w: &mut World, p: &mut Projection) -> NpcHandle {
    if w.player(player(1)).unwrap().position.xyz == [0, 0, 0] {
        step(w, p, Some(UiRequest::MoveDelta([1000, 0, 0])));
    }
    if let Some(respawn) = w.npc(2).unwrap().respawn_at {
        while w.state().tick < respawn {
            step(w, p, None);
        }
    }
    let target = npc(p, w, 2);
    while w.npc(2).unwrap().health > 0 {
        let request = (w.player(player(1)).unwrap().melee_ready_at <= w.state().tick + 1)
            .then_some(UiRequest::MeleeOnce(target));
        step(w, p, request);
    }
    target
}
#[test]
fn opening_loot_never_grants_and_partial_inventory_survives_restore() {
    let mut w = world();
    let mut p = projection(&w);
    let target = kill_construct(&mut w, &mut p);
    for _ in 0..2 {
        let events = step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
        assert!(events.iter().any(|e| matches!(e,SessionEvent::LootResponse{gold:1,items,..} if items.len()==1 && items[0].slot==0 && items[0].item_id==920001)));
        assert!(w.player(player(1)).unwrap().inventory.is_empty());
        assert_eq!(w.player(player(1)).unwrap().money, 0);
        assert!(!events.iter().any(|e| matches!(
            e,
            SessionEvent::ItemCreate { .. } | SessionEvent::ItemPushResult(_)
        )));
    }
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::TakeLootItem { target, slot: 0 }),
    );
    let item = w.player(player(1)).unwrap().inventory[0].clone();
    let guid = p.inventory_guid(&w, item.instance).unwrap();
    assert!(events.iter().any(|e| matches!(e,SessionEvent::ItemCreate{guid:g,fields,..} if *g==guid && fields.item_stack_count()==Some(1) && fields.item_owner()==p.player_guid(player(1)))));
    assert!(events.iter().any(|e| matches!(e,SessionEvent::ObjectValues{fields,..} if fields.player_pack_slot(0)==Some(guid))));
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootRemoved { slot: 0 })));
    assert!(events.iter().any(|e| matches!(e,SessionEvent::ItemPushResult(i) if !i.from_npc && i.count==1 && i.item_slot==23)));
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::TakeLootItem { target, slot: 0 }),
    );
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::LootRemoved { .. }
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::ItemCreate { .. }
    )));
    assert_eq!(w.player(player(1)).unwrap().inventory, vec![item]);
    let bytes = w.snapshot_bytes().unwrap();
    let mut restored = World::restore(w.data().clone(), w.rules().clone(), &bytes).unwrap();
    let mut p = projection(&restored);
    let events = p.initial_events(&restored).unwrap();
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::ItemCreate{guid:g,..} if *g==guid)));
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::LootResponse{gold:1,items,..} if items.is_empty())));
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemPushResult(_))));
    let events = step(
        &mut restored,
        &mut p,
        Some(UiRequest::TakeLootMoney(target)),
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootClearMoney)));
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootMoneyNotify { amount: 1 })));
    assert_eq!(restored.player(player(1)).unwrap().money, 1);
    let events = step(
        &mut restored,
        &mut p,
        Some(UiRequest::TakeLootMoney(target)),
    );
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::LootMoneyNotify { .. } | SessionEvent::LootClearMoney
    )));
    let events = step(&mut restored, &mut p, Some(UiRequest::OpenLoot(target)));
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::LootResponse{gold:0,items,..} if items.is_empty())));
}
#[test]
fn loot_window_expires_on_range_or_respawn_and_keeps_generation_identity() {
    let mut w = world();
    let mut p = projection(&w);
    let target = kill_construct(&mut w, &mut p);
    let old_guid = p.npc_guid(target.id).unwrap();
    step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
    for _ in 0..2 {
        step(&mut w, &mut p, Some(UiRequest::MoveDelta([-1000, 0, 0])));
    }
    let events = step(&mut w, &mut p, Some(UiRequest::MoveDelta([-1000, 0, 0])));
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::LootReleaseResponse{guid,result:1} if *guid==old_guid)));
    assert!(matches!(
        p.commands().local(&w, UiRequest::TakeLootMoney(target)),
        Err(Error::LootNotOpen)
    ));
    let events = step(&mut w, &mut p, Some(UiRequest::ReleaseLoot(target)));
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::LootResponse { .. })));
    step(&mut w, &mut p, Some(UiRequest::MoveDelta([1000, 0, 0])));
    step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
    let respawn = w.npc(2).unwrap().respawn_at.unwrap();
    while w.state().tick + 1 < respawn {
        step(&mut w, &mut p, None);
    }
    let events = step(&mut w, &mut p, None);
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::LootReleaseResponse{guid,result:1} if *guid==old_guid)));
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::ObjectDestroyed(guid) if *guid==old_guid)));
    assert_ne!(p.npc_guid(w.npc(2).unwrap().id), Some(old_guid));
    assert!(w.player(player(1)).unwrap().active_loot.is_none());
}
#[test]
fn quest_panels_are_read_only_until_reward_confirmation_and_status_updates() {
    let mut w = world();
    let mut p = projection(&w);
    let giver = npc(&p, &w, 1);
    assert!(matches!(
        p.quest_status(&w, giver).unwrap(),
        SessionEvent::QuestGiverStatus { status: 5, .. }
    ));
    assert!(
        matches!(p.quest_greeting(&w,giver).unwrap(),SessionEvent::QuestGreeting(g) if g.quests.len()==1 && g.quests[0].icon==5)
    );
    assert!(
        matches!(p.quest_template(910001).unwrap(),SessionEvent::QuestTemplate(q) if q.objectives[0].required_count==2)
    );
    assert!(p.quest_offer(&w, 910001, giver).is_err());
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::AcceptQuest {
            quest: 910001,
            giver,
        }),
    );
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestGiverStatus { status: 3, .. })));
    assert!(
        matches!(p.quest_progress(&w,910001,giver).unwrap(),SessionEvent::QuestProgress(q) if !q.is_complete)
    );
    assert!(p.quest_offer(&w, 910001, giver).is_err());
    kill_construct(&mut w, &mut p);
    kill_construct(&mut w, &mut p);
    assert!(matches!(
        p.quest_status(&w, giver).unwrap(),
        SessionEvent::QuestGiverStatus { status: 7, .. }
    ));
    let before = w.snapshot_bytes().unwrap();
    for _ in 0..2 {
        assert!(
            matches!(p.quest_progress(&w,910001,giver).unwrap(),SessionEvent::QuestProgress(q) if q.is_complete)
        );
        assert!(
            matches!(p.quest_offer(&w,910001,giver).unwrap(),SessionEvent::QuestOffer(q) if q.money==10 && q.rewards[0].item_id==920001)
        );
    }
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    assert_eq!(w.player(player(1)).unwrap().money, 0);
    assert!(w.player(player(1)).unwrap().inventory.is_empty());
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::TurnInQuest {
            quest: 910001,
            giver,
        }),
    );
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::QuestComplete(q) if q.money==10)));
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestGiverStatus { status: 0, .. })));
    assert!(events
        .iter()
        .any(|e| matches!(e,SessionEvent::ItemPushResult(i) if i.from_npc && i.count==1)));
    let guid = p
        .inventory_guid(&w, w.player(player(1)).unwrap().inventory[0].instance)
        .unwrap();
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::TurnInQuest {
            quest: 910001,
            giver,
        }),
    );
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::QuestComplete(_)
            | SessionEvent::ItemCreate { .. }
            | SessionEvent::ItemPushResult(_)
    )));
    assert_eq!(w.player(player(1)).unwrap().money, 10);
    assert_eq!(
        p.inventory_guid(&w, w.player(player(1)).unwrap().inventory[0].instance)
            .unwrap(),
        guid
    );
    assert!(p.quest_offer(&w, 910001, giver).is_err());
}
#[test]
fn item_stack_merges_keep_guid_slot_and_publish_count_update() {
    let mut w = world();
    let mut p = projection(&w);
    let target = kill_construct(&mut w, &mut p);
    step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
    step(
        &mut w,
        &mut p,
        Some(UiRequest::TakeLootItem { target, slot: 0 }),
    );
    let guid = p
        .inventory_guid(&w, w.player(player(1)).unwrap().inventory[0].instance)
        .unwrap();
    let target = kill_construct(&mut w, &mut p);
    step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
    let events = step(
        &mut w,
        &mut p,
        Some(UiRequest::TakeLootItem { target, slot: 0 }),
    );
    assert!(!events
        .iter()
        .any(|e| matches!(e, SessionEvent::ItemCreate { .. })));
    assert!(events.iter().any(|e| matches!(e,SessionEvent::ObjectValues{guid:g,fields} if *g==guid && fields.item_stack_count()==Some(2))));
    assert!(events.iter().any(
        |e| matches!(e,SessionEvent::ItemPushResult(i) if i.item_slot==u32::MAX && i.count==1)
    ));
    assert_eq!(w.player(player(1)).unwrap().inventory[0].slot, 0);
    let template = p.item_template(920001).unwrap();
    assert!(
        matches!(template,SessionEvent::ItemTemplate{info:Some(i),..} if i.stackable==20 && i.display_info_id==770003 && i.description=="An original test token.")
    );
}
#[test]
fn quest_panels_reject_remote_and_unrelated_targets_without_mutation() {
    let mut w = world();
    let mut p = projection(&w);
    let giver = npc(&p, &w, 1);
    let target = npc(&p, &w, 2);
    assert!(matches!(
        p.quest_detail(&w, 910001, target),
        Err(Error::UnavailableQuest)
    ));
    assert!(matches!(
        p.quest_detail(&w, u32::MAX, giver),
        Err(Error::UnavailableQuest)
    ));
    for _ in 0..5 {
        step(&mut w, &mut p, Some(UiRequest::MoveDelta([-1000, 0, 0])));
    }
    let before = w.snapshot_bytes().unwrap();
    assert!(matches!(
        p.quest_detail(&w, 910001, giver),
        Err(Error::OutOfRange)
    ));
    assert!(matches!(
        p.quest_greeting(&w, giver),
        Err(Error::OutOfRange)
    ));
    assert!(matches!(
        p.quest_status(&w, giver).unwrap(),
        SessionEvent::QuestGiverStatus { status: 5, .. }
    ));
    assert_eq!(w.snapshot_bytes().unwrap(), before);
}

struct FaultyCatalog {
    inner: TestCatalog,
    fault: u8,
}
impl ClientCatalog for FaultyCatalog {
    fn build(&self) -> u32 {
        self.inner.build()
    }
    fn manifest_digest(&self) -> r::Digest {
        self.inner.manifest_digest()
    }
    fn contains_map(&self, id: u32) -> bool {
        self.inner.contains_map(id)
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        self.inner.contains_creature_display(id)
    }
    fn contains_item_display(&self, id: u32) -> bool {
        self.inner.contains_item_display(id)
    }
    fn contains_spell(&self, id: u32) -> bool {
        self.inner.contains_spell(id)
    }
    fn player_power_type(&self, p: PlayerId) -> Option<u8> {
        self.inner.player_power_type(p)
    }
    fn item_metadata(&self, id: u32) -> Option<protocol::ItemInfo> {
        if self.fault == 1 {
            return None;
        }
        let mut value = self.inner.item_metadata(id)?;
        match self.fault {
            2 => value.stackable = 999,
            3 => value.required_spell = 123,
            7 => value.description = "hidden\0suffix".into(),
            _ => {}
        }
        Some(value)
    }
    fn quest_metadata(&self, id: u32) -> Option<QuestMetadata> {
        if self.fault == 4 {
            return None;
        }
        let mut value = self.inner.quest_metadata(id)?;
        match self.fault {
            5 => value.template.rewards[0].1 += 1,
            8 => value.template.flags = protocol::quest_flags::AUTO_REWARDED,
            9 => value.template.point_x = f32::NAN,
            _ => {}
        }
        Some(value)
    }
    fn quest_greeting(&self, id: u32) -> Option<String> {
        if self.fault == 6 {
            None
        } else {
            self.inner.quest_greeting(id)
        }
    }
}
#[test]
fn missing_or_unsupported_item_quest_metadata_never_receives_fabricated_defaults() {
    let w = world();
    for fault in 1..=9 {
        let catalog = FaultyCatalog {
            inner: TestCatalog {
                data: w.data().clone(),
                missing_spell: false,
            },
            fault,
        };
        let expected = match fault {
            1 => Error::MissingItemMetadata(920001),
            2 | 3 | 7 => Error::InvalidItemMetadata(920001),
            4 => Error::MissingQuestMetadata(910001),
            5 | 8 | 9 => Error::InvalidQuestMetadata(910001),
            6 => Error::MissingQuestGreeting(900001),
            _ => unreachable!(),
        };
        assert!(
            matches!(Projection::new(&w,player(1),&catalog),Err(e) if e==expected),
            "fault {fault}"
        );
    }
}
#[test]
fn full_bag_failed_take_keeps_loot_and_emits_no_item_grant() {
    let mut export = bound_data().export().clone();
    export.items[0].max_stack = 1;
    let data = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = rules();
    rules.inventory_slots = 1;
    let mut w = World::new(data, rules, [8; 16], 1, vec![player(1)]).unwrap();
    let mut p = projection(&w);
    for kill in 0..2 {
        let target = kill_construct(&mut w, &mut p);
        step(&mut w, &mut p, Some(UiRequest::OpenLoot(target)));
        let before = w.player(player(1)).unwrap().inventory.clone();
        let events = step(
            &mut w,
            &mut p,
            Some(UiRequest::TakeLootItem { target, slot: 0 }),
        );
        if kill == 1 {
            assert_eq!(w.player(player(1)).unwrap().inventory, before);
            assert!(w.npc(2).unwrap().loot_items[0].is_some());
            assert!(events.iter().any(
                |e| matches!(e,SessionEvent::Notification{text} if text.contains("InventoryFull"))
            ));
            assert!(!events.iter().any(|e| matches!(
                e,
                SessionEvent::LootRemoved { .. }
                    | SessionEvent::ItemCreate { .. }
                    | SessionEvent::ItemPushResult(_)
            )));
        }
    }
}

#[test]
fn queued_loot_request_cannot_switch_to_a_new_corpse_window() {
    let mut export = bound_data().export().clone();
    let mut spawn = export.spawns[1].clone();
    spawn.spawn_id = 3;
    export.spawns.push(spawn);
    let data = load_json(&serde_json::to_vec(&export).unwrap(), Limits::default()).unwrap();
    let mut rules = rules();
    rules.melee_damage = 30;
    rules.melee_period_ms = 1;
    let mut w = World::new(data, rules, [8; 16], 1, vec![player(1)]).unwrap();
    let mut p = projection(&w);
    step(&mut w, &mut p, Some(UiRequest::MoveDelta([1000, 0, 0])));
    let first = npc(&p, &w, 2);
    let second = npc(&p, &w, 3);
    step(&mut w, &mut p, Some(UiRequest::MeleeOnce(first)));
    step(&mut w, &mut p, Some(UiRequest::MeleeOnce(second)));
    step(&mut w, &mut p, Some(UiRequest::OpenLoot(first)));
    let captured = p.commands().capture_loot(&w).unwrap();
    step(&mut w, &mut p, Some(UiRequest::OpenLoot(second)));
    let before = w.snapshot_bytes().unwrap();
    assert!(matches!(
        p.commands().local(
            &w,
            UiRequest::TakeLootItem {
                target: captured,
                slot: 0
            }
        ),
        Err(Error::LootNotOpen)
    ));
    assert!(matches!(
        p.commands().local(&w, UiRequest::TakeLootMoney(captured)),
        Err(Error::LootNotOpen)
    ));
    assert_eq!(w.snapshot_bytes().unwrap(), before);
    step(&mut w, &mut p, Some(UiRequest::ReleaseLoot(first)));
    assert_eq!(w.player(player(1)).unwrap().active_loot, Some(second.id));
}
#[test]
fn stock_adapter_refuses_legacy_atomic_headless_loot() {
    let w = world();
    let p = projection(&w);
    let target = npc(&p, &w, 2);
    let owned = p.commands().local(&w, UiRequest::OpenLoot(target)).unwrap();
    let mut forged = p.commands().envelope(&w, owned.input.clone()).unwrap();
    forged.input.command = Command::Loot { target: target.id };
    assert!(matches!(
        p.commands().envelope(&w, forged.input.clone()),
        Err(Error::UnsupportedCommand)
    ));
    assert!(matches!(
        p.commands().remote(&w, player(1).owner, forged),
        Err(Error::UnsupportedCommand)
    ));
}
#[test]
fn next_tick_from_an_independently_progressed_world_is_not_projected() {
    let mut a = world();
    let mut b = world();
    let mut pa = projection(&a);
    let mut pb = projection(&b);
    step(&mut a, &mut pa, Some(UiRequest::MoveDelta([1000, 0, 0])));
    step(&mut b, &mut pb, Some(UiRequest::MoveDelta([-1000, 0, 0])));
    step(&mut b, &mut pb, None);
    assert!(matches!(pa.tick(&b), Err(Error::StaleProjection)));
    step(&mut a, &mut pa, None);
}
