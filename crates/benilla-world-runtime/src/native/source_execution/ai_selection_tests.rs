// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored selection operands, not executed AI bodies or sealed F1 evidence.
use super::*;
use std::collections::BTreeMap;

fn input() -> CreatureAiInputs {
    CreatureAiInputs {
        effective_entry: 71,
        ai_name: String::new(),
        script_name: String::new(),
        control: CreatureAiControl::default(),
        guard: false,
        no_aggro_on_sight: false,
        neutral_to_all: false,
    }
}
fn object(objects: &mut SourceObjects, input: &CreatureAiInputs) -> ObjectHandle {
    objects
        .construct(
            1,
            if input.control.pet.is_some() {
                SourceObjectKind::Pet
            } else {
                SourceObjectKind::Creature
            },
        )
        .unwrap()
}
fn script(nonnull: bool, registration: u64) -> ScriptDevAiBinding {
    ScriptDevAiBinding {
        script_id: 7,
        name: "authored_script".into(),
        registration,
        operation: registration.to_string(),
        source_file: "authored.cpp".into(),
        source_line: 71,
        source_function: "AddSC_authored".into(),
        registry_sha256: Sha256Id::of(b"authored registry"),
        get_ai_nonnull: nonnull,
    }
}
fn scripted_registry(nonnull: bool) -> SourceAiRegistry {
    let mut registry = AiRegistryBuilder::builtin();
    registry.load_scripts([script(nonnull, 1)]).unwrap();
    registry.freeze()
}
fn check(
    registry: &SourceAiRegistry,
    objects: &SourceObjects,
    object: ObjectHandle,
    input: CreatureAiInputs,
    branch: AiSelectionBranch,
    kind: Option<BuiltinCreatureAi>,
    missing: &str,
) -> SelectedCreatureAi {
    let selected = registry.select(objects, object, input).unwrap();
    assert_eq!(selected.branch(), branch);
    assert_eq!(selected.executable().builtin_kind(), kind);
    assert_eq!(selected.constructor_refusal(objects).unwrap().id, missing);
    selected
}
#[test]
fn absent_name_selects_event_ai_then_refuses_its_constructor() {
    let mut objects = SourceObjects::default();
    let object = object(&mut objects, &input());
    check(
        &SourceAiRegistry::builtin(),
        &objects,
        object,
        input(),
        AiSelectionBranch::Permit,
        Some(BuiltinCreatureAi::Event),
        "execution.ai.event_ai_constructor_owner",
    );
}
#[test]
fn every_eligible_precedence_pair_selects_the_earlier_source_branch() {
    // Selector.cpp:40-67. Guardian pets remain eligible for ScriptDev.
    let branches = [
        AiSelectionBranch::ScriptDev,
        AiSelectionBranch::Guardian,
        AiSelectionBranch::Charm,
        AiSelectionBranch::Totem,
        AiSelectionBranch::ExplicitName,
        AiSelectionBranch::Guard,
        AiSelectionBranch::Permit,
    ];
    let registry = scripted_registry(true);
    let mut pairs = 0;
    for (i, &earlier) in branches.iter().enumerate() {
        for &later in &branches[i + 1..] {
            let mut objects = SourceObjects::default();
            let charmer = objects.construct(1, SourceObjectKind::Player).unwrap();
            let mut input = input();
            for branch in [later, earlier] {
                match branch {
                    AiSelectionBranch::ScriptDev => input.script_name = "authored_script".into(),
                    AiSelectionBranch::Guardian => {
                        input.control.pet = Some(CreaturePetKind::Guardian)
                    }
                    AiSelectionBranch::Charm => input.control.charmer = Some(charmer),
                    AiSelectionBranch::Totem => input.control.totem = true,
                    AiSelectionBranch::ExplicitName => input.ai_name = "PossessedAI".into(),
                    AiSelectionBranch::Guard => input.guard = true,
                    AiSelectionBranch::Permit => {}
                    AiSelectionBranch::ControlledPet => unreachable!(),
                }
            }
            let object = object(&mut objects, &input);
            let (kind, missing) = match earlier {
                AiSelectionBranch::ScriptDev => {
                    (None, "execution.ai.scriptdev_get_ai_constructor_owner")
                }
                AiSelectionBranch::Guardian => (
                    Some(BuiltinCreatureAi::Guardian),
                    "execution.ai.guardian_constructor_owner",
                ),
                AiSelectionBranch::Charm => (
                    Some(BuiltinCreatureAi::Pet),
                    "execution.ai.pet_constructor_owner",
                ),
                AiSelectionBranch::Totem => (
                    Some(BuiltinCreatureAi::Totem),
                    "execution.ai.totem_constructor_owner",
                ),
                AiSelectionBranch::ExplicitName => (
                    Some(BuiltinCreatureAi::Possessed),
                    "execution.ai.possessed_constructor_owner",
                ),
                AiSelectionBranch::Guard => (
                    Some(BuiltinCreatureAi::Guard),
                    "execution.ai.guard_constructor_owner",
                ),
                _ => unreachable!(),
            };
            check(&registry, &objects, object, input, earlier, kind, missing);
            pairs += 1;
        }
    }
    assert_eq!(pairs, 21);
}
#[test]
fn controlled_pets_skip_scripts_unless_charmed_and_require_player_control_for_pet_ai() {
    for (player_controlled, charmed, nonnull, expected, kind, missing) in [
        (
            true,
            false,
            true,
            AiSelectionBranch::ControlledPet,
            Some(BuiltinCreatureAi::Pet),
            "execution.ai.pet_constructor_owner",
        ),
        (
            false,
            false,
            true,
            AiSelectionBranch::Guardian,
            Some(BuiltinCreatureAi::Guardian),
            "execution.ai.guardian_constructor_owner",
        ),
        (
            true,
            true,
            true,
            AiSelectionBranch::ScriptDev,
            None,
            "execution.ai.scriptdev_get_ai_constructor_owner",
        ),
        (
            false,
            true,
            true,
            AiSelectionBranch::ScriptDev,
            None,
            "execution.ai.scriptdev_get_ai_constructor_owner",
        ),
        (
            true,
            true,
            false,
            AiSelectionBranch::ControlledPet,
            Some(BuiltinCreatureAi::Pet),
            "execution.ai.pet_constructor_owner",
        ),
        (
            false,
            true,
            false,
            AiSelectionBranch::Guardian,
            Some(BuiltinCreatureAi::Guardian),
            "execution.ai.guardian_constructor_owner",
        ),
    ] {
        let mut objects = SourceObjects::default();
        let charmer = objects.construct(1, SourceObjectKind::Player).unwrap();
        let mut input = input();
        input.script_name = "authored_script".into();
        input.ai_name = "PossessedAI".into();
        input.guard = true;
        input.control = CreatureAiControl {
            pet: Some(CreaturePetKind::Controlled),
            player_controlled,
            owner: Some(charmer),
            charmer: charmed.then_some(charmer),
            totem: true,
            ..Default::default()
        };
        let object = object(&mut objects, &input);
        check(
            &scripted_registry(nonnull),
            &objects,
            object,
            input,
            expected,
            kind,
            missing,
        );
    }
}
#[test]
fn guardian_and_minipets_are_script_eligible_even_when_player_controlled() {
    for pet in [CreaturePetKind::Guardian, CreaturePetKind::Other] {
        for nonnull in [false, true] {
            let mut objects = SourceObjects::default();
            let mut input = input();
            input.script_name = "authored_script".into();
            input.control.pet = Some(pet);
            input.control.player_controlled = true;
            let object = object(&mut objects, &input);
            let (branch, kind, missing) = if nonnull {
                (
                    AiSelectionBranch::ScriptDev,
                    None,
                    "execution.ai.scriptdev_get_ai_constructor_owner",
                )
            } else {
                (
                    AiSelectionBranch::Guardian,
                    Some(BuiltinCreatureAi::Guardian),
                    "execution.ai.guardian_constructor_owner",
                )
            };
            check(
                &scripted_registry(nonnull),
                &objects,
                object,
                input,
                branch,
                kind,
                missing,
            );
        }
    }
}
#[test]
fn charmed_temporary_summons_continue_to_totem_name_guard_or_permit() {
    for (totem, name, guard, branch, kind, missing) in [
        (
            true,
            "PossessedAI",
            true,
            AiSelectionBranch::Totem,
            BuiltinCreatureAi::Totem,
            "execution.ai.totem_constructor_owner",
        ),
        (
            false,
            "PossessedAI",
            true,
            AiSelectionBranch::ExplicitName,
            BuiltinCreatureAi::Possessed,
            "execution.ai.possessed_constructor_owner",
        ),
        (
            false,
            "",
            true,
            AiSelectionBranch::Guard,
            BuiltinCreatureAi::Guard,
            "execution.ai.guard_constructor_owner",
        ),
        (
            false,
            "",
            false,
            AiSelectionBranch::Permit,
            BuiltinCreatureAi::Event,
            "execution.ai.event_ai_constructor_owner",
        ),
    ] {
        let mut objects = SourceObjects::default();
        let charmer = objects.construct(1, SourceObjectKind::Player).unwrap();
        let mut input = input();
        input.ai_name = name.into();
        input.guard = guard;
        input.control.charmer = Some(charmer);
        input.control.temporary_summon = true;
        input.control.totem = totem;
        let object = object(&mut objects, &input);
        check(
            &SourceAiRegistry::builtin(),
            &objects,
            object,
            input,
            branch,
            Some(kind),
            missing,
        );
    }
}
#[test]
fn missing_explicit_name_does_not_resume_guard_or_permit_and_nullai_is_a_real_binding() {
    let mut objects = SourceObjects::default();
    let object = object(&mut objects, &input());
    for guard in [false, true] {
        let mut named = input();
        named.guard = guard;
        named.ai_name = "missing".into();
        let selected = check(
            &SourceAiRegistry::builtin(),
            &objects,
            object,
            named,
            AiSelectionBranch::ExplicitName,
            Some(BuiltinCreatureAi::Null),
            "execution.ai.null_constructor_owner",
        );
        assert_eq!(selected.executable(), &AiExecutable::NullFallback);
    }
    let mut named = input();
    named.ai_name = "NullAI".into();
    let selected = check(
        &SourceAiRegistry::builtin(),
        &objects,
        object,
        named,
        AiSelectionBranch::ExplicitName,
        Some(BuiltinCreatureAi::Null),
        "execution.ai.null_constructor_owner",
    );
    let AiExecutable::Builtin(binding) = selected.executable() else {
        panic!("registered NullAI was not selected")
    };
    assert_eq!(binding.key, "NullAI");
    assert_eq!(binding.registration, 1);
}
#[test]
fn permit_ties_keep_the_lexicographically_first_factory_not_registration_order() {
    let mut registry = AiRegistryBuilder::builtin();
    registry
        .register("ZEvent".into(), BuiltinCreatureAi::Event, false)
        .unwrap();
    registry
        .register("AEvent".into(), BuiltinCreatureAi::Event, false)
        .unwrap();
    let registry = registry.freeze();
    for (no_aggro, neutral) in [(false, false), (false, true), (true, false), (true, true)] {
        let mut objects = SourceObjects::default();
        let mut input = input();
        input.no_aggro_on_sight = no_aggro;
        input.neutral_to_all = neutral;
        let object = object(&mut objects, &input);
        let selected = check(
            &registry,
            &objects,
            object,
            input,
            AiSelectionBranch::Permit,
            Some(BuiltinCreatureAi::Event),
            "execution.ai.event_ai_constructor_owner",
        );
        let AiExecutable::Builtin(binding) = selected.executable() else {
            panic!("missing factory")
        };
        assert_eq!((binding.key.as_str(), binding.registration), ("AEvent", 9));
    }
}
#[test]
fn no_permitted_factories_use_direct_null_fallback_including_an_empty_registry() {
    for possessed in [false, true] {
        let mut registry = AiRegistryBuilder::default();
        if possessed {
            registry
                .register("PossessedAI".into(), BuiltinCreatureAi::Possessed, false)
                .unwrap();
        }
        let mut objects = SourceObjects::default();
        let object = object(&mut objects, &input());
        let selected = check(
            &registry.freeze(),
            &objects,
            object,
            input(),
            AiSelectionBranch::Permit,
            Some(BuiltinCreatureAi::Null),
            "execution.ai.null_constructor_owner",
        );
        assert_eq!(selected.executable(), &AiExecutable::NullFallback);
    }
}
#[test]
fn null_script_slot_and_absent_registration_continue_but_nonnull_never_bypasses_constructor() {
    for name in ["authored_script", "unregistered_script"] {
        for nonnull in [false, true] {
            let mut objects = SourceObjects::default();
            let object = object(&mut objects, &input());
            let mut input = input();
            input.script_name = name.into();
            input.ai_name = "EventAI".into();
            let (branch, kind, missing) = if nonnull && name == "authored_script" {
                (
                    AiSelectionBranch::ScriptDev,
                    None,
                    "execution.ai.scriptdev_get_ai_constructor_owner",
                )
            } else {
                (
                    AiSelectionBranch::ExplicitName,
                    Some(BuiltinCreatureAi::Event),
                    "execution.ai.event_ai_constructor_owner",
                )
            };
            check(
                &scripted_registry(nonnull),
                &objects,
                object,
                input,
                branch,
                kind,
                missing,
            );
        }
    }
}
#[test]
fn unavailable_script_registry_is_not_verified_absence_but_suppressed_pet_scripts_need_no_registry()
{
    let mut objects = SourceObjects::default();
    let mut input = input();
    input.script_name = "unknown".into();
    let creature = object(&mut objects, &input);
    assert_eq!(
        SourceAiRegistry::builtin()
            .select(&objects, creature, input.clone())
            .unwrap_err()
            .id,
        "execution.ai.scriptdev_registry_owner"
    );
    input.control.pet = Some(CreaturePetKind::Controlled);
    input.control.player_controlled = true;
    let pet = object(&mut objects, &input);
    check(
        &SourceAiRegistry::builtin(),
        &objects,
        pet,
        input,
        AiSelectionBranch::ControlledPet,
        Some(BuiltinCreatureAi::Pet),
        "execution.ai.pet_constructor_owner",
    );
}
#[test]
fn builtin_duplicates_reject_by_default_and_explicit_replacement_does_not_mutate_prior_identity() {
    let mut registry = AiRegistryBuilder::builtin();
    assert!(!registry
        .register("EventAI".into(), BuiltinCreatureAi::Null, false)
        .unwrap());
    let mut objects = SourceObjects::default();
    let object = object(&mut objects, &input());
    let original = registry
        .clone()
        .freeze()
        .select(&objects, object, input())
        .unwrap();
    assert!(registry
        .register("EventAI".into(), BuiltinCreatureAi::Null, true)
        .unwrap());
    let replaced = check(
        &registry.freeze(),
        &objects,
        object,
        input(),
        AiSelectionBranch::Permit,
        Some(BuiltinCreatureAi::Null),
        "execution.ai.null_constructor_owner",
    );
    let AiExecutable::Builtin(binding) = replaced.executable() else {
        panic!("missing replacement")
    };
    assert_eq!((binding.key.as_str(), binding.registration), ("EventAI", 8));
    assert_eq!(
        original.executable().builtin_kind(),
        Some(BuiltinCreatureAi::Event)
    );
    assert_eq!(
        original.constructor_refusal(&objects).unwrap().id,
        "execution.ai.event_ai_constructor_owner"
    );
}
#[test]
fn scriptdev_duplicate_ids_overwrite_and_zero_id_is_ignored() {
    let mut registry = AiRegistryBuilder::builtin();
    let mut ignored = script(true, 3);
    ignored.script_id = 0;
    registry
        .load_scripts([script(false, 1), script(true, 2), ignored])
        .unwrap();
    let mut objects = SourceObjects::default();
    let object = object(&mut objects, &input());
    let mut input = input();
    input.script_name = "authored_script".into();
    let selected = check(
        &registry.freeze(),
        &objects,
        object,
        input,
        AiSelectionBranch::ScriptDev,
        None,
        "execution.ai.scriptdev_get_ai_constructor_owner",
    );
    let AiExecutable::ScriptDevGetAi(binding) = selected.executable() else {
        panic!("script was bypassed")
    };
    assert_eq!(binding.registration, 2);
}
#[test]
fn stale_creature_owner_and_charmer_generations_refuse_before_constructor_dispatch() {
    for stale in [0, 1, 2] {
        let mut objects = SourceObjects::default();
        let creature = object(&mut objects, &input());
        let owner = objects.construct(1, SourceObjectKind::Player).unwrap();
        let charmer = objects.construct(1, SourceObjectKind::Player).unwrap();
        let mut input = input();
        input.control.owner = Some(owner);
        input.control.charmer = Some(charmer);
        let selected = SourceAiRegistry::builtin()
            .select(&objects, creature, input.clone())
            .unwrap();
        assert_eq!(
            selected.constructor_refusal(&objects).unwrap().id,
            "execution.ai.pet_constructor_owner"
        );
        let mut cells = SourceCells::new(MapIdentity {
            map: 0,
            instance: 0,
            creation_generation: 1,
        })
        .unwrap();
        objects
            .destroy([creature, owner, charmer][stale], &mut cells)
            .unwrap();
        let replacement = objects.construct(1, SourceObjectKind::Creature).unwrap();
        assert!(replacement.construction > charmer.construction);
        assert_eq!(
            selected.constructor_refusal(&objects).unwrap_err().id,
            "execution.stale_object_handle"
        );
        assert_eq!(
            SourceAiRegistry::builtin()
                .select(&objects, creature, input)
                .unwrap_err()
                .id,
            "execution.stale_object_handle"
        );
    }
}
#[test]
fn inventory_retains_effective_entry_control_and_spawn_without_callback_addresses() {
    let mut objects = SourceObjects::default();
    let creature = object(&mut objects, &input());
    let mut input = input();
    input.effective_entry = 913;
    input.script_name = "authored_script".into();
    input.control.temporary_summon = true;
    let selected = check(
        &scripted_registry(true),
        &objects,
        creature,
        input.clone(),
        AiSelectionBranch::ScriptDev,
        None,
        "execution.ai.scriptdev_get_ai_constructor_owner",
    );
    assert_eq!(selected.inputs(), &input);
    assert_eq!(selected.object(), creature);
    let inventory = serde_json::to_value(&selected).unwrap();
    assert_eq!(inventory["version"], 1);
    assert_eq!(inventory["inputs"]["effective_entry"], 913);
    assert_eq!(inventory["inputs"]["control"]["temporary_summon"], true);
    assert_eq!(inventory["executable"]["ScriptDevGetAi"]["operation"], "1");
    assert!(inventory["executable"]["ScriptDevGetAi"]
        .get("address")
        .is_none());
}
#[test]
fn retained_catalog04_all_template_selection_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP catalog-04 CreatureAI selection census: set BENILLA_F5_AI_CATALOG and BENILLA_F5_SCRIPT_REPORT");
        return;
    };
    let report = std::env::var_os("BENILLA_F5_SCRIPT_REPORT")
        .expect("explicit catalog requires ScriptDev report");
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    assert_eq!(
        catalog.report()["catalog_sha256"],
        "757dcfb85e2b2a0de6571b0fe981c214fa57a4e2e546968ca61171113f379bd6"
    );
    let mut registry = AiRegistryBuilder::builtin();
    registry
        .load_catalog_scripts(&catalog, Path::new(&report))
        .unwrap();
    let registry = registry.freeze();
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    let rows = catalog_creature_ai_census_inputs(&mut catalog).unwrap();
    let mut objects = SourceObjects::default();
    let creature = object(&mut objects, &input());
    let mut kinds = BTreeMap::new();
    let mut refusals = BTreeMap::new();
    let mut count = 0;
    for row in rows {
        assert_eq!(row.ordinal, count);
        let neutral = factions.neutral_to_all(row.faction);
        let selected = registry
            .select(
                &objects,
                creature,
                row.inputs(CreatureAiControl::default(), neutral),
            )
            .unwrap();
        let kind = match selected.executable() {
            AiExecutable::Builtin(binding) => binding.kind.key(),
            AiExecutable::NullFallback => "NullCreatureAI fallback",
            AiExecutable::ScriptDevGetAi(_) => "ScriptDev GetAI",
        };
        *kinds.entry(kind).or_insert(0usize) += 1;
        *refusals
            .entry(selected.constructor_refusal(&objects).unwrap().id)
            .or_insert(0usize) += 1;
        count += 1;
    }
    assert!(count > 0);
    assert_eq!(kinds.values().sum::<usize>(), count as usize);
    assert_eq!(refusals.values().sum::<usize>(), count as usize);
    assert!(catalog_creature_ai_template(&mut catalog, u32::MAX)
        .unwrap()
        .is_none());
    let vermin = catalog_creature_ai_template(&mut catalog, 6)
        .unwrap()
        .unwrap();
    assert_eq!(vermin.ai_name, "EventAI");
    println!("catalog-04 CreatureAI census: templates={count}, selected={kinds:?}, refused={count}, reasons={refusals:?}; raw template alternatives as ordinary uncontrolled creatures, not live-spawn or sealed F1 acceptance");
}

#[test]
fn every_registered_explicit_factory_preserves_its_executable_and_exact_missing_owner() {
    // CreatureAIRegistry.cpp:37-43, including explicit PossessedAI and NullAI.
    let mut objects = SourceObjects::default();
    let object = object(&mut objects, &input());
    for (name, kind, missing) in [
        (
            "NullAI",
            BuiltinCreatureAi::Null,
            "execution.ai.null_constructor_owner",
        ),
        (
            "GuardAI",
            BuiltinCreatureAi::Guard,
            "execution.ai.guard_constructor_owner",
        ),
        (
            "PetAI",
            BuiltinCreatureAi::Pet,
            "execution.ai.pet_constructor_owner",
        ),
        (
            "TotemAI",
            BuiltinCreatureAi::Totem,
            "execution.ai.totem_constructor_owner",
        ),
        (
            "EventAI",
            BuiltinCreatureAi::Event,
            "execution.ai.event_ai_constructor_owner",
        ),
        (
            "GuardianAI",
            BuiltinCreatureAi::Guardian,
            "execution.ai.guardian_constructor_owner",
        ),
        (
            "PossessedAI",
            BuiltinCreatureAi::Possessed,
            "execution.ai.possessed_constructor_owner",
        ),
    ] {
        let mut input = input();
        input.ai_name = name.into();
        let selected = check(
            &SourceAiRegistry::builtin(),
            &objects,
            object,
            input,
            AiSelectionBranch::ExplicitName,
            Some(kind),
            missing,
        );
        let AiExecutable::Builtin(binding) = selected.executable() else {
            panic!("missing named factory")
        };
        assert_eq!(binding.key, name);
    }
}
