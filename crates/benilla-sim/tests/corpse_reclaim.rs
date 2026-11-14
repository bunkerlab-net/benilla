// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored scalar/component evidence, not a released or resurrected native world.
use benilla_sim::corpse_reclaim::*;

fn input() -> Input {
    let location = Location {
        map_instance: 11,
        in_world: true,
        position: [0.0; 3],
    };
    Input {
        policy: Policy::checked(Some(false), Some(true)).unwrap(),
        player: Player {
            id: 7,
            alive: false,
            ghost: true,
            location,
            combat_reach: 1.5,
        },
        corpse: Some(RegisteredCorpse {
            id: 9,
            owner: 7,
            location,
            ghost_time_seconds: 970,
            kind: CorpseKind::ResurrectablePve,
        }),
        death_expire_time_seconds: 1000,
        clocks: Clocks {
            delay_now_seconds: 1000,
            comparison_now_seconds: 1000,
        },
    }
}
fn calls(input: Input) -> bool {
    matches!(prepare(input).unwrap(), Decision::Calls(_))
}

#[test]
fn source_oracle_matches_reclaim_guards_float_boundaries_and_delegated_order() {
    let fixture = include_str!("fixtures/corpse_reclaim_cmangos.tsv");
    assert_eq!(fixture.lines().next().unwrap().split_whitespace().collect::<Vec<_>>().join(" "), "alive ghost present enabled expire delay_now compare_now ghost_time player_world corpse_world player_map corpse_map px_bits py_bits pz_bits cx_bits cy_bits cz_bits reach_bits accepted actions");
    let mut count = 0;
    for line in fixture
        .lines()
        .skip(1)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
    {
        let words: Vec<_> = line.split_whitespace().collect();
        assert_eq!(words.len(), 21, "{line}");
        let number = |i: usize| words[i].parse::<u64>().unwrap();
        let scalar = |i| f32::from_bits(u32::try_from(number(i)).unwrap());
        let mut value = input();
        value.player.alive = number(0) != 0;
        value.player.ghost = number(1) != 0;
        value.policy = Policy::checked(Some(false), Some(number(3) != 0)).unwrap();
        value.death_expire_time_seconds = number(4);
        value.clocks = Clocks {
            delay_now_seconds: number(5),
            comparison_now_seconds: number(6),
        };
        value.player.location = Location {
            in_world: number(8) != 0,
            map_instance: number(10),
            position: [scalar(12), scalar(13), scalar(14)],
        };
        value.player.combat_reach = scalar(18);
        value.corpse = (number(2) != 0).then_some(RegisteredCorpse {
            id: 9,
            owner: 7,
            kind: CorpseKind::ResurrectablePve,
            ghost_time_seconds: number(7),
            location: Location {
                in_world: number(9) != 0,
                map_instance: number(11),
                position: [scalar(15), scalar(16), scalar(17)],
            },
        });
        let decision = prepare(value).unwrap();
        assert_eq!(
            matches!(decision, Decision::Calls(_)),
            number(19) != 0,
            "{line}"
        );
        match decision {
            Decision::Calls(plan) => {
                assert_eq!((plan.player(), plan.corpse()), (7, 9));
                assert_eq!(
                    plan.actions(),
                    &[
                        Action::ResurrectPlayer {
                            restore_percent_bits: 0.5f32.to_bits(),
                            apply_sickness: false,
                        },
                        Action::SpawnCorpseBones
                    ]
                );
                assert_eq!(words[20], "resurrect_half_no_sickness,bones");
            }
            Decision::Rejected(_) => assert_eq!(words[20], "-"),
        }
        count += 1;
    }
    assert_eq!(count, 355, "oracle coverage changed");
}

#[test]
fn guard_precedence_does_not_read_unreached_clock_or_geometry() {
    let mut value = input();
    value.player.alive = true;
    value.player.ghost = false;
    value.player.location.position = [f32::NAN; 3];
    value.clocks = Clocks {
        delay_now_seconds: u64::MAX,
        comparison_now_seconds: 0,
    };
    value.corpse = None;
    assert_eq!(prepare(value), Ok(Decision::Rejected(Rejection::Alive)));
    value.player.alive = false;
    assert_eq!(prepare(value), Ok(Decision::Rejected(Rejection::NotGhost)));
    value.player.ghost = true;
    assert_eq!(
        prepare(value),
        Ok(Decision::Rejected(Rejection::MissingCorpse))
    );
    value = input();
    value.corpse.as_mut().unwrap().ghost_time_seconds = 1000;
    value.player.location.position = [f32::NAN; 3];
    assert_eq!(prepare(value), Ok(Decision::Rejected(Rejection::Delay)));
    value.corpse.as_mut().unwrap().ghost_time_seconds = 970;
    value.player.location.in_world = false;
    assert_eq!(
        prepare(value),
        Ok(Decision::Rejected(Rejection::MapOrDistance))
    );
}

#[test]
fn dynamic_delay_and_distinct_time_samples_are_not_a_frozen_notification() {
    let mut value = input();
    value.death_expire_time_seconds = 1300;
    value.clocks.comparison_now_seconds = 1029;
    assert!(!calls(value)); // delay sample1000 =>60, ghost970 =>1030.
    value.clocks.comparison_now_seconds = 1030;
    assert!(calls(value));
    value.clocks = Clocks {
        delay_now_seconds: 1001,
        comparison_now_seconds: 1001,
    };
    assert!(calls(value)); // sample crosses expiration-step boundary, delay becomes30.
    value.death_expire_time_seconds = (1u64 << 32) + 1300;
    value.clocks = Clocks {
        delay_now_seconds: 1000,
        comparison_now_seconds: 1030,
    };
    assert!(calls(value)); // source uint32 conversion BEFORE division yields60, not120.
}

#[test]
fn strict_three_dimensional_combat_reach_and_instance_identity() {
    let mut value = input();
    let edge = 40.5f32;
    value.player.location.position[2] = edge;
    assert!(!calls(value));
    value.player.location.position[2] = f32::from_bits(edge.to_bits() - 1);
    assert!(calls(value));
    value.player.location.position[2] = f32::from_bits(edge.to_bits() + 1);
    assert!(!calls(value));
    value.player.location.position = [0.0; 3];
    value.player.location.map_instance = 12;
    assert!(!calls(value));
}

#[test]
fn missing_policy_and_unsupported_corpse_domains_fail_closed() {
    assert_eq!(
        Policy::checked(None, Some(true)),
        Err(Error::MissingPolicyEvidence)
    );
    assert_eq!(
        Policy::checked(Some(false), None),
        Err(Error::MissingPolicyEvidence)
    );
    assert_eq!(
        Policy::checked(Some(true), Some(true)),
        Err(Error::UnsupportedBattleground)
    );
    for kind in [CorpseKind::Bones, CorpseKind::ResurrectablePvp] {
        let mut value = input();
        value.corpse.as_mut().unwrap().kind = kind;
        assert_eq!(prepare(value), Err(Error::UnsupportedCorpse));
    }
    let mut value = input();
    value.corpse.as_mut().unwrap().owner = 8;
    assert_eq!(prepare(value), Err(Error::WrongCorpseOwner));
    value = input();
    value.corpse.as_mut().unwrap().id = 0;
    assert_eq!(prepare(value), Err(Error::InvalidIdentity));
}

#[test]
fn reached_invalid_scalars_and_clock_overflows_do_not_create_plans() {
    for reach in [f32::NAN, f32::INFINITY, -1.0, f32::from_bits(1)] {
        let mut value = input();
        value.player.combat_reach = reach;
        assert_eq!(prepare(value), Err(Error::InvalidReach));
    }
    let mut value = input();
    value.player.location.position[0] = f32::INFINITY;
    assert_eq!(prepare(value), Err(Error::InvalidLocation));
    value = input();
    value.player.location.position[0] = f32::from_bits(1);
    assert_eq!(prepare(value), Err(Error::InvalidLocation));
    value = input();
    value.player.location.position[0] = f32::MAX;
    assert_eq!(prepare(value), Err(Error::InvalidLocation));
    value = input();
    value.player.combat_reach = 1.0e10;
    assert_eq!(prepare(value), Err(Error::ArithmeticDomain));
    value = input();
    value.player.location.map_instance = 0;
    value.corpse.as_mut().unwrap().location.map_instance = 0;
    assert_eq!(prepare(value), Err(Error::InvalidLocation));
    value = input();
    value.clocks.comparison_now_seconds = 999;
    assert_eq!(prepare(value), Ok(Decision::Rejected(Rejection::Delay)));
    value = input();
    value.death_expire_time_seconds = u64::MAX;
    assert_eq!(prepare(value), Err(Error::ClockDomain));
    value = input();
    value.corpse.as_mut().unwrap().ghost_time_seconds = i64::MAX as u64;
    assert_eq!(prepare(value), Err(Error::ClockOverflow));
}

#[test]
fn call_plan_has_no_implicit_world_mutation_or_exactly_once_authority() {
    let value = input();
    let first = prepare(value).unwrap();
    assert!(matches!(first, Decision::Calls(_)));
    assert_eq!(prepare(value).unwrap(), first); // Pure observation, not a consumed capability.
    let mut after_actual_host_recovery = value;
    after_actual_host_recovery.player.alive = true;
    assert_eq!(
        prepare(after_actual_host_recovery),
        Ok(Decision::Rejected(Rejection::Alive))
    );
}
