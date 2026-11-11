use super::*;

fn unit() -> ObjectHandle {
    ObjectHandle {
        thread: 2,
        construction: 1,
    }
}
#[derive(Default)]
struct Events {
    calls: Vec<bool>,
    refuse: bool,
}
impl UnitEventOwner for Events {
    fn forced_despawn(&mut self, only_alive: bool) -> Result<()> {
        self.calls.push(only_alive);
        if self.refuse {
            Err(refuse("authored.lifecycle_owner"))
        } else {
            Ok(())
        }
    }
}
#[test]
fn unit_events_delay_is_inclusive_and_relative_to_unit_clock() {
    // EventProcessor.cpp:35-47,124-126; Creature.cpp:1964-1969.
    let mut queue = SourceUnitEvents::default();
    let mut host = Events::default();
    queue.update(7, &mut host).unwrap();
    queue.schedule_forced_despawn(25, false).unwrap();
    queue.update(24, &mut host).unwrap();
    assert!(host.calls.is_empty());
    queue.update(1, &mut host).unwrap();
    assert_eq!(host.calls, [false]);
    assert!(queue.inventory().queued.is_empty());
    assert_eq!(queue.clock_millis(), 32);
}
#[test]
fn unit_events_equal_deadlines_preserve_insertion_order() {
    let mut queue = SourceUnitEvents::default();
    queue.schedule_forced_despawn(1, true).unwrap();
    queue.schedule_forced_despawn(1, false).unwrap();
    let mut host = Events::default();
    queue.update(100, &mut host).unwrap();
    assert_eq!(host.calls, [true, false]);
}
#[test]
fn unit_events_zero_delay_cannot_be_queued() {
    let mut queue = SourceUnitEvents::default();
    assert_eq!(
        queue.schedule_forced_despawn(0, true).unwrap_err().id,
        "execution.unit_events.zero_delay_is_synchronous"
    );
    assert!(queue.inventory().queued.is_empty());
}
#[test]
fn unit_events_cancel_waits_for_due_abort_without_execute() {
    // EventProcessor.cpp:45-56; BasicEvent::Abort is empty, header:48.
    let mut queue = SourceUnitEvents::default();
    let id = queue.schedule_forced_despawn(10, false).unwrap();
    queue.cancel(id).unwrap();
    let mut host = Events::default();
    queue.update(9, &mut host).unwrap();
    assert!(queue.inventory().queued[0].to_abort);
    queue.update(1, &mut host).unwrap();
    assert!(host.calls.is_empty());
    assert!(queue.inventory().queued.is_empty());
}
#[test]
fn unit_events_kill_event_and_both_kill_all_modes_delete_without_despawn() {
    // ForcedDespawnDelayEvent inherits true IsDeletable, EventProcessor.h:46.
    for force in [false, true] {
        let mut queue = SourceUnitEvents::default();
        let id = queue.schedule_forced_despawn(10, false).unwrap();
        assert!(queue.kill_event(id).unwrap());
        assert!(!queue.kill_event(id).unwrap());
        queue.schedule_forced_despawn(10, true).unwrap();
        queue.kill_all(force).unwrap();
        assert!(queue.inventory().queued.is_empty());
        assert!(queue.inventory().aborting);
        // Source AddEvent does not consult m_aborting, EventProcessor.cpp:101-107.
        queue.schedule_forced_despawn(1, false).unwrap();
        let mut host = Events::default();
        queue.update(1, &mut host).unwrap();
        assert_eq!(host.calls, [false]);
        queue.validate().unwrap();
    }
}
#[test]
fn unit_events_callback_refusal_stops_retrying_partial_effects() {
    let mut queue = SourceUnitEvents::default();
    queue.schedule_forced_despawn(1, false).unwrap();
    let mut host = Events {
        refuse: true,
        ..Events::default()
    };
    assert_eq!(
        queue.update(1, &mut host).unwrap_err().id,
        "authored.lifecycle_owner"
    );
    assert_eq!(
        queue.update(1, &mut host).unwrap_err().id,
        "execution.unit_events.stopped"
    );
    assert_eq!(host.calls, [false]);
}

#[derive(Default)]
struct Heartbeat {
    log: Vec<String>,
    auras: Vec<[Option<u64>; 3]>,
    proc_count: u32,
    ai: bool,
    mutate: bool,
    refuse_aura: bool,
}
impl UnitHeartbeatOwner for Heartbeat {
    fn first_holder(&mut self, _: ObjectHandle) -> Result<Option<HeartbeatHolder>> {
        Ok((!self.auras.is_empty()).then_some(HeartbeatHolder {
            spell: 10,
            generation: 1,
        }))
    }
    fn next_holder(
        &mut self,
        _: ObjectHandle,
        holder: HeartbeatHolder,
    ) -> Result<Option<HeartbeatHolder>> {
        let next = holder.generation + 1;
        Ok(
            (next <= self.auras.len() as u64).then_some(HeartbeatHolder {
                spell: 10,
                generation: next,
            }),
        )
    }
    fn aura_at(
        &mut self,
        _: ObjectHandle,
        holder: HeartbeatHolder,
        effect: u8,
    ) -> Result<Option<u64>> {
        Ok(self.auras[holder.generation as usize - 1][effect as usize])
    }
    fn aura_heartbeat(&mut self, _: ObjectHandle, aura: u64) -> Result<()> {
        self.log.push(format!("aura{aura}"));
        if self.refuse_aura {
            return Err(refuse("execution.unit_heartbeat.f5_12_aura_script_owner"));
        }
        if self.mutate && aura == 1 {
            self.auras[0][2] = None;
            self.proc_count = 1;
        }
        Ok(())
    }
    fn heartbeat_proc_count(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(self.proc_count)
    }
    fn heartbeat_proc(&mut self, arguments: HeartbeatProcArguments) -> Result<()> {
        assert_eq!(
            arguments,
            HeartbeatProcArguments {
                attacker: unit(),
                victim: None,
                attacker_flags: 1,
                victim_flags: 0,
                proc_extra: 0,
                amount: 0,
                absorb: 0,
                attack_type: 0,
                spell_info: None,
                spell: None,
                health_gain: 0,
                is_heal: false,
            }
        );
        self.log
            .push("proc:self,null,HEARTBEAT,NONE,NONE,0,0".into());
        if self.mutate {
            self.ai = true;
        }
        Ok(())
    }
    fn has_ai(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.ai)
    }
    fn ai_heartbeat(&mut self, _: ObjectHandle) -> Result<()> {
        self.log.push("ai".into());
        Ok(())
    }
}
#[test]
fn unit_heartbeat_orders_live_holder_effects_then_proc_then_ai() {
    // Unit.cpp:544-556, including duplicate spell IDs and null aura slots.
    let mut host = Heartbeat {
        auras: vec![[Some(1), None, Some(3)], [Some(4), Some(5), None]],
        proc_count: 1,
        ai: true,
        ..Heartbeat::default()
    };
    source_unit_heartbeat(unit(), &mut host).unwrap();
    assert_eq!(
        host.log,
        [
            "aura1",
            "aura3",
            "aura4",
            "aura5",
            "proc:self,null,HEARTBEAT,NONE,NONE,0,0",
            "ai"
        ]
    );
}
#[test]
fn unit_heartbeat_observes_mutation_before_later_effect_proc_and_ai_checks() {
    let mut host = Heartbeat {
        auras: vec![[Some(1), None, Some(3)]],
        mutate: true,
        ..Heartbeat::default()
    };
    source_unit_heartbeat(unit(), &mut host).unwrap();
    assert_eq!(
        host.log,
        ["aura1", "proc:self,null,HEARTBEAT,NONE,NONE,0,0", "ai"]
    );
}
#[test]
fn unit_heartbeat_zero_proc_and_no_ai_are_real_empty_branches() {
    let mut host = Heartbeat::default();
    source_unit_heartbeat(unit(), &mut host).unwrap();
    assert!(host.log.is_empty());
}
#[test]
fn unit_heartbeat_missing_aura_owner_refuses_before_proc_or_ai() {
    let mut host = Heartbeat {
        auras: vec![[Some(1), None, None]],
        proc_count: 1,
        ai: true,
        refuse_aura: true,
        ..Heartbeat::default()
    };
    assert_eq!(
        source_unit_heartbeat(unit(), &mut host).unwrap_err().id,
        "execution.unit_heartbeat.f5_12_aura_script_owner"
    );
    assert_eq!(host.log, ["aura1"]);
    struct Missing;
    impl UnitHeartbeatOwner for Missing {}
    assert_eq!(
        source_unit_heartbeat(unit(), &mut Missing).unwrap_err().id,
        "execution.unit_heartbeat.f5_12_aura_holder_owner"
    );
}
#[test]
fn unit_world_object_timer_starts_due_and_large_diff_does_not_catch_up() {
    // ShortTimeTracker saturates at zero, Timer.h:148-158, not signed debt.
    let mut state = SourceUnitUpdate::default();
    let mut host = Heartbeat {
        ai: true,
        ..Heartbeat::default()
    };
    state.world_object_update(unit(), 0, &mut host).unwrap();
    assert_eq!(state.heartbeat_expiry(), 5000);
    state.world_object_update(unit(), 4999, &mut host).unwrap();
    assert_eq!(host.log, ["ai"]);
    state.world_object_update(unit(), 1, &mut host).unwrap();
    state
        .world_object_update(unit(), u32::MAX, &mut host)
        .unwrap();
    assert_eq!(host.log, ["ai", "ai", "ai"]);
    assert_eq!(state.heartbeat_expiry(), 5000);
}
#[test]
fn unit_world_object_refusal_stops_repeated_heartbeat() {
    struct Missing;
    impl UnitHeartbeatOwner for Missing {}
    let mut state = SourceUnitUpdate::default();
    assert_eq!(
        state
            .world_object_update(unit(), 1, &mut Missing)
            .unwrap_err()
            .id,
        "execution.unit_heartbeat.f5_12_aura_holder_owner"
    );
    assert_eq!(
        state
            .world_object_update(unit(), 1, &mut Missing)
            .unwrap_err()
            .id,
        "execution.unit_update.stopped"
    );
}

#[test]
fn unit_heartbeat_reaches_proc_owner_only_when_live_counter_is_nonzero() {
    struct MissingProc(u32);
    impl UnitHeartbeatOwner for MissingProc {
        fn first_holder(&mut self, _: ObjectHandle) -> Result<Option<HeartbeatHolder>> {
            Ok(None)
        }
        fn heartbeat_proc_count(&mut self, _: ObjectHandle) -> Result<u32> {
            Ok(self.0)
        }
        fn has_ai(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(false)
        }
    }
    source_unit_heartbeat(unit(), &mut MissingProc(0)).unwrap();
    assert_eq!(
        source_unit_heartbeat(unit(), &mut MissingProc(1))
            .unwrap_err()
            .id,
        "execution.unit_heartbeat.f5_14_proc_owner"
    );
}
