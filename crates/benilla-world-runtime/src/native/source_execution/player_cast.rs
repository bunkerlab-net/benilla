// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::super::relations::SourcePlayerRelations;
use super::*;
use benilla_protocol::wire::{read_packed_guid, read_u16_le, read_u32_le};

/// Live external operands of the detached creation casts.
pub struct SourcePlayerCastInputs<'a> {
    pub targets: &'a SourceSpellTargetMgr,
    pub terrain: &'a mut SourceTerrainManager,
    pub map_time: &'a dyn Fn(MapIdentity) -> Result<u32>,
    /// SpellAuras.cpp:5174,270 read wall seconds independently.
    pub aura_seconds: &'a mut dyn FnMut(SourcePlayerAuraClock) -> Result<i64>,
    /// Object.cpp:2955-2957 draws from the same thread as the Player constructor.
    pub irand: &'a mut dyn FnMut(i32, i32) -> Result<i32>,
    pub session: &'a SourceWorldSession,
    pub max_casts: u32,
    pub indoor_policy: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourcePlayerAuraClock {
    Holder,
    Effect,
}

// SpellDefines.h:397,400,402,406,410,413.
pub(in crate::native::source_execution) const MOD_RADIUS: u32 = 6;
pub(in crate::native::source_execution) const MOD_JUMP_TARGETS: u32 = 17;
const MOD_DURATION: u32 = 1;
const MOD_CHARGES: u32 = 4;
const MOD_CASTING_TIME: u32 = 10;
const MOD_COST: u32 = 14;

// PlayerDefines.h:64,66-67. The writer stops at SpellAuras.cpp:937.
pub(super) struct PlayerSpellModifier {
    spell: u32,
    mask: u64,
    charges: i16,
}

pub(in crate::native::source_execution) fn spell_mod_player<'a>(
    player: Option<&'a SourcePlayer>,
    owner: ObjectHandle,
    refusal: &str,
) -> Result<&'a SourcePlayer> {
    player
        .filter(|p| p.object == owner)
        .ok_or_else(|| refuse(refusal))
}

impl SourcePlayer {
    // Player.h:2577-2631. The loaded template is the successful LookupEntry.
    pub(in crate::native::source_execution) fn apply_spell_mod<T: Default + PartialOrd>(
        &self,
        stores: &SourceSpellStores,
        t: &SpellTemplate,
        op: u32,
        base: T,
    ) -> Result<T> {
        if t.spellfamilyname != self.spell_family || t.attributesex3 & 0x2000_0000 != 0 {
            return Ok(base);
        }
        for modifier in self.spell_mods.get(&op).into_iter().flatten() {
            if matches!(op, MOD_CASTING_TIME | MOD_COST) && base <= T::default() {
                break;
            }
            // Player.cpp:16828-16838,350-356. Active casts may retain an expired mod.
            require(
                modifier.charges != -1 || self.active_mod_spell.is_none(),
                "execution.spells.Player_16833.consumed_spell_mods_owner",
            )?;
            if stores.template(modifier.spell).is_some_and(|affect| {
                affect.spellfamilyname == t.spellfamilyname
                    && t.spellfamilyflags & modifier.mask != 0
            }) {
                return Err(refuse(
                    "execution.spells.Player_2592.spell_modifier_value_owner",
                ));
            }
        }
        Ok(base)
    }

    /// SpellHandler.cpp:311-391. Dispatch the client's explicit unit cast.
    pub fn client_unit_cast(
        &mut self,
        mut input: &[u8],
        objects: &SourceObjects,
        units: &mut SourceUnits,
        stores: &SourceSpellStores,
        inputs: &mut SourcePlayerCastInputs<'_>,
        map: SourceClientCastMap<'_>,
    ) -> Result<Option<SpellCastResult>> {
        let id = read_u32_le(&mut input)
            .map_err(|_| refuse("execution.spells.SpellHandler_314.spell_id_packet_owner"))?;
        let mover = objects.get(self.mover)?;
        if self.mover != self.object && mover.kind == SourceObjectKind::Player {
            return Ok(None);
        }
        let Some(template) = stores.template(id) else {
            return Ok(None);
        };
        require(
            self.mover == self.object,
            "execution.spells.SpellHandler_351.mover_spellbook_owner",
        )?;
        let active = self.spellbook().get(&id).is_some_and(|row| {
            row.state != SourcePlayerSpellState::Removed && row.active && !row.disabled
        });
        if !active || template.attributes & 0x40 != 0 {
            return Ok(None);
        }
        let mask = read_u16_le(&mut input)
            .map_err(|_| refuse("execution.spells.Spell_239.target_mask_packet_owner"))?;
        require(
            mask == 2,
            "execution.spells.Spell_250.client_target_mask_owner",
        )?;
        let guid = read_packed_guid(&mut input)
            .map_err(|_| refuse("execution.spells.Spell_251.packed_target_guid_owner"))?;
        let identity = mover
            .map
            .ok_or_else(|| refuse("execution.spells.Spell_214.GetMap_owner"))?;
        // Spell.cpp:210-215. A self GUID resolves without a map lookup.
        let target = if mover.guid.is_some_and(|value| value.raw() == guid) {
            Some(self.object)
        } else {
            objects.lookup_unit(identity, SourceGuid::from_raw(guid))
        };
        if let Some(target) = target {
            require_same_aura_rank(template, PlayerCast::level(units.get(objects, target)?)?)?;
        }
        // SpellMgr.h:1028-1039 and SpellHandler.cpp:381-387.
        let effects = template.effects();
        require(
            is_client_unit_target(effects[0].target_a)
                || !(effects[1..]
                    .iter()
                    .any(|effect| is_client_unit_target(effect.target_a))
                    || effects
                        .iter()
                        .any(|effect| is_client_unit_target(effect.target_b))),
            "execution.spells.SpellHandler_382.GetTarget_owner",
        )?;
        if template.attributes & 2 != 0 && template.attributesex2 & 0x20 != 0 {
            let live = units.get(objects, self.object)?;
            if let Some(id) = live.spell_slots.current(SpellSlot::AutoRepeat) {
                let repeated = live.spell_starts.spells.get(&id).ok_or_else(|| {
                    refuse("execution.spells.SpellHandler_385.GetCurrentSpell_owner")
                })?;
                if repeated.spell == template.id {
                    return Ok(None);
                }
            }
        }
        let targets = SpellTargets {
            unit: target,
            mask: u32::from(mask),
            ..Default::default()
        };
        let caster = self.object;
        let mut owner = PlayerCast {
            player: self,
            objects,
            units,
            stores,
            skills: None,
            inputs,
            client_map: Some(map),
        };
        SpellStartContext {
            stores,
            storage: SpellStartStorage::Unit,
            owner: &mut owner,
        }
        .start_client_spell(caster, template, targets)
        .map(Some)
    }

    // Player.cpp:3400; Unit.cpp:1469-1512. The spellbook row is already live.
    pub(super) fn cast_granted_passive(
        &mut self,
        live: &mut PlayerLearning<'_, '_>,
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
            skills: Some(live.skills),
            inputs,
            client_map: None,
        };
        SpellStartContext {
            stores: live.spells,
            storage: SpellStartStorage::Unit,
            owner: &mut owner,
        }
        .cast_triggered(caster, Some(caster), id, None)?;
        Ok(())
    }

    // SpellEffects.cpp:2311-2329. The dispatcher has checked the caster type.
    fn effect_proficiency(
        &mut self,
        t: &SpellTemplate,
        output: impl FnOnce(u16, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let (kind, mask) = match t.equippeditemclass {
            2 => (2, &mut self.weapon_proficiency),
            4 => (4, &mut self.armor_proficiency),
            _ => return Ok(()),
        };
        let subclass = t.equippeditemsubclassmask as u32;
        if *mask & subclass != 0 {
            return Ok(());
        }
        *mask |= subclass;
        Self::send_proficiency(kind, *mask, output)
    }

    // Player.cpp:16973-16977; Opcodes.h:333.
    fn send_proficiency(
        kind: u8,
        mask: u32,
        output: impl FnOnce(u16, &[u8]) -> Result<()>,
    ) -> Result<()> {
        let [a, b, c, d] = mask.to_le_bytes();
        output(0x127, &[kind, a, b, c, d])
    }
}

struct PlayerCast<'a, 'b> {
    player: &'a mut SourcePlayer,
    objects: &'a SourceObjects,
    units: &'a mut SourceUnits,
    stores: &'a SourceSpellStores,
    skills: Option<&'a player_skills::PlayerSkillInputs>,
    inputs: &'a mut SourcePlayerCastInputs<'b>,
    client_map: Option<SourceClientCastMap<'a>>,
}

// SpellMgr.h:1011-1025. Only these selectors read the client's explicit unit.
fn is_client_unit_target(mode: u32) -> bool {
    matches!(mode, 6 | 21 | 25 | 35 | 37 | 45 | 53 | 57)
}

// SpellMgr.cpp:1169-1175. Rank selection below the fast case is not bound.
fn require_same_aura_rank(template: &SpellTemplate, level: u32) -> Result<()> {
    require(
        level.wrapping_add(10) >= template.spelllevel || template.attributes & 0x40 != 0,
        "execution.spells.SpellMgr_1181.SelectAuraRankForLevel_owner",
    )
}

impl PlayerCast<'_, '_> {
    // Unit.cpp:11932,11951-11957 reads the live scale and native display.
    fn collision_height(&self, unit: ObjectHandle, models: &SourceCollisionModels) -> Result<f32> {
        let object = self.objects.get(unit)?;
        if object.kind == SourceObjectKind::Player {
            require(
                unit == self.player.object,
                "execution.spells.Unit_11951.player_GetNativeDisplayId_owner",
            )?;
            return models.collision_height(self.player.native_display, self.player.scale);
        }
        let fields = &self.units.get(self.objects, unit)?.loaded_creature;
        let scale = fields
            .scale
            .ok_or_else(|| refuse("execution.spells.Unit_11932.GetObjectScale_owner"))?;
        models.collision_height(fields.native_display, f32::from_bits(scale))
    }
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
    fn level(live: &SourceUnitFields) -> Result<u32> {
        live.power_fields
            .as_ref()
            .map(|power| power.level)
            .ok_or_else(|| refuse("execution.spells.Unit_getLevel_owner"))
    }

    fn aura_operands<'a>(
        objects: &'a SourceObjects,
        target: ObjectHandle,
        level: u32,
        totem: bool,
        control_guids: &'a SourceControlGuids,
    ) -> SelfAuraTarget<'a> {
        SelfAuraTarget {
            target,
            level: Some(level),
            alive: true,
            player: true,
            player_group: Some(false),
            totem,
            objects,
            control_guids,
        }
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
        let client = self.client_map.is_some();
        require(
            if client {
                object.in_world && object.map == Some(self.map()?)
            } else {
                caster == self.player.object && !object.in_world
            },
            if client {
                "execution.spells.Spell_214.live_map_unit_owner"
            } else {
                "execution.player.Player_3400.detached_spell_caster_owner"
            },
        )?;
        let live = self.units.get(self.objects, caster)?;
        let player = object.kind == SourceObjectKind::Player;
        let movement = if client && player {
            live.movement
                .as_ref()
                .and_then(|value| value.flags)
                .ok_or_else(|| refuse("execution.spells.Spell_4743.IsMoving_owner"))?
        } else {
            0
        };
        if player && client {
            require(
                caster == self.player.object && self.player.battleground_instance == 0,
                "execution.spells.Spell_4702.GetBattleGround_owner",
            )?;
        }
        // Unit.cpp:348. Creation has not changed the ALIVE death state.
        Ok(SpellUnitState {
            player,
            gameobject: false,
            alive: object.alive,
            standing: live.is_standing(),
            player_controlled: live.flags & 8 != 0,
            combat: live.flags & 0x80000 != 0,
            battleground_wait_leave: false,
            gm: live.gm,
            // Object.h:739-744 and Unit.h:1785.
            moving: movement & 0x0400_60cf != 0,
            falling_far: movement & 0x4000 != 0,
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

    // Spell.cpp:4850 and Object.cpp:1379-1391. The object overload ignores M2.
    fn line_of_sight(&mut self, caster: ObjectHandle, target: ObjectHandle) -> Result<bool> {
        let origin = self.objects.get(caster)?;
        let victim = self.objects.get(target)?;
        if !origin.in_world || !victim.in_world || origin.map != victim.map {
            return Ok(false);
        }
        let map = self.map()?;
        let map_data = self
            .client_map
            .as_ref()
            .ok_or_else(|| refuse("execution.spells.f6_WorldObject_IsWithinLOSInMap_owner"))?;
        let [x2, y2, z2, _] = self.objects.pose(target)?.0.map(f32::from_bits);
        let end = [
            x2,
            y2,
            z2 + self.collision_height(target, map_data.collision)?,
        ];
        let [x1, y1, z1, _] = self.objects.pose(caster)?.0.map(f32::from_bits);
        let start = [
            x1,
            y1,
            z1 + self.collision_height(caster, map_data.collision)?,
        ];
        let vmap = &self.inputs.terrain.vmap;
        if !vmap.line_of_sight(map.map, start, end, true)? {
            return Ok(false);
        }
        let map_data = self
            .client_map
            .as_mut()
            .ok_or_else(|| refuse("execution.dynamic_collision.spell_map_tree_owner"))?;
        require(
            map_data.dynamic.map() == map,
            "execution.dynamic_collision.spell_map_tree_owner",
        )?;
        map_data
            .dynamic
            .line_of_sight(map_data.models, vmap, start, end, true)
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
            SpellCheck::IsSpellReady | SpellCheck::HasGcd if self.client_map.is_some() => {
                let map_data = self
                    .client_map
                    .as_ref()
                    .ok_or_else(|| check.missing_owner())?;
                let now = || (map_data.clock_millis)(self.map()?);
                let ready = if check == SpellCheck::IsSpellReady {
                    live.cooldowns.ready_at(t, spell.item_cast, now()?)?
                } else {
                    !live
                        .cooldowns
                        .gcd
                        .has_with_clock(t.startrecoverycategory, now)?
                };
                Ok(if ready { R::Ok } else { R::NotReady })
            }
            SpellCheck::Shapeshift => self.stores.shapeshift_error(t, live.shapeshift_form),
            SpellCheck::LowLevelRank if self.client_map.is_some() => {
                let target = spell.targets.unit.ok_or_else(|| check.missing_owner())?;
                require_same_aura_rank(t, Self::level(self.units.get(self.objects, target)?)?)?;
                Ok(R::Ok)
            }
            SpellCheck::Visible if self.client_map.is_some() => {
                let target = spell.targets.unit.ok_or_else(|| check.missing_owner())?;
                let flags = self.units.get(self.objects, target)?.flags;
                // Spell.cpp:4868-4870. This harmful visibility check is for controlled targets.
                require(
                    flags & 8 == 0
                        || positive_unit_spell(self.stores, t, 7, Some((spell.caster, target)))?,
                    "execution.spells.Spell_4869.IsVisibleForOrDetect_owner",
                )?;
                Ok(R::Ok)
            }
            SpellCheck::TargetFilter(index) if self.client_map.is_some() => {
                let map_data = self
                    .client_map
                    .as_ref()
                    .ok_or_else(|| check.missing_owner())?;
                spell.check_target_filter(
                    t,
                    usize::from(index),
                    self.objects,
                    &SourcePlayerRelations {
                        live: SourceRelations {
                            objects: self.objects,
                            units: self.units,
                            factions: map_data.factions,
                        },
                        players: &[self.player],
                    },
                )
            }
            SpellCheck::PositiveImmunity(mask) => {
                let target = spell.targets.unit.unwrap_or(self.player.object);
                let live = self.units.get(self.objects, target)?;
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
            SpellCheck::SpiritOfRedemption => {
                let target = spell.targets.unit.unwrap_or(self.player.object);
                let fields = self.units.get(self.objects, target)?;
                Ok(
                    if fields.auras.has_aura_type(176) && t.attributesex3 & 0x0010_0000 == 0 {
                        R::BadTargets
                    } else {
                        R::Ok
                    },
                )
            }
            SpellCheck::AllowedInLocation(location) => {
                self.location_context()?.check(t, spell.caster, location)
            }
            SpellCheck::Items => spell.check_items(t, true, self),
            SpellCheck::Range(_) if self.client_map.is_some() => {
                Err(refuse("execution.spells.Spell_5145.CheckRange_owner"))
            }
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
                || {
                    live.player.spell_rank(
                        live.skills.ok_or_else(|| {
                            refuse("execution.spells.Player_5885.GetSpellRank_owner")
                        })?,
                        t,
                    )
                },
                |time| {
                    live.player
                        .apply_spell_mod(live.stores, t, MOD_CASTING_TIME, time)
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
        spell.cast_checked(t, self.stores, None, self, |_, _| Ok(()))
    }
}

impl SpellCastEffects for PlayerCast<'_, '_> {
    fn cast_unit(&mut self) -> Result<SpellUnitState> {
        self.unit(self.player.object)
    }

    fn cast_auras(&mut self) -> Result<&mut SourceUnitAuras> {
        Ok(&mut self.live_mut()?.auras)
    }

    fn finish_operands(&mut self) -> Result<SpellFinishOperands<'_>> {
        Ok(SpellFinishOperands::new(
            self.player.object,
            self.objects,
            self.units,
            self.stores,
            self.inputs.terrain.pathfinder_map_types.as_ref(),
        ))
    }

    fn damage_multiplier_jump_targets(
        &self,
        player: ObjectHandle,
        t: &SpellTemplate,
        count: u32,
    ) -> Result<u32> {
        spell_mod_player(
            Some(self.player),
            player,
            "execution.spells.Spell_1670.ApplySpellMod_jump_owner",
        )?
        .apply_spell_mod(self.stores, t, MOD_JUMP_TARGETS, count)
    }

    fn duration(&self, t: &SpellTemplate) -> Result<i32> {
        // SpellMgr.cpp:84-85. Unit.cpp:432 starts the detached player's combo count at zero.
        let duration = creature_self_aura_duration(self.stores, t)?;
        if duration != -1 && t.attributesex3 & 0x2000_0000 == 0 {
            return self
                .player
                .apply_spell_mod(self.stores, t, MOD_DURATION, duration);
        }
        Ok(duration)
    }

    fn fill_self_targets(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        let unit = self.cast_unit()?;
        let live = self.live()?;
        SelfSpellTargeting {
            stores: self.stores,
            targets: self.inputs.targets,
            objects: self.objects,
            unit: &unit,
            level: Self::level(live)?,
            immunities: &live.immunities,
            auras: &live.auras,
            control_guids: &live.control_guids,
            mod_player: Some(self.player),
            relations: &SourceCasterRelations {
                caster: spell.caster,
                objects: self.objects,
                flags: &live.flags,
                control_guids: &live.control_guids,
            },
        }
        .fill(spell, t, |_| {})
    }

    fn send_cooldown(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        require(
            skips_spell_cooldown(t, spell.flags.channel_only()),
            "execution.spells.Player_20196.AddCooldown_owner",
        )
    }

    fn send_spell_go(&mut self, _: &mut SourceSpell, _: &SpellTemplate) -> Result<()> {
        Err(refuse(
            "execution.spells.Spell_3983.detached_spell_go_owner",
        ))
    }

    fn remove_cast_auras(&mut self, _: &SpellTemplate, _: ObjectHandle, _: u32) -> Result<()> {
        Err(refuse(
            "execution.spells.Spell_3288.RemoveAurasOnCast_owner",
        ))
    }

    fn hit_self(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<()> {
        spell.hit_self_auras(t, self.stores, self)
    }

    fn proc_cast_end(&mut self, spell: &mut SourceSpell) -> Result<()> {
        let live = self.units.get_mut(self.objects, self.player.object)?;
        live.auras.proc_bound_cast_end(
            self.stores,
            spell,
            self.objects,
            live.stand_state,
            true,
            |_, _| {},
        )
    }
}

impl SpellAuraHitOwner for PlayerCast<'_, '_> {
    fn effect_dice(&mut self, low: i32, high: i32) -> Result<i32> {
        (self.inputs.irand)(low, high)
    }

    fn effect_proficiency(&mut self, t: &SpellTemplate) -> Result<()> {
        let session = self.inputs.session;
        self.player.effect_proficiency(t, |opcode, body| {
            session.send_packet(opcode, body, false, |_, _| {
                Err(refuse(
                    "execution.session.WorldSession_252.packet_output_owner",
                ))
            })
        })
    }

    fn targetless_proc(&mut self, spell: &SourceSpell, t: &SpellTemplate, mask: u8) -> Result<()> {
        let live = self.units.get_mut(self.objects, self.player.object)?;
        live.auras.proc_bound_targetless(
            self.stores,
            spell,
            t,
            (self.objects, live.stand_state, true),
            mask,
        )
    }

    fn aura_holder_seconds(&mut self, _: &SourceSpell) -> Result<i64> {
        (self.inputs.aura_seconds)(SourcePlayerAuraClock::Holder)
    }

    fn aura_holder_map_ms(&mut self, spell: &SourceSpell) -> Result<u32> {
        self.map_time(spell.caster)
    }

    fn aura_effect_seconds(&mut self, _: &SourceSpell) -> Result<i64> {
        (self.inputs.aura_seconds)(SourcePlayerAuraClock::Effect)
    }

    fn aura_holder_charges(
        &self,
        player: ObjectHandle,
        t: &SpellTemplate,
        charges: u32,
    ) -> Result<u32> {
        spell_mod_player(
            Some(self.player),
            player,
            "execution.spells.SpellAuraHolder_5193.ApplySpellMod_charges_owner",
        )?
        .apply_spell_mod(self.stores, t, MOD_CHARGES, charges)
    }

    fn aura_target(&self, target: ObjectHandle) -> Result<SelfAuraTarget<'_>> {
        require(
            target == self.player.object,
            "execution.spells.Spell_1445.aura_target_owner",
        )?;
        let live = self.live()?;
        Ok(Self::aura_operands(
            self.objects,
            target,
            Self::level(live)?,
            live.totem,
            &live.control_guids,
        ))
    }

    fn aura_hit_target(&mut self, target: ObjectHandle) -> Result<SelfAuraHitTarget<'_>> {
        require(
            target == self.player.object,
            "execution.spells.Spell_1490.AddSpellAuraHolder_target_owner",
        )?;
        let live = self.units.get_mut(self.objects, target)?;
        Ok(SelfAuraHitTarget {
            unit: Self::aura_operands(
                self.objects,
                target,
                Self::level(live)?,
                live.totem,
                &live.control_guids,
            ),
            stand: live.stand_state,
            auras: &mut live.auras,
            visibility: &mut live.visibility,
            damage: DamageAuraTarget::new(
                &mut live.creature_damage,
                &live.loaded_creature,
                live.flags,
                live.shapeshift_form,
            ),
        })
    }

    fn apply_inserted_aura_holder(
        &mut self,
        t: &SpellTemplate,
        holder: SourceAuraHolderId,
    ) -> Result<()> {
        let stores = self.stores;
        apply_self_aura_holder(self, stores, t, holder)
    }

    fn proc_aura_hit(&mut self, spell: &mut SourceSpell) -> Result<()> {
        let live = self.units.get_mut(self.objects, spell.caster)?;
        live.auras.proc_bound_self_hit(
            self.stores,
            spell,
            true,
            (self.objects, live.stand_state, true),
            |_, _| {},
        )
    }
}

impl SelfAuraApplicationOwner for PlayerCast<'_, '_> {
    fn modify_player_skill_bonus(
        &mut self,
        target: ObjectHandle,
        skill: u16,
        delta: i16,
        permanent: bool,
    ) -> Result<()> {
        require(
            target == self.player.object,
            "execution.spells.SpellAuras_2072.ModifySkillBonus_owner",
        )?;
        let unit = self.units.get_mut(self.objects, target)?;
        self.player
            .modify_skill_bonus(self.objects, unit, skill, delta, permanent)
    }

    fn apply_stat_percent_buff_mod(&mut self, stat: usize, amount: f32, apply: bool) -> Result<()> {
        self.player
            .apply_stat_percent_buff_mod(self.objects, stat, amount, apply)
    }

    fn self_aura_application(&mut self) -> Result<SelfAuraApplication<'_>> {
        let target = self.player.object;
        let live = self.units.get_mut(self.objects, target)?;
        Ok(SelfAuraApplication {
            unit: Self::aura_operands(
                self.objects,
                target,
                Self::level(live)?,
                live.totem,
                &live.control_guids,
            ),
            auras: &mut live.auras,
            visibility: &mut live.visibility,
            resistances: &mut live.resistances,
            stat_modifiers: &mut live.stat_modifiers,
            damage: DamageAuraTarget::new(
                &mut live.creature_damage,
                &live.loaded_creature,
                live.flags,
                live.shapeshift_form,
            ),
            can_modify_stats: live.loaded_creature.can_modify_stats,
            threat_modifier: &mut live.threat_modifier,
            power_fields: &mut live.power_fields,
            power_type: live.power_type.as_mut(),
            vitals: &mut live.vitals,
            form: &mut live.shapeshift_form,
        })
    }
}

#[test]
fn detached_proficiency_ors_new_bits_and_builds_the_packet() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::tests::construct(&mut objects, &mut units);
    assert_eq!(
        (player.weapon_proficiency, player.armor_proficiency),
        (0, 0)
    );
    let mut sent = Vec::new();
    for (kind, mask) in [(2, 0x400), (2, 0x8400), (4, 1), (4, 2), (5, 8)] {
        let t = SpellTemplate {
            equippeditemclass: kind,
            equippeditemsubclassmask: mask,
            ..Default::default()
        };
        player
            .effect_proficiency(&t, |opcode, body| {
                sent.push((opcode, <[u8; 5]>::try_from(body).unwrap()));
                Ok(())
            })
            .unwrap();
    }
    assert_eq!(
        (player.weapon_proficiency, player.armor_proficiency),
        (0x400, 3)
    );
    assert_eq!(
        sent,
        [
            (0x127, [2, 0, 4, 0, 0]),
            (0x127, [4, 1, 0, 0, 0]),
            (0x127, [4, 3, 0, 0, 0])
        ]
    );
}

#[test]
fn detached_cast_binds_empty_duration_mods_and_refuses_cooldown_and_delivery() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = SourcePlayer::construct(
        &mut objects,
        &mut units,
        12,
        900_000,
        |_, _| Ok(477_818),
        || Ok(0),
    )
    .unwrap();
    let caster = player.object;
    let mut stores = SourceSpellStores::default();
    stores.bind_test_dbc("SpellDuration.dbc", []);
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let mut terrain = SourceTerrainManager::new(Default::default(), Default::default());
    let session = SourceWorldSession::construct(19, true);
    let mut inputs = SourcePlayerCastInputs {
        targets: &targets,
        terrain: &mut terrain,
        map_time: &|_| Ok(0),
        aura_seconds: &mut |_| panic!("owner guard does not read aura clocks"),
        irand: &mut |_, _| panic!("owner guard does not draw dice"),
        session: &session,
        max_casts: 20,
        indoor_policy: false,
    };
    let mut cast = PlayerCast {
        player: &mut player,
        objects: &objects,
        units: &mut units,
        stores: &stores,
        skills: Some(&Default::default()),
        inputs: &mut inputs,
        client_map: None,
    };
    let t = SpellTemplate::default();
    let mut spell = SourceSpell::new(1, &t, caster, SpellTriggerFlags(0), 0).unwrap();
    // SpellMgr.cpp:92 reaches Player.h:2584 for this matching family.
    assert_eq!(cast.duration(&t).unwrap(), 0);
    assert_eq!(
        cast.send_cooldown(&mut spell, &t).unwrap_err().id,
        "execution.spells.Player_20196.AddCooldown_owner"
    );
    assert_eq!(
        cast.send_spell_go(&mut spell, &t).unwrap_err().id,
        "execution.spells.Spell_3983.detached_spell_go_owner"
    );
    assert_eq!(
        cast.remove_cast_auras(&t, caster, 0x10000).unwrap_err().id,
        "execution.spells.Spell_3288.RemoveAurasOnCast_owner"
    );
}

#[test]
fn apply_spell_mod_checks_the_client_gate_and_the_selected_operation() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::tests::construct(&mut objects, &mut units);
    player.spell_family = 3;
    let t = SpellTemplate {
        id: 17,
        spellfamilyname: 3,
        spellfamilyflags: 1,
        ..Default::default()
    };
    let mut stores = SourceSpellStores::default();
    stores.insert_template(t.clone()).unwrap();
    let radius = |player: &SourcePlayer, t: &SpellTemplate| {
        player.apply_spell_mod(&stores, t, MOD_RADIUS, 12.5)
    };
    assert_eq!(radius(&player, &t).unwrap(), 12.5);
    for op in [MOD_RADIUS, MOD_CASTING_TIME, MOD_COST] {
        player.spell_mods.insert(
            op,
            vec![PlayerSpellModifier {
                spell: t.id,
                mask: 1,
                charges: 0,
            }],
        );
    }
    assert_eq!(
        player
            .apply_spell_mod(&stores, &t, MOD_JUMP_TARGETS, 2)
            .unwrap(),
        2
    );
    for op in [MOD_CASTING_TIME, MOD_COST] {
        assert_eq!(player.apply_spell_mod(&stores, &t, op, 0).unwrap(), 0);
    }
    assert_eq!(
        radius(&player, &t).unwrap_err().id,
        "execution.spells.Player_2592.spell_modifier_value_owner"
    );
    for t in [
        SpellTemplate {
            spellfamilyname: 0,
            ..t.clone()
        },
        SpellTemplate {
            attributesex3: 0x2000_0000,
            ..t
        },
    ] {
        assert_eq!(radius(&player, &t).unwrap(), 12.5);
    }
}

#[test]
fn client_fireball_admission_retains_the_queued_spell_at_range() {
    use crate::native::production_compiler::VerifiedCatalog;
    use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
    use std::path::Path;

    let Some(root) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP client Fireball admission: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let collision = SourceCollisionModels::load(&mut catalog).unwrap();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut mage = SourcePlayer::construct(
        &mut objects,
        &mut units,
        12,
        900_000,
        |min, _| Ok(min),
        || Ok(0),
    )
    .unwrap();
    let caster = mage.object();
    let victim = objects.construct(12, SourceObjectKind::Creature).unwrap();
    for (unit, guid, x, class, level, faction) in [
        (caster, SourceGuid::from_raw(1), 0.0_f32, 8, 1, 1),
        (
            victim,
            SourceGuid::with_entry(HighGuid::Creature, 6, 79994),
            4.0,
            1,
            2,
            25,
        ),
    ] {
        objects.assign_guid(unit, 0, guid).unwrap();
        objects.bind_map(unit, map, &cells).unwrap();
        objects
            .set_pose(unit, SourcePose([x.to_bits(), 0, 10.0_f32.to_bits(), 0]))
            .unwrap();
        objects.add_to_world(unit).unwrap();
        if unit == victim {
            units
                .register(&objects, victim, SourceUnitFields::constructed_creature())
                .unwrap();
        }
        let fields = units.get_mut(&objects, unit).unwrap();
        fields.unit_class = Some(class);
        fields.power_fields = Some(SourceUnitPowerFields {
            level,
            create_health: 0,
            create_mana: 0,
            auxiliary_current: None,
            auxiliary_maximum: None,
        });
        fields.faction = faction;
    }
    units.get_mut(&objects, caster).unwrap().flags |= 8;
    mage.native_display = 49;
    mage.scale = 1.;
    let creature = &mut units.get_mut(&objects, victim).unwrap().loaded_creature;
    creature.native_display = 10913; // pre-start-classicmangos.sql:421114.
    creature.scale = Some(1.0_f32.to_bits());
    mage.spells.insert(
        133,
        SourcePlayerSpell {
            state: SourcePlayerSpellState::Unchanged,
            active: true,
            dependent: false,
            disabled: false,
        },
    );
    let mut stores = SourceSpellStores::default();
    // pre-start-classicmangos.sql:837896. Fields reached by the admission path.
    stores
        .insert_template(SpellTemplate {
            id: 133,
            school: 2,
            spelllevel: 1,
            attributes: 65536,
            equippeditemclass: -1,
            effect1: 2,
            effect2: 6,
            effectimplicittargeta1: 6,
            effectimplicittargeta2: 6,
            effectapplyauraname2: 3,
            rangeindex: 35,
            dmgclass: 1,
            ..Default::default()
        })
        .unwrap();
    let targets = SourceSpellTargetMgr::initialize(&stores);
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    let mut terrain = SourceTerrainManager::default();
    terrain.files.retain_source_absence(0, 32, 32);
    terrain.dbc.areas.insert(
        87,
        SourceAreaEntry {
            id: 87,
            map: 0,
            zone: 12,
            flag: 0,
        },
    );
    terrain.dbc.linked_zones.insert(0, 87);
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(12, &mut mt, &mut cursor, &mut sink).unwrap();
    terrain.load_terrain(0, &mut random).unwrap();
    let session = SourceWorldSession::construct(1, true);
    let mut inputs = SourcePlayerCastInputs {
        targets: &targets,
        terrain: &mut terrain,
        map_time: &|_| Ok(29558),
        aura_seconds: &mut |_| panic!("cast has not reached aura work"),
        irand: &mut |_, _| panic!("cast has not reached launch"),
        session: &session,
        max_casts: 20,
        indoor_policy: false,
    };
    let mut dynamic = SourceDynamicMap::new(map);
    let models = SourceDynamicModels::default();
    let mut invoke = |player: &mut SourcePlayer, units: &mut SourceUnits, packet: &[u8]| {
        player.client_unit_cast(
            packet,
            &objects,
            units,
            &stores,
            &mut inputs,
            SourceClientCastMap {
                factions: &factions,
                collision: &collision,
                dynamic: &mut dynamic,
                models: &models,
                clock_millis: &|_| Ok(29558),
            },
        )
    };
    // northshire-03 A trace.tsv:75148171 and client-packets.jsonl:371.
    let packet = [0x85, 0, 0, 0, 2, 0, 0xcf, 0x7a, 0x38, 1, 6, 0x30, 0xf1];
    mage.spells.get_mut(&133).unwrap().disabled = true;
    assert_eq!(invoke(&mut mage, &mut units, &packet).unwrap(), None);
    assert!(units
        .get(&objects, caster)
        .unwrap()
        .unit_events
        .inventory()
        .queued
        .is_empty());
    mage.spells.get_mut(&133).unwrap().disabled = false;
    assert_eq!(
        invoke(&mut mage, &mut units, &packet).unwrap_err().id,
        "execution.spells.Spell_5145.CheckRange_owner"
    );
    let live = units.get(&objects, caster).unwrap();
    let spell = &live.spell_starts.spells[&1];
    assert_eq!(spell.targets.unit, Some(victim));
    assert!(spell.client_cast);
    assert_eq!(spell.state, SourceSpellState::Targeting);
    assert_eq!(live.spell_slots.current(SpellSlot::Generic), None);
    // Trace.tsv:75160591-75160593. SpellStart queues before CheckCast.
    assert_eq!(
        spell.event,
        Some(live.unit_events.inventory().queued[0].event_id)
    );
}

#[test]
fn check_cast_uses_false_line_of_sight_before_target_rank() {
    struct BlockedSight(SpellUnitState);
    impl SpellExecutionOwner for BlockedSight {
        fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
            Ok(self.0)
        }
        fn check(
            &mut self,
            _: &mut SourceSpell,
            _: &SpellTemplate,
            check: SpellCheck,
        ) -> Result<SpellCastResult> {
            assert_eq!(check, SpellCheck::IsSpellReady);
            Ok(SpellCastResult::Ok)
        }
        fn indoor_policy(&mut self) -> Result<bool> {
            Ok(false)
        }
        fn line_of_sight(&mut self, caster: ObjectHandle, target: ObjectHandle) -> Result<bool> {
            assert_ne!(caster, target);
            Ok(false)
        }
    }
    let (mut objects, mut units) = (SourceObjects::default(), SourceUnits::default());
    let player = super::tests::construct(&mut objects, &mut units);
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let template = SpellTemplate {
        effect1: 2,
        effectimplicittargeta1: 6,
        ..Default::default()
    };
    let mut spell =
        SourceSpell::new(1, &template, player.object(), SpellTriggerFlags(0), 0).unwrap();
    spell.targets.unit = Some(target);
    spell.targets.mask = 2;
    let state = SpellUnitState {
        player: true,
        gameobject: false,
        alive: true,
        standing: true,
        player_controlled: true,
        combat: false,
        battleground_wait_leave: false,
        gm: false,
        moving: false,
        falling_far: false,
        stealth: false,
        form: 0,
        class: 8,
        combo_target: None,
        mounted: false,
        taxi: false,
        totem: false,
        untargetable: false,
        has_ai: false,
        position: [0; 4],
    };
    // Spell.cpp:4850-4859 returns for a blocked sight line before the rank read.
    assert_eq!(
        spell
            .check_cast(&template, false, &mut BlockedSight(state))
            .unwrap(),
        SpellCastResult::LineOfSight
    );
}
