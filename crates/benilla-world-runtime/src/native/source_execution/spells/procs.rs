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

// Unit.cpp:4863-4865,5373-5377. Both the template and proc row count.
pub(super) fn heartbeat_aura(stores: &SourceSpellStores, t: &SpellTemplate) -> bool {
    let event = stores.proc_event(t.id);
    t.procflags & 1 != 0 || event.is_some_and(|event| event.flags & 1 != 0)
}

impl SourceUnitAuras {
    /// Spell.cpp:3351-3352. The cast-end event has no victim proc.
    pub fn proc_creature_self_cast_end(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SourceSpell,
        mut observe_lookup: impl FnMut(),
    ) -> Result<()> {
        let t = stores
            .template(spell.spell)
            .ok_or_else(|| refuse("execution.spells.missing_template"))?;
        let [attacker, _] = prepare_masks_for_proc_system(stores, spell, t, 7, None)?;
        self.scan_creature_self_procs(stores, [attacker, 0], &mut observe_lookup)
    }

    /// Spell.cpp:1190-1206,1321-1331. Bound successful, non-damaging self hit.
    pub fn proc_creature_self_hit(
        &mut self,
        stores: &SourceSpellStores,
        spell: &SourceSpell,
        alive: bool,
        mut observe_lookup: impl FnMut(),
    ) -> Result<()> {
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
        if !trigger.can_trigger
            || (attacker | victim) & NEGATIVE_TRIGGER != 0
                && target.hit_mask & trigger.negative_effect_mask == 0
            || attacker | victim == 0
        {
            return Ok(());
        }
        self.scan_creature_self_procs(
            stores,
            [attacker, if alive { victim } else { 0 }],
            &mut observe_lookup,
        )
    }

    fn scan_creature_self_procs(
        &mut self,
        stores: &SourceSpellStores,
        flags: [u32; 2],
        observe_lookup: &mut impl FnMut(),
    ) -> Result<()> {
        let current = !self.spell_procs_happening;
        self.spell_procs_happening = true;
        for flags in flags.into_iter().filter(|flags| *flags != 0) {
            // UnitAuraProcHandler.cpp:415-424,514-532.
            for (id, holder) in self.aura_holders() {
                if !holder.ready || holder.deleted {
                    continue;
                }
                let t = stores
                    .template(id.spell())
                    .ok_or_else(|| refuse("execution.spells.missing_template"))?;
                let event = stores.proc_event(t.id);
                observe_lookup();
                let event_flags = event
                    .filter(|v| v.flags != 0)
                    .map_or(t.procflags, |v| v.flags);
                // Zero EventProcFlag exits before SpellMgr.cpp:849-850.
                if event_flags == 0 || event_flags & flags == 0 {
                    continue;
                }
                return Err(refuse(
                    "execution.spells.UnitAuraProcHandler_532.IsSpellProcEventCanTriggeredBy_owner",
                ));
            }
        }
        if current {
            self.spell_procs_happening = false;
        }
        Ok(())
    }
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
        .proc_creature_self_cast_end(&stores, &helpful_cast, || lookups += 1)
        .unwrap();
    assert_eq!(lookups, 1);
    assert!(!auras.spell_procs_happening);
    assert_eq!(
        auras
            .proc_creature_self_cast_end(&stores, &melee_cast, || lookups += 1)
            .unwrap_err()
            .id,
        "execution.spells.UnitAuraProcHandler_532.IsSpellProcEventCanTriggeredBy_owner"
    );
    assert_eq!(lookups, 2);
}
