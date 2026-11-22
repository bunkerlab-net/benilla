// SPDX-License-Identifier: GPL-3.0-or-later
//! Exact controller/store/projection with authored NPC-owned SpellStart fixtures.
use super::*;
use n::synthetic_slow_fixture::{self as slow, SyntheticNpcSpellAuthority, SyntheticSlowTrigger};
fn data() -> n::Profile {
    slow::with_triggers(vec![SyntheticSlowTrigger {
        tick: 1,
        spell_id: 61004,
        caster: runtime::NpcId {
            spawn: 2,
            generation: 0,
        },
        after_npc_update: true,
    }])
}
fn start(h: &Harness, data: n::Profile, open: bool) -> NativeController {
    h.enter(
        if open {
            Entry::Open { world: [8; 16] }
        } else {
            Entry::Create {
                world: [8; 16],
                seed: 7,
            }
        },
        data,
        SyntheticNpcSpellAuthority::default(),
    )
    .unwrap()
}
fn speed_count(events: &[SessionEvent]) -> usize {
    events
        .iter()
        .filter(|e| matches!(e, SessionEvent::ForceSpeedChange { .. }))
        .count()
}
#[test]
fn npc_trigger_commit_retry_preserves_owned_finished_event_and_publishes_effect_once() {
    let h = Harness::new();
    let profile = data();
    let mut c = start(&h, profile.clone(), false);
    c.initial_events().unwrap();
    let generation = c.store.generation();
    fs::write(
        h.path().join(".world-checkpoint.pending"),
        b"authored NPC SpellStart save fault",
    )
    .unwrap();
    let paused = c.pump(c.tick_duration());
    assert!(matches!(
        paused.pause.unwrap().kind,
        PauseKind::Checkpoint {
            requires_reopen: false
        }
    ));
    assert!(paused.events.is_empty());
    assert_eq!(c.store.generation(), generation);
    let state = c.world.state_digest();
    let lifetime = c.world.state().npcs[&2]
        .events
        .spells()
        .next()
        .unwrap()
        .clone();
    assert_eq!(
        lifetime.terminal,
        Some(n::NpcSpellTerminal::Hit {
            effect_applied: true
        })
    );
    let holder = c
        .world
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .unwrap()
        .id;
    assert_eq!(speed_count(&c.outbox.as_ref().unwrap().events), 3);
    fs::remove_file(h.path().join(".world-checkpoint.pending")).unwrap();
    c.retry_checkpoint().unwrap();
    assert_eq!(c.world.state_digest(), state);
    let published = c.pump(Duration::ZERO);
    assert!(published.pause.is_none());
    assert_eq!(speed_count(&published.events), 3);
    assert_eq!(
        published
            .events
            .iter()
            .filter(|e| matches!(
                e,
                SessionEvent::AuraDuration {
                    slot: 32,
                    remaining_ms: 400
                }
            ))
            .count(),
        1
    );
    assert!(c.pump(Duration::ZERO).events.is_empty());
    drop(c);
    let mut reopened = start(&h, profile, true);
    assert_eq!(reopened.world.state_digest(), state);
    assert_eq!(
        reopened.world.state().npcs[&2].events.spells().next(),
        Some(&lifetime)
    );
    assert_eq!(speed_count(&reopened.initial_events().unwrap()), 3);
    let next = reopened.pump(reopened.tick_duration());
    assert!(next.pause.is_none());
    assert_eq!(speed_count(&next.events), 0);
    assert!(reopened.world.state().npcs[&2]
        .events
        .spells()
        .next()
        .is_none());
    assert_eq!(
        reopened
            .world
            .state()
            .player_slow
            .as_ref()
            .unwrap()
            .holders()
            .next()
            .unwrap()
            .id,
        holder
    );
    drop(reopened);
    let reopened = start(&h, data(), true);
    assert!(reopened.world.state().npcs[&2]
        .events
        .spells()
        .next()
        .is_none());
}
#[test]
fn strict_failure_is_durable_cleanup_without_aura_or_speed_publication() {
    let h = Harness::new();
    let profile = data();
    let mut c = h
        .enter(
            Entry::Create {
                world: [8; 16],
                seed: 7,
            },
            profile.clone(),
            SyntheticNpcSpellAuthority {
                fail: Some(n::NpcSpellCheck::TargetNotTaxiFlying),
                ..Default::default()
            },
        )
        .unwrap();
    c.initial_events().unwrap();
    let out = c.pump(c.tick_duration());
    assert!(out.pause.is_none());
    assert_eq!(speed_count(&out.events), 0);
    assert_eq!(
        c.world.state().npcs[&2]
            .events
            .spells()
            .next()
            .unwrap()
            .terminal,
        Some(n::NpcSpellTerminal::BadTargets)
    );
    let state = c.world.state_digest();
    drop(c);
    let mut reopened = start(&h, profile, true);
    assert_eq!(reopened.world.state_digest(), state);
    reopened.initial_events().unwrap();
    assert!(reopened.pump(reopened.tick_duration()).pause.is_none());
    assert!(reopened.world.state().npcs[&2]
        .events
        .spells()
        .next()
        .is_none());
    assert!(reopened
        .world
        .state()
        .player_slow
        .as_ref()
        .unwrap()
        .holders()
        .next()
        .is_none());
}
