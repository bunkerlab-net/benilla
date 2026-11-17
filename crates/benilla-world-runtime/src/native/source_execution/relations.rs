// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Relations.cpp:100-267,458-510,1131-1134, with first-read refusals.
use super::*;

pub trait SourceRelationOwner {
    fn relation_flags(&self, unit: ObjectHandle) -> Result<u32>;
    fn relation_faction(&self, _unit: ObjectHandle) -> Result<Option<&SourceFactionTemplate>> {
        Err(refuse(
            "execution.relations.GetFactionTemplateEntry_6209_owner",
        ))
    }
    fn relation_master(&self, _unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.relations.GetMaster_6716_owner"))
    }
    fn relation_persuaded(&self, _unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Err(refuse(
            "execution.relations.GetReactionTo_258_GetPersuadedGuid_owner",
        ))
    }
    fn relation_has_reputation(&self, _faction: u32) -> Result<bool> {
        Err(refuse(
            "execution.relations.GetReactionTo_262_Faction_owner",
        ))
    }
}

pub struct SourceRelations<'a> {
    pub objects: &'a SourceObjects,
    pub units: &'a SourceUnits,
    pub factions: &'a CreatureAiFactions,
}
impl SourceRelationOwner for SourceRelations<'_> {
    fn relation_flags(&self, unit: ObjectHandle) -> Result<u32> {
        Ok(self.units.get(self.objects, unit)?.flags)
    }
    fn relation_faction(&self, unit: ObjectHandle) -> Result<Option<&SourceFactionTemplate>> {
        Ok(self
            .factions
            .relation_template(self.units.get(self.objects, unit)?.faction))
    }
    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        let fields = self.units.get(self.objects, unit)?;
        let master = fields.charmer.or(fields.owner);
        if let Some(master) = master {
            self.objects.get(master)?;
        }
        Ok(master)
    }
    fn relation_persuaded(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.units.get(self.objects, unit)?.persuaded)
    }
    fn relation_has_reputation(&self, faction: u32) -> Result<bool> {
        Ok(self.factions.relation_has_reputation(faction))
    }
}

/// DBCStructure.h:401-410 retains every operand of Relations.cpp:100-142.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceFactionTemplate {
    pub faction: u32,
    pub flags: u32,
    pub group: u32,
    pub friends: u32,
    pub enemies: u32,
    pub enemy_factions: [u32; 4],
    pub friend_factions: [u32; 4],
}
impl SourceFactionTemplate {
    pub fn reaction_to(&self, other: &Self) -> u32 {
        if other.group & self.enemies != 0 {
            return 1;
        }
        if self.enemy_factions[0] != 0
            && other.faction != 0
            && self.enemy_factions.contains(&other.faction)
        {
            return 1;
        }
        if other.group & self.friends != 0 {
            return 4;
        }
        if self.friend_factions[0] != 0
            && other.faction != 0
            && self.friend_factions.contains(&other.faction)
        {
            return 4;
        }
        if self.group & other.friends != 0 {
            return 4;
        }
        if other.friend_factions[0] != 0
            && self.faction != 0
            && other.friend_factions.contains(&self.faction)
        {
            return 4;
        }
        3
    }
}

pub fn unit_reaction_to(
    caster: ObjectHandle,
    target: ObjectHandle,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<u32> {
    // Relations.cpp:203-204 returns before any flag or faction read.
    if caster == target {
        return Ok(4);
    }
    if owner.relation_flags(caster)? & 8 != 0 {
        return Err(refuse(
            "execution.relations.GetReactionTo_208_GetControllingPlayer_owner",
        ));
    }
    let mut reaction = 3;
    if let Some(this) = owner.relation_faction(caster)? {
        if let Some(other) = owner.relation_faction(target)? {
            if owner.relation_flags(target)? & 8 != 0 {
                return Err(refuse(
                    "execution.relations.GetFactionReaction_166_GetControllingPlayer_owner",
                ));
            }
            reaction = this.reaction_to(other);
        }
    }
    if reaction > 1
        && reaction < 5
        && (owner.relation_flags(target)? & 0x4000 != 0
            || owner.relation_persuaded(caster)? == Some(target))
    {
        if let Some(other) = owner.relation_faction(target)? {
            if owner.relation_has_reputation(other.faction)? {
                reaction += 1;
            }
        }
    }
    Ok(reaction)
}

/// Relations.cpp:473-482 reads immunity only when a master exists.
pub fn unit_is_pvp_ui(
    unit: ObjectHandle,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<bool> {
    if let Some(master) = owner.relation_master(unit)? {
        if owner.relation_flags(unit)? & 0x100 != 0 {
            return Ok(false);
        }
        return Ok(owner.relation_flags(master)? & 0x1000 != 0);
    }
    Ok(owner.relation_flags(unit)? & 0x1000 != 0)
}

pub fn unit_can_assist(
    caster: ObjectHandle,
    target: ObjectHandle,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<bool> {
    if owner.relation_flags(target)? & 0x0200_0000 != 0 {
        return Ok(false);
    }
    if unit_reaction_to(caster, target, owner)? < 4 {
        return Ok(false);
    }
    if owner.relation_flags(target)? & 8 != 0 {
        return Err(refuse(
            "execution.relations.CanAssist_487_GetControllingPlayer_owner",
        ));
    }
    if owner.relation_flags(caster)? & 8 == 0 {
        return Ok(true);
    }
    unit_is_pvp_ui(target, owner)
}

/// Relations.cpp:1131-1134 ignores spellInfo and delegates to CanAssist.
pub fn unit_can_assist_spell(
    caster: ObjectHandle,
    target: ObjectHandle,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<bool> {
    unit_can_assist(caster, target, owner)
}
