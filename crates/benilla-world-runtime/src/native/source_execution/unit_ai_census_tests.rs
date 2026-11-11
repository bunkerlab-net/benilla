use super::*;
use std::collections::BTreeMap;

struct Unbound;
impl UnitAiExecutionOwner for Unbound {}
impl CreatureRandom for Unbound {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        Err(refuse("execution.unit_ai.live_rng_binding_unavailable"))
    }
    fn creature_urand(&mut self, _: u32, _: u32) -> Result<u32> {
        Err(refuse("execution.unit_ai.live_rng_binding_unavailable"))
    }
}
#[test]
fn catalog04_unit_ai_readiness_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP catalog-04 UnitAI census: set BENILLA_F5_AI_CATALOG and BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let mut catalog =
        super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let config_path =
        std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config required");
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config_path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    assert_eq!(config.guard_sight().unwrap(), 50f32.to_bits());
    let templates = catalog_creature_ai_census_inputs(&mut catalog)
        .unwrap()
        .into_iter()
        .map(|t| (t.entry, t))
        .collect::<BTreeMap<_, _>>();
    assert_eq!(templates.len(), 10384);
    let spawns = catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap()
        .into_iter()
        .map(|(_, w)| (w[0], w[1]))
        .collect::<BTreeMap<_, _>>();
    let mut stats_reasons = BTreeMap::from([
        ("non_normal_rank", 0usize),
        ("health_or_mana_sentinel_with_modern_armor", 0),
        ("flying", 0),
    ]);
    let mut template_numeric_candidates = 0;
    let mut calls_guards_templates = 0;
    let mut calls_guards_spawns = 0;
    for t in templates.values() {
        let rank = t.rank != 0;
        let sentinel = t.stats_multipliers[..2]
            .iter()
            .any(|bits| f32::from_bits(*bits) < 0.)
            && f32::from_bits(t.stats_multipliers[2]) >= 0.;
        let flying = t.inhabit_type & 4 != 0;
        for (reason, yes) in [
            ("non_normal_rank", rank),
            ("health_or_mana_sentinel_with_modern_armor", sentinel),
            ("flying", flying),
        ] {
            if yes {
                *stats_reasons.entry(reason).or_default() += 1;
            }
        }
        template_numeric_candidates += usize::from(!rank && !sentinel && !flying);
        if t.static_flags[0] & 0x8000000 != 0 {
            calls_guards_templates += 1;
            calls_guards_spawns += spawns.values().filter(|entry| **entry == t.entry).count();
        }
    }
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let reaction_outside_enum_range = rows
        .iter()
        .map(|row| {
            (0..3)
                .filter(|slot| row.words[12 + 4 * slot] == 50 && row.words[13 + 4 * slot] > 3)
                .count()
        })
        .sum::<usize>();
    assert_eq!(reaction_outside_enum_range, 0);
    let mut loader = EventAiCatalog::new(&mut catalog, config.clone());
    let mut first = BTreeMap::<String, usize>::new();
    let mut constructor_refusals = BTreeMap::<String, usize>::new();
    let mut loaded = 0;
    let mut passed_base_gate = 0;
    let mut ready_true = 0;
    let mut ready_false = 0;
    let mut invocation_unavailable = 0;
    let mut action_owners = BTreeMap::<String, usize>::new();
    for row in rows {
        let event = match load_event_ai_row(&mut loader, row) {
            Err(e) => {
                *first.entry(e.id).or_default() += 1;
                continue;
            }
            Ok(EventAiLoad {
                disposition: EventAiDisposition::Loaded(e),
                ..
            }) => e,
            Ok(_) => panic!("unexpected source skip"),
        };
        loaded += 1;
        let entry = if event.entry != 0 {
            event.entry
        } else {
            *spawns.get(&event.guid).expect("loaded GUID exists")
        };
        let t = templates.get(&entry).expect("catalog template");
        let requirements = event
            .actions
            .iter()
            .filter_map(|a| match a.kind {
                0 | 6..=8 | 13 | 14 | 18 | 19 | 20 | 22 | 23 | 30 | 31 | 46 | 49 | 50 | 57 => None,
                _ => Some(event_ai_action_owner(a.kind)),
            })
            .collect::<std::collections::BTreeSet<_>>();
        for requirement in requirements {
            *action_owners.entry(requirement.into()).or_default() += 1;
        }
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        units
            .register(&objects, unit, SourceUnitFields::creature(t))
            .unwrap();
        let mut owner = Unbound;
        let base = match SourceCreatureAi::construct(
            &objects, unit, &mut units, t, false, 0, &config, &mut owner,
        ) {
            Ok(base) => base,
            Err(e) => {
                *constructor_refusals.entry(e.id.clone()).or_default() += 1;
                *first.entry(e.id).or_default() += 1;
                continue;
            }
        };
        passed_base_gate += 1;
        // These source callbacks require their real caller, not a fabricated self invoker.
        if event.kind == 5 && event.params[2] == 1
            || event.kind == 10 && event.params[5] != 0
            || event.kind == 22 && event.params[1] != 0
        {
            invocation_unavailable += 1;
            *first
                .entry("census.live_invoker_binding_unavailable".into())
                .or_default() += 1;
            continue;
        }
        // Reuse F5.03's authored EventAI identity only to isolate CheckEvent.
        // This is not source factory selection or a live spawned-class census.
        let mut state = super::event_ai_dispatch_tests::state();
        state.holders.clear();
        state.phase = (0..32)
            .find(|p| event.inverse_phase & (1u32 << p) == 0)
            .unwrap_or(0) as u8;
        state.holders.push(EventAiHolder {
            event,
            timer: 0,
            enabled: true,
            in_progress: false,
            event_target: None,
        });
        let mut ais = SourceCreatureAis::default();
        ais.install(&objects, &mut units, base, &mut SourceCombat::default())
            .unwrap();
        let mut threats = SourceThreats::default();
        threats.register(unit).unwrap();
        let mut execution = SourceAiExecution {
            queries: SourceUnitQueries {
                objects: &objects,
                units: &mut units,
            },
            ais: &mut ais,
            threats: &mut threats,
            owner: &mut owner,
        };
        match state.ready_event(0, EventAiInvocation::default(), &mut execution) {
            Ok(true) => ready_true += 1,
            Ok(false) => ready_false += 1,
            Err(e) => {
                assert!(
                    !e.id.contains("undefined"),
                    "no invented invalid stock invocation: {}",
                    e.id
                );
                *first.entry(e.id).or_default() += 1;
            }
        }
    }
    assert_eq!(loaded, 11126);
    assert_eq!(
        first.values().sum::<usize>() + ready_true + ready_false,
        11469
    );
    assert!(passed_base_gate > 0);
    assert!(!first.contains_key("execution.event_ai.f5_04_base_ai_owner"));
    assert!(!first.contains_key("execution.event_ai.f5_04_combat_action_owner"));
    let report = serde_json::json!({
        "catalog_sha256":catalog.report()["catalog_sha256"],"raw_rows":11469,"loaded":loaded,
        "scope":"isolated loaded EventAI row alternatives at ordinary uncontrolled raw-template construction; not live dispatch or admission",
        "before":{"base_ai_first_refusals":4310,"combat_action_first_refusals":6816,"end_to_end":0},
        "after":{"passed_base_gate":passed_base_gate,"ready_true":ready_true,"ready_false":ready_false,"invoker_bindings_unavailable":invocation_unavailable,"end_to_end":0,"end_to_end_reason":"no bound live instance, callback invocation, selected executable or event RNG stream; readiness completion is not dispatch execution","constructor_refusals":constructor_refusals,"first_refusals":first},
        "action_dependencies_nonexclusive_rows":action_owners,
        "select_level":{"raw_templates":templates.len(),"template_numeric_candidates":template_numeric_candidates,"known_outside_reasons_nonexclusive":stats_reasons,"inside_full_resolver_domain_proven":0,"missing_complete_initialization_bundles":templates.len(),"event_or_group_context_unavailable":templates.len(),"note":"rank/sentinel/flying are raw template checks; complete model/equipment/addon/spawn/external-feature inputs and source-selected level required before calling bounded resolver"},
        "guard_c_rng":{"calls_guards_templates":calls_guards_templates,"calls_guards_static_spawns":calls_guards_spawns,"area_221_222_live_reach":"unavailable without F6 area and actual combat invocations; reached unbound C stream refuses, never substituted MT"},
        "new_undefined_source_operations":[
            {"site":"TimerAI.h:189-190","guard":"action_index_undefined","stock_EventAI_index_accesses":0,"reason":"CreatureEventAI.cpp:87 constructs zero combat actions","ScriptDev_reach":"unavailable"},
            {"site":"CreatureEventAI.cpp:1211, UnitAI.h:81-86","guard":"react_enum_conversion_undefined","raw_stock_candidates":reaction_outside_enum_range}
        ],"F1_base_ai_or_unit_projection":"unavailable; inherited EventAI helper-word projections are separate tests",
        "next_row":"F5.07 spell inputs and pipeline by largest remaining action-dependent set; F5.05 remains earlier motion construction prerequisite"
    });
    println!(
        "F5_UNIT_AI_CENSUS {}",
        serde_json::to_string(&report).unwrap()
    );
}
