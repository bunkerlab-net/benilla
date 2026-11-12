// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! TotemAI.cpp:28-128; GuardianAI.cpp:25-108. Shared EventAI state is retained.
use super::*;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TotemQuery {
    CanAttackOnSight,
    WithinDistance(u32),
    CanAssist,
    VisibleForOrDetect,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GuardianSightQuery {
    Enemy,
    WithinDistance(u32),
    LineOfSight,
}
#[derive(Clone, Copy, Debug)]
pub struct TotemSpell {
    pub id: u32,
    pub range_index: u32,
}
/// Summon fields, spell metadata and spatial queries require their real owners.
pub trait SummonedAiExecutionOwner:
    BuiltinAiExecutionOwner + HostileTargetExecutionOwner + CreatureRandom
{
    fn guardian_sight(
        &mut self,
        _unit: ObjectHandle,
        _target: ObjectHandle,
        query: GuardianSightQuery,
    ) -> Result<bool> {
        Err(refuse(match query {
            GuardianSightQuery::Enemy => "execution.guardian_ai.f5_04_ooc_los_reaction_owner",
            _ => "execution.guardian_ai.f6_ooc_los_geometry_owner",
        }))
    }
    fn mini_pet(&mut self, _unit: ObjectHandle) -> Result<bool> {
        Err(refuse("execution.guardian_ai.f5_15_pet_subtype_owner"))
    }
    fn totem_type(&mut self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.totem_ai.f5_15_totem_type_owner"))
    }
    fn totem_spell(&mut self, _unit: ObjectHandle) -> Result<u32> {
        Err(refuse("execution.totem_ai.f5_15_totem_spell_owner"))
    }
    fn spell(&mut self, _id: u32) -> Result<Option<TotemSpell>> {
        Err(refuse("execution.totem_ai.f5_07_spell_template_owner"))
    }
    fn spell_ready(&mut self, _unit: ObjectHandle, _spell: TotemSpell) -> Result<bool> {
        Err(refuse("execution.totem_ai.f5_08_spell_cooldown_owner"))
    }
    fn spell_max_range(&mut self, _range_index: u32) -> Result<f32> {
        Err(refuse("execution.totem_ai.f5_07_spell_range_owner"))
    }
    fn totem_query(
        &mut self,
        _unit: ObjectHandle,
        _target: ObjectHandle,
        q: TotemQuery,
    ) -> Result<bool> {
        Err(refuse(match q {
            TotemQuery::CanAttackOnSight | TotemQuery::CanAssist => {
                "execution.totem_ai.f5_04_reaction_owner"
            }
            _ => "execution.totem_ai.f6_target_geometry_owner",
        }))
    }
    fn nearest_totem_target(
        &mut self,
        _unit: ObjectHandle,
        _owner: Option<ObjectHandle>,
        _range: f32,
    ) -> Result<Option<ObjectHandle>> {
        Err(refuse(
            "execution.totem_ai.f6_nearest_attackable_visitor_owner",
        ))
    }
    fn cast_totem_spell(
        &mut self,
        _unit: ObjectHandle,
        _target: ObjectHandle,
        _spell: u32,
    ) -> Result<()> {
        Err(refuse("execution.totem_ai.f5_08_nontriggered_cast_owner"))
    }
    fn follow_data(&mut self, _unit: ObjectHandle, _player: ObjectHandle) -> Result<(f32, f32)> {
        Err(refuse(
            "execution.guardian_ai.f5_15_player_follow_data_owner",
        ))
    }
    fn move_follow(
        &mut self,
        _unit: ObjectHandle,
        _owner: ObjectHandle,
        _distance: f32,
        _angle: f32,
    ) -> Result<()> {
        Err(refuse("execution.guardian_ai.f5_05_follow_motion_owner"))
    }
    fn relinquish_follow(&mut self, _unit: ObjectHandle, _player: ObjectHandle) -> Result<()> {
        Err(refuse(
            "execution.guardian_ai.f5_15_relinquish_follow_owner",
        ))
    }
}
#[derive(Clone, Debug)]
pub struct SourceSummonedAi {
    pub event: SourceEventAi,
    victim_guid: Option<SourceGuid>,
    requested_follower: Option<(ObjectHandle, SourceGuid)>,
    pub follow_default: bool,
}
impl SourceSummonedAi {
    #[allow(clippy::too_many_arguments)]
    pub fn construct(
        selected: SelectedCreatureAi,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        definitions: Arc<EventAiDefinitions>,
        build: EventAiBuild,
        created_by_spell: Option<u32>,
    ) -> Result<Self> {
        let event = SourceEventAi::construct_inherited(selected, objects, definitions, build)?;
        Self::from_event(event, objects, units, ais, created_by_spell)
    }
    pub(crate) fn from_event(
        event: SourceEventAi,
        objects: &SourceObjects,
        units: &mut SourceUnits,
        ais: &mut SourceCreatureAis,
        created_by_spell: Option<u32>,
    ) -> Result<Self> {
        event.validate_inherited(objects)?;
        let unit = event.selected.object();
        let ai = ais.get_mut(objects, unit)?;
        let guardian =
            event.selected.executable().builtin_kind() == Some(BuiltinCreatureAi::Guardian);
        if guardian {
            // GuardianAI.cpp:27-38 asserts owner, then changes only follow defaults.
            let owner = event
                .selected
                .inputs()
                .control
                .owner
                .ok_or_else(|| refuse("execution.guardian_ai.f5_15_required_owner"))?;
            objects.get(owner)?;
            ai.follow_angle = std::f32::consts::FRAC_PI_2;
            ai.follow_distance = 1.5;
            let spell = created_by_spell
                .ok_or_else(|| refuse("execution.guardian_ai.f5_15_created_by_spell_owner"))?;
            if spell == 11939 {
                ai.follow_distance = 3.0;
            }
        }
        units.bind_ai_selection(objects, &event.selected)?;
        units.get_mut(objects, unit)?.has_ai = true;
        Ok(Self {
            event,
            victim_guid: None,
            requested_follower: None,
            follow_default: guardian,
        })
    }
    fn guardian(&self) -> bool {
        self.event.selected.executable().builtin_kind() == Some(BuiltinCreatureAi::Guardian)
    }
    pub fn inventory(
        &self,
        objects: &SourceObjects,
        ais: &SourceCreatureAis,
    ) -> Result<serde_json::Value> {
        Ok(
            serde_json::json!({"version":1,"event":self.event.inventory(objects)?,
            "base":ais.get(objects,self.event.selected.object())?,"victim_guid":self.victim_guid,
            "requested_follower":self.requested_follower,"follow_default":self.follow_default,
            "restore":"execution.summoned_ai.f7_restore_owner"}),
        )
    }
    pub fn update<O: SummonedAiExecutionOwner>(
        &mut self,
        diff: u32,
        execution: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        self.event.validate_inherited(execution.queries.objects)?;
        let unit = self.event.selected.object();
        if self.guardian() {
            // GuardianAI.cpp:56-73 performs assistance before inherited UnitAI timers.
            let react = execution
                .ais
                .get(execution.queries.objects, unit)?
                .base
                .react_state;
            if matches!(react, 1 | 2) {
                let fields = execution
                    .queries
                    .units
                    .get(execution.queries.objects, unit)?;
                if let Some(owner) = fields.owner {
                    if !fields.in_combat()
                        && execution
                            .queries
                            .units
                            .get(execution.queries.objects, owner)?
                            .in_combat()
                        && (fields.pet_kind != Some(CreaturePetKind::Other)
                            || !execution.owner.mini_pet(unit)?)
                    {
                        let target = execution.owner.helper_attacker(owner)?;
                        self.attack_start(target, execution)?;
                    }
                }
            }
            let combat = inherited_update_prefix(
                execution.ais.get_mut(execution.queries.objects, unit)?,
                diff,
                execution.queries.objects,
                execution.queries.units,
                &mut SourceAiDependencies {
                    threats: execution.threats,
                    owner: execution.owner,
                },
            )?;
            self.event
                .update_timers(diff, &mut EventAiDispatcher(execution))?;
            return inherited_update_combat(
                execution.ais.get_mut(execution.queries.objects, unit)?,
                combat,
                execution.queries.objects,
                execution.queries.units,
                &mut SourceAiDependencies {
                    threats: execution.threats,
                    owner: execution.owner,
                },
            );
        }
        // TotemAI.cpp:60 deliberately omits generic timers and hostile selection.
        self.event
            .update_timers(diff, &mut EventAiDispatcher(execution))?;
        require(
            execution
                .queries
                .units
                .get(execution.queries.objects, unit)?
                .totem,
            "execution.totem_ai.non_totem_downcast_undefined",
        )?; // TotemAI.cpp:62,128.
        if execution.owner.totem_type(unit)? != 1 {
            return Ok(());
        }
        if !execution.queries.objects.get(unit)?.alive
            || execution
                .queries
                .units
                .get(execution.queries.objects, unit)?
                .casting_non_melee
        {
            return Ok(());
        }
        let spell_id = execution.owner.totem_spell(unit)?;
        let Some(spell) = execution.owner.spell(spell_id)? else {
            return Ok(());
        };
        if !execution.owner.spell_ready(unit, spell)? {
            return Ok(());
        }
        let range = execution.owner.spell_max_range(spell.range_index)?;
        let objects = execution.queries.objects;
        let map = objects
            .get(unit)?
            .map
            .ok_or_else(|| refuse("execution.totem_ai.map_owner"))?;
        let mut victim = self.victim_guid.and_then(|guid| {
            // Map.cpp:2298-2310,2371-2376 reads creature/pet storage without an in-world filter.
            match guid.high() {
                0 | 0xf130 | 0xf140 => objects.lookup_world_object(map, guid),
                _ => None,
            }
        });
        let valid = if let Some(target) = victim {
            execution
                .owner
                .totem_query(unit, target, TotemQuery::CanAttackOnSight)?
                && execution.owner.totem_query(
                    unit,
                    target,
                    TotemQuery::WithinDistance(range.to_bits()),
                )?
                && !execution
                    .owner
                    .totem_query(unit, target, TotemQuery::CanAssist)?
                && execution
                    .owner
                    .totem_query(unit, target, TotemQuery::VisibleForOrDetect)?
        } else {
            false
        };
        if !valid {
            victim = if range != 0.0 {
                execution.owner.nearest_totem_target(
                    unit,
                    execution.queries.units.get(objects, unit)?.owner,
                    range,
                )?
            } else {
                Some(unit)
            };
        }
        if let Some(target) = victim {
            self.victim_guid = Some(
                objects
                    .get(target)?
                    .guid
                    .ok_or_else(|| refuse("execution.unit.guid_absent"))?,
            );
            execution.owner.set_in_front(unit, target)?;
            execution.owner.cast_totem_spell(unit, target, spell_id)?;
        } else {
            self.victim_guid = None;
        }
        Ok(())
    }
    pub fn attack_start<O: SummonedAiExecutionOwner>(
        &mut self,
        target: Option<ObjectHandle>,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        self.event.validate_inherited(e.queries.objects)?;
        if self.guardian() {
            e.ais
                .get_mut(e.queries.objects, self.event.selected.object())?
                .attack_start(
                    target,
                    e.queries.objects,
                    e.queries.units,
                    &mut SourceAiDependencies {
                        threats: e.threats,
                        owner: e.owner,
                    },
                )?;
        } // TotemAI.cpp:122-124 is genuinely empty.
        Ok(())
    }
    pub fn attacked_by<O: SummonedAiExecutionOwner>(
        &mut self,
        attacker: Option<ObjectHandle>,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        let fields = e
            .queries
            .units
            .get(e.queries.objects, self.event.selected.object())?;
        if !fields.in_combat() && fields.victim.is_none() {
            self.attack_start(attacker, e)?;
        }
        Ok(())
    }
    pub fn is_visible(&self) -> Result<bool> {
        if self.guardian() {
            Err(refuse("execution.builtin_ai.f6_visibility_owner"))
        } else {
            Ok(false)
        }
    }
    fn events<O: EventAiExecutionOwner>(
        &mut self,
        kind: u32,
        invoker: Option<ObjectHandle>,
        owner: &mut O,
    ) -> Result<()> {
        self.event.ensure_depth();
        let call = EventAiInvocation {
            invoker,
            sender: None,
        };
        for i in 0..self.event.holders.len() {
            if self.event.holders[i].event.kind == kind {
                self.event.ready_event(i, call, owner)?;
            }
        }
        self.event.dispatch_ready(call, owner)
    }
    pub fn enter_evade<O: SummonedAiExecutionOwner>(
        &mut self,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        self.event.validate_inherited(e.queries.objects)?;
        let unit = self.event.selected.object();
        if self.guardian() {
            e.ais.get_mut(e.queries.objects, unit)?.base.enter_evade(
                e.queries.objects,
                e.queries.units,
                e.owner,
            )?;
        } else {
            e.owner.call(
                unit,
                UnitAiCall::CombatStopWithPets,
                e.queries.units,
                e.queries.objects,
            )?;
        }
        // Both overrides omit EventAI's aggregation-despawn tail.
        self.events(7, None, e)
    }
    pub fn just_respawned<O: SummonedAiExecutionOwner>(
        &mut self,
        settings: EventAiRespawnSettings,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        self.event.validate_inherited(e.queries.objects)?;
        let unit = self.event.selected.object();
        let owner = e.queries.units.get(e.queries.objects, unit)?.owner;
        if self.guardian() && owner.is_none() {
            return Ok(());
        }
        // The lifecycle needs two borrows of one RNG. The adapter forwards to the same owner.
        {
            let objects = e.queries.objects;
            let cell = std::cell::RefCell::new(&mut *e);
            self.event
                .just_respawned(objects, settings, &mut Shared(&cell), &mut Shared(&cell))?;
        }
        if self.guardian() && self.follow_default {
            let owner = owner.expect("guardian owner checked");
            if e.queries.objects.get(owner)?.kind == SourceObjectKind::Player {
                let (angle, distance) = e.owner.follow_data(unit, owner)?;
                let ai = e.ais.get_mut(e.queries.objects, unit)?;
                ai.follow_angle = angle;
                ai.follow_distance = distance;
                let guid = e
                    .queries
                    .objects
                    .get(owner)?
                    .guid
                    .ok_or_else(|| refuse("execution.guardian_ai.f5_15_owner_guid_owner"))?;
                self.requested_follower = Some((owner, guid));
            }
            let ai = e.ais.get(e.queries.objects, unit)?;
            e.owner
                .move_follow(unit, owner, ai.follow_distance, ai.follow_angle)?;
        }
        Ok(())
    }
    pub fn just_died<O: SummonedAiExecutionOwner>(
        &mut self,
        killer: Option<ObjectHandle>,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        let unit = self.event.selected.object();
        e.ais.get_mut(e.queries.objects, unit)?.base.just_died(
            e.queries.objects,
            e.queries.units,
            e.owner,
        )?;
        // CreatureEventAI.cpp:1495-1513, without inventing a summon executor.
        reset_event(&mut self.event, e)?;
        if self.event.throw_mask & 1 != 0 {
            return Err(refuse(
                "execution.summoned_ai.f5_06_death_event_broadcast_owner",
            ));
        }
        self.events(6, killer, e)?;
        self.event.phase = 0;
        if self.event.despawn_mask & 4 != 0 && !self.event.despawn_guids.is_empty() {
            return Err(refuse(
                "execution.summoned_ai.f5_15_death_aggregation_owner",
            ));
        }
        self.on_unsummon(e)
    }
    pub fn on_unsummon<O: SummonedAiExecutionOwner>(
        &mut self,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        self.event.validate_inherited(e.queries.objects)?;
        if let (true, Some((requested, guid))) = (self.guardian(), self.requested_follower) {
            let unit = self.event.selected.object();
            let current = e.queries.units.get(e.queries.objects, unit)?.owner;
            let matches_owner = match current {
                None => true,
                Some(owner) if owner == requested => true,
                Some(owner) => e.queries.objects.get(owner)?.guid == Some(guid),
            };
            if matches_owner {
                let map = e
                    .queries
                    .objects
                    .get(unit)?
                    .map
                    .ok_or_else(|| refuse("execution.guardian_ai.map_owner"))?;
                if let Some(player) = e.queries.objects.lookup(map, LiveLookupKind::Player, guid) {
                    e.owner.relinquish_follow(unit, player)?;
                }
            }
        }
        Ok(())
    }
    pub fn move_in_line_of_sight<O: SummonedAiExecutionOwner>(
        &mut self,
        who: ObjectHandle,
        config: &EventAiLoaderConfig,
        e: &mut SourceAiExecution<'_, O>,
    ) -> Result<()> {
        self.event.validate_inherited(e.queries.objects)?;
        if !self.guardian() {
            return Ok(());
        }
        self.event.ensure_depth();
        let unit = self.event.selected.object();
        if self.event.has_ooc_los
            && e.queries
                .units
                .get(e.queries.objects, unit)?
                .victim
                .is_none()
        {
            // CreatureEventAI.cpp:1648-1655, do not substitute the event-ready predicate.
            for i in 0..self.event.holders.len() {
                let event = &self.event.holders[i].event;
                if event.kind != 10
                    || (event.params[5] != 0
                        && e.queries.objects.get(who)?.kind != SourceObjectKind::Player)
                {
                    continue;
                }
                let enemy = e
                    .owner
                    .guardian_sight(unit, who, GuardianSightQuery::Enemy)?;
                if (event.params[0] != 0) != enemy
                    && e.owner.guardian_sight(
                        unit,
                        who,
                        GuardianSightQuery::WithinDistance((event.params[1] as f32).to_bits()),
                    )?
                    && e.owner
                        .guardian_sight(unit, who, GuardianSightQuery::LineOfSight)?
                {
                    self.event.ready_event(
                        i,
                        EventAiInvocation {
                            invoker: Some(who),
                            sender: None,
                        },
                        e,
                    )?;
                }
            }
            self.event.dispatch_ready(
                EventAiInvocation {
                    invoker: Some(who),
                    sender: None,
                },
                e,
            )?;
        }
        e.ais
            .get_mut(e.queries.objects, unit)?
            .base
            .move_in_line_of_sight(who, e.queries.objects, e.queries.units, config, e.owner)
    }
}

// Lifecycle and RNG calls alternate synchronously; both borrow the same live owner.
struct Shared<'a, 'b, O>(&'a std::cell::RefCell<&'b mut O>);
impl<O: CreatureRandom> CreatureRandom for Shared<'_, '_, O> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.0.borrow_mut().creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.0.borrow_mut().creature_urand(min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.0.borrow_mut().creature_urand_default()
    }
}
impl<O: EventAiExecutionOwner> EventAiLifecycleOwner for Shared<'_, '_, O> {
    fn callback(&mut self, s: &mut SourceEventAi, c: EventAiCallback) -> Result<()> {
        self.0.borrow_mut().lifecycle(s, c)
    }
    fn check_and_ready(
        &mut self,
        s: &mut SourceEventAi,
        i: usize,
        invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        EventAiDispatcher(&mut **self.0.borrow_mut()).check_and_ready(s, i, invoker)
    }
    fn process_event(
        &mut self,
        s: &mut SourceEventAi,
        i: usize,
        invoker: Option<ObjectHandle>,
    ) -> Result<()> {
        EventAiDispatcher(&mut **self.0.borrow_mut()).process_event(s, i, invoker)
    }
    fn spawned_conditions(&mut self, s: &SourceEventAi, i: usize) -> Result<bool> {
        s.spawned_event_conditions(i, &mut **self.0.borrow_mut())
    }
}
fn reset_event<O: EventAiExecutionOwner>(s: &mut SourceEventAi, e: &mut O) -> Result<()> {
    let cell = std::cell::RefCell::new(e);
    s.reset(&mut Shared(&cell), &mut Shared(&cell))
}
