use super::super::creature_tests::Random;
use super::*;
use std::collections::BTreeMap;

#[derive(Default)]
struct Dependencies {
    calls: Vec<UnitAiCall>,
    refuse_chase: bool,
    loading: Option<bool>,
}
impl UnitAiExecutionOwner for Dependencies {
    fn pet_is_loading(&mut self, _: ObjectHandle) -> Result<bool> {
        self.loading.ok_or_else(|| {
            refuse("execution.pet_ai.Unit_GetCharmer_GetCreator_out_of_world_loading_owner")
        })
    }
    fn has_unattackable_aura(&mut self, _: ObjectHandle) -> Result<bool> {
        Ok(false)
    }
    fn call(
        &mut self,
        _: ObjectHandle,
        call: UnitAiCall,
        _: &mut SourceUnits,
        _: &SourceObjects,
    ) -> Result<()> {
        self.calls.push(call);
        if self.refuse_chase && matches!(call, UnitAiCall::Chase { .. }) {
            Err(call.missing_owner())
        } else {
            Ok(())
        }
    }
}
impl BuiltinAiExecutionOwner for Dependencies {
    fn select_hostile_target(
        &mut self,
        _: &mut SourceCreatureAi,
        _: &SourceObjects,
        _: &mut SourceUnits,
    ) -> Result<bool> {
        panic!("PetAI must not select a hostile target during UpdateTimers");
    }
}
impl PetAuraExecutionOwner for Dependencies {}

fn template(guard: bool) -> CreatureAiTemplateOperands {
    CreatureAiTemplateOperands {
        ordinal: 0,
        entry: 901,
        faction: 14,
        extra_flags: if guard { 0x400 } else { 0 },
        unit_flags: 0,
        type_flags: 0,
        static_flags: [0; 4],
        rank: 0,
        inhabit_type: 1,
        stats_multipliers: [1f32.to_bits(); 3],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: 0,
        ai_name: "PetAI".into(),
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
    ais: SourceCreatureAis,
    selected: SelectedCreatureAi,
    target: ObjectHandle,
}
impl Fixture {
    fn new(guard: bool, pet: bool) -> Self {
        let mut objects = SourceObjects::default();
        let unit = objects
            .construct(
                1,
                if pet {
                    SourceObjectKind::Pet
                } else {
                    SourceObjectKind::Creature
                },
            )
            .unwrap();
        let target = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let map = MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        };
        for (u, high, counter) in [
            (
                unit,
                if pet {
                    HighGuid::Pet
                } else {
                    HighGuid::Creature
                },
                1,
            ),
            (target, HighGuid::Creature, 2),
        ] {
            objects
                .assign_guid(u, 0, SourceGuid::with_entry(high, 901, counter))
                .unwrap();
            objects.get_mut(u).unwrap().alive = true;
            objects.get_mut(u).unwrap().map = Some(map);
            objects.set_pose(u, SourcePose([0; 4])).unwrap();
            objects.add_to_world(u).unwrap();
        }
        let t = template(guard);
        let control = CreatureAiControl {
            pet: pet.then_some(CreaturePetKind::Controlled),
            player_controlled: pet,
            ..Default::default()
        };
        let selected = SourceAiRegistry::builtin()
            .select(&objects, unit, t.clone().inputs(control, false))
            .unwrap();
        let mut units = SourceUnits::default();
        for u in [unit, target] {
            units
                .register(&objects, u, SourceUnitFields::creature(&t))
                .unwrap();
        }
        units.bind_ai_selection(&objects, &selected).unwrap();
        let base = SourceCreatureAi::construct(
            &objects,
            unit,
            &mut units,
            &t,
            false,
            0,
            &config(),
            &mut Random::new(),
        )
        .unwrap();
        let mut ais = SourceCreatureAis::default();
        ais.install(&objects, &mut units, base, &mut SourceCombat::default())
            .unwrap();
        Self {
            objects,
            units,
            ais,
            selected,
            target,
        }
    }
    fn control(&mut self) -> &mut SourceControlGuids {
        &mut self
            .units
            .get_mut(&self.objects, self.selected.object())
            .unwrap()
            .control_guids
    }
    fn construct(
        &mut self,
        spell: Option<u32>,
        behind: Option<&PetAttackFromBehindConfig>,
    ) -> Result<SourceBuiltinAi> {
        self.construct_with(spell, behind, &mut Dependencies::default())
    }
    fn construct_with(
        &mut self,
        spell: Option<u32>,
        behind: Option<&PetAttackFromBehindConfig>,
        owner: &mut impl UnitAiExecutionOwner,
    ) -> Result<SourceBuiltinAi> {
        SourceBuiltinAi::construct_pet(
            self.selected.clone(),
            &self.objects,
            PetAiConstructionOwners {
                units: &mut self.units,
                ais: &mut self.ais,
                created_by_spell: spell,
                config: &config(),
                attack_from_behind: behind,
                execution: owner,
            },
        )
    }
    fn authored(&self) -> SourcePetAi {
        SourcePetAi {
            unit: self.selected.object(),
            is_pet: self.selected.inputs().control.pet.is_some(),
            in_combat: false,
        }
    }
}

#[test]
fn pet_constructor_preserves_first_reads_and_partial_base_writes() {
    let mut f = Fixture::new(false, false);
    assert_eq!(
        f.construct(None, None).unwrap_err().id,
        "execution.pet_ai.UNIT_CREATED_BY_SPELL_owner"
    );
    let base = f.ais.get(&f.objects, f.selected.object()).unwrap();
    assert_eq!(
        (base.follow_angle, base.follow_distance),
        (std::f32::consts::FRAC_PI_2, 1.5)
    );
    assert_eq!(base.base.attack_distance, 0.0);
    assert_eq!(
        f.construct(Some(688), None).unwrap_err().id,
        "execution.pet_ai.Unit_GetCharmInfo_owner"
    );
    let base = f.ais.get(&f.objects, f.selected.object()).unwrap();
    assert_eq!(base.base.attack_distance, 25.0);
    assert!(!base.base.melee_enabled);
    assert_eq!(base.base.visibility_distance, 166.0);
}
#[test]
fn pet_controlled_config_precedes_follow_and_unwritten_created_spell() {
    let mut f = Fixture::new(true, true);
    assert_eq!(
        f.construct(None, None).unwrap_err().id,
        "execution.pet_ai.CONFIG_BOOL_PET_ATTACK_FROM_BEHIND_owner"
    );
    let behind = PetAttackFromBehindConfig::parse(b"PetAttackFromBehind=yes").unwrap();
    assert_eq!(
        f.construct(None, Some(&behind)).unwrap_err().id,
        "execution.pet_ai.UNIT_CREATED_BY_SPELL_owner"
    );
    assert_eq!(
        f.ais
            .get(&f.objects, f.selected.object())
            .unwrap()
            .base
            .attack_angle,
        std::f32::consts::PI
    );
    assert!(f.selected.inputs().control.owner.is_none());
    f.control().creator = f.objects.get(f.target).unwrap().guid.unwrap();
    let mut owner = Dependencies::default();
    assert_eq!(
        f.construct_with(None, Some(&behind), &mut owner)
            .unwrap_err()
            .id,
        "execution.pet_ai.MotionMaster_MoveFollow_owner"
    );
    let player = f.objects.construct(1, SourceObjectKind::Player).unwrap();
    let guid = SourceGuid::without_entry(HighGuid::Player, 5);
    f.objects.assign_guid(player, 0, guid).unwrap();
    f.objects.get_mut(player).unwrap().map = f.objects.get(f.target).unwrap().map;
    f.objects.register_player_lookup(player).unwrap();
    f.objects.mark_in_world(player).unwrap();
    f.control().creator = guid;
    assert_eq!(
        f.construct_with(None, Some(&behind), &mut owner)
            .unwrap_err()
            .id,
        "execution.pet_ai.Player_RequestFollowData_owner"
    );
}
#[test]
fn pet_guard_short_circuit_constructs_and_inventories_real_runtime_state() {
    let mut f = Fixture::new(true, false);
    let ai = f.construct(Some(688), None).unwrap();
    assert!(!ai.pet().unwrap().in_combat);
    let base = f.ais.get(&f.objects, f.selected.object()).unwrap();
    assert_eq!(base.base.visibility_distance, 50.0);
    assert_eq!(base.base.attack_distance, 25.0);
    assert!(!base.base.melee_enabled);
    let inventory = ai.inventory(&f.objects, &f.ais).unwrap();
    assert_eq!(
        inventory["pet"]["restore"],
        "execution.pet_ai.f7_restore_owner"
    );
    assert_eq!(inventory["pet"]["in_combat"], false);
}
#[test]
fn pet_update_advances_timers_before_owner_and_never_selects_threat() {
    let mut f = Fixture::new(true, false);
    let mut ai = f.construct(Some(0), None).unwrap();
    let unit = f.selected.object();
    let base = &mut f.ais.get_mut(&f.objects, unit).unwrap().base;
    base.generic_timers[0].disabled = false;
    base.generic_timers[0].remaining = 100;
    base.distancing_cooldown = true;
    ai.update(
        99,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &mut Dependencies::default(),
    )
    .unwrap();
    assert_eq!(
        f.ais.get(&f.objects, unit).unwrap().base.generic_timers[0].remaining,
        1
    );
    ai.update(
        1,
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &mut Dependencies::default(),
    )
    .unwrap();
    let base = &f.ais.get(&f.objects, unit).unwrap().base;
    assert!(!base.distancing_cooldown);
    assert!(base.generic_timers[0].disabled);
}
#[test]
fn pet_update_script_then_actions_then_charm_read_are_ordered() {
    let mut f = Fixture::new(true, false);
    let unit = f.selected.object();
    f.control().creator = f.objects.get(f.target).unwrap().guid.unwrap();
    let mut owner = Dependencies::default();
    let mut state = f.authored();
    state.in_combat = true;
    f.ais.get_mut(&f.objects, unit).unwrap().base.combat_script = true;
    state
        .update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
        .unwrap();
    assert!(!state.in_combat);
    f.ais.get_mut(&f.objects, unit).unwrap().base.combat_script = false;
    assert_eq!(
        state
            .update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
            .unwrap_err()
            .id,
        "execution.pet_ai.Unit_GetCharmInfo_owner"
    );
    let mut f = Fixture::new(true, true);
    let mut state = f.authored();
    f.control().creator = f.objects.get(f.target).unwrap().guid.unwrap();
    assert_eq!(
        state
            .update(1, &f.objects, &mut f.units, &mut f.ais, &mut owner)
            .unwrap_err()
            .id,
        "execution.pet_ai.Pet_HasActionsDisabled_owner"
    );
}
#[test]
fn pet_attack_uses_unit_attack_and_sets_combat_only_after_motion() {
    let mut f = Fixture::new(true, false);
    let mut state = f.authored();
    let mut owner = Dependencies {
        refuse_chase: true,
        ..Default::default()
    };
    assert_eq!(
        state
            .attack_start(
                Some(f.target),
                &f.objects,
                &mut f.units,
                &mut f.ais,
                &mut owner
            )
            .unwrap_err()
            .id,
        "execution.unit_ai.f5_05_motion_owner"
    );
    assert!(!state.in_combat);
    assert_eq!(
        f.units.get(&f.objects, state.unit).unwrap().victim,
        Some(f.target)
    );
    assert!(f
        .units
        .get(&f.objects, f.target)
        .unwrap()
        .attackers
        .contains(&state.unit));
    assert!(owner.calls.iter().any(|call| matches!(
        call,
        UnitAiCall::Chase {
            target_change: false,
            ..
        }
    )));
    assert!(!owner.calls.iter().any(|call| matches!(
        call,
        UnitAiCall::AddInitialThreat(_) | UnitAiCall::PropagateCombat(_)
    )));
    owner.refuse_chase = false;
    state
        .attack_start(
            Some(f.target),
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &mut owner,
        )
        .unwrap();
    assert!(state.in_combat);
}
#[test]
fn pet_attack_disabled_read_precedes_script_and_null_checks() {
    let mut f = Fixture::new(true, true);
    f.ais
        .get_mut(&f.objects, f.selected.object())
        .unwrap()
        .base
        .combat_script = true;
    let mut state = f.authored();
    assert_eq!(
        state
            .attack_start(
                None,
                &f.objects,
                &mut f.units,
                &mut f.ais,
                &mut Dependencies::default()
            )
            .unwrap_err()
            .id,
        "execution.pet_ai.Pet_HasActionsDisabled_owner"
    );
    let mut f = Fixture::new(true, false);
    f.ais
        .get_mut(&f.objects, f.selected.object())
        .unwrap()
        .base
        .combat_script = true;
    let mut state = f.authored();
    state
        .attack_start(
            None,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &mut Dependencies::default(),
        )
        .unwrap();
    assert!(!state.in_combat);
}
#[test]
fn pet_overrides_do_not_fall_back_to_creature_ai() {
    let mut f = Fixture::new(true, false);
    let mut ai = f.construct(Some(0), None).unwrap();
    let unit = f.selected.object();
    f.ais.get_mut(&f.objects, unit).unwrap().base.generic_timers[0].remaining = 100;
    ai.just_respawned(
        &f.objects,
        &mut f.ais,
        &mut super::super::creature_tests::MissingRandom,
    )
    .unwrap();
    assert_eq!(
        f.ais.get(&f.objects, unit).unwrap().base.generic_timers[0].remaining,
        100
    );
    assert_eq!(
        ai.move_in_line_of_sight(
            f.target,
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &config(),
            &mut Dependencies::default()
        )
        .unwrap_err()
        .id,
        "execution.pet_ai.Unit_GetCharmInfo_owner"
    );
    assert_eq!(
        ai.enter_evade(
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &mut Dependencies::default()
        )
        .unwrap_err()
        .id,
        "execution.pet_ai.Unit_GetCharmInfo_owner"
    );
    assert_eq!(
        ai.attacked_by(
            Some(f.target),
            &f.objects,
            &mut f.units,
            &mut f.ais,
            &mut Dependencies::default()
        )
        .unwrap_err()
        .id,
        "execution.pet_ai.Unit_GetCharmInfo_owner"
    );
    // PetAI.cpp:593 reads a set creator GUID even with no registered follower.
    f.control().creator = f.objects.get(f.target).unwrap().guid.unwrap();
    ai.on_unsummon(&f.objects, &f.units, &f.ais).unwrap();
    ai.just_died(
        &f.objects,
        &mut f.units,
        &mut f.ais,
        &mut Dependencies::default(),
    )
    .unwrap();
}
#[derive(Default)]
struct Auras {
    values: BTreeMap<(ObjectHandle, u32, u8), u32>,
    reads: Vec<(ObjectHandle, u32, u8)>,
}
impl UnitAiExecutionOwner for Auras {}
impl PetAuraExecutionOwner for Auras {
    fn pet_has_aura(&mut self, unit: ObjectHandle, spell: u32, effect: u8) -> Result<bool> {
        self.reads.push((unit, spell, effect));
        Ok(self.values.contains_key(&(unit, spell, effect)))
    }
    fn pet_aura_stack(
        &mut self,
        unit: ObjectHandle,
        spell: u32,
        effect: u8,
    ) -> Result<Option<u32>> {
        self.reads.push((unit, spell, effect));
        Ok(self.values.get(&(unit, spell, effect)).copied())
    }
}
#[test]
fn pet_area_aura_targets_and_missing_target_follow_source_branches() {
    let f = Fixture::new(true, false);
    let state = f.authored();
    let mut t = SpellTemplate {
        id: 7,
        effect2: 35,
        effectimplicittargeta2: 1,
        ..Default::default()
    };
    let mut auras = Auras::default();
    auras.values.insert((state.unit, 7, 1), 1);
    assert!(!state.can_auto_cast_area_aura(&t, None, &mut auras).unwrap());
    assert_eq!(auras.reads, [(state.unit, 7, 1)]);
    t.effectimplicittargeta2 = 6;
    assert!(!state
        .can_auto_cast_area_aura(&t, None, &mut Dependencies::default())
        .unwrap());
    t.effect2 = 119;
    assert!(state
        .can_auto_cast_area_aura(&t, Some(f.target), &mut auras)
        .unwrap());
    assert_eq!(
        state
            .can_auto_cast_area_aura(&t, Some(f.target), &mut Dependencies::default())
            .unwrap_err()
            .id,
        "execution.pet_ai.Unit_HasAura_effect_owner"
    );
}
#[test]
fn pet_aura_stack_rules_and_null_dereference_guard_cover_both_sides() {
    let f = Fixture::new(true, false);
    let state = f.authored();
    let mut t = SpellTemplate {
        id: 7,
        effect3: 6,
        stackamount: 2,
        ..Default::default()
    };
    let mut auras = Auras::default();
    assert_eq!(
        state
            .can_auto_cast(&t, None, &f.objects, &f.units, &mut auras)
            .unwrap_err()
            .id,
        "execution.pet_ai.null_aura_target_undefined"
    );
    assert!(auras.reads.is_empty());
    auras.values.insert((f.target, 7, 2), 1);
    assert!(state
        .can_auto_cast(&t, Some(f.target), &f.objects, &f.units, &mut auras)
        .unwrap());
    auras.values.insert((f.target, 7, 2), 2);
    assert!(!state
        .can_auto_cast(&t, Some(f.target), &f.objects, &f.units, &mut auras)
        .unwrap());
    t.stackamount = 1;
    assert!(!state
        .can_auto_cast(&t, Some(f.target), &f.objects, &f.units, &mut auras)
        .unwrap());
    assert_eq!(
        state
            .can_auto_cast(
                &t,
                Some(f.target),
                &f.objects,
                &f.units,
                &mut Dependencies::default()
            )
            .unwrap_err()
            .id,
        "execution.pet_ai.Unit_HasAura_effect_owner"
    );
    t.stackamount = 2;
    assert_eq!(
        state
            .can_auto_cast(
                &t,
                Some(f.target),
                &f.objects,
                &f.units,
                &mut Dependencies::default()
            )
            .unwrap_err()
            .id,
        "execution.pet_ai.Unit_GetAura_stack_owner"
    );
}
#[test]
fn pet_cast_checks_preserve_casting_combat_master_and_target_table_order() {
    let mut f = Fixture::new(true, true);
    let state = f.authored();
    let mut t = SpellTemplate::default();
    f.control().creator = f.objects.get(f.target).unwrap().guid.unwrap();
    let mut owner = Dependencies::default();
    f.units
        .get_mut(&f.objects, state.unit)
        .unwrap()
        .casting_non_melee = true;
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut owner)
            .unwrap(),
        SpellCastResult::SpellInProgress
    );
    t.attributesex4 = 0x80;
    f.units.get_mut(&f.objects, state.unit).unwrap().flags |= UNIT_FLAG_COMBAT;
    t.attributes = 0x1000_0000;
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut owner)
            .unwrap(),
        SpellCastResult::AffectingCombat
    );
    t.attributes = 0;
    f.units.get_mut(&f.objects, state.unit).unwrap().owner = Some(f.target);
    f.objects.get_mut(f.target).unwrap().alive = false;
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut owner)
            .unwrap(),
        SpellCastResult::CasterDead
    );
    f.objects.get_mut(f.target).unwrap().alive = true;
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut owner)
            .unwrap(),
        SpellCastResult::Ok
    );
    t.effectimplicittargeta1 = 64;
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut owner)
            .unwrap_err()
            .id,
        "execution.pet_ai.SpellTargetInfoTable_index_undefined"
    );
    t.effectimplicittargeta1 = 63;
    assert_eq!(
        state
            .check_pet_cast(&t, Some(f.target), &f.objects, &f.units, &mut owner)
            .unwrap(),
        SpellCastResult::Ok
    );
    t.effectimplicittargeta1 = 6;
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut owner)
            .unwrap(),
        SpellCastResult::BadImplicitTargets
    );
    assert_eq!(
        state
            .check_pet_cast(&t, Some(f.target), &f.objects, &f.units, &mut owner)
            .unwrap_err()
            .id,
        "execution.pet_ai.IsPositiveSpell_target_owner"
    );
}
#[test]
fn pet_script_target_before_unit_target_preserves_early_success_quirk() {
    let f = Fixture::new(true, true);
    let state = f.authored();
    let t = SpellTemplate {
        effectimplicittargeta1: 7,
        effectimplicittargeta2: 6,
        ..Default::default()
    };
    assert_eq!(
        state
            .check_pet_cast(
                &t,
                Some(f.target),
                &f.objects,
                &f.units,
                &mut Dependencies::default()
            )
            .unwrap(),
        SpellCastResult::Ok
    );
    assert_eq!(
        state
            .check_pet_cast(&t, None, &f.objects, &f.units, &mut Dependencies::default())
            .unwrap(),
        SpellCastResult::BadImplicitTargets
    );
}
#[test]
fn pet_opener_refuses_before_unowned_autocast_list_or_condition() {
    assert_eq!(
        Fixture::new(true, false)
            .authored()
            .pick_spell_with_target()
            .unwrap_err()
            .id,
        "execution.pet_ai.CharmInfo_GetSpellOpener_owner"
    );
}
#[test]
fn pet_recorded_boolean_config_preserves_source_parsing() {
    for value in ["1", "true", "TRUE", "yEs"] {
        assert!(PetAttackFromBehindConfig::parse(
            format!("petattackfrombehind=\"{value}\"").as_bytes()
        )
        .unwrap()
        .enabled());
    }
    for value in ["0", "false", "anything", "2", ""] {
        assert!(!PetAttackFromBehindConfig::parse(
            format!("PetAttackFromBehind={value}").as_bytes()
        )
        .unwrap()
        .enabled());
    }
    let absent = PetAttackFromBehindConfig::parse(b"[world]\n# empty config").unwrap();
    assert!(!absent.enabled());
    assert_eq!(absent.line, None);
    let last =
        PetAttackFromBehindConfig::parse(b"PetAttackFromBehind=1\nPETATTACKFROMBEHIND=0").unwrap();
    assert!(!last.enabled());
    assert_eq!(last.line, Some(2));
    assert!(PetAttackFromBehindConfig::parse(b"broken").is_err());
}
#[test]
fn pet_catalog04_constructor_and_reach_census_or_explicit_absence() {
    struct Unbound;
    impl UnitAiExecutionOwner for Unbound {}
    impl BuiltinAiExecutionOwner for Unbound {}
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP PetAI census: bind BENILLA_F5_AI_CATALOG, BENILLA_F5_SCRIPT_REPORT, BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let report = std::env::var_os("BENILLA_F5_SCRIPT_REPORT").expect("script report required");
    let path = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let approved =
        Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap();
    let config = EventAiLoaderConfig::recorded(Path::new(&path), &approved).unwrap();
    let behind = PetAttackFromBehindConfig::recorded(Path::new(&path), &approved).unwrap();
    assert_eq!(behind.line, Some(870));
    assert!(!behind.enabled());
    let mut catalog =
        crate::native::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
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
    let mut counts = BTreeMap::<String, (usize, usize)>::new();
    let mut selected_count = (0, 0);
    let mut completed = 0;
    let mut update_reach = 0;
    let mut entries = Vec::new();
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
        if selected.executable().builtin_kind() != Some(BuiltinCreatureAi::Pet) {
            continue;
        }
        selected_count.0 += 1;
        selected_count.1 += spawns.get(&t.entry).copied().unwrap_or(0);
        entries.push(t.entry);
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(&t))
            .unwrap();
        let mut ais = SourceCreatureAis::default();
        let result = (|| {
            let base = SourceCreatureAi::construct(
                &objects,
                unit,
                &mut units,
                &t,
                false,
                0,
                &config,
                &mut Random::new(),
            )?;
            ais.install(&objects, &mut units, base, &mut SourceCombat::default())?;
            // Ordinary raw-template construction does not own UNIT_CREATED_BY_SPELL.
            SourceBuiltinAi::construct_pet(
                selected,
                &objects,
                PetAiConstructionOwners {
                    units: &mut units,
                    ais: &mut ais,
                    created_by_spell: None,
                    config: &config,
                    attack_from_behind: Some(&behind),
                    execution: &mut Unbound,
                },
            )
        })();
        let reason = match result {
            Ok(mut ai) => {
                completed += 1;
                update_reach += 1;
                ai.update(100, &objects, &mut units, &mut ais, &mut Unbound)
                    .map_or_else(|e| e.id, |_| "updated".into())
            }
            Err(e) => e.id,
        };
        let count = counts.entry(reason).or_default();
        count.0 += 1;
        count.1 += spawns.get(&t.entry).copied().unwrap_or(0);
    }
    assert_eq!(completed, 0);
    assert_eq!(update_reach, 0);
    assert_eq!(
        counts.values().map(|v| v.0).sum::<usize>(),
        selected_count.0
    );
    println!("PetAI catalog-04 templates/raw-spawns={selected_count:?}; entries={entries:?}; constructors-complete={completed}; update-reach={update_reach}; stops={counts:?}; aura/CheckPetCast/SpellStart=ported-not-reached; UB-null-aura-target stock reach=0; UB-target-table-index stock reach=0; basis=ordinary uncontrolled raw-template projection, no live summon/charm/F1 acceptance");
}

#[test]
fn pet_live_empty_guids_advance_update_timers_and_constructor() {
    struct Unbound;
    impl UnitAiExecutionOwner for Unbound {}
    impl BuiltinAiExecutionOwner for Unbound {}
    let mut f = Fixture::new(true, false);
    let mut ai = f.construct(Some(0), None).unwrap();
    let unit = f.selected.object();
    f.ais.get_mut(&f.objects, unit).unwrap().base.generic_timers[0] = SourceAiTimer {
        id: 2000,
        remaining: 100,
        disabled: false,
    };
    // PetAI.cpp:146 falls through to UpdateTimers(:154) for null charmer and creator.
    ai.update(1, &f.objects, &mut f.units, &mut f.ais, &mut Unbound)
        .unwrap();
    assert_eq!(
        f.ais.get(&f.objects, unit).unwrap().base.generic_timers[0].remaining,
        99
    );
    let mut f = Fixture::new(true, true);
    let behind = PetAttackFromBehindConfig::parse(b"PetAttackFromBehind=1").unwrap();
    assert_eq!(
        f.construct_with(None, Some(&behind), &mut Unbound)
            .unwrap_err()
            .id,
        "execution.pet_ai.UNIT_CREATED_BY_SPELL_owner"
    );
    assert_eq!(
        f.ais
            .get(&f.objects, f.selected.object())
            .unwrap()
            .base
            .attack_angle,
        std::f32::consts::PI
    );
    assert_eq!(
        Unbound.pet_is_loading(unit).unwrap_err().id,
        "execution.pet_ai.Unit_GetCharmer_GetCreator_out_of_world_loading_owner"
    );
    assert_eq!(crate::native::NATIVE_FORMAT_VERSION, 91);
}

#[test]
fn pet_master_guid_lookup_prioritizes_charmer_then_creator_and_keeps_missing_fields() {
    let mut f = Fixture::new(true, false);
    let unit = f.selected.object();
    let charmer = f.objects.get(f.target).unwrap().guid.unwrap();
    let creator = f.objects.construct(1, SourceObjectKind::Creature).unwrap();
    let creator_guid = SourceGuid::with_entry(HighGuid::Creature, 902, 3);
    f.objects.assign_guid(creator, 0, creator_guid).unwrap();
    f.objects.get_mut(creator).unwrap().map = f.objects.get(unit).unwrap().map;
    f.objects.add_to_world(creator).unwrap();
    *f.control() = SourceControlGuids {
        charmer,
        creator: creator_guid,
    };
    let read = |f: &Fixture| {
        f.units
            .get(&f.objects, unit)
            .unwrap()
            .control_guids
            .get_master(&f.objects, unit, None, unbound_pet_loading)
            .unwrap()
    };
    assert_eq!(read(&f).unit(), Some(f.target));
    assert_eq!(read(&f).creator, None);
    f.objects.remove_from_world(f.target).unwrap();
    assert_eq!(read(&f).unit(), Some(creator));
    assert_eq!(
        read(&f),
        SourceMasterLookup {
            charmer: SourceControlLookup::LoggedNull(SourceControlNotice::MissingUnit {
                source_line: 6602
            }),
            creator: Some(SourceControlLookup::Found(creator)),
        }
    );
    f.objects.add_to_world(f.target).unwrap();
    f.objects.get_mut(f.target).unwrap().map = Some(MapIdentity {
        map: 1,
        instance: 0,
        creation_generation: 2,
    });
    assert_eq!(read(&f).unit(), Some(creator));
    f.control().creator = SourceGuid::default();
    assert_eq!(read(&f).unit(), None);
    assert!(f.control().has_master());
    assert_eq!(f.control().master_guid(), charmer);
    f.control().charmer = SourceGuid::default();
    assert_eq!(read(&f).unit(), None);
    assert!(!f.control().has_master());
    assert_eq!(f.control().master_guid().raw(), 0);
    // Cached control handles must not stand in for absent source GUID fields.
    f.units.get_mut(&f.objects, unit).unwrap().owner = Some(creator);
    f.units.get_mut(&f.objects, unit).unwrap().charmer = Some(f.target);
    assert_eq!(read(&f).unit(), None);
    let mut owner = Dependencies::default();
    let mut threats = SourceThreats::default();
    let mut wrapped = SourceAiDependencies {
        threats: &mut threats,
        owner: &mut owner,
    };
    assert_eq!(
        f.units
            .get(&f.objects, unit)
            .unwrap()
            .control_guids
            .get_master(&f.objects, unit, None, |pet| wrapped.pet_is_loading(pet))
            .unwrap()
            .unit(),
        None
    );
}

#[test]
fn pet_master_out_of_world_preserves_loading_only_global_lookup_branch() {
    let mut f = Fixture::new(true, false);
    let unit = f.selected.object();
    f.objects.remove_from_world(unit).unwrap();
    f.control().charmer = f.objects.get(f.target).unwrap().guid.unwrap();
    let mut owner = Dependencies::default();
    let read = |f: &Fixture, owner: &mut Dependencies| {
        let unit = f.selected.object();
        f.units
            .get(&f.objects, unit)
            .unwrap()
            .control_guids
            .get_master(&f.objects, unit, None, |pet| owner.pet_is_loading(pet))
    };
    assert_eq!(
        read(&f, &mut owner).unwrap(),
        SourceMasterLookup {
            charmer: SourceControlLookup::LoggedNull(SourceControlNotice::AccessorOutsideWorld {
                source_line: 6594
            }),
            creator: Some(SourceControlLookup::EmptyGuid),
        }
    );
    let mut f = Fixture::new(true, true);
    let unit = f.selected.object();
    f.objects.remove_from_world(unit).unwrap();
    let guid = SourceGuid::without_entry(HighGuid::Player, 5);
    f.control().charmer = guid;
    assert_eq!(
        read(&f, &mut owner).unwrap_err().id,
        "execution.pet_ai.Unit_GetCharmer_GetCreator_out_of_world_loading_owner"
    );
    owner.loading = Some(false);
    assert_eq!(read(&f, &mut owner).unwrap().unit(), None);
    owner.loading = Some(true);
    assert_eq!(
        read(&f, &mut owner).unwrap().charmer,
        SourceControlLookup::LoadingPlayerMissing
    );
    assert_eq!(f.objects.find_player(SourceGuid::default()).unwrap(), None);
    assert_eq!(f.objects.find_player(guid).unwrap(), None);
    let player = f.objects.construct(1, SourceObjectKind::Player).unwrap();
    f.objects.assign_guid(player, 0, guid).unwrap();
    f.objects.get_mut(player).unwrap().map = Some(MapIdentity {
        map: 1,
        instance: 7,
        creation_generation: 2,
    });
    f.objects.register_player_lookup(player).unwrap();
    assert_eq!(f.objects.find_player(guid).unwrap(), None);
    assert_eq!(read(&f, &mut owner).unwrap().unit(), None);
    f.objects.mark_in_world(player).unwrap();
    assert_eq!(f.objects.find_player(guid).unwrap(), Some(player));
    assert_eq!(read(&f, &mut owner).unwrap().unit(), Some(player));
    assert_eq!(
        f.objects
            .find_player(SourceGuid::without_entry(HighGuid::Player, 6))
            .unwrap(),
        None
    );
}

#[test]
fn pet_cast_refuses_if_second_master_read_becomes_null() {
    struct Loading(std::array::IntoIter<bool, 2>);
    impl UnitAiExecutionOwner for Loading {
        fn pet_is_loading(&mut self, _: ObjectHandle) -> Result<bool> {
            Ok(self.0.next().unwrap())
        }
    }
    let mut f = Fixture::new(true, true);
    let pet = f.authored();
    f.objects.remove_from_world(pet.unit).unwrap();
    let guid = SourceGuid::without_entry(HighGuid::Player, 7);
    f.control().charmer = guid;
    let player = f.objects.construct(1, SourceObjectKind::Player).unwrap();
    f.objects.assign_guid(player, 0, guid).unwrap();
    f.objects.get_mut(player).unwrap().alive = false;
    f.objects.register_player_lookup(player).unwrap();
    f.objects.mark_in_world(player).unwrap();
    let check = |loading| {
        pet.check_pet_cast(
            &SpellTemplate::default(),
            None,
            &f.objects,
            &f.units,
            &mut Loading(loading),
        )
    };
    assert_eq!(
        check([true, true].into_iter()).unwrap(),
        SpellCastResult::CasterDead
    );
    assert_eq!(
        check([true, false].into_iter()).unwrap_err().id,
        "execution.pet_ai.PetAI_494.master_null_undefined"
    );
}
