// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Source spawn-group selection arithmetic, without runtime or admission ownership.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RandomEntry {
    pub entry: u32,
    pub min: u32,
    pub max: u32,
    pub chance: u32,
}
pub trait GroupRandom {
    type Error;
    fn urand(&mut self, min: u32, max: u32) -> Result<u32, Self::Error>;
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<(), Self::Error>;
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SelectionError<E> {
    Owner(E),
    ChanceSubtractUndefined,
    RandomIndex,
    EntryIndex,
}

/// SpawnGroup.cpp:116-154. Unlike PoolGroup, entry chances are cumulative.
pub fn eligible_entry<R: GroupRandom>(
    entries: &[RandomEntry],
    explicit: &[usize],
    equal: &[usize],
    valid: &mut BTreeMap<u32, u32>,
    minimum: &BTreeMap<u32, u32>,
    random: &mut R,
) -> Result<u32, SelectionError<R::Error>> {
    if entries.is_empty() {
        return Ok(0);
    }
    if !minimum.is_empty() {
        let index = random
            .urand(0, minimum.len() as u32 - 1)
            .map_err(SelectionError::Owner)?;
        return minimum
            .keys()
            .nth(index as usize)
            .copied()
            .ok_or(SelectionError::RandomIndex);
    }
    if !explicit.is_empty() {
        let mut roll = random.urand(1, 100).map_err(SelectionError::Owner)? as i32;
        for &index in explicit {
            let entry = entries.get(index).ok_or(SelectionError::EntryIndex)?;
            if *valid.entry(entry.entry).or_default() > 0 {
                let chance = entry.chance as i32;
                if roll <= chance {
                    return Ok(entry.entry);
                }
                roll = roll
                    .checked_sub(chance)
                    .ok_or(SelectionError::ChanceSubtractUndefined)?;
            }
        }
    }
    if equal.is_empty() {
        return Ok(0);
    }
    let mut order = equal.to_vec();
    random.shuffle(&mut order).map_err(SelectionError::Owner)?;
    for index in order {
        let entry = entries.get(index).ok_or(SelectionError::EntryIndex)?;
        if *valid.entry(entry.entry).or_default() > 0 {
            return Ok(entry.entry);
        }
    }
    Ok(0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[repr(usize)]
pub enum GroupPhase {
    Templates,
    Formations,
    Members,
    Entries,
    Links,
    Squads,
}
impl GroupPhase {
    pub const ALL: [Self; 6] = [
        Self::Templates,
        Self::Formations,
        Self::Members,
        Self::Entries,
        Self::Links,
        Self::Squads,
    ];
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupTemplate {
    pub id: u32,
    pub name: Vec<u8>,
    pub kind: u32,
    pub max_count: u32,
    pub condition: i32,
    pub expression: i32,
    pub flags: u32,
    pub string_id: u32,
    pub respawn_min: Option<u32>,
    pub respawn_max: Option<u32>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Formation {
    pub group: u32,
    pub kind: u32,
    pub spread_bits: u32,
    pub options: u32,
    pub path: u32,
    pub movement: u32,
    pub comment: Vec<u8>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMember {
    pub group: u32,
    pub guid: u32,
    pub slot: i32,
    pub chance: u32,
    pub ordinal: u64,
    pub own_entry: u32,
    pub random_entry: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupInput {
    pub ordinal: u64,
    pub data: GroupInputData,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupInputData {
    Template(GroupTemplate),
    Formation(Formation),
    Member {
        group: u32,
        guid: u32,
        slot: i32,
        chance: u32,
    },
    Entry {
        group: u32,
        entry: RandomEntry,
    },
    Link {
        group: u32,
        linked: u32,
    },
    Squad {
        group: u32,
        squad: u32,
        guid: u32,
        entry: u32,
    },
}

/// ObjectMgr.cpp:1469-1484 uses wrapping uint32 addition, even for unlimited entries.
pub fn default_max_count(max: u32, entries: &[RandomEntry], members: u32) -> u32 {
    if max != 0 {
        return max;
    }
    if entries.is_empty() || entries.iter().any(|e| e.chance == 0) {
        return members;
    }
    entries
        .iter()
        .fold(0_u32, |sum, e| sum.wrapping_add(e.max))
        .min(members)
}
/// SpawnGroup.cpp:251-275 preserves zero-as-unlimited and first-insert minima.
pub fn entry_capacities(
    entries: &[RandomEntry],
    objects: &BTreeMap<u32, u32>,
) -> (BTreeMap<u32, u32>, BTreeMap<u32, u32>) {
    let mut valid = BTreeMap::new();
    let mut minimum = BTreeMap::new();
    for entry in entries {
        valid.insert(
            entry.entry,
            if entry.max == 0 { u32::MAX } else { entry.max },
        );
        if entry.min != 0 {
            minimum.entry(entry.entry).or_insert(entry.min);
        }
    }
    for entry in objects.values() {
        if !valid.is_empty() {
            let count = valid.entry(*entry).or_default();
            *count = count.saturating_sub(1);
        }
        if let Some(count) = minimum.get_mut(entry) {
            *count = count.wrapping_sub(1);
            if *count == 0 {
                minimum.remove(entry);
            }
        }
    }
    (valid, minimum)
}
/// SpawnGroup.cpp:219-234 runs after each selected nonzero entry.
pub fn erase_entry(entry: u32, valid: &mut BTreeMap<u32, u32>, minimum: &mut BTreeMap<u32, u32>) {
    if entry == 0 {
        return;
    }
    let count = valid.entry(entry).or_default();
    if *count != 0 {
        *count -= 1;
    }
    if let Some(count) = minimum.get_mut(&entry) {
        *count = count.wrapping_sub(1);
        if *count == 0 {
            minimum.remove(&entry);
        }
    }
}

/// SpawnGroup.cpp:439-471 and Object.cpp:3037-3046. Z comes from the caller twice.
pub fn in_vicinity(position_bits: [u32; 4], spawn_bits: [u32; 3], range_bits: u32) -> bool {
    use rustc_apfloat::{ieee::Single, Float, Round};
    let f = |bits| Single::from_bits(u128::from(bits));
    let dx = f(position_bits[0])
        .sub_r(f(spawn_bits[0]), Round::NearestTiesToEven)
        .value;
    let dy = f(position_bits[1])
        .sub_r(f(spawn_bits[1]), Round::NearestTiesToEven)
        .value;
    let dz = f(position_bits[2])
        .sub_r(f(position_bits[2]), Round::NearestTiesToEven)
        .value;
    let square = |value: Single| value.mul_r(value, Round::NearestTiesToEven).value;
    let distance = square(dx)
        .add_r(square(dy), Round::NearestTiesToEven)
        .value
        .add_r(square(dz), Round::NearestTiesToEven)
        .value;
    distance < square(f(range_bits))
}

// GCC 12.2 stl_algo.h:78-100,1780-1950 and stl_heap.h:135-147,224-250,340-360,418-426.
// FSF GPL-3.0-or-later WITH GCC-exception-3.1; existing SOURCE-RNG notices apply.
/// ObjectMgr.cpp:1336 uses std::sort, whose equal-slot ordering is not stable.
pub fn sort_members(values: &mut [GroupMember]) {
    if values.is_empty() {
        return;
    }
    introsort(
        values,
        (usize::BITS - 1 - values.len().leading_zeros()) as usize * 2,
    );
    let prefix = values.len().min(16);
    insertion(&mut values[..prefix]);
    for index in prefix..values.len() {
        linear_insert(values, index);
    }
}
fn insertion(values: &mut [GroupMember]) {
    for index in 1..values.len() {
        if values[index].slot < values[0].slot {
            let value = values[index];
            values.copy_within(0..index, 1);
            values[0] = value;
        } else {
            linear_insert(values, index);
        }
    }
}
fn linear_insert(values: &mut [GroupMember], mut hole: usize) {
    let value = values[hole];
    let mut next = hole - 1;
    while value.slot < values[next].slot {
        values[hole] = values[next];
        hole = next;
        next -= 1;
    }
    values[hole] = value;
}
fn introsort(mut values: &mut [GroupMember], mut depth: usize) {
    while values.len() > 16 {
        if depth == 0 {
            heap_sort(values);
            return;
        }
        depth -= 1;
        let a = 1;
        let b = values.len() / 2;
        let c = values.len() - 1;
        let median = if values[a].slot < values[b].slot {
            if values[b].slot < values[c].slot {
                b
            } else if values[a].slot < values[c].slot {
                c
            } else {
                a
            }
        } else if values[a].slot < values[c].slot {
            a
        } else if values[b].slot < values[c].slot {
            c
        } else {
            b
        };
        values.swap(0, median);
        let (mut first, mut last) = (1, values.len());
        let cut = loop {
            while values[first].slot < values[0].slot {
                first += 1;
            }
            last -= 1;
            while values[0].slot < values[last].slot {
                last -= 1;
            }
            if first >= last {
                break first;
            }
            values.swap(first, last);
            first += 1;
        };
        let (left, right) = values.split_at_mut(cut);
        introsort(right, depth);
        values = left;
    }
}
fn adjust_heap(values: &mut [GroupMember], mut hole: usize, length: usize, value: GroupMember) {
    let top = hole;
    let mut child = hole;
    while child < (length - 1) / 2 {
        child = 2 * (child + 1);
        if values[child].slot < values[child - 1].slot {
            child -= 1;
        }
        values[hole] = values[child];
        hole = child;
    }
    if length & 1 == 0 && child == (length - 2) / 2 {
        child = 2 * (child + 1);
        values[hole] = values[child - 1];
        hole = child - 1;
    }
    while hole > top {
        let parent = (hole - 1) / 2;
        if values[parent].slot >= value.slot {
            break;
        }
        values[hole] = values[parent];
        hole = parent;
    }
    values[hole] = value;
}
fn heap_sort(values: &mut [GroupMember]) {
    let length = values.len();
    let mut parent = (length - 2) / 2;
    loop {
        let value = values[parent];
        adjust_heap(values, parent, length, value);
        if parent == 0 {
            break;
        }
        parent -= 1;
    }
    let mut end = length;
    while end > 1 {
        end -= 1;
        let value = values[end];
        values[end] = values[0];
        adjust_heap(values, 0, end, value);
    }
}
#[cfg(test)]
#[path = "source_spawn_groups_tests.rs"]
mod tests;
