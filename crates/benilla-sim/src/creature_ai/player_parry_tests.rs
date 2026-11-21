// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn profile() -> Profile {
    Profile {
        creature_id: 1,
        map_id: 7,
        content_revision: Revision([3; 32]),
        spawn: Point::new(0.0, 0.0, 0.0).unwrap(),
        clock_origin_ms: 0,
        level: 1,
        reaction: Reaction::Defensive,
        neutral_to_all: false,
        detection: Real::new(18.0).unwrap(),
        aggro_rate: Real::ONE,
        combat_reach: Real::new(1.5).unwrap(),
        bounding_radius: Real::new(0.5).unwrap(),
        melee_leeway: Real::from_bits(0x402a_aaab).unwrap(),
        run_speed: Real::new(7.0).unwrap(),
        home_speed: Real::new(7.0).unwrap(),
        mainhand_interval_ms: 2000,
        pursuit_ms: 15_000,
        leash_radius: Real::new(30.0).unwrap(),
        hard_leash_radius: None,
        corpse_decay_ms: 200,
        respawn_delay_seconds: 5,
        unsupported: vec![],
    }
}

#[test]
fn actual_owned_mainhand_parry_changes_only_source_timer() {
    for (before, after) in [
        (0, 0),
        (400, 400),
        (401, 400),
        (1200, 400),
        (1201, 401),
        (2000, 1200),
    ] {
        let mut ai = SoloCreatureAi::new(profile()).unwrap();
        ai.mainhand_ms = before;
        let mut expected = ai.clone();
        expected.mainhand_ms = after;
        ai.apply_player_mainhand_parry_haste().unwrap();
        assert_eq!(ai, expected);
    }
}

#[test]
fn player_parry_rejects_dead_pending_or_invalid_state_atomically() {
    for phase in [Phase::Corpse, Phase::Despawned] {
        let mut ai = SoloCreatureAi::new(profile()).unwrap();
        ai.phase = phase;
        ai.mainhand_ms = 2000;
        let before = ai.clone();
        assert!(ai.apply_player_mainhand_parry_haste().is_err());
        assert_eq!(ai, before);
    }
    let mut ai = SoloCreatureAi::new(profile()).unwrap();
    ai.pending_receipts = Some((1, 0));
    let before = ai.clone();
    assert!(ai.apply_player_mainhand_parry_haste().is_err());
    assert_eq!(ai, before);
    ai.pending_receipts = None;
    ai.mainhand_ms = 1_000_001;
    let before = ai.clone();
    assert!(ai.apply_player_mainhand_parry_haste().is_err());
    assert_eq!(ai, before);
    let mut offhand = profile();
    offhand.unsupported.push(Unsupported::OffhandOrExtraAttacks);
    assert!(SoloCreatureAi::new(offhand).is_err());
}
