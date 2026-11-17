// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! SpellMgr.h:1054-1287. Explicit unit targets retain the live reaction dependency.
use super::*;

/// `units` is absent for definition-only calls, or contains actual unit identities.
pub fn positive_unit_effect(
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    index: usize,
    units: Option<(ObjectHandle, ObjectHandle)>,
) -> Result<bool> {
    positive_effect(stores, t, index, units, false)
}
fn positive_effect(
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    index: usize,
    units: Option<(ObjectHandle, ObjectHandle)>,
    recursive: bool,
) -> Result<bool> {
    let e = t.effects().get(index).copied().ok_or_else(|| {
        refuse("execution.spells.SpellMgr_1161.effect_index_out_of_bounds_undefined")
    })?;
    match e.effect {
        123 | 16 | 5 | 24 | 34 => return Ok(true),
        129 => return Ok(false),
        3 if t.id == 18153 => return Ok(true),
        6 | 128 if e.aura == 4 && t.id == 13139 => return Ok(false),
        _ => {}
    }
    // IsPositiveEffectTargetMode, :1115-1133. The recursive child call does
    // not repeat the effect exceptions in IsPositiveEffect.
    positive_target_mode(stores, t, index, units, recursive)
}
fn positive_target_mode(
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    index: usize,
    units: Option<(ObjectHandle, ObjectHandle)>,
    recursive: bool,
) -> Result<bool> {
    let e = t.effects()[index];
    if matches!(e.effect, 32 | 64) {
        if !recursive && e.trigger != 0 && e.trigger != t.id {
            if let Some(child) = stores.template(e.trigger) {
                for i in 0..3 {
                    if !positive_target_mode(stores, child, i, units, true)? {
                        return Ok(false);
                    }
                }
            }
        }
        return Ok(true);
    }
    let filter = |target: u32, kind| {
        SPELL_TARGET_INFO_TABLE
            .get(target as usize)
            .is_some_and(|info| info.filter == kind)
    };
    use SpellTargetFilter::*;
    if e.target_a == 0 && e.target_b == 0
        || filter(e.target_a, Helpful)
        || filter(e.target_b, Helpful)
        || filter(e.target_a, Script)
        || filter(e.target_b, Script)
    {
        return Ok(true);
    }
    if filter(e.target_a, Harmful) || filter(e.target_b, Harmful) {
        return Ok(t.attributes & 0x40 != 0);
    }
    if filter(e.target_a, Neutral) && filter(e.target_b, Neutral) {
        let target = if e.target_b != 0 {
            e.target_b
        } else {
            e.target_a
        };
        if matches!(target, 9 | 17 | 22 | 46 | 53 | 63)
            || SPELL_TARGET_INFO_TABLE[target as usize].target_type != SpellTargetImplicitType::Unit
            || units.is_none_or(|(caster, target)| caster == target)
        {
            return Ok(true);
        }
        return Err(refuse(
            "execution.spells.SpellMgr_1105.WorldObject_CanAttackSpell_owner",
        ));
    }
    Ok(true)
}
pub fn positive_unit_spell(
    stores: &SourceSpellStores,
    t: &SpellTemplate,
    mask: u32,
    units: Option<(ObjectHandle, ObjectHandle)>,
) -> Result<bool> {
    for (i, e) in t.effects().iter().enumerate() {
        if e.effect != 0 && mask & (1 << i) != 0 && !positive_unit_effect(stores, t, i, units)? {
            return Ok(false);
        }
    }
    Ok(true)
}
/// SpellMgr.h:75-109. Empty selected slots do not constitute an aura.
pub fn aura_apply_effects(t: &SpellTemplate, mask: u32) -> bool {
    let mut empty = 0;
    for (i, e) in t.effects().iter().enumerate() {
        let current = 1 << i;
        if e.effect != 0 {
            if mask & current != 0 && !matches!(e.effect, 6 | 27 | 35 | 119) {
                return false;
            }
        } else {
            empty |= current;
        }
    }
    mask & !empty != 0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn positivity_preserves_effect_exceptions_and_target_mode_precedence() {
        let stores = SourceSpellStores::default();
        let mut t = SpellTemplate {
            effect1: 6,
            effectimplicittargeta1: 1,
            ..Default::default()
        };
        assert!(positive_unit_spell(&stores, &t, 7, None).unwrap());
        t.effectimplicittargeta1 = 6;
        assert!(!positive_unit_spell(&stores, &t, 7, None).unwrap());
        t.attributes = 0x40;
        assert!(positive_unit_spell(&stores, &t, 7, None).unwrap());
        t.effect1 = 129;
        assert!(!positive_unit_spell(&stores, &t, 7, None).unwrap());
        t.effect1 = 5;
        t.attributes = 0;
        assert!(positive_unit_spell(&stores, &t, 7, None).unwrap());
        t.effect1 = 6;
        t.effectapplyauraname1 = 4;
        t.id = 13139;
        t.effectimplicittargeta1 = 1;
        assert!(!positive_unit_spell(&stores, &t, 7, None).unwrap());
    }
    #[test]
    fn neutral_unit_self_cast_returns_before_live_reaction_dependency() {
        let stores = SourceSpellStores::default();
        let t = SpellTemplate {
            effect1: 6,
            effectimplicittargeta1: 25,
            ..Default::default()
        };
        let caster = ObjectHandle {
            thread: 6,
            construction: 1,
        };
        let other = ObjectHandle {
            thread: 6,
            construction: 2,
        };
        assert!(positive_unit_spell(&stores, &t, 7, Some((caster, caster))).unwrap());
        assert!(positive_unit_spell(&stores, &t, 7, None).unwrap());
        assert_eq!(
            positive_unit_spell(&stores, &t, 7, Some((caster, other)))
                .unwrap_err()
                .id,
            "execution.spells.SpellMgr_1105.WorldObject_CanAttackSpell_owner"
        );
    }
    #[test]
    fn positivity_effect_bounds_and_aura_empty_mask_keep_source_limits() {
        let stores = SourceSpellStores::default();
        let t = SpellTemplate {
            effect1: 6,
            ..Default::default()
        };
        assert!(positive_unit_effect(&stores, &t, 2, None).unwrap());
        assert_eq!(
            positive_unit_effect(&stores, &t, 3, None).unwrap_err().id,
            "execution.spells.SpellMgr_1161.effect_index_out_of_bounds_undefined"
        );
        assert!(aura_apply_effects(&t, 1));
        assert!(!aura_apply_effects(&t, 0));
        assert!(!aura_apply_effects(&t, 2));
    }
}
