// SPDX-License-Identifier: GPL-3.0-or-later
//! Quest 7's public scalar/actor contract in an expressly authored scene.
//! Sources: classic-db ec4f5961, Full_DB/ClassicDB_1_12_1_z2815.sql.gz,
//! quest_template 7/783/15 and creature_{quest,involved}relation; Updates/4769
//! adds the zero spillover mask, Updates/4833 renames it RewFactionFlags.
//! The remaining Updates quest/relation statements do not change these rows
//! (4777's RequiredRaces=255 normalization does not match their value 77).
//! CMaNGOS 8ec338a1 ObjectMgr.cpp:4618 adds KILL|SPEAK to creature objectives.
//! This is no raw-input compiler or production admission proof. Stats, spells,
//! faction dependencies, loot, spawn timing, AI/interaction/spell witnesses and
//! geometry are authored. The legacy component authority has no selected source
//! visitor/combat/spell host; this does not extend the 1|2-owner source scheduler.
//! No installed Northshire map, client content, or live parity is implied.
use super::{synthetic_fixture as f, synthetic_loot_fixture as loot_fixture, *};
use benilla_sim::{player_progression::kill_xp, quest_lifecycle as q, reputation as rep};

const VERMIN: u64 = 2;
const MCBRIDE: u64 = 3;
const WILLEM: u64 = 4;
const QUEST: u32 = 7;
const PREREQUISITE: u32 = 783;
const NEXT: u32 = 15;
const FACTION: u32 = 72;
const SPELL: u32 = 60001;
const ITEM: u32 = 70001;

fn quest_rows(mut row: q::QuestRow) -> Vec<q::QuestRow> {
    row.id = QUEST;
    row.eligibility.maximum_level = 255;
    row.eligibility.required_races = 77;
    row.reward.quest_flags = 8;
    row.reward.xp.quest_level = 2;
    row.reward.xp.reward_money_max_level = 102;
    row.reward.reward_or_required_money = 25;
    row.special_flags = q::KILL | q::SPEAK;
    row.previous_quest = PREREQUISITE as i32;
    row.next_quest = 0;
    row.next_in_chain = 0;
    row.required_item = [0; 4];
    row.required_item_count = [0; 4];
    row.required_creature = [6, 0, 0, 0];
    row.required_creature_count = [10, 0, 0, 0];
    row.excluded.reward_rep_faction = [FACTION, 0, 0, 0, 0];
    row.excluded.reward_rep_value = [250, 0, 0, 0, 0];

    let mut prerequisite = row.clone();
    prerequisite.id = PREREQUISITE;
    prerequisite.previous_quest = 0;
    prerequisite.next_in_chain = QUEST;
    prerequisite.required_creature = [0; 4];
    prerequisite.required_creature_count = [0; 4];
    prerequisite.special_flags = 0;
    prerequisite.reward.xp.quest_level = 1;
    prerequisite.reward.xp.reward_money_max_level = 24;
    prerequisite.reward.reward_or_required_money = 0;
    prerequisite.excluded.reward_rep_value[0] = 75;

    let mut next = row.clone();
    next.id = NEXT;
    next.previous_quest = QUEST as i32;
    next.next_in_chain = 21;
    next.required_creature[0] = 257;
    next.reward.xp.quest_level = 3;
    next.reward.xp.reward_money_max_level = 150;
    next.reward.reward_or_required_money = 40;

    // An unoffered synthetic boundary closes 15's chain reference. This does not
    // represent quest 21 or establish the downstream Northshire quest catalog.
    let mut boundary = prerequisite.clone();
    boundary.id = 21;
    boundary.active = false;
    boundary.next_in_chain = 0;
    boundary.reward.xp.reward_money_max_level = 0;
    boundary.excluded.reward_rep_faction = [0; 5];
    boundary.excluded.reward_rep_value = [0; 5];
    vec![row, prerequisite, next, boundary]
}

fn reputation() -> rep::Catalog {
    // Only the quest reward's faction ID and amount are source facts. The base
    // standing, list slot, no-spillover closure and rates are authored controls.
    rep::Catalog::new_with_general_reputation_auras(
        [(FACTION, 3)]
            .into_iter()
            .map(|(id, reputation_list_id)| rep::Row {
                faction: rep::Faction {
                    id,
                    reputation_list_id,
                    race_masks: [0; 4],
                    class_masks: [0; 4],
                    base_values: [0; 4],
                    flags: [0; 4],
                    team: 0,
                },
                reward_rates: rep::Resolved::Absent,
                spillover: rep::Resolved::Absent,
            })
            .collect(),
        vec![],
        rep::Config {
            gain_rate: spell::SpellFloat::ONE,
            low_level_kill_rate: spell::SpellFloat::ONE,
            low_level_quest_rate: spell::SpellFloat::ONE,
        },
        rep::Audit {
            general_reputation_auras: rep::Evidence::Absent,
            faction_reputation_auras: rep::Evidence::Absent,
            forced_reactions: rep::Evidence::Absent,
            reputation_scripts: rep::Evidence::Absent,
        },
        vec![],
    )
    .unwrap()
}

fn profile() -> Profile {
    let (mut profile, initial, mut data) = loot_fixture::setup();
    for npc in &mut profile.npcs {
        if npc.ai.creature_id == VERMIN {
            npc.entry = 6;
            npc.name = "Authored entry-6 kill target".into();
            npc.ai.corpse_decay_ms = 1500;
            npc.ai.respawn_delay_seconds = 2;
        } else {
            npc.entry = 197;
            npc.name = "Authored McBride role".into();
        }
    }
    let mut willem = profile.npc(MCBRIDE).unwrap().clone();
    willem.entry = 823;
    willem.ai.creature_id = WILLEM;
    willem.ai.spawn = f::point(-3.0);
    willem.name = "Authored Willem role".into();
    profile.npcs.push(willem);
    // The configured level closure is authored, rather than the legacy fixture's
    // three supplied levels; repeated XP rewards must remain reconstructible.
    data.levels = synthetic_quest_fixture::setup_with_levels(60, 60, true, 1.0f32.to_bits())
        .2
        .levels;
    data.quests = quest_rows(data.quests[0].clone());
    data.quest_relationships = synthetic_quest_fixture::relationship_capture(&data.quests);
    let relation = |creature, quest| q::CreatureQuestRelation { creature, quest };
    data.relations = q::QuestRelations {
        starters: vec![
            relation(823, PREREQUISITE),
            relation(197, QUEST),
            relation(197, NEXT),
        ],
        finishers: vec![
            relation(197, PREREQUISITE),
            relation(197, QUEST),
            relation(197, NEXT),
        ],
    };
    data.reputation = Some(reputation());
    data.npc_rows = [6, 197, 823]
        .into_iter()
        .map(|entry| QuestNpcRow {
            entry,
            npc_flags: if entry == 6 { 0 } else { 2 },
            unit_flags: 0,
        })
        .collect();
    let kill = data.kill.creatures[0].clone();
    data.kill.creatures = [6, 197, 823]
        .into_iter()
        .map(|entry| KillCreatureRow {
            entry,
            reputation: kill_xp::ReputationAdmission::ProvenAbsentOnKillRow { entry },
            ..kill.clone()
        })
        .collect();
    let loot = data.loot.as_mut().unwrap();
    // Authored one-hop body/reference captures, not public loader evidence. The
    // body rolls 50% at reference rate 2; its positive child yields one token.
    let mut child = loot.captures[0].clone();
    child.loot_id = 199;
    child.source_fingerprint = [0xa7; 32];
    child.loader_order.ordered_result_fingerprint = [0xb7; 32];
    child.rows[0].chance_or_quest_chance_bits = 100.0f32.to_bits();
    child.rows[0].min_count_or_reference = 1;
    child.rows[0].max_count = 1;
    let body = &mut loot.captures[0];
    body.rows[0].item = 0;
    body.rows[0].chance_or_quest_chance_bits = 50.0f32.to_bits();
    body.rows[0].min_count_or_reference = -(child.loot_id as i32);
    body.rows[0].max_count = 1;
    body.items.clear();
    loot.reference_captures = vec![child];
    loot.rules.referenced_rate_bits = 2.0f32.to_bits();
    let creature = loot.creatures[0].clone();
    loot.creatures = [6, 197, 823]
        .into_iter()
        .map(|entry| LootCreatureRow {
            entry,
            loot_id: if entry == 6 { 99 } else { 0 },
            minimum_money: if entry == 6 { 3 } else { 0 },
            maximum_money: if entry == 6 { 3 } else { 0 },
            ..creature.clone()
        })
        .collect();
    profile
        .initialize_quest_progression(&initial, data)
        .unwrap();
    profile
}

fn progress(world: &NativeWorld) -> &QuestProgressionState {
    world.state.quest_progression.as_ref().unwrap()
}
fn quest(world: &NativeWorld, id: u32) -> &q::Progress {
    progress(world).quests.progress(id).unwrap()
}
fn step(world: &mut NativeWorld, authority: &mut impl Authority, command: Option<Command>) {
    let input = command.map(|command| synthetic_quest_fixture::command(world, command));
    world.step(world.state.tick + 1, input, authority).unwrap();
}
fn command(
    world: &mut NativeWorld,
    authority: &mut impl Authority,
    id: u32,
    actor: u64,
    reward: bool,
) {
    let giver = world.state.npcs[&actor].id;
    let command = if reward {
        Command::TurnInQuest { quest: id, giver }
    } else {
        Command::AcceptQuest { quest: id, giver }
    };
    step(world, authority, Some(command));
}
fn rejected(world: &NativeWorld, rejection: Rejection) {
    assert!(world
        .last_tick()
        .unwrap()
        .events
        .contains(&Event::Rejected(rejection)));
}
fn total_xp(world: &NativeWorld) -> u32 {
    let player = progress(world).player.view();
    player.xp + (1..player.level).map(|level| level * 40).sum::<u32>()
}
fn standing(world: &NativeWorld) -> i32 {
    progress(world)
        .reputation(world.profile.quest_progression.as_ref().unwrap(), FACTION)
        .unwrap()
}
fn replay_and_reopen(world: &mut NativeWorld, initial: &mut Checkpoint) {
    let expected = world.checkpoint().unwrap();
    let records = decode_records(&encode_records(world.records()).unwrap()).unwrap();
    let mut replay =
        NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial.clone())
            .unwrap();
    replay.replay(&records).unwrap();
    assert_eq!(replay.checkpoint().unwrap(), expected);
    *world = NativeWorld::restore_synthetic_component_test(
        world.profile.clone(),
        Checkpoint::decode(&expected.encode().unwrap()).unwrap(),
    )
    .unwrap();
    assert_eq!(world.checkpoint().unwrap(), expected);
    *initial = expected;
}
fn kill_generation(world: &mut NativeWorld, authority: &mut impl Authority) -> crate::NpcId {
    let target = world.state.npcs[&VERMIN].id;
    assert!(world.state.npcs[&VERMIN].health > 0);
    for _ in 0..160 {
        let cast = world.state.casts.is_empty().then_some(Command::Cast {
            ability: SPELL,
            target,
        });
        step(world, authority, cast);
        if world.state.npcs[&VERMIN].health == 0 {
            assert_eq!(world.state.npcs[&VERMIN].id, target);
            assert!(world
                .last_tick()
                .unwrap()
                .events
                .contains(&Event::QuestProgression(QuestProgressionEvent::PartyKill {
                    victim: target
                },)));
            return target;
        }
    }
    panic!("authored spell did not kill entry 6");
}
fn respawn(world: &mut NativeWorld, authority: &mut impl Authority, previous: crate::NpcId) {
    for _ in 0..80 {
        step(world, authority, None);
        let npc = &world.state.npcs[&VERMIN];
        if npc.id != previous && npc.health > 0 {
            assert_eq!(npc.id.generation, previous.generation + 1);
            return;
        }
    }
    panic!("authored entry-6 spawn did not respawn");
}

#[test]
fn quest_seven_source_shaped_kills_reward_gates_and_checkpoint_replay() {
    let profile = profile();
    let row = profile
        .quest_progression
        .as_ref()
        .unwrap()
        .quest_catalog()
        .row(QUEST)
        .unwrap();
    assert_eq!(row.required_item, [0; 4]);
    assert_eq!(row.excluded.source_item, 0);
    assert_eq!(row.excluded.required_source, [0; 4]);
    assert_eq!(row.excluded.reward_item, [0; 4]);
    assert_eq!(row.excluded.reward_choice_item, [0; 6]);
    assert_eq!(row.excluded.reward_faction_flags, 0);
    let mut world =
        NativeWorld::synthetic_component_test(profile, [7; 16], f::player(), 42).unwrap();
    let mut initial = world.checkpoint().unwrap();
    let mut authority = loot_fixture::LootAuthority::default();
    assert!(world.profile.source_visitors.is_none());
    assert!(world.profile.source_motion.is_none());
    assert!(!world.profile.source_combat_authority());
    assert!(!world.profile.has_source_spell_authority());

    command(&mut world, &mut authority, QUEST, MCBRIDE, false);
    rejected(&world, Rejection::Quest(q::Gate::Previous));
    command(&mut world, &mut authority, PREREQUISITE, MCBRIDE, false);
    rejected(&world, Rejection::Quest(q::Gate::Giver));
    command(&mut world, &mut authority, PREREQUISITE, WILLEM, false);
    assert_eq!(quest(&world, PREREQUISITE).status, q::Status::Complete);
    command(&mut world, &mut authority, QUEST, MCBRIDE, false);
    rejected(&world, Rejection::Quest(q::Gate::Previous));
    command(&mut world, &mut authority, PREREQUISITE, WILLEM, true);
    rejected(&world, Rejection::Quest(q::Gate::Giver));
    command(&mut world, &mut authority, PREREQUISITE, MCBRIDE, true);
    assert!(quest(&world, PREREQUISITE).rewarded);
    let events = &world.last_tick().unwrap().events;
    let reward = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::QuestProgression(QuestProgressionEvent::Quest {
                    event: q::Event::RewardAnnouncement {
                        quest: PREREQUISITE,
                        ..
                    },
                    ..
                })
            )
        })
        .unwrap();
    let offer = events
        .iter()
        .position(|event| {
            matches!(
                event,
                Event::QuestProgression(QuestProgressionEvent::ChainDetails { quest: QUEST, .. })
            )
        })
        .unwrap();
    assert!(reward < offer);
    replay_and_reopen(&mut world, &mut initial);
    assert_eq!(standing(&world), 75);
    command(&mut world, &mut authority, QUEST, WILLEM, false);
    rejected(&world, Rejection::Quest(q::Gate::Giver));
    command(&mut world, &mut authority, QUEST, VERMIN, false);
    rejected(&world, Rejection::QuestInteraction);
    command(&mut world, &mut authority, QUEST, MCBRIDE, false);
    assert_eq!(quest(&world, QUEST).status, q::Status::Incomplete);
    command(&mut world, &mut authority, QUEST, MCBRIDE, true);
    rejected(&world, Rejection::Quest(q::Gate::NotComplete));
    command(&mut world, &mut authority, NEXT, MCBRIDE, false);
    rejected(&world, Rejection::Quest(q::Gate::Previous));

    let money_before = progress(&world).money;
    let mut generations = std::collections::BTreeSet::new();
    for count in 1..=10 {
        let corpse = kill_generation(&mut world, &mut authority);
        assert!(generations.insert(corpse.generation));
        assert_eq!(quest(&world, QUEST).creatures, [count, 0, 0, 0]);
        assert_eq!(quest(&world, QUEST).items, [0; 4]);
        assert_eq!(
            quest(&world, QUEST).status,
            if count == 10 {
                q::Status::Complete
            } else {
                q::Status::Incomplete
            }
        );
        assert_eq!(
            progress(&world).credited_generations[&VERMIN],
            corpse.generation
        );
        assert_eq!(progress(&world).money, money_before);
        assert_eq!(
            progress(&world)
                .inventory
                .as_ref()
                .unwrap()
                .item_count(ITEM)
                .unwrap(),
            0
        );
        let loot = world.state.npcs[&VERMIN].corpse_loot.as_ref().unwrap();
        assert_eq!(loot.generated.items.len(), 1);
        assert_eq!(
            (loot.generated.items[0].item, loot.generated.items[0].count),
            (ITEM, 1)
        );
        assert!(
            matches!(loot.draws[0].purpose, DrawPurpose::LootChance { corpse: id } if id == corpse)
        );
        let death_event = progress(&world).next_death_event;
        let xp = total_xp(&world);
        // A repeated attack on the same lethal generation cannot credit it again.
        step(
            &mut world,
            &mut authority,
            Some(Command::Cast {
                ability: SPELL,
                target: corpse,
            }),
        );
        assert_eq!(quest(&world, QUEST).creatures[0], count);
        assert_eq!(progress(&world).next_death_event, death_event);
        assert_eq!(total_xp(&world), xp);
        assert!(!world
            .last_tick()
            .unwrap()
            .events
            .iter()
            .any(|event| matches!(
                event,
                Event::QuestProgression(QuestProgressionEvent::PartyKill { .. })
            )));
        // Replay each journal segment and reopen serialized checkpoints, including
        // the nine incomplete counters and Complete-with-uncollected-loot state.
        replay_and_reopen(&mut world, &mut initial);
        if count < 10 {
            respawn(&mut world, &mut authority, corpse);
        }
    }
    assert_eq!(generations.len(), 10);
    assert!(!quest(&world, QUEST).rewarded);
    command(&mut world, &mut authority, NEXT, MCBRIDE, false);
    rejected(&world, Rejection::Quest(q::Gate::Previous));
    command(&mut world, &mut authority, QUEST, WILLEM, true);
    rejected(&world, Rejection::Quest(q::Gate::Giver));
    let xp_before = total_xp(&world);
    let rep_before = standing(&world);
    command(&mut world, &mut authority, QUEST, MCBRIDE, true);
    assert!(quest(&world, QUEST).rewarded);
    assert_eq!(progress(&world).money, money_before + 25);
    assert_eq!(standing(&world), rep_before + 250);
    assert_eq!(total_xp(&world), xp_before + 170);
    command(&mut world, &mut authority, NEXT, WILLEM, false);
    rejected(&world, Rejection::Quest(q::Gate::Giver));
    command(&mut world, &mut authority, NEXT, MCBRIDE, false);
    assert_eq!(quest(&world, NEXT).status, q::Status::Incomplete);
    assert_eq!(quest(&world, NEXT).creatures, [0; 4]);

    // The ordinary authored drop is collectible after reward and has no effect
    // on Quest 7's already-earned kill completion or Quest 15's worker objective.
    let corpse = world.state.npcs[&VERMIN].id;
    for command in [
        Command::OpenLoot { corpse },
        Command::LootItem { corpse, slot: 0 },
        Command::LootMoney { corpse },
        Command::ReleaseLoot { corpse },
    ] {
        step(&mut world, &mut authority, Some(command));
    }
    assert_eq!(
        progress(&world)
            .inventory
            .as_ref()
            .unwrap()
            .item_count(ITEM)
            .unwrap(),
        1
    );
    assert_eq!(progress(&world).money, money_before + 28);
    assert_eq!(quest(&world, NEXT).creatures, [0; 4]);
    replay_and_reopen(&mut world, &mut initial);
    let rewarded_money = progress(&world).money;
    let rewarded_xp = total_xp(&world);
    let rewarded_rep = standing(&world);
    command(&mut world, &mut authority, QUEST, MCBRIDE, true);
    rejected(&world, Rejection::Quest(q::Gate::AlreadyRewarded));
    assert_eq!(
        (progress(&world).money, total_xp(&world), standing(&world)),
        (rewarded_money, rewarded_xp, rewarded_rep)
    );
    replay_and_reopen(&mut world, &mut initial);
}
