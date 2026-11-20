// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS Classic 8ec338a1; WaypointMovementGenerator.cpp:257-286.
use super::super::*;
use std::collections::VecDeque;

/// Component-only MovementInfo and Unit speed-rate writes. Never serialized.
#[derive(Clone, Debug)]
pub struct SourceCreatureMovement {
    pub flags: Option<u32>,
    pub rates: [Option<f32>; 6],
    pub(in crate::native::source_execution) server_time: u32,
    pub(in crate::native::source_execution) fall_time: u32,
}
impl SourceCreatureMovement {
    /// Object.h:753, Unit.cpp:412-413, Creature.cpp:155,2781-2798.
    pub fn construct() -> Self {
        Self {
            flags: Some(0x100),
            rates: [Some(1.); 6],
            server_time: 0, // MovementInfo constructor, Object.h:753-754.
            fall_time: 0,
        }
    }
    /// Unit.cpp:8492-8499. std::max preserves a NaN first argument.
    pub fn set_speed_rate(&mut self, kind: usize, rate: f32) {
        self.rates[kind] = Some(f32::from_bits(
            benilla_sim::creature_init::source_speed_rate_bits(rate.to_bits()),
        ));
    }
    pub(in crate::native::source_execution) fn set_walk(
        &mut self,
        state: &mut u32,
        guid: u64,
        walk: bool,
        as_default: bool,
    ) -> Result<Option<SourceMovementPacket>> {
        source_set_creature_walk(state, self.flags.as_mut(), guid, walk, as_default)
    }
    /// Unit.cpp:12371-12378. Client-controlled movement does not write this bit.
    pub fn set_levitate(&mut self, enable: bool, client_controlled: bool) -> Result<()> {
        if !client_controlled {
            let flags = self
                .flags
                .as_mut()
                .ok_or_else(|| refuse("execution.movement.flags_owner"))?;
            if enable {
                *flags |= 0x400;
            } else {
                *flags &= !0x400;
            }
        }
        Ok(())
    }
    /// Creature.cpp:435-452. The swimmable closure owns collision height and terrain.
    pub fn init_entry_flags(
        &mut self,
        template: &CreatureAiTemplateOperands,
        controlled_pet: bool,
        client_controlled: bool,
        spawn: Result<Option<&LoadedCreatureSpawnTemplate>>,
        swimmable: impl FnOnce() -> Result<bool>,
        set_running: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        self.init_entry_flags_from_bits(
            [template.inhabit_type, template.extra_flags],
            controlled_pet,
            client_controlled,
            spawn,
            swimmable,
            set_running,
        )
    }

    pub(in crate::native::source_execution) fn init_entry_flags_from_bits(
        &mut self,
        [inhabit_type, extra_flags]: [u32; 2],
        controlled_pet: bool,
        client_controlled: bool,
        spawn: Result<Option<&LoadedCreatureSpawnTemplate>>,
        swimmable: impl FnOnce() -> Result<bool>,
        set_running: impl FnOnce(&mut Self) -> Result<()>,
    ) -> Result<()> {
        if !controlled_pet {
            self.set_levitate(inhabit_type & 4 != 0, client_controlled)?;
        }
        if inhabit_type & 2 != 0 && extra_flags & 0x8000 == 0 && swimmable()? {
            *self
                .flags
                .as_mut()
                .ok_or_else(|| refuse("execution.movement.flags_owner"))? |= 0x20_0000;
        }
        if let Some(spawn) = spawn? {
            if spawn.spawn_flags & 1 != 0 {
                set_running(self)?;
            }
            // Unit::SetHover, Unit.cpp:12451-12459, has the same control guard.
            if spawn.spawn_flags & 2 != 0 && !client_controlled {
                *self
                    .flags
                    .as_mut()
                    .ok_or_else(|| refuse("execution.movement.flags_owner"))? |= 0x4000_0000;
            }
            if spawn.spawn_flags & 4 != 0 {
                self.set_levitate(true, client_controlled)?;
            }
        }
        Ok(())
    }
    /// Reuse the bounded model/template rate selection, not a speed-to-rate division.
    pub fn bind_model_rates(
        &mut self,
        template: &benilla_sim::creature_init::Template,
        model: &benilla_sim::creature_init::ModelRow,
    ) -> Result<()> {
        let rates = benilla_sim::creature_init::model_speed_rates(template, model)
            .map_err(|_| refuse("execution.movement.model_speed_rates_owner"))?;
        self.rates[0] = Some(f32::from_bits(rates[0].bits()));
        self.rates[1] = Some(f32::from_bits(rates[1].bits()));
        Ok(())
    }
    /// MovementInfo::GetSpeedType, Object.h:769-777; Unit::GetSpeed, Unit.cpp:8439-8441.
    pub fn speed(&self) -> Result<f32> {
        let flags = self
            .flags
            .ok_or_else(|| refuse("execution.waypoint.movement_flags_owner_261"))?;
        self.speed_for_flags(flags)
    }
    pub fn speed_for_flags(&self, flags: u32) -> Result<f32> {
        let kind = source_movement_speed_type(flags);
        let rate =
            self.rates[kind].ok_or_else(|| refuse("execution.waypoint.speed_rate_owner_262"))?;
        const BASE: [f32; 6] = [2.5, 7., 4.5, 4.722222, 2.5, 3.141594];
        Ok(rate * BASE[kind])
    }
}
/// Object.h:769-777, including swimming's precedence over walking.
pub fn source_movement_speed_type(flags: u32) -> usize {
    if flags & 0x20_0000 != 0 {
        if flags & 2 != 0 {
            4
        } else {
            3
        }
    } else if flags & 0x100 != 0 {
        0
    } else if flags & 2 != 0 {
        2
    } else {
        1
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> Result<f32> {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    // G3D/Vector3.h:714-715. Overflow remains IEEE infinity until the conversion.
    let square = (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2];
    if !square.is_finite() {
        return Ok(square);
    }
    benilla_sim::source_transports::sqrt_single_bits(square.to_bits())
        .map(f32::from_bits)
        .map_err(|_| refuse("execution.waypoint.sqrt_numeric_binding"))
}
fn travel_uint(value: f32, speed: f32, accumulation: bool) -> Result<u32> {
    // Conversion truncates first; (-1,0) is defined zero, unlike <= -1.
    if !value.is_finite() || value <= -1. || value >= 4294967296. {
        return Err(refuse(match (accumulation, speed == 0.) {
            (true, true) => "execution.waypoint.zero_speed_conversion_undefined_279",
            (false, true) => "execution.waypoint.zero_speed_conversion_undefined_284",
            (true, false) => "execution.waypoint.travel_conversion_undefined_279",
            (false, false) => "execution.waypoint.travel_conversion_undefined_284",
        }));
    }
    Ok(value as u32)
}
/// BuildIntPath's post-calculate body, cpp:268-286. firstPoint never advances.
pub fn source_waypoint_append_path(
    path: &mut Vec<[f32; 3]>,
    generated: &[[f32; 3]],
    end: [f32; 3],
    speed: f32,
) -> Result<u32> {
    let first = *path
        .last()
        .ok_or_else(|| refuse("execution.waypoint.path_back_undefined_259"))?;
    let mut travel = 0u32;
    for &point in generated.iter().skip(1) {
        let time = distance(point, first)? / speed * 1000.;
        travel = travel_uint(travel as f32 + time, speed, true)?;
        path.push(point);
    }
    // cpp:282-284 overwrites every loop accumulation, even for an empty genPath.
    travel_uint(distance(end, first)? / speed * 1000., speed, false)
}

/// SendNextWayPointPath, cpp:367-394. Return the last node before spline setup.
pub fn source_waypoint_lookahead(
    path: &SourceWaypointPath,
    mut cursor: u32,
    generated: &mut Vec<[f32; 3]>,
    mut travel: u32,
    indexes: &mut VecDeque<i32>,
    mut next: impl FnMut(u32, bool) -> Result<Option<u32>>,
    mut build: impl FnMut(&mut Vec<[f32; 3]>, [f32; 3]) -> Result<u32>,
) -> Result<u32> {
    let mut looped = false;
    let mut seen = std::collections::BTreeSet::new();
    while path[&cursor].delay == 0 && travel < 6000 {
        let back = generated
            .last()
            .ok_or_else(|| refuse("execution.waypoint.path_back_undefined_259"))?;
        // Node data, speed and mesh stay fixed here. Only these inputs determine
        // the next iteration; a bitwise repeat grows both containers until allocation fails.
        require(
            seen.insert((cursor, back.map(f32::to_bits), travel, looped)),
            "execution.waypoint.minimum_path_time_nonterminating_cycle_370",
        )?;
        let mut after = next(cursor, looped)?;
        if after.is_none() {
            if looped {
                break;
            }
            looped = true;
            after = next(cursor, true)?;
            if after.is_none() {
                break;
            }
        }
        let after = after.unwrap();
        indexes.push_back((generated.len() - 1) as i32);
        travel = travel.wrapping_add(build(generated, path[&after].position)?);
        cursor = after;
    }
    Ok(cursor)
}

#[cfg(test)]
#[path = "waypoint_travel_tests.rs"]
mod tests;
