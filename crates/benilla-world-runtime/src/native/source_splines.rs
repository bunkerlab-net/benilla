// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned MoveSplineInit.cpp's thread-local counter, for one fresh isolated
//! logical source thread. Snapshot endpoints prove a reachable interleaving;
//! actual callback order is established by normal TickRecord replay.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SplineAllocation {
    pub owner: u64,
    pub ordinal: u64,
    pub wire_id: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Endpoint {
    owner: u64,
    ordinal: u64,
    last: u32,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlayerSplineStop {
    pub allocation: SplineAllocation,
    pub tick: u64,
    pub position: ai::Point,
    pub orientation: Orientation,
    pub before_flags: benilla_sim::creature_motion::GroundMovementFlags,
    pub after_flags: benilla_sim::creature_motion::GroundMovementFlags,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceSplineIds {
    // Sorted vector avoids silently accepting duplicate JSON map keys.
    owners: Vec<Endpoint>,
    // MAX+1 is an exhausted sentinel. Wrapping source counters are unsupported.
    next: u64,
    #[serde(deserialize_with = "creature_collision_width::required_option")]
    stop: Option<PlayerSplineStop>,
}
pub(super) fn enabled(profile: &Profile) -> bool {
    profile.source_motion.is_some()
        && profile.combat.as_ref().is_some_and(|g| g.death.is_some())
        && profile.has_source_spell_authority()
        && profile.player_slow.is_none()
}
impl SourceSplineIds {
    #[cfg(any(test, feature = "synthetic-native-tests"))]
    pub(super) fn new(profile: &Profile) -> Option<Self> {
        enabled(profile).then(|| Self {
            owners: std::iter::once(SOLO_UNIT)
                .chain(profile.npcs.iter().map(|n| n.ai.creature_id))
                .map(|owner| Endpoint {
                    owner,
                    ordinal: 0,
                    last: 0,
                })
                .collect(),
            next: 1,
            stop: None,
        })
    }
    pub fn next_wire_id(&self) -> u64 {
        self.next
    }
    pub fn player_stop(&self) -> Option<&PlayerSplineStop> {
        self.stop.as_ref()
    }
    pub fn wire_id(&self, owner: u64, ordinal: u64) -> Result<u32, NativeError> {
        let endpoint = self.endpoint(owner)?;
        if ordinal == 0 || endpoint.ordinal != ordinal {
            return Err(NativeError::InvalidState);
        }
        Ok(endpoint.last)
    }
    fn endpoint(&self, owner: u64) -> Result<&Endpoint, NativeError> {
        let index = self
            .owners
            .binary_search_by_key(&owner, |e| e.owner)
            .map_err(|_| NativeError::InvalidState)?;
        Ok(&self.owners[index])
    }
    pub(super) fn allocate(
        &mut self,
        owner: u64,
        ordinal: u64,
    ) -> Result<SplineAllocation, NativeError> {
        let index = self
            .owners
            .binary_search_by_key(&owner, |e| e.owner)
            .map_err(|_| NativeError::InvalidState)?;
        if self.owners[index].ordinal.checked_add(1) != Some(ordinal) {
            return Err(NativeError::InvalidState);
        }
        let wire_id = u32::try_from(self.next).map_err(|_| NativeError::Limit)?;
        if wire_id == 0 {
            return Err(NativeError::InvalidState);
        }
        self.owners[index].ordinal = ordinal;
        self.owners[index].last = wire_id;
        self.next += 1;
        Ok(SplineAllocation {
            owner,
            ordinal,
            wire_id,
        })
    }
    /// Projection replays only already emitted allocations on its private frame.
    pub fn observe(&mut self, allocation: SplineAllocation) -> Result<(), NativeError> {
        // Validate every fallible operand before mutation. This keeps the
        // public observer atomic without cloning all fixed owners per event.
        if self.endpoint(allocation.owner)?.ordinal.checked_add(1) != Some(allocation.ordinal)
            || u64::from(allocation.wire_id) != self.next
            || allocation.wire_id == 0
        {
            return Err(NativeError::InvalidState);
        }
        self.allocate(allocation.owner, allocation.ordinal)?;
        Ok(())
    }
    pub fn observe_player_stop(&mut self, stop: &PlayerSplineStop) -> Result<(), NativeError> {
        if self.stop.is_some() || stop.allocation.owner != SOLO_UNIT || stop.allocation.ordinal != 1
        {
            return Err(NativeError::InvalidState);
        }
        self.observe(stop.allocation)?;
        self.stop = Some(stop.clone());
        Ok(())
    }
    pub(super) fn stop_player(state: &mut State) -> Result<Option<PlayerSplineStop>, NativeError> {
        let Some(ids) = &mut state.source_splines else {
            return Ok(None);
        };
        if ids.stop.is_some() {
            return Err(NativeError::InvalidState);
        }
        let before_flags = state
            .player_movement_flags
            .ok_or(NativeError::InvalidState)?;
        let after_flags =
            benilla_sim::creature_motion::GroundMovementFlags::from_bits(before_flags.bits() & !1)?;
        let stop = PlayerSplineStop {
            allocation: ids.allocate(SOLO_UNIT, 1)?,
            tick: state.tick,
            position: state.position,
            orientation: state.orientation,
            before_flags,
            after_flags,
        };
        ids.stop = Some(stop.clone());
        state.player_movement_flags = Some(after_flags);
        Ok(Some(stop))
    }
    fn reachable(&self) -> Result<(), NativeError> {
        if self.owners.is_empty()
            || self.owners.len() > 4097
            || self.owners.windows(2).any(|p| p[0].owner >= p[1].owner)
        {
            return Err(NativeError::InvalidState);
        }
        let mut ends = Vec::new();
        let mut total = 0u64;
        for e in &self.owners {
            if (e.ordinal == 0) != (e.last == 0) {
                return Err(NativeError::InvalidState);
            }
            total = total.checked_add(e.ordinal).ok_or(NativeError::Limit)?;
            if e.ordinal != 0 {
                ends.push((e.last, e.ordinal));
            }
        }
        if total > u64::from(u32::MAX) || self.next != total + 1 {
            return Err(NativeError::InvalidState);
        }
        ends.sort_unstable();
        let mut prefix = 0u64;
        let mut prior = 0;
        for (last, count) in ends {
            prefix = prefix.checked_add(count).ok_or(NativeError::Limit)?;
            if last == prior || u64::from(last) > total || prefix > u64::from(last) {
                return Err(NativeError::InvalidState);
            }
            prior = last;
        }
        Ok(())
    }
    /// Check published callback order too: some source AI callbacks are parked
    /// until their owning MainhandSwing is emitted. Future reorderings must fail,
    /// never silently renumber an already allocated source spline.
    pub(super) fn validate_events(
        before: &State,
        after: &State,
        events: &[Event],
    ) -> Result<(), NativeError> {
        let mut ids = before.source_splines.clone();
        for event in events {
            match event {
                Event::NpcMotion {
                    id,
                    event,
                    allocation,
                } => {
                    let ordinal = match event {
                        benilla_sim::creature_motion::Event::SplineLaunched {
                            spline_id, ..
                        }
                        | benilla_sim::creature_motion::Event::SplineInterrupted {
                            spline_id,
                            ..
                        } => Some(*spline_id),
                        _ => None,
                    };
                    match (&mut ids, ordinal, allocation) {
                        (Some(ids), Some(ordinal), Some(a))
                            if a.owner == id.spawn && a.ordinal == ordinal =>
                        {
                            ids.observe(*a)?
                        }
                        (Some(_), None, None) | (None, _, None) => (),
                        _ => return Err(NativeError::InvalidState),
                    }
                }
                Event::PlayerSplineStopped(stop) => ids
                    .as_mut()
                    .ok_or(NativeError::InvalidState)?
                    .observe_player_stop(stop)?,
                _ => (),
            }
        }
        if ids != after.source_splines {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }
    pub(super) fn validate(profile: &Profile, state: &State) -> Result<(), NativeError> {
        let Some(ids) = &state.source_splines else {
            return if enabled(profile) {
                Err(NativeError::InvalidState)
            } else {
                Ok(())
            };
        };
        if !enabled(profile) {
            return Err(NativeError::InvalidState);
        }
        ids.reachable()?;
        if ids
            .owners
            .iter()
            .map(|e| e.owner)
            .ne(std::iter::once(SOLO_UNIT).chain(profile.npcs.iter().map(|n| n.ai.creature_id)))
        {
            return Err(NativeError::InvalidState);
        }
        for npc in state.npcs.values() {
            let count = npc
                .source_motion
                .as_ref()
                .ok_or(NativeError::InvalidState)?
                .motion
                .snapshot()
                .spline_id;
            if ids.endpoint(npc.id.spawn)?.ordinal != count {
                return Err(NativeError::InvalidState);
            }
        }
        let death = state
            .combat
            .as_ref()
            .and_then(|c| c.death.as_ref())
            .ok_or(NativeError::InvalidState)?
            .snapshot();
        let dead = death.phase != benilla_sim::player_death::DeathPhase::Alive;
        if ids.endpoint(SOLO_UNIT)?.ordinal != u64::from(dead) || ids.stop.is_some() != dead {
            return Err(NativeError::InvalidState);
        }
        if let Some(stop) = &ids.stop {
            if stop.allocation.owner != SOLO_UNIT
                || stop.allocation.ordinal != 1
                || ids.wire_id(SOLO_UNIT, 1)? != stop.allocation.wire_id
                || Some(stop.tick) != death.died_at_tick
                || stop.position != state.position
                || stop.orientation != state.orientation
                || stop.after_flags.bits() != (stop.before_flags.bits() & !1)
                || state.player_movement_flags != Some(stop.after_flags)
            {
                return Err(NativeError::InvalidState);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::native::synthetic_source_spline_fixture::CompleteAuthority;
    fn ids(counts: &[(u64, u32)]) -> SourceSplineIds {
        SourceSplineIds {
            owners: counts
                .iter()
                .enumerate()
                .map(|(i, (ordinal, last))| Endpoint {
                    owner: i as u64 + 1,
                    ordinal: *ordinal,
                    last: *last,
                })
                .collect(),
            next: counts.iter().map(|(n, _)| *n).sum::<u64>() + 1,
            stop: None,
        }
    }
    #[test]
    fn endpoint_prefix_proof_rejects_impossible_interleavings() {
        for good in [
            vec![(0, 0), (0, 0)],
            vec![(2, 3), (1, 2)],
            vec![(2, 2), (1, 3)],
            vec![(1, 1), (2, 3)],
        ] {
            ids(&good).reachable().unwrap();
        }
        for bad in [
            vec![(2, 1), (1, 3)],
            vec![(1, 2), (1, 2)],
            vec![(0, 1)],
            vec![(1, 0)],
            vec![(1, 3)],
        ] {
            assert!(ids(&bad).reachable().is_err(), "{bad:?}");
        }
        let mut bad = ids(&[(1, 1), (1, 2)]);
        bad.owners[1].owner = 1;
        assert!(bad.reachable().is_err());
    }
    #[test]
    fn allocation_is_ordered_atomic_and_exhaustion_never_wraps() {
        let mut value = ids(&[(0, 0), (0, 0)]);
        assert_eq!(value.allocate(2, 1).unwrap().wire_id, 1);
        assert_eq!(value.allocate(1, 1).unwrap().wire_id, 2);
        assert_eq!(value.allocate(2, 2).unwrap().wire_id, 3);
        value.reachable().unwrap();
        let before = value.clone();
        for a in [
            SplineAllocation {
                owner: 3,
                ordinal: 1,
                wire_id: 4,
            },
            SplineAllocation {
                owner: 2,
                ordinal: 4,
                wire_id: 4,
            },
            SplineAllocation {
                owner: 2,
                ordinal: 3,
                wire_id: 5,
            },
        ] {
            assert!(value.observe(a).is_err());
            assert_eq!(value, before);
        }
        let mut last = ids(&[(u64::from(u32::MAX) - 1, u32::MAX - 1)]);
        assert_eq!(
            last.allocate(1, u64::from(u32::MAX)).unwrap().wire_id,
            u32::MAX
        );
        last.reachable().unwrap();
        let before = last.clone();
        assert!(matches!(
            last.allocate(1, u64::from(u32::MAX) + 1),
            Err(NativeError::Limit)
        ));
        assert_eq!(last, before);
    }
    #[test]
    fn publication_order_check_rejects_parked_callback_reordering() {
        // This tests the private publication-order checker, not a restorable
        // world: complete-boundary local-motion validation is separate.
        let world = synthetic_source_spline_fixture::world();
        let before = world.state.clone();
        let mut after = before.clone();
        let mut events = Vec::new();
        for (owner, ordinal) in [(2, 1), (3, 1), (2, 2)] {
            let allocation = after
                .source_splines
                .as_mut()
                .unwrap()
                .allocate(owner, ordinal)
                .unwrap();
            events.push(Event::NpcMotion {
                id: before.npcs[&owner].id,
                event: benilla_sim::creature_motion::Event::SplineInterrupted {
                    pose: benilla_sim::creature_motion::Pose {
                        position: before.position,
                        orientation: ai::Real::ZERO,
                    },
                    spline_id: ordinal,
                },
                allocation: Some(allocation),
            });
        }
        SourceSplineIds::validate_events(&before, &after, &events).unwrap();
        events.swap(0, 1);
        assert!(SourceSplineIds::validate_events(&before, &after, &events).is_err());
        events.swap(0, 1);
        events.pop();
        assert!(SourceSplineIds::validate_events(&before, &after, &events).is_err());
    }
    fn reject_forged(world: &NativeWorld, value: serde_json::Value) {
        let mut checkpoint = world.checkpoint().unwrap();
        checkpoint.state = serde_json::from_value(value).unwrap();
        checkpoint.digest = crate::digest(b"benilla-native-state-v81\0", &checkpoint.state);
        checkpoint.checksum = crate::digest(
            b"benilla-native-checkpoint-v81\0",
            &(&checkpoint.state, checkpoint.digest, checkpoint.history_tip),
        );
        assert!(
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), checkpoint)
                .is_err()
        );
    }
    #[test]
    fn required_fixed_owner_state_rejects_missing_extra_and_foreign_allocations() {
        let world = synthetic_source_spline_fixture::world();
        let original = serde_json::to_value(world.checkpoint().unwrap().state).unwrap();
        for mutation in 0..5 {
            let mut value = original.clone();
            let ids = &mut value["source_splines"];
            match mutation {
                0 => *ids = serde_json::Value::Null,
                1 => {
                    ids["owners"].as_array_mut().unwrap().pop();
                }
                2 => ids["owners"][1]["owner"] = serde_json::json!(99),
                3 => {
                    ids["owners"][0]["ordinal"] = serde_json::json!(1);
                    ids["owners"][0]["last"] = serde_json::json!(1);
                    ids["next"] = serde_json::json!(2);
                }
                4 => {
                    ids["owners"][1]["ordinal"] = serde_json::json!(1);
                    ids["owners"][1]["last"] = serde_json::json!(1);
                    ids["next"] = serde_json::json!(2);
                }
                _ => unreachable!(),
            }
            reject_forged(&world, value);
        }
        let mut no_stop = original.clone();
        no_stop["source_splines"]
            .as_object_mut()
            .unwrap()
            .remove("stop");
        assert!(serde_json::from_value::<persistence::StateCheckpoint>(no_stop).is_err());
        let mut missing = original;
        missing.as_object_mut().unwrap().remove("source_splines");
        assert!(serde_json::from_value::<persistence::StateCheckpoint>(missing).is_err());
        assert!(synthetic_fixture::world().state.source_splines.is_none());
    }
    #[test]
    fn real_death_allocates_shared_stop_preserves_ground_bits_and_replays() {
        let mut world = synthetic_source_spline_fixture::world();
        assert!(world.state.source_splines.is_some());
        let initial = world.checkpoint().unwrap();
        let mut authority = CompleteAuthority::default();
        let flags = benilla_sim::creature_motion::GroundMovementFlags::from_bits(0x13f).unwrap();
        let target = world.state.npcs[&2].id;
        let movement = synthetic_quest_fixture::command(
            &world,
            Command::MovementPose {
                destination: world.state.position,
                orientation: world.state.orientation,
                movement_flags: flags,
            },
        );
        let mut attack = synthetic_quest_fixture::command(&world, Command::AttackStart { target });
        attack.input.sequence += 1;
        world
            .step_batch(1, vec![movement, attack], &mut authority)
            .unwrap();
        for _ in 0..300 {
            if !world.state.player_alive() {
                break;
            }
            world
                .step_batch(world.state.tick + 1, vec![], &mut authority)
                .unwrap();
        }
        assert!(!world.state.player_alive());
        let ids = world.state.source_splines.as_ref().unwrap();
        let stop = ids.player_stop().unwrap().clone();
        assert_eq!(stop.before_flags, flags);
        assert_eq!(stop.after_flags.bits(), 0x13e);
        assert_eq!(stop.position, world.state.position);
        let allocations = world
            .records()
            .iter()
            .flat_map(|r| &r.events)
            .filter_map(|e| match e {
                Event::NpcMotion { allocation, .. } => *allocation,
                Event::PlayerSplineStopped(s) => Some(s.allocation),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(!allocations.is_empty());
        assert!(allocations.iter().any(|a| a.owner == SOLO_UNIT));
        assert_eq!(
            allocations.iter().map(|a| a.wire_id).collect::<Vec<_>>(),
            (1..=allocations.len() as u32).collect::<Vec<_>>()
        );
        let saved = serde_json::to_value(world.checkpoint().unwrap().state).unwrap();
        for mutation in 0..4 {
            let mut value = saved.clone();
            let stop = &mut value["source_splines"]["stop"];
            match mutation {
                0 => *stop = serde_json::Value::Null,
                1 => stop["tick"] = serde_json::json!(0),
                2 => stop["after_flags"] = serde_json::json!(0),
                3 => stop["allocation"]["wire_id"] = serde_json::json!(0),
                _ => unreachable!(),
            }
            reject_forged(&world, value);
        }
        let denied = synthetic_quest_fixture::command(
            &world,
            Command::MovementPose {
                destination: synthetic_fixture::point(1.0),
                orientation: Orientation::new(1.0).unwrap(),
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            },
        );
        world
            .step(world.state.tick + 1, Some(denied), &mut authority)
            .unwrap();
        assert_eq!(
            world.state.source_splines.as_ref().unwrap().player_stop(),
            Some(&stop)
        );
        assert_eq!(world.state.position, stop.position);
        assert_eq!(world.state.player_movement_flags, Some(stop.after_flags));
        let profile = world.profile.clone();
        let mut replay =
            NativeWorld::restore_synthetic_component_test(profile.clone(), initial).unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.state_digest(), world.state_digest());
        let restored =
            NativeWorld::restore_synthetic_component_test(profile, world.checkpoint().unwrap())
                .unwrap();
        assert_eq!(restored.state.source_splines, world.state.source_splines);
        assert_eq!(restored.state_digest(), world.state_digest());
    }
    #[test]
    fn composed_two_owner_chase_death_respawn_retains_shared_endpoints() {
        let mut world = synthetic_source_spline_fixture::surviving_world();
        let initial = world.checkpoint().unwrap();
        let mut authority = CompleteAuthority::default();
        let target = world.state.npcs[&2].id;
        let victim = world.state.npcs[&3].id;
        let cast = synthetic_quest_fixture::command(
            &world,
            Command::Cast {
                ability: 133,
                target: victim,
            },
        );
        world.step(1, Some(cast), &mut authority).unwrap();
        for _ in 0..100 {
            if world.state.npcs[&3].health == 0 {
                break;
            }
            world
                .step(world.state.tick + 1, None, &mut authority)
                .unwrap();
        }
        assert_eq!(
            world.state.npcs[&3].health, 0,
            "actual Fireball must kill selected source NPC"
        );
        let ordinal = world
            .state
            .source_splines
            .as_ref()
            .unwrap()
            .endpoint(3)
            .unwrap()
            .ordinal;
        // Only now engage the other NPC: this raw spell correctly interrupts
        // on entering combat, so attacking first would test a canceled cast.
        let movement = synthetic_quest_fixture::command(
            &world,
            Command::MovementPose {
                destination: synthetic_fixture::point(8.0),
                orientation: Orientation::new(std::f32::consts::PI).unwrap(),
                movement_flags: benilla_sim::creature_motion::GroundMovementFlags::STOPPED,
            },
        );
        world
            .step(world.state.tick + 1, Some(movement), &mut authority)
            .unwrap();
        let hit = synthetic_quest_fixture::command(
            &world,
            Command::Cast {
                ability: 133,
                target,
            },
        );
        world
            .step(world.state.tick + 1, Some(hit), &mut authority)
            .unwrap();
        for _ in 0..150 {
            if world.state.npcs[&3].id.generation > victim.generation {
                break;
            }
            world
                .step(world.state.tick + 1, None, &mut authority)
                .unwrap();
        }
        assert!(ordinal > 0);
        assert!(world.state.npcs[&3].id.generation > victim.generation);
        assert_eq!(
            world
                .state
                .source_splines
                .as_ref()
                .unwrap()
                .endpoint(3)
                .unwrap()
                .ordinal,
            ordinal
        );
        let allocations = world
            .records()
            .iter()
            .flat_map(|r| &r.events)
            .filter_map(|e| match e {
                Event::NpcMotion { allocation, .. } => *allocation,
                Event::PlayerSplineStopped(s) => Some(s.allocation),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(allocations.iter().any(|a| a.owner == 2));
        assert!(allocations.iter().any(|a| a.owner == 3));
        assert_eq!(
            allocations.iter().map(|a| a.wire_id).collect::<Vec<_>>(),
            (1..=allocations.len() as u32).collect::<Vec<_>>()
        );
        let mut replay =
            NativeWorld::restore_synthetic_component_test(world.profile.clone(), initial).unwrap();
        replay.replay(world.records()).unwrap();
        assert_eq!(replay.state_digest(), world.state_digest());
        NativeWorld::restore_synthetic_component_test(
            world.profile.clone(),
            world.checkpoint().unwrap(),
        )
        .unwrap();
    }
}
