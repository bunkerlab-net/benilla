// SPDX-License-Identifier: GPL-3.0-or-later
// Authored rows; pinned RewardQuest + exact storage source differential proof.
use benilla_sim::inventory;
use benilla_sim::inventory::{Audit, Item, Spell};
use benilla_sim::quest::{
    Binary32Rate, QuestEligibilityInput, QuestPlayerIdentity, QuestRewardInput, QuestXpInput,
    RewardContext,
};
use benilla_sim::quest_lifecycle::*;
use std::collections::BTreeMap;
fn excluded() -> ExcludedFields {
    ExcludedFields {
        required_skill: 0,
        required_skill_value: 0,
        required_condition: 0,
        rep_objective_faction: 0,
        rep_objective_value: 0,
        required_min_rep_faction: 0,
        required_min_rep_value: 0,
        required_max_rep_faction: 0,
        required_max_rep_value: 0,
        quest_type: 0,
        suggested_players: 0,
        limit_time: 0,
        exclusive_group: 0,
        breadcrumb_for_quest: 0,
        source_item: 0,
        source_item_count: 0,
        source_spell: 0,
        required_source: [0; 4],
        required_source_count: [0; 4],
        required_spell: [0; 4],
        reward_choice_item: [0; 6],
        reward_choice_count: [0; 6],
        reward_item: [0; 4],
        reward_item_count: [0; 4],
        reward_rep_faction: [0; 5],
        reward_rep_value: [0; 5],
        reward_faction_flags: 0,
        reward_spell: 0,
        reward_spell_cast: 0,
        reward_mail_template: 0,
        reward_mail_delay: 0,
        start_script: 0,
        complete_script: 0,
    }
}
fn audit() -> DependencyAudit {
    DependencyAudit {
        complete_relation_scan: true,
        questgiver_hooks: Evidence::Absent,
        spell_area: Evidence::Absent,
        collection_item_effects: Evidence::Absent,
    }
}
fn relations(rows: &[QuestRow]) -> QuestRelations {
    let all: Vec<_> = rows
        .iter()
        .map(|q| CreatureQuestRelation {
            creature: 900,
            quest: q.id,
        })
        .collect();
    QuestRelations {
        starters: all.clone(),
        finishers: all,
    }
}
fn catalog(rows: Vec<QuestRow>) -> Catalog {
    let r = relations(&rows);
    Catalog::new(rows, r, audit()).unwrap()
}
fn inventory_audit() -> Audit {
    Audit {
        item_creation_and_loot_scripts: inventory::Evidence::Absent,
        equipped_bags: inventory::Evidence::Absent,
        bank_and_keyring_items: inventory::Evidence::Absent,
        trade_or_temporary_item_loot: inventory::Evidence::Absent,
        additional_item_mutations: inventory::Evidence::Absent,
    }
}
fn item(entry: u32, stackable: i32, maximum_count: i32) -> Item {
    Item {
        entry,
        class: 15,
        inventory_type: 0,
        display_id: 90001,
        quality: 1,
        flags: 0,
        extra_flags: 0,
        stackable,
        maximum_count,
        bonding: 0,
        maximum_durability: 0,
        bag_family: 0,
        container_slots: 0,
        random_property: 0,
        start_quest: 0,
        duration: 0,
        item_set: 0,
        area: 0,
        map: 0,
        lock_id: 0,
        spells: [Spell {
            id: 0,
            trigger: 0,
            charges: 0,
        }; 5],
    }
}
fn event(e: &Event) -> String {
    match e {
        Event::Host(HostEffect::StoreRewardItem { item, count }) => format!("grant:{item}:{count}"),
        Event::LogSet { slot, quest } => format!("log:{slot}:{}", quest.unwrap_or(0)),
        Event::AcceptHook { quest, giver } => format!("accept:{quest}:{giver}"),
        Event::WorldObjects { .. } => "world".into(),
        Event::LogComplete { slot, complete } => format!("bit:{slot}:{}", u8::from(*complete)),
        Event::KillCredit {
            quest,
            guid,
            objective,
            count,
        } => format!("kill:{quest}:{guid}:{objective}:{count}"),
        Event::ItemCredit {
            quest,
            objective,
            previous,
            added,
        } => format!("item:{quest}:{objective}:{previous}:{added}"),
        Event::RemoveTimedQuest(id) => format!("timed:{id}"),
        Event::RewardReputation(id) => format!("rep:{id}"),
        Event::Host(HostEffect::DestroyCarriedStack { item, count }) => {
            format!("destroy:{item}:{count}")
        }

        Event::Host(HostEffect::GiveXp(xp)) => format!("xp:{xp}"),
        Event::Host(HostEffect::ModifyMoney(d)) => format!("money:{d}"),
        Event::RewardAnnouncement { quest, xp } => format!("announce:{quest}:{xp}"),
        Event::RewardHook { quest, giver } => format!("reward:{quest}:{giver}"),
    }
}

#[derive(Clone, Debug)]
struct InventoryHost {
    catalog: inventory::Catalog,
    state: inventory::State,
    money: u32,
}
impl Host for InventoryHost {
    fn participation(&self) -> Participation {
        Participation::Solo
    }
    fn identity(&self) -> QuestPlayerIdentity {
        QuestPlayerIdentity {
            level: 1,
            class: 8,
            race: 1,
        }
    }
    fn reward_context(&self) -> RewardContext {
        RewardContext {
            player_level: 1,
            max_player_level: 60,
            xp_rate: Binary32Rate::ONE,
            money_rate: Binary32Rate::ONE,
        }
    }
    fn money(&self) -> u32 {
        self.money
    }
    fn item_count(&self, item: u32, _: bool) -> Result<u32, Error> {
        self.state
            .item_count(item)
            .map_err(|_| Error::Host("item count"))
    }
    fn removal_steps(&self, item: u32, count: u32) -> Result<Vec<u32>, Error> {
        self.state
            .removal_steps(&self.catalog, item, count)
            .map_err(|_| Error::Host("removal steps"))
    }
    fn can_store_reward(
        &self,
        item: u32,
        count: u32,
        destinations: &mut Vec<inventory::Destination>,
    ) -> Result<inventory::InventoryResult, Error> {
        self.state
            .can_store_with_destinations(&self.catalog, item, count, destinations)
            .map(|p| p.result)
            .map_err(|_| Error::Host("storage plan"))
    }
    fn apply(&mut self, effect: &HostEffect) -> Result<(), Error> {
        match *effect {
            HostEffect::StoreRewardItem { item, count } => {
                self.state
                    .store_new(&self.catalog, item, count, |_, _, _| Ok(()))
                    .map_err(|_| Error::Host("store"))?;
            }
            HostEffect::DestroyCarriedStack { item, count } => self
                .state
                .remove_first_stack(&self.catalog, item, count)
                .map_err(|_| Error::Host("remove"))?,
            HostEffect::GiveXp(xp) => assert_eq!(xp, 0),
            HostEffect::ModifyMoney(delta) => {
                self.money =
                    (i64::from(self.money) + i64::from(delta)).clamp(0, i64::from(MAX_MONEY)) as u32
            }
        }
        Ok(())
    }
}
fn quest(id: u32) -> QuestRow {
    QuestRow {
        id,
        method: 2,
        active: true,
        eligibility: QuestEligibilityInput {
            minimum_level: 1,
            maximum_level: 60,
            required_classes: 0,
            required_races: 0,
        },
        reward: QuestRewardInput {
            xp: QuestXpInput {
                quest_level: 1,
                reward_money_max_level: 0,
            },
            reward_or_required_money: 0,
            quest_flags: 0,
        },
        special_flags: 0,
        previous_quest: 0,
        next_quest: 0,
        next_in_chain: 0,
        required_item: [0; 4],
        required_item_count: [0; 4],
        required_creature: [0; 4],
        required_creature_count: [0; 4],
        excluded: excluded(),
    }
}
fn recipe(c: u32) -> (Catalog, State, InventoryHost, u32) {
    let stack = [1, 2, 7, 255][c as usize % 4];
    let maximum = if c.is_multiple_of(5) { 1 + c % 13 } else { 0 };
    let items = inventory::Catalog::new(
        vec![
            item(70001, stack, maximum as i32),
            item(70002, stack, 0),
            item(70003, 1, 0),
        ],
        inventory_audit(),
    )
    .unwrap();
    let mut inventory = inventory::State::new(BTreeMap::new(), 1000).unwrap();
    let fill = (c / 4) % 17;
    for i in 0..fill {
        seed(&mut inventory, 23 + i as u8, 70003);
    }
    if fill < 16 && c.is_multiple_of(3) {
        seed(&mut inventory, 23 + fill as u8, 70001);
    }
    let reward_count = [1, 2, 3, 7, 15, 254, 256, 4096][(c / 7) as usize % 8];
    let mut q = quest(1);
    let choice = c % 6;
    let choices = (c / 5) % 7;
    for i in 0..choices as usize {
        q.excluded.reward_choice_item[i] = if i % 2 == 1 { 70002 } else { 70001 };
        q.excluded.reward_choice_count[i] = reward_count;
    }
    let fixed = (c / 11) % 5;
    for i in 0..fixed as usize {
        q.excluded.reward_item[i] = if (c / 13) % 2 == 1 { 70001 } else { 70002 };
        q.excluded.reward_item_count[i] = 1 + (reward_count + i as u32) % 17;
    }
    if c.is_multiple_of(9) && fill < 16 && c.is_multiple_of(3) {
        q.special_flags = DELIVER;
        q.required_item[0] = 70001;
        q.required_item_count[0] = 1;
    }
    let mut q2 = quest(2);
    q2.special_flags = DELIVER;
    q2.required_item[0] = 70001;
    q2.required_item_count[0] = 3 + c % 61;
    let catalog = catalog(vec![q, q2]);
    let mut state = State::new(&catalog);
    let mut host = InventoryHost {
        catalog: items,
        state: inventory,
        money: 0,
    };
    for quest in [1, 2] {
        let tx = reduce(
            &catalog,
            &state,
            &host,
            state.next_sequence(),
            Command::Accept { quest, giver: 900 },
        )
        .unwrap();
        state = tx.state;
        host = tx.host;
    }
    (catalog, state, host, choice)
}
fn seed(state: &mut inventory::State, slot: u8, item: u32) {
    state.slots.insert(
        slot,
        inventory::Instance {
            id: u64::from(slot) + 1,
            item,
            count: 1,
            bound: false,
            charges: [0; 5],
            durability: 0,
            maximum_durability: 0,
        },
    );
}
fn stored_slots(text: &str) -> BTreeMap<u8, inventory::Instance> {
    text.split(',')
        .filter(|s| !s.is_empty())
        .map(|instance| {
            let n = instance
                .split(':')
                .map(|s| s.parse::<u64>().unwrap())
                .collect::<Vec<_>>();
            (
                n[0] as u8,
                inventory::Instance {
                    id: n[1],
                    item: n[2] as u32,
                    count: n[3] as u32,
                    bound: n[4] != 0,
                    durability: n[5] as u32,
                    maximum_durability: n[6] as u32,
                    charges: std::array::from_fn(|i| n[7 + i] as i32),
                },
            )
        })
        .collect()
}
#[test]
fn exact_pinned_reward_preflight_then_fresh_store_and_item_callbacks() {
    let mut cases = 0;
    let mut refusals = 0;
    let mut grants = 0;
    let mut skipped_grants = 0;
    for line in include_str!("fixtures/quest_reward_items_cmangos.tsv").lines() {
        let cols = line.split('\t').collect::<Vec<_>>();
        let case = cols[0].parse::<u32>().unwrap();
        let (catalog, mut state, mut host, choice) = recipe(case);
        let status = cols[1]
            .split(':')
            .map(|s| s.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            can_reward(&catalog, &state, &host, 1).is_ok(),
            status[0] != 0,
            "base case {case}"
        );
        let planned = can_reward_choice(&catalog, &state, &host, 1, choice);
        assert_eq!(planned.is_ok(), status[1] != 0, "preflight case {case}");
        if let Err(Error::Rejected(Gate::RewardStorage { result, .. })) = &planned {
            assert_eq!(
                u32::from(result.protocol()),
                status[2],
                "refusal case {case}"
            );
            refusals += 1;
        }
        let mut emitted = Vec::new();
        if planned.is_ok() {
            let requested = reward_items(catalog.row(1).unwrap(), choice).unwrap().len();
            let tx = reduce(
                &catalog,
                &state,
                &host,
                state.next_sequence(),
                Command::Reward {
                    quest: 1,
                    giver: 900,
                    announce: true,
                    choice,
                },
            )
            .unwrap();
            let received = tx
                .events
                .iter()
                .filter(|e| matches!(e, Event::Host(HostEffect::StoreRewardItem { .. })))
                .count();
            grants += received;
            skipped_grants += requested - received;
            emitted = tx.events.iter().map(event).collect();
            state = tx.state;
            host = tx.host;
        }
        assert_eq!(
            host.state.next_item,
            cols[2].parse::<u64>().unwrap(),
            "allocation case {case}"
        );
        assert_eq!(
            host.state.slots,
            stored_slots(cols[3]),
            "inventory case {case}"
        );
        assert_eq!(emitted.join(","), cols[4], "events case {case}");
        let end = cols[5]
            .split(':')
            .map(|s| s.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let observer = state.progress(2).unwrap();
        assert_eq!(
            observer.status,
            if end[0] == 1 {
                Status::Complete
            } else {
                Status::Incomplete
            },
            "observer status case {case}"
        );
        assert_eq!(observer.items[0], end[1], "observer count case {case}");
        assert_eq!(
            state.progress(1).unwrap().rewarded,
            end[2] != 0,
            "rewarded case {case}"
        );
        cases += 1;
    }
    assert_eq!(cases, 1024);
    assert!(refusals > 100);
    assert!(grants > 100);
    assert!(
        skipped_grants > 0,
        "later source unique refusal must be exercised"
    );
}

#[test]
fn loader_normalized_objectives_and_dense_reward_items_are_required() {
    let mut q = quest(1);
    q.special_flags = DELIVER;
    q.required_item[0] = 70001;
    q.required_item_count[0] = 63;
    let build = |q: QuestRow| {
        let rows = vec![q];
        let relations = relations(&rows);
        Catalog::new(rows, relations, audit())
    };
    assert!(build(q.clone()).is_ok());
    q.required_item_count[0] = 64;
    assert!(matches!(build(q), Err(Error::Invalid("item_objective"))));
    let mut q = quest(1);
    q.excluded.reward_item[1] = 70001;
    q.excluded.reward_item_count[1] = 1;
    assert!(matches!(
        build(q),
        Err(Error::Unsupported("sparse_reward_items"))
    ));
    let mut q = quest(1);
    q.excluded.reward_choice_item[0] = 70001;
    assert!(matches!(build(q), Err(Error::Invalid("reward_item_pair"))));
    let (catalog, state, host, _) = recipe(1);
    assert_eq!(
        can_reward_choice(&catalog, &state, &host, 1, 6),
        Err(Error::Rejected(Gate::RewardChoice))
    );
}
