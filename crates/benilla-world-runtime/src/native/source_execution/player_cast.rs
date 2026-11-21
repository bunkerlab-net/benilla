// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

/// Live external operands of the detached creation casts.
pub struct SourcePlayerCastInputs<'a> {
    pub targets: &'a SourceSpellTargetMgr,
    pub terrain: &'a mut SourceTerrainManager,
    pub map_time: &'a dyn Fn(MapIdentity) -> Result<u32>,
    pub session: &'a SourceWorldSession,
    pub max_casts: u32,
    pub indoor_policy: bool,
}

impl SourcePlayer {
    // Player.cpp:3400; Unit.cpp:1469-1512. The spellbook row is already live.
    pub(super) fn cast_granted_passive(
        &mut self,
        live: &mut PlayerLearning<'_>,
        id: u32,
    ) -> Result<()> {
        let inputs = live
            .cast
            .as_mut()
            .ok_or_else(|| refuse("execution.player.Player_3400.passive_cast_owner"))?;
        let caster = self.object;
        let mut owner = PlayerCast {
            player: self,
            objects: live.objects,
            units: live.units,
            stores: live.spells,
            skills: live.skills,
            inputs,
        };
        SpellStartContext {
            stores: live.spells,
            storage: SpellStartStorage::Unit,
            owner: &mut owner,
        }
        .cast_triggered(caster, Some(caster), id, None)?;
        Ok(())
    }
}

struct PlayerCast<'a, 'b> {
    player: &'a mut SourcePlayer,
    objects: &'a SourceObjects,
    units: &'a mut SourceUnits,
    stores: &'a SourceSpellStores,
    skills: &'a player_skills::PlayerSkillInputs,
    inputs: &'a mut SourcePlayerCastInputs<'b>,
}

impl PlayerCast<'_, '_> {
    fn live(&self) -> Result<&SourceUnitFields> {
        self.units.get(self.objects, self.player.object)
    }
    fn live_mut(&mut self) -> Result<&mut SourceUnitFields> {
        self.units.get_mut(self.objects, self.player.object)
    }
    fn map(&self) -> Result<MapIdentity> {
        self.objects
            .get(self.player.object)?
            .map
            .ok_or_else(|| refuse("execution.spells.WorldObject_GetMap_owner"))
    }
    fn no_family_mod(&self, t: &SpellTemplate, refusal: &str) -> Result<()> {
        // Player.h:2580 returns before consulting the modifier list.
        require(
            t.spellfamilyname != self.player.spell_family || t.attributesex3 & 0x2000_0000 != 0,
            refusal,
        )
    }
}

impl SpellLocationOwner for SourcePlayer {}

impl SpellExecutionOwner for PlayerCast<'_, '_> {
    fn spell_storage(
        &mut self,
        caster: ObjectHandle,
    ) -> Result<(&mut SourceSpellStarts, &mut SourceUnitEvents)> {
        let live = self.units.get_mut(self.objects, caster)?;
        Ok((&mut live.spell_starts, &mut live.unit_events))
    }
    fn active_spell_mod(&mut self, player: ObjectHandle, _: bool) -> Result<&mut Option<u64>> {
        require(
            player == self.player.object,
            "execution.spells.Player_20542.spell_mod_owner",
        )?;
        Ok(&mut self.player.active_mod_spell)
    }
    fn unit(&mut self, caster: ObjectHandle) -> Result<SpellUnitState> {
        let object = self.objects.get(caster)?;
        require(
            caster == self.player.object && !object.in_world,
            "execution.player.Player_3400.detached_spell_caster_owner",
        )?;
        let live = self.live()?;
        // Unit.cpp:348. Creation has not changed the ALIVE death state.
        Ok(SpellUnitState {
            player: true,
            gameobject: false,
            alive: true,
            standing: live.stand_state == 0,
            player_controlled: live.flags & 8 != 0,
            combat: live.flags & 0x80000 != 0,
            battleground_wait_leave: false,
            gm: live.gm,
            moving: false,
            falling_far: false,
            stealth: live.auras.has_aura_type(16),
            form: live.shapeshift_form,
            class: live
                .unit_class
                .ok_or_else(|| refuse("execution.spells.Unit_getClass_owner"))?,
            combo_target: live.combo_target,
            mounted: live.is_mounted(),
            taxi: live.flags & 0x100000 != 0,
            totem: live.totem,
            untargetable: live.flags & 0x0200_0000 != 0,
            has_ai: live.has_ai,
            position: self.objects.pose(caster)?.0,
        })
    }
    fn map_time(&mut self, _caster: ObjectHandle) -> Result<u32> {
        (self.inputs.map_time)(self.map()?)
    }
    fn targeting_data(&mut self, spell: u32) -> Result<[[u32; 2]; 3]> {
        self.inputs
            .targets
            .get_spell_targeting_data(spell)
            .map(|data| {
                data.filtering_scheme
                    .map(|row| row.map(|scheme| scheme as u32))
            })
            .map_err(|error| refuse(&format!("execution.spells.targeting.{error}")))
    }
    fn positive_definition(&mut self, t: &SpellTemplate) -> Result<bool> {
        positive_unit_spell(self.stores, t, 7, None)
    }
    fn terrain_geometry(&mut self) -> Result<(&SourceObjects, &mut SourceTerrainManager)> {
        Ok((self.objects, self.inputs.terrain))
    }
    fn indoor_policy(&mut self) -> Result<bool> {
        Ok(self.inputs.indoor_policy && self.inputs.terrain.vmap.settings()[0])
    }
    fn location_context(&mut self) -> Result<SpellLocationContext<'_>> {
        Ok(SpellLocationContext {
            stores: self.stores,
            objects: self.objects,
            units: self.units,
            owner: self.player,
        })
    }
    fn check(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        check: SpellCheck,
    ) -> Result<SpellCastResult> {
        use SpellCastResult as R;
        let live = self.live()?;
        match check {
            SpellCheck::PositiveImmunity(mask) => {
                let target = self.player.object;
                let positive =
                    positive_unit_spell(self.stores, t, 7, Some((spell.caster, target)))?;
                if positive
                    && mask != 0
                    && live.immunities.immune_to_self_spell(
                        self.stores,
                        t,
                        mask,
                        target,
                        &live.auras,
                    )?
                {
                    Ok(R::TargetAuraState)
                } else {
                    Ok(R::Ok)
                }
            }
            SpellCheck::SpiritOfRedemption
                if live.auras.has_aura_type(176) && t.attributesex3 & 0x0010_0000 == 0 =>
            {
                Ok(R::BadTargets)
            }
            SpellCheck::SpiritOfRedemption => Ok(R::Ok),
            SpellCheck::AllowedInLocation(location) => {
                self.location_context()?.check(t, spell.caster, location)
            }
            SpellCheck::Items => spell.check_items(t, true, self),
            _ => Err(check.missing_owner()),
        }
    }
    fn call(&mut self, spell: &mut SourceSpell, call: SpellCall) -> Result<()> {
        match call {
            SpellCall::PrepareTriggerData => spell.prepare_trigger_data(self.stores),
            SpellCall::SendResult(_)
                if spell.triggered_by_aura.is_some() || self.inputs.session.player_loading =>
            {
                Ok(())
            }
            SpellCall::SendResult(SpellCastResult::Ok) => {
                // Spell.cpp:3844-3847,3896-3899. Build the success packet before the session gate.
                let mut body = [0; 5];
                body[..4].copy_from_slice(&spell.spell.to_le_bytes());
                self.inputs
                    .session
                    .send_packet(0x130, &body, false, |_, _| {
                        Err(refuse(
                            "execution.session.WorldSession_252.packet_output_owner",
                        ))
                    })
            }
            _ => Err(call.missing_owner()),
        }
    }
    fn cast_time(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<i32> {
        let mut time = 0;
        spell.with_spell_mod_scope(Some(self.player.object), true, self, |spell, live| {
            time = fresh_cast_time_for_rank(
                |index| live.stores.dbc("SpellCastTimes.dbc", index),
                spell,
                t,
                || live.player.spell_rank(live.skills, t),
                |time| {
                    live.no_family_mod(t, "execution.spells.SpellMgr_154.ApplySpellMod_owner")?;
                    Ok(time)
                },
                || Ok(live.live()?.cast_speed),
            )?;
            Ok(true)
        })?;
        Ok(time)
    }
    fn cast_scheduled(&mut self, spell: &mut SourceSpell, skip: bool) -> Result<()> {
        spell.cast_self(skip, SelfCastCaller::TriggeredUnit, self.stores, self)
    }
}

impl SpellSelfCastOwner for PlayerCast<'_, '_> {
    fn cast_objects(&self) -> &SourceObjects {
        self.objects
    }
    fn cast_mod_owner(&mut self, caster: ObjectHandle) -> Result<Option<ObjectHandle>> {
        world_object_spell_mod_owner(self.objects, caster, false)
    }
    fn cast_chain(&mut self) -> Result<(&mut SourceUnitAuras, u32)> {
        let max = self.inputs.max_casts;
        Ok((&mut self.live_mut()?.auras, max))
    }
    fn cast_checked(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        spell.checked_cast_prefix(t)?;
        Err(refuse(
            "execution.spells.Spell_3258.CalculateSpellDuration_owner",
        ))
    }
}
