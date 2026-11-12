// SPDX-License-Identifier: GPL-3.0-or-later
//! Map.cpp:1023-1030. Reuse the existing map-end phase, not another update loop.
use super::*;

pub struct SourceDbScriptMapEnd<'a, 'definitions, O, M> {
    pub scripts: &'a mut SourceDbScriptExecutor<'definitions>,
    pub units: &'a mut SourceUnits,
    pub owner: &'a mut O,
    pub other_phases: &'a mut M,
}
impl<O: DbScriptExecutionOwner, M: SourceMapEndWork> SourceMapEndWork
    for SourceDbScriptMapEnd<'_, '_, O, M>
{
    fn map_end_phase(
        &mut self,
        phase: MapEndPhase,
        map: MapIdentity,
        diff: u32,
        objects: &mut SourceObjects,
    ) -> Result<()> {
        require(
            map == self.scripts.map,
            "execution.db_script_executor.map_phase_binding",
        )?;
        if phase == MapEndPhase::Scripts {
            self.scripts.process(&mut DbScriptContext {
                objects,
                units: self.units,
                owner: self.owner,
            })
        } else {
            self.other_phases.map_end_phase(phase, map, diff, objects)
        }
    }
}
impl<O: DbScriptExecutionOwner, M: SourceMapEndWork> WeatherPacketOwner
    for SourceDbScriptMapEnd<'_, '_, O, M>
{
    fn zone_id(&mut self, player: ObjectHandle, objects: &SourceObjects) -> Result<u32> {
        self.other_phases.zone_id(player, objects)
    }
    fn send_weather(&mut self, player: ObjectHandle, payload: [u8; 13]) -> Result<()> {
        self.other_phases.send_weather(player, payload)
    }
}
