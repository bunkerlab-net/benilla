// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAI.cpp:665-790,890-941,993-1001,1142-1157,1248-1271.
use super::*;
use benilla_sim::event_ai::select_text_slot;

/// The first unavailable dependent action owner, not an action support declaration.
pub fn event_ai_action_owner(kind: u16) -> &'static str {
    match kind {
        1 | 44 | 54 => "execution.event_ai.presentation_text_owner",
        4 | 9 => "execution.event_ai.presentation_sound_owner",
        5 | 10 => "execution.event_ai.presentation_emote_owner",
        11 | 60 => "execution.event_ai.f5_07_spell_owner",
        12 | 32 | 56 => "execution.event_ai.f5_15_summon_owner",
        13 | 14 => "execution.threat.f5_04b_unit_fields_owner",
        38 => "execution.event_ai.f5_04b_zone_combat_owner",
        15 | 16 | 26 | 27 | 33 => "execution.event_ai.f5_24_quest_owner",
        21 | 25 | 29 | 48 | 51 | 58 | 59 | 64 | 65 => "execution.event_ai.f5_05_motion_owner",
        28 => "execution.event_ai.f5_12_aura_owner",
        34 | 35 => "execution.event_ai.instance_script_owner",
        36 => "execution.event_ai.update_template_owner",
        37 => "execution.event_ai.f5_04_death_owner",
        39 | 45 => "execution.event_ai.f5_06_ai_event_visitor_owner",
        41 => "execution.event_ai.f5_06_despawn_owner",
        52 => "execution.event_ai.f5_08_spell_owner",
        53 => "execution.event_ai.f5_19_relay_owner",
        2 | 3 | 17..=20 | 24 | 40 | 42 | 43 | 47 | 50 | 55 | 57 | 61 | 63 => {
            "execution.event_ai.f5_04_unit_fields_owner"
        }
        _ => "execution.event_ai.unknown_action_owner",
    }
}
impl SourceEventAi {
    pub fn execute_action(
        &mut self,
        action: &EventAiAction,
        word: u32,
        call: EventAiInvocation,
        event_target: Option<ObjectHandle>,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<bool> {
        // :667-668 NONE is false, not a successful empty action.
        if action.kind == 0 {
            return Ok(false);
        }
        let p = action
            .params
            .ok_or_else(|| refuse("execution.event_ai.action_parameters"))?;
        let creature = self.selected.object();
        match action.kind {
            1 | 44 => {
                // Both cases test union text.TextId[0] first. For CHANCED_TEXT this is chance.
                if p[0] == 0 {
                    return Ok(false);
                }
                let text = if action.kind == 1 {
                    p[usize::from(select_text_slot(p.map(|n| n as i32), word))] as i32
                } else if word % 100 < p[0] {
                    if p[1] != 0 && p[2] != 0 {
                        p[1 + (word % 2) as usize] as i32
                    } else {
                        p[1] as i32
                    }
                } else {
                    0
                };
                if text != 0 {
                    let target = self.text_target(call.invoker, owner)?;
                    owner.display_text(creature, text, target)?;
                }
            }
            4 => owner.sound(creature, p[0])?,
            5 => owner.emote(creature, p[0])?,
            9 | 10 => {
                let value = p[(word % 3) as usize] as i32;
                if value >= 0 {
                    if action.kind == 9 {
                        owner.sound(creature, value as u32)?;
                    } else {
                        owner.emote(creature, value as u32)?;
                    }
                }
            }
            13 => {
                if let Some(target) = self.action_target(p[1], call, event_target, owner)? {
                    let context = owner.threat_context()?;
                    if p[2] != 0 {
                        context.state.add(
                            creature,
                            Some(target),
                            ThreatValue::from_i32(p[0] as i32),
                            false,
                            0,
                            None,
                            context.owner,
                        )?;
                    } else {
                        let guid = context.owner.threat_unit(target)?.guid;
                        context
                            .state
                            .modify_percent(creature, guid, p[0] as i32, context.owner)?;
                    }
                }
            }
            14 => {
                let context = owner.threat_context()?;
                context
                    .state
                    .event_ai_all_percent(creature, p[0] as i32, context.owner)?;
            }
            18 | 19 => {
                if let Some(target) = self.action_target(p[1], call, event_target, owner)? {
                    owner.unit_flag(target, p[0], action.kind == 18)?;
                }
            }
            // m_Phase is uint8 (:931 in CreatureEventAI.h). Invalid shifts refuse only when reached.
            22 => self.phase = p[0] as u8,
            23 => {
                let phase = i32::from(self.phase)
                    .checked_add(p[0] as i32)
                    .ok_or_else(|| refuse("execution.event_ai.phase_add_overflow_undefined"))?;
                self.phase = phase.clamp(0, 31) as u8;
            }
            30 => self.phase = p[(word % 3) as usize] as u8,
            31 => {
                if p[1] > p[0] {
                    let width = p[1].wrapping_sub(p[0]).wrapping_add(1);
                    // Unsigned interval width can wrap to zero even though max > min.
                    let offset = word.checked_rem(width).ok_or_else(|| {
                        refuse("execution.event_ai.phase_range_modulo_zero_undefined")
                    })?;
                    self.phase = offset.wrapping_add(p[0]) as u8;
                }
                // :1001 merely logs when max <= min. It is not an undefined modulo.
            }
            45 => {
                let Some(target) = self.action_target(p[2], call, event_target, owner)? else {
                    return Ok(false);
                };
                // UnitAI.cpp:617 skips the visitor entirely for zero radius.
                if p[1] != 0 {
                    owner.send_ai_event(self, p[0], target, p[1])?;
                }
            }
            46 => self.throw_mask = p[0],
            54 => {
                let Some(target) = self.action_target(p[1], call, event_target, owner)? else {
                    return Ok(false);
                };
                let text = if p[2] != 0 {
                    owner.text_template(p[2])?
                } else {
                    p[0] as i32
                };
                if p[2] == 0 || text != 0 {
                    owner.display_text(creature, text, Some(target))?;
                }
            }
            // Source switch default is genuinely false for these unused opcodes (:1374-1376).
            6..=8 | 49 => return Ok(false),
            _ => return owner.dependent_action(self, action, call, event_target, word),
        }
        Ok(true)
    }
    fn text_target(
        &self,
        invoker: Option<ObjectHandle>,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<Option<ObjectHandle>> {
        let creature = self.selected.object();
        if let Some(invoker) = invoker {
            let unit = owner.unit(invoker)?;
            if unit.kind == SourceObjectKind::Player {
                return Ok(Some(invoker));
            }
            if let Some(parent) = unit.owner {
                if owner.unit(parent)?.kind == SourceObjectKind::Player {
                    return Ok(Some(parent));
                }
            }
            Ok(None)
        } else {
            let victim = owner.unit(creature)?.victim;
            if let Some(victim) = victim {
                let unit = owner.unit(victim)?;
                if unit.kind != SourceObjectKind::Player {
                    if let Some(parent) = unit.owner {
                        if owner.unit(parent)?.kind == SourceObjectKind::Player {
                            return Ok(Some(parent));
                        }
                    }
                }
            }
            // :718-724 retains a nonplayer victim if its owner is not a player.
            Ok(victim)
        }
    }
    pub fn action_target(
        &self,
        kind: u32,
        call: EventAiInvocation,
        event_target: Option<ObjectHandle>,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<Option<ObjectHandle>> {
        // This subset needs no threat/geometry owner. Error logging does not change the pointer result.
        Ok(match kind {
            0 => Some(self.selected.object()),
            1 => owner.unit(self.selected.object())?.victim,
            6 => call.invoker,
            7 => {
                if let Some(invoker) = call.invoker {
                    owner.unit(invoker)?.beneficiary
                } else {
                    None
                }
            }
            10 => call.sender,
            11 => owner.unit(self.selected.object())?.spawner,
            12 => event_target,
            15 => None,
            2..=5 | 8 | 9 | 13 | 14 | 16..=18 => {
                return owner.target(self.selected.object(), kind, call, event_target)
            }
            _ => None,
        })
    }
}
