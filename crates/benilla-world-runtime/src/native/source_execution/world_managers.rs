// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Called by WorldWork, never an alternative world scheduler or timer loop.
use super::*;

pub struct WorldManagerContext<'a> {
    pub thread: u64,
    pub corpses: &'a mut SourceCorpseRegistry,
    pub objects: &'a mut SourceObjects,
    pub conversion: CorpseConversion<'a>,
}
impl WorldManagerContext<'_> {
    /// SourceWorld owns the 20-minute conditional and Reset, World.cpp:1655-1659.
    /// Call from WorldWork::remove_old_corpses, not WorldWork::phase(Corpses).
    pub fn sweep(&mut self, owner: &mut impl CorpseConversionWork) -> Result<()> {
        require(
            self.thread == self.conversion.thread,
            "execution.managers.corpse_thread_binding",
        )?;
        self.corpses
            .remove_old_corpses(self.objects, &mut self.conversion, owner)
    }
}
