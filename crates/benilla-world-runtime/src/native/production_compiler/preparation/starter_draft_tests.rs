// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::path::PathBuf;

fn compiler<'a>(source: &'a Value, source_sha: &'a str, dbcs: &'a Tables) -> Compiler<'a> {
    Compiler {
        source,
        source_sha,
        dbcs,
        request: ProductionRequest {
            race: 1,
            class: 8,
            gender: 0,
        },
        issues: vec![],
        dependencies: BTreeSet::new(),
        selected: BTreeMap::new(),
        binding_bytes: 0,
    }
}
fn candidate(c: &Compiler<'_>, report: &Value) -> init::CapturedStarterLearning {
    let skills = &report["player"]["skills"];
    learning_numeric::candidate(
        c,
        &skills["dependency_closure"],
        &report["spells"],
        &skills["loader_plan"],
    )
    .unwrap()
    .1
    .expect("checked numeric capture")
}
fn casts(plan: &init::initial_cast_phases::InitialCastPlan) -> Vec<u32> {
    plan.occurrences()
        .iter()
        .map(|cast| cast.row().id)
        .collect()
}

#[test]
fn diagnostic_errors_retain_exact_reducer_or_input_failure() {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../../../../scripts/native_starter/production_verifier_cases.json"
    ))
    .unwrap();
    let fixture = corpus["fixtures"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["name"] == "numeric")
        .unwrap();
    let dbcs: Tables = fixture["dbc_hex"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(name, value)| {
            let bytes = value
                .as_str()
                .unwrap()
                .as_bytes()
                .as_chunks::<2>()
                .0
                .iter()
                .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
                .collect();
            (name.clone(), dbc::Table::parse(name, bytes).unwrap())
        })
        .collect();
    let sha = digest(&fixture["source"]);
    let c = compiler(&fixture["source"], &sha, &dbcs);
    let capture = candidate(&c, &fixture["preparation"]);
    let mut missing = capture.clone();
    missing.numeric = init::Rows::Missing;
    assert!(matches!(
        build(&c, &missing),
        Err(StarterDraftError::Reducer(init::InitError::Missing { .. }))
    ));
    let mut wrong_source = capture;
    wrong_source.source_identity[0] ^= 1;
    assert!(matches!(
        build(&c, &wrong_source),
        Err(StarterDraftError::Input(error)) if error.id == "starter_draft.candidate_source_binding"
    ));
    let error = StarterDraftError::Input(fail("starter_draft.test"));
    assert_eq!(error.to_string(), "starter_draft.test");
    let error = StarterDraftError::Reputation(rep::Error::Invalid("initial_faction_identity"));
    assert!(error.to_string().contains("initial_faction_identity"));
}

/// Opt-in because install-derived source/DBC rows must never enter the corpus.
#[test]
#[ignore = "requires private source, DBC, preparation and evidence paths"]
fn private_human_mage_starter_draft_and_python_parity() {
    let path = |name| PathBuf::from(std::env::var(name).expect(name));
    let source_path = path("BENILLA_STARTER_SOURCE");
    let dbc_path = path("BENILLA_STARTER_DBC");
    let preparation_path = path("BENILLA_STARTER_PREPARATION");
    let evidence_path = path("BENILLA_STARTER_EVIDENCE");
    let private_root = PathBuf::from("/home/robbie/Work/benilla/benilla-config")
        .canonicalize()
        .unwrap();
    assert!(evidence_path
        .parent()
        .unwrap()
        .canonicalize()
        .unwrap()
        .starts_with(&private_root));
    let source = decode(
        &std::fs::read(source_path).unwrap(),
        super::super::super::source::MAX_BUNDLE,
    )
    .unwrap();
    super::super::super::source::validate(&source).unwrap();
    let (dbcs, extraction) = dbc::load(&dbc_path).unwrap();
    let source_sha = digest(&source);
    let c = compiler(&source, &source_sha, &dbcs);
    let rules =
        serde_json::to_value(benilla_sim::world_rules::NativeWorldRules::cmangos_classic_stock())
            .unwrap();
    let report = super::super::compile(&source, &dbcs, c.request, extraction, &rules).unwrap();
    let python = decode(&std::fs::read(preparation_path).unwrap(), 128 * 1024 * 1024).unwrap();
    assert_eq!(
        report, python,
        "complete independent Python/Rust preparation parity"
    );
    let draft = build(&c, &candidate(&c, &report)).unwrap_or_else(|error| panic!("{error}"));
    let p = &draft.projection;
    let created = p.projected_creation();
    let entry = p.projected_first_entry();
    let roots: Vec<_> = canonical_order::collection(&source, "playercreateinfo_spell")
        .iter()
        .map(|r| int(&r["Spell"]) as u32)
        .collect();
    assert_eq!(roots.len(), 35);
    assert_eq!(p.pending_learning_hooks(), roots);
    assert_eq!(draft.refusals.len(), 36);
    assert_eq!(p.reached_dependencies().len(), 15);
    assert_eq!(casts(p.creation_plan()).len(), 15);
    assert_eq!(casts(p.first_entry_plan()).len(), 15);
    let init::Rows::Complete(defaults) = &draft.input.default_skills else {
        panic!("defaults")
    };
    assert_eq!(defaults.len(), 11);
    // ObjectMgr.cpp:8669-8672: GENERIC(DND)183 has no skill range.
    assert_eq!(dbcs["SkillLine"].get(183).unwrap().word(1), 12);
    assert_eq!(
        created.skills.keys().copied().collect::<Vec<_>>(),
        [6, 8, 95, 98, 136, 162, 228, 237, 415, 754]
    );
    let trained: Vec<_> = entry
        .trace
        .iter()
        .filter_map(|event| match event {
            init::InitEvent::TrainLoadedSkill { skill, .. } => Some(*skill),
            _ => None,
        })
        .collect();
    assert_eq!(trained, entry.skills.keys().copied().collect::<Vec<_>>());
    assert_eq!(p.creation_plan().draw_requests().len(), 1);
    assert_eq!(p.first_entry_plan().draw_requests().len(), 1);
    let masks = init::initial_cast_phases::InitialProficiencyMasks {
        weapon: 0,
        armor: 0,
    };
    assert_eq!(
        p.first_entry_plan().evaluate(&[0], masks).unwrap_err(),
        init::InitError::Invalid("pending reached initial hook binding")
    );
    assert!(init::initialize_mage(&draft.input).is_err());
    assert!(created
        .initial_auras
        .iter()
        .any(|a| a.spell_id == 20599 && a.aura_type == 156 && a.amount == 10));
    assert_ne!(created.effective_attributes, created.create_attributes);
    assert!(created.equipped.contains_key(&15));
    let equip = created
        .trace
        .iter()
        .position(|event| matches!(event, init::InitEvent::Equip { .. }))
        .unwrap();
    let proficiency = created
        .trace
        .iter()
        .rposition(|event| matches!(event, init::InitEvent::EffectApplied { effect: 60, .. }))
        .unwrap();
    assert!(proficiency < equip);
    let capture = reputation_capture(&c);
    assert_eq!(draft.initial_reputation, capture.initialize().unwrap());
    assert_eq!(
        draft.initial_reputation.len(),
        report["player"]["initial_reputation"]["initial_standings"]
            .as_array()
            .unwrap()
            .len()
    );
    for (actual, expected) in draft.initial_reputation.iter().zip(
        report["player"]["initial_reputation"]["initial_standings"]
            .as_array()
            .unwrap(),
    ) {
        assert_eq!(json!(actual.id), expected["faction"]);
        assert_eq!(
            json!(actual.reputation_list_id),
            expected["reputation_list_id"]
        );
        assert_eq!(json!(actual.standing), expected["stored_delta"]);
        assert_eq!(json!(actual.flags), expected["flags"]);
        assert_eq!(json!(actual.need_send), expected["need_send"]);
        assert_eq!(json!(actual.need_save), expected["need_save"]);
    }
    let per_root: Vec<_> = roots.iter().map(|id| {
        let cast_count = |plan: &init::initial_cast_phases::InitialCastPlan| plan.occurrences().iter().filter(|cast| cast.row().id == *id).count();
        json!({"spell":id,"numeric_learning_composed":true,"fully_composed":false,
            "pending":"reached_hook_binding_and_source_seal","creation_occurrences":cast_count(p.creation_plan()),
            "entry_occurrences":cast_count(p.first_entry_plan()),"availability":created.abilities[id].availability,
            "holders":created.initial_auras.iter().filter(|a| a.spell_id == *id).collect::<Vec<_>>()})
    }).collect();
    let evidence = json!({"format":"benilla-private-starter-draft-evidence-v1","source_sha256":source_sha,
        "preparation_sha256":report["preparation_sha256"],"python_rust_complete_parity":true,
        "top_level_objects":report["unsupported"].as_array().unwrap().len(),"selected_numeric":draft.input.spells.len(),
        "fully_composed_learning_objects":0,"pending_learning_objects":35,"roots":per_root,
        "creation_casts":casts(p.creation_plan()),"entry_casts":casts(p.first_entry_plan()),"entry_skill_order":trained,
        "reached_dependencies":p.reached_dependencies(),"initial_reputation":draft.initial_reputation,
        "create_attributes":created.create_attributes,"projected_attributes":created.effective_attributes,
        "projected_resources":created.resources,"initial_auras":created.initial_auras,
        "weapon_proficiency":created.weapon_proficiency,"armor_proficiency":created.armor_proficiency,
        "equipped":created.equipped.iter().map(|(slot,item)| json!({"slot":slot,"item":item.item})).collect::<Vec<_>>(),
        "inventory":created.inventory.iter().map(|item|json!({"item":item.item,"count":item.count})).collect::<Vec<_>>(),
        "creation_pending_draws":p.creation_plan().draw_requests(),"entry_pending_draws":p.first_entry_plan().draw_requests(),
        "runtime_admitted":false,"source_sealed":false});
    std::fs::write(evidence_path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    println!(
        "starter draft parity={} objects={} learned={} reached={} creation={} entry={} factions={}",
        report["preparation_sha256"],
        report["unsupported"].as_array().unwrap().len(),
        roots.len(),
        p.reached_dependencies().len(),
        casts(p.creation_plan()).len(),
        casts(p.first_entry_plan()).len(),
        draft.initial_reputation.len()
    );
}

fn reputation_capture(c: &Compiler<'_>) -> rep::InitialFactionCapture {
    rep::InitialFactionCapture {
        factions: Some(
            c.dbcs["Faction"]
                .by_id
                .keys()
                .filter_map(|id| c.dbcs["Faction"].get(*id))
                .map(words)
                .collect(),
        ),
        templates: vec![],
        required_templates: vec![],
        required_factions: vec![],
        race: c.request.race,
        class: c.request.class,
    }
}
