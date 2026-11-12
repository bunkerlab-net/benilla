use super::super::super::production_compiler::VerifiedCatalog;
use super::super::SpellLoaderOwner;
use super::*;
use std::path::Path;

fn spell(effects: [u32; 3], a: [u32; 3], b: [u32; 3]) -> SpellTemplate {
    SpellTemplate {
        id: 1,
        effect1: effects[0],
        effect2: effects[1],
        effect3: effects[2],
        effectimplicittargeta1: a[0],
        effectimplicittargeta2: a[1],
        effectimplicittargeta3: a[2],
        effectimplicittargetb1: b[0],
        effectimplicittargetb2: b[1],
        effectimplicittargetb3: b[2],
        ..Default::default()
    }
}
fn data(s: &SpellTemplate) -> SpellTargetingData {
    initialize_spell(s, || Ok(&[])).unwrap()
}
fn mgr(s: &SpellTemplate) -> SourceSpellTargetMgr {
    SourceSpellTargetMgr {
        entries: BTreeMap::from([(s.id, Ok(data(s)))]),
    }
}

#[test]
fn effectless_entries_keep_map_value_initialization_and_skip_invalid_targets() {
    // SpellTargets.cpp:260,264-265; SpellTargets.h:45-50.
    let s = spell([0; 3], [u32::MAX; 3], [u32::MAX; 3]);
    assert_eq!(data(&s), SpellTargetingData::default());
    assert_eq!(
        SourceSpellTargetMgr::initialize(&SourceSpellStores::default())
            .entries()
            .len(),
        0
    );
}

#[test]
fn dynamic_precedence_and_logged_unknown_match_source() {
    use SpellTargetImplicitType::*;
    // SpellTargets.cpp:274-311. LOCK beats GO, GO beats UNIT+DEST.
    for (a, b, expected) in [
        (26, 23, Lock),
        (23, 26, Lock),
        (23, 6, Gameobject),
        (6, 23, Gameobject),
        (18, 6, UnitDest),
        (6, 18, UnitDest),
        (6, 22, Unit),
        (22, 6, Unit),
        (18, 22, LocationDest),
        (22, 18, LocationDest),
        (22, 0, None),
        (0, 0, None),
        (0, 22, Unknown),
        (11, 0, Unknown),
        (19, 0, Unknown),
    ] {
        let s = spell([3, 64, 77], [a; 3], [b; 3]);
        assert_eq!(data(&s).implicit_type, [expected; 3], "{a},{b}");
    }
    // DYNAMIC_ANY chooses DEST before the NONE branch, :300-308.
    assert_eq!(
        data(&spell([32; 3], [0; 3], [0; 3])).implicit_type,
        [LocationDest; 3]
    );
    // Fixed effect requirements ignore target types, :270-271.
    assert_eq!(
        data(&spell([5, 18, 27], [26; 3], [23; 3])).implicit_type,
        [UnitDest, Corpse, SpecialDest]
    );
}

#[test]
fn same_target_merges_in_slot_order_and_singles_ignore_chain_radius_differences() {
    // SpellTargets.cpp:315-429. Effect 0 A consumes all six equal targets.
    let mut s = spell([6; 3], [1; 3], [1; 3]);
    s.effectradiusindex2 = 10;
    s.effectchaintarget3 = 5;
    let d = data(&s);
    assert_eq!(d.target_mask, [[7, 1], [2, 2], [4, 4]]);
    assert_eq!(
        d.ignored_targets,
        [[false, true], [true, true], [true, true]]
    );
    // Source TARGET_UNIT is SINGLE although scheme switch mentions RANDOM_CHAIN.
    assert_eq!(
        data(&spell([6; 3], [25; 3], [0; 3])).filtering_scheme,
        [[SpellTargetFilterScheme::Random; 2]; 3]
    );
}

#[test]
fn chains_and_aoe_require_equal_chain_counts_and_radius_indexes() {
    // SpellTargets.cpp:396-402 compares raw index, not physical radius.
    let mut s = spell([2; 3], [6; 3], [0; 3]);
    s.effectchaintarget2 = 2;
    s.effectradiusindex3 = 1;
    assert_eq!(data(&s).ignored_targets, [[false; 2]; 3]);
    s.effectchaintarget2 = 0;
    assert_eq!(data(&s).target_mask, [[3, 1], [2, 2], [4, 4]]);
    assert_eq!(
        data(&s).ignored_targets,
        [[false, false], [true, false], [false, false]]
    );
    s.effectimplicittargeta1 = 15;
    s.effectimplicittargeta2 = 15;
    s.effectimplicittargeta3 = 15;
    s.effectchaintarget2 = 999;
    assert!(data(&s).ignored_targets[1][0]);
}

#[test]
fn area_aura_and_destination_exceptions_ignore_without_extending_masks() {
    // SpellTargets.cpp:371-375 bypasses mask extension at :414-415.
    for first in [35, 119, 128, 129] {
        let d = data(&spell([first, 2, 2], [15; 3], [0; 3]));
        assert_eq!(
            d.ignored_targets,
            [[false, false], [true, false], [true, false]]
        );
        assert_eq!(d.target_mask, [[1, 1], [2, 2], [4, 4]]);
    }
    let d = data(&spell([2, 28, 2], [6; 3], [0; 3]));
    assert_eq!(d.target_mask[0][0], 5);
    assert!(d.ignored_targets[1][0]);
    assert!(d.ignored_targets[2][0]);
    // NONE source requirement cannot consume a unit, :393-394,487-500.
    assert!(!data(&spell([7, 2, 0], [1; 3], [0; 3])).ignored_targets[1][0]);
}

#[test]
fn script_bounds_compare_every_inverse_mask_at_all_effect_pairs() {
    // SpellTargets.cpp:241-250,403-407; SpellMgr.h:1790.
    for source in 0..3 {
        for target in source + 1..3 {
            let mut effects = [0; 3];
            effects[source] = 2;
            effects[target] = 2;
            let s = spell(effects, [7; 3], [0; 3]);
            for mask in 0..8 {
                let rows = [
                    SpellScriptTarget {
                        kind: 1,
                        entry: 7,
                        inverse_effect_mask: mask,
                    },
                    SpellScriptTarget {
                        kind: 1,
                        entry: 8,
                        inverse_effect_mask: 0,
                    },
                ];
                let d = initialize_spell(&s, || Ok(&rows)).unwrap();
                let equal = mask & (1 << source) != 0;
                let other = mask & (1 << target) != 0;
                assert_eq!(d.ignored_targets[target][0], equal == other);
                assert_eq!(
                    d.target_mask[source][0],
                    (1 << source) | if equal == other { 1 << target } else { 0 }
                );
            }
            assert!(data(&s).ignored_targets[target][0]); // Empty bounds are equal.
        }
    }
    // A later differing row invalidates equality even after an equal row.
    let rows = [
        SpellScriptTarget {
            kind: 1,
            entry: 7,
            inverse_effect_mask: 0,
        },
        SpellScriptTarget {
            kind: 1,
            entry: 8,
            inverse_effect_mask: 2,
        },
    ];
    assert!(
        !initialize_spell(&spell([2, 2, 0], [7; 3], [0; 3]), || Ok(&rows))
            .unwrap()
            .ignored_targets[1][0]
    );
}

#[test]
fn unavailable_script_bounds_refuse_only_after_prior_merge_guards() {
    let fail = || {
        Err(SpellTargetError::ScriptBoundsOwner {
            spell: 1,
            refusal: Refusal {
                id: "execution.spells.creature_template_owner".into(),
            },
        })
    };
    let mut s = spell([2, 2, 0], [7; 3], [0; 3]);
    assert!(matches!(
        initialize_spell(&s, fail),
        Err(SpellTargetError::ScriptBoundsOwner { .. })
    ));
    s.effectradiusindex2 = 1;
    assert!(initialize_spell(&s, fail).is_ok());
    s.effectradiusindex2 = 0;
    s.effectimplicittargeta2 = 8;
    assert!(initialize_spell(&s, fail).is_ok());
    // Same effect A/B does not inspect cross-effect bounds, :403.
    assert!(initialize_spell(&spell([2, 0, 0], [7, 0, 0], [7, 0, 0]), fail).is_ok());
    // Area-aura exception bypasses bounds, :371.
    assert!(initialize_spell(&spell([35, 2, 0], [7; 3], [0; 3]), fail).is_ok());
}

#[test]
fn filtering_includes_ignored_targets_and_preserves_all_override_ids() {
    use SpellTargetFilterScheme::*;
    // SpellTargets.cpp:430-465,508-528, including unreachable switch case 25.
    let d = data(&spell([2, 2, 2], [6, 45, 38], [2, 3, 4]));
    assert_eq!(
        d.filtering_scheme,
        [
            [ClosestChain, ClosestChain],
            [LowestHpChain, ClosestChain],
            [Random, ClosestChain]
        ]
    );
    for id in [
        26052, 26180, 30284, 37144, 37146, 37148, 37151, 37152, 37153, 30469, 41294,
    ] {
        let mut s = spell([2; 3], [6; 3], [0; 3]);
        s.id = id;
        let d = data(&s);
        assert_eq!(d.filtering_scheme, [[Closest, Random]; 3]);
        assert!(d.ignored_targets[2][0]);
    }
    assert_eq!(
        SourceSpellTargetMgr::get_spell_targeting_filter_scheme(LowestHpChain, 42005),
        Furthest
    );
    assert_eq!(
        SourceSpellTargetMgr::get_spell_targeting_filter_scheme(PrioritizeMana, 1),
        PrioritizeMana
    );
}

#[test]
fn table_index_boundaries_refuse_without_clamping_or_skipping() {
    // SpellTargets.cpp:270,274,368,446. Last valid table indexes vs first UB.
    assert!(initialize_spell(&spell([129, 0, 0], [63; 3], [63; 3]), || Ok(&[])).is_ok());
    assert_eq!(
        initialize_spell(&spell([130, 0, 0], [0; 3], [0; 3]), || Ok(&[])),
        Err(SpellTargetError::EffectTableIndexUndefined {
            spell: 1,
            effect: 0,
            value: 130
        })
    );
    for right in 0..2 {
        let mut a = [0; 3];
        let mut b = [0; 3];
        if right == 0 {
            a[2] = 64;
        } else {
            b[2] = 64;
        }
        assert_eq!(
            initialize_spell(&spell([0, 0, 3], a, b), || Ok(&[])),
            Err(SpellTargetError::TargetTableIndexUndefined {
                spell: 1,
                effect: 2,
                right,
                value: 64
            })
        );
    }
}

#[test]
fn lookup_missing_spell_and_effect_boundaries_are_typed_and_jump_radius_is_unconditional() {
    // SpellTargets.cpp:237,472,503-505; SharedDefines.h:1623.
    let s = spell([2, 2, 2], [6; 3], [0; 3]);
    let m = mgr(&s);
    assert!(m.get_spell_targeting_data(1).is_ok());
    assert_eq!(
        m.get_spell_targeting_data(0),
        Err(SpellTargetError::MissingSpellUndefined { spell: 0 })
    );
    assert!(m
        .can_effect_be_filled_with_mask(1, 2, TARGET_FLAG_UNIT)
        .unwrap());
    assert_eq!(
        m.can_effect_be_filled_with_mask(1, 3, TARGET_FLAG_UNIT),
        Err(SpellTargetError::EffectIndexUndefined {
            spell: 1,
            effect: 3
        })
    );
    assert_eq!(
        m.can_effect_be_filled_with_mask(0, 3, 0),
        Err(SpellTargetError::MissingSpellUndefined { spell: 0 })
    );
    for id in [0, 1, u32::MAX] {
        assert_eq!(
            SourceSpellTargetMgr::get_jump_radius(id).to_bits(),
            0x41200000
        );
    }
}

#[test]
fn fill_mask_uses_exact_source_flags_not_related_party_trade_or_source_flags() {
    // SpellTargets.cpp:474-483, not a general target compatibility predicate.
    for (effect, yes, no) in [
        (27, 0x40, 0x20),
        (28, 0x40, 0x20),
        (86, 0x4800, 0x10),
        (5, 0x5c2, 0x8),
        (6, 0x582, 0x40),
        (18, 0x8200, 0x400),
        (33, 0x4810, 0x1000),
        (53, 0x4010, 0x800),
        (20, 0, 0xffff_ffff),
    ] {
        let m = mgr(&spell([effect; 3], [0; 3], [0; 3]));
        for bit in 0..32 {
            let flag = 1 << bit;
            if flag & yes != 0 {
                assert!(m.can_effect_be_filled_with_mask(1, 0, flag).unwrap());
            }
            if flag & no != 0 {
                assert!(!m.can_effect_be_filled_with_mask(1, 0, flag).unwrap());
            }
        }
    }
}

#[test]
fn real_catalog_target_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP SpellTargetMgr catalog census: set BENILLA_F5_AI_CATALOG");
        return;
    };
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let stores = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let m = SourceSpellTargetMgr::initialize(&stores);
    assert_eq!(m.entries().len(), 22374);
    let mut implicit = BTreeMap::new();
    let mut filter = BTreeMap::new();
    let mut schemes = BTreeMap::new();
    let mut refused = BTreeMap::new();
    let mut refused_spells = BTreeMap::new();
    let mut complete = 0;
    let mut missing = Vec::new();
    let mut undefined = Vec::new();
    for (&id, entry) in m.entries() {
        match entry {
            Ok(d) => {
                complete += 1;
                for t in d.implicit_type {
                    *implicit.entry(format!("{t:?}")).or_insert(0usize) += 1;
                }
                for row in d.filtering_scheme {
                    for scheme in row {
                        *schemes.entry(format!("{scheme:?}")).or_insert(0usize) += 1;
                    }
                }
            }
            Err(SpellTargetError::ScriptBoundsOwner { refusal, .. }) => {
                *refused.entry(refusal.id.clone()).or_insert(0usize) += 1;
                refused_spells.insert(id, refusal.id.clone());
            }
            Err(error) => undefined.push((id, format!("{error:?}"))),
        }
    }
    // Constructor asserts server SpellEntry identity, Spell.cpp:387-393.
    for (&id, s) in stores.templates() {
        if !m.entries().contains_key(&id) {
            missing.push(id);
        }
        for e in s.effects() {
            if e.effect != 0 {
                for target in [e.target_a, e.target_b] {
                    if target != 0 {
                        if let Some(t) = SPELL_TARGET_INFO_TABLE.get(target as usize) {
                            *filter.entry(format!("{:?}", t.filter)).or_insert(0usize) += 1;
                        }
                    }
                }
            }
        }
    }
    let census = serde_json::json!({"catalog_sha256":catalog.execution_identity()["catalog_sha256"],"entries":m.entries().len(),"complete":complete,"refused":refused,"refused_spells":refused_spells,"implicit_type_including_inactive_complete_only":implicit,"filter_active_nonzero_targets_all_templates":filter,"filtering_scheme_including_inactive_complete_only":schemes,"constructor_missing_entries":missing,"undefined":undefined});
    println!("SpellTargetMgr census {census}");
    assert_eq!(complete, 22331);
    assert_eq!(
        census["refused"],
        serde_json::json!({
            "execution.spells.creature_template_owner": 36,
            "execution.spells.gameobject_template_owner": 7
        })
    );
    assert_eq!(refused_spells.len(), 43);
    assert_eq!(
        census["implicit_type_including_inactive_complete_only"],
        serde_json::json!({"Corpse":31,"Gameobject":137,"Item":364,
            "LocationDest":1226,"Lock":126,"None":406,"SpecialDest":137,
            "SpecialUnit":205,"Unit":23978,"UnitDest":193,"Unknown":40190})
    );
    assert_eq!(
        census["filter_active_nonzero_targets_all_templates"],
        serde_json::json!({"Group":66,"Harmful":5723,"Helpful":13905,
            "Neutral":3255,"Party":126,"Script":1139})
    );
    assert_eq!(
        census["filtering_scheme_including_inactive_complete_only"],
        serde_json::json!({"Closest":4,"ClosestChain":4046,
            "LowestHpChain":16,"Random":129920})
    );
    assert!(undefined.is_empty(), "stock undefined reach: {undefined:?}");
    assert!(missing.is_empty());
    if let Some(path) = std::env::var_os("BENILLA_F5_TARGET_CENSUS_OUTPUT") {
        let path = Path::new(&path);
        assert!(path.starts_with("/home/robbie/Work/benilla/benilla-config/"));
        std::fs::write(path, serde_json::to_vec_pretty(&census).unwrap()).unwrap();
    }
}
