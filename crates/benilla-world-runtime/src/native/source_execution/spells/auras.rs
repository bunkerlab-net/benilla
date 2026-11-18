// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! SpellAuras.cpp:257-290,5155-5212. State belongs to the target Unit.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAura {
    pub kind: u32,
    pub amount: i32,
    pub base_points: i32,
    pub misc: i32,
    pub periodic_time: u32,
    pub periodic_timer: i32,
    pub periodic_tick: u32,
    pub positive: bool,
    pub apply_time: i64,
    pub applied: bool,
    pub periodic: bool,
    pub area: bool,
    pub persistent: bool,
    pub magnet_used: bool,
    pub remove_mode: u32,
    pub script_value: u64,
    pub affect_overridden: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAuraHolder {
    pub caster: ObjectHandle,
    pub target: ObjectHandle,
    pub triggered_by: Option<u32>,
    pub apply_time: i64,
    pub apply_ms: u32,
    pub duration: i32,
    pub max_duration: i32,
    pub permanent: bool,
    pub passive: bool,
    pub death_persistent: bool,
    pub removed_on_shape_lost: bool,
    pub charges: u32,
    pub stack: u32,
    pub slot: u8,
    pub level: u32,
    pub ready: bool,
    pub deleted: bool,
    pub skip_update: bool,
    pub tracked: bool,
    pub time_cla: i32,
    pub remove_mode: u32,
    pub diminish_group: u32,
    pub heartbeat_chance: u32,
    pub heartbeat_timer: i32,
    pub heartbeat_interval: i32,
    pub proc_cooldown_ms: i64,
    pub effects: [Option<SourceAura>; 3],
}
impl SourceAuraHolder {
    pub fn new(
        t: &SpellTemplate,
        target: ObjectHandle,
        time: i64,
        ms: u32,
        duration: i32,
    ) -> Result<Self> {
        let passive = t.attributes & 0x40 != 0;
        let tracked = !checks::area_spell(t)?
            && (match t.mechanic {
                5 | 23 => true,
                7 | 10 | 14 | 17 | 18 | 20 => t.spellfamilyname != 0 && t.spellfamilyflags != 0,
                _ => false,
            } || (t.spellfamilyname == 9
                && t.effects().iter().any(|effect| effect.aura == 68)));
        Ok(Self {
            caster: target,
            target,
            triggered_by: None,
            apply_time: time,
            apply_ms: ms,
            duration,
            max_duration: duration,
            permanent: duration == -1 || (passive && t.durationindex == 0),
            passive,
            death_persistent: t.attributesex3 & 0x0010_0000 != 0,
            removed_on_shape_lost: t.attributes & 0x0400_0000 == 0
                && ((t.stances != 0
                    && (matches!(t.id, 11327 | 11329)
                        || (t.attributesex2 & 0x0008_0000 == 0
                            && t.attributes & 0x0001_0000 == 0)))
                    || (t.stances == 0 && t.spellfamilyname == 7 && t.effectapplyauraname1 == 49)),
            charges: t.proccharges,
            stack: match t.id {
                24575 | 24659 | 24662 | 26464 => t.stackamount,
                _ => 1,
            },
            slot: 48,
            level: 1,
            ready: false,
            deleted: false,
            skip_update: false,
            tracked,
            time_cla: 1000,
            remove_mode: 0,
            diminish_group: 0,
            heartbeat_chance: 0,
            heartbeat_timer: 0,
            heartbeat_interval: 0,
            proc_cooldown_ms: 0,
            effects: [None, None, None],
        })
    }
}

/// SpellMgr.cpp:56-64. abs(INT_MIN) is undefined in the pinned C++.
pub fn source_spell_duration(value: i32) -> Result<i32> {
    if value == -1 {
        return Ok(-1);
    }
    value
        .checked_abs()
        .ok_or_else(|| refuse("execution.spells.SpellMgr_63.duration_abs_undefined"))
}

/// SpellMgr.cpp:66-74, maximum duration has its own abs call.
pub fn source_spell_max_duration(value: i32) -> Result<i32> {
    if value == -1 {
        return Ok(-1);
    }
    value
        .checked_abs()
        .ok_or_else(|| refuse("execution.spells.SpellMgr_73.max_duration_abs_undefined"))
}

#[test]
fn duration_absolute_value_preserves_permanent_and_refuses_int_min() {
    assert_eq!(source_spell_duration(-1).unwrap(), -1);
    assert_eq!(source_spell_duration(i32::MIN + 1).unwrap(), i32::MAX);
    assert_eq!(source_spell_duration(i32::MAX).unwrap(), i32::MAX);
    assert_eq!(
        source_spell_duration(i32::MIN).unwrap_err().id,
        "execution.spells.SpellMgr_63.duration_abs_undefined"
    );
    assert_eq!(source_spell_max_duration(-1).unwrap(), -1);
    assert_eq!(source_spell_max_duration(i32::MIN + 1).unwrap(), i32::MAX);
    assert_eq!(source_spell_max_duration(i32::MAX).unwrap(), i32::MAX);
    assert_eq!(
        source_spell_max_duration(i32::MIN).unwrap_err().id,
        "execution.spells.SpellMgr_73.max_duration_abs_undefined"
    );
}

/// Unit.cpp:330-366 zero-initializes these update fields.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceAuraFields {
    pub spells: [u32; 48],
    pub flags: [u32; 6],
    pub levels: [u32; 12],
    pub applications: [u32; 12],
}
impl Default for SourceAuraFields {
    fn default() -> Self {
        Self {
            spells: [0; 48],
            flags: [0; 6],
            levels: [0; 12],
            applications: [0; 12],
        }
    }
}

/// SpellAuras.cpp:280; the unscoped AuraType enum has the range 0..255.
pub fn source_aura_type(value: u32) -> Result<u32> {
    require(
        value <= 255,
        "execution.spells.SpellAuras_280.AuraType_enum_undefined",
    )?;
    Ok(value)
}

#[test]
fn aura_type_cast_checks_both_sides_of_the_nonfixed_enum_boundary() {
    assert_eq!(source_aura_type(191).unwrap(), 191);
    assert_eq!(source_aura_type(192).unwrap(), 192);
    assert_eq!(source_aura_type(255).unwrap(), 255);
    assert_eq!(
        source_aura_type(256).unwrap_err().id,
        "execution.spells.SpellAuras_280.AuraType_enum_undefined"
    );
}
