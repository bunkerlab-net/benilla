// SPDX-License-Identifier: GPL-3.0-or-later
//! Creature.cpp:1557-1567,3109-3162; Spell.cpp:3510-3516. Caster-specific cooldowns.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreatureCooldownMatch {
    Missing,
    Spell(u32),
    Category(u32),
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatureCooldownObservation {
    pub searches: u32,
    pub result: Option<CreatureCooldownMatch>,
    pub fallback: Option<Option<u32>>,
    pub master: Option<SourceMasterLookup>,
}

pub fn creature_spell_cooldown(
    list: Option<&CreatureSpellList>,
    spell: u32,
    random: &mut dyn CreatureRandom,
) -> Result<CreatureCooldownMatch> {
    // CreatureSpellList.h:84-91 constructs Spells empty even when Disabled is unwritten.
    if let Some(list) = list {
        for data in list.spells.values() {
            if data.spell_id == spell {
                let cooldown = random.creature_urand(data.repeat[0], data.repeat[1])?;
                return Ok(if data.flags & 4 != 0 {
                    CreatureCooldownMatch::Category(cooldown)
                } else {
                    CreatureCooldownMatch::Spell(cooldown)
                });
            }
        }
    }
    Ok(CreatureCooldownMatch::Missing)
}

pub fn world_object_spell_mod_owner(
    objects: &SourceObjects,
    caster: ObjectHandle,
    totem: bool,
) -> Result<Option<ObjectHandle>> {
    match objects.get(caster)?.kind {
        SourceObjectKind::Pet => Err(refuse("execution.spells.Pet_596.GetSpellModOwner_owner")),
        SourceObjectKind::Creature if totem => {
            Err(refuse("execution.spells.Totem_193.GetSpellModOwner_owner"))
        }
        SourceObjectKind::Player => Ok(Some(caster)),
        // Object.h:1156 returns null; plain Creature and Unit have no override.
        _ => Ok(None),
    }
}

pub trait CreatureCooldownOwner {
    fn caster_kind(&self) -> Result<SourceObjectKind>;
    fn spell_cooldown(&mut self, spell: u32) -> Result<CreatureCooldownMatch>;
    fn creature_cooldown(&mut self, spell: u32) -> Result<Option<u32>>;
    fn spell_mod_owner(&self) -> Result<Option<ObjectHandle>>;
    fn clock(&self) -> Result<i64>;
    fn category_flags(&self, category: u32) -> Result<Option<u32>>;
    fn master(&mut self) -> Result<Option<ObjectHandle>>;
    fn is_player(&self, unit: ObjectHandle) -> Result<bool>;
    fn modify_cooldown(
        &mut self,
        _player: ObjectHandle,
        _spell: u32,
        _duration: u32,
    ) -> Result<u32> {
        Err(refuse(
            "execution.spells.Player_ApplySpellMod_COOLDOWN_owner",
        ))
    }
    fn cooldown_packet(
        &mut self,
        _player: ObjectHandle,
        event: bool,
        _spell: u32,
        _duration: u32,
    ) -> Result<()> {
        Err(refuse(if event {
            "execution.spells.Creature_3146.cooldown_event_packet_owner"
        } else {
            "execution.spells.Creature_3154.spell_cooldown_packet_owner"
        }))
    }
}

impl SourceSpellCooldowns {
    pub fn send_spell_cooldown(
        &mut self,
        t: &SpellTemplate,
        channel_only: bool,
        owner: &mut impl CreatureCooldownOwner,
    ) -> Result<()> {
        if t.attributes & 0x40 != 0 || channel_only {
            return Ok(());
        }
        match owner.caster_kind()? {
            SourceObjectKind::Player => {
                Err(refuse("execution.spells.Player_2199.AddCooldown_owner"))
            }
            SourceObjectKind::Creature | SourceObjectKind::Pet => {
                self.add_creature_cooldown(t, 0, t.attributes & 0x0200_0000 != 0, false, owner)
            }
            _ => self.add_cooldown(
                t,
                0,
                false,
                || owner.clock(),
                |category| owner.category_flags(category),
            ),
        }
    }

    pub fn add_creature_cooldown(
        &mut self,
        t: &SpellTemplate,
        forced: u32,
        permanent: bool,
        ignore_category: bool,
        owner: &mut impl CreatureCooldownOwner,
    ) -> Result<()> {
        let mut duration = if forced != 0 { forced } else { t.recoverytime };
        let mut category_duration = if ignore_category {
            0
        } else {
            t.categoryrecoverytime
        };
        if forced == 0 {
            let result = owner.spell_cooldown(t.id)?;
            let cooldown = match result {
                CreatureCooldownMatch::Missing => owner.creature_cooldown(t.id)?,
                CreatureCooldownMatch::Spell(value) | CreatureCooldownMatch::Category(value) => {
                    Some(value)
                }
            };
            if let Some(cooldown) = cooldown.filter(|&value| value != 0) {
                match result {
                    CreatureCooldownMatch::Spell(_) => duration = cooldown,
                    CreatureCooldownMatch::Category(_) => category_duration = cooldown,
                    // Creature.cpp:3122-3125 leaves NOT_FOUND unchanged after an ObjectMgr hit.
                    CreatureCooldownMatch::Missing => {}
                }
            }
        }
        if duration != 0 || category_duration != 0 {
            if let Some(player) = owner.spell_mod_owner()? {
                if duration != 0 {
                    duration = owner.modify_cooldown(player, t.id, duration)?;
                } else if t.category != 0 && category_duration != 0 {
                    category_duration = owner.modify_cooldown(player, t.id, category_duration)?;
                }
            }
            self.add(
                owner.clock()?,
                t,
                duration,
                category_duration,
                permanent,
                |category| owner.category_flags(category),
            )?;
        }
        if duration != 0 || category_duration != 0 {
            if let Some(master) = owner.master()? {
                if owner.is_player(master)? {
                    owner.cooldown_packet(
                        master,
                        t.attributes & 0x0200_0000 != 0 && !permanent,
                        t.id,
                        duration,
                    )?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "creature_cooldowns_tests.rs"]
mod tests;
