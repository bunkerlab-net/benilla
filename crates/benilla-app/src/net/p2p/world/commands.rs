// SPDX-License-Identifier: GPL-3.0-or-later
//! Translate actual app input; query panels never grant rewards or consume simulation ticks.
use super::*;
use crate::net::ClientCommand;

pub(crate) struct Received {
    pub events: Vec<SessionEvent>,
    pub logout: bool,
}
impl Controller {
    pub(crate) fn receive(&mut self, command: ClientCommand) -> Result<Received, Error> {
        // Logout is accepted even when disk backpressure has paused gameplay.
        if matches!(command, ClientCommand::Logout | ClientCommand::ForceLogout) {
            return Ok(Received {
                events: vec![],
                logout: true,
            });
        }
        self.ensure_input()?;
        let mut events = Vec::new();
        let request = match command {
            ClientCommand::Move {
                pos,
                orientation,
                transport: None,
                flags,
                pitch,
                jump,
                ..
            } => {
                if flags & 0x200 != 0
                    || !pitch.is_finite()
                    || pitch.abs() > std::f32::consts::PI
                    || jump.is_some_and(|j| {
                        [j.zspeed, j.cos_angle, j.sin_angle, j.xy_speed]
                            .iter()
                            .any(|x| !x.is_finite() || x.abs() > 100.0)
                    })
                {
                    return Err(Error::InvalidMovement);
                }
                self.movement(pos, orientation)?;
                self.correct_blocked_movement(&mut events);
                None
            }
            ClientCommand::CastSpell { spell_id, target } => Some(match target {
                None => UiRequest::CastSelf { ability: spell_id },
                Some(guid) if guid == self.player_guid() => {
                    UiRequest::CastSelf { ability: spell_id }
                }
                Some(guid) => UiRequest::CastAtNpc {
                    ability: spell_id,
                    target: self.projection.capture_npc(guid)?,
                },
            }),
            ClientCommand::AttackSwing { guid } => {
                let target = self.projection.capture_npc(guid)?;
                if self.autoattack != Some(target) {
                    self.stop_attack(&mut events);
                    self.autoattack = Some(target);
                }
                None
            }
            ClientCommand::AttackStop => {
                self.stop_attack(&mut events);
                None
            }
            ClientCommand::QuestgiverAccept { npc, quest } => Some(UiRequest::AcceptQuest {
                quest,
                giver: self.projection.capture_npc(npc)?,
            }),
            ClientCommand::QuestgiverChooseReward {
                npc,
                quest,
                choice: 0,
            } => Some(UiRequest::TurnInQuest {
                quest,
                giver: self.projection.capture_npc(npc)?,
            }),
            ClientCommand::Loot { guid } => {
                Some(UiRequest::OpenLoot(self.projection.capture_npc(guid)?))
            }
            ClientCommand::AutostoreLootItem { slot } => Some(UiRequest::TakeLootItem {
                target: self.projection.commands().capture_loot(&self.world)?,
                slot,
            }),
            ClientCommand::LootMoney => Some(UiRequest::TakeLootMoney(
                self.projection.commands().capture_loot(&self.world)?,
            )),
            ClientCommand::LootRelease { guid } => {
                Some(UiRequest::ReleaseLoot(self.projection.capture_npc(guid)?))
            }
            ClientCommand::ItemQuery { entry, .. } => {
                events.push(self.projection.item_template(entry)?);
                None
            }
            ClientCommand::QuestQuery { quest } => {
                events.push(self.projection.quest_template(quest)?);
                None
            }
            ClientCommand::QuestgiverStatusQuery { npc } => {
                events.push(
                    self.projection
                        .quest_status(&self.world, self.projection.capture_npc(npc)?)?,
                );
                None
            }
            ClientCommand::QuestgiverHello { npc } => {
                events.push(
                    self.projection
                        .quest_greeting(&self.world, self.projection.capture_npc(npc)?)?,
                );
                None
            }
            ClientCommand::QuestgiverQuery { npc, quest } => {
                events.push(self.projection.quest_detail(
                    &self.world,
                    quest,
                    self.projection.capture_npc(npc)?,
                )?);
                None
            }
            ClientCommand::QuestgiverComplete { npc, quest } => {
                events.push(self.projection.quest_progress(
                    &self.world,
                    quest,
                    self.projection.capture_npc(npc)?,
                )?);
                None
            }
            ClientCommand::QuestgiverRequestReward { npc, quest } => {
                events.push(self.projection.quest_offer(
                    &self.world,
                    quest,
                    self.projection.capture_npc(npc)?,
                )?);
                None
            }
            // Stock movement/selection acknowledgements do not mutate the original simulation.
            ClientCommand::SetActiveMover { guid } if guid == self.player_guid() => None,
            ClientCommand::WorldportAck | ClientCommand::SetSelection { .. } => None,
            // In particular: corpse release/Revive, item/ground/corpse casts and nonzero
            // reward choices are not substituted by the runtime's original shortcuts.
            _ => return Err(Error::UnsupportedCommand),
        };
        if let Some(request) = request {
            self.queue(request)?;
        }
        Ok(Received {
            events,
            logout: false,
        })
    }
}
