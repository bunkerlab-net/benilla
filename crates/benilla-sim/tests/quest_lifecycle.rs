// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic algorithm probes only. No native world rows or game assets.
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
fn row(id: u32, kind: u32, money: i32) -> QuestRow {
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
                reward_money_max_level: 60,
            },
            reward_or_required_money: money,
            quest_flags: 0,
        },
        special_flags: match kind {
            0 => KILL | SPEAK,
            1 => DELIVER,
            _ => KILL | SPEAK | DELIVER,
        },
        previous_quest: 0,
        next_quest: 0,
        next_in_chain: 0,
        required_item: [if kind == 0 { 0 } else { 200 }, 0, 0, 0],
        required_item_count: [if kind == 0 { 0 } else { 2 }, 0, 0, 0],
        required_creature: [if kind == 1 { 0 } else { 100 }, 0, 0, 0],
        required_creature_count: [if kind == 1 { 0 } else { 2 }, 0, 0, 0],
        excluded: excluded(),
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
/// Synthetic callback ledger, not an inventory/GiveXP port. Test facts are explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ledger {
    level: u8,
    money: u32,
    xp: u32,
    carried: BTreeMap<u32, u32>,
    bank: BTreeMap<u32, u32>,
    fail: bool,
    fail_after: Option<u32>,
    applied: u32,
    split: bool,
}
impl Ledger {
    fn new() -> Self {
        Self {
            level: 1,
            money: 0,
            xp: 0,
            carried: BTreeMap::new(),
            bank: BTreeMap::new(),
            fail: false,
            fail_after: None,
            applied: 0,
            split: false,
        }
    }
}
impl Host for Ledger {
    fn participation(&self) -> Participation {
        Participation::Solo
    }
    fn identity(&self) -> QuestPlayerIdentity {
        QuestPlayerIdentity {
            level: self.level,
            class: 8,
            race: 1,
        }
    }
    fn reward_context(&self) -> RewardContext {
        RewardContext {
            player_level: self.level,
            max_player_level: 60,
            xp_rate: Binary32Rate::ONE,
            money_rate: Binary32Rate::ONE,
        }
    }
    fn money(&self) -> u32 {
        self.money
    }
    fn item_count(&self, id: u32, include_bank: bool) -> Result<u32, Error> {
        Ok(self.carried.get(&id).copied().unwrap_or(0)
            + if include_bank {
                self.bank.get(&id).copied().unwrap_or(0)
            } else {
                0
            })
    }
    fn removal_steps(&self, _id: u32, count: u32) -> Result<Vec<u32>, Error> {
        Ok(if self.split {
            vec![1; count as usize]
        } else {
            vec![count]
        })
    }
    fn can_store_reward(
        &self,
        _: u32,
        _: u32,
        _: &mut Vec<benilla_sim::inventory::Destination>,
    ) -> Result<benilla_sim::inventory::InventoryResult, Error> {
        Err(Error::Unsupported("fixture reward items"))
    }
    fn apply(&mut self, effect: &HostEffect) -> Result<(), Error> {
        match *effect {
            HostEffect::StoreRewardItem { .. } => {
                return Err(Error::Unsupported("fixture reward items"));
            }
            HostEffect::DestroyCarriedStack { item, count } => {
                *self.carried.get_mut(&item).ok_or(Error::Host("item"))? -= count
            }
            HostEffect::GiveXp(xp) => self.xp += xp,
            HostEffect::ModifyMoney(d) => {
                self.money =
                    (i64::from(self.money) + i64::from(d)).clamp(0, i64::from(MAX_MONEY)) as u32
            }
        }
        self.applied += 1;
        if self.fail || self.fail_after == Some(self.applied) {
            return Err(Error::Host("deliberate_failure_after_mutation"));
        }
        Ok(())
    }
}
fn event(e: &Event) -> String {
    match e {
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
        Event::ReputationVisible(faction) => format!("visible:{faction}"),
        Event::RewardReputation(id) => format!("rep:{id}"),
        Event::Host(HostEffect::DestroyCarriedStack { item, count }) => {
            format!("destroy:{item}:{count}")
        }
        Event::Host(HostEffect::StoreRewardItem { item, count }) => format!("item:{item}:{count}"),
        Event::Host(HostEffect::GiveXp(xp)) => format!("xp:{xp}"),
        Event::Host(HostEffect::ModifyMoney(d)) => format!("money:{d}"),
        Event::RewardAnnouncement { quest, xp } => format!("announce:{quest}:{xp}"),
        Event::RewardHook { quest, giver } => format!("reward:{quest}:{giver}"),
    }
}
fn run(c: &Catalog, state: &mut State, host: &mut Ledger, command: Command) -> Vec<Event> {
    let tx = reduce(c, state, host, state.next_sequence(), command).unwrap();
    *state = tx.state;
    *host = tx.host;
    tx.events
}
fn accept(c: &Catalog, s: &mut State, h: &mut Ledger, id: u32) {
    run(
        c,
        s,
        h,
        Command::Accept {
            quest: id,
            giver: 900,
        },
    );
}
fn kill(c: &Catalog, s: &mut State, h: &mut Ledger) {
    run(
        c,
        s,
        h,
        Command::KillCredit {
            entry: 100,
            guid: 500,
        },
    );
}
fn reward(c: &Catalog, s: &mut State, h: &mut Ledger, id: u32) -> Vec<Event> {
    run(
        c,
        s,
        h,
        Command::Reward {
            choice: 0,
            quest: id,
            giver: 900,
            announce: true,
        },
    )
}

#[test]
fn pinned_original_cpp_lifecycle_traces_match() {
    let lines: Vec<_> = include_str!("fixtures/quest_lifecycle_cmangos.tsv")
        .lines()
        .filter(|line| !line.starts_with("g ") && !line.starts_with("d "))
        .collect();
    assert_eq!(lines.len(), 4320);
    for chunk in lines.as_chunks::<5>().0 {
        let words: Vec<_> = chunk[0]
            .split(" | ")
            .next()
            .unwrap()
            .split_whitespace()
            .collect();
        let kind: u32 = words[1].parse().unwrap();
        let stock: u32 = words[2].parse().unwrap();
        let bank: u32 = words[3].parse().unwrap();
        let money: i32 = words[4].parse().unwrap();
        let talk: bool = words[7] == "1";
        let c = catalog(vec![row(1, kind, money)]);
        let mut s = State::new(&c);
        let mut h = Ledger::new();
        h.carried.insert(200, stock);
        h.bank.insert(200, bank);
        h.money = words[5].parse().unwrap();
        h.level = words[6].parse().unwrap();
        for (step, line) in chunk.iter().enumerate() {
            let parts: Vec<_> = line.split('|').map(str::trim).collect();
            let events = match step {
                0 => run(
                    &c,
                    &mut s,
                    &mut h,
                    Command::Accept {
                        quest: 1,
                        giver: 900,
                    },
                ),
                1 | 2 => run(
                    &c,
                    &mut s,
                    &mut h,
                    if step == 2 && talk {
                        Command::TalkCredit {
                            entry: 100,
                            guid: 501,
                        }
                    } else {
                        Command::KillCredit {
                            entry: 100,
                            guid: 499 + step as u64,
                        }
                    },
                ),
                3 => {
                    *h.carried.get_mut(&200).unwrap() += 2;
                    run(
                        &c,
                        &mut s,
                        &mut h,
                        Command::ItemAdded {
                            item: 200,
                            count: 2,
                        },
                    )
                }
                _ => {
                    if parts[0].starts_with('r') {
                        reward(&c, &mut s, &mut h, 1)
                    } else {
                        assert!(can_reward(&c, &s, &h, 1).is_err());
                        Vec::new()
                    }
                }
            };
            let p = s.progress(1).unwrap();
            let actual = format!(
                "{} {} {} {} {} {} {}",
                if p.status == Status::Complete { 1 } else { 3 },
                u8::from(p.rewarded),
                p.items[0],
                p.creatures[0],
                s.snapshot().log[0].unwrap_or(0),
                h.money,
                h.xp
            );
            assert_eq!(actual, parts[1], "{line}");
            assert_eq!(
                events.iter().map(event).collect::<Vec<_>>().join(","),
                parts[2],
                "{line}"
            );
            // Actual API checkpoint restoration between every source operation.
            s = State::restore(&c, s.snapshot()).unwrap();
        }
    }
}
#[test]
fn reward_is_atomic_and_exactly_once_across_restore() {
    let c = catalog(vec![row(1, 2, 7)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    h.carried.insert(200, 2);
    accept(&c, &mut s, &mut h, 1);
    kill(&c, &mut s, &mut h);
    let checkpoint = s.snapshot();
    assert_eq!(checkpoint.records[&1].creatures[0], 1);
    s = State::restore(&c, checkpoint).unwrap();
    kill(&c, &mut s, &mut h);
    let before = s.clone();
    h.fail = true;
    let host_before = h.clone();
    assert!(reduce(
        &c,
        &s,
        &h,
        s.next_sequence(),
        Command::Reward {
            choice: 0,
            quest: 1,
            giver: 900,
            announce: true
        }
    )
    .is_err());
    assert_eq!(s, before);
    assert_eq!(h, host_before);
    h.fail = false;
    let sequence = s.next_sequence();
    reward(&c, &mut s, &mut h, 1);
    let final_host = h.clone();
    s = State::restore(&c, s.snapshot()).unwrap();
    assert_eq!(
        reduce(
            &c,
            &s,
            &h,
            sequence,
            Command::Reward {
                choice: 0,
                quest: 1,
                giver: 900,
                announce: true
            }
        )
        .unwrap_err(),
        Error::Invalid("command_sequence")
    );
    assert_eq!(
        reduce(
            &c,
            &s,
            &h,
            s.next_sequence(),
            Command::Reward {
                choice: 0,
                quest: 1,
                giver: 900,
                announce: true
            }
        )
        .unwrap_err(),
        Error::Rejected(Gate::AlreadyRewarded)
    );
    assert_eq!(h, final_host);
    assert_eq!(
        can_take(&c, &s, &h, 1),
        Err(Error::Rejected(Gate::AlreadyTaken))
    );
}
#[test]
fn bank_completion_cannot_pay_carried_turnin() {
    let c = catalog(vec![row(1, 1, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    h.bank.insert(200, 2);
    accept(&c, &mut s, &mut h, 1);
    assert_eq!(s.progress(1).unwrap().status, Status::Complete);
    assert_eq!(can_reward(&c, &s, &h, 1), Err(Error::Rejected(Gate::Items)));
    let before = s.clone();
    assert!(reduce(
        &c,
        &s,
        &h,
        s.next_sequence(),
        Command::Reward {
            choice: 0,
            quest: 1,
            giver: 900,
            announce: true
        }
    )
    .is_err());
    assert_eq!(s, before);
}
#[test]
fn signed_previous_reverse_next_and_chain_relations_are_not_optional() {
    let mut q1 = row(1, 0, 0);
    q1.next_quest = 2;
    let mut q2 = row(2, 0, 0);
    q2.next_quest = -3;
    let mut q3 = row(3, 0, 0);
    q3.next_in_chain = 4;
    let q4 = row(4, 0, 0);
    let c = catalog(vec![q1, q2, q3, q4]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    assert_eq!(
        can_take(&c, &s, &h, 2),
        Err(Error::Rejected(Gate::Previous))
    );
    accept(&c, &mut s, &mut h, 1);
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    assert_eq!(
        can_take(&c, &s, &h, 2),
        Err(Error::Rejected(Gate::Previous))
    );
    reward(&c, &mut s, &mut h, 1);
    accept(&c, &mut s, &mut h, 2);
    assert!(can_take(&c, &s, &h, 3).is_ok());
    accept(&c, &mut s, &mut h, 3);
    assert_eq!(
        can_take(&c, &s, &h, 4),
        Err(Error::Rejected(Gate::PreviousChain))
    );
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    reward(&c, &mut s, &mut h, 3);
    assert!(can_take(&c, &s, &h, 4).is_ok());
    let mut alt = s.snapshot();
    alt.records.remove(&3);
    let s = State::restore(&c, alt).unwrap();
    let mut s = s;
    accept(&c, &mut s, &mut h, 4);
    assert_eq!(
        can_take(&c, &s, &h, 3),
        Err(Error::Rejected(Gate::NextChain))
    );
}
#[test]
fn money_callbacks_and_stack_removals_keep_source_order() {
    let c = catalog(vec![row(1, 1, -3), row(2, 0, -2)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    h.money = 3;
    h.carried.insert(200, 2);
    h.split = true;
    accept(&c, &mut s, &mut h, 1);
    accept(&c, &mut s, &mut h, 2);
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    let events = reward(&c, &mut s, &mut h, 1);
    assert_eq!(h.money, 0);
    assert_eq!(s.progress(2).unwrap().status, Status::Incomplete);
    let trace = events.iter().map(event).collect::<Vec<_>>().join(",");
    assert!(trace.starts_with("world,bit:0:0,world,destroy:200:1,world,bit:0:0,world,destroy:200:1,timed:1,rep:1,log:0:0,xp:100,money:-3,world,bit:1:0,"),"{trace}");
}
#[test]
fn reject_missing_dependencies_unknown_effects_and_corrupt_snapshots() {
    let mut q = row(1, 0, 0);
    q.previous_quest = 2;
    assert!(Catalog::new(vec![q.clone()], relations(&[q]), audit()).is_err());
    let q = row(1, 0, 0);
    let mut a = audit();
    a.questgiver_hooks = Evidence::Unknown;
    assert!(Catalog::new(vec![q.clone()], relations(std::slice::from_ref(&q)), a).is_err());
    for field in 0..5 {
        let mut q = q.clone();
        match field {
            0 => q.excluded.reward_item[0] = 2,
            1 => q.excluded.reward_spell = 5,
            2 => q.excluded.complete_script = 3,
            3 => q.special_flags |= 1,
            _ => q.excluded.exclusive_group = -1,
        };
        assert!(Catalog::new(vec![q.clone()], relations(&[q]), audit()).is_err());
    }
    let c = catalog(vec![q]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    let mut snap = s.snapshot();
    snap.records.get_mut(&1).unwrap().rewarded = true;
    assert!(State::restore(&c, snap).is_err());
    let mut snap = s.snapshot();
    snap.log[1] = Some(1);
    assert!(State::restore(&c, snap).is_err());
    let mut snap = s.snapshot();
    snap.records.get_mut(&1).unwrap().creatures[0] = 3;
    assert!(State::restore(&c, snap).is_err());
    let other = catalog(vec![row(1, 0, 7)]);
    assert!(State::restore(&other, s.snapshot()).is_err());
}
#[test]
fn log_limit_first_free_slot_and_gate_order() {
    let c = catalog((1..=21).map(|id| row(id, 0, 0)).collect());
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    for id in 1..=20 {
        accept(&c, &mut s, &mut h, id);
    }
    assert_eq!(can_add(&c, &s, 21), Err(Error::Rejected(Gate::LogFull)));
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    reward(&c, &mut s, &mut h, 3);
    accept(&c, &mut s, &mut h, 21);
    assert_eq!(s.snapshot().log[2], Some(21));
    let mut q = row(22, 0, 0);
    q.active = false;
    q.eligibility.required_classes = 1;
    q.eligibility.required_races = 2;
    q.eligibility.minimum_level = 2;
    let c = catalog(vec![q]);
    assert_eq!(
        can_take(&c, &State::new(&c), &h, 22),
        Err(Error::Rejected(Gate::Class))
    );
}

#[test]
fn pinned_loader_relation_and_acceptance_probes_match() {
    let mut cases = 0;
    for line in include_str!("fixtures/quest_lifecycle_cmangos.tsv")
        .lines()
        .filter(|line| line.starts_with("g "))
    {
        let parts: Vec<_> = line.split('|').map(str::trim).collect();
        let v: Vec<u32> = parts[0]
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse().unwrap())
            .collect();
        let (relation, history, deny) = (v[0], v[1], v[2]);
        let mut one = row(1, 0, 0);
        let mut two = row(2, 0, 0);
        match relation {
            0 => two.previous_quest = 1,
            1 => two.previous_quest = -1,
            2 => one.next_quest = 2,
            3 => one.next_quest = -2,
            _ => one.next_in_chain = 2,
        }
        match deny {
            1 => two.eligibility.required_classes = 1,
            2 => two.eligibility.required_races = 2,
            3 => two.eligibility.minimum_level = 2,
            4 => two.active = false,
            _ => {}
        }
        let c = catalog(vec![one, two]);
        let mut snap = State::new(&c).snapshot();
        if history != 0 {
            let status = if history == 1 {
                Status::Incomplete
            } else {
                Status::Complete
            };
            snap.records.insert(
                1,
                Progress {
                    status,
                    rewarded: history == 3,
                    items: [0; 4],
                    creatures: [if history == 1 { 0 } else { 2 }, 0, 0, 0],
                },
            );
            if history != 3 {
                snap.log[0] = Some(1);
            }
        }
        if deny == 5 {
            snap.records.insert(
                2,
                Progress {
                    status: Status::Incomplete,
                    rewarded: false,
                    items: [0; 4],
                    creatures: [0; 4],
                },
            );
            snap.log[1] = Some(2);
        }
        let s = State::restore(&c, snap).unwrap();
        assert_eq!(
            can_take(&c, &s, &Ledger::new(), 2).is_ok(),
            parts[1] == "1",
            "{line}"
        );
        cases += 1;
    }
    assert_eq!(cases, 120);
}

#[test]
fn callbacks_retain_transient_source_state_before_final_reward_flag() {
    let c = catalog(vec![row(1, 1, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    h.carried.insert(200, 2);
    accept(&c, &mut s, &mut h, 1);
    let events = reward(&c, &mut s, &mut h, 1);
    let views: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::WorldObjects { view } = e {
                Some(view)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(views[0].records[&1].status, Status::Incomplete);
    assert_eq!(views[0].log[0], Some(1));
    assert!(!views[views.len() - 2].records[&1].rewarded);
    assert_eq!(views[views.len() - 2].records[&1].status, Status::Complete);
    assert_eq!(views[views.len() - 2].log[0], None);
    assert!(views.last().unwrap().records[&1].rewarded);
}

fn event_with_view(e: &Event) -> String {
    let Event::WorldObjects { view } = e else {
        return event(e);
    };
    let mut result = String::from("world@");
    for id in view.log {
        result.push_str(&format!("{}/", id.unwrap_or(0)));
    }
    result.push('@');
    for (id, p) in &view.records {
        result.push_str(&format!(
            "{id}:{}:{}",
            if p.status == Status::Complete { 1 } else { 3 },
            u8::from(p.rewarded)
        ));
        for count in p.items.into_iter().chain(p.creatures) {
            result.push_str(&format!(":{count}"));
        }
        result.push(';');
    }
    result
}

#[test]
fn source_surplus_removal_and_shared_reward_preserve_over_requirement_counters() {
    let lines: Vec<_> = include_str!("fixtures/quest_lifecycle_cmangos.tsv")
        .lines()
        .filter(|line| line.starts_with("d "))
        .collect();
    assert_eq!(lines.len(), 32);
    for chunk in lines.as_chunks::<2>().0 {
        let words: Vec<u32> = chunk[0]
            .split(" | ")
            .next()
            .unwrap()
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse().unwrap())
            .collect();
        let (is_reward, split, stock) = (words[0] != 0, words[1] != 0, words[2]);
        let mut one = row(1, 1, 0);
        one.required_item_count[0] = 3;
        let c = catalog(vec![one, row(2, 1, 0)]);
        let mut s = State::new(&c);
        let mut h = Ledger::new();
        h.carried.insert(200, stock);
        h.split = split;
        accept(&c, &mut s, &mut h, 1);
        accept(&c, &mut s, &mut h, 2);
        let command = if is_reward {
            Command::Reward {
                choice: 0,
                quest: 1,
                giver: 900,
                announce: true,
            }
        } else {
            Command::RemoveItems {
                item: 200,
                count: 3,
            }
        };
        let before = s.clone();
        let effects = if split { 3 } else { 1 } + if is_reward { 2 } else { 0 };
        for after in 1..=effects {
            h.fail_after = Some(after);
            let host_before = h.clone();
            assert!(reduce(&c, &s, &h, s.next_sequence(), command.clone()).is_err());
            assert_eq!(s, before);
            assert_eq!(h, host_before);
        }
        h.fail_after = None;
        let events = run(&c, &mut s, &mut h, command);
        for (index, line) in chunk.iter().enumerate() {
            let parts: Vec<_> = line.split('|').map(str::trim).collect();
            let p = s.progress(index as u32 + 1).unwrap();
            let actual = format!(
                "{} {} {} {} {} {} {}",
                if p.status == Status::Complete { 1 } else { 3 },
                u8::from(p.rewarded),
                p.items[0],
                p.creatures[0],
                s.snapshot().log[0].unwrap_or(0),
                h.money,
                h.xp
            );
            assert_eq!(actual, parts[1], "{line}");
            assert_eq!(h.carried[&200], words[4], "{line}");
            assert_eq!(h.carried[&200], stock - 3);
            let trace = if index == 0 {
                events
                    .iter()
                    .map(event_with_view)
                    .collect::<Vec<_>>()
                    .join(",")
            } else {
                String::new()
            };
            assert_eq!(trace, parts[2], "{line}");
        }
        assert_eq!(s.snapshot().log[1], Some(2));
        s = State::restore(&c, s.snapshot()).unwrap();
        if !split && stock == 4 {
            assert_eq!(s.progress(2).unwrap().items[0], 3);
            assert_eq!(s.progress(2).unwrap().status, Status::Incomplete);
            let first_view = events
                .iter()
                .find_map(|event| match event {
                    Event::WorldObjects { view } if view.records[&2].items[0] == 3 => Some(view),
                    _ => None,
                })
                .unwrap();
            assert_eq!(first_view.log[0], Some(1));
            assert_eq!(first_view.records[&2].status, Status::Incomplete);
        }
    }
}

#[test]
fn restore_rejects_absent_item_credit_and_unearned_creature_history() {
    let c = catalog(vec![row(1, 0, 0)]);
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    accept(&c, &mut s, &mut h, 1);
    let mut snapshot = s.snapshot();
    snapshot.records.get_mut(&1).unwrap().items[1] = 1;
    assert_eq!(
        State::restore(&c, snapshot),
        Err(Error::Invalid("objective_progress"))
    );
    let mut snapshot = s.snapshot();
    snapshot.log[0] = None;
    let progress = snapshot.records.get_mut(&1).unwrap();
    progress.status = Status::Complete;
    progress.rewarded = true;
    assert_eq!(
        State::restore(&c, snapshot),
        Err(Error::Invalid("completed_objectives"))
    );
}

#[test]
fn callback_view_eligibility_preserves_source_gates_without_mutation() {
    let c = catalog(vec![row(1, 0, 0)]);
    let state = State::new(&c);
    let host = Ledger::new();
    let original = state.snapshot();
    assert_eq!(
        can_take_view(&c, &state.view(), host.identity(), 1),
        can_take(&c, &state, &host, 1)
    );
    let tx = reduce(
        &c,
        &state,
        &host,
        0,
        Command::Accept {
            quest: 1,
            giver: 900,
        },
    )
    .unwrap();
    assert_eq!(
        can_take_view(&c, &tx.state.view(), host.identity(), 1),
        Err(Error::Rejected(Gate::AlreadyTaken))
    );
    assert_eq!(state.snapshot(), original);
    for level in [0, 1, 60, 61] {
        let mut h = host.clone();
        h.level = level;
        assert_eq!(
            can_take_view(&c, &state.view(), h.identity(), 1),
            can_take(&c, &state, &h, 1)
        );
    }
    for event in tx.events {
        let encoded = serde_json::to_vec(&event).unwrap();
        assert_eq!(serde_json::from_slice::<Event>(&encoded).unwrap(), event);
    }
}

#[test]
fn source_visible_start_omits_take_status_and_level_gates() {
    let mut r = row(1, 0, 0);
    r.eligibility.minimum_level = 5;
    let c = catalog(vec![r]);
    let state = State::new(&c);
    let mut host = Ledger::new();
    assert_eq!(
        can_take_view(&c, &state.view(), host.identity(), 1),
        Err(Error::Rejected(Gate::Level))
    );
    assert!(can_see_start_view(&c, &state.view(), host.identity(), 1).is_ok());
    host.level = 5;
    let tx = reduce(
        &c,
        &state,
        &host,
        0,
        Command::Accept {
            quest: 1,
            giver: 900,
        },
    )
    .unwrap();
    assert!(can_see_start_view(&c, &tx.state.view(), host.identity(), 1).is_ok());
    assert_eq!(
        can_take_view(&c, &tx.state.view(), host.identity(), 1),
        Err(Error::Rejected(Gate::AlreadyTaken))
    );
}

#[path = "quest_lifecycle/reputation.rs"]
mod reputation;
