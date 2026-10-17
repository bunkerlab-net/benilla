// SPDX-License-Identifier: GPL-3.0-or-later
//! Explicitly authored two-owner source-shaped fixtures. No private input, loaded
//! cell traversal, native geometry, or production gameplay admission is implied.
use super::{synthetic_fixture as f, *};
use benilla_sim::creature_motion as m;

pub const HOSTILE: u64 = 2;
pub const NEUTRAL: u64 = 3;

struct OwnerParameters {
    spawn: u64,
    entry: u32,
    faction_template: u32,
    name: &'static str,
    neutral_to_all: bool,
    position: ai::Point,
    max_health: u32,
    run_speed: f32,
    combat_reach: f32,
    bounding_radius: f32,
    detection: f32,
    mainhand_interval_ms: u32,
}
fn owner_profile(p: &Profile, values: OwnerParameters) -> NpcProfile {
    NpcProfile {
        health_regeneration: benilla_sim::creature_regeneration::Policy {
            kind: benilla_sim::creature_regeneration::CreatureKind::Ordinary,
            regenerate_stats: 2,
            creature_type_flags: 0,
            unit_flags: 0,
            power: benilla_sim::creature_regeneration::PowerEvidence::Known {
                power_type: 0,
                max_power: 0,
            },
        },
        combat: Some(CreatureCombatProfile {
            minimum_damage: f::float(6.0),
            maximum_damage: f::float(8.0),
            no_crushing_blows: false,
            creature_is_critter: false,
        }),
        ai: ai::Profile {
            creature_id: values.spawn,
            map_id: p.map_id,
            content_revision: ai::movement::Revision(p.content_digest),
            spawn: values.position,
            clock_origin_ms: p.clock_origin_ms,
            level: 1,
            reaction: ai::Reaction::Aggressive,
            neutral_to_all: values.neutral_to_all,
            detection: f::real(values.detection),
            aggro_rate: f::real(1.0),
            combat_reach: f::real(values.combat_reach),
            bounding_radius: f::real(values.bounding_radius),
            melee_leeway: f::real(0.0),
            run_speed: f::real(values.run_speed),
            home_speed: f::real(values.run_speed),
            mainhand_interval_ms: values.mainhand_interval_ms,
            pursuit_ms: 15000,
            leash_radius: f::real(30.0),
            hard_leash_radius: None,
            corpse_decay_ms: 500,
            respawn_delay_seconds: 2,
            unsupported: vec![],
        },
        entry: values.entry,
        display_id: values.entry,
        faction_template: values.faction_template,
        name: values.name.into(),
        max_health: values.max_health,
    }
}
fn owner_motion(
    npc: &NpcProfile,
    walk_speed: f32,
    collision_height: f32,
    spawn_orientation: f32,
) -> NpcMotionGenesis {
    NpcMotionGenesis {
        npc_policy: MotionPolicy {
            walk_speed: f::real(walk_speed),
            run_speed: npc.ai.run_speed,
            settings: m::GroundSettings {
                combat_reach: npc.ai.combat_reach,
                bounding_radius: npc.ai.bounding_radius,
                collision_height: f::real(collision_height),
                no_wounded_slowdown: false,
                backpedal_enabled: true,
                fanning_enabled: true,
            },
            audit: m::MotionAudit {
                flight_swimming_transport: m::Evidence::Absent,
                immobilization: m::Evidence::Absent,
                non_melee_casting: m::Evidence::Absent,
                speed_auras: m::Evidence::Absent,
                combat_scripts: m::Evidence::Absent,
                target_airborne_or_transport: m::Evidence::Absent,
            },
            home: HomeAudit {
                reset_position_override: m::Evidence::Absent,
                restore_temporary_faction: m::Evidence::Absent,
                addon_closure: HomeAddonClosure::Absent,
                scripted_home_events: m::Evidence::Absent,
                temporary_summon_callback: m::Evidence::Absent,
            },
        },
        npc_can_enter_combat: true,
        npc_stand_state: 0,
        npc_spawn_orientation: Orientation::new(spawn_orientation).unwrap(),
        npc_initial_active: false,
        chase_walking: false,
        home_walking: false,
        creature_detection_range: npc.ai.detection.as_f32() as u32,
    }
}

/// Explicit fabricated closures, not a raw source producer or inferred absence.
pub fn visitor_input(profile: &Profile, owner_order: Vec<u64>) -> SourceVisitorInput {
    SourceVisitorInput {
        owner_order,
        map_id: profile.map_id,
        map_instance_type: 0,
        owners: SourceVisitorRows::Complete(
            profile
                .npcs
                .iter()
                .map(|npc| SourceVisitorOwner {
                    spawn_id: npc.ai.creature_id,
                    entry: npc.entry,
                    ai_name: String::new(),
                    script_name: String::new(),
                    script_id: 0,
                    default_movement_type: 0,
                    spell_list_id: 0,
                    entry_events: SourceVisitorRows::Complete(vec![]),
                    guid_events: SourceVisitorRows::Complete(vec![]),
                    addon_closure: SourceVisitorAddonClosure::Absent,
                    overrides: SourceVisitorRows::Complete(vec![]),
                })
                .collect(),
        ),
    }
}
pub fn profile() -> Profile {
    profile_with_order([HOSTILE, NEUTRAL])
}
pub fn profile_with_order(owner_order: [u64; 2]) -> Profile {
    profile_with_spawns(HOSTILE, NEUTRAL, owner_order)
}
/// Each role's stable ID is authored independently of both traversal and update order.
pub fn profile_with_spawns(
    hostile_spawn: u64,
    neutral_spawn: u64,
    owner_order: [u64; 2],
) -> Profile {
    let mut profile = f::initialized_profile();
    let hostile = owner_profile(
        &profile,
        OwnerParameters {
            spawn: hostile_spawn,
            entry: 60001,
            faction_template: 2,
            name: "Authored hostile visitor A",
            neutral_to_all: false,
            position: f::point(6.0),
            max_health: 3000,
            run_speed: 2.5,
            combat_reach: 1.5,
            bounding_radius: 0.5,
            detection: 20.0,
            mainhand_interval_ms: 100,
        },
    );
    let neutral = owner_profile(
        &profile,
        OwnerParameters {
            spawn: neutral_spawn,
            entry: 96002,
            faction_template: 3,
            name: "Authored neutral visitor B",
            neutral_to_all: true,
            position: ai::Point::new(-6.0, 3.0, 0.0).unwrap(),
            max_health: 2200,
            run_speed: 3.75,
            combat_reach: 2.0,
            bounding_radius: 0.75,
            detection: 25.0,
            mainhand_interval_ms: 400,
        },
    );
    let owner_motion = BTreeMap::from([
        (hostile_spawn, owner_motion(&hostile, 1.25, 2.0, 1.0)),
        (neutral_spawn, owner_motion(&neutral, 1.5, 3.5, 2.5)),
    ]);
    profile.npcs = vec![hostile, neutral];
    profile.npcs.sort_by_key(|npc| npc.ai.creature_id);
    // Validate interactions on an initialized non-source draft; final selection
    // happens only once every owner and the combat authority can be checked.
    let mut interactions = synthetic_combat_fixture::source_rows(&profile);
    interactions.factions.push(InteractionFactionSource {
        template_id: 3,
        faction: 0,
        template_flags: 0,
        group_mask: 0,
        friend_group_mask: 0,
        enemy_group_mask: 0,
        enemies: [0; 4],
        friends: [0; 4],
        reputation_list_id: None,
    });
    profile.initialize_interactions(interactions).unwrap();
    profile.source_motion = Some(MotionGenesis {
        spell_hits: profile
            .abilities
            .iter()
            .filter_map(|ability| {
                let AbilityAdmission::Available(spec) = &ability.admission else {
                    return None;
                };
                Some((
                    ability.id,
                    SpellHitPolicy {
                        attributes: [0; 5],
                        implicit_target_a: std::array::from_fn(|index| {
                            if matches!(spec.effects[index], spell::MageEffect::Empty) {
                                0
                            } else {
                                6
                            }
                        }),
                        implicit_target_b: [0; 3],
                        aura_interrupt_flags: 0,
                    },
                ))
            })
            .collect(),
        player_can_enter_combat: true,
        player_collision_height: f::real(2.0),
        player_movement_flags: m::GroundMovementFlags::STOPPED,
        creature_initial_aggro_delay_ms: 200,
        relocation_aggro_delay_ms: 100,
        aggro_rate: f::real(1.0),
        npcs: owner_motion,
    });
    profile.creature_capabilities = Some(
        CreatureCapabilityCatalog::validate_sources(
            synthetic_motion_fixture::creature_capability_sources(&profile),
        )
        .unwrap(),
    );
    profile.source_visitors = Some(
        SourceVisitorCatalog::validate_sources(visitor_input(&profile, owner_order.into()))
            .unwrap(),
    );
    profile
        .initialize_combat_authority(
            profile
                .npcs
                .iter()
                .map(|npc| NpcCombatAuthoritySource {
                    entry: npc.entry,
                    extra_flags: 0,
                    static_flags1: 0,
                    static_flags2: 0,
                    static_flags3: 0,
                    static_flags4: 0,
                })
                .collect(),
        )
        .unwrap();
    profile.validate().unwrap();
    profile
}

#[path = "synthetic_two_owner_authority.rs"]
mod authority;
pub use authority::*;
