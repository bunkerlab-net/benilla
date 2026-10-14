// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored native item rewards through the exact app controller and durable outbox.
use super::*;
use benilla_protocol::messages as p;
use benilla_sim::quest_lifecycle as q;

struct RewardCatalog {
    quest: q::QuestRow,
    items: inv::Catalog,
}
impl RewardCatalog {
    fn new(profile: &n::Profile) -> Self {
        let g = profile.quest_progression.as_ref().unwrap();
        Self {
            quest: g.quest(70001).unwrap().clone(),
            items: g.loot.as_ref().unwrap().inventory.clone(),
        }
    }
}
impl ClientCatalog for RewardCatalog {
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
    fn player_power_type(&self, player: PlayerId) -> Option<u8> {
        LootCatalog.player_power_type(player)
    }
    fn player_bytes_2(&self, player: PlayerId) -> Option<u32> {
        LootCatalog.player_bytes_2(player)
    }
    fn item_metadata(&self, entry: u32) -> Option<p::ItemInfo> {
        let item = self.items.item(entry).ok()?;
        let mut info = LootCatalog.item_metadata(if entry == 900001 { 900001 } else { 70001 })?;
        info.name = format!("Authored reward item {entry}");
        info.stackable = item.stackable as u32;
        info.max_count = item.maximum_count as u32;
        Some(info)
    }
    fn quest_metadata(&self, entry: u32) -> Option<benilla_world_client::QuestMetadata> {
        let mut m = Catalog.quest_metadata(entry)?;
        let row = &self.quest;
        m.template.flags = row.reward.quest_flags.into();
        m.template.rewards = std::array::from_fn(|i| {
            (
                row.excluded.reward_item[i],
                row.excluded.reward_item_count[i],
            )
        });
        m.template.choices = std::array::from_fn(|i| {
            (
                row.excluded.reward_choice_item[i],
                row.excluded.reward_choice_count[i],
            )
        });
        for (i, o) in m.template.objectives.iter_mut().enumerate() {
            o.creature_or_go = row.required_creature[i] as u32;
            o.required_count = row.required_creature_count[i];
            o.item_id = row.required_item[i];
            o.item_count = row.required_item_count[i];
        }
        Some(m)
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
fn data(
    carried: u32,
    required: u32,
    choices: &[(u32, u32)],
    fixed: &[(u32, u32)],
    hidden: bool,
    unique: bool,
) -> n::Profile {
    let (mut p, input, mut data) = setup(carried);
    let row = &mut data.quests[0];
    row.special_flags = if required > 0 { q::DELIVER } else { 0 };
    row.required_creature = [0; 4];
    row.required_creature_count = [0; 4];
    row.required_item = [if required > 0 { 70001 } else { 0 }, 0, 0, 0];
    row.required_item_count = [required, 0, 0, 0];
    if hidden {
        row.reward.quest_flags |= p::quest_flags::HIDDEN_REWARDS as u16;
    }
    for (i, &(id, count)) in choices.iter().enumerate() {
        row.excluded.reward_choice_item[i] = id;
        row.excluded.reward_choice_count[i] = count;
    }
    for (i, &(id, count)) in fixed.iter().enumerate() {
        row.excluded.reward_item[i] = id;
        row.excluded.reward_item_count[i] = count;
    }
    let l = data.loot.as_mut().unwrap();
    let mut items = l.inventory.items().cloned().collect::<Vec<_>>();
    for entry in [70003, 70004] {
        let mut item = loot::item(entry);
        if unique && entry == 70004 {
            item.maximum_count = 1;
        }
        items.push(item);
    }
    l.inventory = loot::inventory_catalog(items);
    p.initialize_quest_progression(&input, data).unwrap();
    p
}
fn enter_reward(h: &Harness, profile: n::Profile, reopen: bool) -> NativeController {
    let catalog = RewardCatalog::new(&profile);
    NativeController::enter_component_test(
        &h.root,
        &h.profile,
        &h.character,
        if reopen {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        profile,
        &catalog,
        loot::LootAuthority::default(),
    )
    .unwrap()
}
fn giver(c: &NativeController) -> u64 {
    c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap()
}
fn accept(c: &mut NativeController) {
    c.receive(ClientCommand::QuestgiverAccept {
        npc: giver(c),
        quest: 70001,
    })
    .unwrap();
    step(c);
    assert_eq!(
        progress(c).quests.progress(70001).unwrap().status,
        q::Status::Complete
    );
}
fn choose(c: &mut NativeController, choice: u32) {
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: giver(c),
        quest: 70001,
        choice,
    })
    .unwrap();
}
fn pushes(events: &[SessionEvent]) -> Vec<&p::ItemPushResult> {
    events
        .iter()
        .filter_map(|e| {
            if let SessionEvent::ItemPushResult(p) = e {
                Some(p)
            } else {
                None
            }
        })
        .collect()
}
fn completed(events: &[SessionEvent]) -> &p::QuestComplete {
    events
        .iter()
        .find_map(|e| {
            if let SessionEvent::QuestComplete(p) = e {
                Some(p)
            } else {
                None
            }
        })
        .unwrap()
}

#[test]
fn choice_panel_save_failure_retry_restore_and_duplicate_grant_once() {
    let h = Harness::new();
    let profile = data(0, 0, &[(70003, 2), (70004, 1)], &[(70001, 3)], false, false);
    let mut c = enter_reward(&h, profile.clone(), false);
    c.initial_events().unwrap();
    let detail = c
        .receive(ClientCommand::QuestgiverQuery {
            npc: giver(&c),
            quest: 70001,
        })
        .unwrap();
    assert!(
        matches!(&detail.events[..],[SessionEvent::QuestDetail(p)] if p.choices.len()==2 && p.rewards.len()==1)
    );
    assert!(matches!(
        c.receive(ClientCommand::QuestgiverChooseReward {
            npc: u64::MAX,
            quest: 70001,
            choice: 6
        }),
        Err(Error::UnsupportedCommand)
    ));
    assert!(c.actions.is_empty());
    accept(&mut c);
    let offer = c
        .receive(ClientCommand::QuestgiverRequestReward {
            npc: giver(&c),
            quest: 70001,
        })
        .unwrap();
    assert!(
        matches!(&offer.events[..],[SessionEvent::QuestOffer(p)] if p.choices[1].item_id==70004)
    );
    choose(&mut c, 1);
    inject(&h);
    paused(&c.pump(c.tick_duration()));
    assert_eq!(inventory(&c).item_count(70003).unwrap(), 0);
    assert_eq!(inventory(&c).item_count(70004).unwrap(), 1);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 3);
    let digest = c.world.state_digest();
    let sequence = c.world.state().next_sequence;
    assert!(c.retry_tick().is_err());
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(
        pushes(&out.events)
            .iter()
            .map(|p| p.item_entry)
            .collect::<Vec<_>>(),
        [70004, 70001]
    );
    assert_eq!(completed(&out.events).items, [(70001, 3)]);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.world.state().next_sequence, sequence);
    drop(c);
    let mut c = enter_reward(&h, profile, true);
    let initial = c.initial_events().unwrap();
    assert!(pushes(&initial).is_empty());
    assert!(!initial
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestComplete(_))));
    choose(&mut c, 1);
    let duplicate = step(&mut c);
    assert!(pushes(&duplicate.events).is_empty());
    assert_eq!(inventory(&c).item_count(70004).unwrap(), 1);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 3);
}

#[test]
fn split_pushes_keep_full_grant_count_flags_and_fixed_only_completion() {
    let h = Harness::new();
    let mut c = enter_reward(
        &h,
        data(0, 0, &[(70003, 7)], &[(70004, 1)], false, false),
        false,
    );
    accept(&mut c);
    choose(&mut c, 0);
    let out = step(&mut c);
    let push = pushes(&out.events);
    assert_eq!(push.len(), 2);
    assert_eq!(
        (push[0].item_entry, push[0].count, push[0].item_slot),
        (70003, 7, u32::MAX)
    );
    assert_eq!(
        (push[1].item_entry, push[1].count, push[1].item_slot),
        (70004, 1, 25)
    );
    assert!(push
        .iter()
        .all(|p| p.from_npc && !p.created && !p.show_in_chat && p.bag_slot == 255));
    assert_eq!(completed(&out.events).items, [(70004, 1)]);
    let last_item = out
        .events
        .iter()
        .rposition(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    let xp = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::XpGain(_)))
        .unwrap();
    assert!(last_item < xp);
}

#[test]
fn full_bag_refusal_keeps_requirements_and_offers_retry_in_packet_order() {
    let h = Harness::new();
    let mut c = enter_reward(&h, data(80, 1, &[(70003, 1)], &[], false, false), false);
    accept(&mut c);
    let before = inventory(&c).clone();
    choose(&mut c, 0);
    let out = step(&mut c);
    assert_eq!(inventory(&c), &before);
    let failure = out
        .events
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::InventoryFailure {
                    reason: 50,
                    bag_slot: 0,
                    ..
                }
            )
        })
        .unwrap();
    let offer = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::QuestOffer(_)))
        .unwrap();
    assert!(failure < offer);
    assert!(pushes(&out.events).is_empty());
    assert!(!progress(&c).quests.progress(70001).unwrap().rewarded);
    // An empty in-range choice is legal and does not grant the chosen item.
    choose(&mut c, 5);
    let out = step(&mut c);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 79);
    assert!(pushes(&out.events).is_empty());
    assert!(completed(&out.events).items.is_empty());
}

#[test]
fn hidden_detail_and_template_hide_rewards_offer_shows_them_and_empty_choice_works() {
    let h = Harness::new();
    let mut c = enter_reward(
        &h,
        data(0, 0, &[(70003, 2)], &[(70004, 1)], true, false),
        false,
    );
    let detail = c
        .receive(ClientCommand::QuestgiverQuery {
            npc: giver(&c),
            quest: 70001,
        })
        .unwrap();
    assert!(
        matches!(&detail.events[..],[SessionEvent::QuestDetail(p)] if p.choices.is_empty() && p.rewards.is_empty() && p.money==0)
    );
    let query = c
        .receive(ClientCommand::QuestQuery { quest: 70001 })
        .unwrap();
    assert!(
        matches!(&query.events[..],[SessionEvent::QuestTemplate(p)] if p.choices==[(0,0);6] && p.rewards==[(0,0);4] && p.money==0)
    );
    accept(&mut c);
    let offer = c
        .receive(ClientCommand::QuestgiverRequestReward {
            npc: giver(&c),
            quest: 70001,
        })
        .unwrap();
    assert!(
        matches!(&offer.events[..],[SessionEvent::QuestOffer(p)] if p.choices.len()==1 && p.rewards.len()==1 && p.money==7)
    );
    choose(&mut c, 5);
    let out = step(&mut c);
    assert_eq!(inventory(&c).item_count(70003).unwrap(), 0);
    assert_eq!(inventory(&c).item_count(70004).unwrap(), 1);
    assert_eq!(pushes(&out.events).len(), 1);
    assert_eq!(completed(&out.events).items, [(70004, 1)]);
}

#[test]
fn refusal_then_success_same_batch_preserves_error_offer_before_push_and_complete() {
    let h = Harness::new();
    let mut c = enter_reward(
        &h,
        data(79, 1, &[(70003, 1), (70001, 1)], &[], false, false),
        false,
    );
    accept(&mut c);
    choose(&mut c, 0);
    choose(&mut c, 1);
    let out = step(&mut c);
    let failure = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::InventoryFailure { .. }))
        .unwrap();
    let offer = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::QuestOffer(_)))
        .unwrap();
    let push = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::ItemPushResult(_)))
        .unwrap();
    let complete = out
        .events
        .iter()
        .position(|e| matches!(e, SessionEvent::QuestComplete(_)))
        .unwrap();
    assert!(failure < offer && offer < push && push < complete);
    assert_eq!(inventory(&c).item_count(70001).unwrap(), 79);
    assert_eq!(inventory(&c).item_count(70003).unwrap(), 0);
    assert_eq!(pushes(&out.events).len(), 1);
    assert!(progress(&c).quests.progress(70001).unwrap().rewarded);
}

#[test]
fn fixed_completion_includes_later_unique_grant_skipped_by_actual_storage() {
    let h = Harness::new();
    let mut c = enter_reward(
        &h,
        data(0, 0, &[], &[(70004, 1), (70004, 1)], false, true),
        false,
    );
    accept(&mut c);
    choose(&mut c, 0);
    let out = step(&mut c);
    assert_eq!(inventory(&c).item_count(70004).unwrap(), 1);
    assert_eq!(pushes(&out.events).len(), 1);
    assert_eq!(completed(&out.events).items, [(70004, 1), (70004, 1)]);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::InventoryFailure { .. })));
}

#[test]
fn incomplete_and_already_rewarded_choices_offer_without_invented_inventory_errors() {
    let h = Harness::new();
    let mut c = enter_reward(&h, data(0, 0, &[(70003, 1)], &[], false, false), false);
    // Valid giver/relation reach CanRewardQuest even before the quest is accepted.
    choose(&mut c, 0);
    let out = step(&mut c);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::QuestOffer(_)))
            .count(),
        1
    );
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::InventoryFailure { .. }
            | SessionEvent::Notification { .. }
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestComplete(_)
    )));
    assert!(progress(&c).quests.progress(70001).is_none());
    accept(&mut c);
    choose(&mut c, 0);
    step(&mut c);
    choose(&mut c, 0);
    // A subsequent invalid-giver request has no RewardRefused callback and must
    // retain the controller's ordinary unsupported-interaction feedback.
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: corpse(&c),
        quest: 70001,
        choice: 0,
    })
    .unwrap();
    let out = step(&mut c);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::QuestOffer(_)))
            .count(),
        1
    );
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::Notification { .. }))
            .count(),
        1
    );
    assert!(!out.events.iter().any(|e| matches!(
        e,
        SessionEvent::InventoryFailure { .. }
            | SessionEvent::ItemPushResult(_)
            | SessionEvent::QuestComplete(_)
    )));
    assert_eq!(inventory(&c).item_count(70003).unwrap(), 1);
}
