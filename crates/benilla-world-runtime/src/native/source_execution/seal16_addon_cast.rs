// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Creature.cpp:2343, Unit.cpp:1453-1512. Fresh Create precedes AI and AddToWorld.
use super::*;
use rustc_apfloat::{ieee::Single, Float, Round, Status};

pub(super) struct Execution<'a> {
    pub object: ObjectHandle,
    pub objects: &'a SourceObjects,
    pub terrain: &'a mut SourceTerrainManager,
    pub stores: &'a SourceSpellStores,
    pub targets: &'a SourceSpellTargetMgr,
    pub immunities: &'a CreatureImmunityState,
    pub events: &'a mut SourceUnitEvents,
    pub starts: &'a mut SourceSpellStarts,
    pub unit: SpellUnitState,
    pub level: u32,
    pub cast_speed: u32,
}
impl CreatureAddonExecutionOwner for Execution<'_> {
    fn cast_spell(&mut self, state: &mut CreatureAddonState, spell: u32) -> Result<()> {
        let mut owner = Owner {
            object: self.object,
            objects: self.objects,
            terrain: self.terrain,
            stores: self.stores,
            targets: self.targets,
            immunities: self.immunities,
            auras: state,
            unit: self.unit,
            level: self.level,
            cast_speed: self.cast_speed,
        };
        // Creature.cpp:2343 ignores the cast result. A refusal still stops here.
        SpellStartContext {
            stores: self.stores,
            starts: self.starts,
            events: self.events,
            owner: &mut owner,
        }
        .cast(
            self.object,
            Some(self.object),
            spell,
            SpellTriggerFlags(1),
            false,
        )?;
        Ok(())
    }
}
struct Owner<'a> {
    object: ObjectHandle,
    objects: &'a SourceObjects,
    terrain: &'a mut SourceTerrainManager,
    stores: &'a SourceSpellStores,
    targets: &'a SourceSpellTargetMgr,
    immunities: &'a CreatureImmunityState,
    auras: &'a CreatureAddonState,
    unit: SpellUnitState,
    level: u32,
    cast_speed: u32,
}
impl SpellLocationOwner for Owner<'_> {}
impl SpellExecutionOwner for Owner<'_> {
    fn unit(&mut self, unit: ObjectHandle) -> Result<SpellUnitState> {
        require(
            unit == self.object,
            "execution.creature_addon.other_unit_fields_owner",
        )?;
        self.objects.get(unit)?;
        let mut value = self.unit;
        value.standing = self.auras.stand == 0;
        value.mounted = self.auras.mount != 0;
        Ok(value)
    }
    fn map_time(&mut self, caster: ObjectHandle) -> Result<u32> {
        self.unit(caster)?;
        // Map.cpp:78-80; World.cpp:107,1389,1511. Startup precedes World::Update.
        Ok(0)
    }
    fn positive_definition(&mut self, t: &SpellTemplate) -> Result<bool> {
        positive_unit_spell(self.stores, t, 7, None)
    }
    fn targeting_data(&mut self, spell: u32) -> Result<[[u32; 2]; 3]> {
        self.targets
            .get_spell_targeting_data(spell)
            .map(|data| {
                data.filtering_scheme
                    .map(|row| row.map(|scheme| scheme as u32))
            })
            .map_err(|error| refuse(&format!("execution.spells.targeting.{error}")))
    }
    fn terrain_geometry(&mut self) -> Result<(&SourceObjects, &mut SourceTerrainManager)> {
        Ok((self.objects, self.terrain))
    }
    fn casting_source(&mut self, spell: &SourceSpell) -> Result<Option<[u32; 3]>> {
        let position = self.unit(spell.caster)?.position;
        Ok(Some(position[..3].try_into().unwrap()))
    }
    fn check(
        &mut self,
        spell: &mut SourceSpell,
        t: &SpellTemplate,
        check: SpellCheck,
    ) -> Result<SpellCastResult> {
        use SpellCastResult as R;
        match check {
            SpellCheck::PositiveImmunity(mask) => {
                // Spell.cpp:4926 evaluates IsPositiveSpell even for an empty mask.
                let positive =
                    positive_unit_spell(self.stores, t, 7, Some((spell.caster, self.object)))?;
                Ok(
                    if positive
                        && mask != 0
                        && self.immunities.immune_to_self_spell(
                            self.stores,
                            t,
                            mask,
                            self.object,
                            self.auras,
                        )?
                    {
                        R::TargetAuraState
                    } else {
                        R::Ok
                    },
                )
            }
            SpellCheck::SpiritOfRedemption => Ok(
                if self.auras.has_aura_type(176) && t.attributesex3 & 0x0010_0000 == 0 {
                    R::BadTargets
                } else {
                    R::Ok
                },
            ),
            SpellCheck::StrongerAura(_) if self.auras.aura_holders.is_empty() => Ok(R::Ok),
            SpellCheck::AllowedInLocation(location) => {
                // Unit.cpp:6488-6494: this fresh generic creature has no beneficiary.
                let map = self
                    .objects
                    .get(self.object)?
                    .map
                    .ok_or_else(|| refuse("execution.spells.location.WorldObject_GetMapId_owner"))?
                    .map;
                self.stores
                    .allowed_in_location(t, map, location[0], location[1], None, self)
            }
            SpellCheck::Items => spell.check_items(t, false, self),
            _ => Err(check.missing_owner()),
        }
    }
    fn call(&mut self, spell: &mut SourceSpell, call: SpellCall) -> Result<()> {
        if call != SpellCall::PrepareTriggerData {
            return Err(call.missing_owner());
        }
        // Spell.cpp:795-805. No cast item was supplied by Creature.cpp:2343.
        let t = self
            .stores
            .template(spell.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        let mut negative_effect_mask = 0;
        for i in 0..3 {
            if !positive_unit_effect(self.stores, t, i, None)? {
                negative_effect_mask |= 1 << i;
            }
        }
        spell.trigger_data = Some(SpellTriggerData {
            can_trigger: !spell.flags.do_not_proc(),
            negative_effect_mask,
        });
        Ok(())
    }
    fn cast_time(&mut self, spell: &mut SourceSpell, t: &SpellTemplate) -> Result<i32> {
        fresh_cast_time(self.stores, spell, t, self.level, self.cast_speed)
    }
}

// SpellMgr.cpp:104-172; Object.h:1156 has no spell-mod owner on a generic
// creature. Unit.cpp:341-343 constructs its ranged speed multiplier as one.
fn fresh_cast_time(
    stores: &SourceSpellStores,
    spell: &SourceSpell,
    t: &SpellTemplate,
    level: u32,
    speed: u32,
) -> Result<i32> {
    if spell.flags.old_triggered() && (t.manacost != 0 || t.manacostpercentage != 0) {
        return Ok(0); // Spell.cpp:7105-7108.
    }
    let Some(row) = stores.dbc("SpellCastTimes.dbc", t.castingtimeindex)? else {
        return Ok(0);
    };
    let word = |index| {
        row.words
            .get(&index)
            .copied()
            .ok_or_else(|| refuse("execution.creature_addon.SpellCastTimes_layout"))
    };
    // Mixed signed/unsigned source arithmetic converts to uint32 before the
    // implementation-defined int32 assignment on the pinned GCC target.
    let rank = creature_power_rank(level, t) as i32;
    let mut time = word(1)?
        .wrapping_add(word(2)?.wrapping_mul(((rank / 5) as u32).wrapping_sub(t.baselevel)))
        as i32;
    time = time.max(word(3)? as i32);
    if t.attributes & 2 != 0 && !spell.auto_repeat {
        time = add_ranged_delay(time)?;
    }
    if t.attributes & 0x0000_0030 == 0 && t.attributesex3 & 0x2000_0000 == 0 {
        time = scale_cast_time(time, speed)?;
    } else if t.attributes & 2 != 0 && !spell.auto_repeat {
        time = scale_cast_time(time, 1f32.to_bits())?;
    }
    if t.id == 19968 {
        time = 0;
    }
    Ok(time.max(0))
}

fn add_ranged_delay(time: i32) -> Result<i32> {
    time.checked_add(500)
        .ok_or_else(|| refuse("execution.spells.SpellMgr_145.cast_time_add_overflow_undefined"))
}

fn scale_cast_time(time: i32, speed: u32) -> Result<i32> {
    let value = Single::from_i128(i128::from(time))
        .value
        .mul_r(
            Single::from_bits(u128::from(speed)),
            Round::NearestTiesToEven,
        )
        .value;
    let converted = value.to_i128_r(32, Round::TowardZero, &mut false);
    require(
        !converted.status.contains(Status::INVALID_OP),
        "execution.spells.SpellMgr_160.cast_time_float_to_int_undefined",
    )?;
    Ok(converted.value as i32)
}

#[test]
fn cast_time_float_conversion_refuses_both_undefined_edges() {
    assert_eq!(scale_cast_time(i32::MIN, 1f32.to_bits()).unwrap(), i32::MIN);
    assert_eq!(
        scale_cast_time(2147483520, 1f32.to_bits()).unwrap(),
        2147483520
    );
    for (time, speed) in [(i32::MAX, 1f32), (i32::MIN, 2f32), (1, f32::NAN)] {
        assert_eq!(
            scale_cast_time(time, speed.to_bits()).unwrap_err().id,
            "execution.spells.SpellMgr_160.cast_time_float_to_int_undefined"
        );
    }
    assert_eq!(scale_cast_time(3, 0.5f32.to_bits()).unwrap(), 1);
}

#[test]
fn ranged_delay_refuses_at_first_signed_overflow() {
    assert_eq!(add_ranged_delay(i32::MAX - 500).unwrap(), i32::MAX);
    assert_eq!(
        add_ranged_delay(i32::MAX - 499).unwrap_err().id,
        "execution.spells.SpellMgr_145.cast_time_add_overflow_undefined"
    );
}
