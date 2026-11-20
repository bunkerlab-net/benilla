// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
use super::*;

const HORDE_BELL: u32 = 175885;
const ALLIANCE_BELL: u32 = 176573;
const TARREN_MILL: u32 = 272;
const IRONFORGE_LOCAL: u32 = 809;
const UNDERCITY: u32 = 1497;
const IRONFORGE: u32 = 1537;
const DARNASSUS: u32 = 1657;
const BRILL: u32 = 2118;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GameObjectScript {
    Bells,
    OutdoorPvpNotify,
}

#[derive(Debug)]
pub struct SourceGameObjectAi {
    pub object: ObjectHandle,
    pub script: GameObjectScript,
    payload: GameObjectAiPayload,
}

#[derive(Debug)]
enum GameObjectAiPayload {
    Bells(SourceBellsAi),
    OutdoorPvpNotify,
}

#[derive(Debug)]
pub struct SourceBellsAi {
    pub tolls: u32,
    pub sound: u32,
    pub timer: u32,
    pub plays_to_zone: bool,
}

impl GameObjectScript {
    pub(super) fn construct<
        D: GridDefinitions,
        P: GridProductOwner,
        F: GameObjectFactoryOwner<D, P>,
    >(
        self,
        object: ObjectHandle,
        entry: u32,
        owner: &mut F,
        context: &mut GridFactoryContext<'_, D, P>,
    ) -> Result<SourceGameObjectAi> {
        // GameObjectAI.cpp:22-25 binds the object before the derived initializers.
        // battlegrounds.cpp:219-221 inherits the base constructor.
        if self == Self::OutdoorPvpNotify {
            return Ok(SourceGameObjectAi {
                object,
                script: self,
                payload: GameObjectAiPayload::OutdoorPvpNotify,
            });
        }
        let mut area = || {
            let [x, y, z, _] = context.objects.pose(object)?.0.map(f32::from_bits);
            owner.gameobject_area(object, [x, y, z], context)
        };
        // go_scripts.cpp:113,124-189. Each table reads the current area separately.
        let ai = SourceGameObjectAi {
            object,
            script: self,
            payload: GameObjectAiPayload::Bells(SourceBellsAi {
                tolls: 0,
                sound: match entry {
                    HORDE_BELL => match area()? {
                        UNDERCITY | BRILL | TARREN_MILL => 6595,
                        _ => 6675,
                    },
                    ALLIANCE_BELL => match area()? {
                        IRONFORGE_LOCAL | IRONFORGE => 7234,
                        DARNASSUS => 6674,
                        _ => 6594,
                    },
                    _ => 0,
                },
                timer: 0,
                plays_to_zone: match entry {
                    HORDE_BELL => area()? == UNDERCITY,
                    ALLIANCE_BELL => matches!(area()?, DARNASSUS | IRONFORGE),
                    _ => false,
                },
            }),
        };
        context.objects.set_notify_on_event_state(object, true)?;
        let active = context.set_active_object_state(object, true, owner)?;
        if active.is_some_and(|change| change.membership_changed) {
            owner.observe_gameobject_factory(GameObjectFactoryObservation::ScriptAiActiveInserted);
        }
        Ok(ai)
    }
}

impl SourceGameObjectAi {
    pub fn bells(&self) -> Option<&SourceBellsAi> {
        match &self.payload {
            GameObjectAiPayload::Bells(bells) => Some(bells),
            GameObjectAiPayload::OutdoorPvpNotify => None,
        }
    }

    // GameObjectAI.h:60. Both scripts inherit the empty implementation.
    pub(super) fn just_spawned(&self) {}

    // GameObjectAI.h:55,65. OutdoorPvpNotify inherits both empty hooks.
    pub(in crate::native::source_execution) fn inherits_empty_lifecycle_hooks(&self) -> bool {
        matches!(self.payload, GameObjectAiPayload::OutdoorPvpNotify)
    }

    // battlegrounds.cpp:223-230. Bells inherits GameObjectAI.cpp:31-33.
    pub fn on_use(&self, user: SourceObjectKind) -> Result<()> {
        if matches!(self.payload, GameObjectAiPayload::OutdoorPvpNotify)
            && user == SourceObjectKind::Player
        {
            return Err(refuse(
                "execution.gameobject.battlegrounds_229.GetCachedZoneId_owner",
            ));
        }
        Ok(())
    }

    pub fn on_event_happened(&self, _event: u16, _activate: bool, _resume: bool) -> Result<()> {
        // GameObjectAI.h:48.
        if matches!(self.payload, GameObjectAiPayload::OutdoorPvpNotify) {
            return Ok(());
        }
        Err(refuse(
            "execution.gameobject.go_scripts_191.OnEventHappened_owner",
        ))
    }

    pub fn update_ai(&self, _diff: u32) -> Result<()> {
        // GameObjectAI.h:40.
        if matches!(self.payload, GameObjectAiPayload::OutdoorPvpNotify) {
            return Ok(());
        }
        Err(refuse("execution.gameobject.go_scripts_206.UpdateAI_owner"))
    }

    // GameObjectAI.h:70. Both scripts inherit the empty implementation.
    pub fn receive_ai_event(&self, _event: u32, _misc: u32) {}

    // GameObjectAI.h:80. Both scripts inherit the empty implementation.
    pub fn on_heartbeat(&self) {}
}
