// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored solo motion fixtures only. Host trig and assumed spatial witnesses
//! below are deliberately synthetic and cannot establish native-world admission.
use super::{synthetic_fixture as f, *};
use benilla_sim::creature_motion as m;
pub fn genesis(p: &Profile) -> MotionGenesis {
    let n = &p.npcs[0];
    MotionGenesis {
        spell_hits: p
            .abilities
            .iter()
            .filter_map(|a| match &a.admission {
                AbilityAdmission::Available(spec) => Some((
                    a.id,
                    SpellHitPolicy {
                        attributes: [0; 5],
                        implicit_target_a: std::array::from_fn(|i| {
                            if matches!(spec.effects[i], spell::MageEffect::Empty) {
                                0
                            } else {
                                6
                            }
                        }),
                        implicit_target_b: [0; 3],
                        aura_interrupt_flags: 0,
                    },
                )),
                _ => None,
            })
            .collect(),
        player_can_enter_combat: true,
        npc_can_enter_combat: true,
        npc_stand_state: 0,
        npc_policy: MotionPolicy {
            walk_speed: f::real(1.25),
            run_speed: n.ai.run_speed,
            settings: m::GroundSettings {
                combat_reach: n.ai.combat_reach,
                bounding_radius: n.ai.bounding_radius,
                collision_height: f::real(2.0),
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
                addon_effects: m::Evidence::Absent,
                scripted_home_events: m::Evidence::Absent,
                temporary_summon_callback: m::Evidence::Absent,
            },
        },
        player_collision_height: f::real(2.0),
        player_movement_flags: m::GroundMovementFlags::STOPPED,
        npc_spawn_orientation: Orientation::new(1.0).unwrap(),
        npc_initial_active: false,
        chase_walking: false,
        home_walking: false,
        creature_initial_aggro_delay_ms: 200,
        relocation_aggro_delay_ms: 100,
        creature_detection_range: n.ai.detection.as_f32() as u32,
    }
}
pub fn profile() -> Profile {
    let mut p = f::profile();
    p.npcs[0].ai.reaction = ai::Reaction::Aggressive;
    p.source_motion = Some(genesis(&p));
    p
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], f::player(), 1234).unwrap()
}
/// Authored source rows only. No data lookup, default-row inference or production
/// completeness/admission claim is made by this test fixture.
pub fn creature_capability_sources(
    p: &Profile,
) -> CreatureCapabilityRows<CreatureCapabilitySource> {
    CreatureCapabilityRows::Complete(
        p.npcs
            .iter()
            .map(|npc| CreatureCapabilitySource {
                spawn_id: npc.ai.creature_id,
                spawn_entry: npc.entry,
                requested_spawn_template_entry: 0,
                templates: CreatureCapabilityRows::Complete(vec![
                    CreatureCapabilityTemplateSource {
                        entry: npc.entry,
                        inhabit_type: 1,
                    },
                ]),
                spawn_templates: CreatureCapabilityRows::Complete(vec![
                    CreatureCapabilitySpawnTemplateSource {
                        entry: 0,
                        spawn_flags: 1,
                        relay_id: 0,
                        string_id: 0,
                    },
                ]),
                external_features: CreatureCapabilityRows::Complete(vec![]),
            })
            .collect(),
    )
}
fn synthetic_near_point(q: &m::NearPointRequest) -> Result<m::Fact, NativeError> {
    let angle = match q.angle {
        m::NearPointAngle::Absolute(a) => a.as_f32(),
        m::NearPointAngle::CurrentTargetToOwner => (q.owner.position.y.as_f32()
            - q.target.pose.position.y.as_f32())
        .atan2(q.owner.position.x.as_f32() - q.target.pose.position.x.as_f32()),
    };
    Ok(m::Fact::NearPoint(ai::Point::new(
        q.target.pose.position.x.as_f32() + q.distance.as_f32() * angle.cos(),
        q.target.pose.position.y.as_f32() + q.distance.as_f32() * angle.sin(),
        q.target.pose.position.z.as_f32(),
    )?))
}
#[derive(Default)]
pub struct SyntheticMotionAuthority {
    pub legacy: f::SyntheticAuthority,
    pub critical_chance: Option<u16>,
    pub resist_chance: Option<u16>,
    pub hostile_visibility: Option<ai::movement::Check>,
    pub hostile_relation: Option<ai::movement::Check>,
    pub missing: bool,
    pub truncated: bool,
    pub visits: Option<bool>,
    pub queries: Vec<AuthorityRequest>,
}
impl Authority for SyntheticMotionAuthority {
    fn resolve(&mut self, request: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
        self.queries.push(request.clone());
        let Query::SourceMotion(query) = &request.query else {
            let mut reply = self.legacy.resolve(request)?;
            if let (Some(chance), Facts::Spell(facts)) = (self.critical_chance, &mut reply.facts) {
                facts.critical_chance = chance;
            }
            if let (Some(chance), Facts::Spell(facts)) = (self.resist_chance, &mut reply.facts) {
                facts.resist_chance = chance;
            }
            return Ok(reply);
        };
        if self.missing {
            return Err(NativeError::AuthorityUnavailable(
                "synthetic source motion failure",
            ));
        }
        let facts = match query {
            MotionQuery::NearPoint(query) => {
                if query.searcher.allowed_position_z_branch()? != AllowedPositionZBranch::Ground {
                    return Err(NativeError::UnsupportedMechanic(
                        "synthetic near-point capability branch",
                    ));
                }
                MotionFacts::Spatial(Box::new(m::Reply {
                    request: m::Query::NearPoint(query.request.clone()),
                    fact: synthetic_near_point(&query.request)?,
                }))
            }
            MotionQuery::Combat(query) => MotionFacts::Combat(match query {
                CombatQuery::NpcVisibility(_) => {
                    CombatFacts::NpcVisibility(ai::movement::Check::Passed)
                }
                CombatQuery::NpcAccessibility(_) => {
                    CombatFacts::NpcAccessibility(ai::movement::Check::Passed)
                }
                CombatQuery::NpcLineOfSight(_) => {
                    CombatFacts::NpcLineOfSight(ai::movement::Check::Passed)
                }
                CombatQuery::MeleeFront(_) => CombatFacts::MeleeFront(ai::movement::Check::Passed),
                CombatQuery::NpcSemantics(_)
                | CombatQuery::MeleeDamage(_)
                | CombatQuery::MeleeCombatEntry(_) => {
                    return Err(NativeError::AuthorityUnavailable(
                        "runtime-owned combat semantics escaped",
                    ))
                }
            }),
            MotionQuery::HostileSpellRelation(_) => MotionFacts::HostileSpellRelation(
                self.hostile_relation.unwrap_or(ai::movement::Check::Passed),
            ),
            MotionQuery::HostileSpellVisibility(_) => MotionFacts::HostileSpellVisibility(
                self.hostile_visibility
                    .unwrap_or(ai::movement::Check::Passed),
            ),
            MotionQuery::Visitor(_) => MotionFacts::Visitor(VisitorFacts::Complete {
                loaded_membership: ai::movement::Check::Passed,
                call_move_in_line_of_sight: self.visits.unwrap_or(true),
            }),
            MotionQuery::Spatial(query) => {
                let fact = match query {
                    m::Query::NearPoint(q) => synthetic_near_point(q)?,
                    m::Query::Bearing(q) => {
                        let a = (q.toward.y.as_f32() - q.origin.y.as_f32())
                            .atan2(q.toward.x.as_f32() - q.origin.x.as_f32());
                        m::Fact::Bearing(f::real(if a < 0.0 {
                            a + std::f32::consts::TAU
                        } else {
                            a
                        }))
                    }
                    m::Query::Collider(_) => m::Fact::Collider(None),
                    m::Query::FacingTarget(_) => m::Fact::ObjectPosition(None),
                    m::Query::Route(q) => m::Fact::Route(if self.truncated {
                        m::RouteOutcome::Truncated
                    } else {
                        m::RouteOutcome::Complete(m::CompleteRoute {
                            source: m::RouteSource::SyntheticFixture,
                            path_identity: [7; 32],
                            points: vec![q.origin, q.goal],
                            segments: vec![ai::movement::SegmentChecks {
                                loaded_coverage: ai::movement::Check::Passed,
                                line_of_sight: ai::movement::Check::Passed,
                                body_sweep: ai::movement::Check::Passed,
                                continuous_ground_support: ai::movement::Check::Passed,
                                dry_ground: ai::movement::Check::Passed,
                            }],
                        })
                    }),
                };
                MotionFacts::Spatial(Box::new(m::Reply {
                    request: query.clone(),
                    fact,
                }))
            }
        };
        Ok(AuthorityReply {
            request: request.clone(),
            facts: Facts::SourceMotion(facts),
        })
    }
}
