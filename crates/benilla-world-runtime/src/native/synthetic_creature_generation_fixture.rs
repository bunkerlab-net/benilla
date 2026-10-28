// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored component-only generation fixtures; no production admission.
use super::{synthetic_creature_model_fixture as models, synthetic_fixture as f, *};
use benilla_sim::creature_init as init;

pub(in crate::native) fn source(spawn: u64, minimum: u8, maximum: u8) -> CreatureGenerationSource {
    let mut input = models::input();
    input.spawn_id = spawn;
    input.template.extra_flags = 2;
    input.template.minimum_level = minimum;
    input.template.maximum_level = maximum;
    input.template.corpse_delay_seconds = 5;
    input.template.regenerate_stats = 2;
    input.template.displays[0].probability = 1;
    input.class_levels = init::Rows::Complete(
        (minimum..=maximum)
            .map(|level| init::ClassLevelRow {
                class: 1,
                level,
                base_health: 60 + u32::from(level) * 17,
                base_mana: 0,
                base_armor: 70 + u32::from(level) * 11,
                base_melee_attack_power: f::float(50.0 + f32::from(level) * 10.0),
                base_ranged_attack_power: f::float(70.0),
                base_damage: f::float(3.0 + f32::from(level)),
                attributes: [19, 23, 21, 22, 17],
            })
            .collect(),
    );
    let init::Rows::Complete(rows) = &mut input.spawn_template else {
        unreachable!()
    };
    rows[0].current_health = 1;
    CreatureGenerationSource {
        spawn_id: spawn,
        spawn_entry: input.template.entry,
        requested_spawn_template_entry: 0,
        respawn_delay_min_seconds: 2,
        respawn_delay_max_seconds: 3,
        input,
    }
}

pub(in crate::native) fn install(
    mut p: Profile,
    sources: Vec<CreatureGenerationSource>,
    order: Vec<u64>,
) -> Profile {
    // Static fixture metadata is deliberately compatible with the raw rows. The
    // low-level exemplar is not the world's live selection or an RNG receipt.
    for source in &sources {
        let plan = init::prepare(&source.input).unwrap();
        let model = plan.model_roll_range().map(|range| range.0);
        let gender = plan.gender_roll_range(model).unwrap().map(|range| range.1);
        let level = plan.level_roll_range().map(|range| range.0);
        let value = plan
            .resolve(init::SelectionRolls {
                model,
                gender,
                level,
            })
            .unwrap();
        let npc = p
            .npcs
            .iter_mut()
            .find(|npc| npc.ai.creature_id == source.spawn_id)
            .unwrap();
        npc.entry = value.entry;
        npc.display_id = value.model.display_id;
        npc.faction_template = value.faction;
        npc.max_health = value.stats.resources.max_health;
        npc.ai.level = value.level;
        npc.ai.run_speed = ai::Real::from_bits(value.model.run_speed.bits()).unwrap();
        npc.ai.home_speed = npc.ai.run_speed;
        npc.ai.combat_reach = ai::Real::from_bits(value.model.combat_reach.bits()).unwrap();
        npc.ai.bounding_radius = ai::Real::from_bits(value.model.bounding_radius.bits()).unwrap();
        npc.ai.mainhand_interval_ms = value.mainhand_interval_ms;
        if let Some(combat) = &mut npc.combat {
            combat.minimum_damage = value.stats.damage.minimum;
            combat.maximum_damage = value.stats.damage.maximum;
            combat.no_crushing_blows = value.no_crushing_blows;
            combat.creature_is_critter = value.creature_is_critter;
        }
    }
    p.initialize_creature_generations(init::Rows::Complete(sources), order)
        .unwrap();
    p
}

pub(in crate::native) fn complete_scene_sources(p: &Profile) -> Vec<CreatureGenerationSource> {
    p.npcs
        .iter()
        .map(|npc| {
            let mut raw = source(npc.ai.creature_id, 1, 1);
            raw.input.template.extra_flags = match npc.ai.reaction {
                ai::Reaction::Defensive => 2,
                ai::Reaction::Aggressive => 0,
                ai::Reaction::Passive => panic!("source fixture has no passive reaction writer"),
            };
            raw.spawn_entry = npc.entry;
            raw.input.template.entry = npc.entry;
            raw.input.template.faction = npc.faction_template;
            raw.input.template.creature_type = 7; // Exact authored source-spell target row.
            raw.input.template.mainhand_interval_ms = npc.ai.mainhand_interval_ms;
            raw.input.template.npc_flags = u32::from(npc.ai.creature_id == 3) * 2;
            let shift = u32::try_from(npc.ai.creature_id - 2).unwrap() * 100;
            for display in &mut raw.input.template.displays {
                if display.id != 0 {
                    display.id += shift;
                }
            }
            let init::Rows::Complete(rows) = &mut raw.input.models else {
                unreachable!()
            };
            for row in rows {
                row.id += shift;
                if row.other_gender_id != 0 {
                    row.other_gender_id += shift;
                }
                row.bounding_radius = f::float(npc.ai.bounding_radius.as_f32());
                row.combat_reach = f::float(npc.ai.combat_reach.as_f32());
                row.speed_run = f::float(npc.ai.run_speed.as_f32() / 7.0);
                row.speed_walk = f::float(
                    p.npc_motion(npc.ai.creature_id)
                        .unwrap()
                        .npc_policy
                        .walk_speed
                        .as_f32()
                        / 2.5,
                );
            }
            let init::Rows::Complete(rows) = &mut raw.input.displays else {
                unreachable!()
            };
            for row in rows {
                row.id += shift;
            }
            let init::Rows::Complete(rows) = &mut raw.input.spawn_template else {
                unreachable!()
            };
            rows[0].spawn_flags = 1; // Exact authored capability source row.
            raw
        })
        .collect()
}
