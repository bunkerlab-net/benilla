// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;
// NullCreatureAI.cpp:22-25. Empty callbacks are actual overrides, not missing owners.
pub(super) fn construct(
    unit: ObjectHandle,
    objects: &SourceObjects,
    units: &mut SourceUnits,
) -> Result<()> {
    units.get_mut(objects, unit)?.can_enter_combat = false;
    Ok(())
}
