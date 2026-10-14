// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic LootMgr.cpp at 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
// Copyright CMaNGOS contributors, GPL-2.0-or-later; Rust GPL-3.0-or-later, no warranty.
//! Reconstructed ordinary single-owner corpse generation. Draws are distribution outcomes,
//! not a claim of equivalence to the upstream MT engine or std::shuffle word stream.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
pub const UPSTREAM_COMMIT: &str = "8ec338a1704e7dcb1c0213eb7ed58f9231ade40f";
pub const POLICY: &str = "cmangos-8ec338a1-ordinary-loot-reconstruction-v1";
pub const MAX_ITEMS: usize = 16;
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
    pub money_rate_bits: u32,
    pub corpse_empty_loot_show: bool,
}
impl Rules {
    pub fn validate(&self) -> Result<(), Error> {
        for b in self
            .quality_rate_bits
            .into_iter()
            .chain([self.money_rate_bits])
        {
            let f = f32::from_bits(b);
            if !f.is_finite() || !(0.0..=1000.0).contains(&f) {
                return Err(Error::Invalid("loot rate"));
            }
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct Catalog {
    captures: BTreeMap<u32, Capture>,
    identity: [u8; 32],
}
impl Catalog {
    pub fn new(captures: Vec<Capture>) -> Result<Self, Error> {
        if captures.len() > 4096 {
            return Err(Error::Invalid("catalog size"));
        }
        let mut map = BTreeMap::new();
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
            let mut required = BTreeSet::new();
            for r in &c.rows {
                let chance = f32::from_bits(r.chance_or_quest_chance_bits);
                if !chance.is_finite()
                    || chance < 0.0
                    || r.condition_id != 0
                    || r.min_count_or_reference <= 0
                {
                    return Err(Error::Unsupported("quest conditional or reference loot"));
                }
                if r.item == 0
                    || r.group >= 128
                    || r.max_count > 255
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
            if c.items.keys().copied().collect::<BTreeSet<_>>() != required
                || map.insert(c.loot_id, c).is_some()
            {
                return Err(Error::Invalid("catalog identity"));
            }
        }
        let mut hash = Sha256::new();
        hash.update(POLICY.as_bytes());
        hash.update((map.len() as u64).to_le_bytes());
        // Identity is a canonical encoding of all input fields, independent of Debug text.
        for (id, c) in &map {
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
            for i in c.items.values() {
                let ItemLookup::Present(i) = i else {
                    unreachable!()
                };
                for n in [
                    i.entry,
                    i.quality,
                    i.display_id,
                    i.flags,
                    i.start_quest,
                    i.extra_flags,
                    i.random_property,
                ] {
                    hash.update(n.to_le_bytes());
                }
            }
        }
        Ok(Self {
            captures: map,
            identity: hash.finalize().into(),
        })
    }
    pub fn identity(&self) -> [u8; 32] {
        self.identity
    }
    pub fn captures(&self) -> impl Iterator<Item = &Capture> {
        self.captures.values()
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
            for (ordinal, r) in c.rows.iter().enumerate().filter(|(_, r)| r.group == 0) {
                let ItemLookup::Present(i) = &c.items[&r.item] else {
                    unreachable!()
                };
                let chance = f32::from_bits(r.chance_or_quest_chance_bits);
                if chance >= 100.0
                    || f64::from(
                        chance * f32::from_bits(rules.quality_rate_bits[i.quality as usize]),
                    ) > chance_draw(random)?
                {
                    add(&mut out, c, ordinal, random)?;
                }
            }
            let groups = c.rows.iter().map(|r| r.group).max().unwrap_or(0);
            for group in 1..=groups {
                let mut explicit = c
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        r.group == group && f32::from_bits(r.chance_or_quest_chance_bits) != 0.0
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let mut equal = c
                    .rows
                    .iter()
                    .enumerate()
                    .filter(|(_, r)| {
                        r.group == group && f32::from_bits(r.chance_or_quest_chance_bits) == 0.0
                    })
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let mut selected = None;
                if !explicit.is_empty() {
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
                if selected.is_none() && !equal.is_empty() {
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
                    add(&mut out, c, i, random)?;
                }
            }
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
