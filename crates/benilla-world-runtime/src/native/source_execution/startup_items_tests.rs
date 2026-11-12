// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::source_rng::{DrawCursor, EventKind, Mt19937, MtState, Operation, Value};
use rustc_apfloat::{ieee::Double, Float, Round};
use sha2::{Digest, Sha256};
use std::io::{BufRead, BufReader};

#[test]
fn missing_and_filtered_enchantments_do_not_draw_and_invalid_sample_zeroes_property() {
    // ItemEnchantmentMgr.cpp:66,83-90; ObjectMgr.cpp:2769-2773.
    let mut items = SourceStartupItems::load_enchantments([
        (1, 10, 0.000001),
        (1, 10, 100.01),
        (1, 10, f32::NAN),
        (2, 99, 100.0),
    ]);
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    items
        .validate(
            BTreeMap::from([(0, 2), (1, 0), (2, 1), (3, 2), (4, 3)]),
            &BTreeSet::from([10]),
            &mut random,
        )
        .unwrap();
    assert_eq!(items.items_validated, 4);
    assert_eq!(
        items
            .validations
            .iter()
            .map(|v| (
                v.entry,
                v.selected_enchantment,
                v.effective_property,
                v.raw_end - v.raw_begin
            ))
            .collect::<Vec<_>>(),
        [(2, 0, 0, 0), (3, 99, 0, 2), (4, 0, 0, 0)]
    );
    assert_eq!(random.cursor().raw, 2);
}
#[test]
fn fallback_uses_integer_division_and_strict_comparison() {
    // ItemEnchantmentMgr.cpp:99-112. Seed 1 rolls above one, forcing the fallback.
    let items = SourceStartupItems::load_enchantments([(1, 10, 0.5), (1, 20, 0.5)]);
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    let mut sink = |event| events.push(event);
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    assert_eq!(items.enchantment(1, &mut random).unwrap(), 10);
    assert_eq!(random.cursor().raw, 3);
    assert!(events.iter().any(|e| matches!(
        e.kind,
        EventKind::Begin {
            operation: Operation::Irand,
            args: [0, 101, 0],
            ..
        }
    )));
}
#[test]
fn validation_tests_the_selected_enchantment_not_the_property_id() {
    // ObjectMgr.cpp:2769 tests GetItemEnchantMod's return, not RandomProperty itself.
    let mut items = SourceStartupItems::load_enchantments([(7, 99, 100.0)]);
    let mut mt = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    items
        .validate(BTreeMap::from([(1, 7)]), &BTreeSet::from([99]), &mut random)
        .unwrap();
    assert_eq!(items.validations[0].effective_property, 7);
}

#[test]
fn equal_cumulative_chance_does_not_select_and_fallback_can_return_zero() {
    // Strict > in both loops and integer quotient, ItemEnchantmentMgr.cpp:101,105,112.
    struct Words(std::vec::IntoIter<u64>);
    impl RawGenerator for Words {
        const MIN: u64 = 0;
        const MAX: u64 = u32::MAX as u64;
        fn next_word(&mut self) -> u64 {
            self.0.next().unwrap()
        }
    }
    for (rows, words, selected) in [
        (vec![(1, 10, 50.0), (1, 20, 50.0)], vec![0, 1 << 31], 20),
        (vec![(1, 10, 1.0)], vec![u64::from(u32::MAX); 3], 0),
    ] {
        let mut engine = Words(words.into_iter());
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(
            SourceStartupItems::load_enchantments(rows)
                .enchantment(1, &mut random)
                .unwrap(),
            selected
        );
    }
}

fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|p| u8::from_str_radix(std::str::from_utf8(p).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn seal8_full08_catalog_prefix_matches_every_word_and_item_operand_or_explicit_absence() {
    let (Some(root), Some(trace)) = (
        std::env::var_os("BENILLA_F2_CATALOG"),
        std::env::var_os("BENILLA_F3_STARTUP_TRACE"),
    ) else {
        println!("SKIP startup item RNG: bind BENILLA_F2_CATALOG and BENILLA_F3_STARTUP_TRACE to seal8-full-08/run/trace.tsv");
        return;
    };
    let mut catalog =
        super::super::super::production_compiler::VerifiedCatalog::open(Path::new(&root), false)
            .unwrap();
    let expected_enchantments = catalog.execution_item_enchantments().unwrap();
    let expected_items = catalog.execution_item_random_properties().unwrap();
    let mut recorded_enchantments = Vec::new();
    let mut recorded_items = BTreeMap::new();
    let mut address = String::new();
    let mut relation = 0;
    let mut entry = 0;
    let mut ench = 0;
    let mut property_col = usize::MAX;
    let mut words = Vec::new();
    let mut results = Vec::new();
    let mut calls = Vec::new();
    let mut initial = None;
    let mut reader = BufReader::with_capacity(1 << 20, std::fs::File::open(trace).unwrap());
    let mut line = String::new();
    let mut prefix_hash = Sha256::new();
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            panic!("missing terrain boundary");
        }
        prefix_hash.update(line.as_bytes());
        let mut fields = line.trim_end().splitn(4, '\t');
        if fields.next() != Some("1") {
            continue;
        }
        let _sequence = fields.next().unwrap();
        let kind = fields.next().unwrap();
        let data = fields.next().unwrap();
        let p = data.split_whitespace().collect::<Vec<_>>();
        if kind == "sql_query" {
            let query = unhex(p.last().unwrap());
            relation = match query.as_slice() {
                b"SELECT entry, ench, chance FROM item_enchantment_template" => 1,
                b"SELECT * FROM item_template" => 2,
                _ => 0,
            };
            address = p[0].to_owned();
        }
        if kind == "sql_column_name"
            && relation == 2
            && p[0] == address
            && unhex(p.last().unwrap()).eq_ignore_ascii_case(b"RandomProperty")
        {
            property_col = p[1].parse().unwrap();
        }
        if kind == "sql_field" && relation != 0 && p[0] == address {
            let col: usize = p[1].parse().unwrap();
            let value = if p.last() == Some(&"NULL") {
                "0".to_owned()
            } else {
                String::from_utf8(unhex(p.last().unwrap())).unwrap()
            };
            if col == 0 {
                entry = value.parse().unwrap();
            }
            if relation == 1 && col == 1 {
                ench = value.parse().unwrap();
            }
            if relation == 1 && col == 2 {
                recorded_enchantments.push((entry, ench, value.parse::<f64>().unwrap() as f32));
            }
            if relation == 2 && col == property_col && entry != 0 {
                recorded_items.insert(entry, value.parse::<i64>().unwrap() as u32);
            }
        }
        if kind == "mt_state" {
            let mut state = [0; 624];
            for (word, value) in state.iter_mut().zip(&p[1..625]) {
                *word = value.parse().unwrap();
            }
            initial = Some(MtState {
                words: state,
                index: p[625].parse().unwrap(),
            });
        }
        if kind == "rng_call" {
            if p[1].ends_with("GridMap.cpp") {
                assert_eq!(p[2..], ["708", "20", "40"]);
                break;
            }
            assert!(
                p[1].ends_with("ItemEnchantmentMgr.cpp"),
                "unowned pre-terrain RNG site: {data}"
            );
            calls.push((p[0].to_owned(), p[2..].join(" ")));
        }
        if kind == "mt_raw" {
            words.push((p[1].parse::<u64>().unwrap(), p[2].parse::<u64>().unwrap()));
        }
        if kind == "rng_result" {
            results.push(if p[0] == "rand_chance" {
                Value::F64(
                    Double::from_str_r(p[1], Round::NearestTiesToEven)
                        .unwrap()
                        .value
                        .to_bits() as u64,
                )
            } else {
                Value::I64(p[1].parse().unwrap())
            });
        }
    }
    assert_eq!(
        format!("{:x}", prefix_hash.finalize()),
        "1e7164e641cb7b5d16d0b8b9b624c222dd7b91755182560bf5cfb8689b8b973b"
    );
    assert_eq!(
        recorded_enchantments, expected_enchantments,
        "F1 SELECT enchantment order"
    );
    assert_eq!(
        recorded_items, expected_items,
        "F1 item IDs and RandomProperty operands"
    );
    assert_eq!(words.len(), 3387);
    assert_eq!(calls.len(), 1694);
    assert_eq!(calls.iter().filter(|c| c.0 == "rand_chance").count(), 1693);
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.0 == "irand")
            .map(|c| c.1.as_str())
            .collect::<Vec<_>>(),
        ["105 0 9991"]
    );
    let mut mt = Mt19937::from_state(initial.unwrap()).unwrap();
    let mut cursor = DrawCursor::default();
    let mut actual_words = Vec::new();
    let mut actual_results = Vec::new();
    let mut top_span = 0;
    let mut sink = |event: TraceEvent| match event.kind {
        EventKind::Raw { position, word, .. } => actual_words.push((position, word)),
        EventKind::Begin {
            span, parent: None, ..
        } => top_span = span,
        EventKind::End {
            span,
            result: Ok(value),
            ..
        } if span == top_span => actual_results.push(value),
        _ => (),
    };
    let mut random = SourceRandom::new(1, &mut mt, &mut cursor, &mut sink).unwrap();
    let items = SourceStartupItems::catalog_prefix(&mut catalog, &mut random).unwrap();
    assert_eq!(random.cursor().raw, 3387);
    assert_eq!(
        items
            .validations
            .iter()
            .filter(|v| v.raw_end > v.raw_begin)
            .count(),
        1693
    );
    let zeroed = items
        .validations
        .iter()
        .filter(|v| v.effective_property == 0)
        .map(|v| v.entry)
        .collect::<Vec<_>>();
    println!("F3_STARTUP_ITEMS definitions={} items_validated={} nonzero_properties={} zeroed={zeroed:?} draws=1694 raw=3387 drawn_item_entries={:?}", items.definitions, items.items_validated, items.validations.len(), items.validations.iter().filter(|v| v.raw_end > v.raw_begin).map(|v| v.entry).collect::<Vec<_>>());
    let mut owner = items.bind_spawns(&mut random).unwrap();
    if let Some(path) = std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES") {
        let inputs = EffectiveSpawnInputs::catalog_diagnostic(&mut catalog).unwrap();
        let mut rows = super::super::super::production_compiler::ObjectMgrSpawnRows::open(
            &mut catalog,
            Path::new(&path),
        )
        .unwrap();
        let mut spawns = SourceEffectiveSpawns::default();
        assert_eq!(
            spawns
                .load_rows(&mut rows, &inputs, &[], &mut owner)
                .unwrap_err()
                .id,
            "execution.startup_items.terrain_info_constructor_rng_owner"
        );
        assert_eq!(spawns.complete_count(), 0);
        assert_eq!(spawns.pre_terrain_map().len(), 1);
        assert_eq!(spawns.draws_needed, 0);
        println!("F3_STARTUP_SPAWNS pre_terrain=1 complete=0 random_selections=0 waypoint_rows_advanced=0 refusal=execution.startup_items.terrain_info_constructor_rng_owner");
    } else {
        assert_eq!(
            owner.original_zone(0, [0.0; 3]).unwrap_err().id,
            "execution.startup_items.terrain_info_constructor_rng_owner"
        );
    }
    assert_eq!(
        owner.creature_irand(0, 2).unwrap_err().id,
        "execution.startup_items.stopped_at_terrain_owner"
    );
    drop(owner);
    assert_eq!(random.cursor().raw, 3387);
    assert_eq!(actual_words, words);
    assert_eq!(actual_results, results);
}
