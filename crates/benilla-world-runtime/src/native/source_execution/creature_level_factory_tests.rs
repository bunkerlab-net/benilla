use super::*;
use benilla_sim::creature_init as init;
fn rates() -> CreatureLevelConfig {
    let mut text = String::new();
    for rank in [
        "Normal",
        "Elite.Elite",
        "Elite.RAREELITE",
        "Elite.WORLDBOSS",
        "Elite.RARE",
    ] {
        text.push_str(&format!(
            "Rate.Creature.{rank}.HP=1\nRate.Creature.{rank}.Damage=1\n"
        ));
    }
    CreatureLevelConfig::parse(text.as_bytes(), Sha256Id::of(text.as_bytes())).unwrap()
}
#[test]
fn grid_load_runs_non_normal_flying_level_vitals_on_factory_stream() {
    // Creature.cpp:898-903,1602-1610,523,1278-1279,1425-1489,526-536.
    let input = super::super::super::synthetic_creature_generation_fixture::source(40, 5, 6).input;
    let t = &input.template;
    let stats_template = init::SourceLevelTemplate {
        entry: t.entry,
        levels: [5, 6],
        unit_class: t.unit_class,
        rank: 3,
        health_multiplier: t.health_multiplier,
        power_multiplier: t.power_multiplier,
        armor_multiplier: t.armor_multiplier,
        damage_multiplier: t.damage_multiplier,
        damage_variance: t.damage_variance,
        attribute_multipliers: t.attribute_multipliers,
        legacy: t.legacy,
        legacy_ranged_damage: [init_float(3.0), init_float(6.0)],
        regenerate_stats: t.regenerate_stats,
    };
    let init::Rows::Complete(classes) = &input.class_levels else {
        unreachable!()
    };
    let init::Rows::Complete(spawns) = &input.spawn_template else {
        unreachable!()
    };
    let definitions =
        CreatureLevelDefinitions::from_rows(vec![stats_template], classes.clone()).unwrap();
    let mut factory = Factory::new();
    let mut row = spawn(40, 0.0);
    row.entry = t.entry;
    let mut world = World::new(vec![row]);
    let mut ct = template(t.entry);
    ct.levels = [5, 6];
    ct.displays = t.displays;
    ct.can_fly = true;
    factory.templates.insert(t.entry, ct);
    let init::Rows::Complete(models) = &input.models else {
        unreachable!()
    };
    factory.models = models
        .iter()
        .map(|m| {
            (
                m.id,
                CreatureModel {
                    id: m.id,
                    gender: m.gender,
                    other_gender: m.other_gender_id,
                },
            )
        })
        .collect();
    factory.source_level = Some((definitions, rates(), spawns[0]));
    world.load(0.0, &mut factory).unwrap();
    let state = factory.states.values().next().unwrap();
    let value = &factory.source_initialized[&state.object];
    assert!((5..=6).contains(&value.level));
    assert!(matches!(value.create_health, 145 | 162));
    assert_eq!(value.health, 1);
    assert_eq!(value.maximum_power, [0, 0, 100, 0, 1_000_000]);
    assert_eq!(factory.random.cursor.raw, 4);
    assert!(factory
        .random
        .events
        .iter()
        .all(|e| e.stream == state.object.thread));
    assert_eq!(factory.source_initialized.len(), 1);
    // No arithmetic re-resolution consumes a word or changes the selected level.
    let (defs, cfg, spawn) = factory.source_level.as_ref().unwrap();
    assert_eq!(&defs.resolve(state, cfg, spawn, true).unwrap(), value);
    assert_eq!(factory.random.cursor.raw, 4);
    let operands = CreatureAiTemplateOperands {
        ordinal: 0,
        entry: t.entry,
        faction: t.faction,
        extra_flags: t.extra_flags,
        unit_flags: t.unit_flags,
        type_flags: t.type_flags,
        static_flags: t.static_flags,
        rank: 3,
        inhabit_type: 4,
        stats_multipliers: [
            t.health_multiplier.bits(),
            t.power_multiplier.bits(),
            t.armor_multiplier.bits(),
        ],
        civilian: false,
        pursuit_ms: 15000,
        call_for_help: 0,
        creature_type: t.creature_type,
        ai_name: String::new(),
        script_name: String::new(),
    };
    factory
        .units
        .register(
            &world.objects,
            state.object,
            SourceUnitFields::creature(&operands),
        )
        .unwrap();
    let applied = factory
        .units
        .initialize_creature_level(&world.objects, state, defs, cfg, spawn)
        .unwrap();
    assert_eq!(&applied, value);
    let vitals = factory
        .units
        .event_unit(&world.objects, state.object)
        .unwrap();
    assert_eq!(
        (vitals.health, vitals.max_health),
        (1, value.maximum_health)
    );
    assert_eq!(
        (
            vitals.mana,
            vitals.max_mana,
            vitals.energy,
            vitals.max_energy
        ),
        (0, 0, 0, 0)
    );
    assert_eq!(factory.random.cursor.raw, 4);
}
fn init_float(value: f32) -> benilla_sim::classic_spell::SpellFloat {
    benilla_sim::classic_spell::SpellFloat::from_bits(value.to_bits()).unwrap()
}

#[test]
fn grid_load_refuses_unbound_or_wrong_level_stream_after_init_entry() {
    // Creature.cpp:346-452 precedes :523,1279. Use a real spawn model
    // override with no gender alternative so the level is the first draw.
    for stream in [None, Some(99)] {
        let mut row = spawn(40, 0.0);
        row.model = 11;
        let mut world = World::new(vec![row]);
        let mut factory = Factory::new();
        factory.templates.get_mut(&7).unwrap().levels = [5, 6];
        factory.random.stream = stream;
        let error = world.load(0.0, &mut factory).unwrap_err();
        assert_eq!(
            error.id,
            if stream.is_none() {
                "execution.creature.SelectLevel_urand_thread_owner"
            } else {
                "execution.creature.SelectLevel_urand_thread_mismatch"
            }
        );
        assert!(factory.observations.contains(&Observation::Phase(
            40,
            CreaturePhase::InitEntryEquipmentAndFlags
        )));
        assert!(!factory
            .observations
            .contains(&Observation::Phase(40, CreaturePhase::SelectLevelStats)));
        assert_eq!(factory.random.cursor.raw, 0);
    }
    let mut row = spawn(40, 0.0);
    row.model = 11;
    let mut world = World::new(vec![row]);
    let mut factory = Factory::new();
    factory.random.stream = None;
    factory.refuse = Some(CreaturePhase::SelectLevelStats);
    assert_eq!(
        world.load(0.0, &mut factory).unwrap_err().id,
        CreaturePhase::SelectLevelStats.missing_owner().id
    );
    assert_eq!(factory.random.cursor.raw, 0);
}
