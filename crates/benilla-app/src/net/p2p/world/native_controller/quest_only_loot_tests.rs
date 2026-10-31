// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored solo collection objective through the actual controller, projection and durable store.
use super::*;

pub(super) fn profile(required: u32) -> n::Profile {
    let (mut profile, input, mut rows) = loop_fixture::setup();
    let quest = &mut rows.quests[0];
    quest.special_flags = q::DELIVER;
    quest.required_creature = [0; 4];
    quest.required_creature_count = [0; 4];
    quest.required_item_count = [required, 0, 0, 0];
    let loot = rows.loot.as_mut().unwrap();
    loot.captures[0].rows[0].chance_or_quest_chance_bits = (-80f32).to_bits();
    loot.rules.quest_drop_rate = benilla_sim::loot::QuestDropRate {
        multiplier_bits: 1.25f32.to_bits(),
    };
    let mut items: Vec<_> = loot.inventory.items().cloned().collect();
    let item = items
        .iter_mut()
        .find(|item| item.entry == loop_fixture::ITEM)
        .unwrap();
    item.class = 12;
    item.bonding = 4;
    loot.inventory = n::synthetic_loot_fixture::inventory_catalog(items);
    for creature in &mut loot.creatures {
        creature.corpse_acceleration_decay_ms = 1000;
    }
    let hostile = profile
        .npcs
        .iter_mut()
        .find(|npc| npc.ai.creature_id == loop_fixture::HOSTILE)
        .unwrap();
    let combat = hostile.combat.as_mut().unwrap();
    combat.minimum_damage = benilla_sim::classic_spell::SpellFloat::ZERO;
    combat.maximum_damage = benilla_sim::classic_spell::SpellFloat::ZERO;
    profile.initialize_quest_progression(&input, rows).unwrap();
    profile
}

fn respawn(c: &mut NativeController, old: runtime::NpcId) {
    for _ in 0..500 {
        if c.world.state().npcs[&old.spawn].id != old && c.world.state().resources.caster.mana >= 30
        {
            return;
        }
        step(c);
    }
    panic!(
        "authored respawn/mana wait failed: old={old:?}, current={:?}, phase={:?}, mana={}",
        c.world.state().npcs[&old.spawn].id,
        c.world.state().npcs[&old.spawn].ai.phase(),
        c.world.state().resources.caster.mana
    );
}

#[test]
fn eight_quest_only_items_complete_only_on_collection_and_survive_each_reopen_boundary() {
    let h = Harness::new();
    let mut data = profile(8);
    let hostile = loop_fixture::HOSTILE;
    let giver = loop_fixture::GIVER;
    let target = data
        .npcs
        .iter_mut()
        .find(|npc| npc.ai.creature_id == hostile)
        .unwrap();
    target.ai.corpse_decay_ms = 1000;
    target.ai.respawn_delay_seconds = 2;
    let controls = Controls::default();
    let mut c = enter(&h, data.clone(), false, controls.clone());
    c.initial_events().unwrap();
    kill(&mut c, hostile);
    assert!(c.world.state().npcs[&hostile]
        .corpse_loot
        .as_ref()
        .unwrap()
        .generated
        .items
        .is_empty());
    accept(&mut c, giver);
    c = reopen(&h, c, &data, &controls);
    for count in 1..=8 {
        let old = c.world.state().npcs[&hostile].id;
        respawn(&mut c, old);
        c = reopen(&h, c, &data, &controls);
        kill(&mut c, hostile);
        let generated = &c.world.state().npcs[&hostile]
            .corpse_loot
            .as_ref()
            .unwrap()
            .generated;
        assert_eq!(generated.items.len(), 1);
        assert!(generated.items[0].quest_only);
        assert_eq!(
            inventory(&c).item_count(loop_fixture::ITEM).unwrap(),
            count - 1
        );
        c = reopen(&h, c, &data, &controls);
        let opened = open_loot(&mut c, hostile);
        assert!(opened.events.iter().any(|event| matches!(event,
            SessionEvent::LootResponse { items, .. } if items.len() == 1 && items[0].count == 1)));
        let collected = collect(&mut c);
        let credit = collected.events.iter().position(|event| matches!(event,
            SessionEvent::QuestObjectiveItem { item_id, count: 1 } if *item_id == loop_fixture::ITEM)).unwrap();
        let stored = collected
            .events
            .iter()
            .position(|event| {
                matches!(
                    event,
                    SessionEvent::ItemCreate { .. } | SessionEvent::ItemPushResult(_)
                )
            })
            .unwrap();
        assert!(credit < stored);
        assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), count);
        let quest = progress(&c).quests.progress(loop_fixture::QUEST).unwrap();
        assert_eq!(quest.items[0], count);
        assert_eq!(
            quest.status,
            if count == 8 {
                q::Status::Complete
            } else {
                q::Status::Incomplete
            }
        );
        assert!(c
            .receive(ClientCommand::AutostoreLootItem { slot: 0 })
            .is_err());
    }
    c = reopen(&h, c, &data, &controls);
    let old = c.world.state().npcs[&hostile].id;
    respawn(&mut c, old);
    kill(&mut c, hostile);
    assert!(c.world.state().npcs[&hostile]
        .corpse_loot
        .as_ref()
        .unwrap()
        .generated
        .items
        .is_empty());
    choose_reward(&mut c, giver);
    step(&mut c);
    assert!(
        progress(&c)
            .quests
            .progress(loop_fixture::QUEST)
            .unwrap()
            .rewarded
    );
    assert_eq!(inventory(&c).item_count(loop_fixture::ITEM).unwrap(), 0);
    c = reopen(&h, c, &data, &controls);
    let before = progress(&c).clone();
    choose_reward(&mut c, giver);
    no_transient_progress(&step(&mut c).events);
    assert_eq!(progress(&c).quests, before.quests);
    assert_eq!(progress(&c).inventory, before.inventory);
    assert_eq!(progress(&c).money, before.money);
    let old = c.world.state().npcs[&hostile].id;
    respawn(&mut c, old);
    kill(&mut c, hostile);
    assert!(c.world.state().npcs[&hostile]
        .corpse_loot
        .as_ref()
        .unwrap()
        .generated
        .items
        .is_empty());
    c.shutdown().unwrap();
}
