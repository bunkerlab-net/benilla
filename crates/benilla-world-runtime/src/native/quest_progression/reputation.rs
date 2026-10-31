// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Player.cpp/ReputationMgr.cpp, Project contributors.
//! Immutable reputation closure, synchronous quest callbacks and replayed saved state.
use super::*;
use benilla_sim::reputation as rep;

pub const POLICY_ID: &str = "benilla-native-quest-reputation-v3";
const MAX_CALLS: usize = 2048;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
enum Call {
    AtWar {
        list_id: u32,
        on: bool,
    },
    Inactive {
        list_id: u32,
        on: bool,
    },
    Visible {
        faction: u32,
    },
    Reward {
        quest: u32,
        level: u8,
        progression_prefix: u32,
        money: u32,
        before: Box<q::Snapshot>,
        after: Box<q::Snapshot>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    call: Call,
    /// SendInitialReputations after this operation; repeated sends are idempotent.
    initialized: bool,
}
/// The primitive's structurally valid State is not sufficient saved authority. Restore
/// reconstructs every standing, flag and dirtiness bit from genesis and these source calls.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReputationState {
    state: rep::State,
    #[serde(deserialize_with = "deserialize_history")]
    history: Vec<Receipt>,
    item_incomplete: BTreeMap<u32, ItemRemovalWitness>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct ItemRemovalWitness {
    objective: usize,
    previous_counter: u32,
    carried_before: u32,
    removed: u32,
    after_counter: u32,
}
// Player::ItemRemovedQuestCheck uses unsigned arithmetic, including its
// intentional counter-above-requirement branch. Do not replace by saturation.
fn removed_counter(previous: u32, required: u32, carried: u32, count: u32) -> Option<u32> {
    let current = if previous >= required {
        carried
    } else {
        previous
    };
    if previous <= current.wrapping_sub(count) {
        return None;
    }
    let removed = if current <= required {
        count
    } else {
        count.wrapping_add(required).wrapping_sub(current)
    };
    Some(current.wrapping_sub(removed))
}
fn deserialize_history<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Vec<Receipt>, D::Error> {
    struct History;
    impl<'de> serde::de::Visitor<'de> for History {
        type Value = Vec<Receipt>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(f, "at most {MAX_CALLS} reputation source calls")
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            if seq.size_hint().is_some_and(|n| n > MAX_CALLS) {
                return Err(serde::de::Error::custom("reputation history limit"));
            }
            let mut calls = vec![];
            while let Some(call) = seq.next_element()? {
                if calls.len() == MAX_CALLS {
                    return Err(serde::de::Error::custom("reputation history limit"));
                }
                calls.push(call);
            }
            Ok(calls)
        }
    }
    d.deserialize_seq(History)
}
impl NativeReputationState {
    pub fn view(&self) -> &rep::State {
        &self.state
    }
    pub fn needs_initial_send(&self) -> bool {
        self.state.factions().values().any(|s| s.need_send)
    }
    fn record(&mut self, call: Call) -> Result<(), q::Error> {
        if self.history.len() >= MAX_CALLS {
            return Err(q::Error::Host("reputation history limit"));
        }
        self.history.push(Receipt {
            call,
            initialized: false,
        });
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Marker {
    Visible(u32),
    Reward(u32),
}
impl Marker {
    pub(super) fn from_event(event: &q::Event) -> Option<Self> {
        match event {
            q::Event::ReputationVisible(faction) => Some(Self::Visible(*faction)),
            q::Event::RewardReputation(quest) => Some(Self::Reward(*quest)),
            _ => None,
        }
    }
}
fn identity(input: &init::StarterInput) -> rep::Identity {
    rep::Identity {
        race_mask: 1 << (input.race - 1),
        class_mask: 1 << (input.class - 1),
    }
}
fn reward(row: &q::QuestRow) -> rep::QuestReward {
    rep::QuestReward {
        factions: row.excluded.reward_rep_faction,
        values: row.excluded.reward_rep_value,
        faction_flags: row.excluded.reward_faction_flags,
        // Quest lifecycle's existing unsigned level domain is explicitly 0..=255.
        quest_level: row.reward.xp.quest_level as i32,
    }
}
fn related(row: &q::QuestRow) -> impl Iterator<Item = u32> {
    [
        row.excluded.rep_objective_faction,
        row.excluded.required_min_rep_faction,
        row.excluded.required_max_rep_faction,
    ]
    .into_iter()
    .chain(row.excluded.reward_rep_faction)
    .filter(|id| *id != 0)
}
/// The source initializer owns application membership and order. A catalog's audit
/// assertion alone cannot replace the retained holder list or its raw source row.
pub(super) fn validate_binding(
    catalog: Option<&rep::Catalog>,
    input: &init::StarterInput,
    mage: &init::InitializedMage,
    combat: &CombatGenesis,
) -> Result<(), NativeError> {
    let mut expected = vec![];
    let mut metadata = vec![];
    for aura in &mage.initial_auras {
        let raw = input
            .spells
            .iter()
            .find(|raw| raw.id == aura.spell_id)
            .ok_or(NativeError::InvalidProfile)?;
        let retained = InitialAuraMetadata::from_source(aura, raw);
        if aura.aura_type == 156 {
            // initialize_mage already proved -1 or passive index-zero duration-zero.
            expected.push(rep::GeneralReputationAura {
                spell_id: retained.spell_id,
                effect_index: retained.effect_index,
                aura_type: retained.aura_type,
                misc_value: retained.misc_value,
                amount: retained.amount,
                attributes: retained.attributes,
                attributes_ex3: retained.attributes_ex3,
                caster_aura_state: retained.caster_aura_state,
                aura_interrupt_flags: retained.aura_interrupt_flags,
                duration_ms: -1,
                source_spell_hash: raw.source_hash(),
            });
        }
        metadata.push(retained);
    }
    if metadata != combat.initial_auras {
        return Err(NativeError::InvalidProfile);
    }
    if let Some(catalog) = catalog {
        catalog.validate()?;
        if catalog.general_reputation_auras() != expected {
            return Err(NativeError::InvalidProfile);
        }
    }
    // With no catalog, all reputation consumers remain unavailable. Retaining the
    // passive never authorizes a later consumer with an empty substitute context.
    Ok(())
}
impl Profile {
    /// Semantic classifiers cannot admit an arbitrary holder merely by its aura156 tag.
    /// Revalidate the exact source-bound genesis even outside Profile::validate.
    pub(in crate::native) fn validate_retained_reputation_auras(&self) -> Result<(), NativeError> {
        let combat = self.combat.as_ref().ok_or(NativeError::InvalidProfile)?;
        let expected = self
            .quest_progression
            .as_ref()
            .and_then(QuestProgressionGenesis::reputation_catalog)
            .is_some_and(|c| !c.general_reputation_auras().is_empty());
        if expected || combat.initial_auras.iter().any(|a| a.aura_type == 156) {
            let genesis = self
                .quest_progression
                .as_ref()
                .filter(|g| g.reputation_catalog().is_some())
                .ok_or(NativeError::UnsupportedMechanic(
                    "retained reputation aura requires bound source genesis",
                ))?;
            genesis.validate(self)?;
        }
        Ok(())
    }
}
pub(super) fn validate_input(
    catalog: Option<&rep::Catalog>,
    rows: &[q::QuestRow],
    input: &init::StarterInput,
    mage: &init::InitializedMage,
    combat: &CombatGenesis,
) -> Result<(), NativeError> {
    validate_binding(catalog, input, mage, combat)?;
    match catalog {
        Some(catalog) => {
            catalog.validate()?;
            if serde_json::to_vec(catalog)
                .map_err(|_| NativeError::InvalidProfile)?
                .len()
                > MAX_BYTES / 2
            {
                return Err(NativeError::TooLarge);
            }
            if input.race == 0 || input.race > 32 || input.class == 0 || input.class > 32 {
                return Err(NativeError::InvalidProfile);
            }
            let state = catalog.initialize(identity(input))?;
            for row in rows {
                for faction in related(row) {
                    catalog.row(faction)?;
                    state.current(catalog, faction)?;
                }
                // Preflight each admitted player level later is unnecessary for numeric
                // authority: actual operations remain fallible and atomically staged.
            }
        }
        None if rows.iter().any(|row| {
            related(row).next().is_some()
                || row.excluded.reward_rep_value != [0; 5]
                || row.excluded.reward_faction_flags != 0
                || row.excluded.rep_objective_value != 0
                || row.excluded.required_min_rep_value != 0
                || row.excluded.required_max_rep_value != 0
        }) =>
        {
            return Err(NativeError::UnsupportedMechanic(
                "quest reputation closure unavailable",
            ));
        }
        None => {}
    }
    Ok(())
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(super) fn initialize(
    g: &QuestProgressionGenesis,
) -> Result<Option<NativeReputationState>, NativeError> {
    g.reputation
        .as_ref()
        .map(|catalog| {
            let state = catalog
                .initialize(identity(g.training.input()))?
                .reduce(catalog, rep::Command::SendInitial)?
                .state;
            Ok(NativeReputationState {
                state,
                history: vec![],
                item_incomplete: BTreeMap::new(),
            })
        })
        .transpose()
}
impl QuestProgressionGenesis {
    pub fn reputation_catalog(&self) -> Option<&rep::Catalog> {
        self.reputation.as_ref()
    }
}
impl QuestProgressionState {
    pub fn reputation(
        &self,
        g: &QuestProgressionGenesis,
        faction: u32,
    ) -> Result<i32, NativeError> {
        self.reputation
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("reputation unavailable"))?
            .state
            .current(
                g.reputation.as_ref().ok_or(NativeError::InvalidState)?,
                faction,
            )
            .map_err(Into::into)
    }
}
fn compact(state: &q::State) -> q::Snapshot {
    let mut snapshot = state.snapshot();
    snapshot
        .records
        .retain(|id, _| snapshot.log.contains(&Some(*id)));
    snapshot
}
fn q_error(_: rep::Error) -> q::Error {
    q::Error::Host("source reputation transaction")
}

#[derive(Clone)]
struct CallbackHost<'a> {
    quests: q::State,
    host: QuestHost<'a>,
    callbacks: VecDeque<(u32, rep::State, Vec<QuestProgressionEvent>)>,
}
impl rep::Host for CallbackHost<'_> {
    fn current_level(&self) -> u8 {
        self.host.player.view().level as u8
    }
    fn reputation_changed(
        &mut self,
        catalog: &rep::Catalog,
        state: &mut rep::State,
        faction: u32,
    ) -> rep::Result<Vec<rep::Event>> {
        if self.host.genesis.reputation.as_ref() != Some(catalog) {
            return Err(rep::Error::Callback("catalog changed"));
        }
        self.host
            .reputation
            .as_mut()
            .ok_or(rep::Error::Callback("reputation unavailable"))?
            .state = state.clone();
        let mut events = vec![];
        q::reputation_changed(
            &self.host.genesis.quests,
            &mut self.quests,
            &self.host,
            faction,
            &mut events,
        )
        .map_err(|_| rep::Error::Callback("source quest reputation callback"))?;
        let events = events
            .into_iter()
            .map(|event| QuestProgressionEvent::Quest {
                event,
                level: self.current_level(),
                money: self.host.money,
            })
            .collect();
        self.callbacks.push_back((faction, state.clone(), events));
        Ok(vec![])
    }
}
impl QuestHost<'_> {
    pub(super) fn current_reputation(&self, faction: u32) -> Result<i32, q::Error> {
        self.reputation
            .as_ref()
            .ok_or(q::Error::Unsupported("reputation unavailable"))?
            .state
            .current(
                self.genesis
                    .reputation
                    .as_ref()
                    .ok_or(q::Error::Host("reputation catalog"))?,
                faction,
            )
            .map_err(q_error)
    }
    pub(super) fn show_reputation(&mut self, faction: u32) -> Result<(), q::Error> {
        let catalog = self
            .genesis
            .reputation
            .as_ref()
            .ok_or(q::Error::Unsupported("reputation unavailable"))?;
        let reputation = self
            .reputation
            .as_mut()
            .ok_or(q::Error::Host("reputation state"))?;
        let tx = reputation
            .state
            .reduce(catalog, rep::Command::Visible { faction })
            .map_err(q_error)?;
        reputation.record(Call::Visible { faction })?;
        reputation.state = tx.state;
        self.reputation_calls.push_back((
            Marker::Visible(faction),
            tx.events
                .into_iter()
                .map(|e| QuestProgressionEvent::Reputation(Box::new(e)))
                .collect(),
        ));
        Ok(())
    }
    pub(super) fn grant_reputation(
        &mut self,
        catalog: &q::Catalog,
        state: &mut q::State,
        quest: u32,
    ) -> Result<(), q::Error> {
        let row = catalog.row(quest)?;
        let Some(reputation_catalog) = self.genesis.reputation.as_ref() else {
            if related(row).next().is_some()
                || row.excluded.reward_rep_value != [0; 5]
                || row.excluded.reward_faction_flags != 0
            {
                return Err(q::Error::Unsupported("reputation unavailable"));
            }
            return Ok(());
        };
        if catalog.hash() != self.genesis.quests.hash() {
            return Err(q::Error::Host("quest catalog"));
        }
        let initial = self
            .reputation
            .as_ref()
            .ok_or(q::Error::Host("reputation state"))?;
        let before = Box::new(compact(state));
        let level = self.player.view().level as u8;
        let progression_prefix = u32::try_from(self.progression.derived_calls.len())
            .map_err(|_| q::Error::Host("progression history limit"))?;
        let money = self.money;
        let host = CallbackHost {
            quests: state.clone(),
            host: self.clone(),
            callbacks: VecDeque::new(),
        };
        let tx = initial
            .state
            .reward_quest_with_host(reputation_catalog, reward(row), &host)
            .map_err(q_error)?;
        let mut callbacks = tx.host.callbacks;
        let mut events = vec![];
        for event in tx.transition.events {
            let callback = if let rep::Event::ReputationChanged { faction, snapshot } = &event {
                let (expected, state, nested) = callbacks
                    .pop_front()
                    .ok_or(q::Error::Host("reputation callback missing"))?;
                if expected != *faction || state != *snapshot {
                    return Err(q::Error::Host("reputation callback order"));
                }
                nested
            } else {
                vec![]
            };
            events.push(QuestProgressionEvent::Reputation(Box::new(event)));
            events.extend(callback);
        }
        if !callbacks.is_empty() {
            return Err(q::Error::Host("reputation callback residue"));
        }
        *state = tx.host.quests;
        *self = tx.host.host;
        let saved = self
            .reputation
            .as_mut()
            .ok_or(q::Error::Host("reputation state"))?;
        saved.state = tx.transition.state;
        saved.record(Call::Reward {
            quest,
            level,
            progression_prefix,
            money,
            before,
            after: Box::new(compact(state)),
        })?;
        self.reputation_calls
            .push_back((Marker::Reward(quest), events));
        Ok(())
    }
}

/// Replay needs only logged objective counters, money and identity. Unsupported effects
/// remain errors; no inventory or progression values are fabricated for the callback.
#[derive(Clone)]
#[cfg(any(test, feature = "synthetic-native-tests"))]
struct ReplayHost<'a> {
    catalog: &'a rep::Catalog,
    quests_catalog: &'a q::Catalog,
    reputation: rep::State,
    quests: q::State,
    identity: arithmetic::QuestPlayerIdentity,
    context: arithmetic::RewardContext,
    money: u32,
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
impl q::Host for ReplayHost<'_> {
    fn participation(&self) -> q::Participation {
        q::Participation::Solo
    }
    fn identity(&self) -> arithmetic::QuestPlayerIdentity {
        self.identity
    }
    fn reward_context(&self) -> arithmetic::RewardContext {
        self.context
    }
    fn money(&self) -> u32 {
        self.money
    }
    fn reputation(&self, faction: u32) -> Result<i32, q::Error> {
        self.reputation
            .current(self.catalog, faction)
            .map_err(q_error)
    }
    fn item_count(&self, _: u32, _: bool) -> Result<u32, q::Error> {
        Err(q::Error::Host(
            "unexpected reputation replay inventory query",
        ))
    }
    fn removal_steps(&self, _: u32, _: u32) -> Result<Vec<u32>, q::Error> {
        Err(q::Error::Host("unexpected reputation replay removal"))
    }
    fn can_store_reward(
        &self,
        _: u32,
        _: u32,
        _: &mut Vec<inv::Destination>,
    ) -> Result<inv::InventoryResult, q::Error> {
        Err(q::Error::Host("unexpected reputation replay item reward"))
    }
    fn apply(&mut self, _: &q::HostEffect) -> Result<(), q::Error> {
        Err(q::Error::Host("unexpected reputation replay effect"))
    }
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
impl rep::Host for ReplayHost<'_> {
    fn current_level(&self) -> u8 {
        self.identity.level
    }
    fn reputation_changed(
        &mut self,
        _: &rep::Catalog,
        state: &mut rep::State,
        faction: u32,
    ) -> rep::Result<Vec<rep::Event>> {
        self.reputation = state.clone();
        let host = self.clone();
        q::reputation_changed(
            self.quests_catalog,
            &mut self.quests,
            &host,
            faction,
            &mut vec![],
        )
        .map_err(|_| rep::Error::Callback("saved quest callback"))?;
        Ok(vec![])
    }
}
pub(super) fn record_objective_mutations(
    before: &QuestProgressionState,
    next: &mut QuestProgressionState,
    g: &QuestProgressionGenesis,
    events: &[QuestProgressionEvent],
) -> Result<(), NativeError> {
    let Some(reputation) = next.reputation.as_mut() else {
        return Ok(());
    };
    let mut view = before.quests.view();
    let mut removal_before = view.clone();
    let mut counts = BTreeMap::new();
    for id in g.quests.ids() {
        for item in g
            .quests
            .row(id)?
            .required_item
            .into_iter()
            .filter(|id| *id != 0)
        {
            counts.insert(
                item,
                before
                    .inventory
                    .as_ref()
                    .ok_or(NativeError::InvalidState)?
                    .item_count(item)?,
            );
        }
    }
    // Owner-directed destruction can remove non-quest loot too. Keep every required
    // item entry above, and additionally track admitted inventory entries without
    // silently dropping a required-item proof when metadata is missing.
    if let Some(loot) = &g.loot {
        let inventory = before.inventory.as_ref().ok_or(NativeError::InvalidState)?;
        for item in loot.inventory.items() {
            counts
                .entry(item.entry)
                .or_insert(inventory.item_count(item.entry)?);
        }
    }
    for event in events {
        let QuestProgressionEvent::Quest { event, .. } = event else {
            continue;
        };
        match event {
            q::Event::WorldObjects { view: current } => view = current.clone(),
            q::Event::LogSet { slot, quest } => {
                if quest.is_none() {
                    if let Some(previous) = view.log[*slot] {
                        reputation.item_incomplete.remove(&previous);
                    }
                }
                view.log[*slot] = *quest;
            }
            q::Event::LogComplete {
                slot,
                complete: true,
            } => {
                if let Some(id) = view.log[*slot] {
                    reputation.item_incomplete.remove(&id);
                }
            }
            q::Event::Host(q::HostEffect::DestroyCarriedStack { item, count }) => {
                let carried = *counts.get(item).ok_or(NativeError::InvalidState)?;
                if *count == 0 || *count > carried {
                    return Err(NativeError::InvalidState);
                }
                for id in removal_before.log.into_iter().flatten() {
                    let row = g.quests.row(id)?;
                    if row.excluded.rep_objective_faction == 0 {
                        continue;
                    }
                    for objective in 0..q::OBJECTIVES {
                        if row.required_item[objective] != *item {
                            continue;
                        }
                        let previous = removal_before.records[&id].items[objective];
                        if let Some(after_counter) = removed_counter(
                            previous,
                            row.required_item_count[objective],
                            carried,
                            *count,
                        ) {
                            let current = view.records.get(&id).ok_or(NativeError::InvalidState)?;
                            if current.status != q::Status::Incomplete
                                || current.items[objective] != after_counter
                            {
                                return Err(NativeError::InvalidState);
                            }
                            reputation.item_incomplete.insert(
                                id,
                                ItemRemovalWitness {
                                    objective,
                                    previous_counter: previous,
                                    carried_before: carried,
                                    removed: *count,
                                    after_counter,
                                },
                            );
                        }
                    }
                }
                counts.insert(*item, carried - count);
                removal_before = view.clone();
            }
            q::Event::Host(q::HostEffect::StoreRewardItem { item, count }) => {
                if let Some(carried) = counts.get_mut(item) {
                    *carried = carried
                        .checked_add(*count)
                        .ok_or(NativeError::InvalidState)?;
                }
                removal_before = view.clone();
            }
            _ => {}
        }
    }
    let final_view = next.quests.view();
    reputation.item_incomplete.retain(|id, _| {
        final_view.log.contains(&Some(*id))
            && final_view.records[id].status == q::Status::Incomplete
    });
    Ok(())
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
fn validate_incomplete_objectives(
    g: &QuestProgressionGenesis,
    saved: &QuestProgressionState,
) -> Result<(), NativeError> {
    let reputation = saved
        .reputation
        .as_ref()
        .ok_or(NativeError::InvalidCheckpoint)?;
    if reputation.item_incomplete.len() > q::LOG_SIZE {
        return Err(NativeError::InvalidCheckpoint);
    }
    let view = saved.quests.view();
    for (&id, witness) in &reputation.item_incomplete {
        let row = g.quests.row(id)?;
        let progress = view
            .records
            .get(&id)
            .ok_or(NativeError::InvalidCheckpoint)?;
        let item = row
            .required_item
            .get(witness.objective)
            .copied()
            .ok_or(NativeError::InvalidCheckpoint)?;
        let stack = g
            .loot
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?
            .inventory
            .item(item)?
            .stackable as u32;
        if witness.removed > stack
            || witness.carried_before > u32::from(inv::BACKPACK_END - inv::BACKPACK_START) * stack
        {
            return Err(NativeError::InvalidCheckpoint);
        }
        if !view.log.contains(&Some(id))
            || progress.status != q::Status::Incomplete
            || row.excluded.rep_objective_faction == 0
            || witness.objective >= q::OBJECTIVES
            || row.required_item[witness.objective] == 0
            || witness.removed == 0
            || witness.removed > witness.carried_before
            || removed_counter(
                witness.previous_counter,
                row.required_item_count[witness.objective],
                witness.carried_before,
                witness.removed,
            ) != Some(witness.after_counter)
        {
            return Err(NativeError::InvalidCheckpoint);
        }
    }
    for (&id, progress) in &view.records {
        let row = g.quests.row(id)?;
        if progress.rewarded
            || progress.status != q::Status::Incomplete
            || row.excluded.rep_objective_faction == 0
        {
            continue;
        }
        let money =
            arithmetic::reward_or_required_money(row.reward.reward_or_required_money, g.money_rate)
                .map_err(q::Error::from)?;
        let all_met = (0..q::OBJECTIVES).all(|i| {
            progress.items[i] >= row.required_item_count[i]
                && progress.creatures[i] >= row.required_creature_count[i]
        }) && (money >= 0 || saved.money >= money.unsigned_abs())
            && reputation.state.current(
                g.reputation
                    .as_ref()
                    .ok_or(NativeError::InvalidCheckpoint)?,
                row.excluded.rep_objective_faction,
            )? >= row.excluded.rep_objective_value;
        if !all_met {
            continue;
        }
        let witness = reputation
            .item_incomplete
            .get(&id)
            .ok_or(NativeError::InvalidCheckpoint)?;
        let item = row.required_item[witness.objective];
        let carried = saved
            .inventory
            .as_ref()
            .ok_or(NativeError::InvalidCheckpoint)?
            .item_count(item)?;
        if progress.items[witness.objective] != witness.after_counter
            || carried != witness.carried_before - witness.removed
        {
            return Err(NativeError::InvalidCheckpoint);
        }
    }
    Ok(())
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
fn validate_completed_objectives(
    g: &QuestProgressionGenesis,
    reputation: &rep::State,
    quests: &q::Snapshot,
) -> Result<(), NativeError> {
    let catalog = g
        .reputation
        .as_ref()
        .ok_or(NativeError::InvalidCheckpoint)?;
    for (&id, progress) in &quests.records {
        let row = g.quests.row(id)?;
        let faction = row.excluded.rep_objective_faction;
        if !progress.rewarded
            && progress.status == q::Status::Complete
            && faction != 0
            && reputation.current(catalog, faction)? < row.excluded.rep_objective_value
        {
            return Err(NativeError::InvalidCheckpoint);
        }
    }
    Ok(())
}
#[cfg(any(test, feature = "synthetic-native-tests"))]
pub(super) fn validate_saved(
    g: &QuestProgressionGenesis,
    saved: &QuestProgressionState,
    progression_boundaries: &BTreeMap<usize, u32>,
) -> Result<(), NativeError> {
    let (Some(catalog), Some(saved_rep)) = (&g.reputation, &saved.reputation) else {
        return if g.reputation.is_none() && saved.reputation.is_none() {
            Ok(())
        } else {
            Err(NativeError::InvalidCheckpoint)
        };
    };
    if saved_rep.history.len() > MAX_CALLS {
        return Err(NativeError::InvalidCheckpoint);
    }
    let mut state = initialize(g)?.ok_or(NativeError::InvalidCheckpoint)?.state;
    let mut visible = BTreeMap::<u32, usize>::new();
    let mut rewarded = BTreeSet::new();
    let mut previous_level = 1;
    let mut previous_sequence = None;
    let mut previous_progression_prefix = 0;
    let final_quests = saved.quests.snapshot();
    for receipt in &saved_rep.history {
        state = match &receipt.call {
            Call::AtWar { list_id, on } => {
                state
                    .reduce(
                        catalog,
                        rep::Command::AtWar {
                            list_id: *list_id,
                            on: *on,
                        },
                    )?
                    .state
            }
            Call::Inactive { list_id, on } => {
                state
                    .reduce(
                        catalog,
                        rep::Command::Inactive {
                            list_id: *list_id,
                            on: *on,
                        },
                    )?
                    .state
            }
            Call::Visible { faction } => {
                *visible.entry(*faction).or_default() += 1;
                state
                    .reduce(catalog, rep::Command::Visible { faction: *faction })?
                    .state
            }
            Call::Reward {
                quest,
                level,
                progression_prefix,
                money,
                before,
                after,
            } => {
                let prefix_level = *progression_boundaries
                    .get(&(*progression_prefix as usize))
                    .ok_or(NativeError::InvalidCheckpoint)?;
                if u32::from(*level) != prefix_level
                    || *progression_prefix < previous_progression_prefix
                    || *level < previous_level
                    || u32::from(*level) > saved.player.view().level
                    || *money > q::MAX_MONEY
                    || !rewarded.insert(*quest)
                    || previous_sequence.is_some_and(|previous| before.next_sequence <= previous)
                    || before.next_sequence >= saved.quests.next_sequence()
                    || !before.log.contains(&Some(*quest))
                    || before.records.keys().any(|id| {
                        !before.log.contains(&Some(*id)) || !final_quests.records.contains_key(id)
                    })
                    || after
                        .records
                        .keys()
                        .any(|id| !after.log.contains(&Some(*id)))
                {
                    return Err(NativeError::InvalidCheckpoint);
                }
                previous_level = *level;
                previous_progression_prefix = *progression_prefix;
                previous_sequence = Some(before.next_sequence);
                if !g.levels.supplied_levels().any(|l| l == u32::from(*level)) {
                    return Err(NativeError::InvalidCheckpoint);
                }
                validate_completed_objectives(g, &state, before)?;
                let quests = q::State::restore(&g.quests, *before.clone())?;
                let host = ReplayHost {
                    catalog,
                    quests_catalog: &g.quests,
                    reputation: state.clone(),
                    quests,
                    identity: arithmetic::QuestPlayerIdentity {
                        race: saved.player.view().race,
                        class: saved.player.view().class,
                        level: *level,
                    },
                    context: g.reward_context(u32::from(*level)),
                    money: *money,
                };
                let tx =
                    state.reward_quest_with_host(catalog, reward(g.quests.row(*quest)?), &host)?;
                if compact(&tx.host.quests) != **after {
                    return Err(NativeError::InvalidCheckpoint);
                }
                tx.transition.state
            }
        };
        if receipt.initialized {
            state = state.reduce(catalog, rep::Command::SendInitial)?.state;
        }
    }
    validate_completed_objectives(g, &state, &final_quests)?;
    validate_incomplete_objectives(g, saved)?;
    let expected_rewarded: BTreeSet<_> = final_quests
        .records
        .iter()
        .filter_map(|(&id, p)| p.rewarded.then_some(id))
        .collect();
    let mut expected_visible = BTreeMap::<u32, usize>::new();
    for &id in final_quests.records.keys() {
        let faction = g.quests.row(id)?.excluded.rep_objective_faction;
        if faction != 0 {
            *expected_visible.entry(faction).or_default() += 1;
        }
    }
    if state != saved_rep.state || rewarded != expected_rewarded || visible != expected_visible {
        return Err(NativeError::InvalidCheckpoint);
    }
    Ok(())
}

impl QuestProgressionState {
    /// Source CharacterHandler calls by reputation-list slot, without a living/combat guard.
    /// The source emits no immediate packet. Dirty send/save flags remain reducer-owned.
    pub(in crate::native) fn reputation_control(
        &mut self,
        g: &QuestProgressionGenesis,
        command: rep::Command,
    ) -> Result<(), NativeError> {
        let call = match command {
            rep::Command::AtWar { list_id, on } => Call::AtWar { list_id, on },
            rep::Command::Inactive { list_id, on } => Call::Inactive { list_id, on },
            _ => return Err(NativeError::InvalidState),
        };
        let catalog = g
            .reputation
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("reputation unavailable"))?;
        let reputation = self.reputation.as_ref().ok_or(NativeError::InvalidState)?;
        let tx = reputation.state.reduce(catalog, command)?;
        if !tx.events.is_empty() {
            return Err(NativeError::InvalidState);
        }
        let mut staged = reputation.clone();
        staged.record(call)?;
        staged.state = tx.state;
        self.reputation = Some(staged);
        Ok(())
    }
    pub(in crate::native) fn acknowledge_initial_reputations(
        &mut self,
        g: &QuestProgressionGenesis,
    ) -> Result<(), NativeError> {
        let catalog = g
            .reputation
            .as_ref()
            .ok_or(NativeError::UnsupportedMechanic("reputation unavailable"))?;
        let reputation = self.reputation.as_mut().ok_or(NativeError::InvalidState)?;
        reputation.state = reputation
            .state
            .reduce(catalog, rep::Command::SendInitial)?
            .state;
        if let Some(last) = reputation.history.last_mut() {
            last.initialized = true;
        }
        Ok(())
    }
}
impl NativeWorld {
    /// Bootstrap has sent the committed full list; delivery acknowledgement is the
    /// first input of the next natural tick, before any gameplay reputation operation.
    pub fn reputation_initial_ack_needed(&self) -> bool {
        self.state
            .quest_progression
            .as_ref()
            .and_then(|q| q.reputation.as_ref())
            .is_some_and(NativeReputationState::needs_initial_send)
    }
}
