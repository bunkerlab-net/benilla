// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAI.cpp:244-662. Definitions are not executable owner receipts.
use super::*;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EventAiInvocation {
    pub invoker: Option<ObjectHandle>,
    pub sender: Option<ObjectHandle>,
}
/// Live Unit inputs, read from the generation-owning Unit owner, never catalog defaults.
#[derive(Clone, Copy, Debug)]
pub struct EventAiUnit {
    pub kind: SourceObjectKind,
    pub entry: u32,
    pub combat: bool,
    pub evade: bool,
    pub health: u32,
    pub max_health: u32,
    pub mana: u32,
    pub max_mana: u32,
    pub has_mana: bool,
    pub energy: u32,
    pub max_energy: u32,
    pub victim: Option<ObjectHandle>,
    pub owner: Option<ObjectHandle>,
    pub beneficiary: Option<ObjectHandle>,
    pub controlling_player: Option<ObjectHandle>,
    pub spawner: Option<ObjectHandle>,
    pub casting_non_melee: bool,
}
#[derive(Clone, Copy, Debug)]
pub enum EventAiFriendlyQuery {
    LowestHealth {
        radius: u32,
        deficit: u32,
        percent: bool,
        target_self: bool,
    },
    EligibleDispel {
        radius: u32,
    },
    MissingBuff {
        radius: u32,
        spell: u32,
        in_combat: bool,
        self_target: bool,
        spawn_group: u32,
    },
}
/// Dependent owners execute at the reached operation. Defaults never report success.
/// F5.04 owns Unit/base AI, F5.07+ spells, F6 geometry, later presentation rows output.
pub trait EventAiExecutionOwner: CreatureRandom {
    fn spell_context(&mut self) -> Result<SpellStartContext<'_>> {
        Err(refuse("execution.spells.live_SpellStart_context_owner"))
    }
    fn cast_target(
        &mut self,
        _creature: ObjectHandle,
        _kind: u32,
        _call: EventAiInvocation,
        _event_target: Option<ObjectHandle>,
        _spell: u32,
        _cast_flags: u32,
    ) -> Result<(Option<ObjectHandle>, bool)> {
        Err(refuse(
            "execution.spells.EventAI_SelectAttackingTarget_owner",
        ))
    }
    fn threat_context(&mut self) -> Result<ThreatContext<'_>> {
        Err(refuse("execution.threat.f5_04b_unit_fields_owner"))
    }
    fn unit(&mut self, _unit: ObjectHandle) -> Result<EventAiUnit> {
        Err(refuse("execution.event_ai.f5_04_unit_owner"))
    }
    fn combat_state(&mut self, unit: ObjectHandle) -> Result<(bool, bool)> {
        let unit = self.unit(unit)?;
        Ok((unit.combat, unit.evade))
    }
    fn victim(&mut self, unit: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.unit(unit)?.victim)
    }
    fn current_ranged_mode(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.event_ai.f5_04_base_ai_owner"))
    }
    fn can_execute_combat_action(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.event_ai.f5_04_combat_action_owner"))
    }
    fn condition(
        &mut self,
        _condition: u32,
        _subject: ObjectHandle,
        _creature: ObjectHandle,
    ) -> Result<bool> {
        Err(refuse("execution.event_ai.f5_22_condition_owner"))
    }
    fn range(
        &mut self,
        _creature: ObjectHandle,
        _target: ObjectHandle,
        _min: u32,
        _max: u32,
    ) -> Result<bool> {
        Err(refuse("execution.event_ai.f6_range_owner"))
    }
    fn friendly(
        &mut self,
        _creature: ObjectHandle,
        _query: EventAiFriendlyQuery,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.event_ai.f5_04_friendly_visitor_owner"))
    }
    fn creature_group(&mut self, _creature: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.event_ai.creature_group_owner"))
    }
    fn aura_stacks(&mut self, _unit: ObjectHandle, _spell: u32) -> Result<Option<u32>> {
        Err(refuse("execution.event_ai.f5_12_aura_owner"))
    }
    fn attacking_target(
        &mut self,
        _creature: ObjectHandle,
        _min: u32,
        _max: u32,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.event_ai.f5_04_threat_owner"))
    }
    fn facing(
        &mut self,
        _creature: ObjectHandle,
        _target: ObjectHandle,
        _front: bool,
    ) -> Result<bool> {
        Err(refuse("execution.event_ai.f6_facing_owner"))
    }
    fn combat_movement(&mut self, _creature: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.event_ai.f5_04_base_ai_owner"))
    }
    fn reachable(&mut self, _creature: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.event_ai.f5_05_motion_owner"))
    }
    fn spawn_map(&mut self, _creature: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.event_ai.map_owner"))
    }
    fn zone_area(&mut self, _creature: ObjectHandle) -> Result<[u32; 2]> {
        Err(refuse("execution.event_ai.f6_zone_area_owner"))
    }
    fn lifecycle(&mut self, _state: &mut SourceEventAi, call: EventAiCallback) -> Result<()> {
        Err(call.missing_owner())
    }
    fn display_text(
        &mut self,
        _creature: ObjectHandle,
        _text: i32,
        _target: Option<ObjectHandle>,
    ) -> Result<()> {
        Err(refuse("execution.event_ai.presentation_text_owner"))
    }
    fn text_template(&mut self, _template: u32) -> Result<i32> {
        Err(refuse("execution.event_ai.text_template_owner"))
    }
    fn emote(&mut self, _creature: ObjectHandle, _emote: u32) -> Result<()> {
        Err(refuse("execution.event_ai.presentation_emote_owner"))
    }
    fn sound(&mut self, _creature: ObjectHandle, _sound: u32) -> Result<()> {
        Err(refuse("execution.event_ai.presentation_sound_owner"))
    }
    fn unit_flag(&mut self, _target: ObjectHandle, _value: u32, _set: bool) -> Result<()> {
        Err(refuse("execution.event_ai.f5_04_unit_fields_owner"))
    }
    fn target(
        &mut self,
        _creature: ObjectHandle,
        _kind: u32,
        _invocation: EventAiInvocation,
        _event_target: Option<ObjectHandle>,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse("execution.event_ai.f5_04_target_owner"))
    }
    fn send_ai_event(
        &mut self,
        _state: &mut SourceEventAi,
        _event: u32,
        _invoker: ObjectHandle,
        _radius: u32,
    ) -> Result<()> {
        Err(refuse("execution.event_ai.f5_06_ai_event_visitor_owner"))
    }
    fn dependent_action(
        &mut self,
        _state: &mut SourceEventAi,
        action: &EventAiAction,
        _invocation: EventAiInvocation,
        _event_target: Option<ObjectHandle>,
        _word: u32,
    ) -> Result<bool> {
        Err(refuse(event_ai_action_owner(action.kind)))
    }
}

impl SourceEventAi {
    /// CheckEvent mutates only eventTarget; readiness sets inProgress before staging.
    pub fn ready_event(
        &mut self,
        index: usize,
        call: EventAiInvocation,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<bool> {
        let holder = self
            .holders
            .get(index)
            .ok_or_else(|| refuse("execution.event_ai.holder_index"))?;
        if !holder.enabled || holder.timer != 0 || holder.in_progress {
            return Ok(false);
        }
        let creature = self.selected.object();
        if holder.event.flags & 0x400 != 0 && !owner.can_execute_combat_action(creature)? {
            return Ok(false);
        }
        let phase = 1u32
            .checked_shl(u32::from(self.phase))
            .ok_or_else(|| refuse("execution.event_ai.phase_shift_undefined"))?;
        if holder.event.inverse_phase & phase != 0 {
            return Ok(false);
        }
        let ranged = owner.current_ranged_mode(creature)?;
        if holder.event.flags & (if ranged { 0x200 } else { 0x100 }) != 0 {
            return Ok(false);
        }
        let event = &holder.event;
        let p = event.params;
        let mut target = None;
        let ready = match event.kind {
            0 => owner.combat_state(creature)?.0,
            1 => {
                let (combat, evade) = owner.combat_state(creature)?;
                !combat && !evade
            }
            29 | 4 | 7 | 8 | 11 | 21 | 30 | 34 | 35 | 37 => true,
            2 => {
                if p[4] == 0 && !owner.combat_state(creature)?.0 {
                    false
                } else {
                    let unit = owner.unit(creature)?;
                    if p[4] == 0 && unit.max_health == 0 {
                        false
                    } else {
                        percentage(unit.health, unit.max_health, p)?
                    }
                }
            }
            3 | 31 => {
                if !owner.combat_state(creature)?.0 {
                    false
                } else if event.kind == 3 {
                    let unit = owner.unit(creature)?;
                    unit.has_mana && unit.max_mana != 0 && percentage(unit.mana, unit.max_mana, p)?
                } else {
                    let unit = owner.unit(creature)?;
                    unit.max_energy != 0 && percentage(unit.energy, unit.max_energy, p)?
                }
            }
            5 => {
                if p[2] != 1 {
                    true
                } else {
                    let invoker = call
                        .invoker
                        .ok_or_else(|| refuse("execution.event_ai.kill_null_invoker_undefined"))?;
                    owner.unit(invoker)?.kind == SourceObjectKind::Player
                }
            }
            6 => {
                if p[0] == 0 {
                    true
                } else if let Some(invoker) = call.invoker {
                    let unit = owner.unit(invoker)?;
                    owner.condition(p[0], unit.controlling_player.unwrap_or(invoker), creature)?
                } else {
                    false
                }
            }
            9 | 12 | 13 | 18 | 24 | 28 | 33 => {
                if !owner.combat_state(creature)?.0 {
                    false
                } else if let Some(victim) = owner.victim(creature)? {
                    match event.kind {
                        9 => owner.range(creature, victim, p[0], p[1])?,
                        12 => {
                            let victim = owner.unit(victim)?;
                            victim.max_health != 0
                                && percentage(victim.health, victim.max_health, p)?
                        }
                        13 => owner.unit(victim)?.casting_non_melee,
                        18 => {
                            let victim = owner.unit(victim)?;
                            victim.has_mana
                                && victim.max_mana != 0
                                && percentage(victim.mana, victim.max_mana, p)?
                        }
                        24 | 28 => {
                            aura_matches(owner.aura_stacks(victim, p[0])?, p[1], event.kind == 28)
                        }
                        33 if p[0] <= 1 => owner.facing(creature, victim, p[0] == 1)?,
                        _ => true,
                    }
                } else {
                    false
                }
            }
            10 | 22 => {
                let condition = if event.kind == 10 { p[5] } else { p[1] };
                if condition == 0 {
                    true
                } else {
                    let invoker = call.invoker.ok_or_else(|| {
                        refuse("execution.event_ai.beneficiary_null_invoker_undefined")
                    })?;
                    let beneficiary = owner.unit(invoker)?.beneficiary;
                    if let Some(player) = beneficiary {
                        if owner.unit(player)?.kind == SourceObjectKind::Player {
                            owner.condition(condition, player, creature)?
                        } else {
                            true
                        }
                    } else {
                        true
                    }
                }
            }
            14..=16 => {
                let group = if event.kind == 16 && p[4] & 4 != 0 {
                    owner.creature_group(creature)?
                } else {
                    0
                };
                let combat = owner.combat_state(creature)?.0;
                if (event.kind != 16 || p[4] & 3 == 0) && !combat
                    || event.kind == 16 && p[4] & 1 == 0 && p[4] & 2 != 0 && combat
                {
                    false
                } else {
                    let query = match event.kind {
                        14 => EventAiFriendlyQuery::LowestHealth {
                            radius: p[1],
                            deficit: p[0],
                            percent: p[4] != 0,
                            target_self: event.friendly_hp_target_self.ok_or_else(|| {
                                refuse("execution.event_ai.friendly_hp_computed_data_undefined")
                            })?,
                        },
                        15 => EventAiFriendlyQuery::EligibleDispel { radius: p[1] },
                        // :424 passes group as argument four, the bool self, NOT spawnGroupId.
                        _ => EventAiFriendlyQuery::MissingBuff {
                            radius: p[1],
                            spell: p[0],
                            in_combat: if p[4] & 1 != 0 { combat } else { p[4] & 2 == 0 },
                            self_target: if p[4] & 1 != 0 { group != 0 } else { true },
                            spawn_group: if p[4] & 1 != 0 { 0 } else { group },
                        },
                    };
                    target = owner.friendly(creature, query)?;
                    target.is_some()
                }
            }
            17 | 25 | 26 => {
                if let Some(invoker) = call.invoker {
                    let unit = owner.unit(invoker)?;
                    matches!(
                        unit.kind,
                        SourceObjectKind::Creature | SourceObjectKind::Pet
                    ) && unit.entry == p[0]
                } else {
                    false
                }
            }
            23 | 27 => aura_matches(owner.aura_stacks(creature, p[0])?, p[1], event.kind == 27),
            32 => {
                target = owner.attacking_target(creature, p[0], p[1])?;
                target.is_some()
            }
            36 => {
                owner.victim(creature)?.is_some()
                    && owner.combat_movement(creature)?
                    && !owner.reachable(creature)?
            }
            // :551-553 genuinely logs and returns false, including unhandled vehicle events.
            _ => false,
        };
        if !ready {
            return Ok(false);
        }
        if let Some(target) = target {
            self.holders[index].event_target = Some(target);
        }
        self.holders[index].in_progress = true;
        self.ensure_depth();
        self.staged[self.depth].push(index);
        Ok(true)
    }
    pub fn reset_event(&mut self, index: usize, random: &mut impl CreatureRandom) -> Result<()> {
        let holder = &mut self.holders[index];
        let kind = holder.event.kind;
        if benilla_sim::event_ai::timer_based_event(kind) {
            let p = holder.event.params;
            let range = match kind {
                5 | 13 => [p[0], p[1]],
                17 | 25 | 26 => [p[1], p[2]],
                _ => [p[2], p[3]],
            };
            holder.update_timer(range, random)?;
        }
        if !matches!(kind, 4 | 6 | 7 | 11 | 21) && holder.event.flags & 1 == 0 {
            holder.enabled = false;
        }
        Ok(())
    }
    pub fn dispatch_event(
        &mut self,
        index: usize,
        call: EventAiInvocation,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<bool> {
        let combat = self.holders[index].event.flags & 0x400 != 0;
        if combat && !owner.can_execute_combat_action(self.selected.object())? {
            self.holders[index].in_progress = false;
            return Ok(false);
        }
        let chance = owner.creature_urand_default()?;
        if u32::from(self.holders[index].event.chance) <= chance % 100 {
            self.reset_event(index, owner)?;
            self.holders[index].in_progress = false;
            return Ok(false);
        }
        let word = owner.creature_urand_default()?;
        let random = self.holders[index].event.flags & 0x20 != 0;
        let mut success = false;
        if !random {
            for slot in 0..3 {
                // Do not retain a holder borrow across a nested callback or removal.
                let action = self.holders[index].event.actions[slot].clone();
                let target = self.holders[index].event_target;
                let result = self.execute_action(&action, word, call, target, owner)?;
                if slot == 0 {
                    success = result;
                    if combat && !success {
                        break;
                    }
                }
            }
        } else {
            let count = self.holders[index]
                .event
                .actions
                .iter()
                .filter(|a| a.kind != 0)
                .count();
            if count != 0 {
                let slot = word as usize % count;
                let action = self.holders[index]
                    .event
                    .actions
                    .iter()
                    .filter(|a| a.kind != 0)
                    .nth(slot)
                    .expect("counted action")
                    .clone();
                let word = owner.creature_urand_default()?;
                success = self.execute_action(
                    &action,
                    word,
                    call,
                    self.holders[index].event_target,
                    owner,
                )?;
            }
        }
        if !combat || success {
            self.reset_event(index, owner)?;
        }
        self.holders[index].in_progress = false;
        Ok(true)
    }
    pub fn dispatch_ready(
        &mut self,
        call: EventAiInvocation,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<()> {
        self.ensure_depth();
        let depth = self.depth;
        self.depth += 1;
        let mut cursor = 0;
        while cursor < self.staged[depth].len() {
            let index = self.staged[depth][cursor];
            // On refusal retain the exact in-flight depth/list for diagnostic inventory.
            self.dispatch_event(index, call, owner)?;
            cursor += 1;
        }
        self.staged[depth].clear();
        self.depth -= 1;
        Ok(())
    }
    pub fn receive_ai_event(
        &mut self,
        event: u32,
        sender: ObjectHandle,
        invoker: Option<ObjectHandle>,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<()> {
        let call = EventAiInvocation {
            invoker,
            sender: Some(sender),
        };
        for i in 0..self.holders.len() {
            let e = &self.holders[i].event;
            if e.kind == 30
                && e.params[0] == event
                && (e.params[1] == 0 || e.params[1] == owner.unit(sender)?.entry)
            {
                self.ready_event(i, call, owner)?;
            }
        }
        self.dispatch_ready(call, owner)
    }
    pub fn spawned_event_conditions(
        &self,
        index: usize,
        owner: &mut impl EventAiExecutionOwner,
    ) -> Result<bool> {
        let e = &self.holders[index].event;
        if e.kind != 11 {
            return Ok(false);
        }
        Ok(match e.params[0] {
            0 => true,
            1 => owner.spawn_map(self.selected.object())? == e.params[1],
            2 => owner
                .zone_area(self.selected.object())?
                .contains(&e.params[1]),
            _ => false,
        })
    }
}
fn percentage(value: u32, maximum: u32, p: [u32; 6]) -> Result<bool> {
    // uint32 multiplication wraps; only division by zero is undefined (:304).
    let percent = value
        .wrapping_mul(100)
        .checked_div(maximum)
        .ok_or_else(|| refuse("execution.event_ai.health_divide_zero_undefined"))?;
    Ok(percent <= p[0] && percent >= p[1])
}
fn aura_matches(stacks: Option<u32>, amount: u32, missing: bool) -> bool {
    stacks.is_some_and(|n| n >= amount) != missing
}
/// Adapter connects F5.02 lifecycle calls to the same F5.03 dispatcher and RNG owner.
pub struct EventAiDispatcher<'a, O: EventAiExecutionOwner>(pub &'a mut O);
impl<O: EventAiExecutionOwner> EventAiLifecycleOwner for EventAiDispatcher<'_, O> {
    fn callback(&mut self, state: &mut SourceEventAi, call: EventAiCallback) -> Result<()> {
        self.0.lifecycle(state, call)
    }
    fn check_and_ready(
        &mut self,
        state: &mut SourceEventAi,
        index: usize,
        invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        state
            .ready_event(
                index,
                EventAiInvocation {
                    invoker,
                    sender: None,
                },
                self.0,
            )
            .map(|_| ())
    }
    fn process_event(
        &mut self,
        state: &mut SourceEventAi,
        index: usize,
        invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        state
            .dispatch_event(
                index,
                EventAiInvocation {
                    invoker,
                    sender: None,
                },
                self.0,
            )
            .map(|_| ())
    }
    fn spawned_conditions(&mut self, state: &SourceEventAi, index: usize) -> Result<bool> {
        state.spawned_event_conditions(index, self.0)
    }
}
