// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic Player.cpp/Player.h/QuestDef.h/ObjectMgr.cpp
// at 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f. Copyright CMaNGOS contributors.
// Upstream GPL-2.0-or-later; Rust modifications GPL-3.0-or-later, WITHOUT WARRANTY.
//! Bounded, solo ordinary quest lifecycle. See ../../QUEST-LIFECYCLE.md.
//! All inventory, XP and currency changes occur on an isolated transaction host.
//! The caller must durably commit host and State together before publishing events.
mod model;
use crate::quest::{self, QuestPlayerIdentity, RewardContext};
pub use model::*;

pub const POLICY_ID: &str = "cmangos-8ec338a1-ordinary-quest-lifecycle-v1";
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Unsupported(&'static str),
    Invalid(&'static str),
    Rejected(Gate),
    Host(&'static str),
    Arithmetic(quest::QuestError),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Classic quest lifecycle: {self:?}")
    }
}
impl std::error::Error for Error {}
impl From<quest::QuestError> for Error {
    fn from(value: quest::QuestError) -> Self {
        Self::Arithmetic(value)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    AlreadyTaken,
    Class,
    Race,
    Level,
    Previous,
    NextChain,
    PreviousChain,
    Inactive,
    LogFull,
    NotComplete,
    AlreadyRewarded,
    Items,
    Money,
    Giver,
}

/// Explicit execution context; group and unknown contexts are outside this reducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Participation {
    Solo,
    Group,
    Unknown,
}

/// An in-memory deep clone, without externally visible side effects. Implementations
/// must use audited native inventory/GiveXP/ModifyMoney mechanisms, not fallback rules.
/// No blanket/default implementation is provided. Failure must not mutate the original.
pub trait Host: Clone {
    fn participation(&self) -> Participation;
    fn identity(&self) -> QuestPlayerIdentity;
    fn reward_context(&self) -> RewardContext;
    fn money(&self) -> u32;
    fn item_count(&self, item: u32, include_bank: bool) -> Result<u32, Error>;
    /// Exact source DestroyItemCount stack traversal, each step in deletion order.
    fn removal_steps(&self, item: u32, count: u32) -> Result<Vec<u32>, Error>;
    /// Apply only the staged host mutation, without recursively invoking quest
    /// callbacks; this reducer performs those at their exact source positions.
    fn apply(&mut self, effect: &HostEffect) -> Result<(), Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostEffect {
    DestroyCarriedStack { item: u32, count: u32 },
    GiveXp(u32),
    ModifyMoney(i32),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    LogSet {
        slot: usize,
        quest: Option<u32>,
    },
    AcceptHook {
        quest: u32,
        giver: u32,
    },
    WorldObjects {
        view: QuestView,
    },
    LogComplete {
        slot: usize,
        complete: bool,
    },
    KillCredit {
        quest: u32,
        guid: u64,
        objective: usize,
        count: u32,
    },
    ItemCredit {
        quest: u32,
        objective: usize,
        previous: u32,
        added: u32,
    },
    RemoveTimedQuest(u32),
    RewardReputation(u32),
    Host(HostEffect),
    RewardAnnouncement {
        quest: u32,
        xp: u32,
    },
    RewardHook {
        quest: u32,
        giver: u32,
    },
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// Handler-level acceptance: AddQuest followed by CanCompleteQuest/CompleteQuest.
    Accept {
        quest: u32,
        giver: u32,
    },
    /// Trusted solo kill credit, after death/reward eligibility. Not a client claim.
    KillCredit {
        entry: u32,
        guid: u64,
    },
    /// Trusted TalkedToCreature after native interaction eligibility.
    TalkCredit {
        entry: u32,
        guid: u64,
    },
    /// Source ItemAddedQuestCheck after the caller's staged inventory addition.
    /// The supplied host and returned state must be committed in the SAME transaction.
    ItemAdded {
        item: u32,
        count: u32,
    },
    /// Audited carried-stack destruction, including pre-removal quest callbacks.
    RemoveItems {
        item: u32,
        count: u32,
    },
    Complete {
        quest: u32,
    },
    /// MoneyChanged after an external staged currency operation (same transaction).
    MoneyChanged,
    Reward {
        quest: u32,
        giver: u32,
        announce: bool,
    },
}
#[derive(Clone, Debug)]
pub struct Transaction<H> {
    pub state: State,
    pub host: H,
    pub events: Vec<Event>,
}

fn check_catalog(catalog: &Catalog, state: &State) -> Result<(), Error> {
    if state.snapshot.catalog_hash != catalog.hash() {
        return Err(Error::Invalid("catalog_changed"));
    }
    Ok(())
}
fn current(state: &State, id: u32) -> bool {
    state.progress(id).is_some_and(|p| !p.rewarded)
}
fn logged(state: &State, id: u32) -> Option<usize> {
    state.snapshot.log.iter().position(|q| *q == Some(id))
}
fn money_required(q: &QuestRow, host: &impl Host) -> Result<u32, Error> {
    let money = quest::reward_or_required_money(
        q.reward.reward_or_required_money,
        host.reward_context().money_rate,
    )?;
    Ok(if money < 0 { money.unsigned_abs() } else { 0 })
}
pub fn can_take(catalog: &Catalog, state: &State, host: &impl Host, id: u32) -> Result<(), Error> {
    check_catalog(catalog, state)?;
    let q = catalog.row(id)?;
    // The omitted gates are proven inert at Catalog admission, not assumed true.
    if state.progress(id).is_some() {
        return Err(Error::Rejected(Gate::AlreadyTaken));
    }
    let gates = quest::eligibility_gates(q.eligibility, host.identity())?;
    for (ok, gate) in [
        (gates.class, Gate::Class),
        (gates.race, Gate::Race),
        (gates.level, Gate::Level),
    ] {
        if !ok {
            return Err(Error::Rejected(gate));
        }
    }
    if let Some(previous) = catalog.previous.get(&id) {
        if !previous.iter().any(|&prev| {
            if prev > 0 {
                state.progress(prev as u32).is_some_and(|p| p.rewarded)
            } else {
                current(state, prev.unsigned_abs())
            }
        }) {
            return Err(Error::Rejected(Gate::Previous));
        }
    }
    if q.next_in_chain != 0 && state.progress(q.next_in_chain).is_some() {
        return Err(Error::Rejected(Gate::NextChain));
    }
    if catalog
        .previous_chain
        .get(&id)
        .is_some_and(|p| p.iter().any(|&id| current(state, id)))
    {
        return Err(Error::Rejected(Gate::PreviousChain));
    }
    if !q.active {
        return Err(Error::Rejected(Gate::Inactive));
    }
    Ok(())
}
pub fn can_add(catalog: &Catalog, state: &State, id: u32) -> Result<(), Error> {
    check_catalog(catalog, state)?;
    catalog.row(id)?;
    if !state.snapshot.log.contains(&None) {
        return Err(Error::Rejected(Gate::LogFull));
    }
    // Source items were rejected by admission, so CanGiveQuestSourceItemIfNeed is inert.
    Ok(())
}
pub fn can_complete(
    catalog: &Catalog,
    state: &State,
    host: &impl Host,
    id: u32,
) -> Result<bool, Error> {
    check_catalog(catalog, state)?;
    let q = catalog.row(id)?;
    let Some(p) = state.progress(id) else {
        return Ok(false);
    };
    if p.status != Status::Incomplete || p.rewarded {
        return Ok(false);
    }
    Ok((0..OBJECTIVES).all(|i| {
        p.items[i] >= q.required_item_count[i] && p.creatures[i] >= q.required_creature_count[i]
    }) && host.money() >= money_required(q, host)?)
}
pub fn can_reward(
    catalog: &Catalog,
    state: &State,
    host: &impl Host,
    id: u32,
) -> Result<(), Error> {
    check_catalog(catalog, state)?;
    let q = catalog.row(id)?;
    let p = state
        .progress(id)
        .ok_or(Error::Rejected(Gate::NotComplete))?;
    if p.status != Status::Complete {
        return Err(Error::Rejected(Gate::NotComplete));
    }
    if p.rewarded {
        return Err(Error::Rejected(Gate::AlreadyRewarded));
    }
    for i in 0..OBJECTIVES {
        if q.required_item_count[i] != 0
            && host.item_count(q.required_item[i], false)? < q.required_item_count[i]
        {
            return Err(Error::Rejected(Gate::Items));
        }
    }
    if host.money() < money_required(q, host)? {
        return Err(Error::Rejected(Gate::Money));
    }
    // No choice/fixed reward items: their complete storage/equip/random-property rules are excluded.
    Ok(())
}
fn set_status(state: &mut State, id: u32, status: Status, events: &mut Vec<Event>) {
    state
        .snapshot
        .records
        .get_mut(&id)
        .expect("validated record")
        .status = status;
    events.push(Event::WorldObjects { view: state.view() }); // SetQuestStatus calls this BEFORE quest-slot flags.
    if let Some(slot) = logged(state, id) {
        events.push(Event::LogComplete {
            slot,
            complete: status == Status::Complete,
        });
    }
}
fn maybe_complete(
    catalog: &Catalog,
    state: &mut State,
    host: &impl Host,
    id: u32,
    events: &mut Vec<Event>,
) -> Result<bool, Error> {
    if can_complete(catalog, state, host, id)? {
        set_status(state, id, Status::Complete, events);
        return Ok(true);
    }
    Ok(false)
}
fn money_changed(
    catalog: &Catalog,
    state: &mut State,
    host: &impl Host,
    events: &mut Vec<Event>,
) -> Result<(), Error> {
    for id in state.snapshot.log.into_iter().flatten() {
        let q = catalog.row(id)?;
        if q.reward.reward_or_required_money < 0 {
            if state.progress(id).expect("log").status == Status::Incomplete {
                if host.money() >= money_required(q, host)? {
                    maybe_complete(catalog, state, host, id, events)?;
                }
            } else if host.money() < money_required(q, host)? {
                set_status(state, id, Status::Incomplete, events);
            }
        }
    }
    Ok(())
}
fn before_item_removed(
    catalog: &Catalog,
    state: &mut State,
    host: &impl Host,
    item: u32,
    count: u32,
    events: &mut Vec<Event>,
) -> Result<(), Error> {
    for id in state.snapshot.log.into_iter().flatten() {
        let q = catalog.row(id)?;
        for i in 0..OBJECTIVES {
            if q.required_item[i] != item {
                continue;
            }
            let p = state.snapshot.records.get_mut(&id).expect("log");
            let required = q.required_item_count[i];
            let mut current = p.items[i];
            if current >= required {
                current = host.item_count(item, false)?;
            }
            // Source operands are uint32. Preserve defined wrapping (bank-count edge).
            if p.items[i] > current.wrapping_sub(count) {
                let removed = if current <= required {
                    count
                } else {
                    count.wrapping_add(required).wrapping_sub(current)
                };
                p.items[i] = current.wrapping_sub(removed);
                set_status(state, id, Status::Incomplete, events);
                events.push(Event::WorldObjects { view: state.view() });
            }
        }
    }
    Ok(())
}
fn remove_items(
    catalog: &Catalog,
    state: &mut State,
    host: &mut impl Host,
    item: u32,
    count: u32,
    events: &mut Vec<Event>,
) -> Result<(), Error> {
    if count == 0 {
        return Ok(());
    }
    let steps = host.removal_steps(item, count)?;
    if steps.contains(&0)
        || steps.iter().try_fold(0u32, |sum, &v| sum.checked_add(v)) != Some(count)
    {
        return Err(Error::Host("removal_steps"));
    }
    for count in steps {
        let before = host.item_count(item, false)?;
        if before < count {
            return Err(Error::Host("carried_item_count"));
        }
        before_item_removed(catalog, state, host, item, count, events)?;
        let effect = HostEffect::DestroyCarriedStack { item, count };
        host.apply(&effect)?;
        if host.item_count(item, false)? != before - count {
            return Err(Error::Host("item_removal_delta"));
        }
        events.push(Event::Host(effect));
    }
    Ok(())
}
/// Pure transaction: clones inputs, rejects sequence replay and returns staged outputs.
/// Persist the sequence, quest snapshot, host inventory/currency/XP and emitted-event
/// outbox atomically. This function never writes storage or publishes notifications.
pub fn reduce<H: Host>(
    catalog: &Catalog,
    state: &State,
    host: &H,
    sequence: u64,
    command: Command,
) -> Result<Transaction<H>, Error> {
    if state.snapshot.catalog_hash != catalog.hash() {
        return Err(Error::Invalid("catalog_changed"));
    }
    if sequence != state.next_sequence() {
        return Err(Error::Invalid("command_sequence"));
    }
    let next = sequence
        .checked_add(1)
        .ok_or(Error::Invalid("sequence_exhausted"))?;
    if !matches!(host.participation(), Participation::Solo) {
        return Err(Error::Unsupported("group_participation"));
    }
    if host.money() > MAX_MONEY || host.identity().level != host.reward_context().player_level {
        return Err(Error::Host("player_context"));
    }
    quest::eligibility_gates(
        crate::quest::QuestEligibilityInput {
            minimum_level: 1,
            maximum_level: 255,
            required_classes: 0,
            required_races: 0,
        },
        host.identity(),
    )?;
    let mut state = state.clone();
    let mut host = host.clone();
    let mut events = Vec::new();
    match command {
        Command::Accept { quest: id, giver } => {
            if !catalog.relations.starters.contains(&CreatureQuestRelation {
                creature: giver,
                quest: id,
            }) {
                return Err(Error::Rejected(Gate::Giver));
            }
            can_take(catalog, &state, &host, id)?;
            can_add(catalog, &state, id)?;
            let q = catalog.row(id)?;
            let slot = state
                .snapshot
                .log
                .iter()
                .position(Option::is_none)
                .expect("CanAddQuest");
            state.snapshot.records.insert(
                id,
                Progress {
                    status: Status::Incomplete,
                    rewarded: false,
                    items: [0; OBJECTIVES],
                    creatures: [0; OBJECTIVES],
                },
            );
            state.snapshot.log[slot] = Some(id);
            events.push(Event::LogSet {
                slot,
                quest: Some(id),
            });
            events.push(Event::AcceptHook { quest: id, giver });
            for i in 0..OBJECTIVES {
                if q.required_item_count[i] != 0 {
                    state.snapshot.records.get_mut(&id).expect("added").items[i] = host
                        .item_count(q.required_item[i], true)?
                        .min(q.required_item_count[i]);
                }
            }
            events.push(Event::WorldObjects { view: state.view() });
            maybe_complete(catalog, &mut state, &host, id, &mut events)?;
        }
        Command::KillCredit { entry, guid } | Command::TalkCredit { entry, guid } => {
            if entry == 0 {
                return Err(Error::Invalid("kill_entry"));
            }
            for id in state.snapshot.log.into_iter().flatten() {
                let q = catalog.row(id)?;
                if state.progress(id).expect("log").status != Status::Incomplete {
                    continue;
                }
                for i in 0..OBJECTIVES {
                    if q.required_creature[i] <= 0 || q.required_creature[i] as u32 != entry {
                        continue;
                    }
                    let p = state.snapshot.records.get_mut(&id).expect("log");
                    if p.creatures[i] < q.required_creature_count[i] {
                        p.creatures[i] += 1;
                        events.push(Event::KillCredit {
                            quest: id,
                            guid,
                            objective: i,
                            count: p.creatures[i],
                        });
                    }
                    maybe_complete(catalog, &mut state, &host, id, &mut events)?;
                }
            }
        }
        Command::ItemAdded { item, count } => {
            if item == 0 || count == 0 || host.item_count(item, false)? < count {
                return Err(Error::Invalid("item_addition"));
            }
            for id in state.snapshot.log.into_iter().flatten() {
                let q = catalog.row(id)?;
                if state.progress(id).expect("log").status != Status::Incomplete {
                    continue;
                }
                for i in 0..OBJECTIVES {
                    if q.required_item[i] != item {
                        continue;
                    }
                    let p = state.snapshot.records.get_mut(&id).expect("log");
                    let previous = p.items[i];
                    let required = q.required_item_count[i];
                    if previous < required {
                        // Exclude source wraparound rather than invent a saturated amount.
                        let sum = previous
                            .checked_add(count)
                            .ok_or(Error::Invalid("item_add_overflow"))?;
                        let added = if sum <= required {
                            count
                        } else {
                            required - previous
                        };
                        p.items[i] += added;
                        events.push(Event::ItemCredit {
                            quest: id,
                            objective: i,
                            previous,
                            added,
                        });
                    }
                    if !maybe_complete(catalog, &mut state, &host, id, &mut events)?
                        && state.progress(id).expect("log").items[i] == required
                    {
                        events.push(Event::WorldObjects { view: state.view() });
                    }
                }
            }
        }
        Command::RemoveItems { item, count } => {
            if item == 0 {
                return Err(Error::Invalid("item_entry"));
            }
            remove_items(catalog, &mut state, &mut host, item, count, &mut events)?;
        }
        Command::Complete { quest: id } => {
            if !maybe_complete(catalog, &mut state, &host, id, &mut events)? {
                return Err(Error::Rejected(Gate::NotComplete));
            }
        }
        Command::MoneyChanged => money_changed(catalog, &mut state, &host, &mut events)?,
        Command::Reward {
            quest: id,
            giver,
            announce,
        } => {
            if !catalog
                .relations
                .finishers
                .contains(&CreatureQuestRelation {
                    creature: giver,
                    quest: id,
                })
            {
                return Err(Error::Rejected(Gate::Giver));
            }
            can_reward(catalog, &state, &host, id)?;
            let q = catalog.row(id)?;
            let quote = quest::reward_quote(q.reward, host.reward_context())?;
            // Preflight the complete numerical block before mutation.
            if quote.max_level_money.is_some_and(|v| v as u32 > MAX_MONEY)
                || quote.reward_or_required_money > MAX_MONEY as i32
            {
                return Err(Error::Invalid("money_operation"));
            }
            for i in 0..OBJECTIVES {
                if q.required_item[i] != 0 {
                    remove_items(
                        catalog,
                        &mut state,
                        &mut host,
                        q.required_item[i],
                        q.required_item_count[i],
                        &mut events,
                    )?;
                }
            }
            events.push(Event::RemoveTimedQuest(id));
            events.push(Event::RewardReputation(id)); // Proven no-op by row admission.
            if let Some(slot) = logged(&state, id) {
                state.snapshot.log[slot] = None;
                events.push(Event::LogSet { slot, quest: None });
            }
            if let Some(xp) = quote.xp_to_give {
                let effect = HostEffect::GiveXp(xp);
                host.apply(&effect)?;
                events.push(Event::Host(effect));
            }
            for money in quote
                .max_level_money
                .into_iter()
                .chain([quote.reward_or_required_money])
            {
                let effect = HostEffect::ModifyMoney(money);
                let before = host.money();
                host.apply(&effect)?;
                // Player.h ModifyMoney: each operation clamps separately.
                let expected =
                    (i64::from(before) + i64::from(money)).clamp(0, i64::from(MAX_MONEY)) as u32;
                if host.money() != expected {
                    return Err(Error::Host("money_delta"));
                }
                events.push(Event::Host(effect));
                money_changed(catalog, &mut state, &host, &mut events)?;
            }
            // SetQuestStatus without CompleteQuest's slot-bit update: slot is already gone.
            state.snapshot.records.get_mut(&id).expect("reward").status = Status::Complete;
            events.push(Event::WorldObjects { view: state.view() });
            state
                .snapshot
                .records
                .get_mut(&id)
                .expect("reward")
                .rewarded = true;
            if announce {
                events.push(Event::RewardAnnouncement {
                    quest: id,
                    xp: quote.display_xp,
                });
            }
            events.push(Event::RewardHook { quest: id, giver });
            events.push(Event::WorldObjects { view: state.view() });
        }
    }
    state.snapshot.next_sequence = next;
    State::restore(catalog, state.snapshot.clone())?;
    Ok(Transaction {
        state,
        host,
        events,
    })
}
