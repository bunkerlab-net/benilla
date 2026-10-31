// SPDX-License-Identifier: GPL-3.0-or-later
// Pinned-source synthetic corpse-timer differential tests.
use benilla_sim::creature_ai::{movement::Revision, *};
fn real(v: f32) -> Real {
    Real::new(v).unwrap()
}
fn point(x: f32) -> Point {
    Point::new(x, 0., 0.).unwrap()
}
fn profile(reaction: Reaction) -> Profile {
    Profile {
        creature_id: 1,
        map_id: 7,
        content_revision: Revision([3; 32]),
        spawn: point(0.),
        clock_origin_ms: 0,
        level: 1,
        reaction,
        neutral_to_all: false,
        detection: real(18.),
        aggro_rate: Real::ONE,
        combat_reach: real(1.5),
        bounding_radius: real(0.5),
        melee_leeway: real(2.66),
        run_speed: real(7.),
        home_speed: real(7.),
        mainhand_interval_ms: 2000,
        pursuit_ms: 15000,
        leash_radius: real(30.),
        hard_leash_radius: None,
        corpse_decay_ms: 3000,
        respawn_delay_seconds: 5,
        unsupported: vec![],
    }
}

struct Authority;
impl CombatAuthority for Authority {
    fn observe(&mut self, _: u64) -> Result<Observation, AiError> {
        Ok(Observation {
            sequence: 1,
            elapsed_ms: 1,
            geometry_revision: Revision([4; 32]),
            target: None,
            can_initiate_attack: false,
            casting_non_melee: false,
            stationary_melee_refresh_eligible: false,
            creature_moving_and_running: false,
            movement: None,
            receipts: vec![Receipt {
                id: 1,
                event: CombatEvent::Died,
            }],
        })
    }
}
fn corpse(death: u64, decay: u32) -> (SoloCreatureAi, Profile) {
    let mut profile = profile(Reaction::Aggressive);
    profile.clock_origin_ms = death - 1;
    profile.corpse_decay_ms = decay;
    let mut ai = SoloCreatureAi::new(profile.clone()).unwrap();
    assert_eq!(ai.step(&mut Authority).unwrap(), vec![Effect::Died]);
    assert_eq!(ai.death_at_ms(), Some(death));
    (ai, profile)
}
#[test]
fn exact_pinned_inspect_and_reduce_corpse_expiration() {
    let mut count = 0;
    for line in include_str!("fixtures/loot_timer.tsv").lines() {
        let n = line
            .split_whitespace()
            .map(|s| s.parse::<u64>().unwrap())
            .collect::<Vec<_>>();
        let (mut ai, _) = corpse(n[0], n[1] as u32);
        ai.adjust_loot_corpse_timer(CorpseTimerCall {
            at_ms: n[0] + n[2],
            delay_ms: n[3] as u32,
            inspecting: n[4] != 0,
        })
        .unwrap();
        assert_eq!(ai.corpse_expiration_ms(), n[5], "{line}");
        count += 1;
    }
    assert_eq!(count, 800);
}
#[test]
fn timer_callbacks_round_trip_and_reject_backwards_without_mutation() {
    let (mut ai, profile) = corpse(2000, 3000);
    for (delay_ms, inspecting, expiration) in [
        (8000, true, 10000),
        (9000, false, 10000),
        (2000, false, 4000),
        (1, true, 4000),
    ] {
        ai.adjust_loot_corpse_timer(CorpseTimerCall {
            at_ms: 2000,
            delay_ms,
            inspecting,
        })
        .unwrap();
        assert_eq!(ai.corpse_expiration_ms(), expiration);
        let restored =
            SoloCreatureAi::restore(ai.checkpoint(), profile.clone(), 2000, 1, 2).unwrap();
        assert_eq!(ai, restored);
    }
    let before = ai.clone();
    assert!(ai
        .adjust_loot_corpse_timer(CorpseTimerCall {
            at_ms: 1999,
            delay_ms: 100,
            inspecting: true
        })
        .is_err());
    assert_eq!(ai, before);
    assert!(ai
        .adjust_loot_corpse_timer(CorpseTimerCall {
            at_ms: 2000,
            delay_ms: 0,
            inspecting: true
        })
        .is_err());
    assert_eq!(ai, before);
}
