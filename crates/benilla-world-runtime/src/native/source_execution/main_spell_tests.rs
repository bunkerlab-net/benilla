// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored UnitAI.cpp:960-972 calls and diagnostic stock catalog operands.
use super::super::spells::power_tests::PowerFixture;
use super::*;

struct Timers;
impl CreatureRandom for Timers {
    fn creature_irand(&mut self, _: i32, _: i32) -> Result<i32> {
        panic!("unexpected draw")
    }
    fn creature_urand(&mut self, min: u32, max: u32) -> Result<u32> {
        assert_eq!((min, max), (0, 0));
        Ok(0)
    }
}
fn ai(f: &PowerFixture) -> SourceUnitAi {
    SourceUnitAi::construct(&f.objects, f.caster, 0, &mut Timers).unwrap()
}
fn stores(range: bool) -> SourceSpellStores {
    let mut s = SourceSpellStores::default();
    s.insert_template(SpellTemplate {
        id: 7,
        school: 2,
        manacost: 25,
        rangeindex: 3,
        ..Default::default()
    })
    .unwrap();
    s.dbc.insert(
        "SpellRange.dbc".into(),
        if range {
            BTreeMap::from([(
                3,
                SpellDbcRecord {
                    ordinal: 0,
                    words: BTreeMap::from([(1, 5f32.to_bits()), (2, 30f32.to_bits()), (3, 0)]),
                },
            )])
        } else {
            BTreeMap::new()
        },
    );
    s
}
#[test]
fn main_spell_first_call_initializes_and_second_only_inserts_without_lookup() {
    let mut f = PowerFixture::new();
    let mut ai = ai(&f);
    let mut ids = BTreeSet::new();
    ai.add_main_spell(7, &mut ids, &stores(true), &mut f)
        .unwrap();
    assert_eq!(
        (
            ai.main_spell_id,
            ai.main_spell_cost,
            ai.main_spell_info,
            ai.main_attack_mask
        ),
        (7, 25, Some(7), 4)
    );
    assert_eq!(ai.main_spell_min_range, 5.);
    assert_eq!(f.trace, ["level", "flat", "school=2", "multiplier"]);
    f.trace.clear();
    ai.add_main_spell(999, &mut ids, &SourceSpellStores::default(), &mut f)
        .unwrap();
    ai.add_main_spell(7, &mut ids, &SourceSpellStores::default(), &mut f)
        .unwrap();
    assert_eq!(ids, BTreeSet::from([7, 999]));
    assert!(f.trace.is_empty());
    assert_eq!(
        (
            ai.main_spell_id,
            ai.main_spell_cost,
            ai.main_spell_info,
            ai.main_attack_mask
        ),
        (7, 25, Some(7), 4)
    );
    assert_eq!(ai.main_spell_min_range, 5.);
}
#[test]
fn main_spell_null_lookup_writes_id_before_dereference_and_never_inserts() {
    let mut f = PowerFixture::new();
    let mut ai = ai(&f);
    let mut ids = BTreeSet::new();
    assert_eq!(
        ai.add_main_spell(7, &mut ids, &SourceSpellStores::default(), &mut f)
            .unwrap_err()
            .id,
        "execution.unit_ai.AddMainSpell_966.null_spell_undefined"
    );
    assert_eq!(
        (ai.main_spell_id, ai.main_spell_cost, ai.main_spell_info),
        (7, 0, None)
    );
    assert!(ids.is_empty());
    assert!(f.trace.is_empty());
    ai.add_main_spell(8, &mut ids, &SourceSpellStores::default(), &mut f)
        .unwrap();
    assert_eq!(ids, BTreeSet::from([8]));
}
#[test]
fn main_spell_range_null_is_zero_and_mask_addition_keeps_carries() {
    for (initial, expected) in [(0, 4), (2, 6), (4, 8)] {
        let mut f = PowerFixture::new();
        let mut ai = ai(&f);
        let mut ids = BTreeSet::new();
        ai.main_attack_mask = initial;
        ai.add_main_spell(7, &mut ids, &stores(false), &mut f)
            .unwrap();
        assert_eq!(ai.main_spell_min_range, 0.);
        assert_eq!(ai.main_attack_mask, expected);
    }
}
#[test]
fn main_spell_cost_precedes_range_and_range_precedes_final_assignments() {
    let mut f = PowerFixture::new();
    let mut ai = ai(&f);
    let mut ids = BTreeSet::new();
    let mut stores = stores(true);
    stores.dbc.clear();
    assert_eq!(
        ai.add_main_spell(7, &mut ids, &stores, &mut f)
            .unwrap_err()
            .id,
        "execution.spells.dbc_not_loaded"
    );
    assert_eq!(
        (
            ai.main_spell_id,
            ai.main_spell_cost,
            ai.main_spell_info,
            ai.main_attack_mask
        ),
        (7, 25, None, 0)
    );
    assert!(ids.is_empty());
}
#[test]
fn live_power_fields_and_main_spell_state_do_not_change_serialized_unit_bytes() {
    let f = PowerFixture::new();
    let mut fields = f.units.get(&f.objects, f.caster).unwrap().clone();
    let before = serde_json::to_vec(&fields).unwrap();
    fields.power_fields.as_mut().unwrap().create_mana = 999;
    fields.power_cost_modifier[2] = 19;
    fields.power_cost_multiplier[2] = 0.5f32.to_bits();
    assert_eq!(serde_json::to_vec(&fields).unwrap(), before);
    let mut ai = ai(&f);
    let before = serde_json::to_vec(&ai).unwrap();
    ai.main_spell_id = 7;
    ai.main_spell_info = Some(7);
    ai.main_spell_cost = 9;
    ai.main_spell_min_range = 5.;
    ai.main_attack_mask = 8;
    assert_eq!(serde_json::to_vec(&ai).unwrap(), before);
    assert_eq!(
        f.units.inventory(&f.objects).unwrap()["restore"],
        "execution.unit.f7_restore_owner"
    );
    assert_eq!(
        SourceCreatureAis::default().inventory(&f.objects).unwrap()["restore"],
        "execution.unit_ai.f7_restore_owner"
    );
    assert_eq!(
        SourceManagers::default()
            .require_durable_checkpoint()
            .unwrap_err()
            .id,
        "execution.managers.f7_continuation_owner"
    );
}
#[test]
fn unit_power_cost_skips_spell_mod_owner_and_preserves_formula_branches() {
    for (ty, attributesex, percentage, expected) in [
        (0, 2, 0, 17),
        (SPELL_POWER_HEALTH, 2, 0, 23),
        (9, 2, 0, 0),
        (0, 0, 20, 24),
        (SPELL_POWER_HEALTH, 0, 20, 46),
        (3, 0, 25, 22),
        (9, 0, 10, 0),
    ] {
        let mut f = PowerFixture::new();
        f.vitals().mana = 17;
        f.vitals().health = 23;
        f.vitals().max_energy = 90;
        f.initialized.stats.create_mana = 123;
        f.initialized.stats.create_health = 234;
        let t = SpellTemplate {
            id: 7,
            powertype: ty,
            attributesex,
            manacostpercentage: percentage,
            ..Default::default()
        };
        assert_eq!(unit_power_cost(&t, f.caster, &mut f).unwrap(), expected);
        assert!(!f.trace.iter().any(|s| s == "mod_owner" || s == "apply_mod"));
    }
    let mut f = PowerFixture::new();
    f.flat = -5;
    f.multiplier = 0.5f32.to_bits();
    let mut t = SpellTemplate {
        id: 7,
        manacost: 25,
        manacostperlevel: 10,
        baselevel: 0,
        ..Default::default()
    };
    assert_eq!(unit_power_cost(&t, f.caster, &mut f).unwrap(), 45);
    t.attributes = 0x80000;
    t.spelllevel = 5;
    assert_eq!(unit_power_cost(&t, f.caster, &mut f).unwrap(), 45);
    f.flat = -100;
    assert_eq!(unit_power_cost(&t, f.caster, &mut f).unwrap(), 0);
}
#[test]
fn unit_power_undefined_arithmetic_refuses_at_its_read_boundaries() {
    let mut f = PowerFixture::new();
    let mut t = SpellTemplate {
        id: 7,
        school: 32,
        ..Default::default()
    };
    assert_eq!(
        unit_power_cost(&t, f.caster, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_school_shift_undefined"
    );
    t.school = 31;
    assert_eq!(unit_power_cost(&t, f.caster, &mut f).unwrap(), 0);
    t.school = 0;
    t.manacost = i32::MAX as u32;
    f.flat = 1;
    assert_eq!(
        unit_power_cost(&t, f.caster, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_flat_add_undefined"
    );
    t.manacost = i32::MAX as u32 - 1;
    assert_eq!(
        unit_power_cost(&t, f.caster, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_multiplier_conversion_undefined"
    );
    t.manacost = 1;
    f.flat = 0;
    t.attributes = 0x80000;
    t.spelllevel = 0;
    f.level = 0;
    assert_eq!(
        unit_power_cost(&t, f.caster, &mut f).unwrap_err().id,
        "execution.spells.CalculatePowerCost_scaling_conversion_undefined"
    );
    f.level = 5;
    t.spelllevel = 5;
    assert_eq!(unit_power_cost(&t, f.caster, &mut f).unwrap(), 1);
}

#[test]
fn catalog_main_spells_static_census_and_entry4633_operands() {
    use crate::native::production_compiler::VerifiedCatalog;
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP main-spell static census: BENILLA_F5_AI_CATALOG is unset");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    let stores = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    let config = EventAiLoaderConfig::recorded(
        Path::new(&std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").unwrap()),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    let mut builder = AiRegistryBuilder::builtin();
    builder
        .load_catalog_scripts(
            &catalog,
            Path::new(&std::env::var_os("BENILLA_F5_SCRIPT_REPORT").unwrap()),
        )
        .unwrap();
    let registry = builder.freeze();
    let factions = CreatureAiFactions::load_catalog(&mut catalog).unwrap();
    let templates = catalog_creature_ai_census_inputs(&mut catalog).unwrap();
    let mut eligible = BTreeSet::new();
    for t in &templates {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let selected = registry
            .select(
                &objects,
                unit,
                t.clone().inputs(
                    CreatureAiControl::default(),
                    factions.neutral_to_all(t.faction),
                ),
            )
            .unwrap();
        if selected.executable().builtin_kind() == Some(BuiltinCreatureAi::Event) {
            eligible.insert(t.entry);
        }
    }
    let rows = catalog_event_ai_rows(&mut catalog).unwrap();
    let mut definitions = EventAiDefinitions::default();
    let mut unavailable = BTreeMap::<i32, usize>::new();
    let mut loader = EventAiCatalog::new(&mut catalog, config);
    for row in rows {
        let key = row.words[1] as i32;
        match load_event_ai_row(&mut loader, row) {
            Ok(EventAiLoad {
                disposition: EventAiDisposition::Loaded(event),
                ..
            }) => definitions.insert(event),
            Err(_) => {
                *unavailable.entry(key).or_default() += 1;
            }
            _ => {}
        }
    }
    let spawns = catalog
        .execution_event_ai_operands("creature", ["guid", "id"])
        .unwrap();
    let mut counts = [0usize; 2];
    let mut types = [BTreeMap::<u32, usize>::new(), BTreeMap::new()];
    let mut creature_types = [BTreeMap::<u32, usize>::new(), BTreeMap::new()];
    let mut branches = BTreeMap::<String, usize>::new();
    for (column, entry, guid) in templates
        .iter()
        .map(|t| (0, t.entry, 0))
        .chain(spawns.iter().map(|(_, r)| (1, r[1], r[0])))
    {
        if !eligible.contains(&entry) {
            continue;
        }
        let spells = definitions
            .for_spawn(entry, guid)
            .filter(|e| e.flags & 0x80 == 0)
            .flat_map(|e| e.actions.iter())
            .filter(|a| a.kind == 11)
            .filter_map(|a| a.params.filter(|p| p[2] & 0x100 != 0).map(|p| p[0]))
            .collect::<Vec<_>>();
        if spells.is_empty() {
            continue;
        }
        counts[column] += 1;
        for id in &spells {
            if let Some(t) = stores.template(*id) {
                *types[column].entry(t.powertype).or_default() += 1;
            } else {
                *branches.entry("null_spell_undefined".into()).or_default() += 1;
            }
        }
        if let Some(t) = stores.template(spells[0]) {
            *creature_types[column].entry(t.powertype).or_default() += 1;
            for (name, reached) in [
                ("school_shift_undefined", t.school >= 32),
                ("use_all_power", t.attributesex & 2 != 0),
                ("percentage", t.manacostpercentage != 0),
                ("level_scaling", t.attributes & 0x80000 != 0),
                ("mana_cost_per_level", t.manacostperlevel != 0),
                (
                    "unknown_percentage_power",
                    t.manacostpercentage != 0
                        && t.powertype >= 5
                        && t.powertype != SPELL_POWER_HEALTH,
                ),
            ] {
                if reached {
                    *branches.entry(name.into()).or_default() += 1;
                }
            }
        }
    }
    assert_eq!(branches.get("null_spell_undefined"), None);
    assert_eq!(branches.get("school_shift_undefined"), None);
    for e in definitions.for_spawn(4633, 27010) {
        for (slot, a) in e.actions.iter().enumerate() {
            if a.kind != 11 || a.params.is_none_or(|p| p[2] & 0x100 == 0) {
                continue;
            }
            let p = a.params.unwrap();
            let t = stores.template(p[0]).unwrap();
            assert_eq!((e.id, slot, p), (463303, 0, [6660, 1, 256]));
            assert_eq!(
                (
                    t.powertype,
                    t.manacost,
                    t.manacostpercentage,
                    t.rangeindex,
                    t.school
                ),
                (0, 0, 0, 54, 0)
            );
            assert_eq!(
                stores.range_entry(t.rangeindex).unwrap().unwrap().minimum.0,
                1084227584
            );
            println!("MAIN_SPELL_4633_STATIC event={} slot={} params={p:?} spell={} power={} mana={} percentage={} perlevel={} baselevel={} spelllevel={} maxlevel={} attributes={} attributesex={} range={} school={} min={:?}",e.id,slot,t.id,t.powertype,t.manacost,t.manacostpercentage,t.manacostperlevel,t.baselevel,t.spelllevel,t.maxlevel,t.attributes,t.attributesex,t.rangeindex,t.school,stores.range_entry(t.rangeindex).unwrap());
        }
    }
    println!("MAIN_SPELL_STATIC creatures={counts:?} creature_first_power_types={creature_types:?} action_power_types={types:?} first_spell_branches={branches:?} unavailable_event_keys={} scope=loaded_release_event_rows_for_EventAI_templates_and_independent_raw_spawn_rows_not_execution",unavailable.len());
}

#[test]
fn live_unit_power_owner_reads_constructor_modifiers_and_selected_resources() {
    let mut f = PowerFixture::new();
    let fields = f.units.get_mut(&f.objects, f.caster).unwrap();
    let power = fields.power_fields.as_mut().unwrap();
    power.create_health = 234;
    power.create_mana = 123;
    power.auxiliary_current = Some([17, 18, 19]);
    power.auxiliary_maximum = Some([80, 90, 100]);
    fields.power_cost_modifier[2] = 5;
    fields.power_cost_multiplier[2] = 0.5f32.to_bits();
    fields.vitals.as_mut().unwrap().max_energy = 90;
    let mut owner = super::super::main_spell::UnitPowerOwner {
        objects: &f.objects,
        units: super::super::main_spell::UnitPowerSource::Units(&f.units),
    };
    let mut t = SpellTemplate {
        id: 7,
        school: 2,
        manacost: 15,
        ..Default::default()
    };
    assert_eq!(unit_power_cost(&t, f.caster, &mut owner).unwrap(), 30);
    t.school = 0;
    t.manacost = 0;
    t.manacostpercentage = 20;
    for (ty, expected) in [
        (0, 24),
        (SPELL_POWER_HEALTH, 46),
        (1, 16),
        (2, 18),
        (3, 18),
        (4, 20),
    ] {
        t.powertype = ty;
        assert_eq!(unit_power_cost(&t, f.caster, &mut owner).unwrap(), expected);
    }
    t.attributesex = 2;
    for (ty, expected) in [(1, 17), (2, 18), (4, 19)] {
        t.powertype = ty;
        assert_eq!(unit_power_cost(&t, f.caster, &mut owner).unwrap(), expected);
    }
}

#[test]
fn live_power_dispatch_agrees_for_full_and_borrowed_units() {
    use super::super::main_spell::{UnitPowerOperands, UnitPowerOwner, UnitPowerSource};

    let mut f = PowerFixture::new();
    let missing_current = || Err(refuse("execution.spells.Unit_GetPower_type_owner"));
    let missing_maximum = || Err(refuse("execution.spells.Unit_GetMaxPower_type_owner"));
    for (ty, has_auxiliary, current, maximum) in [
        (0, true, Ok(11), Ok(101)),
        (1, true, Ok(12), Ok(102)),
        (2, true, Ok(13), Ok(103)),
        (3, true, Ok(14), Ok(104)),
        (4, true, Ok(15), Ok(105)),
        (5, true, missing_current(), missing_maximum()),
        (1, false, missing_current(), missing_maximum()),
        (2, false, missing_current(), missing_maximum()),
        (4, false, missing_current(), missing_maximum()),
    ] {
        let live = f.units.get_mut(&f.objects, f.caster).unwrap();
        let vitals = live.vitals.as_mut().unwrap();
        vitals.mana = 11;
        vitals.max_mana = 101;
        vitals.energy = 14;
        vitals.max_energy = 104;
        let fields = live.power_fields.as_mut().unwrap();
        fields.auxiliary_current = has_auxiliary.then_some([12, 13, 15]);
        fields.auxiliary_maximum = has_auxiliary.then_some([102, 103, 105]);

        let context = SpellPowerContext {
            objects: &f.objects,
            units: &f.units,
        };
        let live = f.units.get(&f.objects, f.caster).unwrap();
        let borrowed = UnitPowerOperands {
            vitals: &live.vitals,
            fields: &live.power_fields,
            flat: &live.power_cost_modifier,
            multiplier: &live.power_cost_multiplier,
        };
        let actual_current = context.current(f.caster, ty);
        let actual_maximum = context.maximum(f.caster, ty);
        assert_eq!(actual_current, current, "current power type {ty}");
        assert_eq!(actual_maximum, maximum, "maximum power type {ty}");
        for units in [
            UnitPowerSource::Units(&f.units),
            UnitPowerSource::Caster(f.caster, borrowed),
        ] {
            let mut owner = UnitPowerOwner {
                objects: &f.objects,
                units,
            };
            assert_eq!(owner.power_current(f.caster, ty), actual_current);
            assert_eq!(owner.power_maximum(f.caster, ty), actual_maximum);
        }
    }
}

#[test]
fn main_spell_final_school_read_follows_drain_cost_and_range() {
    for school in [31, 32] {
        let mut f = PowerFixture::new();
        let mut ai = ai(&f);
        let mut ids = BTreeSet::new();
        let mut stores = stores(true);
        stores.templates.get_mut(&7).unwrap().attributesex = 2;
        stores.templates.get_mut(&7).unwrap().school = school;
        let result = ai.add_main_spell(7, &mut ids, &stores, &mut f);
        assert_eq!(
            ai.main_spell_cost,
            f.units
                .get(&f.objects, f.caster)
                .unwrap()
                .vitals
                .unwrap()
                .mana
        );
        assert_eq!(ai.main_spell_min_range, 5.);
        if school == 32 {
            assert_eq!(
                result.unwrap_err().id,
                "execution.unit_ai.AddMainSpell_968.school_shift_undefined"
            );
            assert_eq!(ai.main_spell_info, None);
            assert!(ids.is_empty());
        } else {
            result.unwrap();
            assert_eq!(ai.main_attack_mask, 0x80000000);
            assert_eq!(ai.main_spell_info, Some(7));
        }
    }
}

#[test]
fn target_filter_uses_the_source_table_and_refuses_the_selected_relation() {
    let f = PowerFixture::new();
    let factions = CreatureAiFactions::default();
    let relations = SourceRelations {
        objects: &f.objects,
        units: &f.units,
        factions: &factions,
    };
    for (ty, id) in [
        (21, ""),
        (6, "execution.relations.Unit_CanAttackSpell_1065_owner"),
        (35, "execution.relations.CheckCast_5088_IsInGroup_owner"),
        (57, "execution.relations.CheckCast_5088_IsInGroup_owner"),
    ] {
        let t = SpellTemplate {
            id: 7,
            effectimplicittargeta1: ty,
            ..Default::default()
        };
        let mut s = f.spell(&t);
        s.targets.unit = Some(f.caster);
        let result = s.check_target_filter(&t, 0, &f.objects, &relations);
        if id.is_empty() {
            assert_eq!(result.unwrap(), SpellCastResult::Ok);
        } else {
            assert_eq!(result.unwrap_err().id, id);
        }
        s.targets.unit = None;
        assert_eq!(
            s.check_target_filter(&t, 0, &f.objects, &relations)
                .unwrap(),
            SpellCastResult::BadTargets
        );
    }
    let row = SPELL_TARGET_INFO_TABLE[21];
    assert_eq!(
        (row.target_type, row.filter, row.enumerator),
        (
            SpellTargetImplicitType::Unit,
            SpellTargetFilter::Helpful,
            SpellTargetEnumerator::Single
        )
    );
}

#[test]
fn catalog_relations_target_filters_are_static_cast_alternatives() {
    use crate::native::production_compiler::VerifiedCatalog;
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        println!("SKIP static Relations census: BENILLA_F5_AI_CATALOG is unset");
        return;
    };
    let mut catalog = VerifiedCatalog::open(Path::new(&root), false).unwrap();
    struct Missing;
    impl SpellLoaderOwner for Missing {}
    let stores = SourceSpellStores::load(&mut catalog, &mut Missing).unwrap();
    fn filters(stores: &SourceSpellStores, id: u32) -> BTreeSet<SpellTargetFilter> {
        stores
            .template(id)
            .into_iter()
            .flat_map(|t| t.effects())
            .filter_map(|e| {
                if matches!(e.target_a, 1..=5 | 27) {
                    return None;
                }
                let data = SPELL_TARGET_INFO_TABLE.get(e.target_a as usize)?;
                (data.target_type == SpellTargetImplicitType::Unit
                    && matches!(
                        data.enumerator,
                        SpellTargetEnumerator::Single | SpellTargetEnumerator::Chain
                    )
                    && matches!(
                        data.filter,
                        SpellTargetFilter::Helpful
                            | SpellTargetFilter::Harmful
                            | SpellTargetFilter::Party
                            | SpellTargetFilter::Group
                    ))
                .then_some(data.filter)
            })
            .collect()
    }
    for table in [CreatureAddonTable::Template, CreatureAddonTable::Guid] {
        let mut counts = BTreeMap::<SpellTargetFilter, usize>::new();
        let mut casts = 0;
        let mut missing = 0;
        for row in catalog.execution_creature_addon_rows(table).unwrap() {
            for raw in std::str::from_utf8(&row.strings[0])
                .unwrap()
                .split_whitespace()
            {
                let id = raw.parse::<u32>().unwrap();
                if id == 0 {
                    continue;
                }
                casts += 1;
                missing += usize::from(stores.template(id).is_none());
                for filter in filters(&stores, id) {
                    *counts.entry(filter).or_default() += 1;
                }
            }
        }
        println!("RELATIONS_STATIC_ADDON table={table:?} raw_cast_occurrences={casts} filters={counts:?} missing_spells={missing} scope=raw_catalog_alternatives_not_execution");
        // Creature.cpp:2343 always passes this as the addon aura's explicit target.
        assert_eq!(
            counts.get(&SpellTargetFilter::Helpful),
            Some(&match table {
                CreatureAddonTable::Template => 33,
                CreatureAddonTable::Guid => 72,
            })
        );
        println!("RELATIONS_STATIC_ADDON_IDENTITY table={table:?} helpful_self={} helpful_nonself=0 null_unit_target_undefined=0", counts[&SpellTargetFilter::Helpful]);
    }
    let mut counts = BTreeMap::<SpellTargetFilter, usize>::new();
    let mut casts = 0;
    let mut missing = 0;
    let mut helpful_identity = [0usize; 2];
    for row in catalog_event_ai_rows(&mut catalog).unwrap() {
        for i in [12, 16, 20] {
            if row.words[i] != 11 {
                continue;
            }
            let id = row.words[i + 1];
            let filters = filters(&stores, id);
            casts += 1;
            missing += usize::from(stores.template(id).is_none());
            if filters.contains(&SpellTargetFilter::Helpful) {
                // CreatureEventAI.cpp:1720-1721; UnitAI.cpp:169-170 makes force-self identity.
                let identity = row.words[i + 2] == 0 || row.words[i + 3] & 0x10 != 0;
                helpful_identity[usize::from(!identity)] += 1;
            }
            for filter in filters {
                *counts.entry(filter).or_default() += 1;
            }
        }
    }
    println!("RELATIONS_STATIC_EVENTAI raw_cast_action_slots={casts} filters={counts:?} missing_spells={missing} scope=raw_catalog_alternatives_not_execution");
    assert_eq!(helpful_identity, [218, 203]);
    println!("RELATIONS_STATIC_EVENTAI_IDENTITY helpful_guaranteed_self={} helpful_dynamic_identity_or_faction={} null_unit_target_undefined=0 scope=raw_static_alternatives_not_execution", helpful_identity[0], helpful_identity[1]);
}
