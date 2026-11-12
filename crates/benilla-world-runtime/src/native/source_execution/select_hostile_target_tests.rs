use super::*;
use std::collections::BTreeSet;

struct Unbound;
impl UnitAiExecutionOwner for Unbound {}
impl HostileTargetExecutionOwner for Unbound {}
impl BuiltinAiExecutionOwner for Unbound {}

fn template() -> CreatureAiTemplateOperands {
    CreatureAiTemplateOperands {
        ordinal: 0,
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
        ai_name: "GuardAI".into(),
        script_name: String::new(),
    }
}
fn config() -> EventAiLoaderConfig {
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\n";
    EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap()
}
struct Fixture {
    objects: SourceObjects,
    units: SourceUnits,
    threats: SourceThreats,
    ai: SourceCreatureAi,
    a: ObjectHandle,
    b: ObjectHandle,
    threat_host: super::threat_tests::Host,
}
fn fixture() -> Fixture {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let map = MapIdentity {
        map: 0,
        instance: 0,
        creation_generation: 1,
    };
    let cells = SourceCells::new(map).unwrap();
    let handles = std::array::from_fn::<_, 3, _>(|i| {
        let h = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let low = i as u32 + 1;
        objects
            .assign_guid(h, low, SourceGuid::with_entry(HighGuid::Creature, 901, low))
            .unwrap();
        objects.bind_map(h, map, &cells).unwrap();
        objects.set_pose(h, SourcePose([0; 4])).unwrap();
        objects.register_world_lookup(h).unwrap();
        objects.get_mut(h).unwrap().in_world = true;
        units
            .register(&objects, h, SourceUnitFields::creature(&template()))
            .unwrap();
        h
    });
    let [unit, a, b] = handles;
    let ai = SourceCreatureAi::construct(
        &objects,
        unit,
        &mut units,
        &template(),
        false,
        0,
        &config(),
        &mut Unbound,
    )
    .unwrap();
    units.get_mut(&objects, unit).unwrap().has_ai = true;
    let mut threats = SourceThreats::default();
    threats.register(unit).unwrap();
    let mut threat_host = super::threat_tests::Host::default();
    for h in handles {
        threat_host.unit(h.construction, false);
        threat_host.units.get_mut(&h).unwrap().guid = objects.get(h).unwrap().guid.unwrap().raw();
    }
    Fixture {
        objects,
        units,
        threats,
        ai,
        a,
        b,
        threat_host,
    }
}
impl Fixture {
    fn select(&mut self, owner: &mut impl HostileTargetExecutionOwner) -> Result<bool> {
        self.ai
            .select_hostile_target(&self.objects, &mut self.units, &mut self.threats, owner)
    }
    fn add(&mut self, target: ObjectHandle, bits: u32) {
        self.threats
            .add_direct(
                self.ai.base.unit,
                target,
                self.objects.get(target).unwrap().guid.unwrap().raw(),
                ThreatValue::from_bits(bits),
                false,
                &mut self.threat_host,
            )
            .unwrap();
    }
    fn victim(&mut self, target: ObjectHandle) {
        let unit = self.ai.base.unit;
        let f = self.units.get_mut(&self.objects, unit).unwrap();
        f.victim = Some(target);
        f.target = Some(target);
        f.flags |= UNIT_FLAG_COMBAT;
        f.state |= UNIT_STATE_NO_COMBAT_MOVEMENT | UNIT_STATE_MELEE;
        self.units
            .get_mut(&self.objects, target)
            .unwrap()
            .attackers
            .push(unit);
    }
}
struct Authored {
    melee: BTreeSet<ObjectHandle>,
    reachable: bool,
    flying: bool,
    above: bool,
    chase: bool,
    attackable: bool,
    accessible: bool,
    casters: Option<Vec<u64>>,
    calls: Vec<UnitAiCall>,
    queries: Vec<HostileTargetQuery>,
    query_targets: Vec<Option<ObjectHandle>>,
    fronts: Vec<ObjectHandle>,
    taunts: usize,
    propagated: usize,
    stop: Option<UnitAiCall>,
}
impl Default for Authored {
    fn default() -> Self {
        Self {
            melee: BTreeSet::new(),
            reachable: true,
            flying: false,
            above: false,
            chase: true,
            attackable: true,
            accessible: true,
            casters: None,
            calls: Vec::new(),
            queries: Vec::new(),
            query_targets: Vec::new(),
            fronts: Vec::new(),
            taunts: 0,
            propagated: 0,
            stop: None,
        }
    }
}
impl UnitAiExecutionOwner for Authored {
    fn call(
        &mut self,
        _: ObjectHandle,
        call: UnitAiCall,
        units: &mut SourceUnits,
        objects: &SourceObjects,
    ) -> Result<()> {
        self.calls.push(call);
        if self.stop == Some(call) {
            return Err(call.missing_owner());
        }
        if call == UnitAiCall::CombatStopWithPets {
            // Authored callback models its actual observable effect for evade-order tests.
            let handles = [ObjectHandle {
                thread: 1,
                construction: 1,
            }];
            for h in handles {
                units.get_mut(objects, h)?.flags &= !UNIT_FLAG_COMBAT;
            }
        }
        Ok(())
    }
    fn has_unattackable_aura(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
}
impl HostileTargetExecutionOwner for Authored {
    fn hostile_query(
        &mut self,
        _: ObjectHandle,
        target: Option<ObjectHandle>,
        query: HostileTargetQuery,
    ) -> Result<bool> {
        self.queries.push(query);
        self.query_targets.push(target);
        Ok(match query {
            HostileTargetQuery::MeleeReach => self.melee.contains(&target.unwrap()),
            HostileTargetQuery::MotionReachable => self.reachable,
            HostileTargetQuery::TargetFlying => self.flying,
            HostileTargetQuery::AboveMeleeZ => self.above,
            HostileTargetQuery::ChaseMotion => self.chase,
            HostileTargetQuery::CanAttack => self.attackable,
            HostileTargetQuery::Accessible => self.accessible,
        })
    }
    fn set_in_front(&mut self, _: ObjectHandle, target: ObjectHandle) -> Result<()> {
        self.fronts.push(target);
        Ok(())
    }
    fn update_taunt_auras(
        &mut self,
        unit: ObjectHandle,
        threats: &mut SourceThreats,
    ) -> Result<()> {
        self.taunts += 1;
        let casters = self.casters.as_ref().ok_or_else(|| {
            refuse("execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner")
        })?;
        threats.taunt_update(unit, casters)
    }
    fn propagate_evade_none(&mut self, _: ObjectHandle) -> Result<()> {
        self.propagated += 1;
        Ok(())
    }
}

#[test]
fn empty_out_of_combat_returns_false_without_touching_missing_owners_or_empty_end() {
    let mut f = fixture();
    assert!(!f.select(&mut Unbound).unwrap());
    assert!(f
        .threats
        .manager(f.ai.base.unit)
        .unwrap()
        .online()
        .is_empty());
    assert_eq!(
        f.threats
            .select(f.ai.base.unit, &mut f.threat_host)
            .unwrap_err()
            .id,
        "execution.threat.empty_select_end_decrement_undefined"
    );
}
#[test]
fn dead_and_uninitialized_ai_return_before_threat_binding() {
    let mut f = fixture();
    f.threats = SourceThreats::default();
    f.objects.get_mut(f.ai.base.unit).unwrap().alive = false;
    assert!(!f.select(&mut Unbound).unwrap());
    f.objects.get_mut(f.ai.base.unit).unwrap().alive = true;
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().has_ai = false;
    assert!(!f.select(&mut Unbound).unwrap());
}
#[test]
fn single_target_uses_cached_threat_and_never_reads_taunt_aura_state() {
    let mut f = fixture();
    f.add(f.a, 100f32.to_bits());
    f.victim(f.a);
    let mut owner = Authored {
        melee: BTreeSet::from([f.a]),
        ..Default::default()
    };
    assert!(f.select(&mut owner).unwrap());
    assert_eq!(
        f.threats.manager(f.ai.base.unit).unwrap().current(),
        f.objects.get(f.a).unwrap().guid.map(|g| g.raw())
    );
    assert_eq!(owner.fronts, [f.a]);
    assert_eq!(owner.queries, [HostileTargetQuery::MeleeReach]);
    assert_eq!(owner.taunts, 0);
    assert!(owner.calls.is_empty());
}
#[test]
fn melee_and_ranged_switch_thresholds_use_the_existing_binary32_kernel() {
    for (bits, melee, switches) in [
        (0x42dc0000, true, false),
        (0x42dc0001, true, true),
        (0x43020000, false, false),
        (0x43020001, false, true),
    ] {
        let mut f = fixture();
        f.add(f.a, 100f32.to_bits());
        let unit = f.ai.base.unit;
        let old_guid = f.objects.get(f.a).unwrap().guid.unwrap().raw();
        f.threats.set_current(unit, Some(old_guid)).unwrap();
        f.add(f.b, bits);
        f.victim(f.a);
        let mut owner = Authored {
            melee: BTreeSet::from([f.a]),
            ..Default::default()
        };
        if melee {
            owner.melee.insert(f.b);
        }
        assert!(f.select(&mut owner).unwrap());
        let expected = if switches { f.b } else { f.a };
        assert_eq!(
            f.units.get(&f.objects, unit).unwrap().victim,
            Some(expected)
        );
        assert_eq!(
            f.threats.manager(unit).unwrap().current(),
            f.objects.get(expected).unwrap().guid.map(|g| g.raw())
        );
        if switches {
            assert_eq!(owner.calls[0], UnitAiCall::MeleeStop(Some(f.a)));
            assert!(!f
                .units
                .get(&f.objects, f.a)
                .unwrap()
                .attackers
                .contains(&unit));
            assert!(f
                .units
                .get(&f.objects, f.b)
                .unwrap()
                .attackers
                .contains(&unit));
        } else {
            assert!(owner.calls.is_empty());
        }
    }
}
#[test]
fn cached_taunt_overrides_threat_without_an_entry_aura_query() {
    let mut f = fixture();
    f.add(f.a, 100f32.to_bits());
    f.add(f.b, 1f32.to_bits());
    let unit = f.ai.base.unit;
    f.threats
        .set_current(unit, f.objects.get(f.a).unwrap().guid.map(|g| g.raw()))
        .unwrap();
    f.threats
        .taunt_update(unit, &[f.objects.get(f.b).unwrap().guid.unwrap().raw()])
        .unwrap();
    f.victim(f.a);
    let mut owner = Authored::default();
    assert!(f.select(&mut owner).unwrap());
    assert_eq!(f.units.get(&f.objects, unit).unwrap().victim, Some(f.b));
    assert_eq!(owner.taunts, 0);
}
#[test]
fn evade_reads_aura_list_even_when_threat_is_empty_and_refuses_absence_unknown() {
    let mut f = fixture();
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
    let mut owner = Authored::default();
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner"
    );
    assert_eq!(owner.taunts, 1);
    assert!(owner.calls.is_empty());
    // An authored empty aura collection is distinct from an unavailable collection.
    owner.casters = Some(vec![]);
    owner.stop = Some(UnitAiCall::RemoveAurasOnEvade);
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.unit_ai.f5_12_evade_aura_owner"
    );
    assert_eq!(owner.calls, [UnitAiCall::RemoveAurasOnEvade]);
}
#[test]
fn fixate_is_cleared_before_the_missing_aura_read() {
    let mut f = fixture();
    f.add(f.a, 1f32.to_bits());
    let unit = f.ai.base.unit;
    f.threats
        .fixate(unit, f.objects.get(f.a).unwrap().guid.map(|g| g.raw()), &[])
        .unwrap();
    // Rooted out-of-melee target causes selection to fall through to evade.
    let fields = f.units.get_mut(&f.objects, unit).unwrap();
    fields.ignore_ranged_targets = true;
    fields.flags |= UNIT_FLAG_COMBAT;
    // The ignore-ranged/nonempty branch returns true, preserving fixate, :8783-8784.
    assert!(f.select(&mut Authored::default()).unwrap());
    assert_eq!(f.threats.manager(unit).unwrap().online()[0].taunt, u32::MAX);
    // Exercise the reached FixateTarget prefix independently with an existing reference.
    f.threats.fixate_references(unit, None).unwrap();
    assert_eq!(f.threats.manager(unit).unwrap().online()[0].taunt, 0);
    assert_eq!(
        Unbound
            .update_taunt_auras(unit, &mut f.threats)
            .unwrap_err()
            .id,
        "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner"
    );
}
#[test]
fn evade_keeps_aura_combat_stop_home_and_callback_order_for_present_or_absent_taunts() {
    for taunts in [vec![], vec![123]] {
        let mut f = fixture();
        f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
        let mut owner = Authored {
            casters: Some(taunts),
            stop: Some(UnitAiCall::Home),
            ..Default::default()
        };
        assert_eq!(
            f.select(&mut owner).unwrap_err().id,
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
        assert_eq!(owner.taunts, 1);
    }
    let mut f = fixture();
    let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
    fields.flags |= UNIT_FLAG_COMBAT;
    fields.state |= 0x10;
    let mut owner = Authored {
        casters: Some(vec![]),
        ..Default::default()
    };
    assert!(!f.select(&mut owner).unwrap());
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
fn cannot_execute_combat_script_cc_and_evading_home_preserve_source_return_values() {
    for (script, cc, home, expected) in [
        (true, false, false, false),
        (false, false, true, true),
        (true, true, false, false),
    ] {
        let mut f = fixture();
        f.ai.base.combat_script = script;
        let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
        if cc {
            fields.flags |= 0x40000;
        }
        fields.evade_home = home;
        assert_eq!(f.select(&mut Unbound).unwrap(), expected);
    }
    let mut f = fixture();
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().flags |= UNIT_FLAG_COMBAT | 0x40000;
    assert_eq!(
        f.select(&mut Unbound).unwrap_err().id,
        "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner"
    );
}
#[test]
fn cannot_execute_faces_target_before_cc_evade_and_skips_self_target() {
    let mut f = fixture();
    f.ai.base.combat_script = true;
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().target = Some(f.a);
    assert_eq!(
        f.select(&mut Unbound).unwrap_err().id,
        "execution.select_hostile_target.f6_set_in_front_owner"
    );
    let mut owner = Authored::default();
    assert!(!f.select(&mut owner).unwrap());
    assert_eq!(owner.fronts, [f.a]);
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().target = Some(f.ai.base.unit);
    assert!(!f.select(&mut Unbound).unwrap());
}
#[test]
fn rooted_old_victim_detaches_without_attack_stop_or_melee_state_clear() {
    let mut f = fixture();
    f.victim(f.a);
    let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
    fields.ignore_ranged_targets = true;
    fields.flags &= !UNIT_FLAG_COMBAT;
    fields.state |= UNIT_STATE_MELEE;
    let mut owner = Authored::default();
    assert!(!f.select(&mut owner).unwrap());
    let fields = f.units.get(&f.objects, f.ai.base.unit).unwrap();
    assert!(fields.victim.is_none() && fields.target.is_none());
    assert_ne!(fields.state & UNIT_STATE_MELEE, 0);
    assert!(f.units.get(&f.objects, f.a).unwrap().attackers.is_empty());
    assert!(owner.calls.is_empty());
}
#[test]
fn rooted_old_victim_in_melee_is_retained_without_a_threat_entry() {
    let mut f = fixture();
    f.victim(f.a);
    f.units
        .get_mut(&f.objects, f.ai.base.unit)
        .unwrap()
        .ignore_ranged_targets = true;
    let mut owner = Authored {
        melee: BTreeSet::from([f.a]),
        ..Default::default()
    };
    assert!(f.select(&mut owner).unwrap());
    assert_eq!(
        f.units.get(&f.objects, f.ai.base.unit).unwrap().victim,
        Some(f.a)
    );
    assert!(f
        .threats
        .manager(f.ai.base.unit)
        .unwrap()
        .online()
        .is_empty());
}
#[test]
fn missing_melee_facing_attack_aura_and_motion_refuse_at_the_reached_calls() {
    let mut f = fixture();
    f.add(f.a, 1f32.to_bits());
    assert_eq!(
        f.select(&mut Unbound).unwrap_err().id,
        "execution.select_hostile_target.f6_melee_reach_owner"
    );
    let mut owner = Authored::default();
    // Actual AttackStart has no available aura owner with Unbound. Use a dedicated owner below.
    struct ReachOnly;
    impl UnitAiExecutionOwner for ReachOnly {}
    impl HostileTargetExecutionOwner for ReachOnly {
        fn hostile_query(
            &mut self,
            _: ObjectHandle,
            _: Option<ObjectHandle>,
            _: HostileTargetQuery,
        ) -> Result<bool> {
            Ok(true)
        }
        fn set_in_front(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        f.select(&mut ReachOnly).unwrap_err().id,
        "execution.unit_ai.f5_12_attack_aura_owner"
    );
    assert_eq!(
        f.units.get(&f.objects, f.ai.base.unit).unwrap().victim,
        None
    );
    f.victim(f.a);
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().state &= !UNIT_STATE_NO_COMBAT_MOVEMENT;
    struct FacingOnly;
    impl UnitAiExecutionOwner for FacingOnly {}
    impl HostileTargetExecutionOwner for FacingOnly {
        fn hostile_query(
            &mut self,
            _: ObjectHandle,
            _: Option<ObjectHandle>,
            query: HostileTargetQuery,
        ) -> Result<bool> {
            if query == HostileTargetQuery::MeleeReach {
                Ok(true)
            } else {
                Err(query.missing_owner())
            }
        }
        fn set_in_front(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        f.select(&mut FacingOnly).unwrap_err().id,
        "execution.select_hostile_target.f5_05_f6_reachability_owner"
    );
    assert!(f.select(&mut owner).unwrap());
}
#[test]
fn unreachable_target_starts_timer_once_and_reachable_target_stops_only_timer_first() {
    let mut f = fixture();
    f.add(f.a, 1f32.to_bits());
    f.victim(f.a);
    let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
    fields.state &= !UNIT_STATE_NO_COMBAT_MOVEMENT;
    let mut owner = Authored {
        reachable: false,
        ..Default::default()
    };
    assert!(f.select(&mut owner).unwrap());
    assert_eq!(
        f.units.get(&f.objects, f.ai.base.unit).unwrap().evade_timer,
        10000
    );
    f.units
        .get_mut(&f.objects, f.ai.base.unit)
        .unwrap()
        .evade_timer = 42;
    assert!(f.select(&mut owner).unwrap());
    assert_eq!(
        f.units.get(&f.objects, f.ai.base.unit).unwrap().evade_timer,
        42
    );
    owner.reachable = true;
    f.units
        .get_mut(&f.objects, f.ai.base.unit)
        .unwrap()
        .evade_combat = true;
    assert!(f.select(&mut owner).unwrap());
    assert_eq!(
        f.units.get(&f.objects, f.ai.base.unit).unwrap().evade_timer,
        0
    );
    assert!(
        f.units
            .get(&f.objects, f.ai.base.unit)
            .unwrap()
            .evade_combat
    );
    assert_eq!(owner.propagated, 0);
    assert!(f.select(&mut owner).unwrap());
    assert!(
        !f.units
            .get(&f.objects, f.ai.base.unit)
            .unwrap()
            .evade_combat
    );
    assert_eq!(owner.propagated, 1);
}
#[test]
fn only_unreachable_single_flying_high_melee_target_is_removed_before_evade() {
    for (ranged, multiple, flying, above, removes) in [
        (false, false, true, true, true),
        (true, false, true, true, false),
        (false, true, true, true, false),
        (false, false, false, true, false),
        (false, false, true, false, false),
    ] {
        let mut f = fixture();
        f.add(f.a, 100f32.to_bits());
        if multiple {
            f.add(f.b, 1f32.to_bits());
        }
        f.victim(f.a);
        f.ai.base.current_ranged_mode = ranged;
        f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().state &=
            !UNIT_STATE_NO_COMBAT_MOVEMENT;
        let mut owner = Authored {
            reachable: false,
            flying,
            above,
            ..Default::default()
        };
        if removes {
            assert_eq!(
                f.select(&mut owner).unwrap_err().id,
                "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner"
            );
            assert!(f
                .threats
                .manager(f.ai.base.unit)
                .unwrap()
                .online()
                .is_empty());
            assert!(f
                .threats
                .manager(f.ai.base.unit)
                .unwrap()
                .current()
                .is_none());
        } else {
            assert!(f.select(&mut owner).unwrap());
            assert_eq!(
                f.threats.manager(f.ai.base.unit).unwrap().online().len(),
                if multiple { 2 } else { 1 }
            );
        }
    }
}
#[test]
fn nonchasing_accessible_attacker_prevents_evade_but_not_the_false_return() {
    let mut f = fixture();
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
    f.units
        .get_mut(&f.objects, f.ai.base.unit)
        .unwrap()
        .attackers
        .push(f.a);
    let mut owner = Authored {
        chase: false,
        ..Default::default()
    };
    assert!(!f.select(&mut owner).unwrap());
    assert_eq!(
        owner.queries,
        [
            HostileTargetQuery::ChaseMotion,
            HostileTargetQuery::CanAttack,
            HostileTargetQuery::Accessible
        ]
    );
    assert_eq!(owner.taunts, 0);
    owner.attackable = false;
    owner.queries.clear();
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner"
    );
    assert_eq!(
        owner.queries,
        [
            HostileTargetQuery::ChaseMotion,
            HostileTargetQuery::CanAttack
        ]
    );
}
#[test]
fn controlled_unit_skips_fixate_but_still_enters_virtual_evade() {
    let mut f = fixture();
    let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
    fields.flags |= UNIT_FLAG_COMBAT | 8;
    fields.totem = true;
    let mut owner = Authored {
        stop: Some(UnitAiCall::RemoveAurasOnEvade),
        ..Default::default()
    };
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.unit_ai.f5_12_evade_aura_owner"
    );
    assert_eq!(owner.taunts, 0);
    assert_eq!(owner.calls, [UnitAiCall::RemoveAurasOnEvade]);
}

#[test]
fn guard_update_uses_real_selection_and_completes_empty_out_of_combat_update() {
    let mut f = fixture();
    let unit = f.ai.base.unit;
    let selected = SourceAiRegistry::builtin()
        .select(
            &f.objects,
            unit,
            template().inputs(CreatureAiControl::default(), false),
        )
        .unwrap();
    let mut ais = SourceCreatureAis::default();
    ais.install(&f.objects, &mut f.units, f.ai, &mut SourceCombat::default())
        .unwrap();
    let mut builtin = SourceBuiltinAi::construct(selected, &f.objects, &mut f.units, &ais).unwrap();
    let base = &mut ais.get_mut(&f.objects, unit).unwrap().base;
    base.generic_timers[0].disabled = false;
    base.generic_timers[0].remaining = 1;
    base.distancing_cooldown = true;
    builtin
        .update(
            1,
            &f.objects,
            &mut f.units,
            &mut ais,
            &mut SourceAiDependencies {
                threats: &mut f.threats,
                owner: &mut Unbound,
            },
        )
        .unwrap();
    assert!(!ais.get(&f.objects, unit).unwrap().base.distancing_cooldown);
    assert!(f.threats.manager(unit).unwrap().current().is_none());
    assert!(f.units.get(&f.objects, unit).unwrap().victim.is_none());
    f.units.get_mut(&f.objects, unit).unwrap().flags |= UNIT_FLAG_COMBAT;
    assert_eq!(
        builtin
            .update(
                1,
                &f.objects,
                &mut f.units,
                &mut ais,
                &mut SourceAiDependencies {
                    threats: &mut f.threats,
                    owner: &mut Unbound
                },
            )
            .unwrap_err()
            .id,
        "execution.select_hostile_target.f5_05_current_motion_owner"
    );
}

#[test]
fn catalog04_guard_update_and_eventai_selection_stage_census_or_explicit_absence() {
    use std::collections::BTreeMap;
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP target selection catalog-04 census: set BENILLA_F5_AI_CATALOG, BENILLA_F5_SCRIPT_REPORT and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let report = std::env::var_os("BENILLA_F5_SCRIPT_REPORT").expect("ScriptDev report required");
    let path = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let mut builder = AiRegistryBuilder::builtin();
    builder
        .load_catalog_scripts(&catalog, Path::new(&report))
        .unwrap();
    let registry = builder.freeze();
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    let mut spawns = BTreeMap::<u32, usize>::new();
    for (_, row) in catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap()
    {
        *spawns.entry(row[1]).or_default() += 1;
    }
    let mut raw = BTreeMap::<String, (usize, usize)>::new();
    let mut base_constructed = BTreeMap::<String, (usize, usize)>::new();
    let mut construction_refusals = BTreeMap::<String, BTreeMap<String, (usize, usize)>>::new();
    let mut selection_stage = BTreeMap::<String, BTreeMap<String, (usize, usize)>>::new();
    let mut full_update = BTreeMap::<String, BTreeMap<String, (usize, usize)>>::new();
    for t in catalog_creature_ai_census_inputs(&mut catalog).unwrap() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let selected = registry
            .select(
                &objects,
                unit,
                t.clone().inputs(
                    CreatureAiControl::default(),
                    factions.neutral_to_all(t.faction),
                ),
            )
            .unwrap();
        let Some(kind @ (BuiltinCreatureAi::Guard | BuiltinCreatureAi::Event)) =
            selected.executable().builtin_kind()
        else {
            continue;
        };
        let name = kind.key().to_owned();
        let n = spawns.get(&t.entry).copied().unwrap_or(0);
        let counts = raw.entry(name.clone()).or_default();
        counts.0 += 1;
        counts.1 += n;
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(&t))
            .unwrap();
        let base = match SourceCreatureAi::construct(
            &objects,
            unit,
            &mut units,
            &t,
            false,
            0,
            &config,
            &mut Unbound,
        ) {
            Ok(ai) => ai,
            Err(error) => {
                let counts = construction_refusals
                    .entry(name)
                    .or_default()
                    .entry(error.id)
                    .or_default();
                counts.0 += 1;
                counts.1 += n;
                continue;
            }
        };
        let counts = base_constructed.entry(name.clone()).or_default();
        counts.0 += 1;
        counts.1 += n;
        let mut ais = SourceCreatureAis::default();
        ais.install(&objects, &mut units, base, &mut SourceCombat::default())
            .unwrap();
        units.bind_ai_selection(&objects, &selected).unwrap();
        let mut threats = SourceThreats::default();
        threats.register(unit).unwrap();
        let stage = ais
            .get_mut(&objects, unit)
            .unwrap()
            .select_hostile_target(&objects, &mut units, &mut threats, &mut Unbound)
            .map_or_else(|e| e.id, |combat| format!("returned_{combat}"));
        let counts = selection_stage
            .entry(name.clone())
            .or_default()
            .entry(stage)
            .or_default();
        counts.0 += 1;
        counts.1 += n;
        let result = if kind == BuiltinCreatureAi::Guard {
            SourceBuiltinAi::construct(selected, &objects, &mut units, &ais)
                .unwrap()
                .update(
                    100,
                    &objects,
                    &mut units,
                    &mut ais,
                    &mut SourceAiDependencies {
                        threats: &mut threats,
                        owner: &mut Unbound,
                    },
                )
                .map_or_else(|e| e.id, |_| "completed".into())
        } else {
            // No invented EventAI holders or callback invocation is supplied.
            "CreatureEventAI::UpdateAI composition not bound".into()
        };
        let counts = full_update
            .entry(name)
            .or_default()
            .entry(result)
            .or_default();
        counts.0 += 1;
        counts.1 += n;
    }
    assert_eq!(raw["GuardAI"], (96, 337));
    assert_eq!(raw["EventAI"].0, 9865);
    assert_eq!(base_constructed["GuardAI"], (96, 337));
    assert_eq!(selection_stage["GuardAI"]["returned_false"], (96, 337));
    assert_eq!(full_update["GuardAI"]["completed"], (96, 337));
    let report = serde_json::json!({
        "scope":"ordinary uncontrolled raw-template alternatives and raw spawn joins; not live initialization, EventAI full construction, F1 execution or admission",
        "columns":"(templates, raw spawn rows)",
        "raw":raw, "base_constructed":base_constructed, "construction_refusals":construction_refusals,
        "selection_stage":selection_stage, "full_update":full_update,
        "undefined":"no new undefined operations; empty --end at ThreatManager.cpp:340-341 remains guarded at Unit.cpp:8727; zero unguarded stock callers",
        "format":91, "capability":"uninhabited",
    });
    println!(
        "F5_SELECT_TARGET_CENSUS {}",
        serde_json::to_string(&report).unwrap()
    );
}

#[test]
fn passive_attack_start_can_return_combat_true_without_a_victim() {
    let mut f = fixture();
    f.add(f.a, 1f32.to_bits());
    f.ai.base.react_state = 0;
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().state |= UNIT_STATE_NO_COMBAT_MOVEMENT;
    let mut owner = Authored::default();
    assert!(f.select(&mut owner).unwrap());
    assert!(f
        .units
        .get(&f.objects, f.ai.base.unit)
        .unwrap()
        .victim
        .is_none());
    assert_eq!(owner.fronts, [f.a]);
    assert!(owner.calls.is_empty());
}

#[test]
fn map_target_can_face_before_world_but_get_target_triggers_attack_start() {
    let mut f = fixture();
    f.ai.base.combat_script = true;
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().target = Some(f.a);
    f.objects.get_mut(f.ai.base.unit).unwrap().in_world = false;
    assert_eq!(
        f.select(&mut Unbound).unwrap_err().id,
        "execution.select_hostile_target.f6_set_in_front_owner"
    );
    f.ai.base.combat_script = false;
    f.add(f.a, 1f32.to_bits());
    f.victim(f.a);
    struct BeforeWorld;
    impl UnitAiExecutionOwner for BeforeWorld {}
    impl HostileTargetExecutionOwner for BeforeWorld {
        fn hostile_query(
            &mut self,
            _: ObjectHandle,
            _: Option<ObjectHandle>,
            query: HostileTargetQuery,
        ) -> Result<bool> {
            assert_eq!(query, HostileTargetQuery::MeleeReach);
            Ok(true)
        }
        fn set_in_front(&mut self, _: ObjectHandle, _: ObjectHandle) -> Result<()> {
            Ok(())
        }
    }
    assert_eq!(
        f.select(&mut BeforeWorld).unwrap_err().id,
        "execution.unit_ai.f5_12_attack_aura_owner"
    );
}

#[test]
fn nonchasing_multiple_attackers_refuse_unbound_pointer_order_before_predicates() {
    let mut f = fixture();
    let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
    fields.flags |= UNIT_FLAG_COMBAT;
    fields.attackers.extend([f.a, f.b]);
    let mut owner = Authored {
        chase: false,
        ..Default::default()
    };
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.select_hostile_target.f1_attacker_pointer_order_owner"
    );
    assert_eq!(owner.queries, [HostileTargetQuery::ChaseMotion]);
    assert_eq!(owner.taunts, 0);
}

#[test]
fn attacker_iteration_uses_bound_addresses_not_insertion_or_construction_order() {
    let mut f = fixture();
    f.objects
        .bind_source_allocation(f.a, 0x4000, 0x100, 0)
        .unwrap();
    f.objects
        .bind_source_allocation(f.b, 0x2000, 0x100, 0)
        .unwrap();
    let fields = f.units.get_mut(&f.objects, f.ai.base.unit).unwrap();
    fields.flags |= UNIT_FLAG_COMBAT;
    fields.attackers.extend([f.a, f.b]);
    let mut owner = Authored {
        chase: false,
        ..Default::default()
    };
    assert!(!f.select(&mut owner).unwrap());
    assert_eq!(owner.query_targets, [None, Some(f.b), Some(f.b)]);
    assert_eq!(
        f.units.get(&f.objects, f.ai.base.unit).unwrap().attackers,
        [f.b, f.a]
    );
    assert_eq!(owner.taunts, 0);
}

#[test]
fn nonchasing_empty_attackers_need_no_allocation_binding_before_taunt_query() {
    let mut f = fixture();
    f.units.get_mut(&f.objects, f.ai.base.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
    let mut owner = Authored {
        chase: false,
        ..Default::default()
    };
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.select_hostile_target.f5_12_f5_13_taunt_auras_owner"
    );
    assert_eq!(owner.queries, [HostileTargetQuery::ChaseMotion]);
    assert_eq!(owner.taunts, 1);
}

#[test]
fn out_of_world_pet_player_target_refuses_loading_owner_but_npc_target_does_not() {
    let mut f = fixture();
    f.add(f.a, 1f32.to_bits());
    f.victim(f.a);
    let unit = f.ai.base.unit;
    f.objects.get_mut(unit).unwrap().kind = SourceObjectKind::Pet;
    f.objects.get_mut(unit).unwrap().in_world = false;
    let mut owner = Authored::default();
    assert!(f.select(&mut owner).unwrap());
    let player = f.objects.construct(1, SourceObjectKind::Player).unwrap();
    f.objects
        .assign_guid(player, 0, SourceGuid::from_raw(7))
        .unwrap();
    f.units.get_mut(&f.objects, unit).unwrap().target = Some(player);
    assert_eq!(
        f.select(&mut owner).unwrap_err().id,
        "execution.select_hostile_target.f5_15_loading_pet_target_owner"
    );
}
