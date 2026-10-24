// SPDX-License-Identifier: GPL-3.0-or-later
//! Actual app input, filesystem failure, and reopen tests over authored native quest rows.
use super::*;

fn enter(h: &Harness, open: bool) -> NativeController {
    h.enter(
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        quests::profile(),
        quests::QuestAuthority::default(),
    )
    .unwrap()
}
fn giver(c: &NativeController) -> u64 {
    c.projection.npc_guid(c.world.state().npcs[&3].id).unwrap()
}
fn accept(c: &mut NativeController) -> Pump {
    c.receive(ClientCommand::QuestgiverAccept {
        npc: giver(c),
        quest: 70001,
    })
    .unwrap();
    c.pump(c.tick_duration())
}
fn reward(c: &mut NativeController) -> Pump {
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: giver(c),
        quest: 70001,
        choice: 0,
    })
    .unwrap();
    c.pump(c.tick_duration())
}
fn kill(c: &mut NativeController) -> Vec<SessionEvent> {
    c.receive(cast(c)).unwrap();
    let mut events = vec![];
    for _ in 0..25 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(out.saved.as_ref().unwrap().tick, c.tick());
        events.extend(out.events);
        if c.world.state().npcs[&2].health == 0 {
            return events;
        }
    }
    panic!("authored creature did not die");
}
fn ready(h: &Harness) -> NativeController {
    let mut c = enter(h, false);
    assert!(accept(&mut c).pause.is_none());
    let events = kill(&mut c);
    assert!(events
        .iter()
        .any(|e| matches!(e, SessionEvent::XpGain(x) if x.kill)));
    c
}
fn inject(h: &Harness) {
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored recoverable fault",
    )
    .unwrap();
}
fn repair(h: &Harness) {
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
}
fn paused(out: &Pump) {
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.as_ref().unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
}
fn has_reward(events: &[SessionEvent]) -> bool {
    events
        .iter()
        .any(|e| matches!(e, SessionEvent::QuestComplete(x) if x.quest_id == 70001))
}
fn no_rewards(events: &[SessionEvent]) {
    assert!(!events.iter().any(|e| matches!(
        e,
        SessionEvent::QuestComplete(_) | SessionEvent::XpGain(_) | SessionEvent::LevelUp(_)
    )));
}
fn progress(c: &NativeController) -> &n::QuestProgressionState {
    c.world.state().quest_progression.as_ref().unwrap()
}

#[test]
fn stock_quest_routes_are_read_only_until_accept_and_reward_are_committed() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let before = c.world.state_digest();
    let npc = giver(&c);
    assert!(matches!(
        c.receive(ClientCommand::NpcTextQuery {
            text_id: 1,
            guid: npc
        }),
        Err(Error::UnsupportedCommand)
    ));

    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverHello { npc })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGreeting(_)]
    ));
    assert!(matches!(
        &c.receive(ClientCommand::QuestQuery { quest: 70001 })
            .unwrap()
            .events[..],
        [SessionEvent::QuestTemplate(_)]
    ));
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverQuery { npc, quest: 70001 })
            .unwrap()
            .events[..],
        [SessionEvent::QuestDetail(_)]
    ));
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverStatusQuery { npc })
            .unwrap()
            .events[..],
        [SessionEvent::QuestGiverStatus { status: 5, .. }]
    ));
    assert!(c
        .receive(ClientCommand::QuestgiverComplete { npc, quest: 70001 })
        .is_err());
    assert!(c
        .receive(ClientCommand::QuestgiverRequestReward { npc, quest: 70001 })
        .is_err());
    assert!(matches!(
        c.receive(ClientCommand::QuestgiverChooseReward {
            npc,
            quest: 70001,
            choice: 6
        }),
        Err(Error::UnsupportedCommand)
    ));
    assert_eq!(c.world.state_digest(), before);
    assert!(c.actions.is_empty());
    let accepted = accept(&mut c);
    assert!(accepted.pause.is_none());
    assert!(accepted
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::GossipComplete)));
    assert_eq!(accepted.saved.as_ref().unwrap().tick, 1);
    assert!(c.world.last_tick().is_none());
    assert!(c.world.records().is_empty());
    assert!(
        matches!(&c.receive(ClientCommand::QuestgiverComplete { npc, quest: 70001 }).unwrap().events[..], [SessionEvent::QuestProgress(p)] if !p.is_complete)
    );
    let killed = kill(&mut c);
    let xp = killed
        .iter()
        .position(|e| matches!(e, SessionEvent::XpGain(x) if x.kill))
        .unwrap();
    let credit = killed
        .iter()
        .position(|e| {
            matches!(
                e,
                SessionEvent::QuestObjectiveKill {
                    quest_id: 70001,
                    count: 1,
                    ..
                }
            )
        })
        .unwrap();
    assert!(xp < credit);
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverComplete { npc, quest: 70001 })
            .unwrap()
            .events[..],
        [SessionEvent::QuestOffer(_)]
    ));
    assert!(matches!(
        &c.receive(ClientCommand::QuestgiverRequestReward { npc, quest: 70001 })
            .unwrap()
            .events[..],
        [SessionEvent::QuestOffer(_)]
    ));
    let rewarded = reward(&mut c);
    assert!(rewarded.pause.is_none());
    assert!(has_reward(&rewarded.events));
    assert_eq!(progress(&c).money, 18);
    assert_eq!(c.world.player_level(), 3);
    assert_eq!(rewarded.saved.unwrap().tick, c.tick());
    let before = c.world.state_digest();
    drop(c); // Simulated process loss after receipt, without an orderly shutdown.
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), before);
    no_rewards(&c.initial_events().unwrap());
    let duplicate = reward(&mut c);
    assert!(duplicate.pause.is_none());
    no_rewards(&duplicate.events);
    assert_eq!(progress(&c).money, 18);
    assert_eq!(c.world.player_level(), 3);
}

#[test]
fn reward_events_wait_for_receipt_and_retry_flushes_once_without_reexecution() {
    let h = Harness::new();
    let mut c = ready(&h);
    let prior_generation = c.store.generation();
    inject(&h);
    let out = reward(&mut c);
    paused(&out);
    assert_eq!(c.store.generation(), prior_generation);
    assert!(c.pending.is_none());
    assert_eq!(progress(&c).money, 18);
    assert!(progress(&c).quests.progress(70001).unwrap().rewarded);
    let state = c.world.state_digest();
    let tick = c.tick();
    let sequence = c.world.state().next_sequence;
    let count = c.outbox.as_ref().unwrap().events.len();
    assert!(count > 0 && count <= MAX_PENDING_EVENTS);
    assert!(c.outbox.as_ref().unwrap().saved.is_none());
    assert!(c.retry_tick().is_err());
    assert!(c.retry_checkpoint().is_err());
    assert!(matches!(
        c.receive(ClientCommand::QuestQuery { quest: 70001 }),
        Err(Error::Paused)
    ));
    let idle = c.pump(Duration::from_secs(600));
    assert!(idle.events.is_empty());
    assert_eq!(idle.ticks, 0);
    assert_eq!(c.tick(), tick);
    repair(&h);
    let saved = c.retry_checkpoint().unwrap();
    assert_eq!(saved.tick, tick);
    assert_eq!(c.world.state_digest(), state);
    assert!(c.world.records().is_empty());
    // Querying or accepting another action cannot observe the unpublished durable tick.
    assert!(matches!(
        c.receive(ClientCommand::QuestQuery { quest: 70001 }),
        Err(Error::Paused)
    ));
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(out.events.len(), count);
    assert!(has_reward(&out.events));
    assert!(out
        .events
        .iter()
        .any(|e| matches!(e, SessionEvent::XpGain(x) if !x.kill && x.total == 100)));
    assert!(c.outbox.is_none());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    assert_eq!(c.world.state().next_sequence, sequence);
    assert_eq!(c.world.state_digest(), state);
    drop(c);
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), state);
    no_rewards(&c.initial_events().unwrap());
    no_rewards(&reward(&mut c).events);
    assert_eq!(progress(&c).money, 18);
}

#[test]
fn lethal_tick_never_exposes_xp_or_credit_before_its_checkpoint() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    assert!(accept(&mut c).pause.is_none());
    c.receive(cast(&c)).unwrap();
    assert!(c.pump(c.tick_duration()).pause.is_none());
    // Find the lethal boundary with the same deterministic fixture on a disposable clone.
    for _ in 0..25 {
        let mut next = c.world.clone();
        next.step(
            next.state().tick + 1,
            None,
            &mut quests::QuestAuthority::default(),
        )
        .unwrap();
        if next.state().npcs[&2].health == 0 {
            break;
        }
        assert!(c.pump(c.tick_duration()).pause.is_none());
    }
    assert!(c.world.state().npcs[&2].health > 0);
    inject(&h);
    let out = c.pump(c.tick_duration());
    paused(&out);
    assert_eq!(c.world.state().npcs[&2].health, 0);
    assert_eq!(c.world.player_level(), 2);
    let state = c.world.state_digest();
    let draws = c.world.state().rng_counter;
    let credited = progress(&c).credited_generations.clone();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(e, SessionEvent::XpGain(x) if x.kill && x.total == 50))
            .count(),
        1
    );
    assert_eq!(
        out.events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::QuestObjectiveKill {
                    quest_id: 70001,
                    ..
                }
            ))
            .count(),
        1
    );
    assert_eq!(c.world.state().rng_counter, draws);
    assert_eq!(progress(&c).credited_generations, credited);
    assert_eq!(c.world.state_digest(), state);
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), state);
    no_rewards(&c.initial_events().unwrap());
    no_rewards(&c.pump(c.tick_duration()).events);
    assert_eq!(progress(&c).credited_generations, credited);
}

#[test]
fn shutdown_retains_unsaved_reward_then_flushes_receipt_backed_events_once() {
    for explicit_retry in [false, true] {
        let h = Harness::new();
        let mut c = ready(&h);
        inject(&h);
        paused(&reward(&mut c));
        let state = c.world.state_digest();
        assert!(c.shutdown().is_err());
        assert!(c.closing());
        assert!(!c.closed());
        assert!(c.outbox.is_some());
        assert!(c.pump(Duration::from_secs(30)).events.is_empty());
        repair(&h);
        if explicit_retry {
            c.retry_checkpoint().unwrap();
            // Already received publication must not depend on a redundant second write.
            inject(&h);
        }
        let (saved, cancelled, events) = c.shutdown().unwrap();
        assert_eq!(saved.tick, c.tick());
        assert_eq!(cancelled, 0);
        assert!(has_reward(&events));
        assert!(c.outbox.is_none());
        assert!(c.closed());
        assert!(matches!(c.shutdown(), Err(Error::Closed)));
        assert!(c.pump(Duration::ZERO).events.is_empty());
        assert_eq!(c.world.state_digest(), state);
        if explicit_retry {
            repair(&h);
        }
        drop(c);
        assert_eq!(enter(&h, true).world.state_digest(), state);
    }
}

#[test]
fn concurrent_checkpoint_change_is_reopen_required_and_cannot_publish_reward() {
    let h = Harness::new();
    let mut c = ready(&h);
    let before = fs::read(h.path().join("world.checkpoint")).unwrap();
    let last_saved = c.world.state_digest();
    fs::write(
        h.path().join("world.checkpoint"),
        b"authored external modification",
    )
    .unwrap();
    let out = reward(&mut c);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.as_ref().unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: true
        }
    ));
    assert!(c.outbox.as_ref().unwrap().saved.is_none());
    fs::write(h.path().join("world.checkpoint"), before).unwrap();
    assert!(c.retry_checkpoint().is_err());
    assert!(c.retry_tick().is_err());
    assert!(c.shutdown().is_err());
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), last_saved);
    assert_eq!(progress(&c).money, 11);
    // Disk proves this reward was not committed. A newly requested action may award once.
    assert!(has_reward(&reward(&mut c).events));
    assert_eq!(progress(&c).money, 18);
}

#[test]
fn projection_failure_saves_committed_progress_only_on_shutdown_and_reopens_cleanly() {
    let h = Harness::new();
    let mut c = ready(&h);
    c.receive(ClientCommand::QuestgiverChooseReward {
        npc: giver(&c),
        quest: 70001,
        choice: 0,
    })
    .unwrap();
    c.world
        .step(c.tick() + 1, None, &mut quests::QuestAuthority::default())
        .unwrap();
    let out = c.pump(c.tick_duration());
    assert_eq!(out.ticks, 1);
    assert!(out.events.is_empty());
    assert!(matches!(
        out.pause.as_ref().unwrap().kind,
        PauseKind::Projection
    ));
    assert!(c.pending.is_none());
    assert!(c.outbox.is_none());
    assert_eq!(progress(&c).money, 18);
    let state = c.world.state_digest();
    assert!(c.retry_tick().is_err());
    assert!(c.retry_checkpoint().is_err());
    let (_, cancelled, events) = c.shutdown().unwrap();
    assert_eq!(cancelled, 0);
    assert!(events.is_empty());
    drop(c);
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), state);
    no_rewards(&c.initial_events().unwrap());
    no_rewards(&reward(&mut c).events);
    assert_eq!(progress(&c).money, 18);
}

#[test]
fn progression_clock_only_ticks_are_persisted_and_cannot_overtake_retry_outbox() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    inject(&h);
    let out = c.pump(c.tick_duration() * 3);
    paused(&out);
    assert_eq!(out.pending_ticks, 2);
    assert!(!c
        .world
        .last_tick()
        .unwrap()
        .events
        .iter()
        .any(|e| matches!(e, n::Event::QuestProgression(_))));
    let state = c.world.state_digest();
    repair(&h);
    c.retry_checkpoint().unwrap();
    let out = c.pump(Duration::ZERO);
    assert_eq!(out.ticks, 0);
    assert_eq!(out.pending_ticks, 2);
    assert!(!out.events.is_empty());
    assert_eq!(c.world.state_digest(), state);
    assert_eq!(c.pump(Duration::ZERO).ticks, 2);
    assert_eq!(c.saved_tick, c.tick());
    assert!(c.world.records().is_empty());
}

struct PreviewAuthority {
    observed: Arc<Mutex<Vec<n::AuthorityRequest>>>,
    denied: bool,
    wrong: bool,
}
impl n::Authority for PreviewAuthority {
    fn resolve(
        &mut self,
        request: &n::AuthorityRequest,
    ) -> Result<n::AuthorityReply, n::NativeError> {
        self.observed.lock().unwrap().push(request.clone());
        quests::QuestAuthority {
            deny: self.denied,
            wrong: self.wrong,
            ..Default::default()
        }
        .resolve(request)
    }
}
#[test]
fn quest_panels_use_current_bound_authority_and_reject_bad_or_denied_replies() {
    for (denied, wrong) in [(false, false), (true, false), (false, true)] {
        let h = Harness::new();
        let observed = Arc::new(Mutex::new(vec![]));
        let mut c = h
            .enter(
                Entry::Create {
                    world: [8; 16],
                    seed: 7,
                },
                quests::profile(),
                PreviewAuthority {
                    observed: observed.clone(),
                    denied,
                    wrong,
                },
            )
            .unwrap();
        let before = c.world.state_digest();
        let result = c.receive(ClientCommand::QuestgiverHello { npc: giver(&c) });
        assert_eq!(result.is_ok(), !denied && !wrong);
        assert_eq!(c.world.state_digest(), before);
        assert!(c.actions.is_empty());
        let calls = observed.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].binding.tick, 0);
        assert_eq!(calls[0].binding.npc, Some(c.world.state().npcs[&3].id));
        assert!(matches!(
            calls[0].query,
            n::Query::QuestInteraction {
                quest: 0,
                reward: false,
                ..
            }
        ));
    }
}

#[test]
fn crash_after_receipt_before_delivery_restores_reward_without_replaying_notifications() {
    let h = Harness::new();
    let mut c = ready(&h);
    inject(&h);
    paused(&reward(&mut c));
    repair(&h);
    c.retry_checkpoint().unwrap();
    let state = c.world.state_digest();
    assert!(c.outbox.as_ref().unwrap().saved.is_some());
    assert!(has_reward(&c.outbox.as_ref().unwrap().events));
    drop(c); // The local event receiver has not consumed this batch; it has no durable ACK.
    let mut c = enter(&h, true);
    assert_eq!(c.world.state_digest(), state);
    assert_eq!(progress(&c).money, 18);
    no_rewards(&c.initial_events().unwrap());
    no_rewards(&reward(&mut c).events);
    assert_eq!(progress(&c).money, 18);
}

#[test]
fn read_only_quest_requests_cannot_overtake_queued_or_retained_owner_input() {
    let h = Harness::new();
    let mut c = enter(&h, false);
    let npc = giver(&c);
    c.receive(ClientCommand::QuestgiverAccept { npc, quest: 70001 })
        .unwrap();
    let before = c.world.state_digest();
    for command in [
        ClientCommand::QuestQuery { quest: 70001 },
        ClientCommand::QuestgiverStatusQuery { npc },
        ClientCommand::QuestgiverHello { npc },
        ClientCommand::QuestgiverQuery { npc, quest: 70001 },
        ClientCommand::QuestgiverComplete { npc, quest: 70001 },
        ClientCommand::QuestgiverRequestReward { npc, quest: 70001 },
    ] {
        assert!(matches!(c.receive(command), Err(Error::Backpressure)));
    }
    assert_eq!(c.actions.len(), 1);
    assert_eq!(c.world.state_digest(), before);
    c.pending = Some(c.next_tick().unwrap());
    assert!(c.actions.is_empty());
    assert!(matches!(
        c.receive(ClientCommand::QuestgiverHello { npc }),
        Err(Error::Backpressure)
    ));
    assert_eq!(c.pending.as_ref().unwrap().inputs.len(), 1);
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert!(out.saved.is_some());
    assert!(c
        .receive(ClientCommand::QuestgiverStatusQuery { npc })
        .is_ok());
}

#[test]
fn denied_status_uses_bound_generic_reactions_and_returns_none_without_mutation() {
    let h = Harness::new();
    let observed = Arc::new(Mutex::new(vec![]));
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            quests::profile(),
            PreviewAuthority {
                observed: observed.clone(),
                denied: true,
                wrong: false,
            },
        )
        .unwrap();
    let before = c.world.state_digest();
    let npc = giver(&c);
    let out = c
        .receive(ClientCommand::QuestgiverStatusQuery { npc })
        .unwrap();
    assert!(matches!(
        &out.events[..],
        [SessionEvent::QuestGiverStatus { status: 0, .. }]
    ));
    assert_eq!(c.world.state_digest(), before);
    assert!(c.actions.is_empty());
    let calls = observed.lock().unwrap();
    assert_eq!(calls.len(), 1);
    assert!(matches!(calls[0].query, n::Query::QuestStatus { .. }));
    assert_eq!(calls[0].binding.tick, 0);
}

#[test]
fn reward_during_prepare_waits_for_save_then_reopens_prepare_and_release() {
    use benilla_sim::classic_spell::CastState;
    let h = Harness::new();
    let enter = |open| {
        h.enter(
            if open {
                Entry::Open { world: [8; 16] }
            } else {
                Entry::Create {
                    world: [8; 16],
                    seed: 7,
                }
            },
            quests::release_profile(false),
            quests::QuestAuthority::default(),
        )
        .unwrap()
    };
    let mut c = enter(false);
    assert!(accept(&mut c).pause.is_none());
    kill(&mut c);
    let target = c.projection.npc_guid(c.world.state().npcs[&4].id).unwrap();
    c.receive(ClientCommand::CastSpell {
        spell_id: 60001,
        target: Some(target),
    })
    .unwrap();
    let started = c.pump(c.tick_duration());
    assert!(started.pause.is_none());
    assert!(started.events.iter().any(|event| matches!(
        event,
        SessionEvent::SpellStart {
            spell_id: 60001,
            cast_time_ms: 350,
            ..
        }
    )));
    let id = *c.world.state().casts.keys().next().unwrap();
    let prepared = c.world.state().casts[&id].binding.clone();
    inject(&h);
    let withheld = reward(&mut c);
    paused(&withheld);
    assert_eq!(c.world.player_level(), 3);
    assert_eq!(c.world.state().casts[&id].binding, prepared);
    assert_eq!(
        c.world.state().casts[&id].execution.state(),
        CastState::Casting { remaining_ms: 250 }
    );
    let state = c.world.state_digest();
    repair(&h);
    let receipt = c.retry_checkpoint().unwrap();
    assert_eq!(receipt.tick, c.tick());
    let published = c.pump(Duration::ZERO);
    assert_eq!(
        published
            .events
            .iter()
            .filter(|event| matches!(event, SessionEvent::QuestComplete(_)))
            .count(),
        1
    );
    assert_eq!(
        published
            .events
            .iter()
            .filter(|event| matches!(event, SessionEvent::LevelUp(_)))
            .count(),
        1
    );
    assert!(!published.events.iter().any(|event| matches!(
        event,
        SessionEvent::SpellGo { .. } | SessionEvent::SpellFailedOther { .. }
    )));
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut c = enter(true);
    assert_eq!(c.world.state_digest(), state);
    let initial = c.initial_events().unwrap();
    no_rewards(&initial);
    assert!(initial.iter().any(|event| matches!(
        event,
        SessionEvent::SpellStart {
            spell_id: 60001,
            cast_time_ms: 250,
            ..
        }
    )));
    let mut launched = 0;
    for _ in 0..10 {
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(out.saved.unwrap().tick, c.tick());
        launched += out
            .events
            .iter()
            .filter(|event| {
                matches!(
                    event,
                    SessionEvent::SpellGo {
                        spell_id: 60001,
                        ..
                    }
                )
            })
            .count();
        assert!(!out.events.iter().any(|event| matches!(
            event,
            SessionEvent::SpellGo {
                spell_id: 60005,
                ..
            }
        )));
        if matches!(
            c.world.state().casts[&id].execution.state(),
            CastState::Traveling { .. }
        ) {
            break;
        }
    }
    assert_eq!(launched, 1);
    let cast = &c.world.state().casts[&id];
    assert_eq!(cast.binding, prepared);
    assert_eq!(
        cast.release_binding.as_ref().unwrap().captured_ms - prepared.captured_ms,
        400
    );
    let flight = c.world.state_digest();
    drop(c);
    let mut c = enter(true);
    assert_eq!(c.world.state_digest(), flight);
    assert!(!c.initial_events().unwrap().iter().any(|event| matches!(
        event,
        SessionEvent::SpellStart { .. } | SessionEvent::SpellGo { .. }
    )));
    for _ in 0..20 {
        if !c.world.state().casts.contains_key(&id) {
            break;
        }
        let out = c.pump(c.tick_duration());
        assert!(out.pause.is_none(), "{:?}", out.pause);
        assert_eq!(out.saved.unwrap().tick, c.tick());
        assert!(!out
            .events
            .iter()
            .any(|event| matches!(event, SessionEvent::SpellGo { .. })));
    }
    assert!(!c.world.state().casts.contains_key(&id));
    assert_eq!(c.world.player_level(), 3);
}
