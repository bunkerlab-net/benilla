// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Spell.cpp:533-784,1773-2697. Target fill for the immediate creature self-cast.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SelfTargetSelection {
    pub effect: usize,
    pub implicit_type: SpellTargetImplicitType,
    pub modes: [u32; 2],
    pub dispatched: [bool; 2],
    pub had_single: bool,
    pub caster_default: bool,
    pub has_candidate: [bool; 2],
}

pub struct SelfSpellTargeting<'a> {
    pub stores: &'a SourceSpellStores,
    pub targets: &'a SourceSpellTargetMgr,
    pub objects: &'a SourceObjects,
    pub unit: &'a SpellUnitState,
    pub level: u32,
    pub immunities: &'a CreatureImmunityState,
    pub auras: &'a SourceUnitAuras,
    pub relations: &'a dyn SourceRelationOwner,
}

impl SelfSpellTargeting<'_> {
    pub fn fill(
        &self,
        s: &mut SourceSpell,
        t: &SpellTemplate,
        mut observe: impl FnMut(SelfTargetSelection),
    ) -> Result<()> {
        for (i, effect) in t.effects().iter().enumerate() {
            if effect.effect == 0 {
                continue;
            }
            let data = self
                .targets
                .get_spell_targeting_data(t.id)
                .map_err(targeting_error)?;
            let (selection, candidates) =
                self.select(s, t, i, data.implicit_type[i], data.ignored_targets[i])?;
            observe(selection);
            self.fill_unit_targets(s, t, i, data, candidates)?;
        }
        require(
            t.attributesex2 & 0x0020_0000 == 0 && t.attributesex & 0x0400_0000 == 0,
            "execution.spells.Spell_704.all_target_requirements_owner",
        )
    }

    fn select(
        &self,
        s: &SourceSpell,
        t: &SpellTemplate,
        effect: usize,
        implicit_type: SpellTargetImplicitType,
        ignored: [bool; 2],
    ) -> Result<(SelfTargetSelection, [Option<ObjectHandle>; 2])> {
        use SpellTargetImplicitType::{SpecialUnit, Unit};
        let e = t.effects()[effect];
        let modes = [e.target_a, e.target_b];
        require(
            matches!(implicit_type, Unit | SpecialUnit),
            "execution.spells.Spell_550.effect_target_type_owner",
        )?;
        // Bind the old ordinary self path only when explicit modes exist.
        require(
            implicit_type == SpecialUnit || modes != [0; 2],
            "execution.spells.Spell_581.FillFromTargetFlags_or_default_owner",
        )?;
        let mut candidates = [None; 2];
        let mut dispatched = [false; 2];
        let mut had_single = false;
        for (side, mode) in modes.into_iter().enumerate() {
            if mode == 0 || ignored[side] {
                continue;
            }
            if implicit_type == SpecialUnit {
                let info = target_info(t.id, effect, side, mode)?;
                had_single |=
                    info.target_type == Unit && info.enumerator == SpellTargetEnumerator::Single;
                if side == 1 && info.enumerator == SpellTargetEnumerator::Aoe {
                    continue;
                }
            }
            candidates[side] = self.set_target_map(s, t, effect, mode)?;
            dispatched[side] = true;
        }
        let caster_default = implicit_type == SpecialUnit && !had_single;
        if caster_default {
            // Every ported SetTargetMap arm selects at most one single unit.
            candidates[0] = Some(s.caster);
        }
        Ok((
            SelfTargetSelection {
                effect,
                implicit_type,
                modes,
                dispatched,
                had_single,
                caster_default,
                has_candidate: candidates.map(|unit| unit.is_some()),
            },
            candidates,
        ))
    }

    fn set_target_map(
        &self,
        s: &SourceSpell,
        t: &SpellTemplate,
        effect: usize,
        mode: u32,
    ) -> Result<Option<ObjectHandle>> {
        // Spell.cpp:1779-1780,7495-7591. The bound single-unit modes don't use
        // radius, cone, or chain count. Preserve the lookups and callback gates.
        let radius = [
            t.effectradiusindex1,
            t.effectradiusindex2,
            t.effectradiusindex3,
        ][effect];
        if radius != 0 {
            self.stores.dbc("SpellRadius.dbc", radius)?;
        } else {
            self.stores.range_entry(t.rangeindex)?;
        }
        if let Some(caster) = s.affective_caster {
            require(
                world_object_spell_mod_owner(self.objects, caster, self.unit.totem)?.is_none(),
                "execution.spells.Spell_7551.ApplySpellMod_radius_owner",
            )?;
        }
        require(
            t.spellfamilyname != 0 || t.id != 28241,
            "execution.spells.Spell_7570.GetSpellAuraHolder_owner",
        )?;
        require(
            !s.script_present,
            "execution.spells.Spell_7583.OnRadiusCalculate_owner",
        )?;
        self.stores.cone(self.stores.first_rank(t.id));
        let unit = match mode {
            1 => Some(s.caster), // Spell.cpp:2159-2161.
            20 | 33 | 34 => {
                return Err(refuse(
                    "execution.spells.Spell_2389.FillRaidOrPartyTargets_owner",
                ))
            }
            21 | 57 => match s.targets.unit {
                // Spell.cpp:2397-2405.
                Some(unit) if unit_can_assist_spell(s.caster, unit, self.relations)? => Some(unit),
                _ => None,
            },
            _ => return Err(refuse("execution.spells.Spell_1785.SetTargetMap_owner")),
        };
        // Spell.cpp:2693-2696. Exclusion runs after the arm, even for target B.
        Ok(unit.filter(|&unit| mode == 1 || unit != s.caster || t.attributesex & 0x0008_0000 == 0))
    }

    fn fill_unit_targets(
        &self,
        s: &mut SourceSpell,
        t: &SpellTemplate,
        i: usize,
        data: &SpellTargetingData,
        candidates: [Option<ObjectHandle>; 2],
    ) -> Result<()> {
        let e = t.effects()[i];
        for (side, mut unit) in candidates.into_iter().enumerate() {
            if data.ignored_targets[i][side] {
                continue;
            }
            let mode = [e.target_a, e.target_b][side];
            let info = target_info(t.id, i, side, mode)?;
            if let Some(candidate) = unit {
                require(
                    candidate == s.caster,
                    "execution.spells.SetTargetMap_nonself_owner",
                )?;
                if !self.check_self_target(s, t, e.target_a, mode)? {
                    unit = None;
                } else {
                    // Spell.cpp:7614-7646. A one-element list cannot exceed a positive cap.
                    require(
                        matches!(
                            data.filtering_scheme[i][side],
                            SpellTargetFilterScheme::Random
                                | SpellTargetFilterScheme::Closest
                                | SpellTargetFilterScheme::Furthest
                        ),
                        "execution.spells.Spell_749.FilterTargetMap_owner",
                    )?;
                    require(
                        !self.unit.player,
                        "execution.spells.Spell_756.GetBeneficiaryPlayer_owner",
                    )?;
                }
            }
            if unit.is_none()
                && matches!(
                    info.target_type,
                    SpellTargetImplicitType::Unit | SpellTargetImplicitType::Player
                )
            {
                require(
                    t.attributesex & 0x0400_0000 == 0,
                    "execution.spells.Spell_773.SendCastResult_owner",
                )?;
            }
            if unit.is_some() {
                self.add_self_unit_target(s, t, data.target_mask[i][side])?;
            }
        }
        Ok(())
    }

    fn check_self_target(
        &self,
        s: &SourceSpell,
        t: &SpellTemplate,
        mode_a: u32,
        mode: u32,
    ) -> Result<bool> {
        require(
            mode_a == 1 || t.targetcreaturetype == 0 || matches!(t.id, 2641 | 23356),
            "execution.spells.Spell_6918.GetCreatureTypeMask_owner",
        )?;
        // Spell.cpp:6956-7046. The checked self-caster takes no distance or LOS read.
        if t.attributesex3 & 0x1000 != 0 && self.auras.vis_flags & 1 == 0 {
            return Ok(false);
        }
        if !matches!(mode, 1 | 5)
            && (self.unit.untargetable || t.attributesex & 0x100 != 0 && self.unit.combat)
        {
            return Ok(false);
        }
        require(
            t.attributesex3 & 0x8000_0000 == 0,
            "execution.spells.Spell_7072.IsAOEImmune_owner",
        )?;
        if (!self.unit.player && t.attributesex3 & 0x100 != 0 && mode != 1)
            || (t.maxtargetlevel != 0 && self.level > t.maxtargetlevel)
        {
            return Ok(false);
        }
        require(
            !s.script_present,
            "execution.spells.Spell_7093.OnCheckTarget_owner",
        )?;
        Ok(true)
    }

    fn add_self_unit_target(
        &self,
        s: &mut SourceSpell,
        t: &SpellTemplate,
        effect_mask: u8,
    ) -> Result<()> {
        let mask = effect_mask & !(s.partial_application_mask as u8);
        let mut hit = 0;
        for j in 0..3 {
            if mask & (1 << j) != 0 && !self.immunities.immune_to_self_effect(t, j, self.auras)? {
                hit |= 1 << j;
            }
        }
        if t.attributesex4 & 0x0000_0800 != 0 && hit != mask {
            hit = 0;
        }
        if let Some(existing) = s.unique_targets.iter_mut().find(|v| v.unit == s.caster) {
            existing.effect_mask |= mask;
            existing.hit_mask |= hit;
            return Ok(());
        }
        let duration = creature_self_aura_duration(self.stores, t)?;
        // Unit.cpp:2900-2925. Positive self aura: immunity, no hit roll.
        require(
            positive_unit_spell(self.stores, t, u32::from(mask), Some((s.caster, s.caster)))?,
            "execution.spells.Unit_2927.SpellHitResult_negative_owner",
        )?;
        let missed = self.immunities.immune_to_self_spell(
            self.stores,
            t,
            u32::from(mask),
            s.caster,
            self.auras,
        )?;
        require(
            !self.unit.player_controlled && !matches!(t.id, 36924 | 31480),
            "execution.spells.Spell_1027.diminishing_owner",
        )?;
        s.unique_targets.push(SourceSpellTarget {
            unit: s.caster,
            effect_mask: mask,
            hit_mask: hit,
            processed_mask: 0,
            processed: false,
            duration,
            missed,
        });
        Ok(())
    }
}

fn target_info(
    spell: u32,
    effect: usize,
    right: usize,
    value: u32,
) -> Result<&'static SpellTargetInfo> {
    SPELL_TARGET_INFO_TABLE.get(value as usize).ok_or_else(|| {
        targeting_error(SpellTargetError::TargetTableIndexUndefined {
            spell,
            effect,
            right,
            value,
        })
    })
}

fn targeting_error(error: SpellTargetError) -> Refusal {
    refuse(&format!("execution.spells.targeting.{error}"))
}

#[cfg(test)]
#[path = "targeting_tests.rs"]
mod tests;
