// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic LootMgr.cpp at 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS contributors, GPL-2.0-or-later; Rust GPL-3.0-or-later, no warranty.
//! Reconstructed ordinary single-owner corpse generation. Draws are distribution outcomes,
//! not a claim of equivalence to the upstream MT engine or std::shuffle word stream.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub const UPSTREAM_COMMIT: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub const POLICY: &str = "cmangos-8ec338a1-ordinary-loot-reconstruction-v2";
pub const MAX_ITEMS: usize = 16;
/// Admission bounds apply even after the output has filled all sixteen slots.
pub const MAX_PROCESS_BUDGET: u32 = 4096;
/// Bound retained source rows and item lookups before building derived plans or hashing.
pub const MAX_CAPTURE_ROWS: usize = 4096;
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid(&'static str),
    Unsupported(&'static str),
    Draw,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ordinary loot: {self:?}")
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Item {
    pub entry: u32,
    pub quality: u32,
    pub display_id: u32,
    pub flags: u32,
    pub start_quest: u32,
    pub extra_flags: u32,
    pub random_property: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum ItemLookup {
    Present(Item),
    Absent,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Row {
    pub item: u32,
    pub chance_or_quest_chance_bits: u32,
    pub group: u8,
    pub min_count_or_reference: i32,
    pub max_count: u32,
    pub condition_id: u16,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum OrderOrigin {
    WholeTableLoaderResult,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LoaderOrder {
    pub origin: OrderOrigin,
    pub complete_result_observed: bool,
    pub ordered_result_fingerprint: [u8; 32],
    pub result_row_count: u64,
    pub selected_row_positions: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capture {
    pub upstream_commit: String,
    pub source_fingerprint: [u8; 32],
    pub loot_id: u32,
    pub complete_selected_projection: bool,
    pub loader_order: LoaderOrder,
    pub rows: Vec<Row>,
    pub items: BTreeMap<u32, ItemLookup>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub quality_rate_bits: [u32; 7],
    pub referenced_rate_bits: u32,
    pub money_rate_bits: u32,
    pub corpse_empty_loot_show: bool,
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        for b in self
            .quality_rate_bits
            .into_iter()
            .chain([self.referenced_rate_bits, self.money_rate_bits])
        {
            let f = f32::from_bits(b);
            if !f.is_finite() || !(0.0..=1000.0).contains(&f) {
                return Err(Error::Invalid("loot rate"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct GroupPlan {
    explicit: Vec<usize>,
    equal: Vec<usize>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct TemplatePlan {
    entries: Vec<usize>,
    groups: Vec<GroupPlan>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Budget {
    passes: u32,
    work: u32,
    draws: u32,
}
impl Budget {
    fn add_repeated(&mut self, other: Self, repeats: u32) -> Result<(), Error> {
        for (total, cost) in [
            (&mut self.passes, other.passes),
            (&mut self.work, other.work),
            (&mut self.draws, other.draws),
        ] {
            *total = cost
                .checked_mul(repeats)
                .and_then(|cost| total.checked_add(cost))
                .filter(|total| *total <= MAX_PROCESS_BUDGET)
                .ok_or(Error::Invalid("loot processing budget"))?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Catalog {
    captures: BTreeMap<u32, Capture>,
    reference_captures: BTreeMap<u32, Capture>,
    #[serde(skip)]
    plans: BTreeMap<u32, TemplatePlan>,
    #[serde(skip)]
    reference_plans: BTreeMap<u32, TemplatePlan>,
    identity: [u8; 32],
}
impl Catalog {
    pub fn new(captures: Vec<Capture>) -> Result<Self, Error> {
        Self::new_with_references(captures, Vec::new())
    }
    /// Body and reference template IDs occupy distinct source table namespaces.
    /// Captures retain whole-loader witnesses; this does not reconstruct loader order.
    pub fn new_with_references(
        captures: Vec<Capture>,
        reference_captures: Vec<Capture>,
    ) -> Result<Self, Error> {
        if captures.len().saturating_add(reference_captures.len()) > 4096 {
            return Err(Error::Invalid("catalog size"));
        }
        for length in [
            captures
                .iter()
                .chain(&reference_captures)
                .map(|c| c.rows.len())
                .try_fold(0usize, usize::checked_add),
            captures
                .iter()
                .chain(&reference_captures)
                .map(|c| c.items.len())
                .try_fold(0usize, usize::checked_add),
        ] {
            if length.is_none_or(|length| length > MAX_CAPTURE_ROWS) {
                return Err(Error::Invalid("retained loot closure size"));
            }
        }
        let (captures, plans) = validate_namespace(captures, false)?;
        let (reference_captures, reference_plans) = validate_namespace(reference_captures, true)?;
        let mut required = BTreeSet::new();
        let mut items = BTreeMap::new();
        for c in captures.values().chain(reference_captures.values()) {
            for (id, item) in &c.items {
                if items
                    .insert(*id, item)
                    .is_some_and(|previous| previous != item)
                {
                    return Err(Error::Invalid("inconsistent item closure"));
                }
            }
        }
        let mut reference_budgets = BTreeMap::new();
        for (id, c) in &reference_captures {
            reference_budgets.insert(
                *id,
                template_budget(c, &reference_plans[id], &BTreeMap::new())?,
            );
        }
        for (id, c) in &captures {
            for row in &c.rows {
                if row.min_count_or_reference < 0 {
                    required.insert(reference_id(row)?);
                }
            }
            let mut budget = template_budget(c, &plans[id], &reference_budgets)?;
            // GenerateMoneyLoot and fake-empty-window each use at most one integer draw.
            budget.add_repeated(
                Budget {
                    passes: 0,
                    work: 0,
                    draws: 2,
                },
                1,
            )?;
        }
        if required != reference_captures.keys().copied().collect() {
            return Err(Error::Invalid("reference closure"));
        }
        let mut hash = Sha256::new();
        hash.update(POLICY.as_bytes());
        hash_namespace(&mut hash, b"creature_loot_template", &captures);
        hash_namespace(&mut hash, b"reference_loot_template", &reference_captures);
        Ok(Self {
            captures,
            reference_captures,
            plans,
            reference_plans,
            identity: hash.finalize().into(),
        })
    }
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
    pub fn captures(&self) -> impl Iterator<Item = &Capture> {
        self.captures.values()
    }
    pub fn reference_captures(&self) -> impl Iterator<Item = &Capture> {
        self.reference_captures.values()
    }
    pub fn contains(&self, id: u32) -> bool {
        self.captures.contains_key(&id)
    }
    /// Source supports absent loot IDs; the caller must explicitly prove a missing table row.
    pub fn generate(
        &self,
        loot_id: u32,
        min_money: u32,
        max_money: u32,
        rules: &Rules,
        random: &mut impl Random,
    ) -> Result<Generated, Error> {
        rules.validate()?;
        if f64::from(max_money) * f64::from(f32::from_bits(rules.money_rate_bits)) > 1_000_000_000.0
        {
            return Err(Error::Invalid("bounded money"));
        }
        let mut out = Generated {
            items: vec![],
            money: 0,
            fake: false,
        };
        let capture = if loot_id == 0 {
            None
        } else {
            Some(
                self.captures
                    .get(&loot_id)
                    .ok_or(Error::Invalid("unresolved loot id"))?,
            )
        };
        if let Some(c) = capture {
            self.process(c, &self.plans[&loot_id], &mut out, rules, random)?;
        }
        if capture.is_some() || max_money > 0 {
            if max_money > 0 {
                let rate = f32::from_bits(rules.money_rate_bits);
                out.money = if max_money <= min_money {
                    (max_money as f32 * rate) as u32
                } else if max_money - min_money < 32700 {
                    (integer_draw(random, min_money, max_money)? as f32 * rate) as u32
                } else {
                    ((integer_draw(random, min_money >> 8, max_money >> 8)? as f32 * rate) as u32)
                        .checked_shl(8)
                        .ok_or(Error::Invalid("money shift"))?
                };
            }
            if out.items.is_empty() && out.money == 0 && rules.corpse_empty_loot_show {
                out.fake = integer_draw(random, 0, 2)? == 1;
            }
        }
        Ok(out)
    }
    fn process(
        &self,
        c: &Capture,
        plan: &TemplatePlan,
        out: &mut Generated,
        rules: &Rules,
        random: &mut impl Random,
    ) -> Result<(), Error> {
        for &ordinal in &plan.entries {
            let row = &c.rows[ordinal];
            let chance = f32::from_bits(row.chance_or_quest_chance_bits);
            let rate_bits = if row.min_count_or_reference < 0 {
                rules.referenced_rate_bits
            } else {
                let ItemLookup::Present(item) = &c.items[&row.item] else {
                    unreachable!()
                };
                rules.quality_rate_bits[item.quality as usize]
            };
            // LootStoreItem::Roll returns before rate multiplication at >=100%.
            if chance < 100.0
                && f64::from(chance * f32::from_bits(rate_bits)) <= chance_draw(random)?
            {
                continue;
            }
            if row.min_count_or_reference < 0 {
                let id = reference_id(row)?;
                let child = &self.reference_captures[&id];
                // LootTemplate::Process runs the complete child at this entry position.
                for _ in 0..row.max_count {
                    self.process(child, &self.reference_plans[&id], out, rules, random)?;
                }
            } else {
                add(out, c, ordinal, random)?;
            }
        }
        for group in &plan.groups {
            let mut selected = None;
            if !group.explicit.is_empty() {
                let mut explicit = group.explicit.clone();
                shuffle_draw(random, &mut explicit)?;
                let mut chance = chance_draw(random)? as f32;
                for i in explicit {
                    let weight = f32::from_bits(c.rows[i].chance_or_quest_chance_bits);
                    if weight >= 100.0 {
                        selected = Some(i);
                        break;
                    }
                    chance -= weight;
                    if chance < 0.0 {
                        selected = Some(i);
                        break;
                    }
                }
            }
            if selected.is_none() && !group.equal.is_empty() {
                let mut equal = group.equal.clone();
                shuffle_draw(random, &mut equal)?;
                for i in equal {
                    if out.items.iter().any(|x| x.item == c.rows[i].item)
                        && integer_draw(random, 0, 1)? != 0
                    {
                        continue;
                    }
                    selected = Some(i);
                    break;
                }
            }
            if let Some(i) = selected {
                add(out, c, i, random)?;
            }
        }
        Ok(())
    }
}
fn reference_id(row: &Row) -> Result<u32, Error> {
    row.min_count_or_reference
        .checked_neg()
        .filter(|id| *id > 0)
        .map(|id| id as u32)
        .ok_or(Error::Invalid("reference id"))
}
type ValidatedNamespace = (BTreeMap<u32, Capture>, BTreeMap<u32, TemplatePlan>);

fn validate_namespace(
    captures: Vec<Capture>,
    references: bool,
) -> Result<ValidatedNamespace, Error> {
    let mut map = BTreeMap::new();
    let mut plans = BTreeMap::new();
    let mut witness = None;
    let mut selected = BTreeSet::new();
    for c in captures {
        if c.upstream_commit != UPSTREAM_COMMIT
            || c.source_fingerprint == [0; 32]
            || c.loot_id == 0
            || !c.complete_selected_projection
            || !c.loader_order.complete_result_observed
            || c.loader_order.ordered_result_fingerprint == [0; 32]
            || c.rows.is_empty()
            || c.rows.len() > 1024
            || c.rows.len() != c.loader_order.selected_row_positions.len()
            || c.loader_order
                .selected_row_positions
                .windows(2)
                .any(|v| v[0] >= v[1])
            || c.loader_order
                .selected_row_positions
                .iter()
                .any(|p| *p >= c.loader_order.result_row_count)
        {
            return Err(Error::Invalid("complete ordered loader capture"));
        }
        let current = (
            c.source_fingerprint,
            c.loader_order.ordered_result_fingerprint,
            c.loader_order.result_row_count,
        );
        if witness.is_some_and(|previous| previous != current)
            || c.loader_order
                .selected_row_positions
                .iter()
                .any(|position| !selected.insert(*position))
        {
            return Err(Error::Invalid("incoherent loader snapshot"));
        }
        witness = Some(current);
        let mut required = BTreeSet::new();
        let mut plan = TemplatePlan::default();
        for (ordinal, r) in c.rows.iter().enumerate() {
            let chance = f32::from_bits(r.chance_or_quest_chance_bits);
            if !chance.is_finite() || chance < 0.0 || r.condition_id != 0 {
                return Err(Error::Unsupported("quest or conditional loot"));
            }
            // LootStore::Load checks the uint8 maxcount bound for every row.
            if r.group >= 128 || r.max_count > 255 || r.min_count_or_reference == 0 {
                return Err(Error::Invalid("source loader row"));
            }
            if r.min_count_or_reference < 0 {
                if references || r.group != 0 {
                    return Err(Error::Unsupported("nested or grouped reference loot"));
                }
                reference_id(r)?;
                if chance == 0.0 {
                    return Err(Error::Invalid("reference chance"));
                }
            } else {
                if r.item == 0
                    || r.max_count < r.min_count_or_reference as u32
                    || (chance == 0.0 && r.group == 0)
                    || (chance > 0.0 && chance < 0.000001)
                {
                    return Err(Error::Invalid("source loader row"));
                }
                let Some(ItemLookup::Present(i)) = c.items.get(&r.item) else {
                    return Err(Error::Invalid("item closure"));
                };
                if i.entry != r.item
                    || i.quality > 6
                    || i.display_id == 0
                    || i.flags & !0x800 != 0
                    || i.start_quest != 0
                    || i.extra_flags != 0
                    || i.random_property != 0
                {
                    return Err(Error::Unsupported("item properties"));
                }
                required.insert(r.item);
            }
            if r.group == 0 {
                plan.entries.push(ordinal);
            } else {
                plan.groups
                    .resize_with(plan.groups.len().max(r.group as usize), GroupPlan::default);
                let group = &mut plan.groups[r.group as usize - 1];
                if chance == 0.0 {
                    group.equal.push(ordinal);
                } else {
                    group.explicit.push(ordinal);
                }
            }
        }
        if c.items.keys().copied().collect::<BTreeSet<_>>() != required {
            return Err(Error::Invalid("item closure"));
        }
        plans.insert(c.loot_id, plan);
        if map.insert(c.loot_id, c).is_some() {
            return Err(Error::Invalid("catalog identity"));
        }
    }
    Ok((map, plans))
}
fn template_budget(
    c: &Capture,
    plan: &TemplatePlan,
    references: &BTreeMap<u32, Budget>,
) -> Result<Budget, Error> {
    let mut budget = Budget {
        passes: 1,
        work: 1,
        draws: 0,
    };
    for &ordinal in &plan.entries {
        let row = &c.rows[ordinal];
        budget.add_repeated(
            Budget {
                passes: 0,
                work: 1,
                draws: u32::from(f32::from_bits(row.chance_or_quest_chance_bits) < 100.0),
            },
            1,
        )?;
        if row.min_count_or_reference < 0 {
            let child = references
                .get(&reference_id(row)?)
                .ok_or(Error::Invalid("unresolved reference id"))?;
            budget.add_repeated(*child, row.max_count)?;
        } else {
            budget.add_repeated(
                Budget {
                    passes: 0,
                    work: 0,
                    draws: 1,
                },
                1,
            )?;
        }
    }
    for group in &plan.groups {
        let explicit = group.explicit.len() as u32;
        let equal = group.equal.len() as u32;
        // A shuffle is bounded by N host draws (Fisher-Yates uses N-1).
        // Count every candidate, every possible duplicate coin, and one selected count.
        budget.add_repeated(
            Budget {
                passes: 0,
                work: 1 + 2 * (explicit + equal),
                draws: explicit
                    + u32::from(explicit != 0)
                    + 2 * equal
                    + u32::from(explicit + equal != 0),
            },
            1,
        )?;
    }
    Ok(budget)
}
fn hash_namespace(hash: &mut Sha256, namespace: &[u8], captures: &BTreeMap<u32, Capture>) {
    hash.update((namespace.len() as u64).to_le_bytes());
    hash.update(namespace);
    hash.update((captures.len() as u64).to_le_bytes());
    for (id, c) in captures {
        hash.update(id.to_le_bytes());
        hash.update((c.rows.len() as u64).to_le_bytes());
        hash.update((c.items.len() as u64).to_le_bytes());
        hash.update(c.source_fingerprint);
        hash.update(c.loader_order.ordered_result_fingerprint);
        hash.update(c.loader_order.result_row_count.to_le_bytes());
        for (p, r) in c.loader_order.selected_row_positions.iter().zip(&c.rows) {
            hash.update(p.to_le_bytes());
            hash.update(r.item.to_le_bytes());
            hash.update(r.chance_or_quest_chance_bits.to_le_bytes());
            hash.update([r.group]);
            hash.update(r.min_count_or_reference.to_le_bytes());
            hash.update(r.max_count.to_le_bytes());
            hash.update(r.condition_id.to_le_bytes());
        }
        for item in c.items.values() {
            let ItemLookup::Present(i) = item else {
                unreachable!()
            };
            for value in [
                i.entry,
                i.quality,
                i.display_id,
                i.flags,
                i.start_quest,
                i.extra_flags,
                i.random_property,
            ] {
                hash.update(value.to_le_bytes());
            }
        }
    }
}
fn integer_draw(random: &mut impl Random, low: u32, high: u32) -> Result<u32, Error> {
    let value = random.inclusive(low, high)?;
    if !(low..=high).contains(&value) {
        return Err(Error::Draw);
    }
    Ok(value)
}
fn chance_draw(random: &mut impl Random) -> Result<f64, Error> {
    let value = random.chance()?;
    if !value.is_finite() || !(0.0..100.0).contains(&value) {
        return Err(Error::Draw);
    }
    Ok(value)
}
fn shuffle_draw(random: &mut impl Random, values: &mut [usize]) -> Result<(), Error> {
    let mut expected = values.to_vec();
    expected.sort_unstable();
    random.shuffle(values)?;
    let mut actual = values.to_vec();
    actual.sort_unstable();
    if actual != expected {
        return Err(Error::Draw);
    }
    Ok(())
}
fn add(out: &mut Generated, c: &Capture, i: usize, rng: &mut impl Random) -> Result<(), Error> {
    if out.items.len() == MAX_ITEMS {
        return Ok(());
    }
    let r = &c.rows[i];
    let ItemLookup::Present(item) = &c.items[&r.item] else {
        unreachable!()
    };
    let count = integer_draw(rng, r.min_count_or_reference as u32, r.max_count)?;
    if count < r.min_count_or_reference as u32 || count > r.max_count {
        return Err(Error::Draw);
    }
    out.items.push(LootItem {
        slot: out.items.len() as u8,
        item: r.item,
        count: count as u8,
        display_id: item.display_id,
        random_property: 0,
    });
    Ok(())
}
/// Host supplies bounded source distribution outcomes. A shuffle must be a permutation.
pub trait Random {
    fn inclusive(&mut self, low: u32, high: u32) -> Result<u32, Error>;
    fn chance(&mut self) -> Result<f64, Error>;
    fn shuffle(&mut self, values: &mut [usize]) -> Result<(), Error>;
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LootItem {
    pub slot: u8,
    pub item: u32,
    pub count: u8,
    pub display_id: u32,
    pub random_property: i32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generated {
    pub items: Vec<LootItem>,
    pub money: u32,
    pub fake: bool,
}
