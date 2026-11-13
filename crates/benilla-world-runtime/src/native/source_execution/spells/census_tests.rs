use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

#[test]
fn catalog04_spell_start_static_readiness_and_undefined_operands() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP F5.08 catalog-04 census: set BENILLA_F5_AI_CATALOG and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    let stores = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let config = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut loader = EventAiCatalog::new(&mut catalog, config);
    let mut dependent = 0;
    let mut before = BTreeMap::<String, usize>::new();
    let mut constructor = BTreeMap::<String, usize>::new();
    let mut action_slots = BTreeMap::<u16, usize>::new();
    let mut cast_ids = BTreeSet::new();
    let mut xyz_rows = BTreeSet::new();
    let mut bad_interrupt = Vec::new();
    let mut rows_evidence = Vec::new();
    for row in rows {
        let Ok(EventAiLoad {
            disposition: EventAiDisposition::Loaded(e),
            ..
        }) = load_event_ai_row(&mut loader, row)
        else {
            continue;
        };
        if !e.actions.iter().any(|a| [11, 52, 60].contains(&a.kind)) {
            continue;
        }
        dependent += 1;
        let old = e.actions.iter().try_for_each(|a| match a.kind {
            11 => stores.table_dependencies(a.params.unwrap()[0]),
            60 => Err(refuse("execution.spells.creature_spell_list_owner")),
            _ => Ok(()),
        });
        let old = old.err().map_or("table_closure_ready".into(), |e| e.id);
        *before.entry(old.clone()).or_default() += 1;
        let mut first = "no_cast_action".to_owned();
        for a in &e.actions {
            if ![11, 52, 60].contains(&a.kind) {
                continue;
            }
            *action_slots.entry(a.kind).or_default() += 1;
            let p = a.params.unwrap();
            let status = match a.kind {
                11 => {
                    cast_ids.insert(p[0]);
                    if p[2] & 0x1000 != 0 {
                        xyz_rows.insert(e.id);
                    }
                    // Definition-only readiness. No live Unit is manufactured here.
                    if stores.template(p[0]).is_none() {
                        "source_missing_spell".into()
                    } else if let Err(e) = stores
                        .script_binding(SpellScriptKind::Spell, p[0])
                        .and_then(|_| stores.script_binding(SpellScriptKind::Aura, p[0]))
                    {
                        e.id
                    } else {
                        "f5_09_loaded_targeting_data_and_live_constructor_operands_required".into()
                    }
                }
                52 => {
                    if p[0] >= 4 {
                        bad_interrupt.push(e.id);
                    }
                    "live_current_spell_slots_required".into()
                }
                _ => "execution.spells.creature_spell_list_owner".into(),
            };
            if first == "no_cast_action" {
                first = status;
            }
        }
        *constructor.entry(first.clone()).or_default() += 1;
        rows_evidence.push(json!({"event":e.id,"first_cast_spell":e.actions.iter().find(|a| a.kind == 11).map(|a| a.params.unwrap()[0]),"before_static":old,"after_static":first}));
    }
    assert_eq!(dependent, 7724);
    assert_eq!(before["table_closure_ready"], 7415);
    let mut simple_overflow = Vec::new();
    let mut school_shift = Vec::new();
    let mut target_index = Vec::new();
    for (&id, t) in stores.templates() {
        if t.school >= 32 {
            school_shift.push(id);
        }
        if [
            (t.effectbasepoints1, t.effectbasedice1),
            (t.effectbasepoints2, t.effectbasedice2),
            (t.effectbasepoints3, t.effectbasedice3),
        ]
        .into_iter()
        .any(|(b, d)| b.checked_add(d as i32).is_none())
        {
            simple_overflow.push(id);
        }
        if check_effect_mask(t).is_err() {
            target_index.push(id);
        }
    }
    // These are operand candidates, not proof that all prior dynamic calls execute.
    assert!(
        simple_overflow.is_empty(),
        "stock simple value undefined: {simple_overflow:?}"
    );
    assert!(
        school_shift.is_empty(),
        "stock school shift undefined: {school_shift:?}"
    );
    assert!(
        target_index.is_empty(),
        "stock target lookup undefined: {target_index:?}"
    );
    assert!(
        bad_interrupt.is_empty(),
        "stock interrupt undefined: {bad_interrupt:?}"
    );
    let mut db_rows = BTreeMap::<u32, usize>::new();
    let mut db_alternatives = BTreeMap::<String, usize>::new();
    if let Some(occurrences) = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES") {
        for kind in DbScriptType::ALL {
            for row in catalog_db_script_rows(&mut catalog, Path::new(&occurrences), kind).unwrap()
            {
                if ![15, 46, 47].contains(&row.command) {
                    continue;
                }
                *db_rows.entry(row.command).or_default() += 1;
                if row.command == 47 {
                    continue;
                }
                let mut ids = vec![row.data[0]];
                if row.command == 15 {
                    ids.extend(
                        row.text
                            .iter()
                            .take_while(|&&id| id != 0)
                            .map(|&id| id as u32),
                    );
                }
                for id in ids {
                    let status = if stores.template(id).is_none() {
                        "source_null_template_undefined_candidate".into()
                    } else if let Err(e) = stores.script_binding(SpellScriptKind::Spell, id) {
                        e.id
                    } else {
                        "f5_09_loaded_targeting_data_and_live_constructor_operands_required".into()
                    };
                    *db_alternatives.entry(status).or_default() += 1;
                }
            }
        }
    } else {
        println!(
            "SKIP F5.08 DB-script raw candidate census: BENILLA_F5_DB_SCRIPT_OCCURRENCES absent"
        );
    }
    let evidence = json!({
        "catalog_sha256":catalog.execution_identity()["catalog_sha256"],
        "classification":"static definition readiness, not recorded invocation or gameplay admission",
        "event_ai":{"dependent_rows":dependent,"before_static":before,"after_static":constructor,"action_slots":action_slots,"xyz_flag_rows":xyz_rows,"rows":rows_evidence},
        "actual_spell_start_executions":{"observed":0,"passed_checks":0,"reason":"no recorded live caster/target/cooldown/aura/geometry binding; no fixture Units used in this census"},
        "db_scripts":{"counting":"raw rows and spell alternatives, before whole-map loader dispositions","rows_by_command":db_rows,"alternatives_by_constructor_requirement":db_alternatives},
        "unit_ai_teleport":{"spell":21727,"template_present":stores.template(21727).is_some(),"stock_invocations":"unavailable: conditional live UnitAI action, not a catalog row"},
        "totem_cast":{"stock_invocations":"unavailable: F5.15 constructed Totem and GetSpell owner unbound"},
        "undefined_operand_candidates":{"simple_value":simple_overflow,"school_shift":school_shift,"target_index":target_index,"interrupt_slot":bad_interrupt},
        "undefined_dynamic_reach":"unavailable for CAST_ONLY_XYZ null target, tame-caster null, modifier arithmetic and targeting map miss; catalog definitions are not live reach evidence",
        "next":"F5.09 loaded constructor targeting metadata; F6 live location remains mandatory before any cast passes CheckCast"
    });
    println!("F5.08 static before={before:?} after={constructor:?} action_slots={action_slots:?}; actual observed=0 passed=0; DB raw rows={db_rows:?} alternatives={db_alternatives:?}");
    if let Some(path) = std::env::var_os("BENILLA_F5_SPELL_START_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}
#[test]
fn catalog04_spell_area_and_first_cast_location_census() {
    use SpellCastResult as R;
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    impl SpellLocationOwner for Missing {}
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        eprintln!("SKIP spell location census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let s = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let mut loaded = 0;
    let mut dropped = BTreeMap::<String, usize>::new();
    let mut unresolved = BTreeMap::<String, usize>::new();
    for row in &s.loads()["spell_area"] {
        match &row.disposition {
            SpellRowDisposition::Loaded => loaded += 1,
            SpellRowDisposition::Skipped(reason) => {
                *dropped.entry(reason.clone()).or_default() += 1
            }
            SpellRowDisposition::Refused(error) => {
                *unresolved.entry(error.id.clone()).or_default() += 1
            }
        }
    }
    let config = EventAiLoaderConfig::recorded(
        Path::new(
            &std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required"),
        ),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut loader = EventAiCatalog::new(&mut catalog, config);
    let mut total = 0;
    let mut passed = 0;
    let mut rejected = BTreeMap::<String, usize>::new();
    let mut refused = BTreeMap::<String, usize>::new();
    let mut next = BTreeMap::<String, usize>::new();
    let mut items_reached = 0;
    let mut items_passed = 0;
    let mut items_refused = BTreeMap::<String, usize>::new();
    for row in rows {
        let Ok(EventAiLoad {
            disposition: EventAiDisposition::Loaded(e),
            ..
        }) = load_event_ai_row(&mut loader, row)
        else {
            continue;
        };
        let Some(action) = e.actions.iter().find(|a| [11, 52, 60].contains(&a.kind)) else {
            continue;
        };
        if action.kind != 11 {
            continue;
        }
        let id = action.params.unwrap()[0];
        if s.script_binding(SpellScriptKind::Spell, id)
            .and_then(|_| s.script_binding(SpellScriptKind::Aura, id))
            .is_err()
        {
            continue;
        }
        let Some(t) = s.template(id) else { continue };
        total += 1;
        // Only definition-independent null-player paths are classified. No live
        // pose, map or beneficiary is fabricated for these EventAI rows.
        if s.areas(id).is_ok_and(|bounds| {
            bounds.iter().any(|a| {
                a.condition == 0
                    && a.gender == 2
                    && a.race_mask == 0
                    && a.quest_start == 0
                    && a.quest_end == 0
                    && a.area != 0
            })
        }) {
            *refused.entry("live_zone_area_required".into()).or_default() += 1;
            continue;
        }
        match s.allowed_in_location(t, 0, 0, 0, None, &mut Missing) {
            Ok(R::Ok) => {
                passed += 1;
                #[derive(Default)]
                struct Suffix {
                    items_reached: usize,
                    items_passed: usize,
                    items_refused: BTreeMap<String, usize>,
                }
                impl SpellExecutionOwner for Suffix {
                    fn check(
                        &mut self,
                        spell: &mut SourceSpell,
                        t: &SpellTemplate,
                        check: SpellCheck,
                    ) -> Result<R> {
                        if check != SpellCheck::Items {
                            return Err(check.missing_owner());
                        }
                        self.items_reached += 1;
                        // This census is conditional on an ownerless creature.
                        // Spell.cpp:6438 depends only on that actual caster type,
                        // not on an invented inventory, pose or Unit state.
                        let result = spell.check_items(t, false, self);
                        match &result {
                            Ok(R::Ok) => self.items_passed += 1,
                            Err(error) => {
                                *self.items_refused.entry(error.id.clone()).or_default() += 1;
                            }
                            _ => {}
                        }
                        result
                    }
                }
                let p = action.params.unwrap();
                // UnitAI.cpp:211-224; callers.rs maps these flags identically.
                let mut flags = u32::from(p[2] & 2 != 0) | if p[2] & 0x40 != 0 { 4 } else { 0 };
                if flags == 0 {
                    flags = 0x100;
                }
                // Authored allocation only. The real suffix, not an assumed
                // next check, classifies this conditional null-player path.
                let caster = ObjectHandle {
                    thread: 1,
                    construction: 1,
                };
                let mut value =
                    SourceSpell::new(1, t, caster, SpellTriggerFlags(flags), 0).unwrap();
                value.script_present = s
                    .script_binding(SpellScriptKind::Spell, id)
                    .unwrap()
                    .is_some();
                let mut suffix = Suffix::default();
                let name = match value.check_cast_location_suffix(t, true, false, &mut suffix) {
                    Err(error) => error.id,
                    Ok(R::Ok) => SpellCall::PrepareTriggerData.missing_owner().id,
                    Ok(result) => panic!("unexpected static suffix result {result:?} for {id}"),
                };
                *next.entry(name).or_default() += 1;
                items_reached += suffix.items_reached;
                items_passed += suffix.items_passed;
                for (name, count) in suffix.items_refused {
                    *items_refused.entry(name).or_default() += count;
                }
            }
            Ok(result) => *rejected.entry(format!("{result:?}")).or_default() += 1,
            Err(error) => *refused.entry(error.id).or_default() += 1,
        }
    }
    assert_eq!(total, 7542);
    assert_eq!((items_reached, items_passed), (7032, 7032));
    assert!(items_refused.is_empty());
    let raw = catalog.execution_spell_area_rows("spell_area").unwrap();
    let gender_ub = raw.iter().filter(|r| r.words[8] > 3).count();
    let abs_candidates = raw.iter().filter(|r| r.words[6] == 0x80000000).count();
    assert_eq!((gender_ub, abs_candidates), (0, 0));
    let evidence = json!({
        "basis":"static ownerless uncharmed creature/null beneficiary, not live cast execution",
        "spell_area":{"rows":raw.len(),"loaded":loaded,"dropped":dropped,"unresolved":unresolved},
        "first_cast_rows":total,"location_pass":passed,"source_rejected":rejected,
        "location_refused":refused,"next":next,
        "check_items":{"reaches_check":items_reached,"passes_check":items_passed,
            "refused":items_refused,"source_rejected":0,
            "before":{"reaches_check":7032,"passes_check":0,
                "execution.spells.f5_25_Spell_CheckItems_owner":7032},
            "undefined_operations_reached":0},
        "undefined_operand_candidates":{"gender":gender_ub,"abs":abs_candidates},
        "actual_stock_executions":0,"format":91,
    });
    println!("F5_LOCATION {evidence}");
    if let Some(path) = std::env::var_os("BENILLA_F5_SPELL_LOCATION_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}
