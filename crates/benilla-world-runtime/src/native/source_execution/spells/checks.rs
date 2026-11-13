// SPDX-License-Identifier: GPL-3.0-or-later
//! Spell.cpp:4676-6002. Requests stop at unbound Unit, aura, item and geometry owners.
use super::*;
use SpellCastResult as R;

impl SourceSpell {
    pub fn check_cast(
        &mut self,
        t: &SpellTemplate,
        strict: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<R> {
        macro_rules! check {
            ($c:expr) => {{
                let r = owner.check(self, t, $c)?;
                if r != R::Ok {
                    return Ok(r);
                }
            }};
        }
        // :4678-4690. Cooldown failure precedes strict-only GCD, including aura casts.
        if !self.flags.ignore_cooldowns() && t.attributes & 0x40 == 0 {
            let r = owner.check(self, t, SpellCheck::IsSpellReady)?;
            if r != R::Ok {
                return Ok(if self.triggered_by_aura.is_some() {
                    R::DontReport
                } else {
                    R::NotReady
                });
            }
        }
        if strict && !self.flags.ignore_gcd() {
            check!(SpellCheck::HasGcd);
        }
        let c = owner.unit(self.caster)?;
        if !c.gameobject {
            if c.player && !c.alive && t.attributes & 0x00800040 == 0 {
                return Ok(R::CasterDead);
            }
            if !self.flags.old_triggered()
                && !c.standing
                && c.player_controlled
                && t.attributes & 0x08000000 == 0
            {
                return Ok(R::NotStanding);
            }
            if !self.flags.old_triggered() && c.player && c.battleground_wait_leave {
                return Ok(R::DontReport);
            }
            if (!self.flags.old_triggered() || self.triggered_by_aura.is_some())
                && t.attributes & 0x10000000 != 0
                && c.combat
            {
                return Ok(R::AffectingCombat);
            }
            if c.player && !c.gm {
                // :4709-4719. Even the policy is a real World/VMAP input.
                if owner.indoor_policy()? && t.attributes & 0xc000 != 0 {
                    check!(SpellCheck::IsOutdoors);
                }
            }
            if strict && !self.flags.old_triggered() {
                check!(SpellCheck::Shapeshift);
                if t.attributes & 0x20000 != 0 && !c.stealth {
                    return Ok(R::OnlyStealthed);
                }
            }
            if !self.flags.old_triggered()
                && t.attributesex & 0x500000 != 0
                && (self.targets.unit.is_none() || self.targets.unit != c.combo_target)
            {
                return Ok(if c.class == 1 {
                    R::CasterAuraState
                } else {
                    R::NoComboPoints
                });
            }
        }
        if c.player {
            if c.moving
                && (!c.falling_far || t.effect1 != 84)
                && (self.auto_repeat || t.aurainterruptflags & 0x40000 != 0)
            {
                return Ok(R::Moving);
            }
            // The duplicated moving block at :4752-4762 has no intervening mutation.
            let aura = t.effects().iter().any(|e| e.effect == 6 && e.aura == 36);
            if t.spellfamilyname == 7 && aura {
                check!(SpellCheck::OverrideScript(3655));
            }
            if matches!(t.spellfamilyname, 7 | 6 | 11 | 10)
                && t.effects()
                    .iter()
                    .any(|e| e.effect == 10 || e.effect == 38 || e.effect == 6 && e.aura == 8)
            {
                check!(SpellCheck::OverrideScript(4327));
            }
            if t.spellfamilyname == 4 && aura {
                check!(SpellCheck::OverrideScript(3654));
            }
        }
        // :4803 calls the table for all slots, even junk on an empty effect.
        let affected = check_effect_mask(t)?;
        let self_targeting =
            self.targets.unit.is_none() && t.effects().iter().any(|e| e.target_a == 1);
        let target = self.targets.unit.or(self_targeting.then_some(self.caster));
        let scripted = t
            .effects()
            .iter()
            .any(|e| e.effect != 0 && e.target_a == 38);
        if let Some(target) = target.filter(|_| !scripted) {
            let u = owner.unit(target)?;
            let ignore = t.attributesex3 & 0x10000000 != 0;
            if !self.flags.old_triggered()
                && (t.attributesex3 & 0x1000 != 0 || t.id == 2584)
                && u.alive
            {
                return Ok(R::TargetNotDead);
            }
            if t.attributesex & 0x4000 != 0 && !u.player && u.totem {
                return Ok(R::Immune);
            }
            let noncaster = target != self.caster && !caster_source_only(t);
            if noncaster {
                if !ignore && t.attributes & 0x01000000 == 0 && u.taxi && t.id != 7720 {
                    return Ok(R::BadTargets);
                }
                if !self.flags.old_triggered()
                    && !matches!(t.id, 8733 | 8735 | 24742)
                    && t.attributesex2 & 4 == 0
                {
                    owner.line_of_sight(self.caster, target)?;
                }
                if c.player {
                    if !self.flags.old_triggered() && t.attributesex2 & 8 == 0 {
                        check!(SpellCheck::LowLevelRank);
                    }
                    if t.attributesex2 & 0x40 != 0 {
                        check!(SpellCheck::Tapped);
                    }
                    check!(SpellCheck::Visible);
                }
                if strict && t.attributesex3 & 0x100 != 0 && !u.player && !area_spell(t)? {
                    return Ok(R::BadTargets);
                }
            } else if target == self.caster {
                if c.player && self.targets.mask == 0 && t.effectimplicittargeta2 == 6 {
                    owner.selection_target(self)?;
                }
                if t.spellfamilyname == 5 && t.spelliconid == 16 {
                    return Ok(R::BadTargets);
                }
            }
            if !self_targeting
                && (self.targets.mask == 0 || target == self.caster)
                && t.attributesex & 0x80000 != 0
            {
                return Ok(R::BadTargets);
            }
            if !self_targeting && !ignore && u.untargetable {
                return Ok(R::BadTargets);
            }
            if noncaster && t.targetcreaturetype != 0 && !matches!(t.id, 2641 | 23356) {
                check!(SpellCheck::TargetCreatureType);
            }
            if !ignore {
                check!(SpellCheck::PositiveImmunity(if self_targeting {
                    self_effect_mask(t)
                } else {
                    affected
                }));
            }
            // Geometry checks preserve source ordering, including duplicate back-facing guards.
            if t.attributesex2 & 0x100000 != 0
                && t.attributesex & 0x200 != 0
                && !(t.spellfamilyname == 7 && t.spellfamilyflags & 0x20000 != 0)
            {
                check!(SpellCheck::FacingBack);
            }
            if t.attributesserverside & 8 != 0 {
                check!(SpellCheck::FacingBack);
            }
            if t.attributes == 0x150010 {
                check!(SpellCheck::FacingFront);
            }
            if noncaster && t.attributesex & 0x100 != 0 && u.combat {
                return Ok(R::TargetAffectingCombat);
            }
            check!(SpellCheck::SpiritOfRedemption);
            if aura_apply_only(t, affected)
                && !area_spell(t)?
                && !t.effects().iter().any(|e| matches!(e.effect, 35 | 119))
                && !self_targeting
            {
                check!(SpellCheck::StrongerAura(affected));
            }
            if t.effect1 == 38 && t.effect2 == 0 && t.effect3 == 0 {
                check!(SpellCheck::Dispel);
            }
            if t.maxtargetlevel != 0 {
                check!(SpellCheck::TargetLevel);
            }
        }
        for (i, e) in t.effects().into_iter().enumerate() {
            match e.target_a {
                1..=4 | 27 => {}
                5 => check!(SpellCheck::PetTarget(i as u8)),
                ty if check_target(ty)? => {
                    if target.is_none() {
                        return Ok(R::BadTargets);
                    }
                    if e.target_a != 25 {
                        check!(SpellCheck::TargetFilter(i as u8));
                    }
                }
                _ => {}
            }
        }
        // :5096-5106 is unconditional, including triggered, passive and self casts.
        let location = owner.zone_area(self.caster)?;
        let r = owner.check(self, t, SpellCheck::AllowedInLocation(location))?;
        if r != R::Ok {
            return Ok(if self.flags.old_triggered() {
                R::DontReport
            } else {
                r
            });
        }
        self.check_cast_location_suffix(t, strict, c.player, owner)
    }
    // Shared by CheckCast and the definition-only post-location census.
    pub(super) fn check_cast_location_suffix(
        &mut self,
        t: &SpellTemplate,
        strict: bool,
        player: bool,
        owner: &mut (impl SpellExecutionOwner + ?Sized),
    ) -> Result<R> {
        macro_rules! check {
            ($c:expr) => {{
                let r = owner.check(self, t, $c)?;
                if r != R::Ok {
                    return Ok(r);
                }
            }};
        }
        if player {
            let live = owner.unit(self.caster)?;
            if live.mounted && !self.flags.old_triggered() && t.attributes & 0x01000040 == 0 {
                return Ok(if live.taxi {
                    R::NotOnTaxi
                } else {
                    R::NotMounted
                });
            }
        }
        if t.attributes & 0x40 == 0 {
            check!(SpellCheck::Items);
        }
        if t.requiresspellfocus != 0 {
            check!(SpellCheck::Focus);
        }
        if !self.flags.old_triggered()
            && self.triggered_by_aura.is_none()
            && (strict || self.cast_time != 0)
        {
            check!(SpellCheck::Range(strict));
        }
        if !self.flags.ignore_costs() && !self.flags.old_triggered() {
            check!(SpellCheck::Power(strict));
            if !self.flags.ignore_caster_aura() {
                check!(SpellCheck::CasterAuras);
            }
        }
        if self.flags.channel_only() {
            self.partial_application_mask = 7;
            return Ok(R::Ok);
        }
        self.partial_application_mask |= self.effect_skip_mask;
        for (i, e) in t.effects().into_iter().enumerate() {
            if self.effect_skip_mask & (1 << i) != 0 {
                continue;
            }
            // :5193-5759. Only cases with source checks request their dependent operands.
            if effect_has_check(t, e, i) {
                check!(SpellCheck::Effect(i as u8));
            }
        }
        for (i, e) in t.effects().into_iter().enumerate() {
            if matches!(e.effect, 6 | 27 | 35 | 119)
                && matches!(e.aura, 2 | 6 | 64 | 67 | 78 | 104 | 127 | 128 | 177)
            {
                check!(SpellCheck::Aura(i as u8));
            }
        }
        if self.targets.mask & 0x1000 != 0 {
            check!(SpellCheck::Trade);
        }
        if player && t.attributesex2 & 0x10000 != 0 {
            check!(SpellCheck::NoActivePets);
        }
        match t.id {
            7914 => owner.check(self, t, SpellCheck::CaptureSpirit),
            12699 => owner.check(self, t, SpellCheck::ScreecherSpirit),
            _ if self.script_present => owner.check(self, t, SpellCheck::OnCheckCast(strict)),
            _ => Ok(R::Ok),
        }
    }
}
// SpellTargets.cpp:31-97, SpellMgr.h:964-1008. No out-of-bounds C++ table read.
pub fn check_target(target: u32) -> Result<bool> {
    require(
        target < 64,
        "execution.spells.SpellTargetInfoTable_index_undefined",
    )?;
    Ok(matches!(
        target,
        5 | 6 | 21 | 25 | 35 | 37 | 45 | 57 | 58 | 61
    ))
}
pub fn check_effect_mask(t: &SpellTemplate) -> Result<u32> {
    let mut mask = 0;
    for (i, e) in t.effects().into_iter().enumerate() {
        let a = check_target(e.target_a)?;
        // C++ || short-circuits; preserve which undefined lookup is reached.
        if a || check_target(e.target_b)? {
            mask |= 1 << i;
        }
    }
    Ok(mask)
}
fn self_effect_mask(t: &SpellTemplate) -> u32 {
    t.effects()
        .iter()
        .enumerate()
        .filter(|(_, e)| e.target_a == 1)
        .fold(0, |m, (i, _)| m | 1 << i)
}
fn caster_source_only(t: &SpellTemplate) -> bool {
    t.effects().iter().filter(|e| e.effect != 0).all(|e| {
        (e.target_a != 0 || e.target_b != 0)
            && [e.target_a, e.target_b].into_iter().all(|ty| {
                ty == 0 || matches!(ty, 1 | 5 | 20 | 24 | 27 | 32..=34 | 39 | 41..=44 | 52)
            })
    })
}
fn area_spell(t: &SpellTemplate) -> Result<bool> {
    // SpellMgr.h:884-892 casts before each short-circuited predicate.
    // SpellTargetDefines.h:22-90 gives the non-fixed enum range 0..127.
    for e in t.effects() {
        for ty in [e.target_a, e.target_b] {
            require(ty <= 127, "execution.spells.SpellTarget_enum_undefined")?;
            if matches!(
                ty,
                7 | 8 | 15 | 16 | 20 | 24 | 28 | 30 | 31 | 33 | 34 | 37 | 52 | 56 | 61
            ) {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
fn aura_apply_only(t: &SpellTemplate, mask: u32) -> bool {
    let mut nonempty = false;
    for (i, e) in t.effects().into_iter().enumerate() {
        if mask & (1 << i) != 0 && e.effect != 0 {
            nonempty = true;
            if !matches!(e.effect, 6 | 27 | 35 | 119) {
                return false;
            }
        }
    }
    nonempty
}
fn effect_has_check(t: &SpellTemplate, e: SpellEffect, i: usize) -> bool {
    match e.effect {
        3 => t.id == 19938 || t.id == 20577 || matches!(t.spelliconid, 1648 | 156),
        2 => t.spellvisual == 7250 || t.spellfamilyname == 5 && t.spellfamilyflags & 0x200 != 0,
        36 => e.target_a == 5,
        8 | 24 | 28 | 29 | 33 | 43 | 55 | 56 | 57 | 59 | 62 | 69 | 71 | 73 | 85 | 93 | 95 | 96
        | 101 | 109 | 112 => {
            let _ = i;
            true
        }
        _ => false,
    }
}
