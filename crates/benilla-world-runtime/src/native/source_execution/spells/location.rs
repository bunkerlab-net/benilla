// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! SpellMgr.cpp:2290-2338,2796-2854. Missing live reads never become false.
use super::*;
use SpellCastResult as R;

pub trait SpellLocationOwner {
    fn condition(&mut self, _player: ObjectHandle, _id: u16) -> Result<bool> {
        Err(refuse(
            "execution.spells.location.IsConditionSatisfied_owner",
        ))
    }
    fn gender(&mut self, _player: ObjectHandle) -> Result<u8> {
        Err(refuse("execution.spells.location.Player_getGender_owner"))
    }
    fn race_mask(&mut self, _player: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.spells.location.Player_getRaceMask_owner"))
    }
    fn active_quest(&mut self, _player: ObjectHandle, _quest: u32) -> Result<bool> {
        Err(refuse(
            "execution.spells.location.Player_IsActiveQuest_owner",
        ))
    }
    fn rewarded_quest(&mut self, _player: ObjectHandle, _quest: u32) -> Result<bool> {
        Err(refuse(
            "execution.spells.location.Player_GetQuestRewardStatus_owner",
        ))
    }
    fn aura(&mut self, _player: ObjectHandle, _spell: u32) -> Result<bool> {
        Err(refuse("execution.spells.location.Player_HasAura_owner"))
    }
    fn in_battleground(&mut self, _player: ObjectHandle) -> Result<bool> {
        Err(refuse(
            "execution.spells.location.Player_InBattleGround_owner",
        ))
    }
    /// GetBattleGround is evaluated before map_id == 30, SpellMgr.cpp:2319.
    fn battleground(&mut self, _player: ObjectHandle) -> Result<bool> {
        Err(refuse(
            "execution.spells.location.Player_GetBattleGround_owner",
        ))
    }
    fn battleground_wait_join(&mut self, _player: ObjectHandle) -> Result<bool> {
        Err(refuse(
            "execution.spells.location.BattleGround_GetStatus_owner",
        ))
    }
    fn map_battleground(&mut self, _map: u32) -> Result<Option<bool>> {
        Err(refuse("execution.spells.location.MapEntry_owner"))
    }
}

impl SpellArea {
    pub fn fits_location(
        &self,
        player: Option<ObjectHandle>,
        zone: u32,
        area: u32,
        owner: &mut (impl SpellLocationOwner + ?Sized),
    ) -> Result<bool> {
        // SpellMgr.cpp:2798-2832. Condition replaces all legacy requirements.
        if self.condition != 0 {
            let Some(player) = player else {
                return Ok(false);
            };
            if !owner.condition(player, self.condition)? {
                return Ok(false);
            }
        } else {
            if self.gender != 2 {
                let Some(player) = player else {
                    return Ok(false);
                };
                if self.gender != owner.gender(player)? {
                    return Ok(false);
                }
            }
            if self.race_mask != 0 {
                let Some(player) = player else {
                    return Ok(false);
                };
                if self.race_mask & owner.race_mask(player)? == 0 {
                    return Ok(false);
                }
            }
            if self.quest_start != 0 {
                let Some(player) = player else {
                    return Ok(false);
                };
                if (!self.quest_start_active || !owner.active_quest(player, self.quest_start)?)
                    && !owner.rewarded_quest(player, self.quest_start)?
                {
                    return Ok(false);
                }
            }
            if self.quest_end != 0 {
                let Some(player) = player else {
                    return Ok(false);
                };
                if owner.rewarded_quest(player, self.quest_end)? {
                    return Ok(false);
                }
            }
        }
        if self.area != 0 && zone != self.area && area != self.area {
            return Ok(false);
        }
        if self.aura != 0 {
            let Some(player) = player else {
                return Ok(false);
            };
            let id = self
                .aura
                .checked_abs()
                .ok_or_else(|| refuse("execution.spells.area_abs_undefined"))?
                as u32;
            let present = owner.aura(player, id)?;
            return Ok(if self.aura > 0 { present } else { !present });
        }
        Ok(true)
    }
}
impl SourceSpellStores {
    pub fn allowed_in_location(
        &self,
        t: &SpellTemplate,
        map: u32,
        zone: u32,
        area: u32,
        player: Option<ObjectHandle>,
        owner: &mut (impl SpellLocationOwner + ?Sized),
    ) -> Result<R> {
        // SpellMgr.cpp:2292-2302. First matching alternative returns before BG checks.
        let bounds = self.areas(t.id)?;
        if !bounds.is_empty() {
            for row in bounds {
                if row.fits_location(player, zone, area, owner)? {
                    return Ok(R::Ok);
                }
            }
            return Ok(R::RequiresArea);
        }
        // SpellDefines.h:149; SpellMgr.cpp:2307-2309.
        if t.attributesex3 & 0x800 != 0 {
            let Some(player) = player else {
                return Ok(R::OnlyBattlegrounds);
            };
            if !owner.in_battleground(player)? {
                return Ok(R::OnlyBattlegrounds);
            }
        }
        match t.id {
            22563 | 22564 => {
                let Some(player) = player else {
                    return Ok(R::RequiresArea);
                };
                let bg = owner.battleground(player)?;
                Ok(
                    if map == 30 && bg && !owner.battleground_wait_join(player)? {
                        R::Ok
                    } else {
                        R::RequiresArea
                    },
                )
            }
            2584 => Ok(if let Some(player) = player {
                if owner.in_battleground(player)? {
                    R::Ok
                } else {
                    R::OnlyBattlegrounds
                }
            } else {
                R::OnlyBattlegrounds
            }),
            22011 | 22012 | 24171 => Ok(match owner.map_battleground(map)? {
                None => R::RequiresArea,
                Some(true) => R::Ok,
                Some(false) => R::OnlyBattlegrounds,
            }),
            _ => Ok(R::Ok),
        }
    }
}

/// Spell.cpp:5100-5101, Unit.cpp:6482-6493,6714-6717. No cached beneficiary.
pub fn spell_beneficiary_player(
    objects: &SourceObjects,
    units: &SourceUnits,
    caster: Option<ObjectHandle>,
) -> Result<Option<ObjectHandle>> {
    let Some(caster) = caster else {
        return Ok(None);
    };
    let object = objects.get(caster)?;
    let fields = units.get(objects, caster)?;
    if fields.charmer.is_some() {
        return Err(refuse("execution.spells.location.Unit_GetCharmer_owner"));
    }
    if fields.owner.is_some() {
        return Err(refuse("execution.spells.location.Unit_GetOwner_owner"));
    }
    Ok((object.kind == SourceObjectKind::Player).then_some(caster))
}

/// Borrowed real loaded/live owners; no fabricated location or player identity.
pub struct SpellLocationContext<'a> {
    pub stores: &'a SourceSpellStores,
    pub objects: &'a SourceObjects,
    pub units: &'a SourceUnits,
    pub owner: &'a mut dyn SpellLocationOwner,
}
impl SpellLocationContext<'_> {
    pub fn check(
        &mut self,
        t: &SpellTemplate,
        caster: ObjectHandle,
        location: [u32; 2],
    ) -> Result<R> {
        let object = self.objects.get(caster)?;
        let map = object
            .map
            .ok_or_else(|| refuse("execution.spells.location.WorldObject_GetMapId_owner"))?
            .map;
        let unit = matches!(
            object.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet | SourceObjectKind::Player
        )
        .then_some(caster);
        let player = spell_beneficiary_player(self.objects, self.units, unit)?;
        self.stores
            .allowed_in_location(t, map, location[0], location[1], player, self.owner)
    }
}
