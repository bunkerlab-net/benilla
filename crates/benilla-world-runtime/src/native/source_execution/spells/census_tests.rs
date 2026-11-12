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
