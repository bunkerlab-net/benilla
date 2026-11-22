// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 CombatHandler.cpp:26-61; SpellEffects.cpp:136 maps 78 to EffectUnused.
//! Attack is a learned marker for AttackStart/AttackStop, never a damage cast.
use super::*;

pub(super) fn validate(profile: &Profile, starter: &init::StarterInput) -> Result<(), NativeError> {
    let row = starter.spells.iter().find(|row| row.id == 6603).ok_or(
        NativeError::UnsupportedMechanic("captured melee Attack 6603 marker"),
    )?;
    let expected = |i| match i {
        0 => 6603,
        6 => 0x10,
        18 | 36 | 117 => 1,
        25 => 101,
        58 | 166 => u32::MAX,
        61 => 78,
        167..=169 => 0x3f80_0000,
        _ => 0,
    };
    if row
        .words
        .iter()
        .enumerate()
        .any(|(i, &v)| !(120..=155).contains(&i) && v != expected(i))
        || !matches!(row.duration_ms, None | Some(0))
        || row.previous_rank.is_some()
        || row.talent
        || row.external_initial_hooks
        || !matches!(&row.learned_spells, init::Rows::Complete(rows) if rows.is_empty())
    {
        return Err(NativeError::UnsupportedMechanic(
            "captured melee exact Attack 6603 marker",
        ));
    }
    // The initializer and progression host assemble this list from active learned rows.
    // Keep cast admission unavailable: the marker selects the existing melee operation.
    if !profile
        .ability(6603)
        .is_some_and(|ability| matches!(ability.admission, AbilityAdmission::Unavailable { .. }))
    {
        return Err(NativeError::UnsupportedMechanic(
            "captured melee learned Attack 6603",
        ));
    }
    Ok(())
}
