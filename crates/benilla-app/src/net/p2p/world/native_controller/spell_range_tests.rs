// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored raw spell rows through the selected controller, projection and real store.
use super::*;
use benilla_sim::{classic_spell as spell, creature_ai::movement::Check};
use n::synthetic_spell_fixture as source;

#[derive(Default)]
struct Observed {
    external: Vec<n::AuthorityRequest>,
    derived: Vec<n::AuthorityReply>,
}
struct SpatialOnly {
    observed: Arc<Mutex<Observed>>,
    fail_release_range: Arc<AtomicBool>,
    inner: source::SpatialOnly,
}
impl n::Authority for SpatialOnly {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        assert!(
            !matches!(
                request.query,
                n::Query::SourceSpell(n::SourceSpellQuery::Range { .. })
            ),
            "runtime-owned range escaped to the external authority"
        );
        self.observed.lock().unwrap().external.push(request.clone());
        self.inner.resolve(request)
    }
    fn observe_derived(&mut self, reply: &n::AuthorityReply) -> Result<(), n::NativeError> {
        self.observed.lock().unwrap().derived.push(reply.clone());
        if self.fail_release_range.load(Ordering::SeqCst)
            && matches!(&reply.request.query,
                n::Query::SourceSpell(n::SourceSpellQuery::Range { context, .. })
                if context.phase == n::SourceSpellPhase::ReleaseCheck)
        {
            return Err(n::NativeError::AuthorityUnavailable(
                "authored derived range receipt fault",
            ));
        }
        self.inner.observe_derived(reply)
    }
}
fn authority(observed: Arc<Mutex<Observed>>) -> SpatialOnly {
    SpatialOnly {
        observed,
        fail_release_range: Arc::new(AtomicBool::new(false)),
        inner: source::spatial_only(),
    }
}
fn profile(minimum: f32, maximum: f32, distance: f32, cost: u32) -> n::Profile {
    let (mut p, mut initial, mut rows) = source::inputs();
    p.npcs[0].ai.spawn = fixture::point(distance);
    rows.spells[0].range.minimum = fixture::float(minimum);
    rows.spells[0].range.maximum = fixture::float(maximum);
    // Authored raw variant permits moving the caster between real source phases.
    rows.spells[0].words[21] &= !1;
    rows.spells[0].words[32] = cost;
    initial.spells[0].words[21] = rows.spells[0].words[21];
    initial.spells[0].words[32] = cost;
    let n::AbilityAdmission::Available(spec) = &mut p.abilities[0].admission else {
        unreachable!()
    };
    spec.movement_interrupts = false;
    spec.mana_cost = cost;
    p.initialize_player(&initial, fixture::float(100.0), 1)
        .unwrap();
    p.initialize_spell_authority(&initial, rows).unwrap();
    p
}
fn cast_command(c: &NativeController) -> ClientCommand {
    ClientCommand::CastSpell {
        spell_id: 133,
        target: Some(c.projection.npc_guid(c.world.state().npcs[&2].id).unwrap()),
    }
}
fn range_receipts(observed: &Observed) -> Vec<(&n::SourceSpellRequest, n::SpellRangeResult)> {
    observed
        .derived
        .iter()
        .filter_map(|r| match (&r.request.query, &r.facts) {
            (
                n::Query::SourceSpell(n::SourceSpellQuery::Range { context, .. }),
                n::Facts::SourceSpell(n::SourceSpellFacts::Range(result)),
            ) => Some((context, *result)),
            _ => None,
        })
        .collect()
}
fn failures(events: &[SessionEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, SessionEvent::SpellFailedOther { spell_id: 133, .. }))
        .count()
}
#[derive(Debug, PartialEq, Eq)]
enum CastProjection {
    Result(bool, Option<u8>),
    Start,
    Go,
    FailedOther,
}
fn cast_projection(events: &[SessionEvent]) -> Vec<CastProjection> {
    events
        .iter()
        .filter_map(|event| match event {
            SessionEvent::CastResult {
                spell_id: 133,
                success,
                reason,
                arg,
            } => {
                assert_eq!(*arg, None);
                Some(CastProjection::Result(*success, *reason))
            }
            SessionEvent::SpellStart { spell_id: 133, .. } => Some(CastProjection::Start),
            SessionEvent::SpellGo { spell_id: 133, .. } => Some(CastProjection::Go),
            SessionEvent::SpellFailedOther { spell_id: 133, .. } => {
                Some(CastProjection::FailedOther)
            }
            _ => None,
        })
        .collect()
}
fn no_rejection_notification(events: &[SessionEvent]) {
    assert!(!events.iter().any(|event| matches!(event,
        SessionEvent::Notification { text } if text.starts_with("Native action rejected:")
    )));
}
fn no_projected_damage(events: &[SessionEvent]) {
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::SpellDamageLog(_) | SessionEvent::PeriodicAuraLog(_)
    )));
}

#[test]
fn source_range_prepare_bounds_preserve_los_range_facing_and_power_precedence() {
    use n::SpellRangeResult::{TooClose, TooFar, Within};
    // Both authored reaches are 1.5: minimum 5 becomes 8, maximum 30 becomes 33.
    for (distance, los, facing, cost, error, range) in [
        (8.0, Check::Passed, Check::Passed, 10, None, Some(Within)),
        (33.0, Check::Passed, Check::Passed, 10, None, Some(Within)),
        (
            f32::from_bits(8.0_f32.to_bits() - 1),
            Check::Passed,
            Check::Failed,
            1000,
            Some(0x76),
            Some(TooClose),
        ),
        (
            f32::from_bits(33.0_f32.to_bits() + 1),
            Check::Passed,
            Check::Failed,
            1000,
            Some(0x59),
            Some(TooFar),
        ),
        (
            36.0,
            Check::Passed,
            Check::Failed,
            1000,
            Some(0x59),
            Some(TooFar),
        ),
        (36.0, Check::Failed, Check::Failed, 1000, Some(0x2a), None),
        (
            16.0,
            Check::Passed,
            Check::Failed,
            1000,
            Some(0x7c),
            Some(Within),
        ),
    ] {
        let h = Harness::new();
        let observed = Arc::new(Mutex::new(Observed::default()));
        let mut a = authority(observed.clone());
        a.inner.line_of_sight = Some(los);
        a.inner.facing = Some(facing);
        let mut c = h
            .enter(
                Entry::Create {
                    world: [8; 16],
                    seed: 7,
                },
                profile(5.0, 30.0, distance, cost),
                a,
            )
            .unwrap();
        c.initial_events().unwrap();
        let mana = c.world.state().resources.caster.mana;
        if cost == 1000 {
            assert!(cost > mana, "precedence case must also fail CheckPower");
        }
        let health = c.world.state().npcs[&2].health;
        c.receive(cast_command(&c)).unwrap();
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(c.world.state().next_sequence, 1);
        assert_eq!(c.world.state().resources.caster.mana, mana);
        assert_eq!(c.world.state().resources.caster.last_mana_use_ms, None);
        assert_eq!(c.world.state().npcs[&2].health, health);
        assert!(c.world.last_tick().unwrap().draws.is_empty());
        no_projected_damage(&out.events);
        if matches!(error, Some(0x59 | 0x76)) {
            // Pinned Spell.cpp SpellStart: failed prepare only sends SendCastResult.
            assert_eq!(
                cast_projection(&out.events),
                vec![CastProjection::Result(
                    false,
                    error.map(|code: u32| u8::try_from(code).unwrap()),
                )]
            );
            no_rejection_notification(&out.events);
        } else {
            // Other rejection presentation remains outside the range-only boundary.
            assert_eq!(failures(&out.events), usize::from(error.is_some()));
            assert!(out.events.iter().any(|e| matches!(
                e,
                SessionEvent::CastResult {
                    spell_id: 133,
                    success,
                    reason: None,
                    arg: None,
                } if *success == error.is_none()
            )));
        }
        if let Some(code) = error {
            assert!(c.world.last_tick().unwrap().events.iter().any(|e| matches!(
                e,
                n::Event::Spell {
                    ability: 133,
                    event: spell::SpellEvent::Failed(spell::CastFailure::Rejected(value)),
                    ..
                } if *value == code
            )));
            assert!(c.world.state().casts.is_empty());
            assert!(!out.events.iter().any(|e| matches!(
                e,
                SessionEvent::SpellStart { .. } | SessionEvent::SpellGo { .. }
            )));
        } else {
            assert!(out
                .events
                .iter()
                .any(|e| matches!(e, SessionEvent::SpellStart { spell_id: 133, .. })));
        }
        let observed = observed.lock().unwrap();
        let receipts = range_receipts(&observed);
        assert_eq!(receipts.first().map(|(_, result)| *result), range);
        assert_eq!(receipts.len(), usize::from(range.is_some()));
        if let Some((context, _)) = receipts.first() {
            assert_eq!(context.phase, n::SourceSpellPhase::Prepare);
            assert_eq!(context.player_position, fixture::point(0.0));
            assert_eq!(context.target_position, fixture::point(distance));
            assert!(context.scheduler.is_none());
        }
        let kinds: Vec<_> = observed
            .external
            .iter()
            .filter_map(|r| match r.query {
                n::Query::SourceSpell(n::SourceSpellQuery::LineOfSight(_)) => Some("los"),
                n::Query::SourceSpell(n::SourceSpellQuery::Facing(_)) => Some("facing"),
                n::Query::SourceSpell(_) => panic!("unexpected external spell dependency"),
                _ => None,
            })
            .collect();
        assert_eq!(
            kinds,
            if range == Some(Within) {
                vec!["los", "facing"]
            } else {
                vec!["los"]
            }
        );
    }
}

#[test]
fn source_range_release_uses_current_pose_and_capped_or_proportional_tolerance() {
    use n::SpellRangeResult::{TooClose, TooFar, Within};
    for (maximum, target, moved, range) in [
        (30.0, 33.0, -3.0, Within),
        (30.0, 33.0, -3.000_004, TooFar),
        (17.0, 20.0, -2.0, Within),
        (17.0, 20.0, -2.000_002, TooFar),
        (30.0, 10.0, 2.0, Within),
        (30.0, 10.0, 2.000_001, TooClose),
    ] {
        let within = range == Within;
        let h = Harness::new();
        let observed = Arc::new(Mutex::new(Observed::default()));
        let mut c = h
            .enter(
                Entry::Create {
                    world: [8; 16],
                    seed: 7,
                },
                profile(5.0, maximum, target, 10),
                authority(observed.clone()),
            )
            .unwrap();
        c.initial_events().unwrap();
        let mana = c.world.state().resources.caster.mana;
        let health = c.world.state().npcs[&2].health;
        c.receive(cast_command(&c)).unwrap();
        let start = pump_to(&mut c, 2);
        assert_eq!(
            cast_projection(&start),
            vec![CastProjection::Result(true, None), CastProjection::Start]
        );
        c.receive(move_to([moved, 0.0, 0.0])).unwrap();
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert!(out.saved.is_some());
        assert_eq!(failures(&out.events), usize::from(!within));
        assert_eq!(
            cast_projection(&out.events),
            match range {
                Within => vec![CastProjection::Go],
                TooFar | TooClose => vec![
                    CastProjection::Result(false, Some(if range == TooFar { 0x59 } else { 0x76 })),
                    CastProjection::FailedOther,
                ],
                _ => unreachable!(),
            }
        );
        no_rejection_notification(&out.events);
        assert_eq!(
            out.events
                .iter()
                .filter(|e| matches!(e, SessionEvent::SpellGo { spell_id: 133, .. }))
                .count(),
            usize::from(within)
        );
        no_projected_damage(&out.events);
        assert_eq!(c.world.state().npcs[&2].health, health);
        assert_eq!(
            c.world.state().resources.caster.mana,
            mana - u32::from(within) * 10
        );
        assert_eq!(c.world.state().next_sequence, 2);
        assert_eq!(c.position_xyz(), [moved, 0.0, 0.0]);
        assert!(c.pump(Duration::ZERO).events.is_empty());
        let observed = observed.lock().unwrap();
        let receipts = range_receipts(&observed);
        assert_eq!(receipts.len(), 2);
        assert_eq!(receipts[0].0.phase, n::SourceSpellPhase::Prepare);
        assert_eq!(receipts[0].0.player_position, fixture::point(0.0));
        assert_eq!(receipts[0].1, n::SpellRangeResult::Within);
        assert_eq!(receipts[1].0.phase, n::SourceSpellPhase::ReleaseCheck);
        assert_eq!(receipts[1].0.player_position, fixture::point(moved));
        assert_eq!(receipts[1].0.target_position, fixture::point(target));
        assert_eq!(
            receipts[1].0.player_orientation,
            n::Orientation::new(0.75).unwrap()
        );
        assert!(receipts[1].0.scheduler.is_some());
        assert_eq!(receipts[1].1, range);
        assert!(observed.derived.iter().any(|r| matches!(
            (&r.request.query, &r.facts),
            (
                n::Query::SourceSpell(n::SourceSpellQuery::Semantics(q)),
                n::Facts::SourceSpell(n::SourceSpellFacts::Semantics(facts)),
            ) if q.phase == n::SourceSpellPhase::ReleaseCheck
                && facts.other_failure == match range {
                    TooFar => Some(0x59),
                    TooClose => Some(0x76),
                    Within => None,
                    _ => unreachable!(),
                }
        )));
        assert_eq!(
            observed.derived.iter().any(|r| matches!(&r.request.query,
                n::Query::SourceSpell(n::SourceSpellQuery::Semantics(q))
                if q.phase == n::SourceSpellPhase::ReleaseHit)),
            within
        );
    }
}

#[test]
fn source_range_receipt_and_publication_faults_preserve_pose_journal_and_once_only_failure() {
    for (target, moved, code) in [(33.0, -3.000_004, 0x59), (10.0, 2.000_001, 0x76)] {
        release_failure_retries(target, moved, code);
    }
}

fn release_failure_retries(target: f32, moved: f32, code: u8) {
    let h = Harness::new();
    let data = profile(5.0, 30.0, target, 10);
    let observed = Arc::new(Mutex::new(Observed::default()));
    let fail = Arc::new(AtomicBool::new(false));
    let mut a = authority(observed.clone());
    a.fail_release_range = fail.clone();
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            a,
        )
        .unwrap();
    c.initial_events().unwrap();
    let mut replay = c.world.clone();
    let mut replay_projection =
        NativeProjection::new(&replay, replay.state().player, &Catalog).unwrap();
    let generation = c.store.generation();
    let mana = c.world.state().resources.caster.mana;
    let health = c.world.state().npcs[&2].health;
    c.receive(cast_command(&c)).unwrap();
    pump_to(&mut c, 2);
    c.receive(move_to([moved, 0.0, 0.0])).unwrap();
    let before = c.world.checkpoint().unwrap();
    let records_before = c.world.records().to_vec();
    fail.store(true, Ordering::SeqCst);
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 0);
    assert!(out.events.is_empty());
    assert!(matches!(out.pause.unwrap().kind, PauseKind::Authority));
    assert_eq!(c.world.checkpoint().unwrap(), before);
    assert_eq!(c.world.records(), records_before);
    assert_eq!(c.world.state().next_sequence, 1);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    let pending = c.pending.as_ref().unwrap().inputs.clone();
    fail.store(false, Ordering::SeqCst);
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored range save fault",
    )
    .unwrap();
    c.retry_tick().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(
        c.world.last_tick().unwrap().inputs,
        pending
            .into_iter()
            .map(|input| input.input)
            .collect::<Vec<_>>()
    );
    assert!(c.world.last_tick().unwrap().draws.is_empty());
    assert_eq!(c.world.state().next_sequence, 2);
    assert_eq!(c.world.state().resources.caster.mana, mana);
    assert_eq!(c.world.state().resources.caster.last_mana_use_ms, None);
    assert_eq!(c.world.state().npcs[&2].health, health);
    assert_eq!(c.world.state().position, fixture::point(moved));
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert_eq!(c.facing(), 0.0);
    assert_eq!(c.store.generation(), generation);
    assert_eq!(
        c.world
            .records()
            .iter()
            .flat_map(|r| &r.events)
            .filter(|e| matches!(
                e,
                n::Event::Spell {
                    ability: 133,
                    event: spell::SpellEvent::Failed(spell::CastFailure::Rejected(value)),
                    ..
                } if *value == u32::from(code)
            ))
            .count(),
        1
    );
    let records = c.world.records().to_vec();
    let mut replay_events = Vec::new();
    for record in &records {
        replay.replay(std::slice::from_ref(record)).unwrap();
        replay_events.extend(replay_projection.advance(&replay).unwrap());
    }
    assert_eq!(
        cast_projection(&replay_events),
        vec![
            CastProjection::Result(true, None),
            CastProjection::Start,
            CastProjection::Result(false, Some(code)),
            CastProjection::FailedOther,
        ]
    );
    no_rejection_notification(&replay_events);
    let digest = c.world.state_digest();
    assert_eq!(replay.state_digest(), digest);
    let observations = observed.lock().unwrap().derived.len();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    let saved = c.retry_checkpoint().unwrap();
    assert_eq!(saved.tick, 3);
    assert_eq!(saved.generation, generation + 1);
    assert_eq!(c.store.generation(), generation + 1);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    // Pinned Spell.cpp StopCast sends CastResult before SendInterrupted's FailedOther.
    assert_eq!(
        cast_projection(&out.events),
        vec![
            CastProjection::Result(false, Some(code)),
            CastProjection::FailedOther,
        ]
    );
    no_rejection_notification(&out.events);
    no_projected_damage(&out.events);
    assert!(!out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellGo { .. })));
    assert!(out.events.iter().any(|e| matches!(e,
        SessionEvent::ObjectMove { guid, position, orientation, .. }
        if *guid == c.player_guid() && *position == [moved, 0.0, 0.0] && *orientation == 0.75
    )));
    assert_eq!(c.position_xyz(), [moved, 0.0, 0.0]);
    assert_eq!(c.facing(), 0.75);
    assert_eq!(observed.lock().unwrap().derived.len(), observations);
    assert!(c.world.records().is_empty());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut reopened = h
        .enter(Entry::Open { world: [8; 16] }, data, authority(observed))
        .unwrap();
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.position_xyz(), [moved, 0.0, 0.0]);
    assert_eq!(reopened.facing(), 0.75);
    assert!(cast_projection(&reopened.initial_events().unwrap()).is_empty());
    let out = pump_to(&mut reopened, 4);
    assert!(cast_projection(&out).is_empty());
    no_projected_damage(&out);
    assert!(!out
        .iter()
        .any(|e| matches!(e, SessionEvent::SpellGo { .. })));
    replay.step(4, None, &mut source::spatial_only()).unwrap();
    assert_eq!(reopened.world.state_digest(), replay.state_digest());
}

#[test]
fn same_tick_range_failures_keep_each_reason_and_later_start_across_retry_replay_and_reopen() {
    let h = Harness::new();
    let data = profile(5.0, 30.0, 33.0, 10);
    let observed = Arc::new(Mutex::new(Observed::default()));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            data.clone(),
            authority(observed.clone()),
        )
        .unwrap();
    c.initial_events().unwrap();
    let mut replay = c.world.clone();
    let mut replay_projection =
        NativeProjection::new(&replay, replay.state().player, &Catalog).unwrap();
    let mana = c.world.state().resources.caster.mana;
    let health = c.world.state().npcs[&2].health;
    let generation = c.store.generation();
    // One spell ID, three different executing casts: too far, too close, then valid.
    for position in [-1.0, 26.0, 1.0] {
        c.receive(move_to([position, 0.0, 0.0])).unwrap();
        c.receive(cast_command(&c)).unwrap();
    }
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored prepare range save fault",
    )
    .unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(out.saved.is_none());
    assert!(matches!(
        out.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert_eq!(c.world.state().next_sequence, 6);
    assert_eq!(c.world.state().next_cast_id, 4);
    assert_eq!(c.world.state().casts.len(), 1);
    assert!(c.world.state().casts.contains_key(&3));
    assert_eq!(c.world.state().resources.caster.mana, mana);
    assert_eq!(c.world.state().resources.caster.last_mana_use_ms, None);
    assert_eq!(c.world.state().npcs[&2].health, health);
    assert!(c.world.last_tick().unwrap().draws.is_empty());
    assert_eq!(c.world.state().position, fixture::point(1.0));
    assert_eq!(c.position_xyz(), [0.0; 3]);
    assert_eq!(c.facing(), 0.0);
    assert_eq!(c.store.generation(), generation);
    {
        let observed = observed.lock().unwrap();
        assert_eq!(
            range_receipts(&observed)
                .iter()
                .map(|(context, result)| (context.cast_id, context.phase, *result))
                .collect::<Vec<_>>(),
            vec![
                (1, n::SourceSpellPhase::Prepare, n::SpellRangeResult::TooFar),
                (
                    2,
                    n::SourceSpellPhase::Prepare,
                    n::SpellRangeResult::TooClose
                ),
                (3, n::SourceSpellPhase::Prepare, n::SpellRangeResult::Within),
            ]
        );
    }
    let expected = vec![
        CastProjection::Result(false, Some(0x59)),
        CastProjection::Result(false, Some(0x76)),
        CastProjection::Result(true, None),
        CastProjection::Start,
    ];
    replay.replay(c.world.records()).unwrap();
    let replay_events = replay_projection.advance(&replay).unwrap();
    assert_eq!(cast_projection(&replay_events), expected);
    no_rejection_notification(&replay_events);
    let digest = c.world.state_digest();
    assert_eq!(replay.state_digest(), digest);
    let observations = observed.lock().unwrap().derived.len();
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    let saved = c.retry_checkpoint().unwrap();
    assert_eq!(saved.tick, 1);
    assert_eq!(saved.generation, generation + 1);
    assert_eq!(c.store.generation(), generation + 1);
    assert_eq!(c.world.state_digest(), digest);
    assert_eq!(c.position_xyz(), [0.0; 3]);
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert!(out.pause.is_none());
    assert!(out.saved.is_none());
    assert_eq!(cast_projection(&out.events), expected);
    no_rejection_notification(&out.events);
    no_projected_damage(&out.events);
    assert_eq!(
        out.events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::ObjectMove { guid, position, .. } if *guid == c.player_guid() => {
                    Some(*position)
                }
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![[-1.0, 0.0, 0.0], [26.0, 0.0, 0.0], [1.0, 0.0, 0.0]]
    );
    assert_eq!(c.position_xyz(), [1.0, 0.0, 0.0]);
    assert_eq!(c.facing(), 0.75);
    assert_eq!(observed.lock().unwrap().derived.len(), observations);
    assert!(c.world.records().is_empty());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut reopened = h
        .enter(Entry::Open { world: [8; 16] }, data, authority(observed))
        .unwrap();
    assert_eq!(reopened.world.state_digest(), digest);
    assert_eq!(reopened.position_xyz(), [1.0, 0.0, 0.0]);
    assert_eq!(
        cast_projection(&reopened.initial_events().unwrap()),
        vec![CastProjection::Start]
    );
    let out = pump_to(&mut reopened, 3);
    assert_eq!(cast_projection(&out), vec![CastProjection::Go]);
    no_rejection_notification(&out);
    no_projected_damage(&out);
    assert_eq!(reopened.world.state().resources.caster.mana, mana - 10);
    assert_eq!(reopened.world.state().next_sequence, 6);
    for tick in 2..=3 {
        replay
            .step(tick, None, &mut source::spatial_only())
            .unwrap();
    }
    assert_eq!(reopened.world.state_digest(), replay.state_digest());
}

#[test]
fn final_same_spell_input_rejection_does_not_steal_previous_cast_release_reason() {
    let h = Harness::new();
    let observed = Arc::new(Mutex::new(Observed::default()));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            profile(5.0, 30.0, 33.0, 10),
            authority(observed.clone()),
        )
        .unwrap();
    c.initial_events().unwrap();
    let mana = c.world.state().resources.caster.mana;
    c.receive(cast_command(&c)).unwrap();
    pump_to(&mut c, 2);
    c.receive(move_to([-3.000_004, 0.0, 0.0])).unwrap();
    c.receive(cast_command(&c)).unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none(), "{:?}", out.pause);
    // The final input is rejected before the scheduler releases the older cast.
    assert_eq!(
        cast_projection(&out.events),
        vec![
            CastProjection::Result(false, None),
            CastProjection::Result(false, Some(0x59)),
            CastProjection::FailedOther,
        ]
    );
    assert_eq!(
        out.events
            .iter()
            .filter_map(|event| match event {
                SessionEvent::Notification { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec!["Native action rejected: AlreadyCasting"]
    );
    no_projected_damage(&out.events);
    assert_eq!(c.world.state().next_sequence, 3);
    assert_eq!(c.world.state().next_cast_id, 2);
    assert_eq!(c.world.state().resources.caster.mana, mana);
    assert!(c.world.state().casts.is_empty());
    let observed = observed.lock().unwrap();
    let receipts = range_receipts(&observed);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[1].0.cast_id, 1);
    assert_eq!(receipts[1].0.phase, n::SourceSpellPhase::ReleaseCheck);
    assert_eq!(receipts[1].1, n::SpellRangeResult::TooFar);
    drop(observed);
    assert!(c.pump(Duration::ZERO).events.is_empty());
}
