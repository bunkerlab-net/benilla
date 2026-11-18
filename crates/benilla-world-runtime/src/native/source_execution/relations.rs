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
    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>>;
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
    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self
            .units
            .get(self.objects, unit)?
            .control_guids
            .get_master(self.objects, unit, None, unbound_pet_loading)?
            .unit())
    }
    fn relation_faction(&self, unit: ObjectHandle) -> Result<Option<&SourceFactionTemplate>> {
        Ok(self
            .factions
            .relation_template(self.units.get(self.objects, unit)?.faction))
    }
    fn relation_persuaded(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.units.get(self.objects, unit)?.persuaded)
    }
    fn relation_has_reputation(&self, faction: u32) -> Result<bool> {
        Ok(self.factions.relation_has_reputation(faction))
    }
}

/// The live update fields read by Unit.h:2566,2572, independent of object handles.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SourceControlGuids {
    pub charmer: SourceGuid,
    pub creator: SourceGuid,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceControlLookup {
    EmptyGuid,
    LoggedNull(SourceControlNotice),
    LoadingPlayerMissing,
    Found(ObjectHandle),
}
impl SourceControlLookup {
    pub fn unit(self) -> Option<ObjectHandle> {
        match self {
            Self::Found(unit) => Some(unit),
            Self::EmptyGuid | Self::LoggedNull(_) | Self::LoadingPlayerMissing => None,
        }
    }
}

/// The GetCharmer/GetCreator debug-only continuity messages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceControlNotice {
    AccessorOutsideWorld { source_line: u16 },
    MissingUnit { source_line: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceMasterLookup {
    pub charmer: SourceControlLookup,
    pub creator: Option<SourceControlLookup>,
}
impl SourceMasterLookup {
    pub fn unit(self) -> Option<ObjectHandle> {
        self.charmer
            .unit()
            .or(self.creator.and_then(SourceControlLookup::unit))
    }
}

pub(super) fn unbound_pet_loading(_: ObjectHandle) -> Result<bool> {
    Err(refuse(
        "execution.pet_ai.Unit_GetCharmer_GetCreator_out_of_world_loading_owner",
    ))
}

impl SourceControlGuids {
    /// Unit.h:1827 and ObjectGuid.h:124-129 test the GUID kind without a lookup.
    pub fn has_charmer(self) -> bool {
        is_unit_guid(self.charmer)
    }
    /// Unit.cpp:6709-6712. Unresolved unit GUIDs still count.
    pub fn has_master(self) -> bool {
        self.has_charmer() || is_unit_guid(self.creator)
    }
    /// Unit.h:1845,1854. Any nonzero charmer GUID takes precedence.
    pub fn master_guid(self) -> SourceGuid {
        if self.charmer.raw() != 0 {
            self.charmer
        } else {
            self.creator
        }
    }
    /// Unit.cpp:6580-6607.
    pub fn get_charmer(
        self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        pov: Option<ObjectHandle>,
        loading: impl FnMut(ObjectHandle) -> Result<bool>,
    ) -> Result<SourceControlLookup> {
        resolve_control_guid(
            objects,
            pov.unwrap_or(unit),
            self.charmer,
            loading,
            [6594, 6602],
        )
    }
    /// Unit.cpp:6609-6636.
    pub fn get_creator(
        self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        pov: Option<ObjectHandle>,
        loading: impl FnMut(ObjectHandle) -> Result<bool>,
    ) -> Result<SourceControlLookup> {
        resolve_control_guid(
            objects,
            pov.unwrap_or(unit),
            self.creator,
            loading,
            [6623, 6631],
        )
    }
    /// Unit.cpp:6696-6699,6714-6717. Nonrecursive GetOwner is GetCreator.
    pub fn get_master(
        self,
        objects: &SourceObjects,
        unit: ObjectHandle,
        pov: Option<ObjectHandle>,
        mut loading: impl FnMut(ObjectHandle) -> Result<bool>,
    ) -> Result<SourceMasterLookup> {
        let charmer = self.get_charmer(objects, unit, pov, &mut loading)?;
        let creator = if charmer.unit().is_none() {
            Some(self.get_creator(objects, unit, pov, loading)?)
        } else {
            None
        };
        Ok(SourceMasterLookup { charmer, creator })
    }
}

impl SourceUnits {
    /// Unit.cpp:6528-6540 resolves a possessing charmer; player self-control refuses at :6544.
    pub fn possessing_client(
        &self,
        objects: &SourceObjects,
        unit: ObjectHandle,
    ) -> Result<Option<ObjectHandle>> {
        let live = self.get(objects, unit)?;
        if live.flags & 8 == 0 {
            return Ok(None);
        }
        if !live.control_guids.has_charmer() {
            if objects.get(unit)?.kind == SourceObjectKind::Player {
                return Err(refuse(
                    "execution.relations.Unit_6544.IsClientControlled_owner",
                ));
            }
            return Ok(None);
        }
        if live.flags & 0x0100_0000 == 0 {
            return Ok(None);
        }
        let Some(charmer) = live
            .control_guids
            .get_charmer(objects, unit, None, unbound_pet_loading)?
            .unit()
        else {
            return Ok(None);
        };
        Ok((objects.get(charmer)?.kind == SourceObjectKind::Player).then_some(charmer))
    }
}

fn is_unit_guid(guid: SourceGuid) -> bool {
    guid.raw() != 0 && matches!(guid.high(), 0 | 0xf130 | 0xf140)
}

fn resolve_control_guid(
    objects: &SourceObjects,
    accessor: ObjectHandle,
    guid: SourceGuid,
    mut loading: impl FnMut(ObjectHandle) -> Result<bool>,
    [outside_line, missing_line]: [u16; 2],
) -> Result<SourceControlLookup> {
    use SourceControlLookup::*;
    if guid.raw() == 0 {
        return Ok(EmptyGuid);
    }
    let accessor = objects.get(accessor)?;
    if !accessor.in_world {
        if guid.high() == 0 && accessor.kind == SourceObjectKind::Pet && loading(accessor.handle)? {
            return Ok(objects
                .find_player(guid)?
                .map_or(LoadingPlayerMissing, Found));
        }
        return Ok(LoggedNull(SourceControlNotice::AccessorOutsideWorld {
            source_line: outside_line,
        }));
    }
    let map = accessor
        .map
        .ok_or_else(|| refuse("execution.relations.Map_2371.GetUnit_map_owner"))?;
    if let Some(found) = objects.lookup_unit(map, guid) {
        return Ok(Found(found));
    }
    Ok(LoggedNull(SourceControlNotice::MissingUnit {
        source_line: missing_line,
    }))
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
