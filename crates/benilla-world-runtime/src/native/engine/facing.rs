// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1: WorldObject::GetAngle/HasInArc, Unit melee and
// Spell::CheckRange. Upstream GPL-2.0-or-later, Copyright CMaNGOS contributors.
use super::*;
use benilla_sim::{creature_motion as motion, facing};

impl<A: Authority> Frame<'_, A> {
    pub(super) fn facing_facts(
        &self,
        request: &AuthorityRequest,
    ) -> Result<Option<Facts>, NativeError> {
        if !self.profile.source_facing_authority() {
            return Ok(None);
        }
        Ok(Some(match &request.query {
            Query::SourceMotion(MotionQuery::Spatial(motion::Query::Bearing(query))) => {
                // UpdateSplinePosition deliberately supplies its OLD map origin.
                // Replacing it with the new spline pose changes source semantics.
                Facts::SourceMotion(MotionFacts::Spatial(Box::new(motion::Reply {
                    request: motion::Query::Bearing(query.clone()),
                    // Preserve Real's signed-zero canonicalization and refusal of
                    // unsupported subnormal output rather than silently rounding.
                    fact: motion::Fact::Bearing(bearing(query.origin, query.toward)?),
                })))
            }
            Query::SourceMotion(MotionQuery::Combat(CombatQuery::MeleeFront(query))) => {
                // CanDodgeInCombat -> attacker.IsFacingTargetsFront(victim):
                // victim arc first, then attacker arc, with source short circuit.
                Facts::SourceMotion(MotionFacts::Combat(CombatFacts::MeleeFront(mutual_front(
                    query.player_pose,
                    query.creature_pose,
                )?)))
            }
            Query::SourceSpell(SourceSpellQuery::Facing(query)) => {
                Facts::SourceSpell(SourceSpellFacts::Facing(front(
                    query.player_position,
                    query.player_orientation.bits(),
                    query.target_position,
                )?))
            }
            _ => return Ok(None),
        }))
    }
}
fn xy(point: ai::Point) -> [u32; 2] {
    [point.x.bits(), point.y.bits()]
}
fn front(
    origin: ai::Point,
    orientation: u32,
    toward: ai::Point,
) -> Result<ai::movement::Check, NativeError> {
    // These are distinct player/NPC objects even when their coordinates coincide.
    let passed = facing::has_in_arc_bits(
        xy(origin),
        orientation,
        xy(toward),
        0x4049_0fdb, // binary32 pi, source HasInArc(M_PI_F, target)
        false,
    )?;
    Ok(if passed {
        ai::movement::Check::Passed
    } else {
        ai::movement::Check::Failed
    })
}

fn bearing(origin: ai::Point, toward: ai::Point) -> Result<ai::Real, NativeError> {
    Ok(ai::Real::from_bits(facing::bearing_bits(
        xy(origin),
        xy(toward),
    )?)?)
}
fn mutual_front(
    victim: motion::Pose,
    attacker: motion::Pose,
) -> Result<ai::movement::Check, NativeError> {
    let first = front(
        victim.position,
        victim.orientation.bits(),
        attacker.position,
    )?;
    if first == ai::movement::Check::Passed {
        front(
            attacker.position,
            attacker.orientation.bits(),
            victim.position,
        )
    } else {
        Ok(first)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::synthetic_fixture as f;
    use ai::movement::Check;

    #[test]
    fn melee_requires_both_arcs_and_preserves_victim_first_short_circuit() {
        let victim = motion::Pose {
            position: f::point(0.0),
            orientation: ai::Real::ZERO,
        };
        let mut attacker = motion::Pose {
            position: f::point(2.0),
            orientation: ai::Real::ZERO,
        };
        assert_eq!(
            front(
                victim.position,
                victim.orientation.bits(),
                attacker.position
            )
            .unwrap(),
            Check::Passed
        );
        assert_eq!(mutual_front(victim, attacker).unwrap(), Check::Failed);
        attacker.orientation = ai::Real::from_bits(0x4049_0fdb).unwrap();
        assert_eq!(mutual_front(victim, attacker).unwrap(), Check::Passed);
        let away = motion::Pose {
            orientation: attacker.orientation,
            ..victim
        };
        attacker.orientation = f::real(30.0); // Outside numeric domain, unreachable in a valid pose.
        assert_eq!(mutual_front(away, attacker).unwrap(), Check::Failed);
        assert!(mutual_front(victim, attacker).is_err());
    }

    #[test]
    fn coincident_distinct_objects_do_not_get_the_self_target_shortcut() {
        let position = f::point(0.0);
        assert_eq!(
            front(position, 0x4049_0fdb, position).unwrap(),
            Check::Failed
        );
        assert_eq!(front(position, 0, position).unwrap(), Check::Passed);
    }

    #[test]
    fn raw_subnormal_bearing_is_refused_by_native_real_without_rounding_or_clamping() {
        let origin = f::point(0.0);
        let toward = ai::Point::new(32760.0, f32::from_bits(0x0080_0000), 0.0).unwrap();
        let raw = facing::bearing_bits(xy(origin), xy(toward)).unwrap();
        assert_ne!(raw, 0);
        assert_eq!(raw & 0x7f80_0000, 0);
        assert!(matches!(
            bearing(origin, toward),
            Err(NativeError::Navigation(_))
        ));
        assert_eq!(bearing(origin, f::point(2.0)).unwrap(), ai::Real::ZERO);
    }

    #[test]
    fn staged_frame_refuses_subnormal_bearing_without_committing_its_draft() {
        // This isolates the same staging/ask boundary used by step_batch. Normal
        // source distance arithmetic may reject this tiny Y displacement before
        // a live AI callback reaches Bearing; this is not a reachable-AI claim.
        #[derive(Default)]
        struct NoExternal {
            observed: bool,
        }
        impl Authority for NoExternal {
            fn resolve(&mut self, _: &AuthorityRequest) -> Result<AuthorityReply, NativeError> {
                panic!("selected Bearing escaped the executing frame")
            }
            fn observe_derived(&mut self, _: &AuthorityReply) -> Result<(), NativeError> {
                self.observed = true;
                Ok(())
            }
        }
        let mut profile = crate::native::synthetic_combat_fixture::profile();
        profile.player.start = ai::Point::new(32760.0, f32::from_bits(0x0080_0000), 0.0).unwrap();
        profile.npcs[0].ai.spawn = f::point(0.0);
        profile.initialize_facing_authority().unwrap();
        let mut world =
            NativeWorld::synthetic_component_test(profile, [8; 16], f::player(), 7).unwrap();
        let before = world.checkpoint().unwrap();
        let mut authority = NoExternal::default();
        let result = (|| -> Result<(), NativeError> {
            let mut next = world.state.clone();
            next.tick += 1;
            next.now_ms += u64::from(world.profile.tick_ms);
            next.next_sequence += 1;
            next.rng_counter += 1;
            let origin = next.npcs[&2].ai.position();
            let toward = next.position;
            let npc = next.npcs[&2].id;
            let query = motion::Query::Bearing(motion::BearingRequest {
                binding: motion::QueryBinding {
                    identity: motion::Identity {
                        creature: npc.spawn,
                        map: world.profile.map_id,
                        content: ai::movement::Revision(world.profile.content_digest),
                        geometry: ai::movement::Revision(world.profile.geometry_revision),
                    },
                    query_id: 1,
                    now_ms: next.now_ms,
                },
                origin,
                toward,
            });
            {
                let mut frame = Frame {
                    read_only: false,
                    profile: &world.profile,
                    profile_digest: world.profile_digest,
                    state: &mut next,
                    authority: &mut authority,
                    replies: vec![],
                    events: vec![],
                    draws: vec![],
                    receipts: BTreeMap::new(),
                    source_alive_unit: None,
                    moved: false,
                    cast_inputs: BTreeMap::new(),
                    movement_origin: world.state.position,
                    movement_path: vec![],
                    player_auras_updated: false,
                    player_melee_unit_updated: false,
                    pending_player_melee_death: None,
                };
                frame.command(&Command::MovementPose {
                    destination: toward,
                    orientation: Orientation::new(0.25)?,
                    movement_flags: motion::GroundMovementFlags::STOPPED,
                })?;
                assert_ne!(frame.state.orientation, world.state.orientation);
                frame.ask(Some(npc), Query::SourceMotion(MotionQuery::Spatial(query)))?;
            }
            // The fallible numeric conversion above must prevent publication.
            world.state = next;
            Ok(())
        })();
        assert_eq!(
            result,
            Err(NativeError::Navigation(ai::AiError::Invalid(
                "finite normal binary32"
            )))
        );
        assert!(!authority.observed);
        assert_eq!(world.checkpoint().unwrap(), before);
        assert!(world.records().is_empty());
    }
}
