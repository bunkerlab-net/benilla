// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::creature_regeneration::*;
fn policy() -> Policy {
    Policy {
        kind: CreatureKind::Ordinary,
        regenerate_stats: 2,
        creature_type_flags: 0,
        unit_flags: 0,
        power: PowerEvidence::Known {
            power_type: 0,
            max_power: 0,
        },
    }
}
#[test]
fn source_extracted_health_timer_combat_and_flag_oracle() {
    let mut count = 0;
    for line in include_str!("fixtures/creature_regeneration.tsv").lines() {
        let v: Vec<u32> = line.split('\t').map(|x| x.parse().unwrap()).collect();
        let mut p = policy();
        p.regenerate_stats = v[4];
        p.creature_type_flags = v[5];
        let after = update(
            p,
            v[0],
            State {
                health: v[1],
                elapsed_ms: v[2],
            },
            Frame {
                diff_ms: v[3],
                alive_update: true,
                in_combat: v[6] & 1 != 0,
                evade_regeneration: v[6] & 2 != 0,
            },
        )
        .unwrap();
        assert_eq!((after.health, after.elapsed_ms), (v[7], v[8]), "{line}");
        assert_eq!(
            v[9],
            u32::from(v[2].wrapping_add(v[3]) >= REGEN_INTERVAL_MS)
        );
        count += 1;
    }
    assert_eq!(count, 13440);
}
#[test]
fn missing_controlled_overridden_and_relevant_power_are_explicit() {
    let p = policy();
    for kind in [
        CreatureKind::Unknown,
        CreatureKind::PetOrRegenerationOverride,
    ] {
        assert!(matches!(
            Policy { kind, ..p }.validate(),
            Err(Error::Unsupported(_))
        ));
    }
    assert!(matches!(
        Policy { unit_flags: 8, ..p }.validate(),
        Err(Error::Unsupported(_))
    ));
    assert!(matches!(
        Policy {
            power: PowerEvidence::Unknown,
            ..p
        }
        .validate(),
        Err(Error::Unsupported(_))
    ));
    for power_type in 0..=4 {
        for maximum in [0, 1, 1000] {
            let candidate = Policy {
                regenerate_stats: 14,
                power: PowerEvidence::Known {
                    power_type,
                    max_power: maximum,
                },
                ..p
            };
            assert_eq!(
                candidate.validate().is_ok(),
                maximum == 0 || matches!(power_type, 1 | 4)
            );
            assert!(Policy {
                regenerate_stats: 2,
                ..candidate
            }
            .validate()
            .is_ok());
        }
    }
    let mut json = serde_json::to_value(p).unwrap();
    json.as_object_mut().unwrap().remove("unit_flags");
    assert!(serde_json::from_value::<Policy>(json).is_err());
}
#[test]
fn corpse_respawn_and_dead_after_unit_update_freeze_timer_and_health() {
    for health in [0, 1, 100] {
        let before = State {
            elapsed_ms: 4999,
            health,
        };
        assert_eq!(
            update(
                policy(),
                100,
                before,
                Frame {
                    diff_ms: 10001,
                    alive_update: false,
                    in_combat: false,
                    evade_regeneration: false
                }
            )
            .unwrap(),
            before
        );
    }
    assert!(update(
        policy(),
        100,
        State {
            elapsed_ms: 4999,
            health: 0
        },
        Frame {
            diff_ms: 1,
            alive_update: true,
            in_combat: false,
            evade_regeneration: false
        }
    )
    .is_err());
}
#[test]
fn large_update_is_one_pulse_not_catchup_and_full_or_blocked_still_consumes_interval() {
    let before = State {
        health: 1,
        elapsed_ms: 4999,
    };
    let frame = Frame {
        diff_ms: 10001,
        alive_update: true,
        in_combat: false,
        evade_regeneration: false,
    };
    assert_eq!(
        update(policy(), 100, before, frame).unwrap(),
        State {
            health: 34,
            elapsed_ms: 10000
        }
    );
    assert_eq!(
        update(
            policy(),
            100,
            State {
                health: 100,
                ..before
            },
            frame
        )
        .unwrap(),
        State {
            health: 100,
            elapsed_ms: 10000
        }
    );
    assert_eq!(
        update(
            policy(),
            100,
            before,
            Frame {
                in_combat: true,
                ..frame
            }
        )
        .unwrap(),
        State {
            health: 1,
            elapsed_ms: 10000
        }
    );
}
