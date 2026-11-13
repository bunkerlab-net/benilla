// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:6157-6277. Live operands are requested only at their source reads.
use super::*;
use SpellCastResult as R;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellRangeEntry {
    pub minimum: SpellFloat,
    pub maximum: SpellFloat,
    pub flags: u32,
}
impl SourceSpellStores {
    // DBCStructure.h:904-909; a missing row is a defined null LookupEntry.
    pub fn range_entry(&self, index: u32) -> Result<Option<SpellRangeEntry>> {
        self.dbc("SpellRange.dbc", index)?
            .map(|row| {
                let word = |column| {
                    row.words
                        .get(&column)
                        .copied()
                        .ok_or_else(|| refuse("execution.spells.SpellRange_dbc_column_owner"))
                };
                Ok(SpellRangeEntry {
                    minimum: SpellFloat(word(1)?),
                    maximum: SpellFloat(word(2)?),
                    flags: word(3)?,
                })
            })
            .transpose()
    }
}

impl SourceSpell {
    pub fn min_max_range(
        &self,
        t: &SpellTemplate,
        strict: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<(f32, f32)> {
        // Spell.cpp:6159-6162. This precedes even the true-caster type read.
        if strict && is_melee(t) {
            return Ok((0.0, 100.0));
        }
        let kind = owner.range_objects()?.get(self.caster)?.kind;
        let unit = matches!(
            kind,
            SourceObjectKind::Player | SourceObjectKind::Creature | SourceObjectKind::Pet
        );
        let mut minimum = 0.0;
        let mut maximum = 0.0;
        let mut modifier = 0.0;
        if let Some(range) = owner.range_entry(t.rangeindex)? {
            let target = self.targets.unit;
            if range.flags & 1 != 0 {
                // :6170-6173; Object.cpp:1729-1746.
                if unit {
                    modifier = combined_reach(owner, self.caster, target.unwrap_or(self.caster))?;
                }
            } else {
                let mut melee = 0.0;
                if range.flags & 2 != 0 && unit {
                    melee = combined_reach(owner, self.caster, target.unwrap_or(self.caster))?;
                }
                minimum = f32::from_bits(range.minimum.0) + melee;
                maximum = f32::from_bits(range.maximum.0);
                // :6187-6192. The corpse pointer is read only after a null Unit.
                if target.is_some() || owner.range_corpse(self)? {
                    modifier = owner.combat_reach(self.caster)?
                        + owner.combat_reach(target.unwrap_or(self.caster))?;
                    if minimum > 0.0 && range.flags & 2 == 0 {
                        minimum += modifier;
                    }
                }
            }
            // :6196-6198. Preserve the short-circuit movement reads.
            if let Some(target) = target {
                if unit
                    && owner.moving_forward(self.caster)?
                    && owner.moving_forward(target)?
                    && !owner.walking(self.caster)?
                    && !owner.walking(target)?
                    && (range.flags & 1 != 0
                        || owner.range_objects()?.get(target)?.kind == SourceObjectKind::Player)
                {
                    modifier += f32::from_bits(0x402a_aaab); // ObjectDefines.h:27, 8.f/3.f.
                }
            }
        }
        if unit {
            // :6203-6208. Never replace equipped-item or player spellmods with 1.
            if t.attributes & 2 != 0 && kind == SourceObjectKind::Player {
                if let Some(range) = owner.ranged_mod_range(self.caster)? {
                    maximum *= range * 0.01;
                }
            }
            if let Some(player) = owner.range_mod_owner(self.caster)? {
                maximum = owner.apply_range_mod(player, t.id, maximum)?;
            }
        }
        maximum += modifier; // :6211, after spellmods.
        Ok((minimum, maximum))
    }

    pub fn check_range(
        &mut self,
        t: &SpellTemplate,
        strict: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<R> {
        if !strict && self.cast_time == 0 {
            return Ok(R::Ok);
        } // :6219-6220.
        let mut target = self.targets.unit; // :6222, before GetMinMaxRange.
        let (minimum, mut maximum) = self.min_max_range(t, strict, owner)?;
        if let Some(range) = owner.range_entry(t.rangeindex)? {
            if range.flags & 1 == 0 && !strict {
                // :6230, std::min's left operand wins unordered comparisons.
                let tolerance = maximum * 0.1;
                maximum += if tolerance < 3.0 { tolerance } else { 3.0 };
            }
        }
        if target.is_none() && t.targets & 0x4800 != 0 {
            target = self.targets.gameobject;
        } // :6232-6233.
        if target.is_none()
            && self.client_cast
            && t.effects()
                .iter()
                .any(|e| e.target_a == 5 || e.target_b == 5)
        {
            // :6235-6236, GetPet is called twice on the non-null path.
            // Spell.cpp:397: a non-Unit true caster leaves m_caster null.
            require(
                matches!(
                    owner.range_objects()?.get(self.caster)?.kind,
                    SourceObjectKind::Player | SourceObjectKind::Creature | SourceObjectKind::Pet
                ),
                "execution.spells.CheckRange_GetPet_null_caster_undefined",
            )?;
            target = if owner.range_pet(self.caster)?.is_some() {
                owner.range_pet(self.caster)?
            } else {
                owner.range_charm(self.caster)?
            };
        }
        if let Some(target) = target.filter(|target| *target != self.caster) {
            let origin = pose(owner, self.caster)?;
            let destination = pose(owner, target)?;
            let result = compare_distance(
                origin[..3].try_into().unwrap(),
                destination[..3].try_into().unwrap(),
                minimum,
                maximum,
            );
            if result != R::Ok {
                return Ok(result);
            } // :6241-6245.
            if owner.range_objects()?.get(self.caster)?.kind == SourceObjectKind::Player
                && owner.range_facing(t.id)? & 1 != 0
                && !benilla_sim::facing::has_in_arc_bits(
                    [origin[0], origin[1]],
                    origin[3],
                    [destination[0], destination[1]],
                    0x4049_0fdb,
                    false,
                )
                .map_err(|_| refuse("execution.spells.WorldObject_HasInArc_numeric"))?
            {
                return Ok(R::UnitNotInfront); // :6246-6248, inclusive arc borders.
            }
        }
        // :6251 and :6263 use equality, not membership in the target mask.
        for (mask, position) in [
            (0x40, self.targets.destination),
            (0x20, self.targets.source),
        ] {
            if self.targets.mask != mask {
                continue;
            }
            let destination =
                position.ok_or_else(|| refuse("execution.spells.SpellTarget_position_owner"))?;
            let origin = pose(owner, self.caster)?;
            let result = compare_distance(
                origin[..3].try_into().unwrap(),
                destination,
                minimum,
                maximum,
            );
            if result != R::Ok {
                return Ok(result);
            }
            // SpellMgr.h:1400-1413. rangeIndex 13 does not bypass these calls.
            if !matches!(t.id, 8733 | 8735 | 24742) && t.attributesex2 & 4 == 0 {
                let mut end = destination.map(f32::from_bits);
                end[2] += 1.0; // :6259,6271, before IsWithinLOS.
                if !owner.range_point_los(self.caster, end)? {
                    return Ok(R::LineOfSight);
                }
            }
        }
        self.max_range = maximum.to_bits(); // :6275, never on an earlier return.
        Ok(R::Ok)
    }
}
fn pose(owner: &mut (impl SpellExecutionOwner + ?Sized), object: ObjectHandle) -> Result<[u32; 4]> {
    owner
        .range_objects()?
        .get(object)?
        .pose
        .map(|p| p.0)
        .ok_or_else(|| refuse("execution.spells.WorldObject_position_owner"))
}
fn combined_reach(
    owner: &mut (impl SpellExecutionOwner + ?Sized),
    caster: ObjectHandle,
    target: ObjectHandle,
) -> Result<f32> {
    // Object.cpp:1731 passes (0.f + target reach) to the overload.
    let flat = 0.0 + owner.combat_reach(target)?;
    let mut reach = owner.combat_reach(caster)? + flat;
    reach += 1.33;
    if reach < 5.0 {
        reach = 5.0;
    }
    Ok(reach)
}
fn compare_distance(origin: [u32; 3], target: [u32; 3], minimum: f32, maximum: f32) -> R {
    // Object.cpp:1254-1261,1286-1289,1279,1306: DIST_CALC_NONE is squared.
    // No bounding radius, sqrt, host libm, reassociation or fused operations.
    let [dx, dy, dz] =
        std::array::from_fn(|i| f32::from_bits(origin[i]) - f32::from_bits(target[i]));
    let mut square = dx * dx + dy * dy;
    square += dz * dz;
    if square > maximum * maximum {
        R::OutOfRange
    } else if minimum != 0.0 && square < minimum * minimum {
        R::TooClose
    } else {
        R::Ok
    }
}
