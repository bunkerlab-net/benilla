use super::*;
use std::collections::BTreeMap;

#[test]
fn retained_loaded_command_census_or_explicit_absence() {
    command_census(false);
    command_census(true);
}

fn command_census(bind_pool_maximum: bool) {
    use crate::native::production_compiler::VerifiedCatalog;
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP DB-script executor census: set BENILLA_F5_AI_CATALOG");
        return;
    };
    let occurrences = std::env::var_os("BENILLA_F5_DB_SCRIPT_OCCURRENCES")
        .expect("catalog requires DB-script occurrences");
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    struct SpellMissing;
    impl SpellLoaderOwner for SpellMissing {}
    let spells = SourceSpellStores::load(&mut catalog, &mut SpellMissing).unwrap();
    let templates = catalog_db_script_templates(&mut catalog, Path::new(&occurrences)).unwrap();
    let mut tables = BTreeMap::new();
    for kind in DbScriptType::ALL {
        tables.insert(
            kind,
            catalog_db_script_rows(&mut catalog, Path::new(&occurrences), kind).unwrap(),
        );
    }
    let mut external = catalog_group_census_owner(&mut catalog);
    let maximum = if bind_pool_maximum {
        let path = std::env::var_os("BENILLA_F2_POOL_OCCURRENCES")
            .expect("pool-max component census requires pool occurrences");
        Some(SourcePoolMaximum::load_catalog(&mut catalog, Path::new(&path)).unwrap())
    } else {
        None
    };
    let mut owner = DbScriptCatalog::new(&mut catalog, Some(&spells), &mut external, None);
    if let Some(maximum) = &maximum {
        assert_eq!(maximum.maximum().unwrap(), 50003);
        owner = owner.with_pool_maximum(maximum);
    }
    println!("DB-script component census pool MAX phase bound={bind_pool_maximum}; no claim of loaded pool templates/memberships or integrated startup past earlier spawn owners");
    let mut relay = None;
    // raw, complete-loaded, effect/termination, executor-refused, loader-unexecutable, source-skipped
    let mut counts = BTreeMap::<u32, [usize; 6]>::new();
    let mut incomplete = BTreeMap::new();
    let mut incomplete_maps = BTreeMap::new();
    let mut complete_inputs = Vec::new();
    let mut ub_operands = BTreeMap::<&str, usize>::new();
    let mut spell_counts = [0usize; 6];
    let mut spell_refused = BTreeMap::<String, usize>::new();
    for kind in [
        DbScriptType::Relay,
        DbScriptType::Gossip,
        DbScriptType::QuestStart,
        DbScriptType::QuestEnd,
        DbScriptType::Spell,
        DbScriptType::GameObject,
        DbScriptType::GameObjectTemplate,
        DbScriptType::Event,
        DbScriptType::CreatureDeath,
        DbScriptType::CreatureMovement,
    ] {
        let rows = tables.remove(&kind).unwrap();
        for row in &rows {
            for (site, reaches) in [
                (
                    "ScriptMgr.cpp:2826-2828/Object.cpp:1639 uninitialized z",
                    row.command == 37 && row.data[0] == 0 && row.flags & 8 != 0,
                ),
                (
                    "ScriptMgr.cpp:1856,1877,1894 forced enum",
                    row.command == 3 && row.data[2] > 3,
                ),
                (
                    "ScriptMgr.cpp:2848 forced enum",
                    row.command == 37 && !(0..=3).contains(&row.text[0]),
                ),
                (
                    "ScriptMgr.cpp:2990/Unit.cpp:4304 spell index",
                    row.command == 47 && row.data[0] >= 4,
                ),
                (
                    "ScriptMgr.cpp:2301 forced enum",
                    row.command == 20 && !(0..=3).contains(&row.text[0]),
                ),
                (
                    "ScriptMgr.cpp:2313 motion enum",
                    row.command == 20 && row.data[0] > 31,
                ),
                (
                    "ScriptMgr.cpp:3297 sheath enum",
                    row.command == 54 && row.data[0] > 3,
                ),
            ] {
                *ub_operands.entry(site).or_default() += usize::from(reaches);
            }
        }
        let report =
            LoadedDbScriptMap::inspect(kind, rows.clone(), &templates, relay.as_ref(), &mut owner);
        let reasons: Vec<_> = report
            .rows()
            .iter()
            .filter_map(|r| {
                if let DbScriptDisposition::Refused(e) = &r.disposition {
                    Some(e.id.clone())
                } else {
                    None
                }
            })
            .collect();
        let complete = reasons.is_empty();
        for (row, disposition) in rows.iter().zip(report.rows()) {
            let count = counts.entry(row.command).or_default();
            count[0] += 1;
            if kind == DbScriptType::Spell {
                spell_counts[0] += 1;
                spell_counts[4] += usize::from(!complete);
                spell_counts[1] += usize::from(complete);
            }
            if !complete {
                count[4] += 1;
            }
            match &disposition.disposition {
                DbScriptDisposition::Loaded if complete => count[1] += 1,
                DbScriptDisposition::Refused(e) => {
                    incomplete.insert(
                        format!("{}:{}:{}", kind.table(), row.id, row.ordinal),
                        e.id.clone(),
                    );
                }
                DbScriptDisposition::Dropped => panic!(
                    "new loader drop in {} ordinal {}",
                    kind.table(),
                    row.ordinal
                ),
                _ => {}
            }
        }
        if complete {
            complete_inputs.push((kind, rows));
        } else {
            incomplete_maps.insert(kind.table(),serde_json::json!({"rows":rows.len(),"status":"not executable: loader refused","owners":reasons}));
        }
        if kind == DbScriptType::Relay {
            relay = Some(report.into_loaded().expect("stock relay map must load"));
        }
    }
    let loaded = SourceDbScriptMaps::load(complete_inputs, templates, &mut owner).unwrap();
    let mut refused = BTreeMap::<String, usize>::new();
    let mut undefined = Vec::new();
    let mut gossip7166 = 0;
    for kind in DbScriptType::ALL {
        let Ok(map) = loaded.map(kind) else { continue };
        for row in map.scripts().values().flatten() {
            let mut f = super::tests::fixture();
            // Bind actual loaded F5.07 metadata; the cast itself still requires F5.08.
            if row.command == 15 {
                f.host.spells.clear();
                for id in std::iter::once(row.data[0])
                    .chain(row.text.into_iter().filter(|&v| v != 0).map(|v| v as u32))
                {
                    let s = spells.template(id).expect("loader checked spell presence");
                    f.host
                        .spells
                        .insert(id, (s.attributesex & 0x0008_0000 != 0, s.targets));
                }
            }
            // Explicit authored roles, not reconstructed production callers or an F1 dispatch census.
            let (mut source, mut target) = match row.command {
                9 | 11 | 12 | 27 => (f.h[4], f.h[3]),
                13 | 40 | 43 => (f.h[0], f.h[4]),
                41 | 52 => (f.h[0], f.h[1]),
                _ => (f.h[0], f.h[3]),
            };
            if row.flags & 2 != 0 {
                std::mem::swap(&mut source, &mut target);
            }
            let invocation = DbScriptInvocation {
                kind: Some(kind),
                id: row.id,
                source: f.objects.get(source).unwrap().guid,
                target: f.objects.get(target).unwrap().guid,
                item_owner: None,
            };
            let mut executor = SourceDbScriptExecutor::new(super::tests::map_id(), &loaded);
            let result = executor.handle_step(
                Step {
                    invocation,
                    definition: Definition::Loaded(row),
                },
                &mut DbScriptContext {
                    objects: &mut f.objects,
                    units: &mut f.units,
                    owner: &mut f.host,
                },
            );
            if kind == DbScriptType::Gossip && row.id == 7166 && row.command == 35 {
                gossip7166 += 1;
                assert_eq!(row.data[0], 7166);
                assert!(result.is_ok());
                assert_eq!(
                    f.host.received.iter().map(|r| r.3).collect::<Vec<_>>(),
                    [7166]
                );
            }
            let count = counts.get_mut(&row.command).unwrap();
            let outcome = match &result {
                Ok(terminated)
                    if *terminated || !f.host.effects.is_empty() || !f.host.received.is_empty() =>
                {
                    2
                }
                Ok(_) => 5,
                Err(error) => {
                    if kind == DbScriptType::Spell {
                        *spell_refused.entry(error.id.clone()).or_default() += 1;
                    }
                    3
                }
            };
            if kind == DbScriptType::Spell {
                spell_counts[outcome] += 1;
            }
            match result {
                Ok(terminated)
                    if terminated || !f.host.effects.is_empty() || !f.host.received.is_empty() =>
                {
                    count[2] += 1
                }
                Ok(_) => count[5] += 1,
                Err(e) => {
                    count[3] += 1;
                    if e.id.ends_with("_undefined") {
                        undefined.push((kind.table(), row.id, row.ordinal, e.id.clone()));
                    }
                    *refused.entry(e.id).or_default() += 1;
                }
            }
        }
    }
    assert_eq!(
        gossip7166, 1,
        "the real gossip7166 command35 row must reach its receiver"
    );
    assert_eq!(counts.values().map(|c| c[0]).sum::<usize>(), 7166);
    for count in counts.values() {
        assert_eq!(count[1], count[2] + count[3] + count[5]);
        assert_eq!(count[0], count[1] + count[4]);
    }
    assert!(
        undefined.is_empty(),
        "stock operands reach undefined execution in authored host: {undefined:?}"
    );
    assert_eq!(
        spell_counts,
        if bind_pool_maximum {
            [31, 31, 3, 27, 0, 1]
        } else {
            [31, 0, 0, 0, 31, 0]
        }
    );
    let totals = counts.values().fold([0; 6], |mut total, count| {
        for (sum, value) in total.iter_mut().zip(count) {
            *sum += value;
        }
        total
    });
    assert_eq!(
        totals,
        if bind_pool_maximum {
            [7166, 4755, 1450, 2775, 2411, 530]
        } else {
            [7166, 4724, 1447, 2748, 2442, 529]
        }
    );
    println!("DB-script component totals {totals:?}");
    println!(
        "DB-script spell component counts {spell_counts:?}; executor refusals {}",
        serde_json::to_string(&spell_refused).unwrap()
    );
    println!("DB-script executor census [raw, complete-loaded, effect-or-termination, executor-refused, loader-unexecutable, source-skipped] {}",serde_json::to_string(&counts).unwrap());
    println!(
        "DB-script executor refusals {}",
        serde_json::to_string(&refused).unwrap()
    );
    println!(
        "DB-script incomplete maps {}",
        serde_json::to_string(&incomplete_maps).unwrap()
    );
    println!(
        "DB-script loader refused rows {}",
        serde_json::to_string(&incomplete).unwrap()
    );
    println!("DB-script UB static operand reach {}; dynamic null/downcast/clock reach is authored-host only, not a production F1 census",serde_json::to_string(&ub_operands).unwrap());
}
