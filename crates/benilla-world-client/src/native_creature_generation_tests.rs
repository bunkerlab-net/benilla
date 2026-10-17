// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored source rows exercise actual client create/value events across lives.
use super::*;
use benilla_sim::creature_init as init;
use n::{synthetic_creature_model_fixture as models, synthetic_fixture as f};

struct Catalog;
impl ClientCatalog for Catalog {
    fn build(&self) -> u32 {
        5875
    }
    fn manifest_digest(&self) -> Digest {
        [9; 32]
    }
    fn contains_map(&self, id: u32) -> bool {
        id == 7
    }
    fn contains_creature_display(&self, id: u32) -> bool {
        matches!(id, 31 | 34)
    }
    fn contains_item_display(&self, _: u32) -> bool {
        false
    }
    fn contains_spell(&self, id: u32) -> bool {
        matches!(id, 60001 | 60002)
    }
    fn player_power_type(&self, _: PlayerId) -> Option<u8> {
        Some(0)
    }
    fn item_metadata(&self, _: u32) -> Option<p::ItemInfo> {
        None
    }
    fn quest_metadata(&self, _: u32) -> Option<crate::QuestMetadata> {
        None
    }
    fn quest_greeting(&self, _: u32) -> Option<String> {
        None
    }
}

fn world() -> n::NativeWorld {
    let mut input = models::input();
    input.template.extra_flags = 2;
    input.template.unit_class = 2;
    input.template.maximum_level = 4;
    input.template.regenerate_stats = 0;
    input.template.corpse_delay_seconds = 1;
    input.template.npc_flags = 2;
    input.class_levels = init::Rows::Complete(
        (1..=4)
            .map(|level| init::ClassLevelRow {
                class: 2,
                level,
                base_health: 80 + u32::from(level) * 20,
                base_mana: 150 + u32::from(level) * 30,
                base_armor: 100 + u32::from(level) * 15,
                base_melee_attack_power: f::float(50.0 + f32::from(level) * 10.0),
                base_ranged_attack_power: f::float(70.0),
                base_damage: f::float(3.0 + f32::from(level)),
                attributes: [19 + u32::from(level), 23, 21, 22 + u32::from(level), 17],
            })
            .collect(),
    );
    let init::Rows::Complete(spawns) = &mut input.spawn_template else {
        unreachable!()
    };
    spawns[0].current_health = 1;
    spawns[0].current_mana = 7;
    let mut profile = f::profile();
    profile.npcs[0].ai.spawn = f::point(3.0);
    // These deliberately stale copied values must never reach the client.
    profile.npcs[0].ai.level = 20;
    profile.npcs[0].max_health = 9999;
    profile.npcs[0].faction_template = 17;
    profile.npcs[0].health_regeneration.regenerate_stats = 0;
    profile
        .initialize_creature_generations(
            init::Rows::Complete(vec![n::CreatureGenerationSource {
                spawn_id: 2,
                spawn_entry: input.template.entry,
                requested_spawn_template_entry: 0,
                respawn_delay_min_seconds: 2,
                respawn_delay_max_seconds: 2,
                input,
            }]),
            vec![2],
        )
        .unwrap();
    n::NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 1234).unwrap()
}

fn assert_fields(world: &n::NativeWorld, fields: &ObjectFields) {
    let npc = &world.state().npcs[&2];
    let selected = npc.initialized_creature().unwrap();
    assert_eq!(fields.unit_level(), Some(u32::from(selected.level)));
    assert_eq!(fields.unit_health(), Some(npc.health));
    assert_eq!(
        fields.unit_max_health(),
        Some(selected.stats.resources.max_health)
    );
    assert_ne!(
        fields.unit_level(),
        Some(u32::from(world.profile().npcs[0].ai.level))
    );
    assert_ne!(
        fields.unit_max_health(),
        Some(world.profile().npcs[0].max_health)
    );
    assert_eq!(fields.unit_power_type(), 0);
    assert_eq!(fields.unit_power(0), Some(7));
    assert_eq!(
        fields.unit_max_power(0),
        Some(selected.stats.resources.max_mana)
    );
    assert!(selected.stats.resources.max_mana > 7);
    assert_eq!(
        fields.unit_base_health(),
        Some(selected.stats.create_health)
    );
    assert_eq!(fields.unit_base_mana(), Some(selected.stats.create_mana));
    assert_eq!(fields.unit_class(), Some(2));
    assert_eq!(fields.unit_gender(), Some(selected.model.gender));
    assert_eq!(
        fields.unit_displayid(),
        Some(selected.model.display_id as i32)
    );
    assert_eq!(
        fields.unit_native_displayid(),
        Some(selected.model.display_id as i32)
    );
    assert_eq!(
        fields.object_scale_x().unwrap().to_bits(),
        selected.model.scale.bits()
    );
    assert_eq!(
        fields.unit_bounding_radius().to_bits(),
        selected.model.bounding_radius.bits()
    );
    assert_eq!(
        fields.unit_combat_reach().to_bits(),
        selected.model.combat_reach.bits()
    );
    assert_eq!(fields.unit_faction_template(), Some(selected.faction));
    assert_eq!(
        fields.unit_npc_flags(),
        if npc.health == 0 {
            0
        } else {
            selected.npc_flags
        }
    );
    for (index, value) in selected.stats.attributes.iter().enumerate() {
        assert_eq!(fields.unit_stat(index as u8), Some(*value));
    }
    for (index, value) in selected.stats.resistances.iter().enumerate() {
        assert_eq!(fields.unit_resistance(index as u8), Some(*value));
    }
    assert!(fields.unit_resistance(0).unwrap() > 0);
    assert_eq!(
        fields.unit_attack_power(),
        Some(selected.stats.damage.attack_power as i32)
    );
    assert_eq!(
        fields.unit_min_damage().unwrap().to_bits(),
        selected.stats.damage.minimum.bits()
    );
    assert_eq!(
        fields.unit_max_damage().unwrap().to_bits(),
        selected.stats.damage.maximum.bits()
    );
    for hand in 0..2 {
        assert_eq!(
            fields.unit_base_attack_time(hand),
            Some(selected.mainhand_interval_ms)
        );
    }
}

fn assert_create(world: &n::NativeWorld, projection: &NativeProjection, events: &[SessionEvent]) {
    let npc = &world.state().npcs[&2];
    let selected = npc.initialized_creature().unwrap();
    let expected_guid = projection.npc_guid(npc.id).unwrap();
    let (display, scale, fields) = events
        .iter()
        .find_map(|event| match event {
            SessionEvent::ObjectCreate {
                guid,
                display_id,
                scale,
                fields,
                ..
            } if *guid == expected_guid => Some((*display_id, *scale, fields)),
            _ => None,
        })
        .expect("new selected life must emit actual ObjectCreate");
    assert_eq!(display, Some(selected.model.display_id));
    assert_eq!(scale.to_bits(), selected.model.scale.bits());
    assert_fields(world, fields);
    assert!(events
        .iter()
        .any(|event| matches!(event, SessionEvent::CreatureName {
        display_id, creature_type: Some(1), rank: 0, ..
    } if *display_id == selected.model.display_id)));
}

#[test]
fn selected_generation_create_values_and_respawn_project_owned_stats() {
    let mut world = world();
    let mut projection = NativeProjection::new(&world, world.state().player, &Catalog).unwrap();
    assert_create(
        &world,
        &projection,
        &projection.initial_events(&world).unwrap(),
    );
    let initial = world.state().npcs[&2]
        .initialized_creature()
        .unwrap()
        .clone();
    let mut authority = f::SyntheticAuthority::default();
    let mut level_changed = false;
    let mut model_changed = false;
    let mut saw_corpse_values = false;
    for _ in 0..16 {
        let old_id = world.state().npcs[&2].id;
        let old_guid = projection.npc_guid(old_id).unwrap();
        let cast = f::cast(&world, 60001);
        world
            .step(world.state().tick + 1, Some(cast), &mut authority)
            .unwrap();
        let mut events = projection.advance(&world).unwrap();
        let mut revived = false;
        let mut destroyed = false;
        for _ in 0..100 {
            destroyed |= events.iter().any(|event| {
                matches!(event,
                SessionEvent::ObjectDestroyed(guid) if *guid == old_guid)
            });
            for event in &events {
                if let SessionEvent::ObjectValues { guid, fields } = event {
                    if *guid == old_guid && world.state().npcs[&2].id == old_id {
                        assert_fields(&world, fields);
                        saw_corpse_values |= fields.unit_health() == Some(0);
                    }
                }
            }
            if world.state().npcs[&2].id != old_id {
                assert_create(&world, &projection, &events);
                assert!(
                    destroyed,
                    "previous generation must be removed before replacement create"
                );
                let selected = world.state().npcs[&2].initialized_creature().unwrap();
                level_changed |= selected.level != initial.level;
                model_changed |= selected.model.display_id != initial.model.display_id;
                revived = true;
                break;
            }
            world
                .step(world.state().tick + 1, None, &mut authority)
                .unwrap();
            events = projection.advance(&world).unwrap();
        }
        assert!(
            revived,
            "authored lethal spell/corpse lifecycle did not respawn"
        );
        if level_changed && model_changed {
            break;
        }
    }
    assert!(saw_corpse_values);
    assert!(
        level_changed,
        "source stream must exercise a new level in this fixed-seed fixture"
    );
    assert!(
        model_changed,
        "source stream must exercise a new model in this fixed-seed fixture"
    );
    let restored = n::NativeWorld::restore_synthetic_component_test(
        world.profile().clone(),
        world.checkpoint().unwrap(),
    )
    .unwrap();
    let resumed = NativeProjection::new(&restored, restored.state().player, &Catalog).unwrap();
    assert_create(
        &restored,
        &resumed,
        &resumed.initial_events(&restored).unwrap(),
    );
}
