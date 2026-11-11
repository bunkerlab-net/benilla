use super::*;
use std::collections::{BTreeMap, VecDeque};

fn template(name: &str) -> CreatureAiTemplateOperands {
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
        ai_name: name.into(),
        script_name: String::new(),
    }
}
fn config() -> EventAiLoaderConfig {
    let bytes = b"LogFilter_EventAiDev=1\nGuarderSight=50\n";
    EventAiLoaderConfig::parse(bytes, Sha256Id::of(bytes)).unwrap()
}
#[derive(Default)]
struct Unbound;
impl UnitAiExecutionOwner for Unbound {}
impl BuiltinAiExecutionOwner for Unbound {}
struct Fixture {
    objects: SourceObjects,
    units: SourceUnits,
    ais: SourceCreatureAis,
    ai: SourceBuiltinAi,
    target: ObjectHandle,
}
fn fixture(name: &str) -> Fixture {
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let mut units = SourceUnits::default();
    let t = template(name);
    units
        .register(&objects, unit, SourceUnitFields::creature(&t))
        .unwrap();
    units
        .register(&objects, target, SourceUnitFields::creature(&t))
        .unwrap();
    objects.get_mut(unit).unwrap().alive = true;
    objects.get_mut(target).unwrap().alive = true;
    objects.get_mut(target).unwrap().in_world = true;
    objects.set_pose(unit, SourcePose([0; 4])).unwrap();
    objects.set_pose(target, SourcePose([0; 4])).unwrap();
    let selected = SourceAiRegistry::builtin()
        .select(
            &objects,
            unit,
            t.clone().inputs(CreatureAiControl::default(), false),
        )
        .unwrap();
    let mut ais = SourceCreatureAis::default();
    if name != "PossessedAI" {
        let base = SourceCreatureAi::construct(
            &objects,
            unit,
            &mut units,
            &t,
            false,
            0,
            &config(),
            &mut Unbound,
        )
        .unwrap();
        ais.install(&objects, &mut units, base, &mut SourceCombat::default())
            .unwrap();
    }
    let ai = SourceBuiltinAi::construct(selected, &objects, &mut units, &ais).unwrap();
    Fixture {
        objects,
        units,
        ais,
        ai,
        target,
    }
}
#[derive(Default)]
struct Authored {
    calls: Vec<UnitAiCall>,
    queries: Vec<(ObjectHandle, GuardQuery)>,
    query_results: VecDeque<bool>,
    flying: bool,
    vertical: bool,
    helper: Option<ObjectHandle>,
    select: Option<bool>,
    selections: usize,
    melee: usize,
}
impl UnitAiExecutionOwner for Authored {
    fn call(
        &mut self,
        _: ObjectHandle,
        call: UnitAiCall,
        _: &mut SourceUnits,
        _: &SourceObjects,
    ) -> Result<()> {
        self.calls.push(call);
        match call {
            UnitAiCall::Chase { .. } | UnitAiCall::Home => Err(call.missing_owner()),
            _ => Ok(()),
        }
    }
    fn sight_query(
        &mut self,
        _: ObjectHandle,
        _: ObjectHandle,
        query: UnitAiSightQuery,
    ) -> Result<bool> {
        Ok(match query {
            UnitAiSightQuery::TargetFlying => self.flying,
            UnitAiSightQuery::WithinVerticalRange(_) => self.vertical,
            _ => panic!("unexpected sight query"),
        })
    }
    fn has_unattackable_aura(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
}
impl BuiltinAiExecutionOwner for Authored {
    fn select_hostile_target(
        &mut self,
        _: &mut SourceCreatureAi,
        _: &SourceObjects,
        _: &mut SourceUnits,
    ) -> Result<bool> {
        self.selections += 1;
        self.select.ok_or_else(|| refuse("authored.select_unbound"))
    }
    fn update_melee(
        &mut self,
        _: ObjectHandle,
        _: &SourceObjects,
        _: &mut SourceUnits,
    ) -> Result<()> {
        self.melee += 1;
        Ok(())
    }
    fn guard_query(
        &mut self,
        _: ObjectHandle,
        who: ObjectHandle,
        query: GuardQuery,
    ) -> Result<bool> {
        self.queries.push((who, query));
        Ok(self
            .query_results
            .pop_front()
            .expect("unexpected guard query"))
    }
    fn helper_attacker(&mut self, _: ObjectHandle) -> Result<Option<ObjectHandle>> {
        Ok(self.helper)
    }
}
#[test]
fn selection_constructs_real_null_guard_and_possessed_bodies() {
    for (name, kind) in [
        ("NullAI", BuiltinCreatureAi::Null),
        ("GuardAI", BuiltinCreatureAi::Guard),
        ("PossessedAI", BuiltinCreatureAi::Possessed),
    ] {
        let f = fixture(name);
        assert_eq!(f.ai.selected().executable().builtin_kind(), Some(kind));
        assert_eq!(
            f.units
                .get(&f.objects, f.ai.selected().object())
                .unwrap()
                .can_enter_combat,
            name != "NullAI"
        );
        assert_eq!(f.ai.inventory(&f.objects, &f.ais).unwrap()["version"], 1);
    }
}
#[test]
fn null_missing_name_fallback_and_negative_guard_possessed_selection() {
    let mut f = fixture("NullAI");
    let unit = f.ai.selected().object();
    let mut input = template("").inputs(CreatureAiControl::default(), false);
    let registry = SourceAiRegistry::builtin();
    assert_eq!(
        registry
            .select(&f.objects, unit, input.clone())
            .unwrap()
            .executable()
            .builtin_kind(),
        Some(BuiltinCreatureAi::Event)
    );
    input.guard = true;
    assert_eq!(
        registry
            .select(&f.objects, unit, input.clone())
            .unwrap()
            .executable()
            .builtin_kind(),
        Some(BuiltinCreatureAi::Guard)
    );
    input.ai_name = "not_registered".into();
    let selected = registry.select(&f.objects, unit, input).unwrap();
    assert_eq!(*selected.executable(), AiExecutable::NullFallback);
    let body = SourceBuiltinAi::construct(selected, &f.objects, &mut f.units, &f.ais).unwrap();
    assert!(!body.is_visible(&f.objects, &f.ais).unwrap());
}
#[test]
fn null_callbacks_are_real_noops_but_death_inherits_combat_root_clear() {
    let mut f = fixture("NullAI");
    let mut owner = Authored::default();
    f.ai.update(1000, &f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    f.ai.attack_start(
        Some(f.target),
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &mut owner,
    )
    .unwrap();
    f.ai.attacked_by(
        Some(f.target),
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &mut owner,
    )
    .unwrap();
    f.ai.move_in_line_of_sight(
        f.target,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &config(),
        &mut owner,
    )
    .unwrap();
    f.ai.enter_evade(&f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    assert!(owner.calls.is_empty() && owner.queries.is_empty());
    assert_eq!(owner.selections, 0);
    let unit = f.ai.selected().object();
    f.ais
        .get_mut(&f.objects, unit)
        .unwrap()
        .base
        .combat_only_root = true;
    f.units.get_mut(&f.objects, unit).unwrap().state |= UNIT_STATE_AI_ROOT;
    f.ai.just_died(&f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    assert_eq!(owner.calls, [UnitAiCall::Immobilized(false)]);
    assert_eq!(
        f.units.get(&f.objects, unit).unwrap().state & UNIT_STATE_AI_ROOT,
        0
    );
}
#[test]
fn possessed_uses_unit_constructor_without_creature_reaction_or_null_combat_disable() {
    let mut f = fixture("PossessedAI");
    let BuiltinBody::Possessed(base) = &mut f.ai.body else {
        panic!()
    };
    assert_eq!(base.react_state, 2);
    assert_eq!(base.visibility_distance, 166.);
    assert!(base.melee_enabled);
    base.generic_timers[0].disabled = false;
    base.generic_timers[0].remaining = 1;
    f.ai.update(10, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
        .unwrap();
    let BuiltinBody::Possessed(base) = &f.ai.body else {
        panic!()
    };
    assert_eq!(base.generic_timers[0].remaining, 1); // PossessedAI.h:41-44 skips timers.
    assert!(f.units.get(&f.objects, base.unit).unwrap().can_enter_combat);
}
#[test]
fn possessed_melee_short_circuits_order_missing_victim_and_casting_before_owner() {
    let mut f = fixture("PossessedAI");
    let unit = f.ai.selected().object();
    f.units.get_mut(&f.objects, unit).unwrap().state = UNIT_STATE_MELEE;
    f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
        .unwrap();
    f.units.get_mut(&f.objects, unit).unwrap().victim = Some(f.target);
    f.units.get_mut(&f.objects, unit).unwrap().casting_non_melee = true;
    f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
        .unwrap();
    f.units.get_mut(&f.objects, unit).unwrap().casting_non_melee = false;
    let BuiltinBody::Possessed(base) = &mut f.ai.body else {
        panic!()
    };
    base.order = 1;
    f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
        .unwrap();
    let BuiltinBody::Possessed(base) = &mut f.ai.body else {
        panic!()
    };
    base.order = 0;
    assert_eq!(
        f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.builtin_ai.f5_04_melee_update_binding_owner"
    );
    let mut owner = Authored::default();
    f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    assert_eq!(owner.melee, 1);
    assert_eq!(owner.selections, 0);
}
#[test]
fn guard_inherited_update_expires_exact_boundary_timer_before_target_binding() {
    let mut f = fixture("GuardAI");
    let unit = f.ai.selected().object();
    let base = &mut f.ais.get_mut(&f.objects, unit).unwrap().base;
    base.generic_timers[0].disabled = false;
    base.generic_timers[0].remaining = 10;
    base.distancing_cooldown = true;
    assert_eq!(
        f.ai.update(9, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.builtin_ai.f5_04_select_hostile_target_binding_owner"
    );
    assert!(
        f.ais
            .get(&f.objects, unit)
            .unwrap()
            .base
            .distancing_cooldown
    );
    let mut owner = Authored {
        select: Some(false),
        ..Default::default()
    };
    f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    assert!(
        !f.ais
            .get(&f.objects, unit)
            .unwrap()
            .base
            .distancing_cooldown
    );
    assert_eq!(owner.selections, 1);
    assert_eq!(owner.melee, 0);
}
#[test]
fn guard_vertical_and_existing_victim_gates_precede_assistance() {
    let mut f = fixture("GuardAI");
    let mut owner = Authored {
        flying: true,
        vertical: false,
        ..Default::default()
    };
    f.ai.move_in_line_of_sight(
        f.target,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &config(),
        &mut owner,
    )
    .unwrap();
    assert!(owner.queries.is_empty());
    owner.flying = false;
    let unit = f.ai.selected().object();
    f.units.get_mut(&f.objects, unit).unwrap().victim = Some(f.target);
    f.ai.move_in_line_of_sight(
        f.target,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &config(),
        &mut owner,
    )
    .unwrap();
    assert!(owner.queries.is_empty());
}
#[test]
fn guard_direct_attack_checks_exact_spatial_order_then_real_creature_attack() {
    let mut f = fixture("GuardAI");
    let mut owner = Authored {
        query_results: [true; 5].into(),
        ..Default::default()
    };
    assert_eq!(
        f.ai.move_in_line_of_sight(
            f.target,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &config(),
            &mut owner
        )
        .unwrap_err()
        .id,
        "execution.unit_ai.f5_05_motion_owner"
    );
    assert_eq!(
        owner.queries.iter().map(|(_, q)| *q).collect::<Vec<_>>(),
        [
            GuardQuery::CanInitiateAttack,
            GuardQuery::AttackOnSight,
            GuardQuery::Accessible,
            GuardQuery::AttackDistance,
            GuardQuery::LineOfSight
        ]
    );
    assert_eq!(
        f.units
            .get(&f.objects, f.ai.selected().object())
            .unwrap()
            .victim,
        Some(f.target)
    );
}
#[test]
fn guard_assistance_with_missing_helper_returns_without_hostile_fallback() {
    let mut f = fixture("GuardAI");
    f.units.get_mut(&f.objects, f.target).unwrap().flags |= UNIT_FLAG_COMBAT;
    let mut owner = Authored {
        query_results: [true].into(),
        ..Default::default()
    };
    f.ai.move_in_line_of_sight(
        f.target,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &config(),
        &mut owner,
    )
    .unwrap();
    assert_eq!(owner.queries, [(f.target, GuardQuery::CanAssist)]);
    assert!(owner.calls.is_empty());
}
#[test]
fn guard_and_possessed_control_evade_and_death_use_their_actual_base() {
    for name in ["GuardAI", "PossessedAI"] {
        let mut f = fixture(name);
        f.ai.attack_start(None, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
            .unwrap();
        f.ai.attacked_by(None, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
            .unwrap();
        assert_eq!(
            f.ai.enter_evade(&f.objects, &mut f.units, &mut f.ais, &mut Unbound)
                .unwrap_err()
                .id,
            "execution.unit_ai.f5_12_evade_aura_owner"
        );
        let unit = f.ai.selected().object();
        let base = match &mut f.ai.body {
            BuiltinBody::Possessed(base) => base,
            _ => &mut f.ais.get_mut(&f.objects, unit).unwrap().base,
        };
        base.combat_only_root = true;
        assert_eq!(
            f.ai.just_died(&f.objects, &mut f.units, &mut f.ais, &mut Unbound)
                .unwrap_err()
                .id,
            "execution.unit_ai.f5_05_immobilized_owner"
        );
        assert!(match &f.ai.body {
            BuiltinBody::Possessed(base) => base.combat_only_root,
            _ => f.ais.get(&f.objects, unit).unwrap().base.combat_only_root,
        });
        let mut owner = Authored::default();
        f.ai.just_died(&f.objects, &mut f.units, &mut f.ais, &mut owner)
            .unwrap();
        assert_eq!(owner.calls, [UnitAiCall::Immobilized(false)]);
    }
}
#[test]
fn owner_loss_does_not_invent_derived_cleanup_for_null_guard_or_possessed() {
    for name in ["NullAI", "GuardAI", "PossessedAI"] {
        let mut f = fixture(name);
        let unit = f.ai.selected().object();
        let owner = f.objects.construct(2, SourceObjectKind::Player).unwrap();
        let selected = SourceAiRegistry::builtin()
            .select(
                &f.objects,
                unit,
                template(name).inputs(
                    CreatureAiControl {
                        owner: Some(owner),
                        ..CreatureAiControl::default()
                    },
                    false,
                ),
            )
            .unwrap();
        f.ai = SourceBuiltinAi::construct(selected, &f.objects, &mut f.units, &f.ais).unwrap();
        f.objects
            .destroy(
                owner,
                &mut SourceCells::new(MapIdentity {
                    map: 0,
                    instance: 0,
                    creation_generation: 1,
                })
                .unwrap(),
            )
            .unwrap();
        f.units.get_mut(&f.objects, unit).unwrap().owner = None;
        let mut execution = Authored {
            select: Some(false),
            ..Default::default()
        };
        f.ai.update(100, &f.objects, &mut f.units, &mut f.ais, &mut execution)
            .unwrap();
        f.ai.just_died(&f.objects, &mut f.units, &mut f.ais, &mut execution)
            .unwrap();
        assert!(execution.calls.is_empty());
    }
}
#[test]
fn stale_selected_creature_refuses_before_builtin_dispatch() {
    let mut f = fixture("NullAI");
    f.objects
        .destroy(
            f.ai.selected().object(),
            &mut SourceCells::new(MapIdentity {
                map: 0,
                instance: 0,
                creation_generation: 1,
            })
            .unwrap(),
        )
        .unwrap();
    assert_eq!(
        f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
            .unwrap_err()
            .id,
        "execution.stale_object_handle"
    );
}
#[test]
fn builtin_catalog04_constructor_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP builtin catalog-04 constructors: set BENILLA_F5_AI_CATALOG, BENILLA_F5_SCRIPT_REPORT and BENILLA_F5_EVENT_AI_CONFIG");
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
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
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
    let mut spawn_counts = BTreeMap::<u32, usize>::new();
    for (_, row) in catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap()
    {
        *spawn_counts.entry(row[1]).or_default() += 1;
    }
    let mut selected_counts = BTreeMap::<String, usize>::from(
        [
            "NullAI",
            "GuardAI",
            "PetAI",
            "TotemAI",
            "EventAI",
            "GuardianAI",
            "PossessedAI",
            "ScriptDev",
        ]
        .map(|name| (name.into(), 0)),
    );
    let mut outcomes = BTreeMap::<String, BTreeMap<String, usize>>::new();
    let mut update_gates = BTreeMap::<String, BTreeMap<String, (usize, usize)>>::new();
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
        let name = selected
            .executable()
            .builtin_kind()
            .map_or("ScriptDev", |k| k.key())
            .to_owned();
        *selected_counts.entry(name.clone()).or_default() += 1;
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(&t))
            .unwrap();
        let mut ais = SourceCreatureAis::default();
        let result = (|| {
            if matches!(
                selected.executable().builtin_kind(),
                Some(BuiltinCreatureAi::Null | BuiltinCreatureAi::Guard)
            ) {
                let base = SourceCreatureAi::construct(
                    &objects,
                    unit,
                    &mut units,
                    &t,
                    false,
                    0,
                    &config,
                    &mut Unbound,
                )?;
                ais.install(&objects, &mut units, base, &mut SourceCombat::default())?;
            }
            SourceBuiltinAi::construct(selected, &objects, &mut units, &ais)
        })();
        let reason = match result {
            Ok(mut ai) => {
                let update = ai
                    .update(100, &objects, &mut units, &mut ais, &mut Unbound)
                    .map_or_else(|e| e.id, |_| "updated".into());
                let counts = update_gates
                    .entry(name.clone())
                    .or_default()
                    .entry(update)
                    .or_default();
                counts.0 += 1;
                counts.1 += spawn_counts.get(&t.entry).copied().unwrap_or(0);
                "constructed".into()
            }
            Err(e) => e.id,
        };
        *outcomes.entry(name).or_default().entry(reason).or_default() += 1;
    }
    assert_eq!(selected_counts.values().sum::<usize>(), 10384);
    assert_eq!(selected_counts["GuardAI"], 96);
    assert_eq!(selected_counts["TotemAI"], 93);
    assert_eq!(outcomes["GuardAI"]["constructed"], 96);
    assert_eq!(
        update_gates["GuardAI"]["execution.builtin_ai.f5_04_select_hostile_target_binding_owner"],
        (96, 337)
    );
    println!("builtin catalog-04 selection={selected_counts:?}; constructors={outcomes:?}; first-update gates (templates, raw-spawn rows)={update_gates:?}; ordinary uncontrolled raw-template alternatives, not live-spawn or F1 acceptance; EventAI uses its existing separate constructor");
}

#[test]
fn respawn_and_unsummon_follow_the_selected_base_class() {
    for name in ["NullAI", "GuardAI", "PossessedAI"] {
        let mut f = fixture(name);
        let unit = f.ai.selected().object();
        let base = match &mut f.ai.body {
            BuiltinBody::Possessed(base) => base,
            _ => &mut f.ais.get_mut(&f.objects, unit).unwrap().base,
        };
        base.generic_timers[0].disabled = false;
        base.generic_timers[0].remaining = 10;
        base.ranged_mode = true;
        base.current_ranged_mode = false;
        base.chase_distance = 7.;
        f.ai.just_respawned(&f.objects, &mut f.ais).unwrap();
        f.ai.on_unsummon(&f.objects, &f.ais).unwrap();
        f.ai.on_heartbeat(&f.objects, &f.ais).unwrap();
        let base = match &f.ai.body {
            BuiltinBody::Possessed(base) => base,
            _ => &f.ais.get(&f.objects, unit).unwrap().base,
        };
        assert_eq!(base.generic_timers[0].disabled, name != "PossessedAI");
        assert_eq!(
            base.generic_timers[0].remaining,
            if name == "PossessedAI" { 10 } else { 0 }
        );
        assert_eq!(base.current_ranged_mode, name != "PossessedAI");
        assert_eq!(
            base.attack_distance,
            if name == "PossessedAI" { 0. } else { 7. }
        );
    }
}

#[test]
fn missing_base_or_other_selected_class_never_becomes_a_null_body() {
    let mut f = fixture("NullAI");
    let selected = f.ai.selected().clone();
    assert_eq!(
        SourceBuiltinAi::construct(
            selected,
            &f.objects,
            &mut f.units,
            &SourceCreatureAis::default()
        )
        .unwrap_err()
        .id,
        "execution.unit_ai.generation_missing"
    );
    for (name, id) in [
        ("PetAI", "execution.ai.pet_constructor_owner"),
        ("GuardianAI", "execution.ai.guardian_constructor_owner"),
        ("TotemAI", "execution.ai.totem_constructor_owner"),
        ("EventAI", "execution.ai.event_ai_constructor_owner"),
    ] {
        let selected = SourceAiRegistry::builtin()
            .select(
                &f.objects,
                f.ai.selected().object(),
                template(name).inputs(CreatureAiControl::default(), false),
            )
            .unwrap();
        assert_eq!(
            SourceBuiltinAi::construct(selected, &f.objects, &mut f.units, &f.ais)
                .unwrap_err()
                .id,
            id
        );
    }
}

#[test]
fn guard_player_assistance_checks_five_then_ten_then_victim_los() {
    let mut f = fixture("GuardAI");
    f.objects.get_mut(f.target).unwrap().kind = SourceObjectKind::Player;
    f.units.get_mut(&f.objects, f.target).unwrap().flags |= UNIT_FLAG_COMBAT;
    let victim = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    f.units
        .register(
            &f.objects,
            victim,
            SourceUnitFields::creature(&template("")),
        )
        .unwrap();
    let mut owner = Authored {
        helper: Some(victim),
        query_results: [true, true, true, true, true, false].into(),
        ..Default::default()
    };
    f.ai.move_in_line_of_sight(
        f.target,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &config(),
        &mut owner,
    )
    .unwrap();
    assert_eq!(
        owner.queries,
        [
            (f.target, GuardQuery::CanAssist),
            (victim, GuardQuery::CanInitiateAttack),
            (victim, GuardQuery::AttackOnSight),
            (victim, GuardQuery::Accessible),
            (f.target, GuardQuery::WithinDistance(5f32.to_bits())),
            (victim, GuardQuery::WithinDistance(10f32.to_bits())),
        ]
    );
    assert!(owner.calls.is_empty());
}

#[test]
fn guard_pet_assistance_preserves_the_source_creature_guid_test() {
    // GuardAI.cpp:61 and ObjectGuid.h:124 deliberately exclude HIGHGUID_PET.
    for high in [HighGuid::Pet, HighGuid::Creature] {
        let mut f = fixture("GuardAI");
        f.objects.get_mut(f.target).unwrap().guid =
            Some(SourceGuid::with_entry(HighGuid::Creature, 901, 1));
        f.units.get_mut(&f.objects, f.target).unwrap().flags |= UNIT_FLAG_COMBAT;
        let victim = f.objects.construct(1, SourceObjectKind::Pet).unwrap();
        f.objects.get_mut(victim).unwrap().guid = Some(SourceGuid::with_entry(high, 901, 2));
        f.units
            .register(
                &f.objects,
                victim,
                SourceUnitFields::creature(&template("")),
            )
            .unwrap();
        let player = f.objects.construct(1, SourceObjectKind::Player).unwrap();
        let fields = f.units.get_mut(&f.objects, victim).unwrap();
        fields.pet_kind = Some(CreaturePetKind::Controlled);
        fields.owner = Some(player);
        let mut owner = Authored {
            helper: Some(victim),
            query_results: [true, true, true, true, false].into(),
            ..Default::default()
        };
        f.ai.move_in_line_of_sight(
            f.target,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &config(),
            &mut owner,
        )
        .unwrap();
        assert_eq!(
            owner.queries.last().unwrap().1,
            if high == HighGuid::Pet {
                GuardQuery::GuardOrCivilian
            } else {
                GuardQuery::WithinDistance(30f32.to_bits())
            }
        );
        assert!(owner.calls.is_empty());
    }
}

#[test]
fn guard_sight_has_no_inherited_reaction_or_neutrality_gate() {
    // GuardAI.cpp:39-86 differs from UnitAI.cpp:70-90.
    let mut f = fixture("GuardAI");
    let unit = f.ai.selected().object();
    f.ais.get_mut(&f.objects, unit).unwrap().base.react_state = 0;
    f.units.get_mut(&f.objects, unit).unwrap().neutral_to_all = Some(true);
    let mut owner = Authored {
        query_results: [true; 5].into(),
        ..Default::default()
    };
    f.ai.move_in_line_of_sight(
        f.target,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &config(),
        &mut owner,
    )
    .unwrap();
    assert_eq!(owner.queries.len(), 5);
    assert!(owner.calls.is_empty());
    assert!(f.units.get(&f.objects, unit).unwrap().victim.is_none());
}

#[test]
fn guard_missing_flight_predicate_and_spatial_owners_are_distinct() {
    let mut f = fixture("GuardAI");
    assert_eq!(
        f.ai.move_in_line_of_sight(
            f.target,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &config(),
            &mut Unbound
        )
        .unwrap_err()
        .id,
        "execution.unit_ai.f5_05_flight_state_owner"
    );
    let unit = f.ai.selected().object();
    f.units.get_mut(&f.objects, unit).unwrap().can_fly = true;
    assert_eq!(
        f.ai.move_in_line_of_sight(
            f.target,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &config(),
            &mut Unbound
        )
        .unwrap_err()
        .id,
        "execution.builtin_ai.f5_04_guard_predicate_binding_owner"
    );
    assert_eq!(
        Unbound
            .guard_query(unit, f.target, GuardQuery::LineOfSight)
            .unwrap_err()
            .id,
        "execution.builtin_ai.f6_guard_query_owner"
    );
}

#[test]
fn possessed_empty_spell_list_is_real_while_creature_spell_lists_need_the_owner() {
    for name in ["NullAI", "GuardAI", "PossessedAI"] {
        let mut f = fixture(name);
        let result = f.ai.enter_combat(
            Some(f.target),
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &mut Unbound,
        );
        if name == "PossessedAI" {
            result.unwrap();
            let BuiltinBody::Possessed(base) = &f.ai.body else {
                panic!()
            };
            assert!(base.spell_list_cooldown);
        } else {
            assert_eq!(result.unwrap_err().id, "execution.unit_ai.spell_list_owner");
        }
    }
}

#[test]
fn guard_ranged_flags_only_request_owners_for_reached_source_work() {
    let mut f = fixture("GuardAI");
    let unit = f.ai.selected().object();
    let base = &mut f.ais.get_mut(&f.objects, unit).unwrap().base;
    base.ranged_mode = true;
    base.ranged_setting = 0;
    f.units.get_mut(&f.objects, unit).unwrap().victim = Some(f.target);
    let mut owner = Authored {
        select: Some(true),
        ..Default::default()
    };
    f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    assert_eq!(owner.melee, 0);
    f.ais.get_mut(&f.objects, unit).unwrap().base.ranged_setting = 2;
    assert_eq!(
        f.ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
            .unwrap_err()
            .id,
        "execution.builtin_ai.f6_ranged_distancing_owner"
    );
}
