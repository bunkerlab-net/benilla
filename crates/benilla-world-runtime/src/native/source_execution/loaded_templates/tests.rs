// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
#[derive(Default)]
struct Owner {
    calls: Vec<(TemplateLookup, u32)>,
    missing: Option<TemplateLookup>,
    stats: bool,
}
impl TemplateValidationOwner for Owner {
    fn contains(&mut self, domain: TemplateLookup, id: u32) -> Result<bool> {
        self.calls.push((domain, id));
        if self.missing == Some(domain) {
            return Err(refuse("authored_missing_owner"));
        }
        Ok(id == 1)
    }
    fn display_scale(&mut self, id: u32) -> Result<Option<u32>> {
        Ok(match id {
            1 => Some(2.0f32.to_bits()),
            3 => Some(3.0f32.to_bits()),
            _ => None,
        })
    }
    fn class_stats(&mut self, _: u32, _: u32) -> Result<bool> {
        Ok(self.stats)
    }
}
fn creature(entry: u32) -> LoadedCreatureInfo {
    let mut row = LoadedCreatureInfo {
        ordinal: u64::from(entry),
        words: [0; 99],
        strings: std::array::from_fn(|_| Vec::new()),
    };
    for (field, value) in [
        (0, entry),
        (3, 1),
        (4, 1),
        (5, 1),
        (13, 1),
        (14, 1.0f32.to_bits()),
        (17, 1),
        (36, 1),
        (60, 2000),
        (61, 2000),
    ] {
        row.words[field] = value;
    }
    row
}
fn go(entry: u32, kind: u32) -> LoadedGameObjectInfo {
    let mut row = LoadedGameObjectInfo {
        ordinal: u64::from(entry),
        words: [0; 38],
        strings: std::array::from_fn(|_| Vec::new()),
    };
    row.words[0] = entry;
    row.words[1] = kind;
    row.words[8] = 1.0f32.to_bits();
    row
}
fn creatures(rows: Vec<LoadedCreatureInfo>) -> LoadedCreatureTemplates {
    LoadedCreatureTemplates::load(
        rows,
        &mut Owner {
            stats: true,
            ..Owner::default()
        },
    )
    .unwrap()
}
#[test]
fn creature_validation_fixups_preserve_both_sides() {
    for (field, good, bad, fixed, line) in [
        (3, 1, 0, 1, 456),
        (4, 1, 0, 1, 462),
        (3, 70, 71, 70, 468),
        (4, 70, 71, 70, 474),
        (36, 8, 3, 1, 480),
        (62, 6, 7, 0, 495),
        (60, 1, 0, 2000, 499),
        (61, 1, 0, 2000, 502),
        (16, 1, 2, 7, 510),
        (15, 1, 2, 0, 517),
        (17, 7, 8, 7, 523),
        (79, 4, 5, 0, 536),
        (93, 1, 2, 0, 544),
        (91, 1, 2, 0, 566),
        (92, 1, 2, 0, 572),
        (68, 1, 2, 0, 423),
        (69, 1, 2, 0, 423),
    ] {
        for (input, expected, fix) in [(good, good, false), (bad, fixed, true)] {
            let mut row = creature(1);
            row.words[field] = input;
            if field == 3 && input > 1 {
                row.words[4] = input;
            }
            let loaded = creatures(vec![row]);
            assert_eq!(
                loaded.get(1).unwrap().words[field],
                expected,
                "field {field}"
            );
            assert_eq!(
                loaded
                    .notices()
                    .iter()
                    .any(|n| n.line == line && n.field == field),
                fix,
                "field {field} line {line}"
            );
        }
    }
    let mut row = creature(1);
    row.words[15] = 10;
    row.words[17] = 0;
    row.words[36] = 0;
    let loaded = creatures(vec![row]);
    assert_eq!(loaded.get(1).unwrap().words[15], 10);
    assert_eq!(loaded.get(1).unwrap().words[17], 7);
    assert_eq!(loaded.get(1).unwrap().words[36], 1);
}
#[test]
fn creature_display_scale_and_model_diagnostics_follow_fixups() {
    for field in 5..=8 {
        let mut row = creature(1);
        row.words[5] = 0;
        row.words[field] = 2;
        row.words[14] = 0;
        let mut owner = Owner {
            stats: true,
            ..Owner::default()
        };
        let loaded = LoadedCreatureTemplates::load(vec![row], &mut owner).unwrap();
        assert_eq!(loaded.get(1).unwrap().words[field], 0);
        assert_eq!(loaded.get(1).unwrap().words[14], 1.0f32.to_bits());
        assert!(owner.calls.contains(&(TemplateLookup::Model, 0)));
        assert!(loaded
            .notices()
            .iter()
            .any(|n| n.line == 439 && n.field == field));
    }
    let mut row = creature(1);
    row.words[5] = 2;
    row.words[6] = 1;
    row.words[14] = (-1.0f32).to_bits();
    assert_eq!(
        creatures(vec![row]).get(1).unwrap().words[14],
        2.0f32.to_bits()
    );
    let mut row = creature(1);
    row.words[14] = f32::NAN.to_bits();
    assert!(f32::from_bits(creatures(vec![row]).get(1).unwrap().words[14]).is_nan());
    let mut row = creature(1);
    row.words[5] = 3;
    row.words[6] = 1;
    row.words[14] = 0;
    assert_eq!(
        creatures(vec![row]).get(1).unwrap().words[14],
        3.0f32.to_bits()
    );
}
#[test]
fn creature_log_only_checks_never_modify_rows() {
    for (field, value, line) in [(13, 2, 414), (78, 2, 530), (85, 2, 551), (80, 4, 505)] {
        let mut row = creature(1);
        row.words[field] = value;
        if field == 80 {
            row.words[20] = 0x10;
        }
        let loaded = creatures(vec![row.clone()]);
        assert_eq!(loaded.get(1), Some(&row));
        assert!(loaded
            .notices()
            .iter()
            .any(|n| n.line == line && n.kind == TemplateNoticeKind::Diagnostic));
    }
    let loaded = LoadedCreatureTemplates::load(vec![creature(1)], &mut Owner::default()).unwrap();
    assert!(loaded.notices().iter().any(|n| n.line == 487));
}
#[test]
fn loaded_class_stats_requires_health_or_stamina_and_damage_above_threshold() {
    use benilla_sim::{classic_spell::SpellFloat, creature_init::ClassLevelRow};
    for (health, stamina, damage, expected) in [
        (0, 0, 0x3dcc_ccce, false),
        (1, 0, 0x3dcc_cccd, false),
        (1, 0, 0x3dcc_ccce, true),
        (0, 1, 0x3dcc_ccce, true),
    ] {
        let zero = SpellFloat::ZERO;
        let rows = CreatureLevelDefinitions::from_rows(
            Vec::new(),
            vec![ClassLevelRow {
                class: 1,
                level: 1,
                base_health: health,
                base_mana: 0,
                base_armor: 0,
                base_melee_attack_power: zero,
                base_ranged_attack_power: zero,
                base_damage: SpellFloat::from_bits(damage).unwrap(),
                attributes: [0, 0, stamina, 0, 0],
            }],
        )
        .unwrap();
        assert_eq!(rows.has_loader_stats(1, 1), expected);
        assert!(!rows.has_loader_stats(2, 1));
    }
}
#[test]
fn creature_array_visits_ids_not_raw_order_and_ignores_entry_zero() {
    let mut zero = creature(0);
    zero.words[36] = u32::MAX;
    let loaded = creatures(vec![creature(7), zero, creature(1)]);
    assert_eq!(loaded.get(7).unwrap().ordinal, 7);
    assert_eq!(loaded.get(0).unwrap().words[36], u32::MAX);
    assert!(loaded.get(2).is_none());
    let mut first = creature(1);
    first.words[62] = 7;
    assert_eq!(
        creatures(vec![first, creature(1)]).get(1).unwrap().words[62],
        0
    );
    let mut high = creature(7);
    high.words[13] = 7;
    let mut low = creature(1);
    low.words[13] = 2;
    let loaded = creatures(vec![high, low]);
    assert_eq!(
        loaded
            .notices()
            .iter()
            .filter(|n| n.line == 414)
            .map(|n| n.entry)
            .collect::<Vec<_>>(),
        [1, 7]
    );
}
#[test]
fn class_shift_domain_is_censused_before_evaluation() {
    for class in [0, 1, 2, 8, 32, 33, u32::MAX] {
        let mut row = creature(1);
        row.words[36] = class;
        assert_eq!(
            LoadedCreatureTemplates::class_shift_ub_reach(&[row.clone()]),
            usize::from(class > 32)
        );
        let result = LoadedCreatureTemplates::load(vec![row], &mut Owner::default());
        if class > 32 {
            assert_eq!(
                result.unwrap_err().id,
                "execution.templates.ObjectMgr_477.UnitClass_shift_undefined"
            );
        } else {
            assert_eq!(
                result.unwrap().get(1).unwrap().words[36],
                if matches!(class, 1 | 2 | 8) { class } else { 1 }
            );
        }
    }
}
#[test]
fn static_flag_mapping_preserves_existing_bits_and_uint32_domain() {
    assert_eq!(type_flags(0x200, [0; 4]), 0x200);
    for (word, input, expected) in [
        (0, 0x10, 1),
        (0, 0x10000, 4),
        (0, 0x200000, 2),
        (0, 0x1000000, 0x10),
        (0, 0x800000, 8),
        (0, 0x40000000, 0x20),
        (1, 8, 0x40),
        (1, 0x800, 0x80),
        (1, 0x4000, 0x100),
        (1, 0x8000, 0x200),
        (1, 0x40000, 0x800),
        (1, 0x100000, 0x400),
        (1, 0x20000000, 0x1000),
        (1, 0x80000000, 0x2000),
        (2, 8, 0x4000),
        (2, 0x10, 0x8000),
        (2, 0x200, 0x10000),
        (2, 0x2000, 0x100000),
        (2, 0x10000, 0x20000),
        (2, 0x40000, 0x40000),
        (2, 0x400000, 0x80000),
        (2, 0x1000000, 0x200000),
        (2, 0x20000000, 0x400000),
        (3, 8, 0x800000),
        (3, 0x10, 0x1000000),
        (3, 0x20, 0x2000000),
        (3, 0x8000, 0x4000000),
        (3, 0x80000, 0x8000000),
        (3, 0x200000, 0x10000000),
        (3, 0x20000000, 0x20000000),
        (3, 0x40000000, 0x40000000),
        (3, 0x80000000, 0x80000000),
    ] {
        let mut flags = [0; 4];
        flags[word] = input;
        assert_eq!(type_flags(0, flags), expected);
        flags[word] = 0;
        assert_eq!(type_flags(0, flags), 0);
    }
    assert_eq!(type_flags(0, [u32::MAX; 4]), u32::MAX);
    let mut row = creature(1);
    row.words[24] = 0x200;
    row.words[25] = 0x10;
    assert_eq!(creatures(vec![row]).get(1).unwrap().words[24], 0x201);
}
#[test]
fn gameobject_fixups_have_both_sides_and_keep_nan() {
    for (kind, field, good, bad, fixed, line) in [
        (7, 10, 2, 3, 0, 6976),
        (29, 25, 1, 0, 1, 7008),
        (0, 36, 1, 2, 0, 7033),
        (0, 8, 1.0f32.to_bits(), 0, 1.0f32.to_bits(), 7027),
    ] {
        for (input, expected, fix) in [(good, good, false), (bad, fixed, true)] {
            let mut row = go(1, kind);
            row.words[field] = input;
            let loaded = LoadedGameObjectTemplates::load(vec![row], &mut Owner::default()).unwrap();
            assert_eq!(loaded.get(1).unwrap().words[field], expected);
            assert_eq!(loaded.notices().iter().any(|n| n.line == line), fix);
        }
    }
    let mut row = go(1, 0);
    row.words[8] = f32::NAN.to_bits();
    assert_eq!(
        LoadedGameObjectTemplates::load(vec![row.clone()], &mut Owner::default())
            .unwrap()
            .get(1),
        Some(&row)
    );
}
#[test]
fn gameobject_hash_rows_and_transport_display_vector_keep_raw_order() {
    let mut rows = vec![go(9, 11), go(1, 15), go(8, 15), go(3, 15), go(4, 11)];
    for (row, display) in rows.iter_mut().zip([562, 88, 462, 562, 462]) {
        row.words[2] = display;
    }
    let loaded = LoadedGameObjectTemplates::load(rows.clone(), &mut Owner::default()).unwrap();
    assert_eq!(loaded.rows(), rows);
    assert_eq!(loaded.transport_displays(), [562, 88, 462]);
    assert_eq!(loaded.get(1), Some(&rows[1]));
}
#[test]
fn gameobject_diagnostics_and_disabled_spell_checks_preserve_fields() {
    for (kind, data, domain, line) in [
        (0, 1, TemplateLookup::Lock, 6940),
        (8, 0, TemplateLookup::SpellFocus, 7092),
        (10, 7, TemplateLookup::PageText, 7110),
        (15, 0, TemplateLookup::TaxiPath, 7142),
        (22, 0, TemplateLookup::Spell, 6963),
    ] {
        for id in [1, 2] {
            let mut row = go(1, kind);
            row.words[9 + data] = id;
            let mut owner = Owner::default();
            let loaded = LoadedGameObjectTemplates::load(vec![row.clone()], &mut owner).unwrap();
            assert_eq!(loaded.get(1), Some(&row));
            assert!(owner.calls.contains(&(domain, id)));
            assert_eq!(loaded.notices().iter().any(|n| n.line == line), id == 2);
        }
    }
    for (kind, data, line) in [
        (0, 3, 6985),
        (1, 4, 6985),
        (2, 5, 6985),
        (10, 11, 6985),
        (24, 5, 6985),
        (26, 3, 6985),
        (3, 3, 6995),
        (10, 3, 6995),
    ] {
        for value in [0, 1, 2] {
            let mut row = go(1, kind);
            row.words[9 + data] = value;
            let loaded =
                LoadedGameObjectTemplates::load(vec![row.clone()], &mut Owner::default()).unwrap();
            assert_eq!(loaded.get(1), Some(&row));
            assert_eq!(loaded.notices().iter().any(|n| n.line == line), value > 1);
        }
    }
    for (kind, data) in [(6, 3), (10, 10), (18, 1)] {
        let mut row = go(1, kind);
        row.words[9 + data] = 999;
        let mut owner = Owner {
            missing: Some(TemplateLookup::Spell),
            ..Owner::default()
        };
        LoadedGameObjectTemplates::load(vec![row], &mut owner).unwrap();
        assert!(!owner
            .calls
            .iter()
            .any(|(domain, _)| *domain == TemplateLookup::Spell));
    }
    let loaded = LoadedGameObjectTemplates::load(vec![go(1, 22)], &mut Owner::default()).unwrap();
    assert!(loaded.notices().iter().any(|n| n.line == 6963));
}
#[test]
fn gameobject_all_lock_offsets_and_unknown_types_keep_source_values() {
    for (kind, field) in [
        (0, 1),
        (1, 1),
        (2, 0),
        (3, 0),
        (6, 0),
        (10, 0),
        (12, 0),
        (13, 0),
        (24, 0),
        (25, 4),
        (26, 0),
    ] {
        let mut row = go(1, kind);
        row.words[9 + field] = 2;
        let loaded =
            LoadedGameObjectTemplates::load(vec![row.clone()], &mut Owner::default()).unwrap();
        assert_eq!(loaded.get(1), Some(&row));
        assert_eq!(loaded.notices()[0].line, 6940);
    }
    let row = go(1, u32::MAX);
    assert_eq!(
        LoadedGameObjectTemplates::load(vec![row.clone()], &mut Owner::default())
            .unwrap()
            .get(1),
        Some(&row)
    );
    let mut row = go(1, 15);
    row.words[2] = 462;
    row.words[9] = 2;
    let loaded = LoadedGameObjectTemplates::load(vec![row], &mut Owner::default()).unwrap();
    assert!(loaded.transport_displays().is_empty());
    assert_eq!(loaded.notices()[0].line, 7142);
}
#[test]
fn gameobject_linked_trap_checks_use_loaded_hash_and_only_log() {
    for (kind, data) in [(1, 3), (3, 7), (8, 2), (10, 12)] {
        for (target_kind, expected_line) in [(6, None), (5, Some(6949))] {
            let mut row = go(1, kind);
            row.words[9 + data] = 2;
            let loaded = LoadedGameObjectTemplates::load(
                vec![row.clone(), go(2, target_kind)],
                &mut Owner::default(),
            )
            .unwrap();
            assert_eq!(loaded.get(1), Some(&row));
            assert_eq!(loaded.notices().first().map(|n| n.line), expected_line);
        }
        let mut row = go(1, kind);
        row.words[9 + data] = 99;
        assert_eq!(
            LoadedGameObjectTemplates::load(vec![row], &mut Owner::default())
                .unwrap()
                .notices()[0]
                .line,
            6954
        );
    }
}
#[test]
fn missing_dependency_refuses_at_first_source_lookup() {
    let mut owner = Owner {
        missing: Some(TemplateLookup::Faction),
        ..Owner::default()
    };
    assert_eq!(
        LoadedCreatureTemplates::load(vec![creature(1)], &mut owner)
            .unwrap_err()
            .id,
        "execution.templates.ObjectMgr_412.Faction_owner.authored_missing_owner"
    );
    assert_eq!(owner.calls, [(TemplateLookup::Faction, 1)]);
    let mut row = go(1, 0);
    row.words[10] = 1;
    let mut owner = Owner {
        missing: Some(TemplateLookup::Lock),
        ..Owner::default()
    };
    assert_eq!(
        LoadedGameObjectTemplates::load(vec![row], &mut owner)
            .unwrap_err()
            .id,
        "execution.templates.ObjectMgr_6937.Lock_owner.authored_missing_owner"
    );
}
#[test]
fn creature_array_maximum_key_refuses_source_index_wrap() {
    assert_eq!(
        LoadedCreatureTemplates::load(vec![creature(u32::MAX)], &mut Owner::default())
            .unwrap_err()
            .id,
        "execution.templates.SQLStorageImpl_317.max_entry_index_undefined"
    );
    assert_eq!(
        LoadedCreatureTemplates::load(vec![creature(u32::MAX - 1)], &mut Owner::default())
            .unwrap_err()
            .id,
        "execution.templates.creature_index_budget"
    );
    assert!(creatures(vec![creature(100)]).get(100).is_some());
}
#[test]
fn stock_template_loader_census() {
    let Some(path) = std::env::var_os("BENILLA_F2_CATALOG") else {
        eprintln!("SKIP stock_template_loader_census: BENILLA_F2_CATALOG absent");
        return;
    };
    let mut catalog =
        crate::native::production_compiler::VerifiedCatalog::open(Path::new(&path), false).unwrap();
    let creature_rows = catalog.execution_loaded_creature_rows().unwrap();
    let ub = LoadedCreatureTemplates::class_shift_ub_reach(&creature_rows);
    assert_eq!(ub, 0);
    let max_entry_ub = usize::from(creature_rows.iter().any(|row| row.words[0] == u32::MAX));
    let static_conversion_reach = creature_rows
        .iter()
        .filter(|row| row.words[0] != 0 && row.words[25..29].iter().any(|v| *v != 0))
        .count();
    assert_eq!(max_entry_ub, 0);
    let gos = LoadedGameObjectTemplates::catalog_diagnostic(&mut catalog).unwrap();
    let creatures =
        LoadedCreatureTemplates::load(creature_rows, &mut TemplateCatalog::new(&mut catalog))
            .unwrap();
    for (table, rows, notices) in [
        (
            "creature_template",
            creatures.rows().len(),
            creatures.notices(),
        ),
        ("gameobject_template", gos.rows().len(), gos.notices()),
    ] {
        let mut fixed = BTreeMap::<u32, usize>::new();
        let mut diagnostics = BTreeMap::<u32, usize>::new();
        let mut fixed_rows = BTreeMap::<u32, std::collections::BTreeSet<u32>>::new();
        for n in notices {
            if n.kind == TemplateNoticeKind::Fixup {
                *fixed.entry(n.line).or_default() += 1;
                fixed_rows.entry(n.line).or_default().insert(n.entry);
            } else {
                *diagnostics.entry(n.line).or_default() += 1;
            }
        }
        let fixed_rows: BTreeMap<_, _> = fixed_rows
            .into_iter()
            .map(|(line, entries)| (line, entries.len()))
            .collect();
        let static_conversion_reach = if table == "creature_template" {
            static_conversion_reach
        } else {
            0
        };
        println!("TEMPLATE_CENSUS table={table} loaded={rows} fixed_rows={fixed_rows:?} fixup_occurrences={fixed:?} diagnostics={diagnostics:?} refused=0 UnitClass_shift_ub_reach={ub} array_max_entry_ub_reach={max_entry_ub} static_conversion_reach={static_conversion_reach} fixed_uint32_enum_ub_reach=0");
    }
    println!(
        "TEMPLATE_TRANSPORT_DISPLAYS catalog_export_order={:?}",
        gos.transport_displays()
    );
    assert!(creatures.get(4692).is_some());
    assert!(gos.get(3211).is_some());
}
