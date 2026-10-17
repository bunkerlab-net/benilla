// SPDX-License-Identifier: GPL-3.0-or-later
//! Byte-derived dry initialization with explicitly authored model and world inputs.
use super::super::{core::NonRenderParticipants, products};
use super::*;
use benilla_sim::{creature_ai as ai, creature_init as init};
use benilla_world_runtime::native::{
    synthetic_creature_model_fixture as models, synthetic_fixture as f,
    synthetic_motion_fixture as motion, *,
};
use std::time::Duration;

pub(in super::super) fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

/// Habitat=3, addon sheath=1 and spawn-template0 flags0 are retained entry-6
/// fields. Model IDs/scales/heights, stats, pose and aggressive test AI are
/// explicitly authored. Missing genuine client DBCs are not inferred from SQL.
pub(in super::super) fn profile(
    binding: CreatureInitializationSnapshot,
    geometry: [u8; 32],
) -> Profile {
    profile_with_faction(binding, geometry, 1)
}

fn profile_at(binding: CreatureInitializationSnapshot, geometry: [u8; 32], mid: f32) -> Profile {
    profile_at_with_faction(binding, geometry, mid, 1, None)
}

pub(in super::super) fn profile_with_faction(
    binding: CreatureInitializationSnapshot,
    geometry: [u8; 32],
    faction: u32,
) -> Profile {
    profile_at_with_faction(
        binding,
        geometry,
        0.5f32 * (64.0 * 533.333_3),
        faction,
        None,
    )
}

pub(in super::super) fn profile_with_alternate_model(
    binding: CreatureInitializationSnapshot,
    geometry: [u8; 32],
    faction: u32,
    radius: f32,
    reach: f32,
    raw_height: f32,
) -> Profile {
    profile_at_with_faction(
        binding,
        geometry,
        0.5f32 * (64.0 * 533.333_3),
        faction,
        Some((radius, reach, raw_height)),
    )
}

fn profile_at_with_faction(
    binding: CreatureInitializationSnapshot,
    geometry: [u8; 32],
    mid: f32,
    faction: u32,
    alternate_bounds: Option<(f32, f32, f32)>,
) -> Profile {
    let mut p = motion::profile();
    p.npcs[0].faction_template = faction;
    p.content_digest = binding.content;
    p.map_id = binding.map;
    p.geometry_revision = geometry;
    p.player.start = ai::Point::new(mid - 15.0, mid - 1.0, 9.0).unwrap();
    let mut input = models::input();
    if let Some((radius, reach, _)) = alternate_bounds {
        let init::Rows::Complete(models) = &mut input.models else {
            unreachable!()
        };
        let alternate = models.iter_mut().find(|m| m.id == 31).unwrap();
        alternate.bounding_radius =
            benilla_sim::classic_spell::SpellFloat::from_bits(radius.to_bits()).unwrap();
        alternate.combat_reach =
            benilla_sim::classic_spell::SpellFloat::from_bits(reach.to_bits()).unwrap();
    }
    input.template.entry = 6;
    input.template.maximum_level = 2;
    input.template.regenerate_stats = 2;
    input.template.corpse_delay_seconds = 1;
    input.template.faction = p.npcs[0].faction_template;
    input.template_addons = init::Rows::Complete(vec![init::TemplateAddonRow {
        entry: 6,
        addon: init::Addon {
            mount: 0,
            stand_state: 0,
            sheath_state: 1,
            emote: 0,
            move_flags: 0,
            auras: vec![],
        },
    }]);
    let init::Rows::Complete(levels) = &mut input.class_levels else {
        unreachable!()
    };
    let mut second = levels[0];
    second.level = 2;
    second.base_health += 17;
    levels.push(second);
    let init::Rows::Complete(spawns) = &mut input.spawn_template else {
        unreachable!()
    };
    spawns[0].current_health = 1;
    let raw = CreatureGenerationSource {
        spawn_id: 2,
        spawn_entry: 6,
        requested_spawn_template_entry: 0,
        respawn_delay_min_seconds: 1,
        respawn_delay_max_seconds: 2,
        input,
    };
    let (selected, selection) = init::prepare(&raw.input)
        .unwrap()
        .resolve_with_model_receipt(init::SelectionRolls {
            model: Some(0),
            gender: Some(0),
            level: Some(1),
        })
        .unwrap();
    let npc = &mut p.npcs[0];
    npc.entry = selected.entry;
    npc.display_id = selected.model.display_id;
    npc.faction_template = selected.faction;
    npc.max_health = selected.stats.resources.max_health;
    npc.ai.map_id = binding.map;
    npc.ai.content_revision = ai::movement::Revision(binding.content);
    npc.ai.spawn = ai::Point::new(mid - 21.0, mid - 1.0, 9.0).unwrap();
    npc.ai.level = selected.level;
    npc.ai.run_speed = ai::Real::from_bits(selected.model.run_speed.bits()).unwrap();
    npc.ai.home_speed = npc.ai.run_speed;
    npc.ai.combat_reach = ai::Real::from_bits(selected.model.combat_reach.bits()).unwrap();
    npc.ai.bounding_radius = ai::Real::from_bits(selected.model.bounding_radius.bits()).unwrap();
    npc.ai.mainhand_interval_ms = selected.mainhand_interval_ms;
    p.source_motion = Some(motion::genesis(&p, 2));
    let genesis = p.source_motion.as_mut().unwrap().npcs.get_mut(&2).unwrap();
    genesis.npc_policy.walk_speed = f::real(f32::from_bits(selected.model.walk_speed.bits()));
    genesis.npc_policy.home.addon_closure = HomeAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    let CreatureCapabilityRows::Complete(mut rows) = motion::creature_capability_sources(&p) else {
        unreachable!()
    };
    let CreatureCapabilityRows::Complete(templates) = &mut rows[0].templates else {
        unreachable!()
    };
    templates[0].inhabit_type = 3;
    let CreatureCapabilityRows::Complete(spawns) = &mut rows[0].spawn_templates else {
        unreachable!()
    };
    spawns[0].spawn_flags = 0;
    rows[0].addon_closure = CreatureCapabilityAddonClosure::GenerationSource {
        source_identity: raw.identity(),
    };
    rows[0].initialization = CreatureInitializationClosure::DryWater {
        map: binding.map,
        respawn_position: p.npcs[0].ai.spawn,
        snapshot: binding,
    };
    // Assemble the staged mutually dependent owners, then validate once all
    // model/height sources exist. Habitat is never temporarily relabeled Ground.
    p.creature_generations = Some(
        CreatureGenerationCatalog::validate_sources(init::Rows::Complete(vec![raw]), vec![2])
            .unwrap(),
    );
    p.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(CreatureCapabilityRows::Complete(rows))
            .unwrap(),
    );
    p.creature_collision_widths = Some(
        CreatureCollisionWidthCatalog::validate_sources(CreatureCollisionWidthRows::Complete(
            vec![CreatureCollisionWidthSource::from_initial_model(
                2,
                selection,
                &models::catalog_with_model_heights(
                    alternate_bounds.map_or(2.875, |(_, _, height)| height),
                    4.125,
                ),
            )
            .unwrap()],
        ))
        .unwrap(),
    );
    p.initialize_creature_collision_heights().unwrap();
    p.validate().unwrap();
    p
}

pub(in super::super) struct InitialAuthority<'a> {
    pub provider: SourceCreatureInitializationProvider<'a>,
    pub initializations: Vec<AuthorityRequest>,
    pub motion: motion::SyntheticMotionAuthority,
}
impl Authority for InitialAuthority<'_> {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if matches!(request.query, Query::CreatureInitialization(_)) {
            self.initializations.push(request.clone());
            let facts = self.provider.resolve(request, deadline()).map_err(|_| {
                NativeError::AuthorityUnavailable("authored loaded initialization product")
            })?;
            Ok(AuthorityReply {
                request: request.clone(),
                facts,
            })
        } else {
            self.motion.resolve(request)
        }
    }
}

#[test]
fn real_loaded_dry_bytes_initialize_habitat_three_before_level_selection() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let binding = snapshot_binding(&snapshot);
    let p = profile(binding, [77; 32]);
    assert!(NativeWorld::synthetic_component_test(p.clone(), [8; 16], f::player(), 7).is_err());
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = InitialAuthority {
        provider,
        initializations: vec![],
        motion: Default::default(),
    };
    let w = NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority,
    )
    .unwrap();
    assert_eq!(authority.initializations.len(), 1);
    let Query::CreatureInitialization(query) = &authority.initializations[0].query else {
        unreachable!()
    };
    assert_eq!(query.snapshot, binding);
    assert_eq!(query.required_type, 0x0f);
    assert_eq!(query.classification_height.bits(), 2.03128f32.to_bits());
    assert!(query.before_level_counter > query.first_counter);
    assert!(w.state().rng_counter > query.before_level_counter);
    let npc = &w.state().npcs[&2];
    let witness = npc.creature_capabilities().unwrap().witness();
    assert_eq!(witness.inhabit_type(), 3);
    assert_eq!(witness.swimming_movement_flags(), 0);
    assert_eq!(npc.initialized_creature().unwrap().unit_flags & 0x8000, 0);
    assert!(witness.capabilities().unwrap().can_swim());
    assert_eq!(
        witness.allowed_position_z_branch().unwrap(),
        AllowedPositionZBranch::Swimming
    );
    assert_eq!(npc.initialized_creature().unwrap().unit_bytes_2, 0x1001);
    let restored =
        NativeWorld::restore_synthetic_component_test(w.profile().clone(), w.checkpoint().unwrap())
            .unwrap();
    assert_eq!(restored.state_digest(), w.state_digest());
    assert_eq!(
        authority
            .provider
            .resolve(&authority.initializations[0], Instant::now()),
        Err(Failure::Deadline)
    );
    for case in 0..8 {
        let mut bad = authority.initializations[0].clone();
        let Query::CreatureInitialization(query) = &mut bad.query else {
            unreachable!()
        };
        match case {
            0 => query.snapshot.source_epoch[0] ^= 1,
            1 => query.snapshot.effective_config[0] ^= 1,
            2 => query.collision_height = f::float(123.0),
            3 => query.position.x = f::real(0.0),
            4 => query.classification_height = f::float(1.0),
            5 => query.snapshot.inventory[0] ^= 1,
            6 => query.snapshot.policy[0] ^= 1,
            _ => {
                bad.binding.world[0] ^= 1;
                query.binding.world = bad.binding.world;
            }
        }
        assert_eq!(
            authority.provider.resolve(&bad, deadline()),
            Err(Failure::Binding)
        );
    }
}

#[test]
fn declared_dry_inventory_cannot_answer_a_spawn_in_an_unloaded_tile() {
    let (products, config) = products::authored_ground_height();
    let participants = NonRenderParticipants::authored_fixture(config.content, config.map);
    let snapshot = products
        .load_map_height(config, Some(&participants))
        .unwrap();
    let p = profile_at(snapshot_binding(&snapshot), [77; 32], 0.0);
    let provider = SourceCreatureInitializationProvider::bind([8; 16], &p, snapshot).unwrap();
    let mut authority = InitialAuthority {
        provider,
        initializations: vec![],
        motion: Default::default(),
    };
    assert!(NativeWorld::synthetic_component_test_with_initialization(
        p,
        [8; 16],
        f::player(),
        7,
        None,
        &mut authority
    )
    .is_err());
    assert_eq!(authority.initializations.len(), 1);
    assert_eq!(
        authority
            .provider
            .resolve(&authority.initializations[0], deadline()),
        Err(Failure::MissingGeometry)
    );
}

#[path = "lifecycle_tests.rs"]
mod lifecycle_tests;
