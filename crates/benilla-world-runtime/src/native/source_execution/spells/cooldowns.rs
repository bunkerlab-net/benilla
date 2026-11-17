// SPDX-License-Identifier: GPL-3.0-or-later
//! Object.h:117-284; Object.cpp:2599-2745,3102-3138. Live WorldObject cooldowns.
use super::*;

pub trait SpellCooldownClock {
    fn map_clock(&mut self) -> Result<i64>;
    fn world_clock(&mut self) -> Result<i64>;
}
#[derive(Clone, Debug)]
pub struct SourceCooldownData {
    pub spell: u32,
    pub category: u32,
    pub spell_expiry: i64,
    pub category_expiry: i64,
    pub permanent: bool,
    pub item: u32,
}
impl SourceCooldownData {
    fn spell_expired(&self, now: i64) -> bool {
        !self.permanent && now >= self.spell_expiry
    }
    fn category_expired(&self, now: i64) -> bool {
        !self.permanent && (self.category == 0 || now >= self.category_expiry)
    }
}
// Object.cpp:1169-1175; Object.h:1181-1183 default-construct the empty maps.
#[derive(Clone, Debug, Default)]
pub struct SourceSpellCooldowns {
    pub spells: BTreeMap<u32, SourceCooldownData>,
    pub categories: BTreeMap<u32, u32>,
    pub gcd: SourceSpellGcd,
    pub lockouts: BTreeMap<u32, i64>,
    global_expiry: i64,
}
impl SourceSpellCooldowns {
    pub fn is_empty(&self) -> bool {
        self.spells.is_empty()
            && self.categories.is_empty()
            && self.gcd.is_empty()
            && self.lockouts.is_empty()
            && self.global_expiry == 0
    }
    pub fn is_spell_ready(
        &self,
        t: &SpellTemplate,
        item: bool,
        in_world: bool,
        clock: &mut impl SpellCooldownClock,
    ) -> Result<bool> {
        let now = if in_world {
            clock.map_clock()?
        } else {
            clock.world_clock()?
        };
        self.ready_at(t, item, now)
    }
    pub fn ready_at(&self, t: &SpellTemplate, item: bool, now: i64) -> Result<bool> {
        if self.global_expiry > now {
            return Ok(false);
        }
        require(!item, "execution.spells.Object_2718.item_prototype_owner")?;
        if self
            .spells
            .get(&t.id)
            .is_some_and(|cd| !cd.spell_expired(now))
        {
            return Ok(false);
        }
        if t.category != 0
            && self
                .categories
                .get(&t.category)
                .and_then(|id| self.spells.get(id))
                .is_some_and(|cd| !cd.category_expired(now))
        {
            return Ok(false);
        }
        if t.preventiontype == 1 {
            require(
                t.school < 32,
                "execution.spells.Object_2742.school_shift_undefined",
            )?;
            for (&school, &expiry) in &self.lockouts {
                require(
                    school < 32,
                    "execution.spells.Object_2669.lockout_shift_undefined",
                )?;
                if (1u32 << school) & (1u32 << t.school) != 0 && expiry > now {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }
    pub fn add_gcd(
        &mut self,
        t: &SpellTemplate,
        forced: u32,
        now: impl FnOnce() -> Result<i64>,
    ) -> Result<()> {
        let duration = if forced != 0 {
            forced
        } else {
            t.startrecoverytime
        };
        if duration == 0 {
            return Ok(());
        }
        self.gcd.add(t, now()?, duration)
    }
    pub fn add_cooldown(
        &mut self,
        t: &SpellTemplate,
        forced: u32,
        ignore_category: bool,
        now: impl FnOnce() -> Result<i64>,
        category_flags: impl FnOnce(u32) -> Result<Option<u32>>,
    ) -> Result<()> {
        let duration = if forced != 0 { forced } else { t.recoverytime };
        let category_duration = if ignore_category {
            0
        } else {
            t.categoryrecoverytime
        };
        if duration != 0 || category_duration != 0 {
            self.add(
                now()?,
                t,
                duration,
                category_duration,
                false,
                category_flags,
            )?;
        }
        Ok(())
    }
    pub fn send_spell_cooldown(
        &mut self,
        t: &SpellTemplate,
        channel_only: bool,
        now: impl FnOnce() -> Result<i64>,
        category_flags: impl FnOnce(u32) -> Result<Option<u32>>,
    ) -> Result<()> {
        if t.attributes & 0x40 != 0 || channel_only {
            return Ok(());
        }
        self.add_cooldown(t, 0, false, now, category_flags)
    }
    pub fn add(
        &mut self,
        now: i64,
        t: &SpellTemplate,
        duration: u32,
        category_duration: u32,
        permanent: bool,
        category_flags: impl FnOnce(u32) -> Result<Option<u32>>,
    ) -> Result<()> {
        self.remove_spell(t.id);
        let spell_expiry = if duration != 0 {
            expiry(now, duration)?
        } else {
            0
        };
        let category_expiry = if t.category != 0 && category_duration != 0 {
            expiry(now, category_duration)?
        } else {
            0
        };
        self.spells.insert(
            t.id,
            SourceCooldownData {
                spell: t.id,
                category: t.category,
                spell_expiry,
                category_expiry,
                permanent,
                item: 0,
            },
        );
        if t.category != 0 && category_duration != 0 {
            let flags = category_flags(t.category)?
                .ok_or_else(|| refuse("execution.spells.Object_3110.category_null_undefined"))?;
            if flags & 2 != 0 {
                self.global_expiry = category_expiry;
                return Ok(());
            }
            if let Some(&id) = self.categories.get(&t.category) {
                let original = self.spells.get_mut(&id).unwrap();
                if !permanent || !original.permanent {
                    original.category_expiry = category_expiry;
                    original.permanent = false;
                }
                self.spells.get_mut(&t.id).unwrap().category = 0;
            } else {
                self.categories.insert(t.category, t.id);
            }
        }
        Ok(())
    }
    pub fn remove_spell(&mut self, spell: u32) {
        if let Some(cd) = self.spells.remove(&spell) {
            if cd.category != 0 {
                self.categories.remove(&cd.category);
            }
        }
    }
    pub fn remove_category(&mut self, category: u32) {
        if let Some(id) = self.categories.remove(&category) {
            self.spells.get_mut(&id).unwrap().category = 0;
        }
    }
    pub fn update(&mut self, now: i64) {
        self.gcd.expire(now);
        self.lockouts.retain(|_, expiry| *expiry > now);
        self.spells.retain(|_, cd| {
            if cd.spell_expired(now) && cd.category_expired(now) {
                if cd.category != 0 {
                    self.categories.remove(&cd.category);
                }
                false
            } else {
                if cd.category != 0 && cd.category_expired(now) {
                    self.categories.remove(&cd.category);
                    cd.category = 0;
                }
                true
            }
        });
    }
}
fn expiry(now: i64, duration: u32) -> Result<i64> {
    now.checked_add(i64::from(duration))
        .ok_or_else(|| refuse("execution.spells.cooldown_clock_add_undefined"))
}
#[cfg(test)]
#[path = "cooldowns_tests.rs"]
mod tests;
