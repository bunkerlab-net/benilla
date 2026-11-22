// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, Map.cpp:2245-2276 ScriptsProcess;
// ScriptMgr.cpp:1774-1827. Reuses the native MonsterText audience kernel.
use super::*;
use benilla_sim::quest_lifecycle as q;

impl<A: Authority> Frame<'_, A> {
    pub(super) fn quest_scripts(&mut self) -> Result<(), NativeError> {
        let Some(g) = self.profile.quest_progression.as_ref() else {
            return Ok(());
        };
        loop {
            let current = self
                .state
                .quest_progression
                .as_ref()
                .ok_or(NativeError::InvalidState)?;
            let mut selected = None;
            for (index, invocation) in current.completion_scripts.iter().enumerate() {
                let script = g.quest(invocation.quest)?.excluded.complete_script;
                let steps = g
                    .quest_catalog()
                    .mechanics()
                    .script(script)
                    .ok_or(NativeError::InvalidState)?;
                if let Some(step) = steps.get(invocation.completed) {
                    let due = invocation
                        .started_ms
                        .checked_add(u64::from(step.delay_ms))
                        .ok_or(NativeError::Limit)?;
                    if due <= self.state.now_ms
                        && selected.is_none_or(|(time, prior)| (due, index) < (time, prior))
                    {
                        selected = Some((due, index));
                    }
                }
            }
            let Some((_, index)) = selected else {
                break;
            };
            let invocation = current.completion_scripts[index].clone();
            let script = g.quest(invocation.quest)?.excluded.complete_script;
            let step = &g
                .quest_catalog()
                .mechanics()
                .script(script)
                .ok_or(NativeError::InvalidState)?[invocation.completed];
            let source =
                self.state.npcs.get(&invocation.giver.spawn).filter(|npc| {
                    npc.id == invocation.giver && npc.ai.phase() != ai::Phase::Despawned
                });
            let mut delivered = false;
            if let Some(npc) = source {
                match step.effect {
                    q::ScriptEffect::Emote { .. } => {
                        // urand(0, 0) still consumes the source RNG draw.
                        self.state.draw_range(
                            0,
                            DrawPurpose::QuestScriptEmote {
                                quest: invocation.quest,
                                step: invocation.completed,
                                npc: invocation.giver,
                            },
                            0,
                            0,
                            &mut self.draws,
                        )?;
                        delivered = true;
                    }
                    q::ScriptEffect::Talk { .. } => {
                        let rules = g.script_rules.as_ref().ok_or(NativeError::InvalidState)?;
                        let profile = npc.effective_profile(self.profile)?;
                        let player_pose = benilla_sim::creature_motion::Pose {
                            position: self.state.position,
                            orientation: ai::Real::from_bits(self.state.orientation.bits())?,
                        };
                        let request = EventAiAudienceRequest {
                            callback: crate::digest(
                                b"benilla-quest-script-callback-v1\0",
                                &(
                                    self.interaction_witness()?,
                                    &invocation,
                                    invocation.completed,
                                ),
                            ),
                            world: self.state.world,
                            tick: self.state.tick,
                            moment: self.interaction_moment(),
                            catalog: g.quest_catalog().hash(),
                            speaker: invocation.giver,
                            map: self.profile.map_id,
                            speaker_pose: npc
                                .source_motion
                                .as_ref()
                                .ok_or(NativeError::UnsupportedMechanic(
                                    "quest script source pose",
                                ))?
                                .motion
                                .pose(),
                            player: self.state.player,
                            player_pose,
                            camera: EventAiCamera {
                                owner: self.state.player,
                                body: VisitorParticipant::Player(self.state.player),
                                body_pose: player_pose,
                            },
                            speaker_bounding_radius: profile.ai.bounding_radius,
                            speaker_combat_reach: profile.ai.combat_reach,
                            body_combat_reach: self.profile.player.combat_reach,
                            say_range: rules.say_range,
                            visibility_distance: rules.visibility_distance,
                            selection: EventAiAudienceSelection::SoloSelfCamera,
                        };
                        let Facts::EventAiAudience(facts) = self.ask(
                            Some(invocation.giver),
                            Query::EventAiAudience(Box::new(request.clone())),
                        )?
                        else {
                            return Err(NativeError::AuthorityBinding);
                        };
                        delivered = request.validate(&facts)?;
                    }
                }
            }
            self.state
                .quest_progression
                .as_mut()
                .ok_or(NativeError::InvalidState)?
                .completion_scripts[index]
                .completed += 1;
            self.events.push(Event::QuestScript(QuestScriptEvent {
                quest: invocation.quest,
                giver: invocation.giver,
                step: invocation.completed,
                effect: step.effect.clone(),
                delivered,
            }));
        }
        Ok(())
    }
}
