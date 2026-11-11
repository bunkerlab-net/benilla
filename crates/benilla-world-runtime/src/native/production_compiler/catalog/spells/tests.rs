// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
fn table(name: &str, rows: &[Value]) -> Table {
    let names: Vec<_> = fields(name)
        .iter()
        .map(|v| v[0].as_str().unwrap().to_owned())
        .collect();
    Table {
        rows: rows
            .iter()
            .enumerate()
            .map(|(i, r)| (i as u64, names.iter().map(|n| r[n].clone()).collect()))
            .collect(),
        names,
    }
}
#[test]
fn source_conversions_preserve_null_prefix_signed_zero_and_boolean() {
    for (raw, kind, expected) in [
        (None, "u32", json!(0)),
        (Some(b" -1tail".as_slice()), "u32", json!(u32::MAX)),
        (Some(b"-1".as_slice()), "u8", json!(255)),
        (Some(b"65537".as_slice()), "u16", json!(1)),
        (Some(b"-0".as_slice()), "f32", json!(0x80000000u32)),
        (
            Some(b"1.0000000596046448".as_slice()),
            "f32",
            json!(0x3f800000u32),
        ),
        (Some(b"-1".as_slice()), "u64", json!(u64::MAX)),
        (
            Some(b"18446744073709551615".as_slice()),
            "u64",
            json!(u64::MAX),
        ),
        (Some(b"-1".as_slice()), "bool", json!(0)),
        (Some(b"2".as_slice()), "bool", json!(1)),
        (Some(b"a\0ignored".as_slice()), "text", json!("61")),
    ] {
        assert_eq!(convert(raw, kind).unwrap(), expected);
    }
    assert_eq!(convert(Some(b"255"), "unassigned").unwrap(), Value::Null);
    assert_eq!(convert(Some(b"comment"), "ignored").unwrap(), Value::Null);
}
#[test]
fn unsupported_float_encodings_refuse() {
    for raw in [b"nan".as_slice(), b"inf", b"0x1p1"] {
        assert_eq!(
            convert(Some(raw), "f32").unwrap_err().id,
            "spawn.numeric_unsupported"
        );
    }
}
#[test]
fn talent_fallback_is_retained_when_sql_is_whole_empty() {
    let mut w = vec![0; 21];
    w[4..7].copy_from_slice(&[10, 11, 12]);
    let spells = Spells::from([(10, [0; 25]), (11, [0; 25]), (12, [0; 25])]);
    let result = chains(
        &spells,
        &DbcRows::from([(4, w)]),
        &DbcRows::new(),
        &table("spell_chain", &[]),
    )
    .unwrap();
    assert_eq!(result.nodes[&12], [11, 10, 3, 0]);
    assert_eq!(result.nodes, result.fallback);
    assert!(result.dispositions.is_empty());
}
#[test]
fn ability_fallback_includes_herbalism_source_exception() {
    let mut a = vec![0; 15];
    a[2] = 2366;
    let mut b = vec![0; 15];
    b[2] = 2368;
    let result = chains(
        &Spells::from([(2366, [0; 25]), (2368, [0; 25])]),
        &DbcRows::new(),
        &DbcRows::from([(1, a), (2, b)]),
        &table("spell_chain", &[]),
    )
    .unwrap();
    assert_eq!(result.nodes[&2368], [2366, 2366, 2, 0]);
}
#[test]
fn malformed_raw_chain_rows_are_skips_not_catalog_errors() {
    let sql = table(
        "spell_chain",
        &[
            json!({"spell_id":99,"prev_spell":0,"first_spell":99,"rank":1,"req_spell":0}),
            json!({"spell_id":11,"prev_spell":99,"first_spell":10,"rank":2,"req_spell":0}),
            json!({"spell_id":11,"prev_spell":0,"first_spell":10,"rank":2,"req_spell":0}),
            json!({"spell_id":10,"prev_spell":0,"first_spell":10,"rank":1,"req_spell":0}),
            json!({"spell_id":11,"prev_spell":10,"first_spell":10,"rank":2,"req_spell":0}),
        ],
    );
    let result = chains(
        &Spells::from([(10, [0; 25]), (11, [0; 25])]),
        &DbcRows::new(),
        &DbcRows::new(),
        &sql,
    )
    .unwrap();
    assert_eq!(result.nodes[&11], [10, 10, 2, 0]);
    assert_eq!(
        result
            .dispositions
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "skip:missing_spell",
            "skip:missing_previous",
            "skip:incompatible_chain",
            "loaded",
            "loaded"
        ]
    );
}
#[test]
fn cyclic_ability_chains_refuse() {
    let mut a = vec![0; 15];
    a[2] = 10;
    a[8] = 11;
    let mut b = vec![0; 15];
    b[2] = 11;
    b[8] = 10;
    let result = chains(
        &Spells::from([(10, [0; 25]), (11, [0; 25])]),
        &DbcRows::new(),
        &DbcRows::from([(1, a), (2, b)]),
        &table("spell_chain", &[]),
    );
    assert_eq!(result.err().unwrap().id, "spells.chain_cycle");
}
#[test]
fn proc_custom_rank_refusal_does_not_discard_fallback() {
    let t = table(
        "spell_proc_event",
        &[
            json!({"entry":10,"ppmrate":0}),
            json!({"entry":11,"ppmrate":0}),
            json!({"entry":12,"ppmrate":1065353216}),
        ],
    );
    let mut plans = Stream::new(None, "plans").unwrap();
    let spells = Spells::from([(10, [0; 25]), (11, [0; 25]), (12, [0; 25])]);
    let nodes = Nodes::from([
        (10, [0, 10, 1, 0]),
        (11, [10, 10, 2, 0]),
        (12, [11, 10, 3, 0]),
    ]);
    let result = rank_inputs("spell_proc_event", &t, &spells, &nodes, true, &mut plans).unwrap();
    assert_eq!(result[&1], "skip:nonfirst_rank");
    assert_eq!(plans.finish().unwrap()["rows"], 3);
}
#[test]
fn explicit_identity_handler_refusals_and_field_coverage() {
    assert_eq!(POLICY["tables"].as_object().unwrap().len(), 21);
    assert_eq!(fields("spell_template").len(), 159);
    assert!(!DBC_FILES.contains(&"Spell.dbc"));
    assert!(REFUSALS.contains(&"spells.registration_and_handlers_unverified"));
    assert_eq!(fields("unit_condition")[1], json!(["flags", "unassigned"]));
    let t = table("spell_facing", &[json!({"entry":99,"facingcasterflag":1})]);
    assert_eq!(
        basic_disposition(
            "spell_facing",
            &t,
            &t.rows[0].1,
            &Spells::new(),
            &Nodes::new(),
            &BTreeMap::new()
        ),
        "skip:missing_spell"
    );
}
#[test]
fn output_limits_refuse_instead_of_clipping() {
    let mut stream = Stream::new(None, "plans").unwrap();
    assert_eq!(
        stream.add(&json!("x".repeat(FRAME))).unwrap_err().id,
        "spells.record_limit"
    );
    assert_eq!(stream.finish().unwrap()["rows"], 0);
}
#[test]
fn whole_empty_chain_does_not_build_next_cache() {
    let t = table("spell_proc_event", &[json!({"entry":10,"ppmrate":0})]);
    let mut plans = Stream::new(None, "plans").unwrap();
    rank_inputs(
        "spell_proc_event",
        &t,
        &Spells::from([(10, [0; 25]), (11, [0; 25])]),
        &Nodes::from([(10, [0, 10, 1, 0]), (11, [10, 10, 2, 0])]),
        false,
        &mut plans,
    )
    .unwrap();
    assert_eq!(plans.finish().unwrap()["rows"], 1);
}
#[test]
fn repeated_rank_paths_are_not_cycles_and_negative_zero_is_zero() {
    let t = table(
        "spell_proc_event",
        &[
            json!({"entry":10,"ppmrate":0}),
            json!({"entry":11,"ppmrate":2147483648u32}),
        ],
    );
    let mut plans = Stream::new(None, "plans").unwrap();
    let status = rank_inputs(
        "spell_proc_event",
        &t,
        &Spells::from([(10, [0; 25]), (11, [0; 25])]),
        &Nodes::from([(10, [0, 10, 1, 0]), (11, [10, 10, 2, 10])]),
        true,
        &mut plans,
    )
    .unwrap();
    assert_eq!(status[&1], "skip:nonfirst_rank");
    assert_eq!(plans.finish().unwrap()["rows"], 2);
}
#[test]
fn integer_overflow_and_numeric_limit_refuse() {
    assert_eq!(
        convert(Some(b"18446744073709551616"), "u64")
            .unwrap_err()
            .id,
        "spells.uint64_overflow"
    );
    assert_eq!(
        convert(Some(&[b'1'; 128]), "u32").unwrap_err().id,
        "spells.numeric_limit"
    );
}
#[test]
fn target_positions_validate_map_signed_zero_and_database_target() {
    let mut t = table(
        "spell_target_position",
        &[
            json!({"id":10,"target_map":0,"target_position_x":2147483648u32,"target_position_y":0,"target_position_z":0,"target_orientation":0}),
        ],
    );
    let mut spells = Spells::from([(10, [0; 25])]);
    let ids = BTreeMap::from([("Map.dbc".to_owned(), BTreeSet::from([0]))]);
    assert_eq!(
        basic_disposition(
            "spell_target_position",
            &t,
            &t.rows[0].1,
            &spells,
            &Nodes::new(),
            &ids
        ),
        "skip:target_coordinates"
    );
    let x = t.index("target_position_x");
    t.rows[0].1[x] = json!(1065353216);
    assert_eq!(
        basic_disposition(
            "spell_target_position",
            &t,
            &t.rows[0].1,
            &spells,
            &Nodes::new(),
            &ids
        ),
        "skip:database_target"
    );
    spells.get_mut(&10).unwrap()[13] = 17;
    assert_eq!(
        basic_disposition(
            "spell_target_position",
            &t,
            &t.rows[0].1,
            &spells,
            &Nodes::new(),
            &ids
        ),
        "loaded_input"
    );
    let map = t.index("target_map");
    t.rows[0].1[map] = json!(99);
    assert_eq!(
        basic_disposition(
            "spell_target_position",
            &t,
            &t.rows[0].1,
            &spells,
            &Nodes::new(),
            &ids
        ),
        "skip:target_map"
    );
}
#[test]
fn additional_pet_rows_use_the_source_short_validation_branch() {
    let mut w = [0; 25];
    w[1] = 3;
    let spells = Spells::from([(10, w), (11, [0; 25])]);
    let t = table(
        "spell_pet_auras",
        &[
            json!({"spell":10,"pet":1,"aura":99}),
            json!({"spell":10,"pet":2,"aura":11}),
            json!({"spell":10,"pet":3,"aura":99}),
            json!({"spell":11,"pet":4,"aura":10}),
        ],
    );
    assert_eq!(
        pet_inputs(&t, &spells)
            .values()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        [
            "skip:missing_aura",
            "loaded_input",
            "loaded_input:additional_pet",
            "skip:pet_dummy"
        ]
    );
}

#[test]
fn talent_zero_slot_is_not_spell_zero_even_when_sql_has_zero() {
    // SpellMgr.cpp:1302-1304 explicitly skips a zero RankID.
    let spells = BTreeMap::from([(0, [0; 25]), (1, [0; 25]), (2, [0; 25])]);
    let mut talent = vec![0; 21];
    talent[4] = 1;
    talent[5] = 2;
    let loaded = chains(
        &spells,
        &BTreeMap::from([(1, talent)]),
        &BTreeMap::new(),
        &table("spell_chain", &[]),
    )
    .unwrap();
    assert!(!loaded.nodes.contains_key(&0));
    assert_eq!(loaded.nodes[&2], [1, 1, 2, 0]);
}

#[test]
fn ability_chain_rank_narrows_to_uint8_without_saturating() {
    // SpellMgr.cpp:1378 and SpellMgr.h:1892 use uint8 rank.
    let spells = (1..=260).map(|id| (id, [0; 25])).collect();
    let mut talent = vec![0; 21];
    talent[4] = 1;
    talent[5] = 2;
    let abilities = (1..=260)
        .map(|id| {
            let mut w = vec![0; 15];
            w[2] = id;
            if id < 260 {
                w[8] = id + 1;
            }
            (id, w)
        })
        .collect();
    let loaded = chains(
        &spells,
        &BTreeMap::from([(1, talent)]),
        &abilities,
        &table("spell_chain", &[]),
    )
    .unwrap();
    assert_eq!(loaded.nodes[&255][2], 255);
    assert_eq!(loaded.nodes[&256][2], 0);
    assert_eq!(loaded.nodes[&257][2], 1);
}

#[test]
fn loaded_chain_traversal_uses_libstdcpp_insertion_order() {
    // SpellMgr.h:1895 and SpellMgr.cpp:1595-1605 traverse unordered_map.
    let spells = BTreeMap::from([(1, [0; 25]), (2, [0; 25]), (3, [0; 25])]);
    let sql = table(
        "spell_chain",
        &[
            json!({"spell_id":1,"prev_spell":0,"first_spell":1,"rank":1,"req_spell":0}),
            json!({"spell_id":2,"prev_spell":1,"first_spell":1,"rank":2,"req_spell":0}),
            json!({"spell_id":3,"prev_spell":1,"first_spell":1,"rank":2,"req_spell":0}),
        ],
    );
    let loaded = chains(&spells, &BTreeMap::new(), &BTreeMap::new(), &sql).unwrap();
    assert_eq!(loaded.order, [3, 2, 1]);
    assert_eq!(loaded.nodes.keys().copied().collect::<Vec<_>>(), [1, 2, 3]);
}
