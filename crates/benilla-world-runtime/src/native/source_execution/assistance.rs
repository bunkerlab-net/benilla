// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Creature.cpp:2159-2227; Unit.cpp:559-581; UnitAI.cpp:567-653.
use super::*;

pub const AI_EVENT_CALL_ASSISTANCE: u32 = 13;

// Deviation: Preserve the raw u32 event number, because C++ leaves the non-fixed AIEventType enum indeterminate outside 0..1023 (AIDefines.h:22-56) and the owner chose intent on 2026-11-12 so scourge_invasion.cpp:956-965's 7166 reaches its receiver unchanged; ScriptMgr.cpp:2759-2775.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceAiEventType(u32);
impl SourceAiEventType {
    pub const CALL_ASSISTANCE: Self = Self(AI_EVENT_CALL_ASSISTANCE);
    pub const fn raw(self) -> u32 {
        self.0
    }
}
impl From<u32> for SourceAiEventType {
    fn from(raw: u32) -> Self {
        Self(raw)
    }
}
/// GUIDs are looked up again at Execute, not pinned to the captured generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AssistanceCapture {
    pub sender: ObjectHandle,
    pub enemy: Option<SourceGuid>,
    pub receivers: Vec<SourceGuid>,
    pub kind: AssistanceEvent,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistanceEvent {
    Aggro {
        group: Option<GroupIdentity>,
        call_assistance: bool,
    },
    Around {
        event: SourceAiEventType,
        misc: u32,
    },
    ReenableHelp,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistanceQuery {
    CanAssistInCombatAgainst(ObjectHandle),
    CanAssist,
    CanAttack,
    AttackOnSight,
    Visible,
    Accessible,
    Distance(u32),
    Los,
    CanInitiate,
    Fleeing,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistanceVisit {
    AllAssist,
    AllUnits,
    GridHelp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AssistanceAiBase {
    Unit,
    Creature,
}
/// Visitors supply traversal membership in source order, never preapproved receivers.
/// CreatureList is std::list, EntitiesMgr.h:46; no GUID or pointer sorting.
pub trait AssistanceExecutionOwner {
    fn assistance_ai_base(&mut self, _receiver: ObjectHandle) -> Result<AssistanceAiBase> {
        Err(refuse(
            "execution.assistance.selected_ai_base_binding_owner",
        ))
    }
    fn visit(
        &mut self,
        _sender: ObjectHandle,
        _radius: u32,
        _kind: AssistanceVisit,
    ) -> Result<Vec<ObjectHandle>> {
        Err(refuse("execution.assistance.f6_grid_visitor_owner"))
    }
    fn query(
        &mut self,
        _unit: ObjectHandle,
        _target: ObjectHandle,
        query: AssistanceQuery,
    ) -> Result<bool> {
        Err(refuse(match query {
            AssistanceQuery::Distance(_) => "execution.assistance.f6_distance_owner",
            AssistanceQuery::Los => "execution.assistance.f6_los_owner",
            AssistanceQuery::Visible => "execution.assistance.f6_visibility_owner",
            AssistanceQuery::Accessible => "execution.assistance.f6_accessibility_owner",
            AssistanceQuery::Fleeing => "execution.assistance.f5_05_ai_order_owner",
            _ => "execution.assistance.f5_unit_relationship_owner",
        }))
    }
    #[allow(clippy::too_many_arguments)]
    fn receive_ai_event(
        &mut self,
        _receiver: ObjectHandle,
        _sender: ObjectHandle,
        _enemy: Option<ObjectHandle>,
        _event: SourceAiEventType,
        _misc: u32,
        _units: &mut SourceUnits,
        _objects: &SourceObjects,
    ) -> Result<()> {
        Err(refuse("execution.assistance.f5_03_receive_ai_event_owner"))
    }
    fn attack_start(
        &mut self,
        _receiver: ObjectHandle,
        _enemy: ObjectHandle,
        _units: &mut SourceUnits,
        _objects: &SourceObjects,
    ) -> Result<()> {
        Err(refuse(
            "execution.assistance.f5_04_virtual_attack_start_owner",
        ))
    }
    /// Return the actual optional faction flags, not a chosen response.
    fn faction_flags(&mut self, _receiver: ObjectHandle) -> Result<Option<u32>> {
        Err(refuse("execution.assistance.faction_template_owner"))
    }
    fn panic(&mut self, _receiver: ObjectHandle, _millis: u32) -> Result<bool> {
        Err(refuse("execution.assistance.f5_05_panic_owner"))
    }
    fn flee_order(&mut self, _receiver: ObjectHandle) -> Result<()> {
        Err(refuse("execution.assistance.f5_05_flee_order_owner"))
    }
    fn interrupt_panic(&mut self, _receiver: ObjectHandle) -> Result<()> {
        Err(refuse("execution.assistance.f5_05_interrupt_panic_owner"))
    }
    fn linking_aggro(
        &mut self,
        _sender: ObjectHandle,
        _enemy: ObjectHandle,
        _units: &mut SourceUnits,
        _objects: &SourceObjects,
        _queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        Err(refuse("execution.assistance.f4_linking_binding_owner"))
    }
    fn group_aggro(
        &mut self,
        _group: GroupIdentity,
        _enemy: ObjectHandle,
        _units: &mut SourceUnits,
        _objects: &SourceObjects,
        _queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        Err(refuse("execution.assistance.f4_group_binding_owner"))
    }
}
pub trait AssistanceQueue {
    fn add_assistance(&mut self, delay: u32, capture: AssistanceCapture) -> Result<u64>;
}

pub struct SourceAssistance;
impl SourceAssistance {
    /// The factory's F4 linking phase supplies this flag, never a template guess.
    pub fn bind_construction(
        state: &CreatureConstruction,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        let fields = units.get_mut(objects, state.object)?;
        require(
            fields.entry == state.template.entry,
            "execution.assistance.effective_entry",
        )?;
        fields.linking_event_trigger = Some(state.linking_trigger);
        Ok(())
    }
    fn guid(objects: &SourceObjects, unit: ObjectHandle) -> Result<SourceGuid> {
        objects
            .get(unit)?
            .guid
            .ok_or_else(|| refuse("execution.assistance.guid_missing"))
    }
    fn resolve(
        objects: &SourceObjects,
        sender: ObjectHandle,
        guid: SourceGuid,
        creature: bool,
    ) -> Result<Option<ObjectHandle>> {
        let map = objects
            .get(sender)?
            .map
            .ok_or_else(|| refuse("execution.assistance.map_missing"))?;
        let unit = match guid.high() {
            0 | 0xf130 | 0xf140 => objects.lookup_world_object(map, guid),
            _ => None,
        };
        Ok(unit.filter(|h| {
            !creature
                || matches!(
                    objects.get(*h).map(|o| o.kind),
                    Ok(SourceObjectKind::Creature | SourceObjectKind::Pet)
                )
        }))
    }
    fn radius(fields: &SourceUnitFields, family_radius: u32) -> u32 {
        if fields.call_for_help > 0 {
            (fields.call_for_help as f32).to_bits()
        } else {
            family_radius
        }
    }
    fn receivers(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        radius: u32,
        kind: AssistanceVisit,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<Vec<ObjectHandle>> {
        let mut receivers = Vec::new();
        for receiver in owner.visit(sender, radius, kind)? {
            if kind != AssistanceVisit::AllUnits && receiver == sender {
                continue;
            }
            if kind == AssistanceVisit::AllUnits && !objects.get(receiver)?.alive {
                continue;
            }
            if kind == AssistanceVisit::AllAssist {
                let enemy = enemy
                    .ok_or_else(|| refuse("execution.assistance.null_enemy_relationship_owner"))?;
                if !owner.query(
                    receiver,
                    sender,
                    AssistanceQuery::CanAssistInCombatAgainst(enemy),
                )? {
                    continue;
                }
            }
            if !owner.query(sender, receiver, AssistanceQuery::Distance(radius))? {
                continue;
            }
            if kind != AssistanceVisit::AllUnits
                && !owner.query(sender, receiver, AssistanceQuery::Los)?
            {
                continue;
            }
            receivers.push(receiver);
        }
        Ok(receivers)
    }
    /// Source quirk: returns the OLD latch, not its negation, Creature.cpp:2180,2197.
    pub fn mark_on_pull(
        sender: ObjectHandle,
        enemy: ObjectHandle,
        family_radius: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<(bool, Vec<SourceGuid>)> {
        let fields = units.get_mut(objects, sender)?;
        let stored = fields.already_called_assistance;
        fields.already_called_assistance = true;
        if !fields.can_call_assistance {
            return Ok((false, Vec::new()));
        }
        let radius = Self::radius(fields, family_radius);
        Ok((
            stored,
            Self::receivers(
                sender,
                Some(enemy),
                radius,
                AssistanceVisit::AllAssist,
                objects,
                owner,
            )?
            .into_iter()
            .map(|h| Self::guid(objects, h))
            .collect::<Result<Vec<_>>>()?,
        ))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn trigger_aggro(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        group: Option<GroupIdentity>,
        family_radius: u32,
        delay: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
        queue: &mut impl AssistanceQueue,
    ) -> Result<Option<u64>> {
        // Object.h:590 tests TYPEID_UNIT, including Pet subclasses.
        if !matches!(
            objects.get(sender)?.kind,
            SourceObjectKind::Creature | SourceObjectKind::Pet
        ) || enemy.is_none()
        {
            return Ok(None);
        }
        let enemy = enemy.unwrap();
        let (call_assistance, receivers) =
            Self::mark_on_pull(sender, enemy, family_radius, units, objects, owner)?;
        let enemy = Some(Self::guid(objects, enemy)?);
        queue
            .add_assistance(
                delay,
                AssistanceCapture {
                    sender,
                    enemy,
                    receivers,
                    kind: AssistanceEvent::Aggro {
                        group,
                        call_assistance,
                    },
                },
            )
            .map(Some)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn call_assistance(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        family_radius: u32,
        delay: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        let fields = units.get_mut(objects, sender)?;
        if fields.already_called_assistance || enemy.is_none() || fields.charmer.is_some() {
            return Ok(());
        }
        require(fields.has_ai, "execution.assistance.ai_assertion")?;
        fields.already_called_assistance = true;
        if !fields.can_call_assistance {
            return Ok(());
        }
        let radius = Self::radius(fields, family_radius);
        Self::send_around(
            sender,
            enemy,
            SourceAiEventType::CALL_ASSISTANCE,
            0,
            delay,
            radius,
            units,
            objects,
            owner,
            queue,
        )
    }
    #[allow(clippy::too_many_arguments)]
    pub fn send_around(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        event: SourceAiEventType,
        misc: u32,
        delay: u32,
        radius: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        if f32::from_bits(radius) <= 0.0 || f32::from_bits(radius).is_nan() {
            return Ok(());
        }
        let raw = event.raw();
        let kind = if (5..=11).contains(&raw) && raw != 7 || raw > 100 {
            AssistanceVisit::AllUnits
        } else {
            AssistanceVisit::AllAssist
        };
        let receivers = Self::receivers(sender, enemy, radius, kind, objects, owner)?;
        if receivers.is_empty() {
            return Ok(());
        }
        if delay != 0 {
            let enemy = enemy.map(|e| Self::guid(objects, e)).transpose()?;
            let receivers = receivers
                .into_iter()
                .map(|h| Self::guid(objects, h))
                .collect::<Result<Vec<_>>>()?;
            queue.add_assistance(
                delay,
                AssistanceCapture {
                    sender,
                    enemy,
                    receivers,
                    kind: AssistanceEvent::Around { event, misc },
                },
            )?;
        } else {
            for receiver in receivers {
                Self::deliver_one(receiver, sender, enemy, event, misc, units, objects, owner)?;
            }
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn deliver_one(
        receiver: ObjectHandle,
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        event: SourceAiEventType,
        misc: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        require(
            units.get(objects, receiver)?.has_ai,
            "execution.assistance.receiver_ai_null_undefined",
        )?;
        owner.receive_ai_event(receiver, sender, enemy, event, misc, units, objects)?;
        if event == SourceAiEventType::CALL_ASSISTANCE {
            Self::handle_call(receiver, sender, enemy, units, objects, owner)?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn send_immediate(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        event: SourceAiEventType,
        misc: u32,
        radius: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        if f32::from_bits(radius) <= 0. || f32::from_bits(radius).is_nan() {
            return Ok(());
        }
        let raw = event.raw();
        let kind = if (5..=11).contains(&raw) && raw != 7 || raw > 100 {
            AssistanceVisit::AllUnits
        } else {
            AssistanceVisit::AllAssist
        };
        for receiver in Self::receivers(sender, enemy, radius, kind, objects, owner)? {
            Self::deliver_one(receiver, sender, enemy, event, misc, units, objects, owner)?;
        }
        Ok(())
    }
    pub fn execute(
        capture: &AssistanceCapture,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        objects.get(capture.sender)?;
        if capture.kind == AssistanceEvent::ReenableHelp {
            units.get_mut(objects, capture.sender)?.can_check_help = true;
            return Ok(());
        }
        let Some(enemy) = capture
            .enemy
            .map(|g| Self::resolve(objects, capture.sender, g, false))
            .transpose()?
            .flatten()
        else {
            return Ok(());
        };
        match capture.kind {
            AssistanceEvent::Aggro {
                group,
                call_assistance,
            } => {
                if !units.get(objects, capture.sender)?.in_combat() {
                    return Ok(());
                }
                if units
                    .get(objects, capture.sender)?
                    .linking_event_trigger
                    .ok_or_else(|| refuse("execution.assistance.f4_linking_flag_owner"))?
                {
                    owner.linking_aggro(capture.sender, enemy, units, objects, queue)?;
                }
                if let Some(group) = group {
                    owner.group_aggro(group, enemy, units, objects, queue)?;
                }
                if call_assistance && units.get(objects, capture.sender)?.charmer.is_none() {
                    require(
                        units.get(objects, capture.sender)?.has_ai,
                        "execution.assistance.ai_assertion",
                    )?;
                    Self::deliver(
                        capture.sender,
                        Some(enemy),
                        &capture.receivers,
                        SourceAiEventType::CALL_ASSISTANCE,
                        0,
                        units,
                        objects,
                        owner,
                    )?;
                }
            }
            AssistanceEvent::Around { event, misc } => Self::deliver(
                capture.sender,
                Some(enemy),
                &capture.receivers,
                event,
                misc,
                units,
                objects,
                owner,
            )?,
            AssistanceEvent::ReenableHelp => unreachable!(),
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    fn deliver(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        receivers: &[SourceGuid],
        event: SourceAiEventType,
        misc: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        for &guid in receivers {
            if let Some(receiver) = Self::resolve(objects, sender, guid, true)? {
                Self::deliver_one(receiver, sender, enemy, event, misc, units, objects, owner)?;
            }
        }
        Ok(())
    }
    /// ReceiveAIEvent precedes live eligibility, CreatureAI.cpp:224-232.
    pub fn handle_call(
        receiver: ObjectHandle,
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        if owner.assistance_ai_base(receiver)? == AssistanceAiBase::Unit {
            return Ok(()); // UnitAI.h:497 is genuinely empty.
        }
        let Some(enemy) = enemy else {
            return Ok(());
        };
        if units.get(objects, receiver)?.critter {
            return Ok(());
        }
        if owner.query(receiver, sender, AssistanceQuery::CanAssist)?
            && owner.query(receiver, enemy, AssistanceQuery::AttackOnSight)?
            && owner.query(enemy, receiver, AssistanceQuery::Visible)?
        {
            units.get_mut(objects, receiver)?.already_called_assistance = true;
            Self::on_help(receiver, enemy, units, objects, owner)?;
        }
        Ok(())
    }
    pub fn on_help(
        receiver: ObjectHandle,
        enemy: ObjectHandle,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        if owner.assistance_ai_base(receiver)? == AssistanceAiBase::Unit {
            return Ok(()); // UnitAI.h:420 is genuinely empty.
        }
        if let Some(flags) = owner.faction_flags(receiver)? {
            if flags & 0x400 != 0 {
                if owner.panic(receiver, 10000)? {
                    owner.flee_order(receiver)?;
                }
                return Ok(());
            }
            if flags & 1 == 0 {
                return Ok(());
            }
        }
        owner.attack_start(receiver, enemy, units, objects)
    }
    pub fn call_for_help(
        sender: ObjectHandle,
        radius: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        let fields = units.get(objects, sender)?;
        let Some(enemy) = fields.victim else {
            return Ok(());
        };
        if objects.get(sender)?.kind == SourceObjectKind::Pet || fields.charmer.is_some() {
            return Ok(());
        }
        let radius = if f32::from_bits(radius) <= 0.0 {
            (fields.call_for_help as f32).to_bits()
        } else {
            radius
        };
        for receiver in owner.visit(sender, radius, AssistanceVisit::GridHelp)? {
            if receiver == sender
                || !owner.query(receiver, sender, AssistanceQuery::CanAssist)?
                || !owner.query(receiver, enemy, AssistanceQuery::CanAttack)?
            {
                continue;
            }
            if !owner.query(sender, receiver, AssistanceQuery::Distance(radius))?
                || !owner.query(sender, receiver, AssistanceQuery::Los)?
            {
                continue;
            }
            if units.get(objects, receiver)?.has_ai {
                Self::on_help(receiver, enemy, units, objects, owner)?;
            }
        }
        Ok(())
    }
    /// SpawnGroup.cpp:548 writes the latch before recursively engaging.
    pub fn group_callback(
        call: GroupCallback,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        if let GroupCallback::SetNoCallAssistance { object } = call {
            units.get_mut(objects, object)?.already_called_assistance = true;
            Ok(())
        } else {
            Err(call.missing_owner())
        }
    }
    /// Unit.cpp:8007-8013, only after EnterCombat and instance callbacks.
    #[allow(clippy::too_many_arguments)]
    pub fn combat_entry_after_callbacks(
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        group: Option<GroupIdentity>,
        family_radius: u32,
        delay: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        units.get_mut(objects, sender)?.can_check_help = false;
        queue.add_assistance(
            delay,
            AssistanceCapture {
                sender,
                enemy: None,
                receivers: Vec::new(),
                kind: AssistanceEvent::ReenableHelp,
            },
        )?;
        Self::trigger_aggro(
            sender,
            enemy,
            group,
            family_radius,
            delay,
            units,
            objects,
            owner,
            queue,
        )?;
        Ok(())
    }
    /// Unit.h:1239-1247 uses the pointer-ordered AttackerSet, not insertion order.
    pub fn attacker_for_helper(
        unit: ObjectHandle,
        units: &SourceUnits,
        objects: &SourceObjects,
    ) -> Result<Option<ObjectHandle>> {
        let fields = units.get(objects, unit)?;
        if let Some(victim) = fields.victim {
            objects.get(victim)?;
            return Ok(Some(victim));
        }
        let mut first = None;
        for &attacker in &fields.attackers {
            let pointer = objects
                .get(attacker)?
                .source_allocation
                .ok_or_else(|| refuse("execution.assistance.attacker_pointer_order_unbound"))?;
            if first.is_none_or(|(p, _)| pointer < p) {
                first = Some((pointer, attacker));
            }
        }
        Ok(first.map(|(_, h)| h))
    }
    /// UnitAI.cpp:495-521, including InterruptPanic after AttackStart.
    pub fn check_for_help(
        me: ObjectHandle,
        who: ObjectHandle,
        radius: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
        owner: &mut impl AssistanceExecutionOwner,
    ) -> Result<()> {
        let Some(victim) = Self::attacker_for_helper(who, units, objects)? else {
            return Ok(());
        };
        let m = units.get(objects, me)?;
        let w = units.get(objects, who)?;
        if m.in_combat()
            || !m.can_call_assistance
            || !w.can_call_assistance
            || !m.can_check_help
            || !w.can_check_help
        {
            return Ok(());
        }
        if w.state & 0x2408 != 0 {
            return Ok(());
        }
        if owner.query(me, me, AssistanceQuery::CanInitiate)?
            && owner.query(me, victim, AssistanceQuery::AttackOnSight)?
            && owner.query(victim, me, AssistanceQuery::Accessible)?
            && owner.query(victim, me, AssistanceQuery::Visible)?
            && owner.query(me, who, AssistanceQuery::Distance(radius))?
            && owner.query(me, who, AssistanceQuery::Los)?
            && owner.query(me, who, AssistanceQuery::CanAssistInCombatAgainst(victim))?
        {
            owner.attack_start(me, victim, units, objects)?;
            if units.get(objects, who)?.has_ai && owner.query(who, who, AssistanceQuery::Fleeing)? {
                owner.interrupt_panic(who)?;
            }
        }
        Ok(())
    }
    /// CombatStop resets only the latch, never cancels events, Unit.cpp:6374.
    pub fn combat_stopped(
        unit: ObjectHandle,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        units.get_mut(objects, unit)?.already_called_assistance = false;
        Ok(())
    }
}
pub struct SourceAssistanceEvents<'a, O, F> {
    pub objects: &'a SourceObjects,
    pub units: &'a mut SourceUnits,
    pub owner: &'a mut O,
    pub other_events: &'a mut F,
}
impl<O: AssistanceExecutionOwner, F: UnitEventOwner> UnitEventOwner
    for SourceAssistanceEvents<'_, O, F>
{
    fn forced_despawn(&mut self, only_alive: bool) -> Result<()> {
        self.other_events.forced_despawn(only_alive)
    }
    fn assistance(
        &mut self,
        capture: &AssistanceCapture,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        SourceAssistance::execute(capture, self.units, self.objects, self.owner, queue)
    }
}

/// Compose outside SourceAiDependencies so both operations share live Unit fields.
pub struct SourceAssistanceDependencies<'a, O> {
    pub owner: &'a mut O,
}
impl<O: UnitAiExecutionOwner + AssistanceExecutionOwner> UnitAiExecutionOwner
    for SourceAssistanceDependencies<'_, O>
{
    fn call(
        &mut self,
        unit: ObjectHandle,
        call: UnitAiCall,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        match call {
            UnitAiCall::CallForHelp { radius_bits } => {
                return SourceAssistance::call_for_help(
                    unit,
                    radius_bits,
                    units,
                    objects,
                    self.owner,
                )
            }
            UnitAiCall::SendAiEventAround {
                event,
                invoker,
                radius_bits,
            } => {
                return SourceAssistance::send_immediate(
                    unit,
                    Some(invoker),
                    event,
                    0,
                    radius_bits,
                    units,
                    objects,
                    self.owner,
                )
            }
            _ => {}
        }
        if let UnitAiCall::CheckForHelp {
            target,
            radius_bits,
        } = call
        {
            return SourceAssistance::check_for_help(
                unit,
                target,
                radius_bits,
                units,
                objects,
                self.owner,
            );
        }
        self.owner.call(unit, call, units, objects)
    }
    fn execute_action(
        &mut self,
        ai: &mut SourceUnitAi,
        action: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.owner.execute_action(ai, action, units, objects)
    }
    fn sight_query(
        &mut self,
        unit: ObjectHandle,
        who: ObjectHandle,
        query: UnitAiSightQuery,
    ) -> Result<bool> {
        self.owner.sight_query(unit, who, query)
    }
    fn add_initial_threat(
        &mut self,
        unit: ObjectHandle,
        target: ObjectHandle,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.owner.add_initial_threat(unit, target, units, objects)
    }
    fn has_unattackable_aura(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.owner.has_unattackable_aura(unit)
    }
    fn area(&mut self, unit: ObjectHandle) -> Result<u32> {
        self.owner.area(unit)
    }
    fn c_rand(&mut self) -> Result<u32> {
        self.owner.c_rand()
    }
    fn spell_list_has_spells(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.owner.spell_list_has_spells(unit)
    }
    fn reachable(&mut self, unit: ObjectHandle) -> Result<bool> {
        self.owner.reachable(unit)
    }
}
impl<O: CreatureRandom> CreatureRandom for SourceAssistanceDependencies<'_, O> {
    fn creature_irand(&mut self, min: i32, max: i32) -> Result<i32> {
        self.owner.creature_irand(min, max)
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.owner.creature_urand(min, max)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.owner.creature_urand_default()
    }
}
