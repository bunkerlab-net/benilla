use super::*;
use std::cell::RefCell;
use std::rc::Rc;

pub(super) fn template() -> CreatureAiTemplateOperands {
    CreatureAiTemplateOperands {
        ordinal: 1,
        entry: 901,
        faction: 14,
        extra_flags: 0,
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [0x3f800000; 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: "EventAI".into(),
        script_name: String::new(),
    }
}
fn setup() -> (SourceObjects, SourceUnits, SourceMotionMaster) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    objects
        .set_respawn_definition(unit, SourcePose([1f32.to_bits(); 4]), true)
        .unwrap();
    let mut units = SourceUnits::default();
    units
        .register(&objects, unit, SourceUnitFields::creature(&template()))
        .unwrap();
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 0,
            default_path: 19,
        },
    );
    master
        .initialize(
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    (objects, units, master)
}
struct Unbound;
impl MotionExecutionOwner for Unbound {}
#[derive(Clone, Copy, Default)]
enum DuringUpdate {
    #[default]
    Keep,
    Finish,
    Clear(bool, bool),
    Expire(bool),
}
struct Probe {
    name: &'static str,
    kind: u32,
    log: Rc<RefCell<Vec<String>>>,
    during: DuringUpdate,
    removed: bool,
    push_finalize: bool,
    position: Option<[f32; 4]>,
}
impl Probe {
    fn new(name: &'static str, kind: u32, log: &Rc<RefCell<Vec<String>>>) -> Self {
        Self {
            name,
            kind,
            log: log.clone(),
            during: DuringUpdate::Keep,
            removed: false,
            push_finalize: false,
            position: None,
        }
    }
    fn record(&self, action: &str) {
        self.log
            .borrow_mut()
            .push(format!("{}.{action}", self.name));
    }
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.record("drop");
    }
}
impl SourceMovementGenerator for Probe {
    fn kind(&self) -> u32 {
        self.kind
    }
    fn initialize(
        &self,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.record("init");
        Ok(())
    }
    fn finalize(
        &self,
        master: &mut SourceMotionMaster,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.record("finalize");
        if self.push_finalize {
            master.mutate(Probe::new("new", 11, &self.log), context, owner)?;
        }
        Ok(())
    }
    fn interrupt(
        &self,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.record("interrupt");
        Ok(())
    }
    fn reset(
        &self,
        _: &mut SourceMotionMaster,
        _: &mut MotionContext<'_>,
        _: &mut dyn MotionExecutionOwner,
    ) -> Result<()> {
        self.record("reset");
        Ok(())
    }
    fn update(
        &self,
        master: &mut SourceMotionMaster,
        _: u32,
        context: &mut MotionContext<'_>,
        owner: &mut dyn MotionExecutionOwner,
    ) -> Result<bool> {
        self.record("update");
        match self.during {
            DuringUpdate::Keep => {}
            DuringUpdate::Finish => return Ok(false),
            DuringUpdate::Clear(reset, all) => master.clear(reset, all, context, owner)?,
            DuringUpdate::Expire(reset) => master.expire(reset, context, owner)?,
        }
        self.record("return");
        Ok(true)
    }
    fn removed_on_expire(&self) -> bool {
        self.removed
    }
    fn reset_position(&self) -> Option<[f32; 4]> {
        self.position
    }
}
#[test]
fn stack_push_interrupt_and_home_distract_replacement() {
    for replace_kind in [5, 12] {
        let (objects, mut units, mut master) = setup();
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let log = Rc::default();
        master
            .mutate(Probe::new("a", 11, &log), &mut c, &mut Unbound)
            .unwrap();
        master
            .mutate(Probe::new("b", replace_kind, &log), &mut c, &mut Unbound)
            .unwrap();
        master
            .mutate(Probe::new("c", 11, &log), &mut c, &mut Unbound)
            .unwrap();
        assert_eq!(
            &*log.borrow(),
            &[
                "a.init",
                "a.interrupt",
                "b.init",
                "b.finalize",
                "b.drop",
                "a.interrupt",
                "c.init"
            ]
        );
        assert_eq!(master.len(), 3);
    }
}
#[test]
fn clear_reset_and_no_reset_finalize_top_down() {
    for reset in [false, true] {
        let (objects, mut units, mut master) = setup();
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let log = Rc::default();
        master.clear(false, true, &mut c, &mut Unbound).unwrap();
        master
            .mutate(Probe::new("base", 11, &log), &mut c, &mut Unbound)
            .unwrap();
        master
            .mutate(Probe::new("a", 11, &log), &mut c, &mut Unbound)
            .unwrap();
        master
            .mutate(Probe::new("b", 11, &log), &mut c, &mut Unbound)
            .unwrap();
        log.borrow_mut().clear();
        master.clear(reset, false, &mut c, &mut Unbound).unwrap();
        let mut expected = vec!["b.finalize", "b.drop", "a.finalize", "a.drop"];
        if reset {
            expected.push("base.reset");
        }
        assert_eq!(&*log.borrow(), &expected);
        assert_eq!(master.len(), 1);
        master.clear(reset, true, &mut c, &mut Unbound).unwrap();
        assert!(master.is_empty());
    }
}
#[test]
fn delayed_clear_defers_drop_until_update_returns_and_reinitializes_all() {
    for all in [false, true] {
        let (objects, mut units, mut master) = setup();
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let log = Rc::default();
        let mut p = Probe::new("a", 11, &log);
        p.during = DuringUpdate::Clear(true, all);
        master.mutate(p, &mut c, &mut Unbound).unwrap();
        log.borrow_mut().clear();
        master.update(10, &mut c, &mut Unbound).unwrap();
        assert_eq!(
            &*log.borrow(),
            &["a.update", "a.finalize", "a.return", "a.drop"]
        );
        assert_eq!(master.len(), 1);
        assert_eq!(master.current_type(), 0);
    }
}
#[test]
fn expire_reset_avoids_resetting_generator_pushed_by_finalize() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let log = Rc::default();
    let mut p = Probe::new("old", 11, &log);
    p.push_finalize = true;
    master.mutate(p, &mut c, &mut Unbound).unwrap();
    log.borrow_mut().clear();
    master.expire(true, &mut c, &mut Unbound).unwrap();
    assert_eq!(&*log.borrow(), &["old.finalize", "new.init", "old.drop"]);
}
#[test]
fn delayed_expire_resets_new_top_after_destroying_old() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let log = Rc::default();
    let mut p = Probe::new("old", 11, &log);
    p.push_finalize = true;
    p.during = DuringUpdate::Expire(true);
    master.mutate(p, &mut c, &mut Unbound).unwrap();
    log.borrow_mut().clear();
    master.update(1, &mut c, &mut Unbound).unwrap();
    assert_eq!(
        &*log.borrow(),
        &[
            "old.update",
            "old.finalize",
            "new.init",
            "old.return",
            "old.drop",
            "new.reset"
        ]
    );
}
#[test]
fn direct_effect_expiry_preserves_targeted_but_delayed_effect_drops_it() {
    for delayed in [false, true] {
        let (objects, mut units, mut master) = setup();
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        let log = Rc::default();
        let mut target = Probe::new("target", 8, &log);
        target.removed = true;
        master.mutate(target, &mut c, &mut Unbound).unwrap();
        let mut effect = Probe::new("effect", 15, &log);
        if delayed {
            effect.during = DuringUpdate::Expire(false);
        }
        master.mutate(effect, &mut c, &mut Unbound).unwrap();
        log.borrow_mut().clear();
        if delayed {
            master.update(1, &mut c, &mut Unbound).unwrap();
        } else {
            master.expire(false, &mut c, &mut Unbound).unwrap();
        }
        if delayed {
            assert_eq!(
                &*log.borrow(),
                &[
                    "effect.update",
                    "target.finalize",
                    "effect.finalize",
                    "effect.return",
                    "target.drop",
                    "effect.drop"
                ]
            );
            assert_eq!(master.len(), 1);
        } else {
            assert_eq!(&*log.borrow(), &["effect.finalize", "effect.drop"]);
            assert_eq!(master.len(), 2);
        }
    }
}
#[test]
fn update_false_uses_direct_expiry_and_owner_destruction_never_finalizes() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let log = Rc::default();
    let mut p = Probe::new("a", 11, &log);
    p.during = DuringUpdate::Finish;
    master.mutate(p, &mut c, &mut Unbound).unwrap();
    log.borrow_mut().clear();
    master.update(1, &mut c, &mut Unbound).unwrap();
    assert_eq!(&*log.borrow(), &["a.update", "a.finalize", "a.drop"]);
    master
        .mutate(Probe::new("b", 11, &log), &mut c, &mut Unbound)
        .unwrap();
    log.borrow_mut().clear();
    drop(master);
    assert_eq!(&*log.borrow(), &["b.drop"]);
}
#[test]
fn idle_push_does_not_interrupt_or_initialize_and_is_idempotent() {
    let (objects, mut units, mut master) = setup();
    let log = Rc::default();
    master
        .mutate(
            Probe::new("a", 11, &log),
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    log.borrow_mut().clear();
    master.move_idle();
    master.move_idle();
    assert!(log.borrow().is_empty());
    assert_eq!(master.len(), 3);
}
#[test]
fn initialize_defaults_registered_and_null_factories() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    for (kind, refusal) in [
        (1, "execution.motion.respawn_radius_owner"),
        (3, "execution.waypoint.loaded_waypoint_manager_owner"),
    ] {
        master.default.movement_type = kind;
        assert_eq!(
            master.initialize(&mut c, &mut Unbound).unwrap_err().id,
            refusal
        );
        assert!(master.is_empty());
        assert_eq!(master.path_id().unwrap(), 19);
    }
    for kind in [0, 5, 7, 21, 31] {
        master.default.movement_type = kind;
        master.initialize(&mut c, &mut Unbound).unwrap();
        assert_eq!(master.current_type(), 0);
    }
    // MotionMaster.cpp:80-94 retains the initialized generator before group lookup.
    master.default.movement_type = 2;
    assert_eq!(
        master.initialize(&mut c, &mut Unbound).unwrap_err().id,
        "execution.waypoint.formation_path_binding_owner"
    );
    assert_eq!(master.len(), 1);
    assert_eq!(master.current_type(), 2);
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state & WANDER_ACTIVE,
        WANDER_ACTIVE
    );
    master
        .clear(false, true, &mut c, &mut HomeOwner::default())
        .unwrap();
    master.default.movement_type = 4;
    assert_eq!(
        master.initialize(&mut c, &mut Unbound).unwrap_err().id,
        "execution.waypoint.formation_path_binding_owner"
    );
    assert_eq!(master.current_type(), 2);
}
#[test]
fn initialize_possessed_and_player_owned_creature_use_static_idle() {
    let (mut objects, mut units, mut master) = setup();
    let player = objects.construct(1, SourceObjectKind::Player).unwrap();
    objects
        .assign_guid(player, 0, SourceGuid::without_entry(HighGuid::Player, 1))
        .unwrap();
    master.default.movement_type = 1;
    units.get_mut(&objects, master.unit).unwrap().owner = Some(player);
    master
        .initialize(
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    assert_eq!(master.current_type(), 0);
    units.get_mut(&objects, master.unit).unwrap().owner = None;
    units.get_mut(&objects, master.unit).unwrap().state = 0x40;
    master
        .initialize(
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    assert_eq!(master.current_type(), 0);
}
#[test]
fn path_read_refuses_only_before_defined_initialization() {
    let (objects, mut units, _) = setup();
    let unit = ObjectHandle {
        thread: 1,
        construction: 1,
    };
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 0,
            default_path: 7,
        },
    );
    assert_eq!(
        master.path_id().unwrap_err().id,
        "execution.motion.path_id_uninitialized_undefined"
    );
    units.get_mut(&objects, unit).unwrap().state = 0x40;
    master
        .initialize(
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    assert!(master.path_id().is_err());
    units.get_mut(&objects, unit).unwrap().state = 0;
    master
        .initialize(
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    assert_eq!(master.path_id().unwrap(), 7);
}
#[derive(Default)]
struct HomeOwner {
    calls: Vec<MotionCall>,
    start: [f32; 4],
    distance: f32,
    dungeon: bool,
    done: bool,
    faction: bool,
    summoner: Option<ObjectHandle>,
    targets: Vec<[f32; 4]>,
}
impl MotionExecutionOwner for HomeOwner {
    fn motion_call(
        &mut self,
        master: &mut SourceMotionMaster,
        call: MotionCall,
        context: &mut MotionContext<'_>,
    ) -> Result<()> {
        if call == MotionCall::TriggerHome {
            assert!(!context.units.get(context.objects, master.unit)?.evade_home);
        }
        self.calls.push(call);
        Ok(())
    }
    fn combat_start_position(&mut self, _: ObjectHandle) -> Result<[f32; 4]> {
        Ok(self.start)
    }
    fn home_distance(&mut self, _: ObjectHandle, target: [f32; 4]) -> Result<f32> {
        self.targets.push(target);
        Ok(self.distance)
    }
    fn dungeon(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.dungeon)
    }
    fn spline_finalized(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.done)
    }
    fn restore_faction_at_home(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.faction)
    }
    fn temporary_summoner(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.summoner)
    }
    fn ai_order(&mut self, _: ObjectHandle, _: u32) -> Result<()> {
        Ok(())
    }
    fn controlled_evade(&mut self, _: ObjectHandle, _: bool) -> Result<()> {
        Ok(())
    }
}
#[test]
fn home_completion_callback_order_from_existing_spline() {
    let (mut objects, mut units, mut master) = setup();
    // NOT_MOVE without CAN_NOT_MOVE permits Initialize's early return then Update.
    units.get_mut(&objects, master.unit).unwrap().state = 0x800;
    let summoner = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut f = SourceUnitFields::creature(&template());
    f.has_ai = true;
    units.register(&objects, summoner, f).unwrap();
    let mut owner = HomeOwner {
        faction: true,
        summoner: Some(summoner),
        ..Default::default()
    };
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master.move_home(true, &mut c, &mut owner).unwrap();
    assert!(c.units.get(&objects, master.unit).unwrap().evade_home);
    master.update(1, &mut c, &mut owner).unwrap();
    assert_eq!(master.current_type(), 12);
    owner.done = true;
    master.update(1, &mut c, &mut owner).unwrap();
    assert_eq!(
        owner.calls,
        [
            MotionCall::SetActive(true),
            MotionCall::ClearTemporaryFaction,
            MotionCall::LoadAddon,
            MotionCall::TriggerHome,
            MotionCall::SummonedHome(summoner),
            MotionCall::ClearCombatStart
        ]
    );
    assert_eq!(master.current_type(), 0);
}
#[test]
fn home_interruption_clears_evade_but_keeps_active_and_no_home_callbacks() {
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = 0x800;
    let mut owner = HomeOwner::default();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master.move_home(true, &mut c, &mut owner).unwrap();
    master
        .mutate(Probe::new("a", 11, &Rc::default()), &mut c, &mut owner)
        .unwrap();
    assert!(!c.units.get(&objects, master.unit).unwrap().evade_home);
    assert_eq!(owner.calls, [MotionCall::SetActive(true)]);
}
#[test]
fn home_missing_summoner_is_not_a_missing_callback_or_target() {
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = 0x800;
    let mut owner = HomeOwner {
        done: true,
        summoner: Some(ObjectHandle {
            thread: 1,
            construction: 999,
        }),
        ..Default::default()
    };
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master.move_home(true, &mut c, &mut owner).unwrap();
    master.update(1, &mut c, &mut owner).unwrap();
    assert_eq!(
        owner.calls,
        [
            MotionCall::SetActive(true),
            MotionCall::LoadAddon,
            MotionCall::TriggerHome,
            MotionCall::ClearCombatStart
        ]
    );
}
#[test]
fn home_empty_target_ignores_orientation_and_falls_back_before_f6_path() {
    let (objects, mut units, mut master) = setup();
    let mut owner = HomeOwner {
        start: [0., 0., 0., 2.],
        ..Default::default()
    };
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        master.move_home(false, &mut c, &mut owner).unwrap_err().id,
        "execution.motion.f6_home_pathfinder_constructor_owner"
    );
    assert_eq!(owner.targets, [[1.; 4]]);
    assert_eq!(master.len(), 1);
    assert!(c.units.get(&objects, master.unit).unwrap().evade_home);
}
#[test]
fn home_prefers_previous_generators_reset_position() {
    let (objects, mut units, mut master) = setup();
    let log = Rc::default();
    let mut p = Probe::new("a", 11, &log);
    p.position = Some([3.; 4]);
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    // Call Initialize directly with the existing generator, matching Mutate's pre-push order.
    master.mutate(p, &mut c, &mut Unbound).unwrap();
    let home = SourceHomeGenerator::new(true);
    let mut owner = HomeOwner::default();
    assert_eq!(
        home.initialize(&mut master, &mut c, &mut owner)
            .unwrap_err()
            .id,
        "execution.motion.f6_home_pathfinder_constructor_owner"
    );
    assert_eq!(owner.targets, [[3.; 4]]);
}
#[test]
fn home_far_out_of_world_finalizes_before_delay_and_despawn() {
    let (objects, mut units, mut master) = setup();
    let mut owner = HomeOwner {
        start: [2.; 4],
        distance: 22501.,
        ..Default::default()
    };
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master.move_home(true, &mut c, &mut owner).unwrap();
    assert_eq!(
        owner.calls,
        [
            MotionCall::SetActive(true),
            MotionCall::LoadAddon,
            MotionCall::TriggerHome,
            MotionCall::ClearCombatStart,
            MotionCall::RespawnDelayFive,
            MotionCall::ForcedDespawnOne
        ]
    );
    assert_eq!(master.current_type(), 12);
    // Source does not remove the generator after its immediate Finalize call.
    owner.done = true;
    master.update(1, &mut c, &mut owner).unwrap();
    assert_eq!(
        owner
            .calls
            .iter()
            .filter(|c| **c == MotionCall::TriggerHome)
            .count(),
        2
    );
}
#[test]
fn home_exact_far_threshold_still_requires_path() {
    let (objects, mut units, mut master) = setup();
    let mut owner = HomeOwner {
        start: [2.; 4],
        distance: 22500.,
        ..Default::default()
    };
    assert_eq!(
        master
            .move_home(
                true,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units
                },
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.motion.f6_home_pathfinder_constructor_owner"
    );
}
#[test]
fn lost_control_prevents_home_clear_and_missing_spline_refuses_on_update() {
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = 0x40;
    master
        .move_home(
            true,
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    assert_eq!(master.len(), 1);
    units.get_mut(&objects, master.unit).unwrap().state = 0x800;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master
        .move_home(true, &mut c, &mut HomeOwner::default())
        .unwrap();
    assert_eq!(
        master.update(1, &mut c, &mut Unbound).unwrap_err().id,
        "execution.motion.f6_spline_finalized_owner"
    );
}
#[test]
fn immobilization_preserves_other_root_and_refuses_missing_output_after_state_change() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        master
            .immobilize(true, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.root_movement_info_output_owner"
    );
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state & 0x10,
        0x10
    );
    c.units.get_mut(&objects, master.unit).unwrap().state |= UNIT_STATE_AI_ROOT;
    c.units
        .get_mut(&objects, master.unit)
        .unwrap()
        .ignore_ranged_targets = true;
    master.immobilize(false, &mut c, &mut Unbound).unwrap();
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state,
        UNIT_STATE_AI_ROOT
    );
    assert!(
        !c.units
            .get(&objects, master.unit)
            .unwrap()
            .ignore_ranged_targets
    );
    c.units.get_mut(&objects, master.unit).unwrap().state = 0;
    assert_eq!(
        master
            .immobilize(false, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.root_movement_info_output_owner"
    );
}
#[test]
fn empty_update_assertion_is_guarded_by_cannot_move() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master.clear(false, true, &mut c, &mut Unbound).unwrap();
    c.units.get_mut(&objects, master.unit).unwrap().state = 0x10;
    master.update(1, &mut c, &mut Unbound).unwrap();
    c.units.get_mut(&objects, master.unit).unwrap().state = 0;
    assert_eq!(
        master.update(1, &mut c, &mut Unbound).unwrap_err().id,
        "execution.motion.empty_update_assertion"
    );
    assert_eq!(
        master
            .clear(true, false, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.empty_reset_assertion"
    );
}
#[test]
fn formation_commands_use_master_and_leave_follow_waypoints_refused() {
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master
        .formation(
            FormationMotion::Clear {
                reset: false,
                all: true,
            },
            &mut c,
            &mut Unbound,
        )
        .unwrap();
    master
        .formation(FormationMotion::Idle, &mut c, &mut Unbound)
        .unwrap();
    master
        .formation(FormationMotion::Initialize, &mut c, &mut Unbound)
        .unwrap();
    assert_eq!(master.len(), 1);
    assert_eq!(
        master
            .formation(
                FormationMotion::Waypoint {
                    path: 9,
                    linear: false
                },
                &mut c,
                &mut Unbound
            )
            .unwrap_err()
            .id,
        "execution.waypoint.loaded_waypoint_manager_owner"
    );
    assert_eq!(master.current_type(), 2);
    assert_eq!(master.path_id().unwrap(), 9);
    assert_eq!(
        c.units.get(&objects, master.unit).unwrap().state & WANDER_ACTIVE,
        WANDER_ACTIVE
    );
}

#[test]
fn spawn_enum_conversion_preserves_undeclared_values_and_refuses_first_undefined() {
    assert_eq!(MotionDefault::from_spawn(31, 0).unwrap().movement_type, 31);
    assert_eq!(
        MotionDefault::from_spawn(32, 0).unwrap_err().id,
        "execution.motion.default_enum_conversion_undefined"
    );
}

#[test]
fn home_in_world_arrival_restores_inactive_after_summoner_and_before_position_clear() {
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().in_world = true;
    units.get_mut(&objects, master.unit).unwrap().state = 0x800;
    let mut owner = HomeOwner {
        done: true,
        ..Default::default()
    };
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master.move_home(true, &mut c, &mut owner).unwrap();
    master.update(1, &mut c, &mut owner).unwrap();
    assert_eq!(
        owner.calls,
        [
            MotionCall::SetActive(true),
            MotionCall::LoadAddon,
            MotionCall::TriggerHome,
            MotionCall::SetActive(false),
            MotionCall::ClearCombatStart
        ]
    );
}

#[test]
fn home_far_in_dungeon_still_requires_real_path() {
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().in_world = true;
    let mut owner = HomeOwner {
        start: [2.; 4],
        distance: 22501.,
        dungeon: true,
        ..Default::default()
    };
    assert_eq!(
        master
            .move_home(
                true,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units
                },
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.motion.f6_home_pathfinder_constructor_owner"
    );
    assert_eq!(owner.calls, [MotionCall::SetActive(true)]);
}

#[test]
fn home_missing_respawn_position_refuses_without_path_or_completion() {
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().respawn_pose = None;
    let mut owner = HomeOwner::default();
    assert_eq!(
        master
            .move_home(
                true,
                &mut MotionContext {
                    objects: &objects,
                    units: &mut units
                },
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.motion.respawn_position_absent"
    );
    assert!(owner.targets.is_empty());
    assert_eq!(owner.calls, [MotionCall::SetActive(true)]);
}

#[test]
fn top_down_destruction_does_not_finalize() {
    let (objects, mut units, mut master) = setup();
    let log = Rc::default();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master
        .mutate(Probe::new("a", 11, &log), &mut c, &mut Unbound)
        .unwrap();
    master
        .mutate(Probe::new("b", 11, &log), &mut c, &mut Unbound)
        .unwrap();
    log.borrow_mut().clear();
    drop(master);
    assert_eq!(&*log.borrow(), &["b.drop", "a.drop"]);
}

#[test]
fn initialize_stops_before_clear_and_refuses_spline_at_actual_call() {
    let (mut objects, mut units, mut master) = setup();
    objects.get_mut(master.unit).unwrap().in_world = true;
    units.get_mut(&objects, master.unit).unwrap().state = 0x10000;
    let log = Rc::default();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    master
        .mutate(Probe::new("a", 11, &log), &mut c, &mut Unbound)
        .unwrap();
    log.borrow_mut().clear();
    assert_eq!(
        master.initialize(&mut c, &mut Unbound).unwrap_err().id,
        "execution.motion.f6_spline_stop_owner"
    );
    assert!(log.borrow().is_empty());
    assert_eq!(master.len(), 2);
}

#[test]
fn delayed_clear_resets_the_retained_default_only_after_update_returns() {
    for reset in [false, true] {
        let (objects, mut units, mut master) = setup();
        let log = Rc::default();
        let mut c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        master.clear(false, true, &mut c, &mut Unbound).unwrap();
        master
            .mutate(Probe::new("base", 11, &log), &mut c, &mut Unbound)
            .unwrap();
        let mut top = Probe::new("top", 11, &log);
        top.during = DuringUpdate::Clear(reset, false);
        master.mutate(top, &mut c, &mut Unbound).unwrap();
        log.borrow_mut().clear();
        master.update(1, &mut c, &mut Unbound).unwrap();
        let mut expected = vec!["top.update", "top.finalize", "top.return", "top.drop"];
        if reset {
            expected.push("base.reset");
        }
        assert_eq!(&*log.borrow(), &expected);
    }
}

#[test]
fn evade_state_preserves_equality_timer_and_order_then_propagates_unless_charmed() {
    struct EvadeOwner(Vec<String>);
    impl MotionExecutionOwner for EvadeOwner {
        fn ai_order(&mut self, _: ObjectHandle, order: u32) -> Result<()> {
            self.0.push(format!("order{order}"));
            Ok(())
        }
        fn controlled_evade(&mut self, _: ObjectHandle, home: bool) -> Result<()> {
            self.0.push(format!("controlled{home}"));
            Ok(())
        }
    }
    let (objects, mut units, mut master) = setup();
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    c.units.get_mut(&objects, master.unit).unwrap().evade_timer = 9;
    master.set_evade_home(false, &mut c, &mut Unbound).unwrap();
    assert_eq!(c.units.get(&objects, master.unit).unwrap().evade_timer, 9);
    assert_eq!(
        master
            .set_evade_home(true, &mut c, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.motion.base_ai_order_owner"
    );
    assert!(!c.units.get(&objects, master.unit).unwrap().evade_home);
    let mut owner = EvadeOwner(Vec::new());
    master.set_evade_home(true, &mut c, &mut owner).unwrap();
    master.set_evade_home(true, &mut c, &mut Unbound).unwrap();
    master.set_evade_home(false, &mut c, &mut owner).unwrap();
    assert_eq!(c.units.get(&objects, master.unit).unwrap().evade_timer, 0);
    assert_eq!(
        owner.0,
        ["order4", "controlledtrue", "order0", "controlledfalse"]
    );
    c.units.get_mut(&objects, master.unit).unwrap().charmer = Some(master.unit);
    owner.0.clear();
    master.set_evade_home(true, &mut c, &mut owner).unwrap();
    assert_eq!(owner.0, ["order4"]);
}

#[test]
fn home_reset_and_interrupt_leave_arrival_state_unchanged() {
    let (objects, mut units, mut master) = setup();
    units.get_mut(&objects, master.unit).unwrap().state = 0x800;
    let mut c = MotionContext {
        objects: &objects,
        units: &mut units,
    };
    let home = SourceHomeGenerator::new(true);
    let mut owner = HomeOwner::default();
    home.initialize(&mut master, &mut c, &mut owner).unwrap();
    owner.calls.clear();
    home.reset(&mut master, &mut c, &mut Unbound).unwrap();
    home.interrupt(&mut master, &mut c, &mut Unbound).unwrap();
    assert!(owner.calls.is_empty());
    assert!(c.units.get(&objects, master.unit).unwrap().evade_home);
}

#[test]
fn player_initialize_does_not_read_creature_default_or_initialize_path() {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Player).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(&objects, unit, SourceUnitFields::creature(&template()))
        .unwrap();
    let mut master = SourceMotionMaster::new(
        unit,
        MotionDefault {
            movement_type: 1,
            default_path: 7,
        },
    );
    master
        .initialize(
            &mut MotionContext {
                objects: &objects,
                units: &mut units,
            },
            &mut Unbound,
        )
        .unwrap();
    assert_eq!(master.current_type(), 0);
    assert!(master.path_id().is_err());
}

#[test]
fn eventai_action64_reaches_root_state_and_still_refuses_missing_output() {
    struct Dependencies;
    impl UnitAiExecutionOwner for Dependencies {}
    impl MotionExecutionOwner for Dependencies {}
    impl CreatureRandom for Dependencies {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("unexpected root RNG")
        }
        fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
            panic!("unexpected root RNG")
        }
    }
    let (objects, mut units, master) = setup();
    let unit = master.unit;
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\nCreatureCheckForHelpRadius=5\n";
    let config = EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap();
    let base = SourceCreatureAi::construct(
        &objects,
        unit,
        &mut units,
        &template(),
        false,
        0,
        &config,
        &mut Dependencies,
    )
    .unwrap();
    let mut ais = SourceCreatureAis::default();
    ais.install(&objects, &mut units, base, &mut SourceCombat::default())
        .unwrap();
    let mut motions = SourceMotions::default();
    motions.construct(&objects, unit, master.default).unwrap();
    let mut dependencies = Dependencies;
    let mut owner = SourceMotionDependencies {
        motions: &mut motions,
        owner: &mut dependencies,
    };
    let mut threats = SourceThreats::default();
    let mut state = super::event_ai_dispatch_tests::state();
    let error = {
        let mut execution = SourceAiExecution {
            queries: SourceUnitQueries {
                objects: &objects,
                units: &mut units,
            },
            ais: &mut ais,
            threats: &mut threats,
            owner: &mut owner,
        };
        execution
            .dependent_action(
                &mut state,
                &EventAiAction {
                    kind: 64,
                    params: Some([1, 1, 0]),
                },
                EventAiInvocation::default(),
                None,
                0,
            )
            .unwrap_err()
    };
    assert_eq!(error.id, "execution.motion.root_movement_info_output_owner");
    assert_eq!(
        units.get(&objects, unit).unwrap().state & (UNIT_STATE_AI_ROOT | 0x10),
        UNIT_STATE_AI_ROOT | 0x10
    );
    assert!(ais.get(&objects, unit).unwrap().base.combat_only_root);
    units.get_mut(&objects, unit).unwrap().state |= 8;
    let mut execution = SourceAiExecution {
        queries: SourceUnitQueries {
            objects: &objects,
            units: &mut units,
        },
        ais: &mut ais,
        threats: &mut threats,
        owner: &mut owner,
    };
    assert!(execution
        .dependent_action(
            &mut state,
            &EventAiAction {
                kind: 64,
                params: Some([0, 1, 0])
            },
            EventAiInvocation::default(),
            None,
            0
        )
        .unwrap());
    assert_eq!(
        execution.queries.units.get(&objects, unit).unwrap().state,
        8
    );
    assert!(
        !execution
            .ais
            .get(&objects, unit)
            .unwrap()
            .base
            .combat_only_root
    );
}
