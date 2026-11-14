// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored generation rows through the actual controller and durable store.
use super::*;
use benilla_sim::creature_init as init;

#[test]
fn selected_level_stats_survive_controller_save_and_reopen_without_reroll() {
    let h = Harness::new();
    let mut base = fixture::profile();
    base.npcs[0].health_regeneration.unit_flags = 0x100;
    base.source_motion = Some(n::synthetic_motion_fixture::genesis(&base, 2));
    let mut data =
        n::synthetic_creature_model_fixture::with_complete_scene_generations_and_source_edit(
            base,
            |source| {
                let f = |n: f32| {
                    benilla_sim::classic_spell::SpellFloat::from_bits(n.to_bits()).unwrap()
                };
                let t = &mut source.input.template;
                t.minimum_level = 1;
                t.maximum_level = 2;
                t.health_multiplier = f(2.0);
                t.armor_multiplier = f(1.5);
                t.damage_multiplier = f(1.5);
                t.damage_variance = f(0.5);
                t.attribute_multipliers = [f(1.0); 5];
                t.unit_flags = 0x100;
                t.static_flags[0] = 2;
                t.extra_flags |= 0x10000;
                t.displays = [
                    init::DisplayChoice {
                        id: 60001,
                        probability: 1,
                    },
                    init::DisplayChoice {
                        id: 0,
                        probability: 0,
                    },
                    init::DisplayChoice {
                        id: 0,
                        probability: 0,
                    },
                    init::DisplayChoice {
                        id: 0,
                        probability: 0,
                    },
                ];
                source.input.class_levels = init::Rows::Complete(
                    (1..=2)
                        .map(|level| init::ClassLevelRow {
                            class: 1,
                            level,
                            base_health: 40 + 40 * u32::from(level),
                            base_mana: 0,
                            base_armor: 60 + 40 * u32::from(level),
                            base_melee_attack_power: f(14.0 * f32::from(level)),
                            base_ranged_attack_power: f(14.0),
                            base_damage: f(4.0 * f32::from(level)),
                            attributes: [10; 5],
                        })
                        .collect(),
                );
                let init::Rows::Complete(models) = &source.input.models else {
                    unreachable!()
                };
                let mut model = models[0];
                model.id = 60001;
                model.other_gender_id = 0;
                source.input.models = init::Rows::Complete(vec![model]);
                source.input.displays = init::Rows::Complete(vec![init::DisplayRow {
                    id: 60001,
                    scale: f(1.0),
                }]);
                let init::Rows::Complete(spawns) = &mut source.input.spawn_template else {
                    unreachable!()
                };
                spawns[0].current_health = 0;
            },
        );
    data.source_motion = None;
    let mut controller = h.start(data.clone());
    let retained = data
        .creature_generations
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap();
    assert_eq!(retained.input.template.extra_flags & 0x10000, 0x10000);
    assert_eq!(retained.input.template.static_flags[0], 2);
    let selected = controller.world.state().npcs[&2]
        .initialized_creature()
        .unwrap()
        .clone();
    let (health, armor, minimum, maximum) = match selected.level {
        1 => (160, 140, 6_f32, 9_f32),
        2 => (240, 200, 12_f32, 18_f32),
        other => panic!("unexpected selected level {other}"),
    };
    assert_eq!(selected.stats.resources.max_health, health);
    assert_eq!(selected.stats.armor, armor);
    assert_eq!(selected.stats.damage.minimum.bits(), minimum.to_bits());
    assert_eq!(selected.stats.damage.maximum.bits(), maximum.to_bits());
    let checkpoint = controller.world.checkpoint().unwrap();
    let counter = controller.world.state().rng_counter;
    controller.shutdown().unwrap();
    drop(controller);
    let reopened = h.reopen(data);
    assert_eq!(reopened.world.checkpoint().unwrap(), checkpoint);
    assert_eq!(
        reopened.world.state().npcs[&2].initialized_creature(),
        Some(&selected)
    );
    assert_eq!(reopened.world.state().rng_counter, counter);
    let retained = reopened
        .world
        .profile()
        .creature_generations
        .as_ref()
        .unwrap()
        .source(2)
        .unwrap();
    assert_eq!(retained.input.template.extra_flags & 0x10000, 0x10000);
    assert_eq!(retained.input.template.static_flags[0], 2);
}
