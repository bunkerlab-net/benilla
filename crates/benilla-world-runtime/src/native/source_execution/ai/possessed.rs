// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
// PossessedAI.h:41-44 deliberately skips timers, target selection and spell lists.
pub(super) fn update(
    base: &SourceUnitAi,
    objects: &SourceObjects,
    units: &mut SourceUnits,
    owner: &mut impl BuiltinAiExecutionOwner,
) -> Result<()> {
    // UnitAI.cpp:296-299 short-circuits before Unit::UpdateMeleeAttackingState.
    let fields = units.get(objects, base.unit)?;
    if fields.state & UNIT_STATE_MELEE == 0 || base.order != 0 {
        return Ok(());
    }
    // Unit.cpp:652-654 precedes the missing attack-timer/damage/geometry owners.
    if fields.victim.is_none() || fields.casting_non_melee {
        return Ok(());
    }
    owner.update_melee(base.unit, objects, units)
}
