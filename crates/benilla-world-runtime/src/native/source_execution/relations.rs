// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Relations.cpp:100-267,458-510,1131-1134, with first-read refusals.
use super::*;
use super::{player::SourcePlayer, unit_vitals::live_player};

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
    fn relation_kind(&self, _unit: ObjectHandle) -> Result<SourceObjectKind> {
        Err(refuse("execution.relations.Relations_358.GetTypeId_owner"))
    }
    fn relation_player_flags(&self, _player: ObjectHandle, read: &'static str) -> Result<u32> {
        Err(refuse(read))
    }
    fn relation_creature_type_flags(&self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse(
            "execution.relations.Relations_360.GetCreatureInfo_owner",
        ))
    }
    fn relation_controlling_player(
        &self,
        _unit: ObjectHandle,
        read: &'static str,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse(read))
    }
    fn relation_forced_rank(
        &self,
        _player: ObjectHandle,
        _faction: u32,
        read: &'static str,
    ) -> Result<Option<u32>> {
        Err(refuse(read))
    }
    fn relation_alive(&self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.relations.Relations_1074.IsAlive_owner"))
    }
    fn relation_at_war(
        &self,
        _player: ObjectHandle,
        _faction: u32,
        read: &'static str,
    ) -> Result<bool> {
        Err(refuse(read))
    }
    fn relation_reputation_rank(&self, _player: ObjectHandle, _faction: u32) -> Result<u32> {
        Err(refuse("execution.relations.Relations_179.GetRank_owner"))
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

pub(super) struct SourcePlayerRelations<'a> {
    pub live: SourceRelations<'a>,
    pub players: &'a [&'a SourcePlayer],
}

impl SourceRelationOwner for SourcePlayerRelations<'_> {
    fn relation_flags(&self, unit: ObjectHandle) -> Result<u32> {
        self.live.relation_flags(unit)
    }

    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.live.relation_master(unit)
    }

    fn relation_faction(&self, unit: ObjectHandle) -> Result<Option<&SourceFactionTemplate>> {
        self.live.relation_faction(unit)
    }

    fn relation_persuaded(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        self.live.relation_persuaded(unit)
    }

    fn relation_has_reputation(&self, faction: u32) -> Result<bool> {
        self.live.relation_has_reputation(faction)
    }

    fn relation_kind(&self, unit: ObjectHandle) -> Result<SourceObjectKind> {
        Ok(self.live.objects.get(unit)?.kind)
    }

    fn relation_alive(&self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.live.objects.get(unit)?.alive)
    }

    fn relation_player_flags(&self, player: ObjectHandle, read: &'static str) -> Result<u32> {
        Ok(live_player(self.players, player, read)?.player_flags())
    }

    fn relation_controlling_player(
        &self,
        unit: ObjectHandle,
        read: &'static str,
    ) -> Result<Option<ObjectHandle>> {
        // Relations.cpp:72-90 resolves a nonempty master before the self case.
        let object = self.live.objects.get(unit)?;
        let master = self
            .live
            .units
            .get(self.live.objects, unit)?
            .control_guids
            .master_guid();
        if master.raw() != 0 {
            let map = object.map.ok_or_else(|| refuse(read))?;
            if let Some(master) = self.live.objects.lookup_unit(map, master) {
                return Ok(
                    (self.relation_kind(master)? == SourceObjectKind::Player).then_some(master)
                );
            }
            return Ok(None);
        }
        Ok((object.kind == SourceObjectKind::Player).then_some(unit))
    }

    fn relation_forced_rank(
        &self,
        player: ObjectHandle,
        _faction: u32,
        read: &'static str,
    ) -> Result<Option<u32>> {
        // ReputationMgr.cpp:128-129. The forced-reaction writer is not bound.
        require(
            live_player(self.players, player, read)?
                .forced_reactions
                .is_empty(),
            read,
        )?;
        Ok(None)
    }
}

/// Live relation operands available while a cast borrows its caster.
pub struct SourceCasterRelations<'a> {
    pub caster: ObjectHandle,
    pub objects: &'a SourceObjects,
    pub flags: &'a u32,
    pub control_guids: &'a SourceControlGuids,
}
impl SourceRelationOwner for SourceCasterRelations<'_> {
    fn relation_flags(&self, unit: ObjectHandle) -> Result<u32> {
        require(
            unit == self.caster,
            "execution.relations.cast_other_unit_flags_owner",
        )?;
        Ok(*self.flags)
    }
    fn relation_master(&self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        require(
            unit == self.caster,
            "execution.relations.Unit_6582.cast_other_unit_fields_owner",
        )?;
        Ok(self
            .control_guids
            .get_master(self.objects, unit, None, unbound_pet_loading)?
            .unit())
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
    /// Unit.cpp:6496-6520. Check controlling GUID type without resolving the charmer.
    pub(crate) fn is_client_controlled(
        &self,
        flags: u32,
        unit: &SourceObject,
        exact_client: Option<&SourceObject>,
    ) -> bool {
        if flags & 0x0000_0008 == 0 || flags & 0x00c0_0004 != 0 {
            return false;
        }
        if self.charmer.raw() != 0 {
            return flags & 0x0100_0000 != 0
                && self.charmer.is_player()
                && exact_client.is_none_or(|client| client.guid == Some(self.charmer));
        }
        unit.kind == SourceObjectKind::Player
            && exact_client.is_none_or(|client| client.handle == unit.handle)
    }

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

// Relations.cpp:154-187. Player overrides precede the faction table.
fn unit_faction_reaction(
    this: &SourceFactionTemplate,
    other: &SourceFactionTemplate,
    target: ObjectHandle,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<u32> {
    if owner.relation_flags(target)? & 8 != 0 {
        if let Some(player) = owner.relation_controlling_player(
            target,
            "execution.relations.GetFactionReaction_166_GetControllingPlayer_owner",
        )? {
            let flags = owner.relation_player_flags(
                player,
                "execution.relations.Relations_169.PLAYER_FLAGS_owner",
            )?;
            if flags & 0x100 != 0 && this.flags & 0x1000 != 0 {
                return Ok(1);
            }
            if let Some(rank) = owner.relation_forced_rank(
                player,
                this.faction,
                "execution.relations.Relations_172.GetForcedRankIfAny_owner",
            )? {
                return Ok(rank);
            }
            if owner.relation_has_reputation(this.faction)? {
                return owner.relation_reputation_rank(player, this.faction);
            }
        }
    }
    Ok(this.reaction_to(other))
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
        let player = owner.relation_controlling_player(
            caster,
            "execution.relations.GetReactionTo_208_GetControllingPlayer_owner",
        )?;
        require(
            owner.relation_flags(target)? & 8 == 0,
            "execution.relations.Relations_212.GetControllingPlayer_owner",
        )?;
        // Relations.cpp:233-249. Forced rank precedes the reputation row.
        if let Some(player) = player {
            if let Some(other) = owner.relation_faction(target)? {
                if let Some(rank) = owner.relation_forced_rank(
                    player,
                    other.faction,
                    "execution.relations.Relations_238.GetForcedRankIfAny_owner",
                )? {
                    return Ok(rank);
                }
                if owner.relation_has_reputation(other.faction)? {
                    let flags = owner.relation_player_flags(
                        player,
                        "execution.relations.Relations_246.PLAYER_FLAGS_owner",
                    )?;
                    if flags & 0x100 != 0 && other.flags & 0x1000 != 0 {
                        return Ok(1);
                    }
                    return Ok(
                        if owner.relation_at_war(
                            player,
                            other.faction,
                            "execution.relations.Relations_249.IsAtWar_owner",
                        )? {
                            1
                        } else {
                            4
                        },
                    );
                }
            }
        }
    }
    let mut reaction = 3;
    if let Some(this) = owner.relation_faction(caster)? {
        if let Some(other) = owner.relation_faction(target)? {
            reaction = unit_faction_reaction(this, other, target, owner)?;
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

/// Unit::CanAttack, Relations.cpp:351-416. The direction matters.
pub fn unit_can_attack(
    source: ObjectHandle,
    target: ObjectHandle,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<bool> {
    unit_can_attack_checked(source, target, None, owner)
}

// Relations.cpp:351-416 and 1206-1278 share the same reaction rules.
// Only the server path uses the spell's immunity bypass.
fn unit_can_attack_checked(
    source: ObjectHandle,
    target: ObjectHandle,
    server_ignore: Option<bool>,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<bool> {
    // TYPEID_UNIT includes pets. Player flags precede the Unit flag reads.
    if matches!(
        owner.relation_kind(source)?,
        SourceObjectKind::Creature | SourceObjectKind::Pet
    ) && owner.relation_kind(target)? == SourceObjectKind::Player
        && owner.relation_player_flags(
            target,
            if server_ignore.is_some() {
                "execution.relations.Relations_1213.PLAYER_FLAGS_owner"
            } else {
                "execution.relations.Relations_358.PLAYER_FLAGS_owner"
            },
        )? & 0x10
            != 0
        && owner.relation_creature_type_flags(source)? & 0x0020_0000 == 0
    {
        return Ok(false);
    }
    let target_flags = owner.relation_flags(target)?;
    if target_flags & 0x0211_0082 != 0 {
        return Ok(false);
    }
    let source_flags = owner.relation_flags(source)?;
    let source_controlled = source_flags & 8 != 0;
    let target_controlled = target_flags & 8 != 0;
    if !server_ignore.unwrap_or(false)
        && (target_flags & (if source_controlled { 0x100 } else { 0x200 }) != 0
            || source_flags & (if target_controlled { 0x100 } else { 0x200 }) != 0)
    {
        return Ok(false);
    }
    if source_controlled || target_controlled {
        if unit_reaction_to(source, target, owner)? > 3 {
            return Ok(false);
        }
        if source_controlled && target_controlled {
            return Err(refuse(
                "execution.relations.Relations_395.GetControllingPlayer_owner",
            ));
        }
        return Ok(true);
    }
    Ok(
        unit_reaction_to(source, target, owner)? < 2
            || unit_reaction_to(target, source, owner)? < 2,
    )
}

// Relations.cpp:1065-1120,1172-1195. Spell targeting adds the dead-target
// guard and the server-side at-war fallback.
pub(super) fn unit_can_attack_spell(
    source: ObjectHandle,
    target: ObjectHandle,
    attrs_ex2: u32,
    attrs_ex3: u32,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<bool> {
    if !owner.relation_alive(target)? && attrs_ex2 & 1 == 0 {
        return Ok(false);
    }
    let ignore = attrs_ex3 & 0x1000_0000 != 0;
    if !unit_can_attack_checked(source, target, Some(ignore), owner)?
        && (owner.relation_flags(target)? & 8 == 0
            || unit_reaction_to(source, target, owner)? < 4
            || !target_at_war(
                source,
                target,
                "execution.relations.Relations_1181.GetControllingPlayer_owner",
                "execution.relations.Relations_1186.IsAtWar_owner",
                owner,
            )?
            .unwrap_or(false))
    {
        return Ok(false);
    }
    // Spell.cpp:5085 uses the non-AOE call at this entry.
    if owner.relation_flags(target)? & 8 != 0
        && owner.relation_flags(source)? & 8 == 0
        && unit_reaction_to(source, target, owner)? >= 2
    {
        if let Some(at_war) = target_at_war(
            source,
            target,
            "execution.relations.Relations_1106.GetControllingPlayer_owner",
            "execution.relations.Relations_1112.IsAtWar_owner",
            owner,
        )? {
            return Ok(at_war);
        }
    }
    Ok(true)
}

// Relations.cpp:1106-1112,1181-1187. A missing player or faction leaves
// the caller's original attackability result untouched.
fn target_at_war(
    source: ObjectHandle,
    target: ObjectHandle,
    player_read: &'static str,
    war_read: &'static str,
    owner: &(impl SourceRelationOwner + ?Sized),
) -> Result<Option<bool>> {
    let Some(player) = owner.relation_controlling_player(target, player_read)? else {
        return Ok(None);
    };
    let Some(faction) = owner.relation_faction(source)? else {
        return Ok(None);
    };
    if !owner.relation_has_reputation(faction.faction)? {
        return Ok(None);
    }
    Ok(Some(owner.relation_at_war(
        player,
        faction.faction,
        war_read,
    )?))
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
