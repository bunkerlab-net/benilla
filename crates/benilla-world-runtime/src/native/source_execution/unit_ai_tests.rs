use super::*;
use crate::native::source_execution::authored_effect_holder;
use std::collections::VecDeque;

fn template() -> CreatureAiTemplateOperands {
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
fn config() -> EventAiLoaderConfig {
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\nCreatureCheckForHelpRadius=5\n";
    EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap()
}
#[derive(Default)]
struct Authored {
    calls: Vec<UnitAiCall>,
    actions: Vec<u32>,
    stop_at: Option<u32>,
    root_removed: bool,
    process: Option<benilla_sim::source_rng::ProcessCRand>,
    c_draws: u32,
    words: VecDeque<u32>,
    sights: Vec<UnitAiSightQuery>,
}
impl UnitAiExecutionOwner for Authored {
    fn call(
        &mut self,
        unit: ObjectHandle,
        call: UnitAiCall,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.calls.push(call);
        match call {
            UnitAiCall::RemoveAurasOnEvade => {
                if self.root_removed {
                    units.get_mut(objects, unit)?.state &= !0x10;
                }
            }
            UnitAiCall::Home | UnitAiCall::Chase { .. } | UnitAiCall::GuardPosition(_) => {
                return Err(call.missing_owner())
            }
            _ => {}
        }
        Ok(())
    }
    fn has_unattackable_aura(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn sight_query(
        &mut self,
        _: ObjectHandle,
        _: ObjectHandle,
        query: UnitAiSightQuery,
    ) -> Result<bool> {
        self.sights.push(query);
        Ok(matches!(
            query,
            UnitAiSightQuery::WithinVerticalRange(_)
                | UnitAiSightQuery::AttackOnSight
                | UnitAiSightQuery::Accessible
        ))
    }
    fn execute_action(
        &mut self,
        ai: &mut SourceUnitAi,
        action: u32,
        _: &mut SourceUnits,
        _: &SourceObjects,
    ) -> Result<()> {
        self.actions.push(action);
        if self.stop_at == Some(action) {
            ai.combat_script = true;
        }
        Ok(())
    }
    fn c_rand(&mut self) -> Result<u32> {
        let process = self
            .process
            .as_mut()
            .ok_or_else(|| refuse("execution.unit_ai.process_c_rng_owner"))?;
        self.c_draws += 1;
        Ok(process.next_i32() as u32)
    }
    fn spell_list_has_spells(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
}
impl CreatureRandom for Authored {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected MT signed draw")
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        let n = self.words.pop_front().expect("unrecorded MT draw");
        assert!((min..=max).contains(&n));
        Ok(n)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.creature_urand(0, u32::MAX)
    }
}
fn setup(actions: u32) -> (SourceObjects, SourceUnits, SourceUnitAi) {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(&objects, unit, SourceUnitFields::creature(&template()))
        .unwrap();
    let ai = SourceUnitAi::construct(
        &objects,
        unit,
        actions,
        &mut Authored {
            words: [0, 0].into(),
            ..Authored::default()
        },
    )
    .unwrap();
    (objects, units, ai)
}
#[test]
fn unit_ai_constructor_retains_source_defaults_and_disabled_custom_actions() {
    let (_, _, ai) = setup(3);
    assert_eq!(ai.react_state, 2);
    assert_eq!(ai.visibility_distance, 166.);
    assert!(ai.melee_enabled && ai.chase_run && ai.dismount_on_aggro && ai.spell_list_cooldown);
    assert!(
        !ai.combat_script && !ai.current_ranged_mode && !ai.ranged_mode && !ai.teleport_unreachable
    );
    assert_eq!(
        (0..3)
            .map(|i| ai.action_ready(i).unwrap())
            .collect::<Vec<_>>(),
        [false; 3]
    );
    assert_eq!(
        ai.generic_timers.map(|t| (t.id, t.remaining, t.disabled)),
        [(2000, 0, true), (2001, 0, true)]
    );
}
#[test]
fn creature_reaction_precedence_is_two_ifs_not_one_chain() {
    let config = config();
    for no_aggro in [false, true] {
        for defensive in [false, true] {
            for ignore in [false, true] {
                let (objects, mut units, base) = setup(0);
                let mut t = template();
                t.extra_flags = u32::from(no_aggro) * 2;
                t.static_flags[0] = u32::from(ignore) * 0x2000000;
                t.static_flags[1] = u32::from(defensive) * 0x4000000;
                units.get_mut(&objects, base.unit).unwrap().static_flags = t.static_flags;
                let ai = SourceCreatureAi::construct(
                    &objects,
                    base.unit,
                    &mut units,
                    &t,
                    false,
                    0,
                    &config,
                    &mut Authored {
                        words: [0, 0].into(),
                        ..Authored::default()
                    },
                )
                .unwrap();
                assert_eq!(
                    ai.base.react_state,
                    if no_aggro || defensive { 1 } else { 2 }
                );
                assert_eq!(
                    units.get(&objects, base.unit).unwrap().can_enter_combat,
                    defensive || !ignore
                );
            }
        }
    }
}
#[test]
fn creature_constructor_mount_melee_root_and_guard_precedence() {
    let config = config();
    let (objects, mut units, base) = setup(0);
    let mut t = template();
    t.type_flags = 0x800;
    t.static_flags[0] = 0x100 | 0x100000;
    t.extra_flags = 0x400;
    units.get_mut(&objects, base.unit).unwrap().static_flags = t.static_flags;
    let mut owner = Authored::default();
    owner.words.push_front(0);
    owner.words.push_front(0);
    let ai = SourceCreatureAi::construct(
        &objects, base.unit, &mut units, &t, false, 0, &config, &mut owner,
    )
    .unwrap();
    assert!(!ai.base.dismount_on_aggro && !ai.base.melee_enabled);
    assert_eq!(ai.base.visibility_distance, 50.);
    assert_eq!(
        units.get(&objects, base.unit).unwrap().state,
        UNIT_STATE_AI_ROOT
    );
    assert_eq!(owner.calls, [UnitAiCall::Immobilized(true)]);
}
#[test]
fn action_changing_combat_script_stops_iteration_before_next_action() {
    let (objects, mut units, mut ai) = setup(3);
    for i in 0..3 {
        ai.set_action_ready(i, true).unwrap();
    }
    let mut owner = Authored {
        stop_at: Some(1),
        ..Authored::default()
    };
    ai.execute_actions(&objects, &mut units, &mut owner)
        .unwrap();
    assert_eq!(owner.actions, [0, 1]);
    ai.execute_actions(&objects, &mut units, &mut owner)
        .unwrap();
    assert_eq!(owner.actions, [0, 1]);
    ai.combat_script = false;
    owner.stop_at = None;
    ai.execute_actions(&objects, &mut units, &mut owner)
        .unwrap();
    assert_eq!(owner.actions, [0, 1, 0, 1, 2]);
}
#[test]
fn combat_action_gate_uses_flags_state_alive_evade_casting_and_script() {
    let (mut objects, mut units, mut ai) = setup(0);
    for (flags, state, expected) in [
        (0x2000, 0, true),
        (0x20000, 0, true),
        (0x22000, 0, false),
        (0x40000, 0, false),
        (0x400000, 0, false),
        (0x800000, 0, false),
        (0, 0x100, false),
        (0, 0x400, false),
        (0, 0x10, true),
    ] {
        let f = units.get_mut(&objects, ai.unit).unwrap();
        f.flags = flags;
        f.state = state;
        assert_eq!(ai.can_execute(&objects, &units).unwrap(), expected);
    }
    let f = units.get_mut(&objects, ai.unit).unwrap();
    f.state = 0;
    f.evade_combat = true;
    assert!(ai.can_execute(&objects, &units).unwrap());
    units.get_mut(&objects, ai.unit).unwrap().evade_home = true;
    assert!(!ai.can_execute(&objects, &units).unwrap());
    units.get_mut(&objects, ai.unit).unwrap().evade_home = false;
    units.get_mut(&objects, ai.unit).unwrap().casting_non_melee = true;
    assert!(!ai.can_execute(&objects, &units).unwrap());
    units.get_mut(&objects, ai.unit).unwrap().casting_non_melee = false;
    ai.combat_script = true;
    assert!(!ai.can_execute(&objects, &units).unwrap());
    ai.combat_script = false;
    objects.mark_dead(ai.unit).unwrap();
    assert!(!ai.can_execute(&objects, &units).unwrap());
}
#[test]
fn evade_removes_root_aura_before_deciding_home_and_refuses_missing_home() {
    let (objects, mut units, mut ai) = setup(0);
    units.get_mut(&objects, ai.unit).unwrap().state = 0x10;
    let mut owner = Authored {
        root_removed: true,
        ..Authored::default()
    };
    assert_eq!(
        ai.enter_evade(&objects, &mut units, &mut owner)
            .unwrap_err()
            .id,
        "execution.unit_ai.f5_05_motion_owner"
    );
    assert_eq!(
        owner.calls,
        [
            UnitAiCall::RemoveAurasOnEvade,
            UnitAiCall::CombatStopWithPets,
            UnitAiCall::Home
        ]
    );
    units.get_mut(&objects, ai.unit).unwrap().state = 0x10;
    let mut owner = Authored::default();
    ai.enter_evade(&objects, &mut units, &mut owner).unwrap();
    assert_eq!(
        owner.calls,
        [
            UnitAiCall::RemoveAurasOnEvade,
            UnitAiCall::CombatStopWithPets,
            UnitAiCall::TriggerHome,
            UnitAiCall::TriggerEvade
        ]
    );
}
#[test]
fn evade_missing_aura_owner_never_attempts_home_or_resets() {
    struct Missing;
    impl UnitAiExecutionOwner for Missing {}
    let (objects, mut units, mut ai) = setup(0);
    assert_eq!(
        ai.enter_evade(&objects, &mut units, &mut Missing)
            .unwrap_err()
            .id,
        "execution.unit_ai.f5_12_evade_aura_owner"
    );
}
#[test]
fn normal_hostile_npc_attack_engages_before_chase_not_player_shortcut() {
    let (mut objects, mut units, mut ai) = setup(0);
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut t = template();
    t.faction = 12;
    units
        .register(&objects, target, SourceUnitFields::creature(&t))
        .unwrap();
    make_live(&mut objects, ai.unit, 1);
    make_live(&mut objects, target, 2);
    let mut owner = Authored::default();
    assert_eq!(
        ai.attack_start(Some(target), &objects, &mut units, &mut owner)
            .unwrap_err()
            .id,
        "execution.unit_ai.f5_05_motion_owner"
    );
    assert!(
        units.get(&objects, ai.unit).unwrap().in_combat()
            && units.get(&objects, target).unwrap().in_combat()
    );
    assert_eq!(
        owner.calls,
        [
            UnitAiCall::ThreatCurrent(target),
            UnitAiCall::MeleeStart(Some(target)),
            UnitAiCall::AiReaction,
            UnitAiCall::AddInitialThreat(target),
            UnitAiCall::CreatureCombatStart(target),
            UnitAiCall::CreatureCombatStart(ai.unit),
            UnitAiCall::Chase {
                target,
                target_change: false,
                distance_bits: 0,
                angle_bits: 0,
                move_further: false,
                walk: false,
            }
        ]
    );
}
#[test]
fn passive_and_null_attack_do_not_touch_attack_owner() {
    let (objects, mut units, mut ai) = setup(0);
    let mut owner = Authored::default();
    ai.attack_start(None, &objects, &mut units, &mut owner)
        .unwrap();
    ai.react_state = 0;
    ai.attack_start(Some(ai.unit), &objects, &mut units, &mut owner)
        .unwrap();
    assert!(owner.calls.is_empty());
}
#[test]
fn guard_c_rng_branch_refuses_only_at_the_reached_process_draw() {
    let mut owner = Authored::default();
    assert_eq!(guard_entry(1519, &mut owner).unwrap(), 68);
    assert_eq!(guard_entry(9999, &mut owner).unwrap(), 0);
    assert_eq!(
        guard_entry(221, &mut owner).unwrap_err().id,
        "execution.unit_ai.process_c_rng_owner"
    );
    assert_eq!(owner.c_draws, 0);
    owner.process = Some(benilla_sim::source_rng::ProcessCRand::from_seed(1));
    assert_eq!(guard_entry(221, &mut owner).unwrap(), 3214);
    assert_eq!(guard_entry(222, &mut owner).unwrap(), 3213);
    assert_eq!(guard_entry(1519, &mut owner).unwrap(), 68);
    assert_eq!(owner.c_draws, 2);
}
#[test]
fn absent_vitals_are_not_template_health_or_zero_defaults() {
    let (objects, mut units, ai) = setup(0);
    assert_eq!(
        units.event_unit(&objects, ai.unit).unwrap_err().id,
        "execution.unit.select_level_stats_owner"
    );
    units.get_mut(&objects, ai.unit).unwrap().vitals = Some(SourceUnitVitals {
        health: 73,
        max_health: 100,
        mana: 12,
        max_mana: 20,
        has_mana: true,
        energy: 0,
        max_energy: 0,
    });
    let unit = units.event_unit(&objects, ai.unit).unwrap();
    assert_eq!((unit.health, unit.max_health, unit.mana), (73, 100, 12));
}

fn make_live(objects: &mut SourceObjects, unit: ObjectHandle, low: u32) {
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    objects
        .assign_guid(
            unit,
            low,
            SourceGuid::with_entry(HighGuid::Creature, 901, low),
        )
        .unwrap();
    objects.bind_map(unit, map, &cells).unwrap();
    objects.set_pose(unit, SourcePose([0; 4])).unwrap();
    objects.add_to_world(unit).unwrap();
}

#[test]
fn attack_stop_clears_target_then_melee_then_attacker_and_preserves_combat() {
    let (mut objects, mut units, ai) = setup(0);
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    units
        .register(&objects, target, SourceUnitFields::creature(&template()))
        .unwrap();
    make_live(&mut objects, target, 2);
    let mut owner = Authored::default();
    assert!(ai
        .attack_unit(target, &objects, &mut units, &mut owner)
        .unwrap());
    units.get_mut(&objects, ai.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
    owner.calls.clear();
    assert!(ai
        .attack_stop(true, false, false, false, &objects, &mut units, &mut owner)
        .unwrap());
    let fields = units.get(&objects, ai.unit).unwrap();
    assert!(fields.target.is_none() && fields.victim.is_none() && fields.in_combat());
    assert_eq!(fields.state & UNIT_STATE_MELEE, 0);
    assert!(units.get(&objects, target).unwrap().attackers.is_empty());
    assert_eq!(owner.calls, [UnitAiCall::MeleeStop(Some(target))]);
    assert!(!ai
        .attack_stop(true, false, false, false, &objects, &mut units, &mut owner)
        .unwrap());
}

#[test]
fn action_index_refuses_at_source_unchecked_vector_access() {
    let (_, _, mut ai) = setup(1);
    ai.set_action_ready(0, true).unwrap();
    assert!(ai.action_ready(0).unwrap());
    assert_eq!(
        ai.action_ready(1).unwrap_err().id,
        "execution.unit_ai.action_index_undefined"
    );
    assert_eq!(
        ai.set_action_ready(1, true).unwrap_err().id,
        "execution.unit_ai.action_index_undefined"
    );
}

#[test]
fn defensive_sight_assists_before_aggressive_filter_and_passive_reads_nothing() {
    let (mut objects, mut units, mut ai) = setup(0);
    let who = objects.construct(1, SourceObjectKind::Creature).unwrap();
    make_live(&mut objects, who, 2);
    let mut other = SourceUnitFields::creature(&template());
    other.flags |= UNIT_FLAG_COMBAT;
    units.register(&objects, who, other).unwrap();
    let f = units.get_mut(&objects, ai.unit).unwrap();
    f.can_fly = true;
    f.neutral_to_all = Some(false);
    ai.react_state = 1;
    let mut owner = Authored::default();
    ai.move_in_line_of_sight(who, &objects, &mut units, &config(), &mut owner)
        .unwrap();
    assert_eq!(
        owner.calls,
        [UnitAiCall::CheckForHelp {
            target: who,
            radius_bits: 5f32.to_bits()
        }]
    );
    assert!(owner.sights.is_empty());
    ai.react_state = 0;
    ai.move_in_line_of_sight(
        ObjectHandle {
            thread: 7,
            construction: 999,
        },
        &objects,
        &mut units,
        &config(),
        &mut owner,
    )
    .unwrap();
    assert_eq!(owner.calls.len(), 1);
}

#[test]
fn ranged_mode_ooc_keeps_source_true_when_disabling_and_no_melee_mode_returns_early() {
    let (objects, mut units, mut ai) = setup(0);
    let mut owner = Authored::default();
    ai.set_ranged_mode(true, 35., 1, &objects, &mut units, &mut owner)
        .unwrap();
    ai.set_ranged_mode(false, 20., 0, &objects, &mut units, &mut owner)
        .unwrap();
    assert!(!ai.ranged_mode && ai.current_ranged_mode);
    assert_eq!(ai.attack_distance, 20.);
    ai.ranged_setting = 3;
    ai.set_current_ranged(false, &objects, &mut units, &mut owner)
        .unwrap();
    assert!(ai.current_ranged_mode);
    assert_eq!(ai.attack_distance, 20.);
    assert!(owner.calls.is_empty());
}

#[test]
fn event_ai_reads_real_base_flags_reaction_and_threat_without_stats_default() {
    let (mut objects, mut units, base) = setup(0);
    make_live(&mut objects, base.unit, 1);
    let mut state = super::event_ai_dispatch_tests::state();
    let mut owner = Authored {
        words: [0, 0].into(),
        ..Authored::default()
    };
    owner.words.push_front(0);
    owner.words.push_front(0);
    let ai = SourceCreatureAi::construct(
        &objects,
        base.unit,
        &mut units,
        &template(),
        false,
        0,
        &config(),
        &mut owner,
    )
    .unwrap();
    let mut ais = SourceCreatureAis::default();
    ais.install(&objects, &mut units, ai, &mut SourceCombat::default())
        .unwrap();
    let mut threats = SourceThreats::default();
    threats.register(base.unit).unwrap();
    let mut execution = SourceAiExecution {
        queries: SourceUnitQueries {
            objects: &objects,
            units: &mut units,
        },
        ais: &mut ais,
        threats: &mut threats,
        owner: &mut owner,
    };
    state.holders[0].event.flags = 0x400;
    state.holders[0].event.chance = 100;
    assert!(state
        .ready_event(0, EventAiInvocation::default(), &mut execution)
        .unwrap());
    assert!(state
        .dispatch_event(0, EventAiInvocation::default(), &mut execution)
        .is_ok());
    for (kind, params) in [(18, [0x22000, 0, 0]), (50, [1, 0, 0]), (14, [0, 0, 0])] {
        assert!(state
            .execute_action(
                &EventAiAction {
                    kind,
                    params: Some(params)
                },
                0,
                EventAiInvocation::default(),
                None,
                &mut execution
            )
            .unwrap());
    }
    assert!(!execution.can_execute_combat_action(base.unit).unwrap());
    assert_eq!(
        execution
            .ais
            .get(&objects, base.unit)
            .unwrap()
            .base
            .react_state,
        1
    );
    assert!(state
        .execute_action(
            &EventAiAction {
                kind: 19,
                params: Some([0x22000, 0, 0])
            },
            0,
            EventAiInvocation::default(),
            None,
            &mut execution
        )
        .unwrap());
    assert!(execution.can_execute_combat_action(base.unit).unwrap());
    state.holders[0].event.kind = 2;
    state.holders[0].event.params[4] = 0;
    state.holders[0].in_progress = false;
    state.holders[0].enabled = true;
    state.holders[0].timer = 0;
    assert!(!state
        .ready_event(0, EventAiInvocation::default(), &mut execution)
        .unwrap());
    state.holders[0].event.params[4] = 1;
    assert_eq!(
        state
            .ready_event(0, EventAiInvocation::default(), &mut execution)
            .unwrap_err()
            .id,
        "execution.unit.select_level_stats_owner"
    );
}

#[test]
fn recorded_creature_rates_refuse_missing_and_mismatched_values() {
    use benilla_sim::{classic_spell::SpellFloat, creature_init::Rates};
    let rates = Rates {
        normal_health: SpellFloat::ONE,
        normal_damage: SpellFloat::ONE,
        normal_corpse_decay_seconds: 300,
    };
    assert_eq!(
        config().validate_creature_rates(&rates).unwrap_err().id,
        "execution.unit_ai.config_health_rate_missing"
    );
    let text = b"LogFilter_EventAiDev=1\nRate.Creature.Normal.HP=1\nRate.Creature.Normal.Damage=1\nCorpse.Decay.NORMAL=300\n";
    let config = EventAiLoaderConfig::parse(text, Sha256Id::of(text)).unwrap();
    config.validate_creature_rates(&rates).unwrap();
    assert_eq!(
        config
            .validate_creature_rates(&Rates {
                normal_corpse_decay_seconds: 301,
                ..rates
            })
            .unwrap_err()
            .id,
        "execution.unit_ai.kernel_recorded_rates_mismatch"
    );
}

#[test]
fn reaction_enum_allows_undeclared_three_but_refuses_four_before_assignment() {
    let (_, _, mut ai) = setup(0);
    ai.set_react_state(3).unwrap();
    assert_eq!(ai.react_state, 3);
    assert_eq!(
        ai.set_react_state(4).unwrap_err().id,
        "execution.unit_ai.react_enum_conversion_undefined"
    );
    assert_eq!(ai.react_state, 3);
}

#[test]
fn threat_percent_action_mutates_live_npc_reference_using_owned_guid_fields() {
    let (mut objects, mut units, base) = setup(0);
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    units
        .register(&objects, target, SourceUnitFields::creature(&template()))
        .unwrap();
    make_live(&mut objects, base.unit, 1);
    make_live(&mut objects, target, 2);
    let mut owner = Authored::default();
    assert!(base
        .attack_unit(target, &objects, &mut units, &mut owner)
        .unwrap());
    let (mut threats, mut authored, source, npc, _) = super::threat_tests::setup();
    assert_eq!((source, npc), (base.unit, target));
    let guid = objects.get(target).unwrap().guid.unwrap().raw();
    authored.units.get_mut(&target).unwrap().guid = guid;
    super::threat_tests::add(&mut threats, &mut authored, source, target, 100);
    owner.words.push_front(0);
    owner.words.push_front(0);
    let ai = SourceCreatureAi::construct(
        &objects,
        base.unit,
        &mut units,
        &template(),
        false,
        0,
        &config(),
        &mut owner,
    )
    .unwrap();
    let mut ais = SourceCreatureAis::default();
    ais.install(&objects, &mut units, ai, &mut SourceCombat::default())
        .unwrap();
    let mut state = super::event_ai_dispatch_tests::state();
    let mut execution = SourceAiExecution {
        queries: SourceUnitQueries {
            objects: &objects,
            units: &mut units,
        },
        ais: &mut ais,
        threats: &mut threats,
        owner: &mut owner,
    };
    assert!(state
        .execute_action(
            &EventAiAction {
                kind: 13,
                params: Some([50, 1, 0])
            },
            0,
            EventAiInvocation::default(),
            None,
            &mut execution
        )
        .unwrap());
    assert_eq!(
        execution
            .threats
            .manager(source)
            .unwrap()
            .threat(guid, false)
            .unwrap(),
        ThreatValue::from_u32(150)
    );
}

#[test]
fn ranged_melee_switch_with_no_victim_still_sets_melee_state() {
    let (objects, mut units, mut ai) = setup(0);
    units.get_mut(&objects, ai.unit).unwrap().state = 0x10;
    ai.current_ranged_mode = true;
    ai.set_current_ranged(false, &objects, &mut units, &mut Authored::default())
        .unwrap();
    assert!(!ai.current_ranged_mode);
    assert_eq!(units.get(&objects, ai.unit).unwrap().state, 0x11);
    assert!(units.get(&objects, ai.unit).unwrap().ignore_ranged_targets);
}

#[test]
fn threat_rebind_uses_existing_guid_owner_before_npc_base_world_membership() {
    let (mut objects, mut units, ai) = setup(0);
    make_live(&mut objects, ai.unit, 1);
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let map = objects.get(ai.unit).unwrap().map.unwrap();
    let cells = SourceCells::new(map).unwrap();
    let guid = SourceGuid::with_entry(HighGuid::Creature, 902, 2);
    objects.assign_guid(target, 2, guid).unwrap();
    objects.bind_map(target, map, &cells).unwrap();
    objects.register_world_lookup(target).unwrap();
    assert!(!objects.get(target).unwrap().in_world);
    let mut queries = SourceUnitQueries {
        objects: &objects,
        units: &mut units,
    };
    assert_eq!(
        queries.threat_lookup(ai.unit, guid.raw()).unwrap(),
        Some(target)
    );
    assert_eq!(
        queries
            .threat_lookup(
                ai.unit,
                SourceGuid::without_entry(HighGuid::Corpse, 2).raw()
            )
            .unwrap(),
        None
    );
}

#[test]
fn attacked_by_uses_creature_override_and_ping_precedes_passive_filter() {
    let (mut objects, mut units, base) = setup(0);
    let spawner = objects.construct(1, SourceObjectKind::Player).unwrap();
    let mut owner = Authored::default();
    owner.words.push_front(0);
    owner.words.push_front(0);
    let mut ai = SourceCreatureAi::construct(
        &objects,
        base.unit,
        &mut units,
        &template(),
        false,
        0,
        &config(),
        &mut owner,
    )
    .unwrap();
    let fields = units.get_mut(&objects, base.unit).unwrap();
    fields.static_flags[0] |= 0x20000;
    fields.spawner = Some(spawner);
    ai.base.set_react_state(0).unwrap();
    ai.attacked_by(Some(base.unit), &objects, &mut units, &mut owner)
        .unwrap();
    assert_eq!(owner.calls, [UnitAiCall::MinimapPing]);
}

#[test]
fn help_predicate_uses_source_guid_high_not_the_object_kind() {
    let (mut objects, mut units, mut ai) = setup(0);
    let who = objects.construct(1, SourceObjectKind::Creature).unwrap();
    // ObjectGuid.h:87 preserves entry bits overlapping high GUID; UnitAI.cpp:83 reads that high.
    let guid = SourceGuid::with_entry(HighGuid::Creature, 1 << 30, 2);
    objects.assign_guid(who, 2, guid).unwrap();
    assert_ne!(guid.high(), HighGuid::Creature as u16);
    let mut target = SourceUnitFields::creature(&template());
    target.flags |= UNIT_FLAG_COMBAT;
    units.register(&objects, who, target).unwrap();
    let f = units.get_mut(&objects, ai.unit).unwrap();
    f.can_fly = true;
    f.neutral_to_all = Some(false);
    ai.set_react_state(1).unwrap();
    let mut owner = Authored::default();
    ai.move_in_line_of_sight(who, &objects, &mut units, &config(), &mut owner)
        .unwrap();
    assert!(owner.calls.is_empty());
}

#[test]
fn passenger_construction_does_not_change_serialized_unit_bytes() {
    let (objects, units, ai) = setup(0);
    let before = serde_json::to_vec(units.get(&objects, ai.unit).unwrap()).unwrap();
    let world = crate::native::synthetic_fixture::world();
    let world_before = world.checkpoint().unwrap().encode().unwrap();
    let mut passengers = SourcePassengers::default();
    passengers.construct(ai.unit, &objects).unwrap();
    assert_eq!(passengers.transport(ai.unit).unwrap(), None);
    assert_eq!(world.checkpoint().unwrap().encode().unwrap(), world_before);
    assert_eq!(
        serde_json::to_vec(units.get(&objects, ai.unit).unwrap()).unwrap(),
        before
    );
    assert_eq!(
        SourceManagers::default()
            .require_durable_checkpoint()
            .unwrap_err()
            .id,
        "execution.managers.f7_continuation_owner"
    );
}

#[test]
fn fresh_move_spline_is_done_inventory_only_and_cannot_restore_a_unit() {
    use benilla_sim::source_spline::Args;
    let (objects, mut units, ai) = setup(0);
    let fields = units.get(&objects, ai.unit).unwrap();
    assert!(fields.movespline.finalized());
    assert!(!fields.movespline.initialized());
    assert_eq!(
        serde_json::to_value(&fields.movespline).unwrap()["facing"],
        serde_json::json!([0, 0, null])
    );
    let before = serde_json::to_vec(fields).unwrap();
    assert!(!serde_json::to_value(fields)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("movespline"));
    let args = Args {
        path: vec![[0; 3], [1f32.to_bits(), 0, 0]],
        flags: 0,
        facing: [Some(0); 3],
        path_index_offset: 0,
        velocity: 1f32.to_bits(),
        spline_id: 9,
    };
    let spline = &mut units.get_mut(&objects, ai.unit).unwrap().movespline;
    spline.initialize(&args).unwrap();
    spline.update_state(7, |_| {}).unwrap();
    assert_eq!(
        serde_json::to_vec(units.get(&objects, ai.unit).unwrap()).unwrap(),
        before
    );
    let inventory = units.inventory(&objects).unwrap();
    assert_eq!(inventory["splines"][0][1]["time_passed"], 7);
    assert_eq!(inventory["restore"], "execution.unit.f7_restore_owner");
    // SourceUnitFields and MoveSpline have no Deserialize implementation. The
    // enclosing source manager also refuses durable continuation before restore.
    assert_eq!(
        SourceManagers::default()
            .require_durable_checkpoint()
            .unwrap_err()
            .id,
        "execution.managers.f7_continuation_owner"
    );
}

#[test]
fn generic_timer_constructor_and_all_reset_consume_exact_mt_words() {
    use super::creature_tests::{MissingRandom, Random};
    use benilla_sim::source_rng::EventKind;
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    units
        .register(&objects, unit, SourceUnitFields::creature(&template()))
        .unwrap();
    let mut random = Random::new();
    let mut ai = SourceCreatureAi::construct(
        &objects,
        unit,
        &mut units,
        &template(),
        false,
        0,
        &config(),
        &mut random,
    )
    .unwrap();
    assert_eq!(random.cursor.raw, 2);
    ai.base.generic_timers[0].remaining = 37;
    ai.base.generic_timers[0].disabled = false;
    ai.reset_timers(false, &mut MissingRandom).unwrap();
    assert_eq!(ai.base.generic_timers[0].remaining, 37);
    assert!(!ai.base.generic_timers[0].disabled);
    ai.reset_timers(true, &mut random).unwrap();
    assert_eq!(random.cursor.raw, 4);
    assert_eq!(
        ai.base
            .generic_timers
            .map(|t| (t.id, t.remaining, t.disabled)),
        [(2000, 0, true), (2001, 0, true)]
    );
    let raw: Vec<_> = random
        .events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Raw { position, word, .. } => Some((position, word)),
            _ => None,
        })
        .collect();
    assert_eq!(
        raw,
        [
            (0, 3499211612),
            (1, 581869302),
            (2, 3890346734),
            (3, 3586334585)
        ]
    );
    assert_eq!(
        SourceUnitAi::construct(&objects, unit, 0, &mut MissingRandom)
            .unwrap_err()
            .id,
        "execution.timer_ai.thread_rng_owner"
    );
    ai.base.generic_timers[0].remaining = 99;
    ai.base.generic_timers[0].disabled = false;
    assert_eq!(
        ai.reset_timers(true, &mut MissingRandom).unwrap_err().id,
        "execution.timer_ai.thread_rng_owner"
    );
    assert_eq!(ai.base.generic_timers[0].remaining, 99);
    assert!(!ai.base.generic_timers[0].disabled);
    struct OneDraw(bool);
    impl CreatureRandom for OneDraw {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("unexpected signed timer draw")
        }
        fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
            assert_eq!((min, max), (0, 0));
            if std::mem::replace(&mut self.0, true) {
                Err(refuse("execution.timer_ai.thread_rng_owner"))
            } else {
                Ok(0)
            }
        }
    }
    ai.base.generic_timers[1].remaining = 41;
    ai.base.generic_timers[1].disabled = false;
    assert_eq!(
        ai.reset_timers(true, &mut OneDraw(false)).unwrap_err().id,
        "execution.timer_ai.thread_rng_owner"
    );
    assert_eq!(
        ai.base.generic_timers.map(|t| (t.remaining, t.disabled)),
        [(0, true), (41, false)]
    );
}

#[test]
fn spell_list_changed_clears_main_state_before_disabled_or_empty_returns() {
    for (disabled, combat) in [
        (Some(true), true),
        (Some(false), false),
        (Some(false), true),
        (None, false),
        (None, true),
    ] {
        let (objects, mut units, base) = setup(0);
        let mut owner = Authored::default();
        owner.words.extend([0, 0]);
        let ai = SourceCreatureAi::construct(
            &objects,
            base.unit,
            &mut units,
            &template(),
            false,
            0,
            &config(),
            &mut owner,
        )
        .unwrap();
        let mut ais = SourceCreatureAis::default();
        ais.install(&objects, &mut units, ai, &mut SourceCombat::default())
            .unwrap();
        if combat {
            units.get_mut(&objects, base.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
        }
        let mut state = super::event_ai_dispatch_tests::state();
        state.main_spell_id = 7;
        state.main_spells.insert(7);
        let mut threats = SourceThreats::default();
        let mut execution = SourceAiExecution {
            queries: SourceUnitQueries {
                objects: &objects,
                units: &mut units,
            },
            ais: &mut ais,
            threats: &mut threats,
            owner: &mut owner,
        };
        let base = &mut execution.ais.get_mut(&objects, base.unit).unwrap().base;
        base.main_spell_id = 7;
        base.main_spell_cost = 99;
        base.main_spell_min_range = 4.;
        base.main_spell_info = Some(7);
        base.main_attack_mask = 2;
        let list = disabled.map(|disabled| CreatureSpellList {
            disabled,
            ..CreatureSpellList::default()
        });
        let live = execution
            .queries
            .units
            .get_mut(&objects, state.selected.object())
            .unwrap();
        live.spell_list_disabled = disabled;
        live.spell_list = if disabled == Some(true) { None } else { list };
        let result = execution.spell_list_changed(&mut state);
        if disabled.is_none() && combat {
            assert_eq!(
                result.unwrap_err().id,
                "execution.creature_spell_lists.UnitAI_1165.unwritten_disabled"
            );
        } else {
            result.unwrap();
        }
        assert_eq!(state.main_spell_id, 0);
        assert!(state.main_spells.is_empty());
        let base = &execution
            .ais
            .get(&objects, state.selected.object())
            .unwrap()
            .base;
        assert_eq!(
            (
                base.main_spell_id,
                base.main_spell_cost,
                base.main_attack_mask
            ),
            (0, 0, 0)
        );
        assert_eq!(base.main_spell_min_range, 0.);
        assert_eq!(base.main_spell_info, None);
        assert!(base.spell_list_cooldown);
    }
}

#[test]
fn spell_list_changed_uses_ranged_flag_two_and_keeps_first_spell_on_refusal() {
    for (flags, combat, cooldown) in [
        (0, false, 0),
        (1, false, 0),
        (2, false, 0),
        (0, true, 0),
        (0, true, 5),
    ] {
        let (objects, mut units, base) = setup(0);
        let mut owner = Authored::default();
        owner.words.extend([0, 0]);
        let ai = SourceCreatureAi::construct(
            &objects,
            base.unit,
            &mut units,
            &template(),
            false,
            0,
            &config(),
            &mut owner,
        )
        .unwrap();
        let mut ais = SourceCreatureAis::default();
        ais.install(&objects, &mut units, ai, &mut SourceCombat::default())
            .unwrap();
        if combat {
            units.get_mut(&objects, base.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
            owner.words.push_back(cooldown);
        }
        let (lists, census) = LoadedCreatureSpellLists::load(
            vec![SqlTemplateRow {
                ordinal: 0,
                words: [1, 0, 0, 0, 0, u32::MAX],
                strings: [b"current".to_vec()],
            }],
            vec![SqlTemplateRow {
                ordinal: 0,
                words: [1, 100, 100],
                strings: [b"list".to_vec()],
            }],
            vec![SqlTemplateRow {
                ordinal: 0,
                words: [
                    1,
                    0,
                    10,
                    flags,
                    u32::MAX,
                    1,
                    0,
                    100,
                    0,
                    cooldown,
                    cooldown,
                    0,
                    0,
                ],
                strings: [],
            }],
            &std::collections::BTreeMap::from([(10, 0)]),
        );
        assert_eq!(census.loaded, [1, 1, 1]);
        let mut state = super::event_ai_dispatch_tests::state();
        let mut threats = SourceThreats::default();
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
            .queries
            .units
            .get_mut(&objects, state.selected.object())
            .unwrap()
            .spell_list = Some(lists.lists.into_values().next().unwrap());
        assert_eq!(
            execution.spell_list_changed(&mut state).unwrap_err().id,
            "execution.creature_spell_lists.UnitAI_1165.unwritten_disabled"
        );
        execution
            .queries
            .units
            .get_mut(&objects, state.selected.object())
            .unwrap()
            .spell_list_disabled = Some(false);
        let result = execution.spell_list_changed(&mut state);
        let base = &execution
            .ais
            .get(&objects, state.selected.object())
            .unwrap()
            .base;
        if flags == 2 {
            assert_eq!(
                result.unwrap_err().id,
                "execution.event_ai.f5_07_main_spell_state_owner"
            );
            assert_eq!((base.main_spell_id, state.main_spell_id), (10, 10));
            assert!(base.spell_list_cooldown);
        } else if combat && cooldown != 0 {
            assert_eq!(
                result.unwrap_err().id,
                "execution.spells.live_SpellStart_context_owner"
            );
            assert_eq!((base.main_spell_id, state.main_spell_id), (0, 0));
            assert!(!base.spell_list_cooldown);
        } else {
            result.unwrap();
            assert_eq!((base.main_spell_id, state.main_spell_id), (0, 0));
            assert!(!base.spell_list_cooldown);
        }
        assert!(state.main_spells.is_empty());
        assert!(execution.owner.words.is_empty());
    }
}

#[test]
fn empty_unwritten_spell_list_combat_branches_agree() {
    use super::unit_ai::spell_list_combat_predicate;
    for disabled in [Some(true), Some(false), None] {
        assert!(!spell_list_combat_predicate(disabled, false).unwrap());
    }
    assert!(!spell_list_combat_predicate(Some(true), true).unwrap());
    assert!(spell_list_combat_predicate(Some(false), true).unwrap());
    assert_eq!(
        spell_list_combat_predicate(None, true).unwrap_err().id,
        "execution.creature_spell_lists.UnitAI_106.unwritten_disabled"
    );
}

#[test]
fn aura_storage_is_fresh_per_unit_and_excluded_from_saved_fields() {
    let mut first = SourceUnitFields::creature(&template());
    let second = SourceUnitFields::creature(&template());
    let saved = serde_json::to_vec(&first).unwrap();
    assert_eq!(first.auras, SourceUnitAuras::default());
    let spell = SpellTemplate {
        id: 9036,
        attributes: 0x2d800140,
        attributesex3: 0x00100000,
        ..Default::default()
    };
    let unit = ObjectHandle {
        thread: 6,
        construction: 1,
    };
    let mut holder = authored_effect_holder(&spell, unit, 95);
    holder.apply_time = 1794314190;
    let mut other = holder.clone();
    other.caster.construction = 2;
    first
        .auras
        .store_aura_holder(9036, &mut Some(holder))
        .unwrap();
    first
        .auras
        .store_aura_holder(9036, &mut Some(other))
        .unwrap();
    assert_eq!(first.auras.aura_holders().len(), 2);
    assert_eq!(first.auras.auras_of_type(95).len(), 2);
    first.auras.vis_flags = 1;
    assert!(first.auras.has_aura(9036) && first.auras.has_aura_type(95));
    assert_eq!(second.auras, SourceUnitAuras::default());
    assert_eq!(serde_json::to_vec(&first).unwrap(), saved);
}

#[test]
fn construction_population_keeps_live_containers_reach_and_speed() {
    let mut fields = SourceUnitFields::constructed_creature();
    assert_eq!(fields.combat_reach, None);
    assert_eq!(fields.cast_speed, 0);
    assert!(!fields.auras.has_aura_holders());
    assert!(fields.spell_starts.spells.is_empty());
    assert!(fields.cooldowns.is_empty());
    let saved = serde_json::to_vec(&fields).unwrap();
    let unit = ObjectHandle {
        thread: 6,
        construction: 1,
    };
    let spell = SpellTemplate {
        id: 9036,
        ..Default::default()
    };
    fields.combat_reach = Some(1.5f32.to_bits());
    fields.cast_speed = 1f32.to_bits();
    fields.cooldowns.lockouts.insert(1, 20);
    let mut pending = SourceSpell::new(7, &spell, unit, SpellTriggerFlags(1), 0).unwrap();
    pending.pending_aura = Some(SourceAuraHolder::new(&spell, unit, 0, 0, -1).unwrap());
    fields.spell_starts.spells.insert(7, pending);
    fields.unit_events.add_spell(7).unwrap();
    fields
        .auras
        .store_aura_holder(
            9036,
            &mut Some(SourceAuraHolder::new(&spell, unit, 0, 0, -1).unwrap()),
        )
        .unwrap();
    assert_eq!(serde_json::to_vec(&fields).unwrap(), saved);
    let mut template = SqlTemplateRow {
        ordinal: 0,
        words: [0; 99],
        strings: std::array::from_fn(|_| Vec::new()),
    };
    template.words[0] = 6491;
    fields.populate_loaded_creature(&template);
    assert_eq!(fields.entry, 6491);
    assert_eq!(fields.combat_reach, Some(1.5f32.to_bits()));
    assert_eq!(fields.cast_speed, 1f32.to_bits());
    assert_eq!(fields.cooldowns.lockouts[&1], 20);
    assert!(fields.spell_starts.spells.contains_key(&7));
    assert!(fields.spell_starts.spells[&7].pending_aura.is_some());
    assert_eq!(fields.unit_events.inventory().queued.len(), 1);
    assert!(fields.auras.has_aura(9036));
}
