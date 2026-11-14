// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn assert_giver_live(
    c: &NativeController,
    giver: u64,
    original: runtime::NpcId,
    original_guid: u64,
) {
    let npc = &c.world.state().npcs[&giver];
    assert_eq!(npc.id, original);
    assert!(npc.health > 0);
    assert_eq!(npc.ai.phase(), ai::Phase::Idle);
    assert_eq!(npc.ai.next_sequence(), Some(c.tick() + 1));
    assert_eq!(
        npc.events.clock_ms(),
        c.tick() * u64::from(c.world.profile().tick_ms)
    );
    assert_eq!(c.projection.npc_guid(original), Some(original_guid));
}

#[test]
fn distinct_source_quest_loop_stock_commands_reopen_each_boundary_in_both_orders_and_role_ids() {
    for (hostile, giver) in [(2, 3), (30, 20)] {
        for order in [[hostile, giver], [giver, hostile]] {
            let h = Harness::new();
            let data = loop_fixture::profile_with_spawns(hostile, giver, order);
            let controls = Controls::default();
            let mut c = enter(&h, data.clone(), false, controls.clone());
            let b = c.world.state().npcs[&giver].id;
            let b_guid = guid(&c, giver);
            let a = c.world.state().npcs[&hostile].id;
            let a_guid = guid(&c, hostile);
            assert_ne!(a_guid, b_guid);
            c.initial_events().unwrap();
            let before = c.world.checkpoint().unwrap();
            assert!(matches!(
                &c.receive(ClientCommand::QuestgiverHello { npc: b_guid })
                    .unwrap()
                    .events[..],
                [SessionEvent::QuestGreeting(_)]
            ));
            assert!(matches!(
                &c.receive(ClientCommand::QuestQuery { quest: loop_fixture::QUEST }).unwrap().events[..],
                [SessionEvent::QuestTemplate(template)] if template.objectives[0].creature_or_go == 60001
                    && template.objectives[0].item_id == loop_fixture::ITEM
                    && template.objectives[0].item_count == 1
            ));
            assert!(matches!(
                &c.receive(ClientCommand::QuestgiverQuery {
                    npc: b_guid,
                    quest: loop_fixture::QUEST
                })
                .unwrap()
                .events[..],
                [SessionEvent::QuestDetail(_)]
            ));
            assert_eq!(c.world.checkpoint().unwrap(), before);
            assert!(c.actions.is_empty());
            accept(&mut c, giver);
            assert_eq!(
                progress(&c)
                    .quests
                    .progress(loop_fixture::QUEST)
                    .unwrap()
                    .status,
                q::Status::Incomplete
            );
            c = reopen(&h, c, &data, &controls);
            assert_giver_live(&c, giver, b, b_guid);

            let killed = kill(&mut c, hostile);
            let start = killed
                .iter()
                .position(|e| matches!(e, SessionEvent::SpellStart { spell_id: 133, .. }))
                .unwrap();
            let launch = killed
                .iter()
                .position(|e| matches!(e, SessionEvent::SpellGo { spell_id: 133, .. }))
                .unwrap();
            let impact = killed
                .iter()
                .position(|e| matches!(e, SessionEvent::SpellDamageLog(_)))
                .unwrap();
            let xp = killed
                .iter()
                .position(|e| matches!(e, SessionEvent::XpGain(x) if x.kill))
                .unwrap();
            let credit = killed
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        SessionEvent::QuestObjectiveKill {
                            quest_id: 70001,
                            count: 1,
                            ..
                        }
                    )
                })
                .unwrap();
            assert!(start < launch && launch < impact && impact < xp && xp < credit);
            let npc = &c.world.state().npcs[&hostile];
            assert_eq!(npc.id, a);
            assert_eq!(npc.original_loot_recipient, Some(c.world.state().player));
            assert!(npc.corpse_loot.is_some());
            assert_eq!(
                progress(&c).credited_generations.get(&hostile),
                Some(&a.generation)
            );
            let q = progress(&c).quests.progress(loop_fixture::QUEST).unwrap();
            assert_eq!(q.creatures[0], 1);
            assert_eq!(q.items[0], 0);
            assert_eq!(q.status, q::Status::Incomplete);
            assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 0);
            assert_eq!(progress(&c).money, 11);
            assert!(matches!(
                &c.receive(ClientCommand::QuestgiverComplete { npc: b_guid, quest: loop_fixture::QUEST }).unwrap().events[..],
                [SessionEvent::QuestProgress(p)] if !p.is_complete
            ));
            assert_giver_live(&c, giver, b, b_guid);
            c = reopen(&h, c, &data, &controls); // Post-kill, before corpse interaction.

            // A normal saved no-input continuation cannot repeat kill XP or objective credit.
            no_transient_progress(&step(&mut c).events);
            c = reopen(&h, c, &data, &controls); // Pre-loot continuation boundary.
            let opened = open_loot(&mut c, hostile);
            assert!(opened.events.iter().any(|e| matches!(e,
                SessionEvent::LootResponse { guid, gold: 3, items, .. }
                    if *guid == a_guid && items.len() == 1 && items[0].item_id == loop_fixture::ITEM && items[0].count == 1
            )));
            assert!(matches!(
                &c.receive(ClientCommand::ItemQuery { entry: loop_fixture::ITEM, guid: 0 }).unwrap().events[..],
                [SessionEvent::ItemTemplate { info: Some(info), .. }] if info.display_info_id == 70002
            ));
            let collected = collect(&mut c);
            let item_credit = collected
                .events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        SessionEvent::QuestObjectiveItem {
                            item_id: 70001,
                            count: 1
                        }
                    )
                })
                .unwrap();
            let stored = collected
                .events
                .iter()
                .position(|e| matches!(e, SessionEvent::ItemCreate { .. }))
                .unwrap();
            assert!(item_credit < stored);
            let item_guid = collected
                .events
                .iter()
                .find_map(|e| match e {
                    SessionEvent::ItemCreate { guid, .. } => Some(*guid),
                    _ => None,
                })
                .unwrap();
            assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 1);
            assert_eq!(progress(&c).money, 14);
            assert_eq!(
                progress(&c)
                    .quests
                    .progress(loop_fixture::QUEST)
                    .unwrap()
                    .status,
                q::Status::Complete
            );
            c = reopen(&h, c, &data, &controls);
            let collected_inventory = inventory(&c).clone();
            let collected_quests = progress(&c).quests.clone();
            let collected_money = progress(&c).money;
            let corpse_loot = c.world.state().npcs[&hostile].corpse_loot.clone();
            // The same generation's exhausted corpse is rejected by the source loot host,
            // even after a fresh stock Loot command following process loss.
            let reopened_corpse = open_loot(&mut c, hostile);
            assert!(reopened_corpse.events.iter().any(|event| matches!(event,
                SessionEvent::LootReleaseResponse { guid, .. } if *guid == a_guid)));
            no_transient_progress(&reopened_corpse.events);
            assert!(!reopened_corpse
                .events
                .iter()
                .any(|event| matches!(event, SessionEvent::LootResponse { .. })));
            assert!(c
                .receive(ClientCommand::AutostoreLootItem { slot: 0 })
                .is_err());
            assert!(c.receive(ClientCommand::LootMoney).is_err());
            assert_eq!(inventory(&c), &collected_inventory);
            assert_eq!(progress(&c).quests, collected_quests);
            assert_eq!(progress(&c).money, collected_money);
            assert_eq!(c.world.state().npcs[&hostile].corpse_loot, corpse_loot);
            assert_giver_live(&c, giver, b, b_guid);
            assert!(matches!(
                &c.receive(ClientCommand::QuestgiverRequestReward {
                    npc: b_guid,
                    quest: loop_fixture::QUEST
                })
                .unwrap()
                .events[..],
                [SessionEvent::QuestOffer(_)]
            ));
            choose_reward(&mut c, giver);
            let rewarded = step(&mut c);
            let destroyed = rewarded
                .events
                .iter()
                .position(|e| {
                    matches!(e,
                SessionEvent::ObjectDestroyed(guid) if *guid == item_guid)
                })
                .unwrap();
            let reward = rewarded
                .events
                .iter()
                .position(|e| {
                    matches!(
                        e,
                        SessionEvent::QuestComplete(_)
                            | SessionEvent::XpGain(_)
                            | SessionEvent::LevelUp(_)
                    )
                })
                .unwrap();
            assert!(destroyed < reward);
            assert!(rewarded.events.iter().any(
                |e| matches!(e, SessionEvent::QuestComplete(q) if q.quest_id == loop_fixture::QUEST)
            ));
            assert!(rewarded
                .events
                .iter()
                .any(|e| matches!(e, SessionEvent::XpGain(x) if !x.kill)));
            assert!(rewarded
                .events
                .iter()
                .any(|e| matches!(e, SessionEvent::LevelUp(_))));
            assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 0);
            assert_eq!(progress(&c).money, 21);
            assert_eq!(c.world.player_level(), 3);
            assert!(
                progress(&c)
                    .quests
                    .progress(loop_fixture::QUEST)
                    .unwrap()
                    .rewarded
            );
            c = reopen(&h, c, &data, &controls);
            let quest_before = progress(&c).clone();
            choose_reward(&mut c, giver);
            no_transient_progress(&step(&mut c).events);
            assert_eq!(progress(&c).money, quest_before.money);
            assert_eq!(progress(&c).player, quest_before.player);
            assert_eq!(progress(&c).quests, quest_before.quests);
            assert_eq!(progress(&c).inventory, quest_before.inventory);
            assert_giver_live(&c, giver, b, b_guid);
            assert_source_hosts(&controls);
            c.shutdown().unwrap();
        }
    }
}
