// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use benilla_sim::classic_spell::SpellFloat;

#[test]
fn explicit_policy_rejects_inconsistent_consumers_atomically() {
    for field in ["progression", "kill", "reputation", "loot"] {
        let (mut profile, input, mut rows) = synthetic_loot_fixture::setup();
        match field {
            "progression" => rows.world_rules.progression.always_max_skill_for_level = false,
            "kill" => rows.world_rules.kill.kill_rate_bits = 0,
            "reputation" => rows.world_rules.reputation.gain_rate = SpellFloat::ZERO,
            "loot" => rows.world_rules.loot.money_rate_bits = 0,
            _ => unreachable!(),
        }
        let before = profile.clone();
        assert!(
            profile.initialize_quest_progression(&input, rows).is_err(),
            "{field}"
        );
        assert_eq!(profile, before);
    }
}

#[test]
fn absent_consumers_require_stock_inactive_settings() {
    for field in ["reputation", "loot"] {
        let (mut profile, input, mut rows) = synthetic_quest_fixture::setup();
        match field {
            "reputation" => rows.world_rules.reputation.gain_rate = SpellFloat::ZERO,
            "loot" => rows.world_rules.loot.quality_rate_bits[0] = 0,
            _ => unreachable!(),
        }
        let error = profile
            .initialize_quest_progression(&input, rows)
            .unwrap_err();
        assert_eq!(
            error,
            NativeError::UnsupportedMechanic(match field {
                "reputation" => "reputation rate configured but no reputation host",
                _ => "drop rate configured but no loot host",
            })
        );
    }
}

#[test]
fn bound_rates_and_start_money_change_effective_profile_identity() {
    let (mut baseline, input, rows) = synthetic_quest_fixture::setup();
    baseline
        .initialize_quest_progression(&input, rows.clone())
        .unwrap();
    for field in ["kill", "quest", "money"] {
        let (mut changed, _, _) = synthetic_quest_fixture::setup();
        let mut rows = rows.clone();
        match field {
            "kill" => {
                rows.world_rules.kill.kill_rate_bits = 0;
                rows.kill.rules = rows.world_rules.kill;
            }
            "quest" => rows.world_rules.quest_xp_rate_bits = 0,
            "money" => rows.world_rules.start_player_money += 1,
            _ => unreachable!(),
        }
        changed.initialize_quest_progression(&input, rows).unwrap();
        assert_ne!(changed.digest(), baseline.digest(), "{field}");
    }
}
