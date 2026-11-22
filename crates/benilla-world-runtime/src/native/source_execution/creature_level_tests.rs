use super::*;
use benilla_sim::source_rng::{DrawCursor, EventKind, RawGenerator, SourceRandom};
fn config() -> CreatureLevelConfig {
    let mut text = String::new();
    for (index, rank) in [
        "Normal",
        "Elite.Elite",
        "Elite.RAREELITE",
        "Elite.WORLDBOSS",
        "Elite.RARE",
    ]
    .iter()
    .enumerate()
    {
        text.push_str(&format!(
            "Rate.Creature.{rank}.HP={}\nRate.Creature.{rank}.Damage={}\n",
            index + 1,
            index + 6
        ));
    }
    CreatureLevelConfig::parse(text.as_bytes(), Sha256Id::of(text.as_bytes())).unwrap()
}
#[test]
fn rank_rates_and_unknown_rank_fallback_match_source() {
    // Creature.cpp:1492-1527. Pets take normal regardless of template rank.
    let c = config();
    for rank in 0..5 {
        assert_eq!(
            c.rates(rank, false).health.bits(),
            ((rank + 1) as f32).to_bits()
        );
        assert_eq!(
            c.rates(rank, false).damage.bits(),
            ((rank + 6) as f32).to_bits()
        );
        assert_eq!(c.rates(rank, true), c.rates(0, false));
    }
    assert_eq!(c.rates(5, false), c.rates(1, false));
    assert_eq!(c.rates(u32::MAX, false), c.rates(1, false));
    assert_eq!(
        CreatureLevelConfig::parse(b"", Sha256Id::of(b""))
            .unwrap_err()
            .id,
        "execution.creature.level_config_health_owner"
    );
}
struct Words(std::collections::VecDeque<u64>);
impl RawGenerator for Words {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        self.0.pop_front().unwrap()
    }
}
#[test]
fn level_draw_borrows_calling_thread_and_preserves_rejection_word_count() {
    // Creature.cpp:1278-1279; Util.cpp:68-96. No per-creature engine.
    assert_eq!(draw_source_creature_level([10, 10], None).unwrap(), 10);
    assert_eq!(
        draw_source_creature_level([10, 12], None).unwrap_err().id,
        "execution.creature.SelectLevel_urand_thread_owner"
    );
    let mut raw = Words([0, 1].into());
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    let mut sink = |event| events.push(event);
    let mut rng = SourceRandom::new(17, &mut raw, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        draw_source_creature_level([10, 12], Some(&mut rng)).unwrap(),
        10
    );
    assert_eq!(cursor.raw, 2);
    assert!(raw.0.is_empty());
    assert!(events.iter().all(|e| e.stream == 17));
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e.kind, EventKind::Raw { .. }))
            .count(),
        2
    );
}
fn cross_check_damage(
    objects: &SourceObjects,
    unit: ObjectHandle,
    class: u32,
    stats: &SourceLevelStats,
) -> Result<(bool, bool)> {
    use benilla_sim::npc_melee::{creature_damage_range, CreatureDamageInputs, CreatureStatClass};

    let mut live = SourceUnitFields::constructed_creature();
    live.apply_source_level(objects, unit, stats, class)?;
    let weapon = CreatureWeaponState {
        equipment: &live.loaded_creature.equipment,
        can_dual_wield: false,
        flags: 0,
        form: 0,
    };
    let damage = &mut live.creature_damage;
    let first_pass = damage.attack_power_words() == stats.attack_power;
    damage.update_attack_power_and_damage(
        objects,
        unit,
        class,
        live.resistances.agility(),
        weapon,
        false,
    )?;
    let actual = damage.damage_bits();
    let reference_class = match class {
        1 => Some(CreatureStatClass::Warrior),
        2 => Some(CreatureStatClass::Paladin),
        4 => Some(CreatureStatClass::Rogue),
        8 => Some(CreatureStatClass::Mage),
        _ => None,
    };
    let float = |bits| SpellFloat::from_bits(bits).unwrap();
    let [min, max] = stats.weapon_damage_bits[0].map(float);
    let reference = reference_class.and_then(|class| {
        creature_damage_range(CreatureDamageInputs {
            class,
            strength: stats.attributes[0].max(0) as u32,
            agility: stats.attributes[1].max(0) as u32,
            create_strength: float(stats.create_attributes_bits[0]),
            create_agility: float(stats.create_attributes_bits[1]),
            base_attack_power: float(stats.base_attack_power_bits[0]),
            damage_multiplier: float(stats.damage_multiplier_bits),
            base_weapon_minimum: min,
            base_weapon_maximum: max,
            weapon_unusable: false,
        })
        .ok()
    });
    let mainhand_agrees =
        reference.is_none_or(|r| actual[0] == [r.minimum.bits(), r.maximum.bits()]);
    // A source arithmetic check, not an independent measurement. With no
    // equipment or dual wield, GetBaseWeaponDamage returns zero for offhand.
    let melee = reference.map_or(damage.attack_power_words()[0], |r| r.attack_power);
    let offhand = ((melee as i32).max(0) as f32 / 14.0) * 0.5;
    let ranged_ap = (stats.attack_power[1] as i32).max(0) as f32 / 14.0;
    let multiplier = f32::from_bits(stats.damage_multiplier_bits);
    let ranged = stats.weapon_damage_bits[2].map(|bits| {
        (ranged_ap + f32::from_bits(bits) * multiplier)
            .max(0.0)
            .to_bits()
    });
    Ok((
        reference.is_some(),
        first_pass && mainhand_agrees && actual[1] == [offhand.to_bits(); 2] && actual[2] == ranged,
    ))
}

#[test]
fn catalog04_level_vitals_and_undefined_census() {
    let Some(root) = std::env::var_os("BENILLA_F5_AI_CATALOG") else {
        eprintln!("SKIP level/vitals census: BENILLA_F5_AI_CATALOG absent");
        return;
    };
    let path = std::env::var_os("BENILLA_F5_EVENT_AI_CONFIG").expect("recorded config");
    let config = CreatureLevelConfig::recorded(
        Path::new(&path),
        &Sha256Id::parse("321f2dc486e38f77037366bf84fb0ce8d09ed823ffa4524082ab01a8b0b79244")
            .unwrap(),
    )
    .unwrap();
    // seal16 mangosd.conf:1076-1080,1086-1090, hash checked above.
    assert_eq!(
        config.lines,
        [
            [1086, 1076],
            [1087, 1077],
            [1088, 1078],
            [1089, 1079],
            [1090, 1080]
        ]
    );
    for rank in 0..5 {
        assert_eq!(config.rates(rank, false).health.bits(), 1f32.to_bits());
    }
    let mut catalog =
        crate::native::production_compiler::VerifiedCatalog::open(Path::new(&root), false).unwrap();
    let definitions = CreatureLevelDefinitions::load(&mut catalog).unwrap();
    let mut complete = 0usize;
    let mut fixed = 0usize;
    let mut refused = BTreeMap::<String, Vec<(u32, u32)>>::new();
    let mut template_ok = BTreeMap::new();
    let (mut compared, mut agreed, mut armor_ub) = (0, 0, 0);
    let mut objects = SourceObjects::default();
    let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
    let (mut mainhand, mut other_ranges) = (0, 0);
    let mut damage_disagreements = Vec::new();
    for t in definitions.templates.values() {
        let mut ok = true;
        for level in t.levels[0]..=t.levels[1] {
            if let Err(e) = definitions.stats(t.entry, level, false, &config) {
                refused.entry(e.id).or_default().push((t.entry, level));
                ok = false;
            }
            let Ok(stats) = definitions.stats(t.entry, level, false, &config) else {
                continue;
            };
            match cross_check_damage(&objects, unit, t.unit_class, &stats) {
                Ok((bounded, true)) => {
                    mainhand += usize::from(bounded);
                    other_ranges += 1;
                }
                outcome => damage_disagreements.push((t.entry, level, outcome)),
            }
            let mut live = SourceUnitResistances::default();
            compared += 1;
            match live.select_level(
                &objects,
                unit,
                f32::from_bits(stats.armor_base_bits),
                stats.attributes[1],
            ) {
                Ok(()) => {
                    assert_eq!(
                        live.armor(),
                        stats.armor,
                        "entry {}, level {level}",
                        t.entry
                    );
                    agreed += 1;
                }
                Err(e) => {
                    assert_eq!(
                        e.id,
                        "execution.spells.StatSystem_669.armor_float_to_int_undefined"
                    );
                    armor_ub += 1;
                }
            }
        }
        if ok {
            complete += 1;
            if t.levels[0] == t.levels[1] {
                fixed += 1;
            }
        }
        template_ok.insert(t.entry, ok);
    }
    // initial-classicmangos.sql:421110-431494,448229-448438.
    assert_eq!((compared, agreed, armor_ub), (15425, 15425, 0));
    println!("CREATURE_ARMOR_STATIC compared={compared} agreed={agreed} float_to_int_undefined={armor_ub}");
    assert!(damage_disagreements.is_empty(), "{damage_disagreements:?}");
    assert_eq!(other_ranges, compared);
    assert!(mainhand > 0);
    println!("CREATURE_DAMAGE_STATIC bounded_mainhand={mainhand} offhand_and_ranged={other_ranges} live_refusals=0");
    let mut spawns = 0usize;
    let mut numeric_spawns = 0usize;
    let mut spawn_refused = BTreeMap::<String, usize>::new();
    if let Some(path) = std::env::var_os("BENILLA_F2_SPAWN_OCCURRENCES") {
        let mut rows = crate::native::production_compiler::ObjectMgrSpawnRows::open(
            &mut catalog,
            Path::new(&path),
        )
        .unwrap();
        while let Some(row) = rows.next("creature").unwrap() {
            spawns += 1;
            if template_ok.get(&row.entry) == Some(&true) {
                numeric_spawns += 1;
            } else {
                let reason = if row.entry == 0 {
                    "execution.creature.LoadFromDB_effective_entry_owner"
                } else if template_ok.contains_key(&row.entry) {
                    "execution.creature.SelectLevel_numeric_refusal"
                } else {
                    "execution.creature.SelectLevel_template_owner"
                };
                *spawn_refused.entry(reason.to_owned()).or_default() += 1;
            }
        }
    }
    let evidence = serde_json::json!({"templates":definitions.templates.len(),
        "numeric_templates_complete":complete,"fixed_level_numeric_templates":fixed,
        "level_draw_unbound_templates":complete-fixed,"numeric_refused_rows":refused,
        "spawn_rows":spawns,"numeric_spawns_complete":numeric_spawns,
        "spawn_arithmetic_refused":spawn_refused,
        "bound_construction_templates":0,"bound_construction_spawns":0,
        "basis":"all numeric level alternatives; not complete InitEntry execution; no RNG stream, model/equipment/terrain/event/group execution fabricated",
        "format":91});
    println!("CREATURE_LEVEL_CENSUS {evidence}");
    if let Some(path) = std::env::var_os("BENILLA_CREATURE_LEVEL_CENSUS") {
        std::fs::write(path, serde_json::to_vec_pretty(&evidence).unwrap()).unwrap();
    }
}

#[test]
fn class_loader_checks_level_before_the_undefined_class_flag_shift() {
    // ObjectMgr.cpp:749-761; FlagConvertibleEnum.h:56-58.
    let input = super::super::super::synthetic_creature_generation_fixture::source(40, 5, 5).input;
    let init::Rows::Complete(rows) = input.class_levels else {
        unreachable!()
    };
    let mut row = rows[0];
    row.class = 32;
    assert!(CreatureLevelDefinitions::from_rows(vec![], vec![row]).is_ok());
    for class in [0, 33] {
        row.class = class;
        assert_eq!(
            CreatureLevelDefinitions::from_rows(vec![], vec![row])
                .unwrap_err()
                .id,
            "execution.creature.LoadClassStats_class_shift_undefined"
        );
    }
    row.level = 0;
    assert!(CreatureLevelDefinitions::from_rows(vec![], vec![row]).is_ok());
}
