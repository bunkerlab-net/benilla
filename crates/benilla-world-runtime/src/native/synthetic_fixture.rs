// SPDX-License-Identifier: GPL-3.0-or-later
//! Fabricated component-test inputs, never native starter rows, navigation or melee parity.
use super::*;
use ai::movement::*;
pub fn point(x: f32) -> ai::Point {
    ai::Point::new(x, 0.0, 0.0).unwrap()
}
pub fn real(x: f32) -> ai::Real {
    ai::Real::new(x).unwrap()
}
pub fn float(x: f32) -> spell::SpellFloat {
    spell::SpellFloat::from_bits(x.to_bits()).unwrap()
}
pub fn player() -> crate::PlayerId {
    crate::PlayerId {
        owner: [1; 16],
        character: [2; 16],
    }
}
pub fn profile() -> Profile {
    let value = |base_points, die_sides| spell::EffectValue {
        base_points,
        base_dice: 1,
        die_sides,
        points_per_level: spell::SpellFloat::ZERO,
        dice_per_level: spell::SpellFloat::ZERO,
        base_level: 1,
        spell_level: 1,
        max_level: 0,
    };
    let spec = spell::MageProjectileSpec {
        spell_id: 60001,
        effects: [
            spell::MageEffect::SchoolDamage(value(10, 6)),
            spell::MageEffect::PeriodicDamage {
                value: value(1, 3),
                amplitude_ms: 500,
                duration_ms: 2000,
            },
            spell::MageEffect::Empty,
        ],
        spell_rank: 5,
        cast_ms: 200,
        mana_cost: 10,
        global_category: 9,
        global_recovery_ms: 200,
        recovery_ms: 100,
        category: 0,
        category_recovery_ms: 0,
        projectile_speed: float(4.0),
        movement_interrupts: true,
        damage_pushback: true,
    };
    Profile {
        admin: None,
        player_slow: None,
        source_motion: None,
        creature_capabilities: None,
        creature_collision_widths: None,
        creature_collision_heights: None,
        interactions: None,
        quest_progression: None,
        combat: None,
        content_digest: [3; 32],
        authority_policy: [4; 32],
        geometry_revision: [5; 32],
        tick_ms: 100,
        clock_origin_ms: 0,
        map_id: 7,
        player: PlayerProfile {
            start: point(0.0),
            orientation: Orientation::ZERO,
            level: 1,
            max_health: 100,
            max_mana: 100,
            current_health: 90,
            current_mana: 90,
            combat_reach: real(1.5),
            bounding_radius: real(0.5),
            normal_mana_per_second: float(10.0),
            interrupted_mana_per_second: float(0.0),
            world_mana_rate: spell::SpellFloat::ONE,
            mana_exclusions: ManaExclusions {
                cast_item: false,
                triggered_by_aura: false,
                dont_block_mana_regen: false,
                other_mana_mutation: false,
            },
        },
        abilities: vec![
            Ability {
                id: 60001,
                school: 2,
                admission: AbilityAdmission::Available(spec),
            },
            Ability {
                id: 60002,
                school: 4,
                admission: AbilityAdmission::Unavailable {
                    reason: "synthetic unsupported aura opcode 77".into(),
                },
            },
        ],
        npcs: vec![NpcProfile {
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
            combat: None,
            ai: ai::Profile {
                creature_id: 2,
                map_id: 7,
                content_revision: Revision([3; 32]),
                spawn: point(16.0),
                clock_origin_ms: 0,
                level: 1,
                reaction: ai::Reaction::Defensive,
                neutral_to_all: false,
                detection: real(20.0),
                aggro_rate: real(1.0),
                combat_reach: real(1.5),
                bounding_radius: real(0.5),
                melee_leeway: real(0.0),
                run_speed: real(2.5),
                home_speed: real(2.5),
                mainhand_interval_ms: 2000,
                pursuit_ms: 15000,
                leash_radius: real(30.0),
                hard_leash_radius: None,
                corpse_decay_ms: 500,
                respawn_delay_seconds: 2,
                unsupported: vec![],
            },
            entry: 60001,
            display_id: 60001,
            faction_template: 1,
            name: "Synthetic mechanics target".into(),
            max_health: 1000,
        }],
    }
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], player(), 1234).unwrap()
}
pub fn cast(w: &NativeWorld, ability: u32) -> OwnedInput {
    OwnedInput {
        authenticated_owner: player().owner,
        input: Input {
            world: w.state().world,
            tick: w.state().tick + 1,
            player: player(),
            sequence: w.state().next_sequence,
            command: Command::Cast {
                ability,
                target: w.state().npcs[&2].id,
            },
        },
    }
}
#[derive(Default)]
pub struct SyntheticAuthority {
    pub missing_navigation: bool,
    pub stale_binding: bool,
    pub resist_chance: u16,
    pub melee_damage: u32,
    pub missing_melee: bool,
}
impl Authority for SyntheticAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        let facts = match &request.query {
            Query::NpcSpell(_) | Query::SourceSpell(_) => {
                return Err(NativeError::AuthorityUnavailable(
                    "source spell spatial fixture",
                ))
            }
            Query::SourceMotion(_) => {
                return Err(NativeError::AuthorityUnavailable(
                    "legacy authored fixture has no source motion evidence",
                ))
            }
            Query::LootInteraction { .. } => {
                return Err(NativeError::AuthorityUnavailable(
                    "synthetic fixture has no loot facts",
                ))
            }
            Query::QuestInteraction { .. } | Query::QuestStatus { .. } => {
                return Err(NativeError::AuthorityUnavailable(
                    "synthetic quest facts not supplied",
                ))
            }
            Query::NativeMelee { .. } => {
                return Err(NativeError::AuthorityUnavailable(
                    "initialized melee geometry/callback facts",
                ))
            }
            Query::Spell { .. } => Facts::Spell(SpellFacts {
                other_failure: None,
                allowed: true,
                immune: false,
                resist_chance: self.resist_chance,
                critical_chance: 500,
            }),
            Query::Npc {
                player_position, ..
            } => Facts::Npc(NpcFacts {
                can_attack_in_combat: true,
                can_attack_on_sight: true,
                visible: true,
                accessible: true,
                line_of_sight: true,
                can_initiate_attack: true,
                stationary_melee_refresh_eligible: false,
                creature_moving_and_running: true,
                chase_goal: Some(*player_position),
            }),
            Query::PlayerMovement { .. } => {
                if self.missing_navigation {
                    return Err(NativeError::AuthorityUnavailable(
                        "synthetic missing player geometry",
                    ));
                }
                Facts::PlayerMovement { accepted: true }
            }
            Query::Movement(r) => {
                if self.missing_navigation {
                    return Err(NativeError::Navigation(ai::AiError::Movement(
                        BlockReason::MissingGeometry,
                    )));
                }
                // Test-only straight-line X solver. Production requires source near-point,
                // continuous path/terrain/collision validation and truthful revision binding.
                let delta = r.goal.x.as_f32() - r.origin.x.as_f32();
                let step = (r.speed.as_f32() * r.elapsed_ms as f32 / 1000.0).min(delta.abs());
                let x = r.origin.x.as_f32() + step * delta.signum();
                let position = point(x);
                let travelled = real((x - r.origin.x.as_f32()).abs());
                Facts::Movement(MovementOutcome {
                    request_id: r.request_id,
                    map_id: r.map_id,
                    content_revision: r.content_revision,
                    geometry_revision: r.geometry_revision,
                    result: MovementResult::Progress {
                        position,
                        travelled,
                        arrived: position == r.goal,
                        checks: SegmentChecks {
                            loaded_coverage: Check::Passed,
                            line_of_sight: Check::Passed,
                            body_sweep: Check::Passed,
                            continuous_ground_support: Check::Passed,
                            dry_ground: Check::Passed,
                        },
                    },
                })
            }
            Query::Melee { .. } => {
                if self.missing_melee {
                    return Err(NativeError::AuthorityUnavailable(
                        "native NPC melee kernel/callbacks",
                    ));
                }
                Facts::Melee(MeleeOutcome {
                    damage: self.melee_damage,
                    hit_info: 0,
                    victim_state: 1,
                    blocked: 0,
                    absorb: 0,
                    resist: 0,
                    school: 0,
                    pushback: true,
                })
            }
        };
        let mut request = request.clone();
        if self.stale_binding {
            request.binding.tick += 1;
        }
        Ok(AuthorityReply { request, facts })
    }
}

/// Source-row-shaped but deliberately invented metadata. No Classic DB rows or admission.
pub fn initialized_starter_input() -> benilla_sim::starter_init::StarterInput {
    use benilla_sim::starter_init as i;
    let mut rows = Vec::new();
    for id in [60001, 60002] {
        let mut words = [0; 173];
        words[0] = id;
        words[21] = 2;
        words[58] = u32::MAX;
        rows.push(i::SpellRow {
            id,
            words,
            duration_ms: Some(-1),
            previous_rank: None,
            talent: false,
            learned_spells: i::Rows::Complete(vec![]),
            external_initial_hooks: false,
        });
    }
    let mut item = i::ItemRow {
        id: 900001,
        class: 4,
        subclass: 0,
        inventory_type: 5,
        allowable_class: u32::MAX,
        allowable_race: u32::MAX,
        required_level: 1,
        required_skill: 0,
        required_skill_rank: 0,
        required_spell: 0,
        required_honor_rank: 0,
        required_reputation_faction: 0,
        flags: 0,
        unique_max_count: 0,
        stackable: 1,
        bag_family: 0,
        container_slots: 0,
        item_set: 0,
        random_property: 0,
        duration: 0,
        scripted: false,
        stats: [i::ItemStat { kind: 0, value: 0 }; 10],
        armor: 7,
        resistances: [0; 6],
        block: 0,
        spells: [i::ItemSpell { id: 0, trigger: 0 }; 5],
        damage: [i::ItemDamage {
            min_bits: 0,
            max_bits: 0,
            school: 0,
        }; 5],
        delay_ms: 0,
        max_durability: 1,
    };
    item.stats[0] = i::ItemStat { kind: 5, value: 3 };
    item.stats[1] = i::ItemStat { kind: 6, value: 2 };
    i::StarterInput {
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        class_power_type: 0,
        create_attributes: i::Attributes([17, 23, 29, 31, 37]),
        create_health: 5000,
        create_mana: 59,
        default_skills: i::Rows::Complete(vec![
            i::DefaultSkill { id: 95, step: 0 },
            i::DefaultSkill { id: 8, step: 0 },
        ]),
        default_spells: i::Rows::Complete(vec![60001, 60002]),
        skill_lines: vec![
            i::SkillLineRow {
                id: 95,
                category: 6,
            },
            i::SkillLineRow { id: 8, category: 6 },
        ],
        skill_race_class: i::Rows::Complete(vec![]),
        skill_tiers: vec![],
        skill_abilities: i::Rows::Complete(vec![
            i::SkillAbilityRow {
                row_id: 1,
                skill: 8,
                spell: 60001,
                race_mask: 0,
                class_mask: 0,
                required_value: 0,
                learn_on_get_skill: 0,
                forward_spell_id: 0,
                max_value: 0,
            },
            i::SkillAbilityRow {
                row_id: 2,
                skill: 8,
                spell: 60002,
                race_mask: 0,
                class_mask: 0,
                required_value: 0,
                learn_on_get_skill: 0,
                forward_spell_id: 0,
                max_value: 0,
            },
        ]),
        spells: rows,
        active_support: vec![
            i::ActiveAbilitySupport {
                spell_id: 60001,
                unavailable_reason: None,
            },
            i::ActiveAbilitySupport {
                spell_id: 60002,
                unavailable_reason: Some("synthetic unsupported aura opcode 77".into()),
            },
        ],
        item_grants: i::Rows::Complete(vec![i::ItemGrant {
            source: i::GrantSource::Custom,
            source_order: 0,
            item: 900001,
            count: 1,
        }]),
        items: vec![item],
        item_creation_hooks: false,
    }
}
pub fn initialized_profile() -> Profile {
    let input = initialized_starter_input();
    let mut p = profile();
    p.npcs[0].ai.spawn = point(3.0);
    p.npcs[0].ai.mainhand_interval_ms = 100;
    p.npcs[0].max_health = 3000;
    p.npcs[0].combat = Some(CreatureCombatProfile {
        minimum_damage: float(6.0),
        maximum_damage: float(8.0),
        no_crushing_blows: false,
        creature_is_critter: false,
    });
    if let AbilityAdmission::Available(spec) = &mut p.abilities[0].admission {
        spec.spell_rank = 1;
    }
    p.initialize_player(&input, float(100.0), 1).unwrap();
    p
}
pub fn initialized_world(seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(initialized_profile(), [1; 16], player(), seed).unwrap()
}
#[derive(Default)]
pub struct KernelAuthority {
    pub base: SyntheticAuthority,
    pub rear: bool,
    pub stale_action: bool,
}
impl Authority for KernelAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        if let Query::NativeMelee { .. } = &request.query {
            let mut echo = request.clone();
            if self.stale_action {
                if let Query::NativeMelee { action, .. } = &mut echo.query {
                    *action += 1;
                }
            }
            Ok(AuthorityReply {
                request: echo,
                facts: Facts::NativeMelee(MeleeFacts {
                    front_facing: !self.rear,
                    player_can_attack_npc: true,
                    player_can_enter_combat: true,
                    npc_can_enter_combat: true,
                }),
            })
        } else {
            self.base.resolve(request)
        }
    }
}
