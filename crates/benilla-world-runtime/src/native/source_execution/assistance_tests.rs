use super::*;
use std::collections::BTreeSet;

fn fixture() -> (SourceObjects, SourceUnits, [ObjectHandle; 4]) {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let handles = std::array::from_fn(|i| {
        let h = objects.construct(1, SourceObjectKind::Creature).unwrap();
        objects
            .assign_guid(
                h,
                i as u32 + 1,
                SourceGuid::with_entry(HighGuid::Creature, 901, i as u32 + 1),
            )
            .unwrap();
        objects.get_mut(h).unwrap().map = Some(MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        });
        objects.add_to_world(h).unwrap();
        objects.get_mut(h).unwrap().alive = true;
        let mut f = SourceUnitFields::creature(&super::motion_master_tests::template());
        f.has_ai = true;
        f.linking_event_trigger = Some(false);
        units.register(&objects, h, f).unwrap();
        h
    });
    units.get_mut(&objects, handles[0]).unwrap().flags |= UNIT_FLAG_COMBAT;
    units.get_mut(&objects, handles[0]).unwrap().victim = Some(handles[1]);
    (objects, units, handles)
}
#[derive(Default)]
struct Authored {
    members: Vec<ObjectHandle>,
    denied: BTreeSet<ObjectHandle>,
    received: Vec<(ObjectHandle, u32, u32)>,
    attacked: Vec<ObjectHandle>,
    queries: Vec<AssistanceQuery>,
    visits: Vec<(u32, AssistanceVisit)>,
    order: Vec<&'static str>,
    flags: Option<u32>,
    recursive: Option<AssistanceCapture>,
    mutate_on_receive: bool,
    unit_ai_base: bool,
    event_ai: Option<SourceEventAi>,
    draws: u32,
}
impl UnitAiExecutionOwner for Authored {}
impl AssistanceExecutionOwner for Authored {
    fn assistance_ai_base(&mut self, _: ObjectHandle) -> Result<AssistanceAiBase> {
        Ok(if self.unit_ai_base {
            AssistanceAiBase::Unit
        } else {
            AssistanceAiBase::Creature
        })
    }
    fn visit(
        &mut self,
        _: ObjectHandle,
        radius: u32,
        kind: AssistanceVisit,
    ) -> Result<Vec<ObjectHandle>> {
        self.visits.push((radius, kind));
        Ok(self.members.clone())
    }
    fn query(
        &mut self,
        unit: ObjectHandle,
        _: ObjectHandle,
        query: AssistanceQuery,
    ) -> Result<bool> {
        self.queries.push(query);
        Ok(query != AssistanceQuery::Fleeing && !self.denied.contains(&unit))
    }
    fn receive_ai_event(
        &mut self,
        receiver: ObjectHandle,
        sender: ObjectHandle,
        enemy: Option<ObjectHandle>,
        event: SourceAiEventType,
        misc: u32,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.received.push((receiver, event.raw(), misc));
        self.order.push("receive");
        if let Some(mut state) = self.event_ai.take() {
            state.receive_ai_event(event.raw(), sender, enemy, self)?;
            self.event_ai = Some(state);
        }
        if self.mutate_on_receive {
            units.get_mut(objects, receiver)?.critter = true;
        }
        Ok(())
    }
    fn attack_start(
        &mut self,
        receiver: ObjectHandle,
        _: ObjectHandle,
        _: &mut SourceUnits,
        _: &SourceObjects,
    ) -> Result<()> {
        self.attacked.push(receiver);
        self.order.push("attack");
        Ok(())
    }
    fn faction_flags(&mut self, _: ObjectHandle) -> Result<Option<u32>> {
        Ok(self.flags)
    }
    fn panic(&mut self, _: ObjectHandle, millis: u32) -> Result<bool> {
        assert_eq!(millis, 10000);
        self.order.push("panic");
        Ok(true)
    }
    fn flee_order(&mut self, _: ObjectHandle) -> Result<()> {
        self.order.push("flee");
        Ok(())
    }
    fn linking_aggro(
        &mut self,
        _: ObjectHandle,
        _: ObjectHandle,
        _: &mut SourceUnits,
        _: &SourceObjects,
        queue: &mut impl AssistanceQueue,
    ) -> Result<()> {
        self.order.push("linking");
        if let Some(capture) = self.recursive.take() {
            queue.add_assistance(0, capture)?;
        }
        Ok(())
    }
    fn group_aggro(
        &mut self,
        _: GroupIdentity,
        _: ObjectHandle,
        _: &mut SourceUnits,
        _: &SourceObjects,
        _: &mut impl AssistanceQueue,
    ) -> Result<()> {
        self.order.push("group");
        Ok(())
    }
}
struct Other;
impl UnitEventOwner for Other {
    fn forced_despawn(&mut self, _: bool) -> Result<()> {
        Ok(())
    }
}
fn update(
    queue: &mut SourceUnitEvents,
    diff: u32,
    objects: &SourceObjects,
    units: &mut SourceUnits,
    owner: &mut Authored,
) {
    queue
        .update(
            diff,
            &mut SourceAssistanceEvents {
                objects,
                units,
                owner,
                other_events: &mut Other,
            },
        )
        .unwrap();
}
fn capture(objects: &SourceObjects, h: [ObjectHandle; 4]) -> AssistanceCapture {
    AssistanceCapture {
        sender: h[0],
        enemy: objects.get(h[1]).unwrap().guid,
        receivers: vec![objects.get(h[2]).unwrap().guid.unwrap()],
        kind: AssistanceEvent::Around {
            event: SourceAiEventType::CALL_ASSISTANCE,
            misc: 9,
        },
    }
}
#[test]
fn assistance_old_latch_is_captured_without_negation() {
    // Creature.cpp:2180,2197, including the surprising first-pull false result.
    let (objects, mut units, h) = fixture();
    let mut owner = Authored {
        members: vec![h[2]],
        ..Default::default()
    };
    let (first, receivers) = SourceAssistance::mark_on_pull(
        h[0],
        h[1],
        5f32.to_bits(),
        &mut units,
        &objects,
        &mut owner,
    )
    .unwrap();
    assert!(!first);
    assert_eq!(receivers, vec![objects.get(h[2]).unwrap().guid.unwrap()]);
    assert!(
        SourceAssistance::mark_on_pull(
            h[0],
            h[1],
            5f32.to_bits(),
            &mut units,
            &objects,
            &mut owner
        )
        .unwrap()
        .0
    );
}
#[test]
fn assistance_captured_membership_and_live_eligibility_are_distinct() {
    // UnitAI.cpp:570-596; CreatureAI.cpp:224-232.
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    let mut owner = Authored {
        members: vec![h[2]],
        ..Default::default()
    };
    SourceAssistance::call_assistance(
        h[0],
        Some(h[1]),
        5f32.to_bits(),
        20,
        &mut units,
        &objects,
        &mut owner,
        &mut queue,
    )
    .unwrap();
    owner.members = vec![h[3]];
    owner.denied.insert(h[2]);
    update(&mut queue, 20, &objects, &mut units, &mut owner);
    assert_eq!(owner.received, vec![(h[2], 13, 0)]);
    assert!(owner.attacked.is_empty());
    assert!(queue.assistance_captures().is_empty());
    queue.validate().unwrap();
}
#[test]
fn assistance_receive_callback_runs_before_live_critter_check() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    let mut owner = Authored {
        mutate_on_receive: true,
        ..Default::default()
    };
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(owner.order, ["receive"]);
    assert!(owner.queries.is_empty());
}
#[test]
fn assistance_positive_delivery_marks_latch_before_attack() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    let mut owner = Authored::default();
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(owner.order, ["receive", "attack"]);
    assert_eq!(owner.attacked, [h[2]]);
    assert!(units.get(&objects, h[2]).unwrap().already_called_assistance);
}
#[test]
fn assistance_missing_enemy_and_despawned_receiver_skip() {
    for missing_enemy in [true, false] {
        let (mut objects, mut units, h) = fixture();
        let mut queue = SourceUnitEvents::default();
        queue.add_assistance(1, capture(&objects, h)).unwrap();
        objects
            .remove_from_world(if missing_enemy { h[1] } else { h[2] })
            .unwrap();
        let mut owner = Authored::default();
        update(&mut queue, 1, &objects, &mut units, &mut owner);
        assert!(owner.received.is_empty());
        assert!(queue.assistance_captures().is_empty());
    }
}
#[test]
fn assistance_guid_lookup_rebinds_to_current_construction() {
    let (mut objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    let guid = objects.get(h[2]).unwrap().guid.unwrap();
    objects.remove_from_world(h[2]).unwrap();
    let replacement = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects.assign_guid(replacement, 3, guid).unwrap();
    objects.get_mut(replacement).unwrap().map = objects.get(h[0]).unwrap().map;
    objects.add_to_world(replacement).unwrap();
    let fields = units.get(&objects, h[2]).unwrap().clone();
    units.register(&objects, replacement, fields).unwrap();
    let mut owner = Authored::default();
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(owner.attacked, [replacement]);
}
#[test]
fn assistance_equal_deadline_and_receiver_order_are_not_guid_sorted() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    let mut c = capture(&objects, h);
    c.receivers = vec![
        objects.get(h[3]).unwrap().guid.unwrap(),
        objects.get(h[2]).unwrap().guid.unwrap(),
    ];
    queue.add_assistance(4, c.clone()).unwrap();
    c.kind = AssistanceEvent::Around {
        event: 5.into(),
        misc: 77,
    };
    queue.add_assistance(4, c).unwrap();
    let mut owner = Authored::default();
    update(&mut queue, 3, &objects, &mut units, &mut owner);
    assert!(owner.received.is_empty());
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(
        owner.received,
        [(h[3], 13, 9), (h[2], 13, 9), (h[3], 5, 77), (h[2], 5, 77)]
    );
}
#[test]
fn assistance_cancel_kill_and_destruction_release_captured_lists() {
    for mode in 0..4 {
        let (objects, mut units, h) = fixture();
        let mut queue = SourceUnitEvents::default();
        let id = queue.add_assistance(4, capture(&objects, h)).unwrap();
        match mode {
            0 => queue.cancel(id).unwrap(),
            1 => {
                assert!(queue.kill_event(id).unwrap());
            }
            2 => queue.kill_all(false).unwrap(),
            _ => queue.kill_all(true).unwrap(),
        }
        let mut owner = Authored::default();
        update(&mut queue, 4, &objects, &mut units, &mut owner);
        assert!(owner.received.is_empty());
        assert!(queue.assistance_captures().is_empty());
        queue.validate().unwrap();
    }
}
#[test]
fn assistance_nonempty_grid_help_does_not_send_ai_event() {
    // GridNotifiers.cpp:245-262 calls OnCallForHelp directly, not ReceiveAIEvent.
    let (objects, mut units, h) = fixture();
    units.get_mut(&objects, h[0]).unwrap().call_for_help = 12;
    let mut owner = Authored {
        members: vec![h[0], h[3], h[2]],
        ..Default::default()
    };
    SourceAssistance::call_for_help(h[0], 0, &mut units, &objects, &mut owner).unwrap();
    assert_eq!(owner.visits, [(12f32.to_bits(), AssistanceVisit::GridHelp)]);
    assert_eq!(owner.attacked, [h[3], h[2]]);
    assert!(owner.received.is_empty());
    assert!(!units.get(&objects, h[2]).unwrap().already_called_assistance);
}
#[test]
fn assistance_faction_flee_takes_precedence_over_response() {
    let (objects, mut units, h) = fixture();
    for (flags, expected) in [
        (Some(0x401), vec!["panic", "flee"]),
        (Some(0), vec![]),
        (Some(1), vec!["attack"]),
        (None, vec!["attack"]),
    ] {
        let mut owner = Authored {
            flags,
            ..Default::default()
        };
        SourceAssistance::on_help(h[2], h[1], &mut units, &objects, &mut owner).unwrap();
        assert_eq!(owner.order, expected);
    }
}
#[test]
fn assistance_first_pull_still_searches_but_does_not_deliver() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    let mut owner = Authored {
        members: vec![h[2]],
        ..Default::default()
    };
    SourceAssistance::trigger_aggro(
        h[0],
        Some(h[1]),
        None,
        5f32.to_bits(),
        0,
        &mut units,
        &objects,
        &mut owner,
        &mut queue,
    )
    .unwrap();
    assert_eq!(owner.visits.len(), 1);
    update(&mut queue, 0, &objects, &mut units, &mut owner);
    assert!(owner.received.is_empty());
}
#[test]
fn assistance_live_out_of_combat_suppresses_pull_but_not_around_event() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    let mut c = capture(&objects, h);
    c.kind = AssistanceEvent::Aggro {
        group: None,
        call_assistance: true,
    };
    queue.add_assistance(1, c).unwrap();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    units.get_mut(&objects, h[0]).unwrap().flags &= !UNIT_FLAG_COMBAT;
    let mut owner = Authored::default();
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(owner.received, [(h[2], 13, 9)]);
}
#[test]
fn assistance_recursive_linking_queues_same_update_after_original_due_events() {
    let (objects, mut units, h) = fixture();
    units.get_mut(&objects, h[0]).unwrap().linking_event_trigger = Some(true);
    let mut queue = SourceUnitEvents::default();
    let mut c = capture(&objects, h);
    let child = c.clone();
    c.kind = AssistanceEvent::Aggro {
        group: None,
        call_assistance: true,
    };
    queue.add_assistance(1, c).unwrap();
    let mut owner = Authored {
        recursive: Some(child),
        ..Default::default()
    };
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(
        owner.order,
        ["linking", "receive", "attack", "receive", "attack"]
    );
    queue.validate().unwrap();
}
#[test]
fn assistance_group_latch_and_combat_stop_preserve_queue() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    SourceAssistance::group_callback(
        GroupCallback::SetNoCallAssistance { object: h[0] },
        &mut units,
        &objects,
    )
    .unwrap();
    assert!(units.get(&objects, h[0]).unwrap().already_called_assistance);
    SourceAssistance::combat_stopped(h[0], &mut units, &objects).unwrap();
    assert!(!units.get(&objects, h[0]).unwrap().already_called_assistance);
    assert_eq!(queue.assistance_captures().len(), 1);
}
#[test]
fn assistance_reenable_help_precedes_aggro_event_even_at_zero_delay() {
    let (objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    let mut owner = Authored::default();
    SourceAssistance::combat_entry_after_callbacks(
        h[0],
        Some(h[1]),
        None,
        0,
        0,
        &mut units,
        &objects,
        &mut owner,
        &mut queue,
    )
    .unwrap();
    assert!(!units.get(&objects, h[0]).unwrap().can_check_help);
    assert_eq!(
        queue.assistance_captures().values().next().unwrap().kind,
        AssistanceEvent::ReenableHelp
    );
    update(&mut queue, 0, &objects, &mut units, &mut owner);
    assert!(units.get(&objects, h[0]).unwrap().can_check_help);
}
#[test]
fn assistance_check_for_help_routes_through_unit_ai_adapter() {
    let (objects, mut units, h) = fixture();
    let mut owner = Authored::default();
    UnitAiExecutionOwner::call(
        &mut SourceAssistanceDependencies { owner: &mut owner },
        h[2],
        UnitAiCall::CheckForHelp {
            target: h[0],
            radius_bits: 5f32.to_bits(),
        },
        &mut units,
        &objects,
    )
    .unwrap();
    assert_eq!(owner.attacked, [h[2]]);
    units.get_mut(&objects, h[0]).unwrap().can_check_help = false;
    SourceAssistance::check_for_help(h[3], h[0], 0, &mut units, &objects, &mut owner).unwrap();
    assert_eq!(owner.attacked, [h[2]]);
}
#[test]
fn assistance_helper_uses_source_pointer_order_or_refuses() {
    let (mut objects, mut units, h) = fixture();
    units.get_mut(&objects, h[0]).unwrap().victim = None;
    units.get_mut(&objects, h[0]).unwrap().attackers = vec![h[2], h[3]];
    assert_eq!(
        SourceAssistance::attacker_for_helper(h[0], &units, &objects)
            .unwrap_err()
            .id,
        "execution.assistance.attacker_pointer_order_unbound"
    );
    objects
        .bind_source_allocation(h[2], 0x2000, 0x100, 0)
        .unwrap();
    objects
        .bind_source_allocation(h[3], 0x1000, 0x100, 0)
        .unwrap();
    assert_eq!(
        SourceAssistance::attacker_for_helper(h[0], &units, &objects).unwrap(),
        Some(h[3])
    );
    units.get_mut(&objects, h[0]).unwrap().victim = Some(h[1]);
    assert_eq!(
        SourceAssistance::attacker_for_helper(h[0], &units, &objects).unwrap(),
        Some(h[1])
    );
}
#[test]
fn assistance_unavailable_visitors_distance_and_los_are_named_f6() {
    struct Unbound;
    impl AssistanceExecutionOwner for Unbound {}
    let (objects, mut units, h) = fixture();
    assert_eq!(
        SourceAssistance::call_for_help(h[0], 0, &mut units, &objects, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.assistance.f6_grid_visitor_owner"
    );
    for (query, id) in [
        (
            AssistanceQuery::Distance(0),
            "execution.assistance.f6_distance_owner",
        ),
        (AssistanceQuery::Los, "execution.assistance.f6_los_owner"),
    ] {
        assert_eq!(Unbound.query(h[0], h[1], query).unwrap_err().id, id);
    }
}
#[test]
fn assistance_nonassistable_latches_without_visiting() {
    let (objects, mut units, h) = fixture();
    units.get_mut(&objects, h[0]).unwrap().can_call_assistance = false;
    let mut owner = Authored::default();
    let result =
        SourceAssistance::mark_on_pull(h[0], h[1], 0, &mut units, &objects, &mut owner).unwrap();
    assert_eq!(result, (false, vec![]));
    assert!(owner.visits.is_empty());
    assert!(units.get(&objects, h[0]).unwrap().already_called_assistance);
}
#[test]
fn assistance_event_numbers_follow_recorded_raw_u32_deviation() {
    for raw in [0, 1023, 1024, 7166, u32::MAX] {
        assert_eq!(SourceAiEventType::from(raw).raw(), raw);
    }
    for (raw, delay) in [(7166, 0), (7166, 1), (u32::MAX, 0), (u32::MAX, 1)] {
        let (objects, mut units, h) = fixture();
        let mut owner = Authored {
            members: vec![h[2]],
            ..Default::default()
        };
        let mut queue = SourceUnitEvents::default();
        SourceAssistance::send_around(
            h[0],
            Some(h[1]),
            raw.into(),
            17,
            delay,
            5f32.to_bits(),
            &mut units,
            &objects,
            &mut owner,
            &mut queue,
        )
        .unwrap();
        update(&mut queue, delay, &objects, &mut units, &mut owner);
        assert_eq!(owner.received, [(h[2], raw, 17)]);
        assert!(owner.attacked.is_empty());
        assert!(queue.assistance_captures().is_empty());
    }
}

#[path = "assistance_census_tests.rs"]
mod census;

#[test]
fn assistance_custom_events_include_live_self_but_not_dead_receivers() {
    // GridNotifiers.h:1094-1096 intentionally has no self exclusion.
    let (mut objects, mut units, h) = fixture();
    objects.get_mut(h[2]).unwrap().alive = false;
    let mut owner = Authored {
        members: vec![h[0], h[2], h[3]],
        ..Default::default()
    };
    SourceAssistance::send_immediate(
        h[0],
        None,
        5.into(),
        123,
        5f32.to_bits(),
        &mut units,
        &objects,
        &mut owner,
    )
    .unwrap();
    assert_eq!(owner.received, [(h[0], 5, 123), (h[3], 5, 123)]);
    assert!(owner.attacked.is_empty());
}

#[test]
fn assistance_destruction_aborts_queue_before_releasing_sender_generation() {
    let (mut objects, mut units, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    queue.kill_all(true).unwrap();
    objects.remove_from_world(h[0]).unwrap();
    let mut cells = SourceCells::new(objects.get(h[0]).unwrap().map.unwrap()).unwrap();
    objects.destroy(h[0], &mut cells).unwrap();
    let mut owner = Authored::default();
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert!(owner.received.is_empty());
    assert!(queue.assistance_captures().is_empty());
    queue.validate().unwrap();
}

#[test]
fn assistance_live_charmer_suppresses_pull_delivery_not_captured_group_callback() {
    let (objects, mut units, h) = fixture();
    let mut c = capture(&objects, h);
    c.kind = AssistanceEvent::Aggro {
        group: Some(GroupIdentity {
            map: objects.get(h[0]).unwrap().map.unwrap(),
            handle: SpawnGroupHandle {
                id: 17,
                generation: 1,
            },
        }),
        call_assistance: true,
    };
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, c).unwrap();
    units.get_mut(&objects, h[0]).unwrap().charmer = Some(h[1]);
    let mut owner = Authored::default();
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(owner.order, ["group"]);
    assert!(owner.received.is_empty());
}

#[test]
fn assistance_selected_unit_ai_base_does_not_run_creature_override() {
    let (objects, mut units, h) = fixture();
    let mut owner = Authored {
        unit_ai_base: true,
        ..Default::default()
    };
    SourceAssistance::handle_call(h[2], h[0], Some(h[1]), &mut units, &objects, &mut owner)
        .unwrap();
    SourceAssistance::on_help(h[2], h[1], &mut units, &objects, &mut owner).unwrap();
    assert!(owner.queries.is_empty());
    assert!(owner.order.is_empty());
    assert!(!units.get(&objects, h[2]).unwrap().already_called_assistance);
}

impl CreatureRandom for Authored {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected signed draw")
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        assert_eq!((min, max), (0, u32::MAX));
        self.draws += 1;
        Ok(0)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.creature_urand(0, u32::MAX)
    }
}
impl EventAiExecutionOwner for Authored {
    fn current_ranged_mode(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn can_execute_combat_action(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(true)
    }
}

#[test]
fn assistance_delayed_delivery_runs_real_receive_eventai_dispatch() {
    // CreatureEventAI.cpp:1563-1574 dispatches event30 before HandleAssistanceCall.
    let (objects, mut units, h) = fixture();
    let mut state = super::event_ai_dispatch_tests::state();
    let mut registry = AiRegistryBuilder::builtin();
    registry.load_scripts(Vec::new()).unwrap();
    state.selected = registry
        .freeze()
        .select(
            &objects,
            h[2],
            super::motion_master_tests::template().inputs(CreatureAiControl::default(), false),
        )
        .unwrap();
    state.holders[0].event.kind = 30;
    state.holders[0].event.chance = 100;
    state.holders[0].event.params = [AI_EVENT_CALL_ASSISTANCE, 0, 0, 0, 0, 0];
    state.holders[0].event.actions[0] = EventAiAction {
        kind: 22,
        params: Some([7, 0, 0]),
    };
    let mut owner = Authored {
        event_ai: Some(state),
        ..Default::default()
    };
    let mut queue = SourceUnitEvents::default();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    update(&mut queue, 1, &objects, &mut units, &mut owner);
    assert_eq!(owner.event_ai.as_ref().unwrap().phase, 7);
    assert_eq!(owner.draws, 2);
    assert_eq!(owner.order, ["receive", "attack"]);
}

#[test]
fn assistance_and_forced_despawn_share_original_queue_order() {
    #[derive(Default)]
    struct Mixed(Vec<&'static str>);
    impl UnitEventOwner for Mixed {
        fn forced_despawn(&mut self, _: bool) -> Result<()> {
            self.0.push("despawn");
            Ok(())
        }
        fn assistance(
            &mut self,
            _: &AssistanceCapture,
            _: &mut impl AssistanceQueue,
        ) -> Result<()> {
            self.0.push("assistance");
            Ok(())
        }
    }
    let (objects, _, h) = fixture();
    let mut queue = SourceUnitEvents::default();
    queue.schedule_forced_despawn(1, false).unwrap();
    queue.add_assistance(1, capture(&objects, h)).unwrap();
    queue.schedule_forced_despawn(1, true).unwrap();
    let mut owner = Mixed::default();
    queue.update(1, &mut owner).unwrap();
    assert_eq!(owner.0, ["despawn", "assistance", "despawn"]);
    assert!(queue.assistance_captures().is_empty());
}

#[test]
fn assistance_aggro_source_creature_predicate_includes_pet_subclasses() {
    // Unit.cpp:561; Object.h:590 uses TYPEID_UNIT, not GUID-high equality.
    let (mut objects, mut units, h) = fixture();
    let pet = objects.construct(1, SourceObjectKind::Pet).unwrap();
    objects
        .assign_guid(pet, 0, SourceGuid::with_entry(HighGuid::Pet, 901, 12))
        .unwrap();
    objects.get_mut(pet).unwrap().map = objects.get(h[0]).unwrap().map;
    objects.add_to_world(pet).unwrap();
    let mut fields = units.get(&objects, h[0]).unwrap().clone();
    fields.can_call_assistance = false;
    units.register(&objects, pet, fields).unwrap();
    let mut queue = SourceUnitEvents::default();
    let mut owner = Authored::default();
    let event = SourceAssistance::trigger_aggro(
        pet,
        Some(h[1]),
        None,
        0,
        0,
        &mut units,
        &objects,
        &mut owner,
        &mut queue,
    )
    .unwrap();
    assert!(event.is_some());
    assert!(units.get(&objects, pet).unwrap().already_called_assistance);
    assert_eq!(queue.assistance_captures().len(), 1);
    update(&mut queue, 0, &objects, &mut units, &mut owner);
    assert!(queue.assistance_captures().is_empty());
}

#[test]
fn assistance_null_receiver_ai_refuses_before_immediate_or_delayed_callback() {
    // UnitAI.cpp:593,646 and Creature.cpp:2210 dereference receiver->AI().
    for delayed in [false, true] {
        let (objects, mut units, h) = fixture();
        units.get_mut(&objects, h[2]).unwrap().has_ai = false;
        let mut owner = Authored {
            members: vec![h[2]],
            ..Default::default()
        };
        let error = if delayed {
            let mut queue = SourceUnitEvents::default();
            queue.add_assistance(1, capture(&objects, h)).unwrap();
            queue
                .update(
                    1,
                    &mut SourceAssistanceEvents {
                        objects: &objects,
                        units: &mut units,
                        owner: &mut owner,
                        other_events: &mut Other,
                    },
                )
                .unwrap_err()
        } else {
            SourceAssistance::send_immediate(
                h[0],
                Some(h[1]),
                SourceAiEventType::CALL_ASSISTANCE,
                0,
                5f32.to_bits(),
                &mut units,
                &objects,
                &mut owner,
            )
            .unwrap_err()
        };
        assert_eq!(error.id, "execution.assistance.receiver_ai_null_undefined");
        assert!(owner.received.is_empty());
    }
}
