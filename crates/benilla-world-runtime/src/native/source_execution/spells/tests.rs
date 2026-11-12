use super::*;
use SpellCastResult as R;

fn unit() -> ObjectHandle {
    ObjectHandle {
        thread: 1,
        construction: 1,
    }
}
fn template() -> SpellTemplate {
    SpellTemplate {
        id: 7,
        equippeditemclass: -1,
        ..Default::default()
    }
}
fn spell(t: &SpellTemplate, flags: u32) -> SourceSpell {
    SourceSpell::new(1, t, unit(), SpellTriggerFlags(flags), 10).unwrap()
}
/// Explicit authored Unit, no auras, no cooldown, stationary, not a player/GO.
/// These operands exercise the owner, not a catalog or recorded execution claim.
fn operands() -> SpellUnitState {
    SpellUnitState {
        player: false,
        gameobject: false,
        alive: true,
        standing: true,
        player_controlled: false,
        combat: false,
        battleground_wait_leave: false,
        gm: false,
        moving: false,
        falling_far: false,
        stealth: false,
        form: 0,
        class: 1,
        combo_target: None,
        mounted: false,
        taxi: false,
        totem: false,
        untargetable: false,
        has_ai: false,
        position: [0; 4],
    }
}
struct Fixture {
    unit: SpellUnitState,
    trace: Vec<String>,
    fail: Option<(SpellCheck, R)>,
    time: u32,
    cast_time: i32,
    location: bool,
    resist: bool,
    pointers: bool,
    delayed: u64,
    stop_cast: bool,
}
impl Default for Fixture {
    fn default() -> Self {
        Self {
            unit: operands(),
            trace: Vec::new(),
            fail: None,
            time: 11,
            cast_time: 1000,
            location: true,
            resist: false,
            pointers: true,
            delayed: 0,
            stop_cast: false,
        }
    }
}
impl SpellExecutionOwner for Fixture {
    fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
        Ok(self.unit)
    }
    fn check(&mut self, _: &mut SourceSpell, _: &SpellTemplate, check: SpellCheck) -> Result<R> {
        self.trace.push(format!("{check:?}"));
        Ok(self
            .fail
            .filter(|(c, _)| *c == check)
            .map_or(R::Ok, |(_, r)| r))
    }
    fn call(&mut self, _: &mut SourceSpell, call: SpellCall) -> Result<()> {
        self.trace.push(format!("{call:?}"));
        if self.stop_cast
            && matches!(
                call,
                SpellCall::Cast(_) | SpellCall::FillTargetMap | SpellCall::ExecuteEffects
            )
        {
            Err(call.missing_owner())
        } else {
            Ok(())
        }
    }
    fn zone_area(&mut self, _: ObjectHandle) -> Result<[u32; 2]> {
        self.trace.push("GetZoneAndAreaId".into());
        if self.location {
            Ok([12, 87])
        } else {
            Err(refuse("execution.terrain.live_vmap_area_owner"))
        }
    }
    fn cast_time(&mut self, _: &mut SourceSpell, _: &SpellTemplate) -> Result<i32> {
        self.trace.push("GetSpellCastTime".into());
        Ok(self.cast_time)
    }
    fn map_time(&mut self, _: ObjectHandle) -> Result<u32> {
        Ok(self.time)
    }
    fn add_gcd(&mut self, _: &SourceSpell, _: &SpellTemplate) -> Result<()> {
        self.trace.push("AddGCD".into());
        Ok(())
    }
    fn reset_gcd(&mut self, _: &SourceSpell, _: &SpellTemplate) -> Result<()> {
        self.trace.push("ResetGCD".into());
        Ok(())
    }
    fn set_current(&mut self, s: &mut SourceSpell, _: &SpellTemplate) -> Result<()> {
        s.referenced = true;
        self.trace.push("SetCurrentCastedSpell".into());
        Ok(())
    }
    fn update_pointers(&mut self, _: &mut SourceSpell) -> Result<bool> {
        Ok(self.pointers)
    }
    fn resist_pushback(&mut self, _: &SourceSpell) -> Result<bool> {
        self.trace.push("roll_chance_i".into());
        Ok(self.resist)
    }
    fn indoor_policy(&mut self) -> Result<bool> {
        Ok(false)
    }
    fn handle_delayed(&mut self, s: &mut SourceSpell, elapsed: u64) -> Result<u64> {
        self.trace.push(format!("handle_delayed({elapsed})"));
        if self.delayed == 0 {
            s.state = SourceSpellState::Finished;
        }
        Ok(self.delayed)
    }
    fn reset_auto_repeat(&mut self) -> Result<()> {
        self.trace.push("ResetAutoRepeatSpells".into());
        Ok(())
    }
    fn ai_cast_state(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<(bool, bool)> {
        Ok((false, false))
    }
    fn interrupt_for_ai(&mut self, _: ObjectHandle, melee: bool) -> Result<()> {
        self.trace.push(format!("Interrupt({melee})"));
        Ok(())
    }
    fn casting_source(&mut self, _: &SourceSpell) -> Result<Option<[u32; 3]>> {
        Ok(Some([0; 3]))
    }
    fn targeting_data(&mut self, _: u32) -> Result<[[u32; 2]; 3]> {
        Ok([[0; 2]; 3])
    }
}
#[test]
fn failed_check_retains_event_before_finish_without_preparation() {
    // Spell.cpp:3007-3016,3725-3742.
    let t = template();
    let mut s = spell(&t, 0);
    let mut q = SourceUnitEvents::default();
    let mut f = Fixture {
        fail: Some((SpellCheck::IsSpellReady, R::NotReady)),
        ..Default::default()
    };
    assert_eq!(
        s.start(&t, SpellTargets::default(), None, &mut q, &mut f)
            .unwrap(),
        R::NotReady
    );
    assert_eq!(s.state, SourceSpellState::Finished);
    assert_eq!(f.trace, ["IsSpellReady", "SendResult(NotReady)"]);
    let event = &q.inventory().queued[0];
    assert_eq!(event.kind, SourceUnitEvent::Spell { spell: 1 });
    assert_eq!(event.queue.unwrap().deadline_ms, 1);
    assert_eq!(s.event, Some(event.event_id));
}
#[test]
fn unavailable_query_keeps_targeting_object_and_queued_event() {
    struct Unbound;
    impl SpellExecutionOwner for Unbound {}
    let t = template();
    let mut s = spell(&t, 0);
    let mut q = SourceUnitEvents::default();
    assert_eq!(
        s.start(&t, SpellTargets::default(), None, &mut q, &mut Unbound)
            .unwrap_err(),
        SpellCheck::IsSpellReady.missing_owner()
    );
    assert_eq!(s.state, SourceSpellState::Targeting);
    assert_eq!(q.inventory().queued.len(), 1);
}
#[test]
fn triggered_bits_are_independent_including_force_costs() {
    // Spell.cpp:432,458-473. OLD_TRIGGERED never implies INSTANT_CAST.
    let t = template();
    let old = SpellTriggerFlags(1);
    assert!(!old.instant());
    assert!(old.ignore_gcd());
    assert!(old.ignore_costs());
    assert!(!SpellTriggerFlags(8).old_triggered());
    assert!(!SpellTriggerFlags(0x20001).ignore_costs());
    assert!(SpellTriggerFlags(0x20401).ignore_costs());
    for bit in 0..18 {
        let f = SpellTriggerFlags(1 << bit);
        assert_eq!(f.old_triggered(), bit == 0);
        assert_eq!(f.ignore_hit(), bit == 1);
        assert_eq!(f.ignore_unselectable(), bit == 0 || bit == 2);
        assert_eq!(f.instant(), bit == 3);
        assert_eq!(f.autorepeat(), bit == 4);
        assert_eq!(f.ignore_unattackable(), bit == 5);
        assert_eq!(f.do_not_proc(), bit == 6);
        assert_eq!(f.pet_cast(), bit == 7);
        assert_eq!(f.notify_ai(), bit == 8);
        assert_eq!(f.ignore_gcd(), bit == 0 || bit == 9);
        assert_eq!(f.ignore_costs(), bit == 0 || bit == 10);
        assert_eq!(f.ignore_cooldowns(), bit == 0 || bit == 11);
        assert_eq!(f.ignore_current(&t), bit == 0 || bit == 12);
        assert_eq!(f.hide_log(false), bit == 0 || bit == 13);
        assert_eq!(f.reset_leash(), bit != 14);
        assert_eq!(f.channel_only(), bit == 15);
        assert_eq!(f.ignore_caster_aura(), bit == 0 || bit == 16);
    }
    assert!(!old.hide_log(true));
}
#[test]
fn strict_and_non_strict_checks_preserve_order() {
    // Spell.cpp:4678-4690,4724,5098,5145,5153.
    let t = template();
    let mut s = spell(&t, 0);
    let mut f = Fixture::default();
    assert_eq!(s.check_cast(&t, true, &mut f).unwrap(), R::Ok);
    assert_eq!(
        f.trace,
        [
            "IsSpellReady",
            "HasGcd",
            "Shapeshift",
            "GetZoneAndAreaId",
            "AllowedInLocation([12, 87])",
            "Items",
            "Range(true)",
            "Power(true)",
            "CasterAuras"
        ]
    );
    f.trace.clear();
    s.cast_time = 1000;
    assert_eq!(s.check_cast(&t, false, &mut f).unwrap(), R::Ok);
    assert_eq!(
        f.trace,
        [
            "IsSpellReady",
            "GetZoneAndAreaId",
            "AllowedInLocation([12, 87])",
            "Items",
            "Range(false)",
            "Power(false)",
            "CasterAuras"
        ]
    );
    f.trace.clear();
    f.fail = Some((SpellCheck::HasGcd, R::NotReady));
    assert_eq!(s.check_cast(&t, true, &mut f).unwrap(), R::NotReady);
    assert_eq!(f.trace, ["IsSpellReady", "HasGcd"]);
}
#[test]
fn triggered_and_passive_still_reach_location_without_fabricated_geometry() {
    // Spell.cpp:5098 is unconditional. FORCE_COSTS still cannot enter :5151.
    for flags in [1, 0x20001, 9] {
        let t = template();
        let mut s = spell(&t, flags);
        let mut f = Fixture {
            location: false,
            ..Default::default()
        };
        assert_eq!(
            s.check_cast(&t, true, &mut f).unwrap_err().id,
            "execution.terrain.live_vmap_area_owner"
        );
        assert_eq!(f.trace, ["GetZoneAndAreaId"]);
    }
    let t = SpellTemplate {
        attributes: 0x40,
        ..template()
    };
    let mut s = spell(&t, 0);
    let mut f = Fixture {
        location: false,
        ..Default::default()
    };
    assert!(s.check_cast(&t, true, &mut f).is_err());
    assert_eq!(f.trace, ["HasGcd", "Shapeshift", "GetZoneAndAreaId"]);
}
#[test]
fn aura_trigger_cooldown_failure_and_no_combat_guard_are_distinct() {
    let t = SpellTemplate {
        attributes: 0x10000000,
        ..template()
    };
    let mut s = spell(&t, 0);
    s.triggered_by_aura = Some(2);
    let mut f = Fixture {
        fail: Some((SpellCheck::IsSpellReady, R::NotReady)),
        ..Default::default()
    };
    assert_eq!(s.check_cast(&t, true, &mut f).unwrap(), R::DontReport);
    s.flags = SpellTriggerFlags(1);
    f.unit.combat = true;
    f.fail = None;
    assert_eq!(s.check_cast(&t, true, &mut f).unwrap(), R::AffectingCombat);
    s.triggered_by_aura = None;
    assert_eq!(s.check_cast(&t, true, &mut f).unwrap(), R::Ok);
}
#[test]
fn old_triggered_cast_time_is_unreferenced_while_instant_normal_cast_is_current() {
    let t = template();
    let mut s = spell(&t, 1);
    let mut q = SourceUnitEvents::default();
    let mut f = Fixture::default();
    assert_eq!(
        s.start(&t, SpellTargets::default(), None, &mut q, &mut f)
            .unwrap(),
        R::Ok
    );
    assert_eq!(s.timer, 1000);
    assert!(!s.referenced);
    assert_eq!(
        f.trace,
        [
            "GetZoneAndAreaId",
            "AllowedInLocation([12, 87])",
            "Items",
            "PrepareTriggerData",
            "GetSpellCastTime"
        ]
    );
    let mut s = spell(&t, 8);
    let mut q = SourceUnitEvents::default();
    f.trace.clear();
    s.start(&t, SpellTargets::default(), None, &mut q, &mut f)
        .unwrap();
    assert_eq!(s.timer, 0);
    assert!(s.referenced);
    assert!(f.trace.ends_with(&[
        "SetCurrentCastedSpell".into(),
        "AddGCD".into(),
        "SendStart".into(),
        "Cast(false)".into()
    ]));
    assert!(!f.trace.iter().any(|v| v == "GetSpellCastTime"));
}
#[test]
fn autorepeat_dummy_suppresses_failed_check_but_internal_autorepeat_does_not() {
    // Spell.cpp:2984,3083.
    let t = SpellTemplate {
        attributes: 2,
        attributesex2: 0x20,
        ..template()
    };
    for (flags, expected) in [(0, R::Ok), (0x10, R::NotReady)] {
        let mut s = spell(&t, flags);
        let mut q = SourceUnitEvents::default();
        let mut f = Fixture {
            fail: Some((SpellCheck::IsSpellReady, R::NotReady)),
            ..Default::default()
        };
        assert_eq!(
            s.start(&t, SpellTargets::default(), None, &mut q, &mut f)
                .unwrap(),
            expected
        );
        assert_eq!(
            s.state,
            if flags == 0 {
                SourceSpellState::Casting
            } else {
                SourceSpellState::Finished
            }
        );
    }
}
#[test]
fn start_stops_at_named_cast_owner_without_entering_target_or_effect_owners() {
    let t = template();
    let mut s = spell(&t, 9);
    let mut q = SourceUnitEvents::default();
    let mut f = Fixture {
        stop_cast: true,
        ..Default::default()
    };
    assert_eq!(
        s.start(&t, SpellTargets::default(), None, &mut q, &mut f)
            .unwrap_err()
            .id,
        "execution.spells.f5_10_Spell_cast_owner"
    );
    assert_eq!(s.state, SourceSpellState::Casting);
    assert_eq!(q.inventory().queued.len(), 1);
    assert_eq!(
        SpellCall::FillTargetMap.missing_owner().id,
        "execution.spells.f5_09_Spell_FillTargetMap_owner"
    );
    assert_eq!(
        SpellCall::ExecuteEffects.missing_owner().id,
        "execution.spells.f5_11_Spell_ExecuteEffects_owner"
    );
}
#[test]
fn gcd_uses_emplace_and_cancel_resets_only_prelaunch_states() {
    // Object.cpp:2605 is emplace, not assign. Spell.cpp:3116-3123.
    let t = SpellTemplate {
        startrecoverycategory: 7,
        ..template()
    };
    let mut gcd = SourceSpellGcd::default();
    gcd.add(&t, 100, 50).unwrap();
    gcd.add(&t, 110, 500).unwrap();
    assert!(gcd.has(7, 149));
    assert!(!gcd.has(7, 150));
    gcd.reset(7);
    assert!(!gcd.has(7, 101));
    gcd.add(&t, i64::MAX - 1, 1).unwrap();
    assert_eq!(
        gcd.add(&t, i64::MAX, 1).unwrap_err().id,
        "execution.spells.gcd_clock_overflow_undefined"
    );
    for state in [
        SourceSpellState::Created,
        SourceSpellState::Targeting,
        SourceSpellState::Casting,
        SourceSpellState::Traveling,
        SourceSpellState::Finished,
    ] {
        let mut s = spell(&t, 0);
        s.state = state;
        let mut f = Fixture::default();
        s.cancel(&t, &mut f).unwrap();
        assert_eq!(
            f.trace.contains(&"ResetGCD".into()),
            matches!(
                state,
                SourceSpellState::Created | SourceSpellState::Targeting | SourceSpellState::Casting
            )
        );
        assert_eq!(s.state, SourceSpellState::Finished);
        if state == SourceSpellState::Finished {
            assert!(f.trace.is_empty());
        }
    }
}
fn stores() -> SourceSpellStores {
    let mut s = SourceSpellStores::default();
    for t in [
        template(),
        SpellTemplate {
            id: 8,
            attributesex: 4,
            ..template()
        },
        SpellTemplate {
            id: 9,
            attributes: 2,
            attributesex2: 0x20,
            ..template()
        },
    ] {
        s.insert_template(t).unwrap();
    }
    s
}
#[test]
fn flying_spell_survives_slot_replacement_until_its_event_deletes_it() {
    // Unit.cpp:4225-4226,4293-4299; Spell.h:593,645.
    let stores = stores();
    let t = stores.template(7).unwrap();
    let mut first = spell(t, 0);
    first.state = SourceSpellState::Traveling;
    let mut second = spell(t, 0);
    second.id = 2;
    let mut spells = BTreeMap::from([(1, first), (2, second)]);
    let mut slots = SourceSpellSlots::default();
    let mut f = Fixture::default();
    slots.set(1, &mut spells, &stores, &mut f).unwrap();
    slots.set(2, &mut spells, &stores, &mut f).unwrap();
    assert_eq!(slots.current(SpellSlot::Generic), Some(2));
    assert_eq!(spells[&1].state, SourceSpellState::Traveling);
    assert!(!spells[&1].referenced);
    assert!(spells[&1].deletable());
    assert!(f.trace.is_empty());
    spells.get_mut(&2).unwrap().state = SourceSpellState::Finished;
    assert!(!spells[&2].deletable());
    slots.clear_finished(&mut spells).unwrap();
    assert!(spells[&2].deletable());
}
#[test]
fn channel_generic_breakage_preserves_source_autorepeat_slot_quirk() {
    // Unit.cpp:4259 checks AUTOREPEAT before interrupting GENERIC.
    let stores = stores();
    let mut spells = BTreeMap::new();
    for (id, spell_id) in [(1, 7), (2, 8), (3, 9), (4, 8)] {
        let mut s = spell(stores.template(spell_id).unwrap(), 0);
        s.id = id;
        s.state = SourceSpellState::Casting;
        spells.insert(id, s);
    }
    let mut slots = SourceSpellSlots::default();
    let mut f = Fixture::default();
    slots.set(1, &mut spells, &stores, &mut f).unwrap();
    slots.set(2, &mut spells, &stores, &mut f).unwrap();
    assert_eq!(spells[&1].state, SourceSpellState::Casting);
    slots.set(3, &mut spells, &stores, &mut f).unwrap();
    slots.set(4, &mut spells, &stores, &mut f).unwrap();
    assert_eq!(spells[&1].state, SourceSpellState::Finished);
    assert_eq!(spells[&2].state, SourceSpellState::Finished);
}
#[test]
fn pushback_caps_counts_and_consumes_only_reached_resistance_rolls() {
    let t = SpellTemplate {
        interruptflags: 2,
        ..template()
    };
    let mut s = spell(&t, 0);
    s.cast_time = 5000;
    s.state = SourceSpellState::Casting;
    let mut f = Fixture::default();
    f.unit.player = true;
    for expected in [1000, 800, 600, 400, 200, 200] {
        s.timer = 0;
        assert_eq!(s.pushback(&t, false, &mut f).unwrap(), expected);
    }
    assert_eq!(s.damage_delay_count, 5);
    s.timer = 4990;
    assert_eq!(s.pushback(&t, false, &mut f).unwrap(), 10);
    assert_eq!(s.timer, 5000);
    let count = f.trace.len();
    s.state = SourceSpellState::Traveling;
    assert_eq!(s.pushback(&t, false, &mut f).unwrap(), 0);
    assert_eq!(f.trace.len(), count);
    s.state = SourceSpellState::Channeling;
    s.timer = 50;
    assert_eq!(s.pushback(&t, true, &mut f).unwrap(), 50);
    assert_eq!(s.timer, 0);
    f.resist = true;
    let count = s.damage_delay_count;
    assert_eq!(s.pushback(&t, true, &mut f).unwrap(), 0);
    assert_eq!(s.damage_delay_count, count);
}
#[test]
fn pushback_signed_add_undefined_is_not_wrapped() {
    let t = SpellTemplate {
        interruptflags: 2,
        ..template()
    };
    let mut s = spell(&t, 0);
    let mut f = Fixture::default();
    f.unit.player = true;
    s.cast_time = i32::MAX;
    s.timer = i32::MAX as u32 - 1000;
    assert_eq!(s.pushback(&t, false, &mut f).unwrap(), 1000);
    s.damage_delay_count = 0;
    s.timer = i32::MAX as u32 - 999;
    assert_eq!(
        s.pushback(&t, false, &mut f).unwrap_err().id,
        "execution.spells.pushback_signed_add_undefined"
    );
}
#[test]
fn target_table_bounds_and_short_circuit_preserve_both_sides() {
    assert!(!check_target(63).unwrap());
    assert_eq!(
        check_target(64).unwrap_err().id,
        "execution.spells.SpellTargetInfoTable_index_undefined"
    );
    let mut t = SpellTemplate {
        effectimplicittargeta1: 6,
        effectimplicittargetb1: 64,
        ..template()
    };
    assert_eq!(check_effect_mask(&t).unwrap(), 1);
    t.effectimplicittargeta1 = 63;
    assert!(check_effect_mask(&t).is_err());
    t.effectimplicittargetb1 = 63;
    assert_eq!(check_effect_mask(&t).unwrap(), 0);
}
#[test]
fn constructor_simple_value_overflow_is_refused_including_empty_effects() {
    let mut t = SpellTemplate {
        effectbasepoints3: i32::MAX - 1,
        effectbasedice3: 1,
        ..template()
    };
    assert_eq!(spell(&t, 0).base_points[2], i32::MAX);
    t.effectbasepoints3 = i32::MAX;
    assert_eq!(
        SourceSpell::new(1, &t, unit(), SpellTriggerFlags(0), 0)
            .unwrap_err()
            .id,
        "execution.spells.simple_value_overflow_undefined"
    );
}
struct Ordinary;
impl UnitEventOwner for Ordinary {
    fn forced_despawn(&mut self, _: bool) -> Result<()> {
        Ok(())
    }
}
fn update(
    q: &mut SourceUnitEvents,
    starts: &mut SourceSpellStarts,
    stores: &SourceSpellStores,
    f: &mut Fixture,
    diff: u32,
) -> Result<()> {
    q.update(
        diff,
        &mut SpellUnitEventOwner {
            starts,
            stores,
            spell_owner: f,
            ordinary: &mut Ordinary,
        },
    )
}
#[test]
fn event_creation_tick_suppression_uses_map_clock_not_queue_deadline() {
    // Spell.cpp:3521-3525,7153. The next callback receives the entire diff.
    let stores = stores();
    let t = stores.template(7).unwrap();
    let mut starts = SourceSpellStarts::default();
    let mut s = spell(t, 0);
    s.state = SourceSpellState::Casting;
    s.timer = 100;
    let mut q = SourceUnitEvents::default();
    s.event = Some(q.add_spell(1).unwrap());
    starts.spells.insert(1, s);
    let mut f = Fixture {
        time: 10,
        ..Default::default()
    };
    update(&mut q, &mut starts, &stores, &mut f, 50).unwrap();
    assert_eq!(starts.spells[&1].timer, 100);
    update(&mut q, &mut starts, &stores, &mut f, 25).unwrap();
    assert_eq!(starts.spells[&1].timer, 75);
    assert_eq!(q.inventory().queued[0].add_time_ms, 0);
}
#[test]
fn zero_delay_readd_is_eligible_in_same_update_and_finish_deletes_next_tick() {
    // Spell.cpp:7215-7219,7200-7209,7230-7231.
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut s = spell(stores.template(7).unwrap(), 0);
    s.state = SourceSpellState::Traveling;
    let mut q = SourceUnitEvents::default();
    s.event = Some(q.add_spell(1).unwrap());
    starts.spells.insert(1, s);
    let mut f = Fixture::default();
    update(&mut q, &mut starts, &stores, &mut f, 10).unwrap();
    assert_eq!(f.trace, ["handle_delayed(0)"]);
    assert_eq!(starts.spells[&1].state, SourceSpellState::Finished);
    assert_eq!(q.inventory().queued[0].queue.unwrap().deadline_ms, 11);
    update(&mut q, &mut starts, &stores, &mut f, 1).unwrap();
    assert!(starts.spells.is_empty());
    assert!(q.inventory().queued.is_empty());
}
#[test]
fn finished_referenced_spell_readds_until_current_slot_releases_reference() {
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut s = spell(stores.template(7).unwrap(), 0);
    s.state = SourceSpellState::Finished;
    s.referenced = true;
    let mut q = SourceUnitEvents::default();
    s.event = Some(q.add_spell(1).unwrap());
    starts.spells.insert(1, s);
    let mut f = Fixture::default();
    update(&mut q, &mut starts, &stores, &mut f, 1).unwrap();
    assert_eq!(q.inventory().queued[0].queue.unwrap().deadline_ms, 2);
    starts.spells.get_mut(&1).unwrap().referenced = false;
    update(&mut q, &mut starts, &stores, &mut f, 1).unwrap();
    assert!(starts.spells.is_empty());
}
#[test]
fn abort_killall_and_destructor_preserve_non_deletable_lifetime() {
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut s = spell(stores.template(7).unwrap(), 0);
    s.state = SourceSpellState::Casting;
    s.referenced = true;
    let mut q = SourceUnitEvents::default();
    s.event = Some(q.add_spell(1).unwrap());
    starts.spells.insert(1, s);
    let mut f = Fixture::default();
    q.kill_all_with_owner(
        false,
        &mut SpellUnitEventOwner {
            starts: &mut starts,
            stores: &stores,
            spell_owner: &mut f,
            ordinary: &mut Ordinary,
        },
    )
    .unwrap();
    assert_eq!(starts.spells[&1].state, SourceSpellState::Finished);
    assert_eq!(q.inventory().queued.len(), 1);
    update(&mut q, &mut starts, &stores, &mut f, 1).unwrap();
    assert!(q.inventory().queued.is_empty());
    assert!(starts.spells[&1].event.is_none());
    assert!(starts.spells[&1].referenced);
}
#[test]
fn range_reuses_bit_exact_shared_numeric_owner() {
    let out = spell_unit_range(
        [0; 3],
        [5f32.to_bits(), 0, 0],
        0,
        5f32.to_bits(),
        0,
        0,
        true,
    )
    .unwrap();
    assert_eq!(out.result, benilla_sim::spell_range::RangeResult::Within);
}
#[test]
fn direct_and_ai_callers_reach_the_same_start_owner() {
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut q = SourceUnitEvents::default();
    let mut f = Fixture {
        location: false,
        ..Default::default()
    };
    let mut context = SpellStartContext {
        stores: &stores,
        starts: &mut starts,
        events: &mut q,
        owner: &mut f,
    };
    assert_eq!(
        context.ai_cast(unit(), None, 7, 0).unwrap_err().id,
        "execution.terrain.live_vmap_area_owner"
    );
    assert_eq!(context.starts.spells[&1].state, SourceSpellState::Targeting);
    assert_eq!(context.events.inventory().queued.len(), 1);
}

#[test]
fn constructor_refuses_loaded_targeting_map_before_start_and_event_creation() {
    struct Constructor;
    impl SpellExecutionOwner for Constructor {
        fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
            Ok(operands())
        }
        fn map_time(&mut self, _: ObjectHandle) -> Result<u32> {
            Ok(10)
        }
    }
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut events = SourceUnitEvents::default();
    let mut owner = Constructor;
    let mut context = SpellStartContext {
        stores: &stores,
        starts: &mut starts,
        events: &mut events,
        owner: &mut owner,
    };
    assert_eq!(
        context
            .cast(unit(), None, 7, SpellTriggerFlags(0), false)
            .unwrap_err()
            .id,
        "execution.spells.f5_09_SpellTargetMgr_GetSpellTargetingData_owner"
    );
    assert!(starts.spells.is_empty());
    assert!(events.inventory().queued.is_empty());
    let other = ObjectHandle {
        construction: 2,
        ..unit()
    };
    assert_eq!(
        SourceSpell::construct(
            1,
            &stores,
            7,
            unit(),
            Some(other),
            SpellTriggerFlags(0),
            &mut owner
        )
        .unwrap_err()
        .id,
        "execution.spells.Spell_UpdateOriginalCasterPointer_owner"
    );
}

#[test]
fn school_mask_shift_preserves_defined_high_bit_and_refuses_width() {
    // SharedDefines.h:233-235; SpellSchoolMask has fixed uint32 underlying type.
    let mut t = SpellTemplate {
        school: 31,
        ..template()
    };
    assert_eq!(spell(&t, 0).school_mask, 0x80000000);
    t.school = 32;
    assert_eq!(
        SourceSpell::new(1, &t, unit(), SpellTriggerFlags(0), 0)
            .unwrap_err()
            .id,
        "execution.spells.GetSchoolMask_shift_undefined"
    );
}

#[test]
fn signed_zero_position_does_not_interrupt_cast_and_actual_move_does() {
    // Spell.cpp:3538-3547 compares floats, not their storage bits.
    let t = SpellTemplate {
        interruptflags: 1,
        ..template()
    };
    let mut s = spell(&t, 0);
    s.state = SourceSpellState::Casting;
    s.timer = 100;
    let mut f = Fixture::default();
    f.unit.position[0] = (-0.0f32).to_bits();
    s.update(&t, 10, &mut f).unwrap();
    assert_eq!(s.state, SourceSpellState::Casting);
    assert_eq!(s.timer, 90);
    f.unit.position[0] = 1f32.to_bits();
    s.update(&t, 10, &mut f).unwrap();
    assert_eq!(s.state, SourceSpellState::Finished);
    assert!(f.trace.contains(&"ResetGCD".into()));
}

#[test]
fn channel_pushback_preserves_signed_timer_comparison() {
    // Spell.cpp:6856-6862 uses int32(m_timer), then assigns its unsigned bits.
    let t = template();
    let mut s = spell(&t, 0);
    s.state = SourceSpellState::Channeling;
    s.timer = 0x80000000;
    let mut f = Fixture::default();
    f.unit.player = true;
    assert_eq!(s.pushback(&t, true, &mut f).unwrap(), 0x80000000);
    assert_eq!(s.timer, 0);
}

#[test]
fn gameobject_cancel_null_unit_refuses_only_when_source_dereferences_it() {
    // Spell.cpp:3103 returns before :3109's unconditional m_caster dereference.
    let t = template();
    let mut s = spell(&t, 0);
    let mut f = Fixture::default();
    f.unit.gameobject = true;
    assert_eq!(
        s.cancel(&t, &mut f).unwrap_err().id,
        "execution.spells.cancel_null_unit_caster_undefined"
    );
    assert_eq!(s.state, SourceSpellState::Created);
    assert!(f.trace.is_empty());
    s.state = SourceSpellState::Finished;
    s.cancel(&t, &mut f).unwrap();
}

#[test]
fn triggered_channels_are_current_even_with_concurrent_casts_ignored() {
    // Spell.cpp:3089-3091 does not apply the normal concurrent-cast guard.
    let t = SpellTemplate {
        attributesex: 4,
        ..template()
    };
    let mut s = spell(&t, 0x1001);
    let mut f = Fixture::default();
    s.prepare(&t, &mut f).unwrap();
    assert!(s.referenced);
    assert!(f.trace.contains(&"SetCurrentCastedSpell".into()));
    assert!(!f.trace.contains(&"AddGCD".into()));
    assert!(!f.trace.contains(&"SendStart".into()));
}

#[test]
fn eventai_cast_reaches_spellstart_and_keeps_its_event_on_geometry_refusal() {
    struct Bound<'a> {
        stores: &'a SourceSpellStores,
        starts: &'a mut SourceSpellStarts,
        events: &'a mut SourceUnitEvents,
        owner: &'a mut Fixture,
    }
    impl CreatureRandom for Bound<'_> {
        fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
            panic!("unexpected cast draw")
        }
        fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
            panic!("unexpected cast draw")
        }
    }
    impl EventAiExecutionOwner for Bound<'_> {
        fn spell_context(&mut self) -> Result<SpellStartContext<'_>> {
            Ok(SpellStartContext {
                stores: self.stores,
                starts: self.starts,
                events: self.events,
                owner: self.owner,
            })
        }
    }
    let mut state = super::super::event_ai_dispatch_tests::state();
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut events = SourceUnitEvents::default();
    let mut f = Fixture {
        location: false,
        ..Default::default()
    };
    let mut host = Bound {
        stores: &stores,
        starts: &mut starts,
        events: &mut events,
        owner: &mut f,
    };
    let cast = EventAiAction {
        kind: 11,
        params: Some([7, 0, 0]),
    };
    assert_eq!(
        state
            .execute_action(&cast, 123, EventAiInvocation::default(), None, &mut host)
            .unwrap_err()
            .id,
        "execution.terrain.live_vmap_area_owner"
    );
    assert_eq!(host.starts.spells[&1].caster, state.selected.object());
    assert_eq!(host.starts.spells[&1].state, SourceSpellState::Targeting);
    assert_eq!(host.events.inventory().queued.len(), 1);
    for (slot, expected) in [
        (3, "execution.spells.Unit_InterruptSpell_current_slot_owner"),
        (4, "execution.spells.EventAI_interrupt_slot_index_undefined"),
    ] {
        let action = EventAiAction {
            kind: 52,
            params: Some([slot, 0, 0]),
        };
        assert_eq!(
            state
                .execute_action(&action, 0, EventAiInvocation::default(), None, &mut host)
                .unwrap_err()
                .id,
            expected
        );
    }
}

#[test]
fn db_cast_bridge_uses_the_same_constructor_and_start() {
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut events = SourceUnitEvents::default();
    let mut objects = SourceObjects::default();
    let source = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut f = Fixture {
        location: false,
        ..Default::default()
    };
    let mut context = SpellStartContext {
        stores: &stores,
        starts: &mut starts,
        events: &mut events,
        owner: &mut f,
    };
    assert_eq!(
        context
            .db_cast(&objects, source, None, 7, 0, None)
            .unwrap_err()
            .id,
        "execution.terrain.live_vmap_area_owner"
    );
    assert_eq!(context.starts.spells[&1].flags, SpellTriggerFlags(0x40));
    assert_eq!(context.starts.spells[&1].state, SourceSpellState::Targeting);
    assert_eq!(context.events.inventory().queued.len(), 1);
}

#[test]
fn autorepeat_interrupt_sends_cancel_before_non_interruptible_flying_guard() {
    // Unit.cpp:4306-4313 sends the packet before CanBeInterrupted.
    let stores = stores();
    let mut s = spell(stores.template(9).unwrap(), 0);
    s.state = SourceSpellState::Traveling;
    let mut spells = BTreeMap::from([(1, s)]);
    let mut slots = SourceSpellSlots::default();
    let mut f = Fixture::default();
    f.unit.player = true;
    slots.set(1, &mut spells, &stores, &mut f).unwrap();
    f.trace.clear();
    slots
        .interrupt(SpellSlot::AutoRepeat, true, &mut spells, &stores, &mut f)
        .unwrap();
    assert_eq!(f.trace, ["AutoRepeatCancel"]);
    assert_eq!(slots.current(SpellSlot::AutoRepeat), Some(1));
    assert_eq!(spells[&1].state, SourceSpellState::Traveling);
}

#[test]
fn interrupt_calls_ai_before_clearing_current_reference() {
    // Unit.cpp:4316-4325 retains the reference through OnSpellInterrupt.
    struct CheckReference;
    impl SpellExecutionOwner for CheckReference {
        fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
            Ok(SpellUnitState {
                has_ai: true,
                ..operands()
            })
        }
        fn reset_gcd(&mut self, _: &SourceSpell, _: &SpellTemplate) -> Result<()> {
            Ok(())
        }
        fn call(&mut self, s: &mut SourceSpell, call: SpellCall) -> Result<()> {
            if call == SpellCall::AiInterrupt {
                assert!(s.referenced);
                assert_eq!(s.state, SourceSpellState::Finished);
            }
            Ok(())
        }
    }
    let stores = stores();
    let mut s = spell(stores.template(7).unwrap(), 0);
    s.state = SourceSpellState::Casting;
    let mut spells = BTreeMap::from([(1, s)]);
    let mut slots = SourceSpellSlots::default();
    slots
        .set(1, &mut spells, &stores, &mut CheckReference)
        .unwrap();
    slots
        .interrupt(
            SpellSlot::Generic,
            true,
            &mut spells,
            &stores,
            &mut CheckReference,
        )
        .unwrap();
    assert!(!spells[&1].referenced);
    assert_eq!(slots.current(SpellSlot::Generic), None);
}

#[test]
fn ranged_constructor_checks_class_mask_before_player_short_circuit() {
    // Spell.cpp:414; Unit.h:1318; FlagConvertibleEnum.h:56-58.
    let mut stores = stores();
    stores
        .insert_template(SpellTemplate {
            id: 10,
            dmgclass: 3,
            ..template()
        })
        .unwrap();
    let mut f = Fixture::default();
    for class in [1, 32] {
        f.unit.class = class;
        SourceSpell::construct(1, &stores, 10, unit(), None, SpellTriggerFlags(0), &mut f).unwrap();
    }
    for class in [0, 33] {
        f.unit.class = class;
        assert_eq!(
            SourceSpell::construct(1, &stores, 10, unit(), None, SpellTriggerFlags(0), &mut f)
                .unwrap_err()
                .id,
            "execution.spells.getClassMask_shift_undefined"
        );
    }
    // The same zero class does not reach the shift for a non-ranged spell.
    f.unit.class = 0;
    SourceSpell::construct(1, &stores, 7, unit(), None, SpellTriggerFlags(0), &mut f).unwrap();
}

#[test]
fn aura_removal_precedes_live_ai_presence_query() {
    // Spell.cpp:3046 may call aura scripts before :3050 reads AI().
    struct Removing {
        has_ai: bool,
        trace: Vec<SpellCall>,
    }
    impl SpellExecutionOwner for Removing {
        fn unit(&mut self, _: ObjectHandle) -> Result<SpellUnitState> {
            Ok(SpellUnitState {
                has_ai: self.has_ai,
                ..operands()
            })
        }
        fn call(&mut self, _: &mut SourceSpell, call: SpellCall) -> Result<()> {
            self.trace.push(call);
            if matches!(call, SpellCall::RemoveCastAuras(_)) {
                self.has_ai = true;
            }
            Ok(())
        }
        fn cast_time(&mut self, _: &mut SourceSpell, _: &SpellTemplate) -> Result<i32> {
            Ok(1000)
        }
        fn set_current(&mut self, _: &mut SourceSpell, _: &SpellTemplate) -> Result<()> {
            Ok(())
        }
        fn add_gcd(&mut self, _: &SourceSpell, _: &SpellTemplate) -> Result<()> {
            Ok(())
        }
    }
    let t = template();
    let mut s = spell(&t, 0x100);
    let mut f = Removing {
        has_ai: false,
        trace: Vec::new(),
    };
    s.prepare(&t, &mut f).unwrap();
    assert_eq!(
        f.trace,
        [
            SpellCall::PrepareTriggerData,
            SpellCall::RemoveCastAuras(4),
            SpellCall::AiCastState(true),
            SpellCall::SendStart
        ]
    );
}

#[test]
fn xyz_null_target_refuses_before_constructor_dependencies() {
    // UnitAI.cpp:221 dereferences target while evaluating CastSpell arguments.
    struct Unbound;
    impl SpellExecutionOwner for Unbound {}
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut events = SourceUnitEvents::default();
    let mut context = SpellStartContext {
        stores: &stores,
        starts: &mut starts,
        events: &mut events,
        owner: &mut Unbound,
    };
    assert_eq!(
        context
            .cast(unit(), None, 7, SpellTriggerFlags(0), true)
            .unwrap_err()
            .id,
        "execution.spells.UnitAI_CAST_ONLY_XYZ_null_target_undefined"
    );
    assert!(context.starts.spells.is_empty());
    assert!(context.events.inventory().queued.is_empty());
    // A non-null target reaches the position owner instead of undefined behavior.
    assert_eq!(
        context
            .cast(unit(), Some(unit()), 7, SpellTriggerFlags(0), true)
            .unwrap_err()
            .id,
        "execution.spells.Unit_check_operands_owner"
    );
}

#[test]
fn db_custom_constructor_precedes_pose_while_normal_db_pose_precedes_constructor() {
    // Unit.cpp:1553-1565 versus ScriptMgr.cpp:2185-2200.
    struct Unbound;
    impl SpellExecutionOwner for Unbound {}
    let stores = stores();
    let mut starts = SourceSpellStarts::default();
    let mut events = SourceUnitEvents::default();
    let mut objects = SourceObjects::default();
    let source = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut context = SpellStartContext {
        stores: &stores,
        starts: &mut starts,
        events: &mut events,
        owner: &mut Unbound,
    };
    assert_eq!(
        context
            .db_cast(&objects, source, Some(target), 7, 0, None)
            .unwrap_err()
            .id,
        "execution.spells.WorldObject_GetPosition_owner"
    );
    assert_eq!(
        context
            .db_cast(&objects, source, Some(target), 7, 0, Some([0, 1, 2]))
            .unwrap_err()
            .id,
        "execution.spells.Unit_check_operands_owner"
    );
    assert!(context.events.inventory().queued.is_empty());
}

#[test]
fn area_enum_boundary_is_checked_after_short_circuited_target_table_lookup() {
    // Spell.cpp:4803,4873; SpellMgr.h:884-892.
    let mut t = SpellTemplate {
        effect1: 2,
        effectimplicittargeta1: 6,
        effectimplicittargetb1: 127,
        attributesex3: 0x100,
        ..template()
    };
    let mut s = spell(&t, 1);
    s.targets.unit = Some(ObjectHandle {
        construction: 2,
        ..unit()
    });
    s.targets.mask = 2;
    assert_eq!(
        s.check_cast(&t, true, &mut Fixture::default()).unwrap(),
        R::BadTargets
    );
    t.effectimplicittargetb1 = 128;
    assert_eq!(check_effect_mask(&t).unwrap(), 1);
    assert_eq!(
        s.check_cast(&t, true, &mut Fixture::default())
            .unwrap_err()
            .id,
        "execution.spells.SpellTarget_enum_undefined"
    );
}
