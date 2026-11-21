// SPDX-License-Identifier: GPL-3.0-or-later
// Synthetic source-oracle probes. This host does not claim reward arithmetic parity.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ReputationLedger {
    base: Ledger,
    standing: BTreeMap<u32, i32>,
    visible: Vec<u32>,
    reward_views: Vec<QuestView>,
    fail_visible: bool,
    fail_reward_step: Option<usize>,
}
impl ReputationLedger {
    fn new(standing: i32) -> Self {
        Self {
            base: Ledger::new(),
            standing: BTreeMap::from([(71, standing), (72, 0)]),
            visible: vec![],
            reward_views: vec![],
            fail_visible: false,
            fail_reward_step: None,
        }
    }
}
impl Host for ReputationLedger {
    fn participation(&self) -> Participation {
        self.base.participation()
    }
    fn identity(&self) -> QuestPlayerIdentity {
        self.base.identity()
    }
    fn reward_context(&self) -> RewardContext {
        self.base.reward_context()
    }
    fn money(&self) -> u32 {
        self.base.money()
    }
    fn reputation(&self, faction: u32) -> Result<i32, Error> {
        self.standing
            .get(&faction)
            .copied()
            .ok_or(Error::Host("faction"))
    }
    fn reputation_visible(&mut self, faction: u32) -> Result<(), Error> {
        self.visible.push(faction);
        if self.fail_visible {
            return Err(Error::Host("deliberate_visibility_failure"));
        }
        Ok(())
    }
    fn reward_reputation(
        &mut self,
        catalog: &Catalog,
        state: &mut State,
        quest: u32,
        events: &mut Vec<Event>,
    ) -> Result<(), Error> {
        self.reward_views.push(state.view());
        let row = catalog.row(quest)?;
        // Authored host inputs isolate synchronous callback orchestration. The
        // native reputation reducer separately establishes rate/spillover math.
        for (index, (faction, value)) in row
            .excluded
            .reward_rep_faction
            .into_iter()
            .zip(row.excluded.reward_rep_value)
            .enumerate()
        {
            if faction == 0 {
                continue;
            }
            *self
                .standing
                .get_mut(&faction)
                .ok_or(Error::Host("faction"))? += value;
            reputation_changed(catalog, state, self, faction, events)?;
            if self.fail_reward_step == Some(index) {
                return Err(Error::Host("deliberate_reputation_failure"));
            }
        }
        Ok(())
    }
    fn item_count(&self, item: u32, include_bank: bool) -> Result<u32, Error> {
        self.base.item_count(item, include_bank)
    }
    fn removal_steps(&self, item: u32, count: u32) -> Result<Vec<u32>, Error> {
        self.base.removal_steps(item, count)
    }
    fn can_store_reward(
        &self,
        item: u32,
        count: u32,
        destinations: &mut Vec<benilla_sim::inventory::Destination>,
    ) -> Result<benilla_sim::inventory::InventoryResult, Error> {
        self.base.can_store_reward(item, count, destinations)
    }
    fn apply(&mut self, effect: &HostEffect) -> Result<(), Error> {
        self.base.apply(effect)
    }
}
fn reputation_row(id: u32) -> QuestRow {
    let mut q = row(id, 0, 0);
    q.special_flags = 0;
    q.required_creature = [0; 4];
    q.required_creature_count = [0; 4];
    q
}
fn reputation_run(
    c: &Catalog,
    s: &mut State,
    h: &mut ReputationLedger,
    command: Command,
) -> Vec<Event> {
    let tx = reduce(c, s, h, s.next_sequence(), command).unwrap();
    *s = tx.state;
    *h = tx.host;
    tx.events
}
fn reputation_accept(c: &Catalog, s: &mut State, h: &mut ReputationLedger, id: u32) -> Vec<Event> {
    reputation_run(
        c,
        s,
        h,
        Command::Accept {
            quest: id,
            giver: 900,
        },
    )
}
fn records(view: &QuestView) -> String {
    let mut result = String::new();
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
fn assert_source_output(parts: &[&str], state: &State, host: &ReputationLedger, events: &[Event]) {
    let context = parts[0];
    assert_eq!(
        format!(
            "{} {} {} {}",
            host.base.money, host.base.xp, host.standing[&71], host.standing[&72]
        ),
        parts[1],
        "{context}"
    );
    assert_eq!(
        state
            .snapshot()
            .log
            .iter()
            .map(|id| format!("{}/", id.unwrap_or(0)))
            .collect::<String>(),
        parts[2],
        "{context}"
    );
    assert_eq!(records(&state.view()), parts[3], "{context}");
    assert_eq!(
        events
            .iter()
            .map(event_with_view)
            .collect::<Vec<_>>()
            .join(","),
        parts[4],
        "{context}"
    );
}

#[test]
fn pinned_reputation_gates_and_callback_traces_match() {
    let mut counts = [0; 4];
    for line in include_str!("../fixtures/quest_reputation_cmangos.tsv").lines() {
        let parts: Vec<_> = line.split('|').map(str::trim).collect();
        let mut words = parts[0].split_whitespace();
        let kind = words.next().unwrap();
        let n: Vec<i32> = words.map(|word| word.parse().unwrap()).collect();
        match kind {
            "g" => {
                counts[0] += 1;
                let mut q = reputation_row(1);
                q.excluded.required_min_rep_faction = n[0] as u32;
                q.excluded.required_max_rep_faction = n[1] as u32;
                q.excluded.required_min_rep_value = n[2];
                q.excluded.required_max_rep_value = n[3];
                let c = catalog(vec![q]);
                let s = State::new(&c);
                let mut h = ReputationLedger::new(n[4]);
                h.standing.insert(72, n[5]);
                let take = can_take(&c, &s, &h, 1);
                assert_eq!(
                    take,
                    can_take_view_with_reputation(&c, &s.view(), h.identity(), 1, |faction| h
                        .reputation(faction))
                );
                let see =
                    can_see_start_view_with_reputation(&c, &s.view(), h.identity(), 1, |faction| {
                        h.reputation(faction)
                    });
                assert_eq!(
                    format!("{} {}", u8::from(take.is_ok()), u8::from(see.is_ok())),
                    parts[1],
                    "{line}"
                );
            }
            "a" => {
                counts[1] += 1;
                let mut q = reputation_row(1);
                q.excluded.rep_objective_faction = n[0] as u32;
                q.excluded.rep_objective_value = n[1];
                let mut h = ReputationLedger::new(n[2]);
                if n[3] == 1 {
                    q.special_flags = DELIVER;
                    q.required_item[0] = 200;
                    q.required_item_count[0] = 2;
                    h.base.carried.insert(200, if n[4] == 1 { 2 } else { 0 });
                }
                if n[3] == 2 {
                    q.reward.reward_or_required_money = -3;
                    h.base.money = if n[4] == 1 { 3 } else { 0 };
                }
                let c = catalog(vec![q]);
                let mut s = State::new(&c);
                let events = reputation_accept(&c, &mut s, &mut h, 1);
                assert_source_output(&parts, &s, &h, &events);
                State::restore(&c, s.snapshot()).unwrap();
            }
            "c" | "r" => {
                let is_reward = kind == "r";
                counts[if is_reward { 3 } else { 2 }] += 1;
                let before = n[usize::from(is_reward)];
                let mut h = ReputationLedger::new(before);
                let rows = [3, 1, 4, 2].map(|id| {
                    let mut q = reputation_row(id);
                    q.excluded.rep_objective_faction = if id == 4 { 72 } else { 71 };
                    q.excluded.rep_objective_value = if is_reward {
                        if id == 3 {
                            10
                        } else {
                            15
                        }
                    } else if id == 1 {
                        10
                    } else {
                        5
                    };
                    if is_reward && id == 3 {
                        q.excluded.reward_rep_faction[0] = 71;
                        q.excluded.reward_rep_value[0] = n[2];
                        if n[3] == 1 {
                            q.excluded.reward_rep_faction[1] = 71;
                            q.excluded.reward_rep_value[1] = -n[2];
                        }
                    }
                    if (is_reward && id == 3 && n[0] == 1) || (!is_reward && id == 2 && n[2] == 1) {
                        q.special_flags = DELIVER;
                        q.required_item[0] = 200;
                        q.required_item_count[0] = 2;
                    }
                    if !is_reward && id == 2 && n[2] == 2 {
                        q.reward.reward_or_required_money = -3;
                    }
                    q
                });
                let c = catalog(rows.to_vec());
                let mut s = State::new(&c);
                if is_reward {
                    h.base.carried.insert(200, 2);
                }
                for id in [3, 1, 4, 2] {
                    reputation_accept(&c, &mut s, &mut h, id);
                }
                let mut events = vec![];
                if is_reward {
                    events = reputation_run(
                        &c,
                        &mut s,
                        &mut h,
                        Command::Reward {
                            quest: 3,
                            giver: 900,
                            announce: true,
                            choice: 0,
                        },
                    );
                    assert_eq!(h.reward_views.len(), 1);
                    assert_eq!(h.reward_views[0].log[0], Some(3));
                    assert!(!h.reward_views[0].records[&3].rewarded);
                } else {
                    h.standing.insert(71, n[1]);
                    reputation_changed(&c, &mut s, &h, 71, &mut events).unwrap();
                }
                assert_source_output(&parts, &s, &h, &events);
                State::restore(&c, s.snapshot()).unwrap();
            }
            _ => panic!("unknown fixture line {line}"),
        }
    }
    assert_eq!(counts, [900, 180, 108, 24]);
}

#[test]
fn reputation_gates_short_circuit_in_source_order_and_default_queries_fail_closed() {
    let mut q = reputation_row(2);
    q.previous_quest = 1;
    q.excluded.required_min_rep_faction = 71;
    q.excluded.required_min_rep_value = 5;
    q.excluded.required_max_rep_faction = 72;
    q.excluded.required_max_rep_value = 10;
    let c = catalog(vec![reputation_row(1), q]);
    let s = State::new(&c);
    let h = Ledger::new();
    assert_eq!(
        can_take(&c, &s, &h, 2),
        Err(Error::Unsupported("reputation_host"))
    );
    assert_eq!(
        can_take_view(&c, &s.view(), h.identity(), 2),
        Err(Error::Unsupported("reputation_host"))
    );
    assert_eq!(
        can_see_start_view(&c, &s.view(), h.identity(), 2),
        Err(Error::Unsupported("reputation_host"))
    );
    for (min, max, expected, queries) in [
        (4, 0, Gate::Reputation, vec![71]),
        (5, 10, Gate::Reputation, vec![71, 72]),
        (5, 9, Gate::Previous, vec![71, 72]),
    ] {
        let mut actual_queries = vec![];
        assert_eq!(
            can_take_view_with_reputation(&c, &s.view(), h.identity(), 2, |faction| {
                actual_queries.push(faction);
                Ok(if faction == 71 { min } else { max })
            }),
            Err(Error::Rejected(expected))
        );
        assert_eq!(actual_queries, queries);
    }
    let mut identity = h.identity();
    identity.level = 61;
    assert_eq!(
        can_take_view_with_reputation(&c, &s.view(), identity, 2, |_| panic!(
            "level must reject before reputation"
        )),
        Err(Error::Rejected(Gate::Level))
    );
    assert_eq!(
        can_see_start_view_with_reputation(&c, &s.view(), identity, 2, |_| Ok(0)),
        Err(Error::Rejected(Gate::Reputation))
    );
}

#[test]
fn unsupported_reputation_hosts_and_mutating_hook_failures_roll_back() {
    let mut q = reputation_row(1);
    q.excluded.rep_objective_faction = 71;
    q.excluded.rep_objective_value = 10;
    q.excluded.reward_rep_faction[0] = 71;
    q.excluded.reward_rep_value[0] = -10;
    q.excluded.reward_rep_faction[1] = 71;
    q.excluded.reward_rep_value[1] = 10;
    let c = catalog(vec![q]);
    let mut s = State::new(&c);
    let mut h = ReputationLedger::new(10);
    let accept = Command::Accept {
        quest: 1,
        giver: 900,
    };
    assert_eq!(
        reduce(&c, &s, &Ledger::new(), 0, accept.clone()).unwrap_err(),
        Error::Unsupported("reputation_visible_host")
    );
    h.fail_visible = true;
    let before = h.clone();
    assert_eq!(
        reduce(&c, &s, &h, 0, accept.clone()).unwrap_err(),
        Error::Host("deliberate_visibility_failure")
    );
    assert_eq!(h, before);
    assert!(s.snapshot().records.is_empty());
    h.fail_visible = false;
    let events = reputation_accept(&c, &mut s, &mut h, 1);
    assert!(matches!(
        events.as_slice(),
        [
            Event::ReputationVisible(71),
            Event::LogSet {
                slot: 0,
                quest: Some(1)
            },
            ..
        ]
    ));
    let reward = Command::Reward {
        quest: 1,
        giver: 900,
        announce: true,
        choice: 0,
    };
    assert_eq!(
        reduce(&c, &s, &Ledger::new(), s.next_sequence(), reward.clone()).unwrap_err(),
        Error::Unsupported("reward_reputation_host")
    );
    let original = s.clone();
    for step in [0, 1] {
        h.fail_reward_step = Some(step);
        let before = h.clone();
        assert_eq!(
            reduce(&c, &s, &h, s.next_sequence(), reward.clone()).unwrap_err(),
            Error::Host("deliberate_reputation_failure")
        );
        assert_eq!(s, original);
        assert_eq!(h, before);
    }
    h.fail_reward_step = None;
    // Later host failure must also discard all completed reputation callbacks.
    h.base.fail_after = Some(1);
    let before = h.clone();
    assert!(reduce(&c, &s, &h, s.next_sequence(), reward).is_err());
    assert_eq!(s, original);
    assert_eq!(h, before);
}

#[test]
fn default_reward_hook_rejects_each_nonzero_reputation_column() {
    for field in 0..3 {
        let mut q = reputation_row(1);
        match field {
            0 => q.excluded.reward_rep_faction[4] = 71,
            1 => q.excluded.reward_rep_value[4] = 1,
            _ => q.excluded.reward_faction_flags = 1,
        }
        let c = catalog(vec![q]);
        let mut s = State::new(&c);
        let mut h = Ledger::new();
        accept(&c, &mut s, &mut h, 1);
        assert_eq!(
            reduce(
                &c,
                &s,
                &h,
                s.next_sequence(),
                Command::Reward {
                    quest: 1,
                    giver: 900,
                    announce: false,
                    choice: 0
                }
            )
            .unwrap_err(),
            Error::Unsupported("reward_reputation_host")
        );
    }
}
