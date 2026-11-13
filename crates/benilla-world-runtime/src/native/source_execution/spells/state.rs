// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:387-504. Live state has no persistence or admission interface.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceSpellState {
    Created,
    Targeting,
    Casting,
    Delayed,
    Traveling,
    Landing,
    Channeling,
    Finished,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SpellCastResult {
    Ok,
    NotFound,
    NotReady,
    DontReport,
    CasterDead,
    NotStanding,
    AffectingCombat,
    OnlyOutdoors,
    OnlyIndoors,
    NotShapeshift,
    OnlyShapeshift,
    OnlyStealthed,
    CasterAuraState,
    NoComboPoints,
    Moving,
    TargetNotDead,
    Immune,
    BadTargets,
    LowLevel,
    CantCastOnTapped,
    TargetIsPlayer,
    TargetAuraState,
    NotBehind,
    NotInfront,
    TargetAffectingCombat,
    AuraBounced,
    NothingToDispel,
    HighLevel,
    NoPet,
    TargetsDead,
    NotOnTaxi,
    NotMounted,
    RequiresArea,
    OnlyBattlegrounds,
    RequiresSpellFocus,
    Rooted,
    TargetNoWeapons,
    BadImplicitTargets,
    Unknown,
    Interrupted,
    ItemGone,
    EquippedItemClass,
    CantBeDisenchanted,
    OutOfRange,
    TooClose,
    UnitNotInfront,
    LineOfSight,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpellTargets {
    pub unit: Option<ObjectHandle>,
    pub gameobject: Option<ObjectHandle>,
    pub corpse: Option<ObjectHandle>,
    pub item_guid: u64,
    pub item: Option<ObjectHandle>,
    pub mask: u32,
    pub destination: Option<[u32; 3]>,
    pub source: Option<[u32; 3]>,
}
/// Each bit is independent, including OLD_TRIGGERED versus INSTANT_CAST.
/// Spell.cpp:432,458-473; SpellDefines.h:431-449.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellTriggerFlags(pub u32);
impl SpellTriggerFlags {
    pub fn old_triggered(self) -> bool {
        self.0 & 1 != 0
    }
    pub fn instant(self) -> bool {
        self.0 & 8 != 0
    }
    pub fn autorepeat(self) -> bool {
        self.0 & 0x10 != 0
    }
    pub fn notify_ai(self) -> bool {
        self.0 & 0x100 != 0
    }
    pub fn ignore_gcd(self) -> bool {
        self.old_triggered() || self.0 & 0x200 != 0
    }
    pub fn ignore_costs(self) -> bool {
        self.old_triggered() && self.0 & 0x20000 == 0 || self.0 & 0x400 != 0
    }
    pub fn ignore_cooldowns(self) -> bool {
        self.old_triggered() || self.0 & 0x800 != 0
    }
    pub fn ignore_current(self, t: &SpellTemplate) -> bool {
        self.old_triggered() || self.0 & 0x1000 != 0 || t.attributesex4 & 0x80 != 0
    }
    pub fn ignore_caster_aura(self) -> bool {
        self.old_triggered() || self.0 & 0x10000 != 0
    }
    pub fn ignore_unselectable(self) -> bool {
        self.old_triggered() || self.0 & 4 != 0
    }
    pub fn ignore_hit(self) -> bool {
        self.0 & 2 != 0
    }
    pub fn ignore_unattackable(self) -> bool {
        self.0 & 0x20 != 0
    }
    pub fn do_not_proc(self) -> bool {
        self.0 & 0x40 != 0
    }
    pub fn pet_cast(self) -> bool {
        self.0 & 0x80 != 0
    }
    pub fn hide_log(self, auto: bool) -> bool {
        self.old_triggered() && !auto || self.0 & 0x2000 != 0
    }
    pub fn reset_leash(self) -> bool {
        self.0 & 0x4000 == 0
    }
    pub fn channel_only(self) -> bool {
        self.0 & 0x8000 != 0
    }
}
#[derive(Clone, Debug)]
pub struct SourceSpell {
    pub id: u64,
    pub spell: u32,
    pub caster: ObjectHandle,
    pub original_caster: ObjectHandle,
    pub flags: SpellTriggerFlags,
    pub state: SourceSpellState,
    pub targets: SpellTargets,
    pub triggered_by_aura: Option<u32>,
    /// First reagent of m_triggeredBySpellInfo, absent when that pointer is null.
    pub triggered_by_spell_reagent: Option<i32>,
    pub event: Option<u64>,
    pub base_points: [i32; 3],
    pub school_mask: u32,
    pub attack_type: u8,
    pub affective_caster: Option<ObjectHandle>,
    pub reflectable: Option<bool>,
    pub jump_radius: u32,
    pub filtering_scheme: Option<[[u32; 2]; 3]>,
    pub cast_time: i32,
    /// Spell.cpp:6275, binary32 bits, assigned only after CheckRange succeeds.
    pub max_range: u32,
    pub timer: u32,
    pub delay_start: u64,
    pub delay_moment: u64,
    pub damage_delay_count: u8,
    pub referenced: bool,
    pub executing: bool,
    pub creation_time: u32,
    pub updated: bool,
    pub cast_position: [u32; 4],
    pub auto_repeat: bool,
    pub script_present: bool,
    pub client_cast: bool,
    pub item_cast: bool,
    pub partial_application_mask: u32,
    pub effect_skip_mask: u32,
}
impl SourceSpell {
    pub(super) fn new(
        id: u64,
        t: &SpellTemplate,
        caster: ObjectHandle,
        flags: SpellTriggerFlags,
        creation_time: u32,
    ) -> Result<Self> {
        require(
            t.school < 32,
            "execution.spells.GetSchoolMask_shift_undefined",
        )?;
        // DBCStructure.h:817, Spell.cpp:424-425. Do not wrap signed addition.
        let mut base_points = [0; 3];
        for (i, (base, dice)) in [
            (t.effectbasepoints1, t.effectbasedice1),
            (t.effectbasepoints2, t.effectbasedice2),
            (t.effectbasepoints3, t.effectbasedice3),
        ]
        .into_iter()
        .enumerate()
        {
            base_points[i] = base
                .checked_add(dice as i32)
                .ok_or_else(|| refuse("execution.spells.simple_value_overflow_undefined"))?;
        }
        Ok(Self {
            id,
            spell: t.id,
            caster,
            original_caster: caster,
            flags,
            state: SourceSpellState::Created,
            targets: SpellTargets::default(),
            triggered_by_aura: None,
            event: None,
            triggered_by_spell_reagent: None,
            base_points,
            cast_time: 0,
            max_range: 0,
            timer: 0,
            delay_start: 0,
            delay_moment: 0,
            damage_delay_count: 0,
            referenced: false,
            executing: false,
            creation_time,
            updated: false,
            cast_position: [0; 4],
            auto_repeat: is_auto(t),
            script_present: false,
            client_cast: false,
            item_cast: false,
            partial_application_mask: 0,
            effect_skip_mask: 0,
            school_mask: 1u32 << t.school,
            attack_type: attack_type(t),
            affective_caster: Some(caster),
            reflectable: None,
            jump_radius: 10f32.to_bits(),
            filtering_scheme: None,
        })
    }
    pub fn deletable(&self) -> bool {
        !self.referenced && !self.executing
    }
    pub fn finish_failed(&mut self, owner: &mut (impl SpellExecutionOwner + ?Sized)) -> Result<()> {
        if self.state == SourceSpellState::Finished {
            return Ok(());
        }
        self.state = SourceSpellState::Finished;
        if self.flags.notify_ai() {
            let c = owner.unit(self.caster)?;
            if !c.gameobject && c.has_ai {
                owner.call(self, SpellCall::AiCastState(false))?;
            }
        }
        Ok(())
    }
}
pub(super) fn is_channel(t: &SpellTemplate) -> bool {
    t.attributesex & 0x44 != 0
}
pub(super) fn is_melee(t: &SpellTemplate) -> bool {
    t.attributes & 0x404 != 0
}
pub(super) fn is_auto(t: &SpellTemplate) -> bool {
    t.attributes & 2 != 0 && t.attributesex2 & 0x20 != 0
}

/// Object.cpp:2599-2615,2800-2810. emplace never replaces a key.
#[derive(Clone, Debug, Default)]
pub struct SourceSpellGcd {
    deadlines: BTreeMap<u32, i64>,
}
impl SourceSpellGcd {
    pub fn has(&self, category: u32, now: i64) -> bool {
        self.deadlines.get(&category).is_some_and(|at| *at > now)
    }
    pub fn add(&mut self, t: &SpellTemplate, now: i64, duration: u32) -> Result<()> {
        if duration != 0 {
            let at = now
                .checked_add(i64::from(duration))
                .ok_or_else(|| refuse("execution.spells.gcd_clock_overflow_undefined"))?;
            self.deadlines.entry(t.startrecoverycategory).or_insert(at);
        }
        Ok(())
    }
    pub fn reset(&mut self, category: u32) {
        self.deadlines.remove(&category);
    }
    pub fn expire(&mut self, now: i64) {
        self.deadlines.retain(|_, at| *at > now);
    }
}
// SpellMgr.cpp:333-354, SharedDefines.h:512-514.
pub(super) fn attack_type(t: &SpellTemplate) -> u8 {
    if t.dmgclass == 3 || t.dmgclass != 2 && t.attributesex3 & 0x8000 != 0 {
        2
    } else if t.attributesex3 & 0x1000000 != 0 {
        1
    } else {
        0
    }
}
