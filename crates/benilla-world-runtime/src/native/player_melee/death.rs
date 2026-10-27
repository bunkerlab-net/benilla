// SPDX-License-Identifier: GPL-3.0-or-later
//! Immediate source CombatStop fields with a private, non-persistable proof debt.
//! The executing NPC may be temporarily outside State::npcs. Only proof replay
//! waits for its return; lifecycle fields and source Cancel/Stop do not wait.
use super::source::SourceCause;
use super::state::{CauseKey, Lifecycle, Phase, ReplayedCause, Write, WriteStamp};
use super::PlayerMeleeState;
use crate::native::{Event, NativeError, Profile, State};
use crate::NpcId;

pub(in crate::native) struct PendingPlayerMeleeDeath {
    key: CauseKey,
    previous: NpcId,
    original: PlayerMeleeState,
    remaining: [u32; 2],
    captured_ms: u64,
}

fn phase(unit_updated: bool) -> Phase {
    if unit_updated {
        Phase::PostUnit
    } else {
        Phase::PreUnit
    }
}

/// Called only by the real DeathAction::CombatStop branch. No victim is a
/// source no-write operation, but still sends the forced player Cancel event.
pub(in crate::native) fn begin(
    profile: &Profile,
    current: &mut State,
    unit_updated: bool,
) -> Result<(Option<PendingPlayerMeleeDeath>, Vec<Event>), NativeError> {
    match (&profile.player_melee, &current.player_melee) {
        (None, None) => return Ok((None, vec![])),
        (Some(_), Some(_)) => {}
        _ => return Err(NativeError::InvalidState),
    }
    if current.health != 0 {
        return Err(NativeError::InvalidState);
    }
    let fields = super::current_fields(current)?;
    if fields.lifecycle.melee_attacking != fields.lifecycle.victim.is_some() {
        return Err(NativeError::InvalidState);
    }
    let mut events = vec![Event::PlayerAttackCancel];
    let Some(previous) = fields.lifecycle.victim else {
        return Ok((None, events));
    };
    let debt = PendingPlayerMeleeDeath {
        key: super::key(current, phase(unit_updated))?,
        previous,
        original: current
            .player_melee
            .as_ref()
            .ok_or(NativeError::InvalidState)?
            .clone(),
        remaining: fields.remaining_ms,
        captured_ms: current.now_ms,
    };
    // AttackStop changes no BASE/OFF timer or last swing error.
    current
        .combat
        .as_mut()
        .ok_or(NativeError::InvalidState)?
        .melee_timers
        .write_source_fields(None, fields.remaining_ms)?;
    events.push(Event::PlayerAttackStop { target: previous });
    Ok((Some(debt), events))
}

impl PendingPlayerMeleeDeath {
    fn unchanged(&self, current: &State, unit_updated: bool) -> Result<(), NativeError> {
        if current.tick != self.key.tick
            || current.now_ms != self.captured_ms
            || phase(unit_updated) != self.key.phase
            || current.health != 0
            || current.player_melee.as_ref() != Some(&self.original)
        {
            return Err(NativeError::InvalidState);
        }
        let fields = super::current_fields(current)?;
        if fields.lifecycle
            != (Lifecycle {
                melee_attacking: false,
                victim: None,
            })
            || fields.remaining_ms != self.remaining
            || fields.last_error != self.original.last_error
        {
            return Err(NativeError::InvalidState);
        }
        Ok(())
    }

    /// prepare_kill emits CombatStop twice, and JustDied update emits it again.
    /// These exact no-op stops must not overwrite or mint another proof debt.
    pub(in crate::native) fn repeated_stop(
        &self,
        current: &State,
        unit_updated: bool,
    ) -> Result<Vec<Event>, NativeError> {
        self.unchanged(current, unit_updated)?;
        Ok(vec![Event::PlayerAttackCancel])
    }

    pub(in crate::native) fn finish(
        self,
        profile: &Profile,
        current: &mut State,
        unit_updated: bool,
    ) -> Result<(), NativeError> {
        self.unchanged(current, unit_updated)?;
        let died_at_tick = current
            .combat
            .as_ref()
            .and_then(|combat| combat.death.as_ref())
            .and_then(|death| death.snapshot().died_at_tick)
            .ok_or(NativeError::InvalidState)?;
        // The completed death lifecycle is installed by prepare_kill's caller
        // before this point. The actual previous NPC owner must now be back.
        replay_stop(profile, current, self.key, self.previous, died_at_tick)?;
        super::commit_cause(
            profile,
            current,
            self.key.phase,
            SourceCause::PlayerDeath {
                key: self.key,
                previous: self.previous,
                died_at_tick,
            },
        )
    }
}

/// Re-execute the persisted lifecycle-only writer from the actual source death
/// snapshot and original victim life. No generic stop or timestamp is authority.
pub(super) fn replay_stop(
    profile: &Profile,
    current: &State,
    key: CauseKey,
    previous: NpcId,
    died_at_tick: u64,
) -> Result<ReplayedCause, NativeError> {
    let death = current
        .combat
        .as_ref()
        .and_then(|combat| combat.death.as_ref())
        .ok_or(NativeError::InvalidCheckpoint)?;
    if current.health != 0
        || death.is_alive()
        || key.tick == 0
        || key.action == 0
        || key.tick != died_at_tick
        || key.tick > current.tick
        || death.snapshot().died_at_tick != Some(died_at_tick)
    {
        return Err(NativeError::InvalidCheckpoint);
    }
    let captured_ms = key
        .tick
        .checked_mul(u64::from(profile.tick_ms))
        .and_then(|elapsed| profile.clock_origin_ms.checked_add(elapsed))
        .ok_or(NativeError::InvalidCheckpoint)?;
    let owner = current
        .npcs
        .get(&previous.spawn)
        .ok_or(NativeError::InvalidCheckpoint)?;
    crate::native::interactions::spell_authority::historical_target_profile(
        profile, owner, previous, captured_ms,
    )?;
    Ok(ReplayedCause {
        key,
        lifecycle: Some(Write {
            stamp: WriteStamp {
                cause: key,
                subphase: 1,
            },
            value: Lifecycle {
                melee_attacking: false,
                victim: None,
            },
        }),
        base: None,
        off: None,
        last_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending_fixture() -> (crate::native::NativeWorld, PendingPlayerMeleeDeath, NpcId) {
        let mut world = crate::native::player_melee::tests::world();
        // A component-level command boundary, not a fabricated native receipt.
        // The actual start reducer captures the real source-backed NPC context.
        world.state.tick = 1;
        world.state.now_ms += u64::from(world.profile.tick_ms);
        let previous = world.state.npcs[&2].id;
        let (_, rejected) = super::super::start(&world.profile, &mut world.state, previous).unwrap();
        assert!(rejected.is_none());
        // The exact field write of DeathAction::SetHealthZero precedes
        // CombatStop. This fixture deliberately does NOT forge a death state;
        // finishing it must fail until a real source death plan is installed.
        world.state.health = 0;
        let before = world.state.player_melee.clone();
        let (pending, events) = begin(&world.profile, &mut world.state, true).unwrap();
        assert_eq!(
            events,
            vec![
                Event::PlayerAttackCancel,
                Event::PlayerAttackStop { target: previous },
            ]
        );
        assert_eq!(world.state.player_melee, before);
        assert_eq!(
            world.state.combat.as_ref().unwrap().melee_timers.victim(),
            None
        );
        (world, pending.unwrap(), previous)
    }

    #[test]
    fn repeated_actual_stop_boundary_preserves_debt_and_refuses_new_writers() {
        let (world, pending, previous) = pending_fixture();
        let snapshot = world.state.clone();
        assert_eq!(
            pending.repeated_stop(&world.state, true).unwrap(),
            vec![Event::PlayerAttackCancel]
        );
        assert_eq!(
            pending.repeated_stop(&world.state, true).unwrap(),
            vec![Event::PlayerAttackCancel]
        );
        assert_eq!(world.state, snapshot);
        for mutation in 0..6 {
            let mut changed = snapshot.clone();
            match mutation {
                0 => changed.tick += 1,
                1 => changed.now_ms += 1,
                2 => changed.player_melee.as_mut().unwrap().last_error = 2,
                3 => changed
                    .combat
                    .as_mut()
                    .unwrap()
                    .melee_timers
                    .write_source_fields(None, [1, 0])
                    .unwrap(),
                4 => changed
                    .combat
                    .as_mut()
                    .unwrap()
                    .melee_timers
                    .write_source_fields(Some(previous), [0, 0])
                    .unwrap(),
                _ => changed.player_melee = Some(PlayerMeleeState::new(&world.profile, &changed)),
            }
            assert!(
                pending.repeated_stop(&changed, true).is_err(),
                "mutation {mutation}"
            );
        }
        assert!(pending.repeated_stop(&world.state, false).is_err());
    }

    #[test]
    fn missing_real_death_plan_refuses_finish_without_mutating_state() {
        let (mut world, pending, _) = pending_fixture();
        let snapshot = world.state.clone();
        assert!(pending.finish(&world.profile, &mut world.state, true).is_err());
        assert_eq!(world.state, snapshot);
    }

    #[test]
    fn idle_source_stop_sends_cancel_without_new_debt_or_frontier() {
        let mut world = crate::native::player_melee::tests::world();
        world.state.health = 0;
        let before = world.state.player_melee.clone();
        let (pending, events) = begin(&world.profile, &mut world.state, false).unwrap();
        assert!(pending.is_none());
        assert_eq!(events, vec![Event::PlayerAttackCancel]);
        assert_eq!(world.state.player_melee, before);
    }

    fn post_death_owner_query(request: &crate::native::AuthorityRequest) -> bool {
        matches!(
            &request.query,
            crate::native::Query::SourceMotion(crate::native::MotionQuery::SceneVisitor(query))
                if !query.player_alive
                    && matches!(query.notification_owner,
                        crate::native::VisitorParticipant::Creature(id) if id.spawn == 3)
        )
    }

    #[derive(Default)]
    struct RejectAfterDeath {
        base: crate::native::synthetic_two_owner_fixture::CompleteAuthority,
        rejected: bool,
    }

    impl crate::native::Authority for RejectAfterDeath {
        fn resolve(
            &mut self,
            request: &crate::native::AuthorityRequest,
        ) -> Result<crate::native::AuthorityReply, NativeError> {
            if post_death_owner_query(request) {
                self.rejected = true;
                return Err(NativeError::AuthorityUnavailable("post-death owner visitor"));
            }
            crate::native::Authority::resolve(&mut self.base, request)
        }
    }

    #[test]
    fn actual_post_death_owner_authority_failure_rolls_back_entire_frame() {
        use crate::native::{synthetic_fixture, synthetic_quest_fixture, Command, NativeWorld};
        let mut profile = crate::native::player_melee::tests::fragile_profile();
        // Authored source event schedule: both real owners visit on their first
        // update. Owner order [2, 3] makes the second visit observe NPC2's kill.
        profile
            .source_motion
            .as_mut()
            .unwrap()
            .creature_initial_aggro_delay_ms = 0;
        let mut world = NativeWorld::synthetic_component_test(
            profile.clone(),
            [1; 16],
            synthetic_fixture::player(),
            42,
        )
        .unwrap();
        let target = world.state.npcs[&2].id;
        let mut authority = crate::native::synthetic_two_owner_fixture::CompleteAuthority::default();
        let mut death_frame = None;
        for _ in 0..50 {
            let before = world.checkpoint().unwrap();
            let input = (world.state.tick == 0).then(|| {
                synthetic_quest_fixture::command(&world, Command::AttackStart { target })
            });
            authority.queries.clear();
            world
                .step(world.state.tick + 1, input.clone(), &mut authority)
                .unwrap();
            if world.state.health == 0 {
                assert!(
                    authority.queries.iter().any(post_death_owner_query),
                    "actual lethal frame must reach the later living NPC owner"
                );
                let events = &world.last_tick().unwrap().events;
                assert!(events.iter().any(|event| matches!(
                    event,
                    Event::Melee { id, effective, .. } if *id == target && *effective > 0
                )));
                assert!(events.iter().any(|event| matches!(
                    event,
                    Event::PlayerAttackStop { target: id } if *id == target
                )));
                assert_eq!(
                    world.state.combat.as_ref().unwrap().melee_timers.victim(),
                    None
                );
                let restored = NativeWorld::restore_synthetic_component_test(
                    profile.clone(),
                    world.checkpoint().unwrap(),
                )
                .unwrap();
                assert_eq!(restored.state_digest(), world.state_digest());
                death_frame = Some((before, input));
                break;
            }
        }
        let (before, input) = death_frame.expect("actual source NPC melee must kill fragile player");
        let mut retry =
            NativeWorld::restore_synthetic_component_test(profile, before.clone()).unwrap();
        let before_records = retry.records().len();
        let mut rejecting = RejectAfterDeath::default();
        assert!(matches!(
            retry.step(retry.state.tick + 1, input, &mut rejecting),
            Err(NativeError::AuthorityUnavailable("post-death owner visitor"))
        ));
        assert!(
            rejecting.rejected,
            "failure must occur after actual staged player death"
        );
        assert_eq!(retry.checkpoint().unwrap(), before);
        assert_eq!(retry.records().len(), before_records);
        assert!(retry.state.health > 0);
    }
}
