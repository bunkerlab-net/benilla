// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

// Spell.cpp:808-885; SpellMgr.h:1654-1692.
fn prepare_masks_for_proc_system(
    stores: &SourceSpellStores,
    spell: &SourceSpell,
    t: &SpellTemplate,
    mask: u32,
    target: Option<ObjectHandle>,
) -> Result<[u32; 2]> {
    let [mut attacker, mut victim] = match t.dmgclass {
        2 => {
            let swing = match spell.attack_type {
                0 => 0x0040_0000,
                1 => 0x0080_0000,
                _ => 0,
            };
            [0x10 | swing, 0x20]
        }
        3 if t.attributesex2 & 0x20 != 0 => [0x40, 0x80],
        3 => [0x100, 0x200],
        _ => {
            let positive =
                positive_unit_spell(stores, t, mask, target.map(|target| (spell.caster, target)))?;
            let ability = t.attributes & 0x10 != 0;
            match (positive, ability) {
                (true, true) => [0x400, 0x800],
                (true, false) => [0x4000, 0x8000],
                (false, _) if t.attributesex2 & 0x20 != 0 => [0x40, 0x80],
                (false, true) => [0x1000, 0x2000],
                (false, false) => [0x10000, 0x20000],
            }
        }
    };
    if t.spellfamilyname == 9 && t.spellfamilyflags & 0x0000_2000_0000_001c != 0 {
        attacker |= 0x0020_0000;
    }
    if t.attributes & 0x404 != 0 {
        attacker |= 4;
        victim |= 8;
    }
    if t.attributesex3 & 0x10000 != 0 {
        attacker = 0;
    }
    if t.attributesex3 & 0x20000 != 0 {
        victim = 0;
    }
    Ok([attacker, victim])
}

// SpellMgr.h:1694-1707. Preserve the source's exact set of flags.
const MELEE_BASED: u32 = 0x37c;
const NEGATIVE_TRIGGER: u32 = MELEE_BASED | 0x33000;

// SpellMgr.h:1657-1658,1672-1688.
const HEARTBEAT: u32 = 0x1;
const KILL: u32 = 0x2;
const DEAL_HELPFUL_ABILITY: u32 = 0x400;
const TAKE_HELPFUL_ABILITY: u32 = 0x800;
const DEAL_HARMFUL_ABILITY: u32 = 0x1000;
const TAKE_HARMFUL_ABILITY: u32 = 0x2000;
const DEAL_HELPFUL_SPELL: u32 = 0x4000;
const TAKE_HELPFUL_SPELL: u32 = 0x8000;
const DEAL_HARMFUL_SPELL: u32 = 0x1_0000;
const TAKE_HARMFUL_SPELL: u32 = 0x2_0000;
const DEAL_HARMFUL_PERIODIC: u32 = 0x4_0000;
const TAKE_HARMFUL_PERIODIC: u32 = 0x8_0000;
const ON_TRAP_ACTIVATION: u32 = 0x20_0000;

// SpellMgr.h:1711-1713,1728,1731,1735.
const NORMAL_HIT: u32 = 0x1;
const CRITICAL_HIT: u32 = 0x2;
const TRIGGER_ON_NO_DAMAGE: u32 = 0x1_0000;
const CAST_END: u32 = 0x8_0000;
const INTERNAL_HOT: u32 = 0x200_0000;

// SpellDefines.h:57,139,159,165.
const ALLOW_WHILE_SITTING: u32 = 0x0800_0000;
const NO_PROC_EQUIP_REQUIREMENT: u32 = 0x2;
const ONLY_PROC_OUTDOORS: u32 = 0x0020_0000;
const ONLY_PROC_ON_CASTER: u32 = 0x0800_0000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProcTriggerExit {
    ZeroFlags,
    NoOverlap,
    PredicateRejected,
    KillPlayer,
    SelfTrigger,
    OtherCaster,
    Sitting,
    Later(&'static str),
}

impl ProcTriggerExit {
    fn refusal(self) -> Option<Refusal> {
        match self {
            Self::KillPlayer => Some(refuse(
                "execution.spells.UnitAuraProcHandler_538.isHonorOrXPTarget_owner",
            )),
            Self::Later(id) => Some(refuse(id)),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum ProcPredicate {
    NoOverlap,
    Rejected,
    Allowed,
}

// SpellMgr.cpp:843-934.
fn is_spell_proc_event_can_triggered_by(
    event: Option<&SpellProc>,
    event_flags: u32,
    spell: Option<&SpellTemplate>,
    flags: u32,
    extra: u32,
) -> Result<ProcPredicate> {
    use ProcPredicate::*;
    if flags & event_flags == 0 {
        return Ok(NoOverlap);
    }
    if event_flags & (HEARTBEAT | KILL | ON_TRAP_ACTIVATION) != 0 {
        return Ok(Allowed);
    }
    if flags & event_flags & DEAL_HARMFUL_PERIODIC != 0 {
        if extra & INTERNAL_HOT != 0 {
            if event_flags == DEAL_HARMFUL_PERIODIC {
                let spell = spell.ok_or_else(|| {
                    refuse("execution.spells.SpellMgr_863.hot_null_spell_undefined")
                })?;
                if spell.spellfamilyname == 0 {
                    return Ok(Rejected);
                }
            } else if event_flags & (DEAL_HELPFUL_SPELL | DEAL_HELPFUL_ABILITY) == 0 {
                return Ok(Rejected);
            }
        } else if event_flags != DEAL_HARMFUL_PERIODIC
            && event_flags & (DEAL_HARMFUL_SPELL | DEAL_HARMFUL_ABILITY) == 0
        {
            return Ok(Rejected);
        }
    }
    if flags & event_flags & TAKE_HARMFUL_PERIODIC != 0 {
        if extra & INTERNAL_HOT != 0 {
            if event_flags == TAKE_HARMFUL_PERIODIC {
                return Ok(Rejected);
            }
            if event_flags & (TAKE_HELPFUL_SPELL | TAKE_HELPFUL_ABILITY) == 0 {
                return Ok(Rejected);
            }
        } else if event_flags != TAKE_HARMFUL_PERIODIC
            && event_flags & (TAKE_HARMFUL_SPELL | TAKE_HARMFUL_ABILITY) == 0
        {
            return Ok(Rejected);
        }
    }
    let mut required_extra = 0;
    if let Some(event) = event {
        required_extra = event.extra;
        match spell {
            None => {
                // SharedDefines.h:186-189. Melee uses the normal school.
                if event.school_mask != 0 && event.school_mask & 1 == 0 {
                    return Ok(Rejected);
                }
            }
            Some(spell) => {
                if event.school_mask != 0 {
                    // SharedDefines.h:233-235. An out-of-range shift is undefined.
                    let school = 1u32.checked_shl(spell.school).ok_or_else(|| {
                        refuse("execution.spells.SpellMgr_908.school_shift_undefined")
                    })?;
                    if event.school_mask & school == 0 {
                        return Ok(Rejected);
                    }
                }
                if event.family != 0 && event.family != spell.spellfamilyname {
                    return Ok(Rejected);
                }
            }
        }
    }
    let allowed = if required_extra == 0 {
        extra & (CAST_END | NORMAL_HIT | CRITICAL_HIT) != 0
    } else {
        required_extra & TRIGGER_ON_NO_DAMAGE != 0 || required_extra & extra != 0
    };
    Ok(if allowed { Allowed } else { Rejected })
}

#[derive(Clone, Copy)]
struct ProcUnit<'a> {
    objects: &'a SourceObjects,
    stand: u8,
    player: bool,
    victim: bool,
}

// UnitAuraProcHandler.cpp:514-592.
fn is_triggered_at_spell_proc_event(
    stores: &SourceSpellStores,
    aura: &SpellTemplate,
    holder: &SourceAuraHolder,
    spell: Option<&SpellTemplate>,
    flags: u32,
    extra: u32,
    unit: ProcUnit<'_>,
) -> Result<ProcTriggerExit> {
    use ProcTriggerExit::*;
    let event = stores.proc_event(aura.id);
    let event_flags = proc_event_flags(event, aura);
    if event_flags == 0 {
        return Ok(ZeroFlags);
    }
    match is_spell_proc_event_can_triggered_by(event, event_flags, spell, flags, extra)? {
        ProcPredicate::NoOverlap => return Ok(NoOverlap),
        ProcPredicate::Rejected => return Ok(PredicateRejected),
        ProcPredicate::Allowed => {}
    }
    if event_flags & KILL != 0 && unit.player {
        return Ok(KillPlayer);
    }
    if spell.is_some_and(|spell| spell.id == aura.id) && event_flags & TAKE_HARMFUL_PERIODIC == 0 {
        return Ok(SelfTrigger);
    }
    if !unit.victim && unit.player && aura.attributesex3 & NO_PROC_EQUIP_REQUIREMENT == 0 {
        return Ok(Later(
            "execution.spells.UnitAuraProcHandler_548.proc_equipment_owner",
        ));
    }
    if aura.attributesex3 & ONLY_PROC_OUTDOORS != 0 {
        return Ok(Later(
            "execution.spells.UnitAuraProcHandler_582.IsOutdoors_owner",
        ));
    }
    if aura.attributesex3 & ONLY_PROC_ON_CASTER != 0 {
        let guid = |handle| {
            unit.objects
                .get(handle)?
                .guid
                .ok_or_else(|| refuse("execution.spells.UnitAuraProcHandler_585.holder_guid_owner"))
        };
        if guid(holder.target)? != guid(holder.caster)? {
            return Ok(OtherCaster);
        }
    }
    if is_sit_state(unit.stand) && aura.attributes & ALLOW_WHILE_SITTING == 0 {
        return Ok(Sitting);
    }
    Ok(Later(
        "execution.spells.UnitAuraProcHandler_592.proc_chance_owner",
    ))
}

// UnitAuraProcHandler.cpp:522-526.
fn proc_event_flags(event: Option<&SpellProc>, aura: &SpellTemplate) -> u32 {
    event
        .filter(|v| v.flags != 0)
        .map_or(aura.procflags, |v| v.flags)
}

// Unit.cpp:10155-10162; Unit.h:96-106.
fn is_sit_state(stand: u8) -> bool {
    matches!(stand, 1..=2 | 4..=6)
}

// Unit.cpp:4863-4865,5373-5377. Both the template and proc row count.
pub(super) fn heartbeat_aura(stores: &SourceSpellStores, t: &SpellTemplate) -> bool {
    let event = stores.proc_event(t.id);
    t.procflags & HEARTBEAT != 0 || event.is_some_and(|event| event.flags & HEARTBEAT != 0)
}

impl SourceUnitAuras {
    /// Spell.cpp:3351-3352. The cast-end event has no victim proc.
    pub fn proc_creature_self_cast_end(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SourceSpell,
        objects: &SourceObjects,
        stand: u8,
        observe: impl FnMut(u32, ProcTriggerExit),
    ) -> Result<()> {
        self.proc_bound_cast_end(stores, spell, objects, stand, false, observe)
    }

    pub(in crate::native::source_execution) fn proc_bound_cast_end(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SourceSpell,
        objects: &SourceObjects,
        stand: u8,
        player: bool,
        observe: impl FnMut(u32, ProcTriggerExit),
    ) -> Result<()> {
        let t = stores
            .template(spell.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        let [attacker, _] = prepare_masks_for_proc_system(stores, spell, t, 7, None)?;
        self.scan_creature_self_procs(
            stores,
            t,
            [attacker, 0],
            CAST_END,
            ProcUnit {
                objects,
                stand,
                player,
                victim: false,
            },
            observe,
        )
    }

    // Spell.cpp:1544-1552. Targetless non-trap hits have no victim.
    pub(in crate::native::source_execution) fn proc_bound_targetless(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SourceSpell,
        t: &SpellTemplate,
        unit: (&SourceObjects, u8, bool),
        mask: u8,
    ) -> Result<()> {
        let (objects, stand, player) = unit;
        let [attacker, _] = prepare_masks_for_proc_system(stores, spell, t, u32::from(mask), None)?;
        require(
            attacker & ON_TRAP_ACTIVATION == 0,
            "execution.spells.Spell_1551.trap_target_owner",
        )?;
        self.scan_creature_self_procs(
            stores,
            t,
            [attacker, 0],
            0,
            ProcUnit {
                objects,
                stand,
                player,
                victim: false,
            },
            |_, _| {},
        )
    }

    /// Spell.cpp:1190-1206,1321-1331. Bound successful, non-damaging self hit.
    pub(in crate::native::source_execution) fn proc_bound_self_hit(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SourceSpell,
        alive: bool,
        unit: (&SourceObjects, u8, bool),
        observe: impl FnMut(u32, ProcTriggerExit),
    ) -> Result<()> {
        let (objects, stand, player) = unit;
        let t = stores
            .template(spell.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        let target = spell
            .unique_targets
            .first()
            .ok_or_else(|| refuse("execution.spells.Spell_1194.proc_target_owner"))?;
        let [attacker, victim] = prepare_masks_for_proc_system(
            stores,
            spell,
            t,
            u32::from(target.effect_mask),
            Some(target.unit),
        )?;
        let trigger = spell
            .trigger_data
            .as_ref()
            .ok_or_else(|| refuse("execution.spells.Spell_1202.trigger_data_owner"))?;
        if (attacker | victim) & NEGATIVE_TRIGGER != 0
            && target.hit_mask & trigger.negative_effect_mask == 0
            || attacker | victim == 0
        {
            return Ok(());
        }
        require(
            spell.spell_script.is_none(),
            "execution.spells.Spell_1324.OnHit_owner",
        )?;
        if !trigger.can_trigger {
            return Ok(());
        }
        self.scan_creature_self_procs(
            stores,
            t,
            [attacker, if alive { victim } else { 0 }],
            // Spell.cpp:1326-1331; UnitAuraProcHandler.cpp:239-274.
            // The bound hit has no miss, block, absorb, or crit.
            NORMAL_HIT,
            ProcUnit {
                objects,
                stand,
                player,
                victim: false,
            },
            observe,
        )
    }

    fn scan_creature_self_procs(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SpellTemplate,
        flags: [u32; 2],
        extra: u32,
        mut unit: ProcUnit<'_>,
        mut observe: impl FnMut(u32, ProcTriggerExit),
    ) -> Result<()> {
        let current = !self.spell_procs_happening;
        self.spell_procs_happening = true;
        for (side, flags) in flags.into_iter().enumerate() {
            if flags == 0 {
                continue;
            }
            unit.victim = side == 1;
            // UnitAuraProcHandler.cpp:415-440. Only roll failures can burn charges.
            for (id, holder) in self.aura_holders() {
                if !holder.ready || holder.deleted {
                    continue;
                }
                let t = stores
                    .template(id.spell())
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                let exit = is_triggered_at_spell_proc_event(
                    stores,
                    t,
                    holder,
                    Some(spell),
                    flags,
                    extra,
                    unit,
                )?;
                observe(t.id, exit);
                if let Some(refusal) = exit.refusal() {
                    return Err(refusal);
                }
            }
        }
        if current {
            self.spell_procs_happening = false;
        }
        Ok(())
    }
}

#[test]
fn targetless_proc_refuses_trap_target_before_scanning() {
    let stores = SourceSpellStores::default();
    let objects = SourceObjects::default();
    let caster = ObjectHandle {
        thread: 12,
        construction: 1,
    };
    let t = SpellTemplate {
        effect1: 26,
        spellfamilyname: 9,
        spellfamilyflags: 4,
        ..SpellTemplate::default()
    };
    let spell = SourceSpell::new(1, &t, caster, SpellTriggerFlags(1), 0).unwrap();
    let mut auras = SourceUnitAuras::default();
    assert_eq!(
        auras
            .proc_bound_targetless(&stores, &spell, &t, (&objects, 0, true), 1)
            .unwrap_err()
            .id,
        "execution.spells.Spell_1551.trap_target_owner"
    );
    assert!(!auras.spell_procs_happening);
}

#[test]
fn ready_poison_proc_rejects_helpful_cast_but_reaches_melee_checks() {
    let mut stores = SourceSpellStores::default();
    let target = ObjectHandle {
        thread: 7,
        construction: 1,
    };
    let poison = SpellTemplate {
        id: 7276,
        attributes: 0x50,
        procflags: 0x14,
        effect1: 6,
        effectimplicittargeta1: 1,
        effectapplyauraname1: 42,
        ..Default::default()
    };
    let melee = SpellTemplate {
        id: 1,
        dmgclass: 2,
        ..Default::default()
    };
    let mut holder = SourceAuraHolder::new(&poison, target, 0, 0, -1).unwrap();
    holder.ready = true;
    let mut auras = SourceUnitAuras::default();
    auras
        .store_aura_holder(poison.id, &mut Some(holder))
        .unwrap();
    let helpful_cast = SourceSpell::new(1, &poison, target, SpellTriggerFlags(1), 0).unwrap();
    let melee_cast = SourceSpell::new(2, &melee, target, SpellTriggerFlags(1), 0).unwrap();
    assert_eq!(
        prepare_masks_for_proc_system(&stores, &helpful_cast, &poison, 7, None).unwrap(),
        [0x400, 0x800]
    );
    assert_eq!(
        prepare_masks_for_proc_system(&stores, &melee_cast, &melee, 7, None).unwrap(),
        [0x0040_0010, 0x20]
    );
    stores.insert_template(poison).unwrap();
    stores.insert_template(melee).unwrap();
    let mut lookups = 0;
    auras
        .proc_creature_self_cast_end(
            &stores,
            &helpful_cast,
            &SourceObjects::default(),
            0,
            |_, _| lookups += 1,
        )
        .unwrap();
    assert_eq!(lookups, 1);
    assert!(!auras.spell_procs_happening);
    assert_eq!(
        auras
            .proc_creature_self_cast_end(
                &stores,
                &melee_cast,
                &SourceObjects::default(),
                0,
                |_, _| lookups += 1
            )
            .unwrap_err()
            .id,
        "execution.spells.UnitAuraProcHandler_592.proc_chance_owner"
    );
    assert_eq!(lookups, 2);
}

#[cfg(test)]
#[path = "procs_tests.rs"]
mod tests;
