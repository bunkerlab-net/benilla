use super::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;

pub(super) fn state() -> SourceEventAi {
    let mut objects = SourceObjects::default();
    let object = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut registry = AiRegistryBuilder::builtin();
    registry.load_scripts(Vec::new()).unwrap();
    let selected = registry
        .freeze()
        .select(
            &objects,
            object,
            CreatureAiInputs {
                effective_entry: 901,
                ai_name: "EventAI".into(),
                script_name: String::new(),
                control: CreatureAiControl::default(),
                guard: false,
                no_aggro_on_sight: false,
                neutral_to_all: false,
            },
        )
        .unwrap();
    let mut state = SourceEventAi::construct(
        selected,
        &objects,
        Arc::new(EventAiDefinitions::default()),
        EventAiBuild::Release,
    )
    .unwrap();
    state.holders.push(EventAiHolder {
        event: EventAiDefinition {
            ordinal: 1,
            id: 1,
            entry: 901,
            guid: 0,
            kind: 29,
            inverse_phase: 0,
            chance: 10,
            flags: 1,
            params: [0, 0, 77, 77, 0, 0],
            actions: std::array::from_fn(|_| EventAiAction {
                kind: 0,
                params: None,
            }),
            friendly_hp_target_self: None,
        },
        timer: 0,
        enabled: true,
        in_progress: false,
        event_target: None,
    });
    state
}
#[derive(Default)]
struct Host {
    words: VecDeque<u32>,
    draws: Vec<(u32, u32)>,
    units: BTreeMap<ObjectHandle, EventAiUnit>,
    combat_checks: VecDeque<bool>,
    ranged: bool,
    outputs: Vec<String>,
    listeners: usize,
    delivered: usize,
    nested: bool,
    removed: bool,
    flags: u32,
    group: u32,
    friendly: Option<ObjectHandle>,
    stacks: Option<u32>,
    query_result: bool,
}
impl CreatureRandom for Host {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected signed draw")
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        self.draws.push((min, max));
        let value = self.words.pop_front().expect("unrecorded draw");
        assert!((min..=max).contains(&value));
        Ok(value)
    }
    fn creature_urand_default(&mut self) -> Result<u32> {
        self.creature_urand(0, u32::MAX)
    }
}
impl EventAiExecutionOwner for Host {
    fn current_ranged_mode(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.ranged)
    }
    fn unit(&mut self, unit: ObjectHandle) -> Result<EventAiUnit> {
        self.units
            .get(&unit)
            .copied()
            .ok_or_else(|| refuse("authored.unit_absent"))
    }
    fn can_execute_combat_action(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self
            .combat_checks
            .pop_front()
            .expect("unrecorded combat check"))
    }
    fn display_text(
        &mut self,
        _: ObjectHandle,
        text: i32,
        target: Option<ObjectHandle>,
    ) -> Result<()> {
        self.outputs.push(format!("text:{text}:{target:?}"));
        self.delivered += self.listeners;
        Ok(())
    }
    fn emote(&mut self, _: ObjectHandle, emote: u32) -> Result<()> {
        self.outputs.push(format!("emote:{emote}"));
        Ok(())
    }
    fn sound(&mut self, _: ObjectHandle, sound: u32) -> Result<()> {
        self.outputs.push(format!("sound:{sound}"));
        Ok(())
    }
    fn unit_flag(&mut self, _: ObjectHandle, value: u32, set: bool) -> Result<()> {
        if set {
            self.flags |= value;
        } else {
            self.flags &= !value;
        }
        Ok(())
    }
    fn text_template(&mut self, template: u32) -> Result<i32> {
        assert_eq!(template, 77);
        Ok(0)
    }
    fn send_ai_event(
        &mut self,
        state: &mut SourceEventAi,
        event: u32,
        invoker: ObjectHandle,
        _: u32,
    ) -> Result<()> {
        self.outputs.push(format!("enter:{}", state.depth));
        if self.nested {
            state.receive_ai_event(event, state.selected.object(), Some(invoker), self)?;
        }
        self.outputs.push(format!("leave:{}", state.depth));
        Ok(())
    }
    fn dependent_action(
        &mut self,
        _: &mut SourceEventAi,
        action: &EventAiAction,
        _: EventAiInvocation,
        _: Option<ObjectHandle>,
        _: u32,
    ) -> Result<bool> {
        if action.kind == 41 {
            self.removed = true;
            self.outputs.push("remove".into());
            Ok(true)
        } else {
            Err(refuse(event_ai_action_owner(action.kind)))
        }
    }
    fn condition(&mut self, _: u32, _: ObjectHandle, _: ObjectHandle) -> Result<bool> {
        Ok(self.query_result)
    }
    fn range(&mut self, _: ObjectHandle, _: ObjectHandle, _: u32, _: u32) -> Result<bool> {
        Ok(self.query_result)
    }
    fn friendly(
        &mut self,
        _: ObjectHandle,
        query: EventAiFriendlyQuery,
    ) -> Result<Option<ObjectHandle>> {
        self.outputs.push(format!("{query:?}"));
        Ok(self.friendly)
    }
    fn creature_group(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(self.group)
    }
    fn aura_stacks(&mut self, _: ObjectHandle, _: u32) -> Result<Option<u32>> {
        Ok(self.stacks)
    }
    fn attacking_target(
        &mut self,
        _: ObjectHandle,
        _: u32,
        _: u32,
    ) -> Result<Option<ObjectHandle>> {
        Ok(self.friendly)
    }
    fn facing(&mut self, _: ObjectHandle, _: ObjectHandle, _: bool) -> Result<bool> {
        Ok(self.query_result)
    }
    fn combat_movement(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(true)
    }
    fn reachable(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(self.query_result)
    }
    fn spawn_map(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(7)
    }
    fn zone_area(&mut self, _: ObjectHandle) -> Result<[u32; 2]> {
        Ok([8, 9])
    }
}
#[test]
fn chance_failure_resets_timer_before_return_and_consumes_one_raw_word() {
    let mut state = state();
    let mut host = Host {
        words: VecDeque::from([10]),
        ..Host::default()
    };
    assert!(state
        .ready_event(0, EventAiInvocation::default(), &mut host)
        .unwrap());
    state
        .dispatch_ready(EventAiInvocation::default(), &mut host)
        .unwrap();
    assert_eq!(state.holders[0].timer, 77);
    assert!(!state.holders[0].in_progress);
    assert_eq!(host.draws, [(0, u32::MAX)]);
    assert_eq!(state.depth, 0);
    assert!(state.staged[0].is_empty());
}

fn action(kind: u16, p: [u32; 3]) -> EventAiAction {
    EventAiAction {
        kind,
        params: Some(p),
    }
}
fn unit() -> EventAiUnit {
    EventAiUnit {
        kind: SourceObjectKind::Creature,
        entry: 901,
        combat: true,
        evade: false,
        health: 50,
        max_health: 100,
        mana: 30,
        max_mana: 100,
        has_mana: true,
        energy: 80,
        max_energy: 100,
        victim: None,
        owner: None,
        beneficiary: None,
        controlling_player: None,
        spawner: None,
        casting_non_melee: true,
    }
}
fn host(state: &SourceEventAi, words: impl IntoIterator<Item = u32>) -> Host {
    Host {
        words: words.into_iter().collect(),
        units: BTreeMap::from([(state.selected.object(), unit())]),
        ..Host::default()
    }
}
fn run(state: &mut SourceEventAi, host: &mut Host) -> Result<()> {
    state.ready_event(0, EventAiInvocation::default(), host)?;
    state.dispatch_ready(EventAiInvocation::default(), host)
}
#[test]
fn normal_three_slots_share_second_word_and_random_selection_redraws_third() {
    // CreatureEventAI.cpp:618-654. Invented words 99 and 4 give different modulo results.
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.actions = [
        action(9, [10, 11, 12]),
        action(10, [20, 21, 22]),
        action(30, [1, 2, 3]),
    ];
    let mut owner = host(&state, [99, 4]);
    run(&mut state, &mut owner).unwrap();
    assert_eq!(owner.outputs, ["sound:11", "emote:21"]);
    assert_eq!(state.phase, 2);
    assert_eq!(owner.draws, [(0, u32::MAX); 2]);
    state.holders[0].timer = 0;
    state.holders[0].event.flags |= 0x20;
    state.holders[0].event.actions = [
        action(0, [0; 3]),
        action(9, [10, 11, 12]),
        action(10, [20, 21, 22]),
    ];
    let mut owner = host(&state, [99, 1, 5]);
    run(&mut state, &mut owner).unwrap();
    assert_eq!(owner.outputs, ["emote:22"]);
    assert_eq!(owner.draws, [(0, u32::MAX); 3]);
}
#[test]
fn random_event_with_no_actions_still_draws_twice_and_resets() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.flags |= 0x20;
    let mut owner = host(&state, [0, u32::MAX]);
    run(&mut state, &mut owner).unwrap();
    assert_eq!(owner.draws, [(0, u32::MAX); 2]);
    assert_eq!(state.holders[0].timer, 77);
}
#[test]
fn failed_chance_draws_repeat_before_clearing_progress_and_disables_nonrepeatable() {
    let mut state = state();
    state.holders[0].event.flags = 0;
    state.holders[0].event.params[3] = 88;
    let mut owner = host(&state, [10, 83]);
    run(&mut state, &mut owner).unwrap();
    assert_eq!(owner.draws, [(0, u32::MAX), (77, 88)]);
    assert_eq!(state.holders[0].timer, 83);
    assert!(!state.holders[0].enabled);
    assert!(!state.holders[0].in_progress);
}
#[test]
fn combat_action_none_suppresses_later_slots_and_reset_but_not_second_draw() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.flags = 0x400;
    state.holders[0].event.actions[1] = action(22, [7, 0, 0]);
    let mut owner = host(&state, [0, 19]);
    owner.combat_checks = VecDeque::from([true, true]);
    run(&mut state, &mut owner).unwrap();
    assert_eq!(owner.draws.len(), 2);
    assert_eq!(state.phase, 0);
    assert_eq!(state.holders[0].timer, 0);
    assert!(state.holders[0].enabled);
    assert!(!state.holders[0].in_progress);
}
#[test]
fn staged_combat_action_rechecks_before_any_random_word() {
    let mut state = state();
    state.holders[0].event.flags = 0x400;
    let mut owner = host(&state, []);
    owner.combat_checks = VecDeque::from([true, false]);
    run(&mut state, &mut owner).unwrap();
    assert!(owner.draws.is_empty());
    assert_eq!(state.holders[0].timer, 0);
    assert!(!state.holders[0].in_progress);
}
#[test]
fn nested_callback_uses_next_depth_and_inprogress_blocks_reentry() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.actions = [
        action(45, [8, 5, 0]),
        action(5, [11, 0, 0]),
        action(0, [0; 3]),
    ];
    let mut inner = state.holders[0].clone();
    inner.event.id = 2;
    inner.event.kind = 30;
    inner.event.params = [8, 0, 0, 0, 0, 0];
    inner.event.actions = [
        action(22, [7, 0, 0]),
        action(5, [22, 0, 0]),
        action(0, [0; 3]),
    ];
    state.holders.push(inner);
    let mut owner = host(&state, [0, 1, 0, 2]);
    owner.nested = true;
    assert!(state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    assert!(!state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    state
        .dispatch_ready(EventAiInvocation::default(), &mut owner)
        .unwrap();
    assert_eq!(
        owner.outputs,
        ["enter:1", "emote:22", "leave:1", "emote:11"]
    );
    assert_eq!(state.phase, 7);
    assert_eq!(state.depth, 0);
    assert!(state.staged.iter().all(Vec::is_empty));
    assert!(state.holders.iter().all(|h| !h.in_progress));
}
#[test]
fn phase_gates_timer_decrement_but_staged_events_do_not_recheck_phase() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.inverse_phase = 1;
    state.holders[0].timer = 100;
    let mut owner = host(&state, [0, 1]);
    state
        .update_timers(501, &mut EventAiDispatcher(&mut owner))
        .unwrap();
    assert_eq!(state.holders[0].timer, 100);
    assert!(owner.draws.is_empty());
    state.phase = 1;
    state
        .update_timers(501, &mut EventAiDispatcher(&mut owner))
        .unwrap();
    assert_eq!(state.holders[0].timer, 77);
    state.holders[0].timer = 0;
    assert!(state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    state.phase = 0;
    owner.words.extend([0, 1]);
    state
        .dispatch_ready(EventAiInvocation::default(), &mut owner)
        .unwrap();
    assert_eq!(state.holders[0].timer, 77);
}
#[test]
fn action_removal_does_not_suppress_later_staged_actions_or_reset() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.actions = [
        action(41, [0; 3]),
        action(5, [17, 0, 0]),
        action(22, [3, 0, 0]),
    ];
    let mut owner = host(&state, [0, 1]);
    run(&mut state, &mut owner).unwrap();
    assert!(owner.removed);
    assert_eq!(owner.outputs, ["remove", "emote:17"]);
    assert_eq!(state.phase, 3);
    assert_eq!(state.holders[0].timer, 77);
}
#[test]
fn zero_listeners_do_not_suppress_shared_word_text_selection_or_reset() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.actions[0] = action(1, [101, 102, 103]);
    let mut owner = host(&state, [99, 5]);
    run(&mut state, &mut owner).unwrap();
    assert_eq!(owner.outputs, ["text:103:None"]);
    assert_eq!(owner.delivered, 0);
    assert_eq!(owner.draws.len(), 2);
    assert_eq!(state.holders[0].timer, 77);
}
#[test]
fn missing_owner_refuses_before_later_action_and_keeps_inflight_state() {
    let mut state = state();
    state.holders[0].event.chance = 100;
    state.holders[0].event.actions = [
        action(11, [7, 0, 0]),
        action(22, [9, 0, 0]),
        action(5, [17, 0, 0]),
    ];
    let mut owner = host(&state, [0, 1]);
    assert_eq!(
        run(&mut state, &mut owner).unwrap_err().id,
        "execution.event_ai.f5_07_spell_owner"
    );
    assert_eq!(state.phase, 0);
    assert!(owner.outputs.is_empty());
    assert!(state.holders[0].in_progress);
    assert_eq!(state.holders[0].timer, 0);
    assert_eq!(state.depth, 1);
    assert_eq!(state.staged[0], [0]);
}
#[test]
fn invalid_phase_shift_refuses_only_at_reached_check_on_both_sides() {
    // CreatureEventAI.cpp:264,1950. SET_PHASE itself performs uint8 narrowing.
    for (raw, expected) in [(31, 31), (32, 32), (255, 255), (256, 0)] {
        let mut state = state();
        let mut owner = host(&state, []);
        assert!(state
            .execute_action(
                &action(22, [raw, 0, 0]),
                0,
                EventAiInvocation::default(),
                None,
                &mut owner
            )
            .unwrap());
        assert_eq!(state.phase, expected);
        let result = state.ready_event(0, EventAiInvocation::default(), &mut owner);
        if expected < 32 {
            assert!(result.unwrap());
        } else {
            assert_eq!(
                result.unwrap_err().id,
                "execution.event_ai.phase_shift_undefined"
            );
            state.holders[0].enabled = false;
            assert!(!state
                .ready_event(0, EventAiInvocation::default(), &mut owner)
                .unwrap());
            state.holders[0].timer = 1;
            assert_eq!(
                state
                    .update_timers(501, &mut EventAiDispatcher(&mut owner))
                    .unwrap_err()
                    .id,
                "execution.event_ai.phase_shift_undefined"
            );
        }
    }
}
#[test]
fn phase_increment_and_random_range_preserve_clamp_narrowing_and_undefined_boundaries() {
    let mut state = state();
    let mut owner = host(&state, []);
    let call = EventAiInvocation::default();
    for (phase, step, expected) in [(0, -1, 0), (31, 1, 31), (7, -3, 4), (31, i32::MAX - 31, 31)] {
        state.phase = phase;
        state
            .execute_action(&action(23, [step as u32, 0, 0]), 0, call, None, &mut owner)
            .unwrap();
        assert_eq!(state.phase, expected);
    }
    state.phase = 31;
    assert_eq!(
        state
            .execute_action(
                &action(23, [i32::MAX as u32 - 30, 0, 0]),
                0,
                call,
                None,
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.event_ai.phase_add_overflow_undefined"
    );
    state
        .execute_action(
            &action(31, [0, u32::MAX - 1, 0]),
            u32::MAX,
            call,
            None,
            &mut owner,
        )
        .unwrap();
    assert_eq!(state.phase, 0);
    assert_eq!(
        state
            .execute_action(&action(31, [0, u32::MAX, 0]), 1, call, None, &mut owner)
            .unwrap_err()
            .id,
        "execution.event_ai.phase_range_modulo_zero_undefined"
    );
    state.phase = 5;
    for bounds in [[7, 7, 0], [9, 8, 0]] {
        assert!(state
            .execute_action(&action(31, bounds), 99, call, None, &mut owner)
            .unwrap());
        assert_eq!(state.phase, 5);
    }
    state
        .execute_action(&action(30, [31, 32, 256]), 1, call, None, &mut owner)
        .unwrap();
    assert_eq!(state.phase, 32);
    state
        .execute_action(&action(30, [31, 32, 256]), 2, call, None, &mut owner)
        .unwrap();
    assert_eq!(state.phase, 0);
}
#[test]
fn chanced_text_uses_union_chance_gate_and_same_word_for_both_choices() {
    let mut state = state();
    let mut owner = host(&state, []);
    let call = EventAiInvocation::default();
    assert!(!state
        .execute_action(&action(44, [0, 101, 102]), 0, call, None, &mut owner)
        .unwrap());
    assert!(state
        .execute_action(&action(44, [50, 101, 102]), 51, call, None, &mut owner)
        .unwrap());
    assert!(owner.outputs.is_empty());
    state
        .execute_action(&action(44, [50, 101, 102]), 3, call, None, &mut owner)
        .unwrap();
    assert_eq!(owner.outputs, ["text:102:None"]);
    owner.outputs.clear();
    state
        .execute_action(&action(1, [101, 0, 103]), 5, call, None, &mut owner)
        .unwrap();
    assert_eq!(owner.outputs, ["text:101:None"]);
}
#[test]
fn text_targets_preserve_nonplayer_victim_but_not_nonplayer_invoker() {
    let mut state = state();
    let creature = state.selected.object();
    let pet = ObjectHandle {
        thread: 2,
        construction: 1,
    };
    let player = ObjectHandle {
        thread: 3,
        construction: 1,
    };
    let mut owner = host(&state, []);
    owner.units.insert(pet, unit());
    owner.units.insert(
        player,
        EventAiUnit {
            kind: SourceObjectKind::Player,
            ..unit()
        },
    );
    owner.units.get_mut(&creature).unwrap().victim = Some(pet);
    let text = action(1, [101, 0, 0]);
    state
        .execute_action(&text, 0, EventAiInvocation::default(), None, &mut owner)
        .unwrap();
    assert_eq!(
        owner.outputs.pop().unwrap(),
        format!("text:101:Some({pet:?})")
    );
    state
        .execute_action(
            &text,
            0,
            EventAiInvocation {
                invoker: Some(pet),
                sender: None,
            },
            None,
            &mut owner,
        )
        .unwrap();
    assert_eq!(owner.outputs.pop().unwrap(), "text:101:None");
    owner.units.get_mut(&pet).unwrap().owner = Some(player);
    state
        .execute_action(&text, 0, EventAiInvocation::default(), None, &mut owner)
        .unwrap();
    assert_eq!(
        owner.outputs.pop().unwrap(),
        format!("text:101:Some({player:?})")
    );
}
#[test]
fn new_text_target_selection_precedes_template_and_zero_template_result_skips_delivery() {
    let mut state = state();
    let mut owner = host(&state, []);
    let call = EventAiInvocation::default();
    assert!(!state
        .execute_action(&action(54, [101, 15, 77]), 0, call, None, &mut owner)
        .unwrap());
    assert!(state
        .execute_action(&action(54, [101, 0, 77]), 0, call, None, &mut owner)
        .unwrap());
    assert!(owner.outputs.is_empty());
    state
        .execute_action(&action(54, [101, 0, 0]), 0, call, None, &mut owner)
        .unwrap();
    assert_eq!(
        owner.outputs,
        [format!("text:101:Some({:?})", state.selected.object())]
    );
}
#[test]
fn flag_actions_target_real_owner_and_throw_mask_needs_no_unit_owner() {
    let mut state = state();
    let mut owner = host(&state, []);
    for (kind, value) in [(18, 5), (18, 2), (19, 4)] {
        state
            .execute_action(
                &action(kind, [value, 0, 0]),
                0,
                EventAiInvocation::default(),
                None,
                &mut owner,
            )
            .unwrap();
    }
    assert_eq!(owner.flags, 3);
    state
        .execute_action(
            &action(46, [19, 0, 0]),
            0,
            EventAiInvocation::default(),
            None,
            &mut owner,
        )
        .unwrap();
    assert_eq!(state.throw_mask, 19);
    state
        .execute_action(
            &action(45, [8, 0, 0]),
            0,
            EventAiInvocation::default(),
            None,
            &mut owner,
        )
        .unwrap();
    assert!(owner.outputs.is_empty());
}
#[test]
fn negative_random_output_and_unused_switch_results_are_source_false_or_no_output() {
    let mut state = state();
    let mut owner = host(&state, []);
    for kind in [9, 10] {
        assert!(state
            .execute_action(
                &action(kind, [u32::MAX, 7, 8]),
                0,
                EventAiInvocation::default(),
                None,
                &mut owner
            )
            .unwrap());
    }
    for kind in [0, 6, 7, 8, 49] {
        assert!(!state
            .execute_action(
                &action(kind, [0; 3]),
                0,
                EventAiInvocation::default(),
                None,
                &mut owner
            )
            .unwrap());
    }
    assert!(owner.outputs.is_empty());
}
#[test]
fn all_basic_readiness_guards_and_source_unhandled_event_types() {
    for guard in 0..6 {
        let mut state = state();
        let mut owner = host(&state, []);
        match guard {
            0 => state.holders[0].enabled = false,
            1 => state.holders[0].timer = 1,
            2 => state.holders[0].in_progress = true,
            3 => state.holders[0].event.inverse_phase = 1,
            4 => state.holders[0].event.flags = 0x100,
            5 => {
                state.holders[0].event.flags = 0x200;
                owner.ranged = true;
            }
            _ => unreachable!(),
        }
        assert!(!state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap());
        assert!(state.staged.is_empty());
    }
    for kind in [19, 20, 38, 39, 40, 41, 42] {
        let mut state = state();
        state.holders[0].event.kind = kind;
        let mut owner = host(&state, []);
        assert!(!state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap());
    }
}
#[test]
fn health_mana_energy_and_target_predicates_have_inclusive_ranges_and_lazy_zero_guards() {
    for (kind, percent) in [(2, 50), (3, 30), (12, 50), (18, 30), (31, 80)] {
        for (low, high, expected) in [
            (percent, percent, true),
            (percent + 1, 100, false),
            (0, percent - 1, false),
        ] {
            let mut state = state();
            state.holders[0].event.kind = kind;
            state.holders[0].event.params = [high, low, 0, 0, 0, 0];
            let mut owner = host(&state, []);
            owner
                .units
                .get_mut(&state.selected.object())
                .unwrap()
                .victim = Some(state.selected.object());
            assert_eq!(
                state
                    .ready_event(0, EventAiInvocation::default(), &mut owner)
                    .unwrap(),
                expected,
                "kind {kind}"
            );
        }
    }
    let mut state = state();
    state.holders[0].event.kind = 2;
    state.holders[0].event.params = [100, 0, 0, 0, 0, 0];
    let mut owner = host(&state, []);
    owner
        .units
        .get_mut(&state.selected.object())
        .unwrap()
        .max_health = 0;
    assert!(!state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    state.holders[0].event.params[4] = 1;
    assert_eq!(
        state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap_err()
            .id,
        "execution.event_ai.health_divide_zero_undefined"
    );
    owner
        .units
        .get_mut(&state.selected.object())
        .unwrap()
        .max_health = 1;
    owner
        .units
        .get_mut(&state.selected.object())
        .unwrap()
        .health = u32::MAX;
    assert!(!state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
}

#[test]
fn friendly_buff_positional_argument_quirk_and_target_assignment_are_preserved() {
    // CreatureEventAI.cpp:424 passes spawnGroupId as bool self, leaving group zero.
    for (flags, group, in_combat, self_target, spawn_group) in [
        (0, 0, true, true, 0),
        (1, 0, true, false, 0),
        (2, 0, false, true, 0),
        (3, 0, true, false, 0),
        (5, 17, true, true, 0),
        (6, 17, false, true, 17),
    ] {
        let mut state = state();
        state.holders[0].event.kind = 16;
        state.holders[0].event.params = [77, 12, 0, 0, flags, 0];
        let target = ObjectHandle {
            thread: 3,
            construction: 1,
        };
        let mut owner = host(&state, []);
        owner.group = group;
        owner.friendly = Some(target);
        owner
            .units
            .get_mut(&state.selected.object())
            .unwrap()
            .combat = in_combat;
        assert!(state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap());
        assert_eq!(state.holders[0].event_target, Some(target));
        assert_eq!(owner.outputs,[format!("MissingBuff {{ radius: 12, spell: 77, in_combat: {in_combat}, self_target: {self_target}, spawn_group: {spawn_group} }}")]);
    }
}
#[test]
fn predicate_table_covers_combat_summon_aura_casting_geometry_and_conditions() {
    for kind in [
        0, 1, 4, 5, 6, 7, 8, 9, 10, 11, 13, 14, 15, 16, 17, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30,
        32, 33, 34, 35, 36, 37,
    ] {
        let mut state = state();
        state.holders[0].event.kind = kind;
        state.holders[0].event.friendly_hp_target_self = Some(true);
        state.holders[0].event.params = [901, 0, 0, 0, 0, 0];
        if matches!(kind, 5 | 6 | 10 | 22) {
            state.holders[0].event.params = [0; 6];
        }
        let creature = state.selected.object();
        let call = EventAiInvocation {
            invoker: Some(creature),
            sender: Some(creature),
        };
        let mut owner = host(&state, []);
        owner.units.get_mut(&creature).unwrap().victim = Some(creature);
        owner.friendly = Some(creature);
        owner.query_result = kind != 36;
        owner.stacks = if matches!(kind, 27 | 28) {
            None
        } else {
            Some(1)
        };
        if kind == 1 {
            owner.units.get_mut(&creature).unwrap().combat = false;
        }
        assert!(
            state.ready_event(0, call, &mut owner).unwrap(),
            "event {kind}"
        );
    }
    for kind in [0, 1, 3, 9, 12, 13, 14, 15, 18, 24, 28, 31, 33] {
        let mut state = state();
        state.holders[0].event.kind = kind;
        let mut owner = host(&state, []);
        let creature = state.selected.object();
        if kind != 1 {
            owner.units.get_mut(&creature).unwrap().combat = false;
        }
        assert!(
            !state
                .ready_event(0, EventAiInvocation::default(), &mut owner)
                .unwrap(),
            "event {kind}"
        );
    }
}
#[test]
fn conditional_death_and_beneficiary_predicates_skip_absent_subjects_but_refuse_null_dereference() {
    let mut state = state();
    let creature = state.selected.object();
    let mut owner = host(&state, []);
    state.holders[0].event.kind = 6;
    state.holders[0].event.params[0] = 17;
    assert!(!state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    owner.query_result = false;
    let call = EventAiInvocation {
        invoker: Some(creature),
        sender: None,
    };
    assert!(!state.ready_event(0, call, &mut owner).unwrap());
    owner.query_result = true;
    assert!(state.ready_event(0, call, &mut owner).unwrap());
    state.holders[0].in_progress = false;
    state.holders[0].event.kind = 5;
    state.holders[0].event.params[2] = 1;
    assert_eq!(
        state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap_err()
            .id,
        "execution.event_ai.kill_null_invoker_undefined"
    );
    assert!(!state.ready_event(0, call, &mut owner).unwrap());
    owner.units.get_mut(&creature).unwrap().kind = SourceObjectKind::Player;
    assert!(state.ready_event(0, call, &mut owner).unwrap());
    for kind in [10, 22] {
        state.holders[0].in_progress = false;
        state.holders[0].event.kind = kind;
        state.holders[0].event.params = [0, 17, 0, 0, 0, 17];
        assert_eq!(
            state
                .ready_event(0, EventAiInvocation::default(), &mut owner)
                .unwrap_err()
                .id,
            "execution.event_ai.beneficiary_null_invoker_undefined"
        );
        owner.units.get_mut(&creature).unwrap().beneficiary = Some(creature);
        owner.query_result = false;
        assert!(!state.ready_event(0, call, &mut owner).unwrap());
        owner.query_result = true;
        assert!(state.ready_event(0, call, &mut owner).unwrap());
    }
}
#[test]
fn spawned_and_received_ai_event_filters_run_before_readiness() {
    let mut state = state();
    let mut owner = host(&state, [0, 1]);
    for (mode, value, expected) in [
        (0, 999, true),
        (1, 7, true),
        (1, 8, false),
        (2, 8, true),
        (2, 9, true),
        (2, 7, false),
        (3, 0, false),
    ] {
        state.holders[0].event.kind = 11;
        state.holders[0].event.params = [mode, value, 0, 0, 0, 0];
        assert_eq!(
            state.spawned_event_conditions(0, &mut owner).unwrap(),
            expected
        );
    }
    state.holders[0].event.kind = 30;
    state.holders[0].event.chance = 100;
    state.holders[0].event.params = [8, 901, 0, 0, 0, 0];
    let sender = state.selected.object();
    state.receive_ai_event(7, sender, None, &mut owner).unwrap();
    assert!(owner.draws.is_empty());
    state.receive_ai_event(8, sender, None, &mut owner).unwrap();
    assert_eq!(owner.draws.len(), 2);
}
struct Unavailable;
impl CreatureRandom for Unavailable {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected draw")
    }
    fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
        panic!("unexpected draw")
    }
}
impl EventAiExecutionOwner for Unavailable {}
#[test]
fn unavailable_basic_and_dependent_owners_never_report_success() {
    let mut state = state();
    assert_eq!(
        state
            .ready_event(0, EventAiInvocation::default(), &mut Unavailable)
            .unwrap_err()
            .id,
        "execution.event_ai.f5_04_base_ai_owner"
    );
    for (kind, p, expected) in [
        (4, [7, 0, 0], "execution.event_ai.presentation_sound_owner"),
        (5, [7, 0, 0], "execution.event_ai.presentation_emote_owner"),
        (18, [7, 0, 0], "execution.event_ai.f5_04_unit_fields_owner"),
        (54, [7, 0, 0], "execution.event_ai.presentation_text_owner"),
        (
            45,
            [7, 1, 0],
            "execution.event_ai.f5_06_ai_event_visitor_owner",
        ),
        (11, [7, 0, 0], "execution.event_ai.f5_07_spell_owner"),
        (12, [7, 0, 0], "execution.event_ai.f5_15_summon_owner"),
        (13, [7, 0, 0], "execution.threat.f5_04b_unit_fields_owner"),
        (21, [7, 0, 0], "execution.event_ai.f5_05_motion_owner"),
        (15, [7, 0, 0], "execution.event_ai.f5_24_quest_owner"),
    ] {
        assert_eq!(
            state
                .execute_action(
                    &action(kind, p),
                    0,
                    EventAiInvocation::default(),
                    None,
                    &mut Unavailable
                )
                .unwrap_err()
                .id,
            expected,
            "action {kind}"
        );
    }
}

#[derive(Default, Serialize)]
struct DispatchCensus {
    rows: usize,
    dispatch_decided: usize,
    end_to_end: usize,
    first_missing_owner: BTreeMap<String, usize>,
}
impl DispatchCensus {
    fn row(&mut self, decided: bool, missing: &str) {
        self.rows += 1;
        self.dispatch_decided += usize::from(decided);
        *self.first_missing_owner.entry(missing.into()).or_default() += 1;
    }
}
fn basic_action_body(kind: u16) -> bool {
    matches!(kind,0 | 1 | 4..=10 | 18 | 19 | 22 | 23 | 30 | 31 | 44..=46 | 49 | 54)
}
#[test]
fn retained_catalog04_dispatch_census_and_undefined_domains_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP catalog-04 dispatch census: set BENILLA_F5_AI_CATALOG and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    assert_eq!(rows.len(), 11469);
    let config_path = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG")
        .expect("recorded configuration is required, never defaulted");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let mut loader = EventAiCatalog::new(&mut catalog, config);
    let mut events = BTreeMap::<u32, DispatchCensus>::new();
    let mut actions = BTreeMap::<u16, DispatchCensus>::new();
    let mut totals = DispatchCensus::default();
    let mut additional_action_owners = BTreeMap::<String, usize>::new();
    let mut phase_assignments_outside_shift = 0;
    let mut phase_increment_overflow = 0;
    let mut phase_range_zero_width = 0;
    let mut hp_allow_ooc = 0;
    let mut kill_player_only = 0;
    let mut conditional_invoker = 0;
    let mut probe = state();
    for row in rows {
        let event_kind = row.words[2];
        let raw_types = (0..3)
            .map(|s| row.words[12 + 4 * s] as u16)
            .collect::<BTreeSet<_>>();
        let (decided, missing, types) = match load_event_ai_row(&mut loader, row) {
            Err(error) => (false, error.id, raw_types),
            Ok(load) => match load.disposition {
                EventAiDisposition::Skipped { line } => {
                    panic!("unexpected catalog source skip at {line}")
                }
                EventAiDisposition::Loaded(event) => {
                    let decided = event.actions.iter().all(|a| basic_action_body(a.kind));
                    let mut requirements = BTreeSet::new();
                    for a in &event.actions {
                        let p = a.params.unwrap_or([0; 3]);
                        match a.kind {
                            22 => phase_assignments_outside_shift += usize::from(p[0] as u8 >= 32),
                            23 => {
                                phase_increment_overflow +=
                                    usize::from((p[0] as i32) > i32::MAX - 31)
                            }
                            30 => {
                                phase_assignments_outside_shift +=
                                    usize::from(p.iter().any(|v| *v as u8 >= 32))
                            }
                            31 => {
                                phase_range_zero_width += usize::from(
                                    p[1] > p[0] && p[1].wrapping_sub(p[0]).wrapping_add(1) == 0,
                                );
                                phase_assignments_outside_shift +=
                                    usize::from(p[0] >= 32 || p[1] >= 32);
                            }
                            _ => {}
                        }
                        if !matches!(a.kind, 0 | 6..=8 | 22 | 23 | 30 | 31 | 46 | 49) {
                            requirements.insert(event_ai_action_owner(a.kind));
                        }
                    }
                    for owner in requirements {
                        *additional_action_owners.entry(owner.into()).or_default() += 1;
                    }
                    hp_allow_ooc += usize::from(event.kind == 2 && event.params[4] != 0);
                    kill_player_only += usize::from(event.kind == 5 && event.params[2] == 1);
                    conditional_invoker += usize::from(
                        event.kind == 10 && event.params[5] != 0
                            || event.kind == 22 && event.params[1] != 0,
                    );
                    let types = event.actions.iter().map(|a| a.kind).collect();
                    probe.phase = (0..32)
                        .find(|p| event.inverse_phase & (1u32 << p) == 0)
                        .unwrap_or(0) as u8;
                    probe.holders[0].event = event;
                    probe.holders[0].enabled = true;
                    probe.holders[0].timer = 0;
                    probe.holders[0].in_progress = false;
                    let missing = probe
                        .ready_event(0, EventAiInvocation::default(), &mut Unavailable)
                        .unwrap_err()
                        .id;
                    (decided, missing, types)
                }
            },
        };
        totals.row(decided, &missing);
        events.entry(event_kind).or_default().row(decided, &missing);
        // Per-action counts are distinct ROWS containing that type, not slot counts.
        for kind in types {
            actions.entry(kind).or_default().row(decided, &missing);
        }
    }
    assert_eq!(totals.rows, 11469);
    assert_eq!(totals.end_to_end, 0);
    assert_eq!(phase_assignments_outside_shift, 0);
    assert_eq!(phase_increment_overflow, 0);
    assert_eq!(phase_range_zero_width, 0);
    let report = serde_json::json!({
        "catalog_sha256":"757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6",
        "scope":"catalog alternatives, not live F1 dispatch or admission",
        "dispatch_decided_definition":"loader completed and every slot has a ported basic action body, irrespective of unavailable dependencies",
        "end_to_end_definition":"loader and reached live readiness/action owners all available; currently none",
        "totals":totals,"by_event_type":events,"by_action_type":actions,
        "additional_action_missing_owners_nonexclusive_rows":additional_action_owners,
        "undefined_domain_candidates":{
            "phase_assignments_outside_shift":phase_assignments_outside_shift,
            "phase_increment_overflow":phase_increment_overflow,
            "phase_range_modulo_zero":phase_range_zero_width,
            "hp_allow_ooc_rows_requiring_nonzero_max_health":hp_allow_ooc,
            "kill_player_only_rows_requiring_nonnull_invoker":kill_player_only,
            "condition_rows_requiring_nonnull_invoker":conditional_invoker
        },
        "stock_phase_shift_reach":"[INFERENCE] zero: initial/death zero, all stock phase assignments below32, increment clamps without overflow",
        "runtime_zero_health_or_null_invoker_reach":"unavailable from catalog or retained event-ID bindings; authored boundaries refuse at the reached operation",
        "exact_event_action_F1_projection":"unavailable"
    });
    println!(
        "F5_DISPATCH_CENSUS {}",
        serde_json::to_string(&report).unwrap()
    );
}

#[derive(Default)]
struct F1Words(VecDeque<u64>);
impl benilla_sim::source_rng::RawGenerator for F1Words {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        self.0.pop_front().expect("unrecorded F1 word")
    }
}
#[test]
fn retained_f1_chance_shared_and_redraw_words_match_or_explicit_absence() {
    use benilla_sim::source_rng::{DrawCursor, SourceRandom};
    use std::io::{BufRead, BufReader};
    let Some(path) = std::env::var_os("BENILLA_F5_EVENT_AI_TRACE") else {
        println!("SKIP F1 EventAI dispatch-word projection: set BENILLA_F5_EVENT_AI_TRACE; exact event/action-ID projection unavailable");
        return;
    };
    let mut reader = BufReader::with_capacity(1 << 20, std::fs::File::open(path).unwrap());
    let mut line = String::with_capacity(4096);
    let mut hash = Sha256::new();
    let mut pending = BTreeMap::<u64, (u32, F1Words)>::new();
    let mut counts = BTreeMap::<u32, usize>::new();
    let mut words = 0;
    let mut records = 0;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        hash.update(line.as_bytes());
        records += 1;
        let mut fields = line.trim_end_matches('\n').splitn(4, '\t');
        let thread = fields.next().unwrap().parse::<u64>().unwrap();
        let _sequence = fields.next().unwrap();
        let kind = fields.next().unwrap();
        let data = fields.next().unwrap_or("");
        if kind == "rng_call"
            && data.starts_with(
                "urand_default /reference/source/src/game/AI/EventAI/CreatureEventAI.cpp ",
            )
        {
            let source_line = data
                .split_whitespace()
                .nth(2)
                .unwrap()
                .parse::<u32>()
                .unwrap();
            if [607, 618, 653].contains(&source_line) {
                assert!(pending
                    .insert(thread, (source_line, F1Words::default()))
                    .is_none());
            }
        } else if kind == "mt_raw" {
            if let Some((_, raw)) = pending.get_mut(&thread) {
                raw.0
                    .push_back(data.split_whitespace().nth(2).unwrap().parse().unwrap());
            }
        } else if kind == "rng_result" {
            if let Some((site, mut raw)) = pending.remove(&thread) {
                let mut result = data.split_whitespace();
                assert_eq!(result.next(), Some("urand_default"));
                let expected = result.next().unwrap().parse::<u32>().unwrap();
                assert!(result.next().is_none());
                let raw_count = raw.0.len();
                let mut cursor = DrawCursor::default();
                let mut sink = |_| {};
                let mut random =
                    SourceRandom::new(thread, &mut raw, &mut cursor, &mut sink).unwrap();
                assert_eq!(random.urand_default().unwrap(), expected);
                assert_eq!(cursor.raw, raw_count as u64);
                assert!(raw.0.is_empty());
                words += raw_count;
                *counts.entry(site).or_default() += 1;
            }
        }
    }
    assert!(pending.is_empty());
    assert_eq!(records, 73721588);
    assert_eq!(
        format!("{:x}", hash.finalize()),
        "9cfa759e74a34e4768fa011f69365b32b050ede9d35b2a37afbda8df2b7b091a"
    );
    assert_eq!(counts, BTreeMap::from([(607, 181), (618, 181), (653, 1)]));
    assert_eq!(words, 363);
    println!("F1 EventAI words: 181 chance, 181 shared action, 1 random-action redraw, 363 raw words, zero mismatches; helper-only unsealed comparison. Event/action-ID and predicate/recipient projections unavailable.");
}

#[test]
fn all_three_dispatch_sites_record_urand_default_in_source_order() {
    use benilla_sim::source_rng::{DrawCursor, EventKind, Operation, SourceRandom, TraceEvent};
    struct Traced {
        raw: F1Words,
        cursor: DrawCursor,
        trace: Vec<TraceEvent>,
    }
    impl CreatureRandom for Traced {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("unexpected signed draw")
        }
        fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
            panic!("raw sites must not call ranged urand")
        }
        fn creature_urand_default(&mut self) -> Result<u32> {
            SourceRandom::new(7, &mut self.raw, &mut self.cursor, &mut |e| {
                self.trace.push(e)
            })
            .unwrap()
            .creature_urand_default()
        }
    }
    impl EventAiExecutionOwner for Traced {
        fn current_ranged_mode(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(false)
        }
    }
    for (flags, words, phase) in [(1, vec![0u32, 1], 6), (0x21, vec![0u32, 1, 2], 7)] {
        let mut state = state();
        state.holders[0].event.chance = 100;
        state.holders[0].event.flags = flags;
        state.holders[0].event.actions = [
            action(30, [5, 6, 7]),
            action(0, [0; 3]),
            action(30, [5, 6, 7]),
        ];
        let mut owner = Traced {
            raw: F1Words(words.into_iter().map(u64::from).collect()),
            cursor: DrawCursor::default(),
            trace: Vec::new(),
        };
        state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap();
        state
            .dispatch_ready(EventAiInvocation::default(), &mut owner)
            .unwrap();
        assert_eq!(state.phase, phase);
        let operations = owner
            .trace
            .iter()
            .filter_map(|e| match e.kind {
                EventKind::Begin {
                    operation,
                    at,
                    parent: None,
                    ..
                } => Some((operation, at)),
                _ => None,
            })
            .collect::<Vec<_>>();
        let expected = if flags == 1 {
            vec![(Operation::UrandDefault, 0), (Operation::UrandDefault, 1)]
        } else {
            vec![
                (Operation::UrandDefault, 0),
                (Operation::UrandDefault, 1),
                (Operation::UrandDefault, 2),
            ]
        };
        assert_eq!(operations, expected);
    }
    assert_eq!(
        Unavailable.creature_urand_default().unwrap_err().id,
        "execution.creature.urand_default_owner"
    );
}

#[test]
fn friendly_hp_computed_data_is_required_at_the_reached_dereference() {
    // CreatureEventAI.cpp:382 dereferences computed data after the combat guard.
    let mut state = state();
    state.holders[0].event.kind = 14;
    let creature = state.selected.object();
    let mut owner = host(&state, []);
    owner.units.get_mut(&creature).unwrap().combat = false;
    assert!(!state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    owner.units.get_mut(&creature).unwrap().combat = true;
    assert_eq!(
        state
            .ready_event(0, EventAiInvocation::default(), &mut owner)
            .unwrap_err()
            .id,
        "execution.event_ai.friendly_hp_computed_data_undefined"
    );
    state.holders[0].event.friendly_hp_target_self = Some(true);
    owner.friendly = Some(creature);
    assert!(state
        .ready_event(0, EventAiInvocation::default(), &mut owner)
        .unwrap());
    assert_eq!(state.holders[0].event_target, Some(creature));
}
