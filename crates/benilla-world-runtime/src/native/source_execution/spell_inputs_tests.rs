use super::*;
use serde_json::{json, Value};
struct Missing;
impl SpellLoaderOwner for Missing {}
struct Known;
impl SpellLoaderOwner for Known {
    fn contains(&mut self, _: SpellDependency, id: u32) -> Result<bool> {
        Ok(id != 999)
    }
    fn skinning_loot(&mut self, _: u32) -> Result<u32> {
        Ok(0)
    }
    fn script(&mut self, kind: SpellScriptKind, name: &[u8]) -> Result<Option<SpellScriptBinding>> {
        Ok(
            (name == b"both" || name == b"spell" && kind == SpellScriptKind::Spell).then(|| {
                SpellScriptBinding {
                    name: name.into(),
                    registration: 7,
                }
            }),
        )
    }
}
fn fixture() -> SourceSpellStores {
    let mut s = SourceSpellStores::default();
    for id in 1..=20 {
        s.insert_template(SpellTemplate {
            id,
            ..Default::default()
        })
        .unwrap();
    }
    s.dbc.insert(
        "Map.dbc".into(),
        BTreeMap::from([(
            1,
            SpellDbcRecord {
                ordinal: 0,
                words: BTreeMap::new(),
            },
        )]),
    );
    s.dbc.insert(
        "AreaTable.dbc".into(),
        BTreeMap::from([(
            1,
            SpellDbcRecord {
                ordinal: 0,
                words: BTreeMap::new(),
            },
        )]),
    );
    s
}
fn load(s: &mut SourceSpellStores, name: &'static str, v: Value) {
    let ordinal = s.loads.get(name).map_or(0, |r| r.len()) as u64;
    s.load_row(name, ordinal, v.as_array().unwrap(), &mut Known);
}
fn skipped(s: &SourceSpellStores, name: &str, reason: &str) {
    assert_eq!(
        s.loads[name].last().unwrap().disposition,
        SpellRowDisposition::Skipped(reason.into())
    );
}
fn chain(s: &mut SourceSpellStores, id: u32, prev: u32, first: u32, rank: u8, req: u32) {
    s.chains.insert(
        id,
        SpellChain {
            previous: prev,
            first,
            rank,
            required: req,
        },
    );
    // Authored source iteration order for these isolated rank-worker tests.
    s.chain_order.push(id);
}
#[test]
fn spell_store_absence_is_not_client_identity() {
    let mut stores = SourceSpellStores::default();
    stores.client_ids.insert(999);
    assert!(stores.client_contains(999));
    assert!(stores.template(999).is_none());
    assert_eq!(stores.facing(999), 0);
    assert_eq!(stores.first_rank(999), 999);
    assert!(stores.threat(999).is_none());
    assert!(stores.proc_event(999).is_none());
    assert_eq!(stores.script_targets(999).unwrap(), &[]);
    assert_eq!(stores.areas(999).unwrap(), &[]);
    assert_eq!(stores.elixir_mask(999), 0);
    assert_eq!(stores.affect_mask(999, 3).unwrap(), 0);
}
#[test]
fn full_sql_template_preserves_signed_words_float_bits_and_strings() {
    let policy: Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../scripts/classic_export/catalog_spell_policy.json"
    )))
    .unwrap();
    let fields = policy["tables"]["spell_template"]["fields"]
        .as_array()
        .unwrap();
    let mut v: Vec<_> = fields
        .iter()
        .map(|f| if f[1] == "text" { json!("") } else { json!(0) })
        .collect();
    for (name, value) in [
        ("id", json!(7)),
        ("effectdiesides3", json!(4294967295u32)),
        ("spellfamilyflags", json!(9223372036854775809u64)),
        ("speed", json!(2147483648u32)),
        ("spellname", json!("6162")),
        ("rank8", json!("ff")),
    ] {
        let i = fields.iter().position(|f| f[0] == name).unwrap();
        v[i] = value;
    }
    let t = SpellTemplate::decode(8, &v).unwrap();
    assert_eq!(t.id, 7);
    assert_eq!(t.ordinal, 8);
    assert_eq!(t.effectdiesides3, -1);
    assert_eq!(t.spellfamilyflags, 0x8000000000000001);
    assert_eq!(t.speed, SpellFloat(0x80000000));
    assert_eq!(&*t.spellname, b"ab");
    assert_eq!(&*t.rank8, &[255]);
    assert!(SpellTemplate::decode(0, &v[..v.len() - 1]).is_err());
}
#[test]
fn sql_duplicate_overwrites_without_client_fallback() {
    let mut s = fixture();
    s.insert_template(SpellTemplate {
        id: 1,
        school: 9,
        ordinal: 2,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(s.template(1).unwrap().school, 9);
    assert_eq!(s.template(1).unwrap().ordinal, 2);
    assert!(!s.client_contains(1));
    assert!(s.template(999).is_none());
}
#[test]
fn sql_extent_wrap_refuses_instead_of_skipping() {
    let mut s = SourceSpellStores::default();
    s.insert_template(SpellTemplate {
        id: u32::MAX - 1,
        ..Default::default()
    })
    .unwrap();
    assert_eq!(
        s.insert_template(SpellTemplate {
            id: u32::MAX,
            ..Default::default()
        })
        .unwrap_err()
        .id,
        "execution.spells.sql_index_write_undefined"
    );
}
#[test]
fn recursive_triggers_keep_all_slots_cycles_and_absence() {
    let mut s = fixture();
    s.templates.get_mut(&1).unwrap().effecttriggerspell1 = 2;
    s.templates.get_mut(&1).unwrap().effecttriggerspell2 = 999;
    s.templates.get_mut(&1).unwrap().effecttriggerspell3 = 3;
    s.templates.get_mut(&2).unwrap().effecttriggerspell3 = 1;
    let d = s.trigger_dependencies(1);
    assert_eq!(d.len(), 4);
    assert_eq!(d[&1], Some([2, 999, 3]));
    assert_eq!(d[&2], Some([0, 0, 1]));
    assert_eq!(d[&999], None);
}
#[test]
fn facing_and_cones_keep_last_duplicate_and_skip_only_missing_facing_spell() {
    let mut s = fixture();
    load(&mut s, "spell_facing", json!([1, 3]));
    load(&mut s, "spell_facing", json!([1, 8]));
    load(&mut s, "spell_facing", json!([999, 2]));
    skipped(&s, "spell_facing", "missing_spell");
    assert_eq!(s.facing(1), 8);
    load(&mut s, "spell_cone", json!([999, 4294967206u32]));
    assert_eq!(s.cone(999), Some(-90));
}
#[test]
fn target_position_validates_map_coordinates_and_third_slot_in_source_order() {
    let mut s = fixture();
    load(
        &mut s,
        "spell_target_position",
        json!([999, 999, 0, 0, 0, 0]),
    );
    skipped(&s, "spell_target_position", "target_map");
    load(
        &mut s,
        "spell_target_position",
        json!([999, 1, 0, 0, 2147483648u32, 0]),
    );
    skipped(&s, "spell_target_position", "target_coordinates");
    let spell = s.templates.get_mut(&1).unwrap();
    spell.effectimplicittargetb3 = 17;
    load(
        &mut s,
        "spell_target_position",
        json!([1, 1, 1065353216, 0, 0, 0]),
    );
    load(
        &mut s,
        "spell_target_position",
        json!([1, 1, 1073741824, 0, 0, 0]),
    );
    assert_eq!(
        s.target_position(1).unwrap().position[0],
        SpellFloat(2f32.to_bits())
    );
    load(
        &mut s,
        "spell_target_position",
        json!([2, 1, 1065353216, 0, 0, 0]),
    );
    skipped(&s, "spell_target_position", "database_target");
}
#[test]
fn groups_first_win_membership_duplicates_and_zero_rule_fallthrough() {
    let mut s = fixture();
    load(&mut s, "spell_group", json!([1, 0, "61"]));
    load(&mut s, "spell_group", json!([1, 2, "62"]));
    load(&mut s, "spell_group", json!([2, 1, "63"]));
    load(&mut s, "spell_group_spell", json!([1, 1]));
    load(&mut s, "spell_group_spell", json!([1, 1]));
    load(&mut s, "spell_group_spell", json!([2, 1]));
    s.build_stacks();
    assert_eq!(&*s.group(1).unwrap().name, b"a");
    assert_eq!(s.group(1).unwrap().spells, vec![1, 1]);
    assert_eq!(s.stack(1).unwrap(), Some(&SpellStack { mask: 3, rule: 1 }));
    load(&mut s, "spell_group", json!([5, 3, ""]));
    skipped(&s, "spell_group", "invalid_rule");
    load(&mut s, "spell_group", json!([6, 4294967295u32, ""]));
    assert_eq!(s.group(6).unwrap().rule, -1);
}
#[test]
fn group_shifts_validate_only_reached_memberships_on_both_sides() {
    for group in [0, 1, 64, 65] {
        let mut s = fixture();
        load(&mut s, "spell_group", json!([group, 1, ""]));
        load(&mut s, "spell_group_spell", json!([group, 999]));
        s.build_stacks();
        assert!(s.stack(999).unwrap().is_none());
        load(&mut s, "spell_group_spell", json!([group, 1]));
        s.build_stacks();
        if [1, 64].contains(&group) {
            assert_eq!(
                s.stack(1).unwrap().unwrap().mask,
                if group == 1 { 1 } else { 1u64 << 63 }
            );
        } else {
            assert_eq!(
                s.stack(1).unwrap_err().id,
                "execution.spells.stack_shift_undefined"
            );
        }
    }
}
#[test]
fn threat_custom_ranks_and_required_edges_preserve_explicit_data() {
    let mut s = fixture();
    chain(&mut s, 1, 0, 1, 1, 0);
    chain(&mut s, 2, 1, 1, 2, 0);
    chain(&mut s, 3, 2, 1, 3, 0);
    chain(&mut s, 4, 0, 4, 1, 1);
    s.build_next();
    load(&mut s, "spell_threat", json!([2, 0, 1065353216, 0, 0]));
    skipped(&s, "spell_threat", "nonfirst_rank");
    load(&mut s, "spell_threat", json!([1, 5, 1065353216, 0, 0]));
    load(&mut s, "spell_threat", json!([2, 9, 1073741824, 0, 4]));
    s.fill_ranks("spell_threat").unwrap();
    assert_eq!(s.threat(2).unwrap().threat, 9);
    assert_eq!(s.threat(3).unwrap().threat, 5);
    assert_eq!(s.threat(4).unwrap().threat, 5);
    assert_eq!(s.next_ranks(1), &[2, 4]);
}
#[test]
fn proc_custom_ppm_is_the_only_nonfirst_rank_admission_test() {
    let mut s = fixture();
    chain(&mut s, 2, 1, 1, 2, 0);
    s.build_next();
    load(
        &mut s,
        "spell_proc_event",
        json!([2, 0, 0, 0, 0, 0, 0, 0, 2147483648u32, 0, 0]),
    );
    skipped(&s, "spell_proc_event", "nonfirst_rank");
    load(
        &mut s,
        "spell_proc_event",
        json!([1, 7, 8, 9223372036854775809u64, 2, 3, 4, 5, 0, 0, 6]),
    );
    load(
        &mut s,
        "spell_proc_event",
        json!([2, 9, 10, 1, 2, 3, 4, 5, 1065353216, 0, 6]),
    );
    s.fill_ranks("spell_proc_event").unwrap();
    assert_eq!(s.proc_event(1).unwrap().family_masks[0], 0x8000000000000001);
    assert_eq!(s.proc_event(2).unwrap().school_mask, 9);
}
fn proc_boundary(extra: bool) -> SourceSpellStores {
    let mut s = fixture();
    for id in 1..=13 {
        load(
            &mut s,
            "spell_proc_event",
            json!([id, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        );
    }
    chain(&mut s, 14, 1, 1, 2, 0);
    if extra {
        chain(&mut s, 15, 14, 1, 3, 0);
    }
    s.build_next();
    s
}
#[test]
fn proc_rehash_last_call_is_defined_but_next_state_dereference_refuses() {
    let mut s = proc_boundary(false);
    s.fill_ranks("spell_proc_event").unwrap();
    assert_eq!(
        s.proc_rehashes(),
        &[SpellProcRehash {
            root: 1,
            child: 14,
            before: 13,
            after: 14,
            buckets_before: 13,
            buckets_after: 29,
            following_dereferences: 0
        }]
    );
    let mut s = proc_boundary(true);
    assert_eq!(
        s.fill_ranks("spell_proc_event").unwrap_err().id,
        "execution.spells.proc_state_iterator_undefined"
    );
    assert!(s.proc_event(14).is_some());
    assert!(s.proc_event(15).is_none());
}
#[test]
fn enchant_procs_overwrite_higher_ranks_at_each_input_row() {
    let mut s = fixture();
    chain(&mut s, 2, 1, 1, 2, 0);
    s.build_next();
    load(&mut s, "spell_proc_item_enchant", json!([1, 1065353216]));
    load(&mut s, "spell_proc_item_enchant", json!([2, 1073741824]));
    skipped(&s, "spell_proc_item_enchant", "nonfirst_rank");
    load(&mut s, "spell_proc_item_enchant", json!([1, 1077936128]));
    assert_eq!(s.enchant_proc(2), Some(SpellFloat(3f32.to_bits())));
}
#[test]
fn script_target_bad_row_erases_entire_key_and_later_valid_rows_do_not_reinsert() {
    let mut s = fixture();
    s.templates.get_mut(&1).unwrap().effectimplicittargeta3 = 38;
    load(&mut s, "spell_script_target", json!([1, 1, 2, 1]));
    load(&mut s, "spell_script_target", json!([1, 1, 999, 2]));
    load(&mut s, "spell_script_target", json!([1, 0, 0, 4]));
    assert!(s.script_targets(1).unwrap().is_empty());
}
#[test]
fn script_target_all_types_and_source_zero_exceptions() {
    let mut s = fixture();
    for id in 1..=6 {
        s.templates.get_mut(&id).unwrap().effect3 = 27;
        load(&mut s, "spell_script_target", json!([id, id - 1, 2, 7]));
        assert_eq!(s.script_targets(id).unwrap().len(), 1);
    }
    load(&mut s, "spell_script_target", json!([6, 6, 2, 0]));
    skipped(&s, "spell_script_target", "target_type");
    load(&mut s, "spell_script_target", json!([1, 0, 0, 0]));
    assert!(s.script_targets(1).unwrap().iter().any(|t| t.entry == 0));
    load(&mut s, "spell_script_target", json!([2, 1, 0, 0]));
    skipped(&s, "spell_script_target", "zero_creature");
}
#[test]
fn unresolved_external_target_never_becomes_a_negative_lookup() {
    let mut s = fixture();
    s.templates.get_mut(&1).unwrap().effect3 = 27;
    s.load_row(
        "spell_script_target",
        0,
        &[json!(1), json!(1), json!(2), json!(0)],
        &mut Missing,
    );
    assert_eq!(
        s.script_targets(1).unwrap_err().id,
        "execution.spells.creature_template_owner"
    );
    assert!(s.script_targets(2).unwrap().is_empty());
}
#[test]
fn area_zero_racemasks_are_not_duplicates_and_condition_bypasses_old_requirements() {
    let mut s = fixture();
    for _ in 0..2 {
        load(&mut s, "spell_area", json!([1, 1, 0, 0, 0, 0, 0, 0, 2, 0]));
    }
    assert_eq!(s.areas(1).unwrap().len(), 2);
    load(
        &mut s,
        "spell_area",
        json!([2, 1, 999, 0, 999, 1, 0, 256, 3, 1]),
    );
    assert_eq!(s.areas(2).unwrap()[0].gender, 3);
    load(&mut s, "spell_area", json!([3, 1, 0, 0, 0, 0, 0, 1, 0, 0]));
    load(
        &mut s,
        "spell_area",
        json!([3, 1, 0, 1, 999, 1, 0, 1, 0, 1]),
    );
    skipped(&s, "spell_area", "duplicate_requirements");
    assert_eq!(s.areas_for_area(1).unwrap().len(), 4);
}
#[test]
fn area_gender_conversion_precedes_missing_spell_and_abs_is_lazy() {
    let mut s = fixture();
    load(
        &mut s,
        "spell_area",
        json!([999, 0, 0, 0, 0, 0, 0, 0, 3, 0]),
    );
    skipped(&s, "spell_area", "missing_spell");
    s.load_row(
        "spell_area",
        1,
        &[999, 0, 0, 0, 0, 0, 0, 0, 4, 0].map(|x| json!(x)),
        &mut Known,
    );
    assert_eq!(
        s.areas(999).unwrap_err().id,
        "execution.spells.gender_conversion_undefined"
    );
    load(
        &mut s,
        "spell_area",
        json!([1, 0, 0, 0, 0, 0, 2147483649u32, 0, 2, 0]),
    );
    skipped(&s, "spell_area", "missing_aura");
    s.load_row(
        "spell_area",
        3,
        &[1u32, 0, 0, 0, 0, 0, 2147483648, 0, 2, 0].map(|x| json!(x)),
        &mut Known,
    );
    assert_eq!(
        s.areas(1).unwrap_err().id,
        "execution.spells.area_abs_undefined"
    );
}
#[test]
fn aura_third_slot_and_forward_reverse_autocast_chains() {
    let mut s = fixture();
    for id in 1..=3 {
        s.templates.get_mut(&id).unwrap().effectapplyauraname3 = 4;
    }
    load(&mut s, "spell_area", json!([1, 0, 0, 0, 0, 0, 2, 0, 2, 1]));
    load(&mut s, "spell_area", json!([2, 0, 0, 0, 0, 0, 3, 0, 2, 1]));
    skipped(&s, "spell_area", "autocast_chain");
    load(&mut s, "spell_area", json!([3, 0, 0, 0, 0, 0, 1, 0, 2, 1]));
    skipped(&s, "spell_area", "autocast_chain");
    load(
        &mut s,
        "spell_area",
        json!([3, 0, 0, 0, 0, 0, 4294967294u32, 0, 2, 1]),
    );
    assert_eq!(s.areas(3).unwrap().len(), 1);
    assert_eq!(s.areas_for_aura(2).unwrap().len(), 2);
}
#[test]
fn missing_condition_owner_blocks_alternatives_and_secondary_indices() {
    let mut s = fixture();
    s.load_row(
        "spell_area",
        0,
        &[1, 1, 0, 0, 0, 1, 0, 0, 0, 0].map(|x| json!(x)),
        &mut Missing,
    );
    assert_eq!(
        s.areas(1).unwrap_err().id,
        "execution.spells.condition_storage_owner"
    );
    assert!(s.areas_for_area(1).is_err());
}
#[test]
fn affect_masks_first_win_redundancy_and_fallback_index_boundary() {
    let mut s = fixture();
    let t = s.templates.get_mut(&1).unwrap();
    t.effect3 = 6;
    t.effectapplyauraname3 = 107;
    t.effectitemtype3 = 7;
    load(&mut s, "spell_affect", json!([1, 2, 7]));
    skipped(&s, "spell_affect", "redundant_mask");
    load(&mut s, "spell_affect", json!([1, 2, 8]));
    load(&mut s, "spell_affect", json!([1, 2, 9]));
    assert_eq!(s.affect_mask(1, 2).unwrap(), 8);
    assert_eq!(
        s.affect_mask(1, 3).unwrap_err().id,
        "execution.spells.affect_effect_index_undefined"
    );
    load(&mut s, "spell_affect", json!([1, 3, 9]));
    skipped(&s, "spell_affect", "effect_index");
}
#[test]
fn pet_aura_narrows_key_and_additional_pet_skips_aura_validation() {
    let mut s = fixture();
    let t = s.templates.get_mut(&1).unwrap();
    t.effect3 = 3;
    t.effectbasepoints3 = 8;
    t.effectbasedice3 = 2;
    t.effectimplicittargeta3 = 5;
    load(&mut s, "spell_pet_auras", json!([1, 0, 2]));
    load(&mut s, "spell_pet_auras", json!([65537, 7, 999]));
    let a = s.pet_aura(1).unwrap();
    assert_eq!(a.damage, 10);
    assert!(a.remove_on_change_pet);
    assert_eq!(a.auras[&7], 999);
}
#[test]
fn simple_value_signed_overflow_is_guarded_after_real_effect_selection() {
    let mut s = fixture();
    let t = s.templates.get_mut(&1).unwrap();
    t.effect3 = 118;
    t.effectmiscvalue3 = 7;
    t.effectbasepoints3 = i32::MAX - 1;
    t.effectbasedice3 = 1;
    s.load_learn_skills().unwrap();
    assert_eq!(s.learned_skill(1), Some((7, 65535, 118)));
    s.templates.get_mut(&1).unwrap().effectbasepoints3 = i32::MAX;
    assert_eq!(
        s.load_learn_skills().unwrap_err().id,
        "execution.spells.simple_value_overflow_undefined"
    );
}
#[test]
fn learning_covers_all_slots_preserves_sql_duplicates_and_empty_sql_early_return() {
    let mut s = fixture();
    let t = s.templates.get_mut(&1).unwrap();
    t.effect1 = 36;
    t.effecttriggerspell1 = 2;
    t.effect3 = 36;
    t.effecttriggerspell3 = 3;
    assert!(s.learned_spells(1).is_empty());
    load(&mut s, "spell_learn_spell", json!([1, 2, 0]));
    load(&mut s, "spell_learn_spell", json!([1, 2, 1]));
    s.add_template_learning().unwrap();
    assert_eq!(
        s.learned_spells(1),
        &[
            SpellLearned {
                spell: 2,
                active: false,
                auto_learned: false
            },
            SpellLearned {
                spell: 2,
                active: true,
                auto_learned: false
            },
            SpellLearned {
                spell: 3,
                active: true,
                auto_learned: false
            }
        ]
    );
}
#[test]
fn script_registries_are_independent_first_wins_and_missing_names_skip() {
    let mut s = fixture();
    assert_eq!(
        s.bind_script(999, b"both", &mut Missing).unwrap(),
        Some("missing_spell")
    );
    assert_eq!(
        s.bind_script(1, b"absent", &mut Known).unwrap(),
        Some("missing_script")
    );
    s.bind_script(1, b"spell", &mut Known).unwrap();
    s.bind_script(1, b"both", &mut Known).unwrap();
    assert_eq!(
        &*s.script_binding(SpellScriptKind::Spell, 1)
            .unwrap()
            .unwrap()
            .name,
        b"spell"
    );
    assert_eq!(
        &*s.script_binding(SpellScriptKind::Aura, 1)
            .unwrap()
            .unwrap()
            .name,
        b"both"
    );
    assert!(s.bind_script(2, b"both", &mut Missing).is_err());
}

#[test]
fn real_catalog_spell_census_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP F5.07 spell catalog census: set BENILLA_F5_AI_CATALOG");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let s = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    s.validate().unwrap();
    assert_eq!(s.inventory()["version"], 1);
    assert_eq!(s.templates.len(), 22374);
    assert_eq!(s.client_identity().unwrap().1, 22357);
    assert_eq!(s.chains.len(), 2892);
    assert_eq!(
        s.proc_rehashes(),
        &[SpellProcRehash {
            root: 12880,
            child: 14204,
            before: 257,
            after: 258,
            buckets_before: 257,
            buckets_after: 541,
            following_dereferences: 0
        }]
    );
    assert_eq!(s.next_ranks(12880), &[14201]);
    assert_eq!(s.next_ranks(14201), &[14202]);
    assert_eq!(s.next_ranks(14202), &[14203]);
    assert_eq!(s.next_ranks(14203), &[14204]);
    assert!(s.next_ranks(14204).is_empty());
    let roots = s.loads["spell_proc_event"]
        .iter()
        .filter(|r| r.disposition == SpellRowDisposition::Loaded)
        .map(|r| s.first_rank(r.key))
        .collect::<BTreeSet<_>>();
    let mut keys = s.loads["spell_proc_event"]
        .iter()
        .filter(|r| r.disposition == SpellRowDisposition::Loaded)
        .map(|r| r.key)
        .collect::<BTreeSet<_>>();
    assert_eq!(keys.len(), 203);
    let mut adding_roots = 0;
    for root in roots {
        if !keys.contains(&root) {
            continue;
        }
        let mut added = false;
        for child in s.higher(root).unwrap() {
            added |= keys.insert(child);
        }
        adding_roots += usize::from(added);
    }
    assert_eq!(adding_roots, 61);
    assert_eq!(keys.len(), 375);
    let mut summary = BTreeMap::new();
    for (store, rows) in s.loads() {
        let mut counts = BTreeMap::<String, usize>::new();
        for row in rows {
            *counts
                .entry(match &row.disposition {
                    SpellRowDisposition::Loaded => "loaded".into(),
                    SpellRowDisposition::Skipped(r) => format!("skipped:{r}"),
                    SpellRowDisposition::Refused(e) => format!("refused:{}", e.id),
                })
                .or_default() += 1;
        }
        println!("F5.07 {store} {counts:?}");
        summary.insert(*store, counts);
    }
    let guards = json!({"template_max_id":s.templates.keys().max(),"area_abs_int_min":s.loads["spell_area"].iter().filter(|r| matches!(&r.disposition,SpellRowDisposition::Refused(e) if e.id.ends_with("undefined"))).count(),"proc_rehashes":s.proc_rehashes(),"group_shift_refusals":s.loads["spell_group_spell"].iter().filter(|r| matches!(&r.disposition,SpellRowDisposition::Refused(e) if e.id.ends_with("undefined"))).count()});
    println!("F5.07 guards {guards}");
    assert!(!s.loads.values().flatten().any(
        |r| matches!(&r.disposition,SpellRowDisposition::Refused(e) if e.id.ends_with("undefined"))
    ));
    println!("F5.07 expanded stores proc={} threat={} enchant={} stack={} affect={} learned={} skills={} pet={}",s.procs.len(),s.threats.len(),s.enchant_procs.len(),s.stacks.len(),s.affects.len(),s.learned.values().map(Vec::len).sum::<usize>(),s.skills.len(),s.pet_auras.len());
    let Some(config) = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG") else {
        println!("SKIP F5.07 EventAI dependency census: set BENILLA_F5_EVENT_AI_CONFIG");
        return;
    };
    let config = EventAiLoaderConfig::recorded(
        Path::new(&config),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut owner = EventAiCatalog::new(&mut catalog, config);
    let mut dependent = 0;
    let mut met = 0;
    let mut blocked = BTreeMap::<String, usize>::new();
    for row in rows {
        let Ok(EventAiLoad {
            disposition: EventAiDisposition::Loaded(e),
            ..
        }) = load_event_ai_row(&mut owner, row)
        else {
            continue;
        };
        if !e.actions.iter().any(|a| [11, 52, 60].contains(&a.kind)) {
            continue;
        }
        dependent += 1;
        let result = e.actions.iter().try_for_each(|a| match a.kind {
            11 => s.table_dependencies(a.params.expect("CAST loader params")[0]),
            60 => Err(refuse("execution.spells.creature_spell_list_owner")),
            _ => Ok(()),
        });
        match result {
            Ok(()) => met += 1,
            Err(e) => *blocked.entry(e.id).or_default() += 1,
        }
    }
    assert_eq!(dependent, 7724);
    println!("F5.07 EventAI dependencies total={dependent} met={met} blocked={blocked:?}; zero end-to-end casts claimed");
    assert_eq!(met, 7415);
    let evidence = json!({"catalog_sha256":catalog.execution_identity()["catalog_sha256"],"stores":summary,"guards":guards,"inventory":s.inventory(),"proc_roots_adding":adding_roots,"event_ai":{"dependent":dependent,"met":met,"blocked":blocked}});
    if let Some(path) = std::env::var_os("BENILLA_F5_SPELL_CENSUS_OUTPUT") {
        let path = Path::new(&path);
        let private_root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .join("benilla-config");
        assert!(path.starts_with(private_root));
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn next_rank_equal_range_retains_hash_iteration_instead_of_sorting() {
    let mut s = fixture();
    chain(&mut s, 1, 0, 1, 1, 0);
    chain(&mut s, 2, 1, 1, 2, 0);
    chain(&mut s, 3, 1, 1, 2, 0);
    let mut order = super::super::identity_table::IdentityTable::default();
    for id in [1, 2, 3] {
        order.insert(id, (), 100).unwrap();
    }
    s.chain_order = order.iter().map(|(id, ())| id as u32).collect();
    assert_eq!(s.chain_order, [3, 2, 1]);
    s.build_next();
    assert_eq!(s.next_ranks(1), &[3, 2]);
}

#[test]
fn pet_aura_negative_lookup_falls_back_to_pet_zero_then_zero() {
    let a = SpellPetAura {
        auras: BTreeMap::from([(0, 7), (2, 8)]),
        remove_on_change_pet: false,
        damage: 0,
    };
    assert_eq!(a.aura(2), 8);
    assert_eq!(a.aura(999), 7);
    let a = SpellPetAura {
        auras: BTreeMap::new(),
        remove_on_change_pet: false,
        damage: 0,
    };
    assert_eq!(a.aura(999), 0);
}

#[test]
fn defined_threat_and_enchant_negative_results_do_not_require_a_spell() {
    let s = fixture();
    assert_eq!(s.threat_multiplier(None), SpellFloat(1f32.to_bits()));
    assert_eq!(s.threat_multiplier(Some(999)), SpellFloat(1f32.to_bits()));
    assert_eq!(s.enchant_proc_chance(999), SpellFloat(0));
    assert_eq!(
        s.validate().unwrap_err().id,
        "execution.spells.catalog_binding_missing"
    );
}

#[test]
fn signed_simple_value_lower_boundary_and_pet_overflow_remain_distinct_operations() {
    let mut s = fixture();
    let t = s.templates.get_mut(&1).unwrap();
    t.effect3 = 118;
    t.effectmiscvalue3 = 7;
    t.effectbasepoints3 = i32::MIN;
    s.load_learn_skills().unwrap();
    s.templates.get_mut(&1).unwrap().effectbasedice3 = u32::MAX;
    assert_eq!(
        s.load_learn_skills().unwrap_err().id,
        "execution.spells.simple_value_overflow_undefined"
    );
    let mut s = fixture();
    let t = s.templates.get_mut(&1).unwrap();
    t.effect3 = 3;
    t.effectbasepoints3 = i32::MAX - 1;
    t.effectbasedice3 = 1;
    load(&mut s, "spell_pet_auras", json!([1, 0, 2]));
    assert_eq!(s.pet_aura(1).unwrap().damage, i32::MAX);
    let t = s.templates.get_mut(&2).unwrap();
    t.effect3 = 3;
    t.effectbasepoints3 = i32::MAX;
    t.effectbasedice3 = 1;
    load(&mut s, "spell_pet_auras", json!([2, 0, 1]));
    assert!(
        matches!(&s.loads["spell_pet_auras"][1].disposition,SpellRowDisposition::Refused(e) if e.id=="execution.spells.simple_value_overflow_undefined")
    );
}

#[test]
fn unresolved_area_reverse_index_cannot_be_treated_as_absent() {
    let mut s = fixture();
    for id in [1, 2, 3] {
        s.templates.get_mut(&id).unwrap().effectapplyauraname3 = 4;
    }
    s.load_row(
        "spell_area",
        0,
        &[1, 1, 0, 0, 0, 1, 2, 0, 0, 1].map(|v| json!(v)),
        &mut Missing,
    );
    assert_eq!(
        s.areas_for_aura(2).unwrap_err().id,
        "execution.spells.condition_storage_owner"
    );
    assert!(s.areas_for_aura(999).unwrap().is_empty());
    assert!(s.areas_for_area(999).unwrap().is_empty());
    load(&mut s, "spell_area", json!([2, 0, 0, 0, 0, 0, 3, 0, 2, 1]));
    assert_eq!(
        s.areas(2).unwrap_err().id,
        "execution.spells.condition_storage_owner"
    );
}

#[test]
fn unresolved_area_forward_index_cannot_be_treated_as_absent() {
    let mut s = fixture();
    for id in [1, 2, 3] {
        s.templates.get_mut(&id).unwrap().effectapplyauraname3 = 4;
    }
    s.load_row(
        "spell_area",
        0,
        &[2, 1, 0, 0, 0, 1, 3, 0, 0, 1].map(|v| json!(v)),
        &mut Missing,
    );
    load(&mut s, "spell_area", json!([1, 0, 0, 0, 0, 0, 2, 0, 2, 1]));
    assert_eq!(
        s.areas(1).unwrap_err().id,
        "execution.spells.condition_storage_owner"
    );
    assert_eq!(
        s.areas_for_area(1).unwrap_err().id,
        "execution.spells.condition_storage_owner"
    );
}

#[test]
fn cached_talent_membership_never_infers_zero_from_empty_rank_slots() {
    let mut s = fixture();
    s.talent_spells.insert(1);
    assert!(s.is_talent(1));
    assert!(!s.is_talent(0));
    assert!(!s.is_talent(999));
    load(&mut s, "spell_learn_spell", json!([2, 1, 1]));
    skipped(&s, "spell_learn_spell", "talent");
}
