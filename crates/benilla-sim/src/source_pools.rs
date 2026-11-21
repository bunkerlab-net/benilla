// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! PoolManager.cpp:139-265,395-406,1029. No runtime or persistent ownership.
use crate::source_rng::double_to_float_bits;
use rustc_apfloat::{ieee::Single, Float, Round};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolObject {
    pub guid: u32,
    pub chance_bits: u32,
    pub excluded: bool,
    pub ordinal: u64,
}
impl PoolObject {
    pub fn new(guid: u32, chance: f32, ordinal: u64) -> Self {
        Self {
            guid,
            chance_bits: chance.to_bits() & 0x7fff_ffff,
            excluded: false,
            ordinal,
        }
    }
    pub fn chance(self) -> f32 {
        f32::from_bits(self.chance_bits)
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolGroup {
    pub explicit: Vec<PoolObject>,
    pub equal: Vec<PoolObject>,
}
impl PoolGroup {
    pub fn add(&mut self, object: PoolObject, limit: u32) {
        if object.chance() != 0.0 && limit == 1 {
            self.explicit.push(object)
        } else {
            self.equal.push(object)
        }
    }
    pub fn is_empty(&self) -> bool {
        self.explicit.is_empty() && self.equal.is_empty()
    }
    pub fn valid_chances(&self) -> bool {
        if !self.equal.is_empty() {
            return true;
        }
        let total = self.explicit.iter().fold(Single::ZERO, |sum, member| {
            sum.add_r(
                Single::from_bits(member.chance_bits as u128),
                Round::NearestTiesToEven,
            )
            .value
        });
        total == Single::ZERO || total == Single::from_u128(100).value
    }
    // Equal first for exclusion, explicit first for hierarchy erasure; cpp:174-193,348-365.
    pub fn exclude(&mut self, guid: u32, excluded: bool) {
        if let Some(object) = self
            .equal
            .iter_mut()
            .chain(&mut self.explicit)
            .find(|v| v.guid == guid)
        {
            object.excluded = excluded;
        }
    }
    pub fn remove_relation(&mut self, guid: u32) {
        for list in [&mut self.explicit, &mut self.equal] {
            if let Some(index) = list.iter().position(|v| v.guid == guid) {
                list.remove(index);
            }
        }
    }
}
pub trait RollContext {
    type Error;
    /// Actual calling-thread GetRandomGenerator, even when both lists are empty.
    fn engine_access(&mut self) -> Result<(), Self::Error>;
    fn chance(&mut self) -> Result<f64, Self::Error>;
    fn shuffle(&mut self, indices: &mut [usize]) -> Result<(), Self::Error>;
    fn object(&self, explicit: bool, index: usize) -> PoolObject;
    fn can_spawn(&mut self, guid: u32) -> Result<bool, Self::Error>;
    fn selected(&self, guid: u32) -> Result<bool, Self::Error>;
}
/// Shuffles source list indices, never the persistent definition vectors.
pub fn roll_one<C: RollContext>(
    explicit: usize,
    equal: usize,
    trigger: u32,
    context: &mut C,
) -> Result<Option<PoolObject>, C::Error> {
    context.engine_access()?;
    let mut fallback = None;
    for (is_explicit, count) in [(true, explicit), (false, equal)] {
        if count == 0 {
            continue;
        }
        let roll = if is_explicit {
            f32::from_bits(double_to_float_bits(context.chance()?.to_bits()))
        } else {
            0.0
        };
        let mut order: Vec<_> = (0..count).collect();
        context.shuffle(&mut order)?;
        for index in order {
            let object = context.object(is_explicit, index);
            if object.excluded || !context.can_spawn(object.guid)? {
                continue;
            }
            if object.guid != trigger && context.selected(object.guid)? {
                continue;
            }
            if !is_explicit {
                return Ok(Some(object));
            }
            if fallback.is_none() {
                fallback = Some(object);
            }
            if roll < object.chance() {
                return Ok(Some(object));
            }
        }
    }
    Ok(fallback)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    CountIncrementUndefined,
    TemplateLoopUnbounded,
}
/// LP64 GCC converts the unsigned subtraction to signed int before ++count.
pub fn spawn_count(limit: u32, current: u32, selected_trigger: bool) -> Result<i32, Error> {
    let count = limit.wrapping_sub(current) as i32;
    if selected_trigger {
        count.checked_add(1).ok_or(Error::CountIncrementUndefined)
    } else {
        Ok(count)
    }
}
pub fn template_loop(maximum: u16) -> Result<(), Error> {
    if maximum == u16::MAX {
        Err(Error::TemplateLoopUnbounded)
    } else {
        Ok(())
    }
}
#[cfg(test)]
#[path = "source_pools_tests.rs"]
mod tests;
