// SPDX-License-Identifier: GPL-3.0-or-later
// Original numeric probes; independent source-oracle provenance in ../PROVENANCE.md.
use benilla_sim::quest::*;

fn rows(text: &str) -> impl Iterator<Item = Vec<i64>> + '_ {
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            line.split_whitespace()
                .map(|value| value.parse().unwrap())
                .collect()
        })
}
fn xp(quest_level: u32, reward_money_max_level: u32) -> QuestXpInput {
    QuestXpInput {
        quest_level,
        reward_money_max_level,
    }
}
fn input() -> QuestRewardInput {
    QuestRewardInput {
        xp: xp(1, 60),
        reward_or_required_money: 10,
        quest_flags: 0,
    }
}
fn context() -> RewardContext {
    RewardContext {
        player_level: 1,
        max_player_level: 60,
        xp_rate: Binary32Rate::ONE,
        money_rate: Binary32Rate::ONE,
    }
}

#[test]
fn quest_xp_matches_pinned_cpp_body() {
    let mut count = 0;
    for row in rows(include_str!("fixtures/quest_xp_cmangos.tsv")) {
        assert_eq!(row.len(), 4);
        let player = if row[1] == 0 {
            None
        } else {
            Some(row[1] as u8)
        };
        assert_eq!(
            quest_xp_value(xp(row[0] as u32, row[2] as u32), player).unwrap(),
            row[3] as u32,
            "{row:?}"
        );
        count += 1;
    }
    assert_eq!(count, 3298);
}

#[test]
fn signed_money_matches_pinned_cpp_getter() {
    let mut count = 0;
    for row in rows(include_str!("fixtures/quest_money_cmangos.tsv")) {
        assert_eq!(row.len(), 3);
        let rate = Binary32Rate::from_bits(row[1] as u32).unwrap();
        assert_eq!(
            reward_or_required_money(row[0] as i32, rate).unwrap(),
            row[2] as i32,
            "{row:?}"
        );
        count += 1;
    }
    assert_eq!(count, 144);
}

#[test]
fn reward_quote_matches_unchanged_cpp_reward_fragment() {
    let mut count = 0;
    for row in rows(include_str!("fixtures/quest_reward_cmangos.tsv")) {
        assert_eq!(row.len(), 12);
        let input = QuestRewardInput {
            xp: xp(row[0] as u32, row[3] as u32),
            reward_or_required_money: row[4] as i32,
            quest_flags: row[5] as u16,
        };
        let context = RewardContext {
            player_level: row[1] as u8,
            max_player_level: row[2] as u8,
            xp_rate: Binary32Rate::from_bits(row[6] as u32).unwrap(),
            money_rate: Binary32Rate::from_bits(row[7] as u32).unwrap(),
        };
        let result = reward_quote(input, context).unwrap();
        assert_eq!(result.display_xp, row[8] as u32, "{row:?}");
        assert_eq!(
            result.xp_to_give.map(i64::from).unwrap_or(-1),
            row[9],
            "{row:?}"
        );
        assert_eq!(
            result.max_level_money.map(i64::from).unwrap_or(-1),
            row[10],
            "{row:?}"
        );
        assert_eq!(result.reward_or_required_money, row[11] as i32, "{row:?}");
        assert_eq!(
            result.money_required_before_reward,
            (-row[4]).max(0) as u32,
            "{row:?}"
        );
        count += 1;
    }
    assert_eq!(count, 8640);
}

#[test]
fn three_independent_gates_match_pinned_cpp() {
    let mut count = 0;
    for row in rows(include_str!("fixtures/quest_gates_cmangos.tsv")) {
        assert_eq!(row.len(), 10);
        let input = QuestEligibilityInput {
            minimum_level: row[3] as u32,
            maximum_level: row[4] as u32,
            required_classes: row[5] as u32,
            required_races: row[6] as u32,
        };
        let player = QuestPlayerIdentity {
            level: row[0] as u8,
            race: row[1] as u8,
            class: row[2] as u8,
        };
        assert_eq!(
            eligibility_gates(input, player).unwrap(),
            QuestEligibilityGates {
                level: row[7] != 0,
                class: row[8] != 0,
                race: row[9] != 0,
            },
            "{row:?}"
        );
        count += 1;
    }
    assert_eq!(count, 4032);
}

#[test]
fn source_rounding_is_not_replaced_with_rational_arithmetic() {
    // Exact rational 60/3.6 * 0.6 equals ten; source float steps round above ten.
    assert_eq!(quest_xp_value(xp(63, 60), Some(70)).unwrap(), 11);
    // Integer -> float rounds BEFORE division (2^24+1 is not exact binary32).
    assert_eq!(
        quest_xp_value(xp(1, 16_777_217), Some(1)).unwrap(),
        27_962_026
    );
    assert_eq!(
        reward_or_required_money(16_777_217, Binary32Rate::ONE).unwrap(),
        16_777_216
    );
    // Ceil happens before XP rate scaling; do not scale fullxp and ceil once.
    let result = reward_quote(
        QuestRewardInput {
            xp: xp(1, 1),
            ..input()
        },
        RewardContext {
            xp_rate: Binary32Rate::from_bits(0x3f00_0000).unwrap(),
            ..context()
        },
    )
    .unwrap();
    assert_eq!(result.display_xp, 1);
}

#[test]
fn cap_conversion_does_not_suppress_display_xp_or_scale_costs() {
    let cost = QuestRewardInput {
        reward_or_required_money: -100,
        ..input()
    };
    let capped = RewardContext {
        player_level: 60,
        money_rate: Binary32Rate::from_bits(0x4000_0000).unwrap(),
        ..context()
    };
    let quote = reward_quote(cost, capped).unwrap();
    assert_eq!(quote.display_xp, 10);
    assert_eq!(quote.xp_to_give, None);
    assert_eq!(quote.max_level_money, Some(120));
    assert_eq!(quote.reward_or_required_money, -100);
    assert_eq!(quote.money_required_before_reward, 100);
    let flagged = reward_quote(
        QuestRewardInput {
            quest_flags: QUEST_FLAGS_NO_MONEY_FROM_XP,
            ..cost
        },
        capped,
    )
    .unwrap();
    assert_eq!(flagged.display_xp, quote.display_xp);
    assert_eq!(flagged.max_level_money, Some(0));
    assert_eq!(flagged.reward_or_required_money, -100);
    let before_cap = reward_quote(
        cost,
        RewardContext {
            player_level: 59,
            ..capped
        },
    )
    .unwrap();
    assert_eq!(before_cap.xp_to_give, Some(10));
    assert_eq!(before_cap.max_level_money, None);
    // Configured cap is explicit, not a hard-coded 60 or selected export range.
    assert_eq!(
        reward_quote(
            input(),
            RewardContext {
                max_player_level: 1,
                ..context()
            }
        )
        .unwrap()
        .max_level_money,
        Some(60)
    );
    assert_eq!(
        reward_quote(
            input(),
            RewardContext {
                player_level: 255,
                max_player_level: 255,
                ..context()
            }
        )
        .unwrap()
        .xp_to_give,
        None
    );
    assert_eq!(
        reward_quote(
            input(),
            RewardContext {
                xp_rate: Binary32Rate::ZERO,
                ..context()
            }
        )
        .unwrap()
        .xp_to_give,
        Some(0)
    );
}

#[test]
fn reject_undefined_integer_casts_without_saturation_or_wrap() {
    assert_eq!(
        quest_xp_value(xp(1, u32::MAX), Some(1)).unwrap_err().field,
        "xp_value"
    );
    // Penalty can bring that same intermediate value back into the cast domain.
    assert!(quest_xp_value(xp(1, u32::MAX), Some(255)).is_ok());
    assert_eq!(
        reward_or_required_money(i32::MAX, Binary32Rate::ONE)
            .unwrap_err()
            .field,
        "reward_money"
    );
    assert_eq!(
        reward_or_required_money(i32::MAX, Binary32Rate::ZERO).unwrap(),
        0
    );
    assert_eq!(
        reward_or_required_money(i32::MIN, Binary32Rate::ONE).unwrap(),
        i32::MIN
    );
    assert_eq!(
        reward_quote(
            QuestRewardInput {
                reward_or_required_money: i32::MIN,
                ..input()
            },
            context()
        )
        .unwrap_err()
        .field,
        "required_money_negation"
    );
    assert_eq!(
        reward_quote(
            QuestRewardInput {
                xp: xp(65, u32::MAX),
                ..input()
            },
            RewardContext {
                player_level: 60,
                ..context()
            }
        )
        .unwrap_err()
        .field,
        "max_level_money"
    );
    assert_eq!(
        reward_quote(
            QuestRewardInput {
                xp: xp(1, 2_000_000_000),
                ..input()
            },
            RewardContext {
                xp_rate: Binary32Rate::from_bits(0x4000_0000).unwrap(),
                ..context()
            }
        )
        .unwrap_err()
        .field,
        "rated_xp"
    );
    // Display XP is computed even at cap; do not skip a malformed rated result.
    assert_eq!(
        reward_quote(
            QuestRewardInput {
                xp: xp(60, 2_000_000_000),
                ..input()
            },
            RewardContext {
                player_level: 60,
                xp_rate: Binary32Rate::from_bits(0x4000_0000).unwrap(),
                ..context()
            }
        )
        .unwrap_err()
        .field,
        "rated_xp"
    );
}

#[test]
fn reject_bad_rates_levels_and_identity_without_invented_defaults() {
    for bits in [
        1,
        0x007f_ffff,
        0x8000_0000,
        0xbf80_0000,
        0x447a_0001,
        0x7f80_0000,
        0x7fc0_0000,
        0xff80_0000,
    ] {
        assert!(Binary32Rate::from_bits(bits).is_err(), "{bits:08x}");
    }
    for bits in [0, 0x0080_0000, 0x3f80_0000, 0x447a_0000] {
        assert_eq!(Binary32Rate::from_bits(bits).unwrap().to_bits(), bits);
    }
    assert!(quest_xp_value(xp(256, 0), None).is_err());
    assert!(quest_xp_value(xp(u32::MAX, 1), Some(1)).is_err());
    assert!(quest_xp_value(xp(1, 0), Some(0)).is_err());
    assert_eq!(quest_xp_value(xp(255, u32::MAX), None).unwrap(), 0);
    assert_eq!(quest_xp_value(xp(0, u32::MAX), Some(255)).unwrap(), 0);
    assert!(reward_quote(
        input(),
        RewardContext {
            max_player_level: 0,
            ..context()
        }
    )
    .is_err());
    assert!(reward_quote(
        input(),
        RewardContext {
            player_level: 0,
            ..context()
        }
    )
    .is_err());
    let gates = QuestEligibilityInput {
        minimum_level: 1,
        maximum_level: 255,
        required_classes: 0,
        required_races: 0,
    };
    for race in [0, 9, 255] {
        assert!(eligibility_gates(
            gates,
            QuestPlayerIdentity {
                level: 1,
                race,
                class: 1
            }
        )
        .is_err());
    }
    for class in [0, 6, 10, 12, 255] {
        assert!(eligibility_gates(
            gates,
            QuestPlayerIdentity {
                level: 1,
                race: 1,
                class
            }
        )
        .is_err());
    }
    assert!(eligibility_gates(
        gates,
        QuestPlayerIdentity {
            level: 0,
            race: 1,
            class: 1
        }
    )
    .is_err());
    // A raw maximum of zero rejects everyone; loader normalization is a separate job.
    assert!(
        !eligibility_gates(
            QuestEligibilityInput {
                maximum_level: 0,
                ..gates
            },
            QuestPlayerIdentity {
                level: 1,
                race: 1,
                class: 1
            }
        )
        .unwrap()
        .level
    );
}
