// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! CreatureEventAI.cpp:87-162,1382-1472,1597-1632,1929-1975.
use super::*;
use benilla_sim::event_ai::{timer_executed_event, update_holder_timer, EventClock};
use std::collections::BTreeSet;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum EventAiBuild {
    Release,
    MangosDebug,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct EventAiHolder {
    pub event: EventAiDefinition,
    pub timer: u32,
    pub enabled: bool,
    pub in_progress: bool,
    pub event_target: Option<ObjectHandle>,
}
impl EventAiHolder {
    fn new(event: EventAiDefinition) -> Self {
        Self {
            event,
            timer: 0,
            enabled: true,
            in_progress: false,
            event_target: None,
        }
    }
    pub fn update_timer(
        &mut self,
        range: [u32; 2],
        random: &mut dyn CreatureRandom,
    ) -> Result<bool> {
        update_holder_timer(
            &mut self.timer,
            &mut self.enabled,
            range,
            &mut |min, max| random.creature_urand(min, max),
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EventAiRespawnSettings {
    pub no_aggro_on_sight: bool,
    pub spawn_defensive: bool,
    pub ignore_combat: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EventAiCallback {
    ResetAllBaseTimers,
    RestoreRangedModeAndAttackDistance,
    SetAiOrderNone,
    ClearCombatScriptStatus,
    ClearCombatOnlyRoot,
    SetReactDefensive(bool),
    DisableCombatEntry,
    BaseEnterCombat(Option<ObjectHandle>),
    CalculateMainSpell(u32),
}
impl EventAiCallback {
    pub fn missing_owner(self) -> Refusal {
        refuse(match self {
            Self::CalculateMainSpell(_) => "execution.event_ai.f5_07_main_spell_state_owner",
            _ => "execution.event_ai.f5_04_base_ai_owner",
        })
    }
}
/// Real dependent operations occur at their source call. Default means refusal.
pub trait EventAiLifecycleOwner {
    fn callback(&mut self, _state: &mut SourceEventAi, call: EventAiCallback) -> Result<()> {
        Err(call.missing_owner())
    }
    fn check_and_ready(
        &mut self,
        _state: &mut SourceEventAi,
        _holder: usize,
        _invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        Err(refuse("execution.event_ai.f5_03_event_check_owner"))
    }
    fn process_event(
        &mut self,
        _state: &mut SourceEventAi,
        _holder: usize,
        _invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        Err(refuse("execution.event_ai.f5_03_event_dispatch_owner"))
    }
    fn spawned_conditions(&mut self, _state: &SourceEventAi, _holder: usize) -> Result<bool> {
        Err(refuse("execution.event_ai.f5_03_spawned_conditions_owner"))
    }
}
/// F7 inventory v1. Not part of native persistence. Restore remains unavailable.
#[derive(Clone, Debug)]
pub struct SourceEventAi {
    pub selected: SelectedCreatureAi,
    definitions: Arc<EventAiDefinitions>,
    pub build: EventAiBuild,
    pub holders: Vec<EventAiHolder>,
    pub clock: EventClock,
    pub depth: usize,
    pub staged: Vec<Vec<usize>>,
    pub phase: u8,
    pub has_ooc_los: bool,
    pub invincibility_hp: u32,
    pub throw_mask: u32,
    pub throw_step: u32,
    pub last_spell_max_range: f32,
    pub despawn_mask: u32,
    pub despawn_entries: BTreeSet<u32>,
    pub despawn_guids: Vec<SourceGuid>,
    pub main_spell_id: u32,
    pub main_spells: BTreeSet<u32>,
    pub distance_spells: BTreeSet<u32>,
}
impl SourceEventAi {
    /// Constructor does not call InitAI. Creature.cpp:880-887 calls SpellListChanged first.
    pub fn construct(
        selected: SelectedCreatureAi,
        objects: &SourceObjects,
        definitions: Arc<EventAiDefinitions>,
        build: EventAiBuild,
    ) -> Result<Self> {
        selected.validate(objects)?;
        require(
            selected.executable().builtin_kind() == Some(BuiltinCreatureAi::Event),
            "execution.event_ai.executable_identity",
        )?;
        Ok(Self {
            selected,
            definitions,
            build,
            holders: Vec::new(),
            clock: EventClock::constructed(),
            depth: 0,
            staged: Vec::new(),
            phase: 0,
            has_ooc_los: false,
            invincibility_hp: 0,
            throw_mask: 0,
            throw_step: 0,
            last_spell_max_range: 0.0,
            despawn_mask: 0,
            despawn_entries: BTreeSet::new(),
            despawn_guids: Vec::new(),
            main_spell_id: 0,
            main_spells: BTreeSet::new(),
            distance_spells: BTreeSet::new(),
        })
    }
    pub fn validate(&self, objects: &SourceObjects) -> Result<()> {
        self.selected.validate(objects)?;
        require(self.depth <= self.staged.len(), "execution.event_ai.depth")?;
        require(
            self.staged
                .iter()
                .flatten()
                .all(|i| *i < self.holders.len()),
            "execution.event_ai.staged_holder",
        )?;
        require(self.clock.remaining <= 500, "execution.event_ai.clock")?;
        for holder in &self.holders {
            require(
                holder.event.kind < 43
                    && holder.event.chance <= 100
                    && holder.event.actions.iter().all(|action| {
                        action.kind < 66 && (action.params.is_some() || action.kind == 0)
                    }),
                "execution.event_ai.holder_definition",
            )?;
            if let Some(target) = holder.event_target {
                objects.get(target)?;
            }
        }
        Ok(())
    }
    /// Complete fields owned here, not a substitute for F5.04 base AI or F7 restore.
    pub fn inventory(&self, objects: &SourceObjects) -> Result<serde_json::Value> {
        self.validate(objects)?;
        Ok(serde_json::json!({
            "version":1,"selected":self.selected,"definitions":self.definitions.as_ref(),
            "build":self.build,"holders":self.holders,"clock":self.clock,
            "depth":self.depth,"staged":self.staged,"phase":self.phase,
            "has_ooc_los":self.has_ooc_los,"invincibility_hp":self.invincibility_hp,
            "throw_mask":self.throw_mask,"throw_step":self.throw_step,
            "last_spell_max_range_bits":self.last_spell_max_range.to_bits(),
            "despawn_mask":self.despawn_mask,"despawn_entries":self.despawn_entries,
            "despawn_guids":self.despawn_guids,"main_spell_id":self.main_spell_id,
            "main_spells":self.main_spells,"distance_spells":self.distance_spells,
            "base_ai_state":"execution.event_ai.f5_04_base_ai_owner",
            "restore":"execution.event_ai.f7_restore_owner"
        }))
    }
    pub fn init(
        &mut self,
        objects: &SourceObjects,
        owner: &mut impl EventAiLifecycleOwner,
    ) -> Result<()> {
        self.selected.validate(objects)?;
        self.holders.clear();
        let guid = objects.get(self.selected.object())?.database_guid;
        let definitions = Arc::clone(&self.definitions);
        for event in definitions.for_spawn(self.selected.inputs().effective_entry, guid) {
            if self.build == EventAiBuild::Release && event.flags & 0x80 != 0 {
                continue;
            }
            self.holders.push(EventAiHolder::new(event.clone()));
            if event.kind == 10 {
                self.has_ooc_los = true;
            }
            for action in &event.actions {
                if action.kind != 11 {
                    continue;
                }
                let p = action
                    .params
                    .ok_or_else(|| refuse("execution.event_ai.cast_parameters"))?;
                if p[2] & 0x100 != 0 {
                    // UnitAI.cpp:960-971. Set the first ID before CalculatePowerCost.
                    if self.main_spell_id == 0 {
                        self.main_spell_id = p[0];
                        owner.callback(self, EventAiCallback::CalculateMainSpell(p[0]))?;
                    }
                    self.main_spells.insert(p[0]);
                }
                if p[2] & 0x400 != 0 {
                    self.distance_spells.insert(p[0]);
                }
            }
        }
        Ok(())
    }
    fn ensure_depth(&mut self) {
        if self.depth >= self.staged.len() {
            self.staged.resize_with(self.depth + 1, Vec::new);
        }
    }
    fn process(
        &mut self,
        owner: &mut impl EventAiLifecycleOwner,
        invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        let depth = self.depth;
        self.depth += 1;
        // :244-252 iterates the staged list, retaining nested callback depth.
        let mut index = 0;
        while index < self.staged[depth].len() {
            let holder = self.staged[depth][index];
            owner.process_event(self, holder, invoker)?;
            index += 1;
        }
        self.staged[self.depth - 1].clear();
        self.depth -= 1;
        Ok(())
    }
    pub fn reset(
        &mut self,
        random: &mut dyn CreatureRandom,
        owner: &mut impl EventAiLifecycleOwner,
    ) -> Result<()> {
        self.clock.reset();
        self.throw_step = 0;
        self.last_spell_max_range = 0.0;
        for call in [
            EventAiCallback::RestoreRangedModeAndAttackDistance,
            EventAiCallback::SetAiOrderNone,
            EventAiCallback::ClearCombatScriptStatus,
            EventAiCallback::ClearCombatOnlyRoot,
        ] {
            owner.callback(self, call)?;
        }
        for holder in &mut self.holders {
            match holder.event.kind {
                0 | 29 | 4 => {}
                1 => {
                    if holder
                        .update_timer([holder.event.params[0], holder.event.params[1]], random)?
                    {
                        holder.enabled = true;
                    }
                }
                _ => {
                    holder.enabled = true;
                    holder.timer = 0;
                }
            }
        }
        Ok(())
    }
    pub fn just_respawned(
        &mut self,
        objects: &SourceObjects,
        settings: EventAiRespawnSettings,
        random: &mut dyn CreatureRandom,
        owner: &mut impl EventAiLifecycleOwner,
    ) -> Result<()> {
        self.selected.validate(objects)?;
        // Base CreatureAI.cpp:124-128 invokes virtual Reset before InitAI copies new holders.
        owner.callback(self, EventAiCallback::ResetAllBaseTimers)?;
        self.reset(random, owner)?;
        self.init(objects, owner)?;
        owner.callback(
            self,
            EventAiCallback::SetReactDefensive(settings.no_aggro_on_sight),
        )?;
        if settings.spawn_defensive {
            owner.callback(self, EventAiCallback::SetReactDefensive(true))?;
        } else if settings.ignore_combat {
            owner.callback(self, EventAiCallback::DisableCombatEntry)?;
        }
        self.clock.reset();
        self.throw_step = 0;
        self.last_spell_max_range = 0.0;
        self.ensure_depth();
        for index in 0..self.holders.len() {
            match self.holders[index].event.kind {
                11 => {
                    if owner.spawned_conditions(self, index)? {
                        owner.check_and_ready(self, index, None)?;
                    }
                }
                0 | 1 | 29 => {
                    let holder = &mut self.holders[index];
                    if holder
                        .update_timer([holder.event.params[0], holder.event.params[1]], random)?
                    {
                        holder.enabled = true;
                    }
                }
                _ => {
                    self.holders[index].enabled = true;
                    self.holders[index].timer = 0;
                }
            }
        }
        self.process(owner, None)
    }
    pub fn just_reached_home(
        &mut self,
        random: &mut dyn CreatureRandom,
        owner: &mut impl EventAiLifecycleOwner,
    ) -> Result<()> {
        self.ensure_depth();
        for i in 0..self.holders.len() {
            if self.holders[i].event.kind == 21 {
                owner.check_and_ready(self, i, None)?;
            }
        }
        self.process(owner, None)?;
        self.reset(random, owner)
    }
    pub fn enter_combat(
        &mut self,
        enemy: Option<ObjectHandle>,
        objects: &SourceObjects,
        random: &mut dyn CreatureRandom,
        owner: &mut impl EventAiLifecycleOwner,
    ) -> Result<()> {
        self.selected.validate(objects)?;
        if let Some(enemy) = enemy {
            objects.get(enemy)?;
        }
        owner.callback(self, EventAiCallback::BaseEnterCombat(enemy))?;
        self.ensure_depth();
        for i in 0..self.holders.len() {
            let holder = &mut self.holders[i];
            let kind = holder.event.kind;
            match kind {
                4 => {
                    holder.enabled = true;
                    owner.check_and_ready(self, i, enemy)?;
                }
                0 => {
                    if holder
                        .update_timer([holder.event.params[0], holder.event.params[1]], random)?
                    {
                        holder.enabled = true;
                    }
                }
                14..=16 | 32
                    if holder.update_timer(
                        [holder.event.params[2], holder.event.params[3]],
                        random,
                    )? =>
                {
                    holder.enabled = true;
                }
                _ => {}
            }
        }
        self.process(owner, enemy)?;
        self.clock.reset();
        Ok(())
    }
    pub fn update_timers(
        &mut self,
        diff: u32,
        owner: &mut impl EventAiLifecycleOwner,
    ) -> Result<()> {
        let Some(elapsed) = self.clock.begin_update(diff) else {
            return Ok(());
        };
        self.ensure_depth();
        for i in 0..self.holders.len() {
            let holder = &mut self.holders[i];
            if holder.event.kind == 36 {
                owner.check_and_ready(self, i, None)?;
                continue;
            }
            if holder.timer != 0 {
                // Exact reached shift, not a blanket rejection of phase-independent work.
                let phase = 1u32
                    .checked_shl(u32::from(self.phase))
                    .ok_or_else(|| refuse("execution.event_ai.phase_shift_undefined"))?;
                if holder.event.inverse_phase & phase == 0 {
                    holder.timer = holder.timer.saturating_sub(elapsed);
                }
            }
            if holder.enabled && holder.timer == 0 && timer_executed_event(holder.event.kind) {
                owner.check_and_ready(self, i, None)?;
            }
        }
        self.process(owner, None)?;
        self.clock.reset();
        Ok(())
    }
}
