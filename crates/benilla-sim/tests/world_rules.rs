// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::{
    classic_spell::SpellFloat, player_progression::kill_xp as kill, quest, reputation as rep,
    world_rules::NativeWorldRules,
};

#[test]
fn named_stock_preset_matches_world_loader_defaults() {
    let rules = NativeWorldRules::cmangos_classic_stock();
    rules.validate().unwrap();
    assert_eq!(
        (
            rules.version,
            rules.start_player_level,
            rules.start_player_money
        ),
        (1, 1, 0)
    );
    assert_eq!(
        (
            rules.progression.max_player_level,
            rules.progression.global_skill_max
        ),
        (60, 300)
    );
    assert!(!rules.progression.always_max_skill_for_level);
    assert_eq!(rules.progression.talent_rate_bits, 0x3f800000);
    assert_eq!(rules.kill.kill_rate_bits, 0x3f800000);
    assert_eq!(rules.quest_xp_rate().unwrap(), quest::Binary32Rate::ONE);
    assert_eq!(rules.reputation.gain_rate, SpellFloat::ONE);
    assert_eq!(rules.reputation.low_level_kill_rate.bits(), 0x3e4ccccd);
    assert_eq!(rules.reputation.low_level_quest_rate.bits(), 0x3e4ccccd);
    assert_eq!(rules.loot.quality_rate_bits, [0x3f800000; 7]);
    assert_eq!(rules.loot.quest_drop_rate.multiplier_bits, 0x3f800000);
    assert_eq!(rules.loot.referenced_rate_bits, 0x3f800000);
    assert_eq!(rules.loot.money_rate_bits, 0x3f800000);
    assert!(rules.loot.corpse_empty_loot_show);
}

#[test]
fn explicit_rules_refuse_unknown_keys_versions_starts_and_rate_domains() {
    let rules = NativeWorldRules::cmangos_classic_stock();
    let mut json = serde_json::to_value(&rules).unwrap();
    json["Rate.XP.Explore"] = 1.into();
    assert!(serde_json::from_value::<NativeWorldRules>(json).is_err());
    for (field, value) in [
        ("version", 2),
        ("start_player_level", 0),
        ("start_player_level", 2),
        ("start_player_money", u32::MAX),
    ] {
        let mut json = serde_json::to_value(&rules).unwrap();
        json[field] = value.into();
        assert!(serde_json::from_value::<NativeWorldRules>(json)
            .unwrap()
            .validate()
            .is_err());
    }
    for bits in [
        0x80000000u32, 0xbf800000, 1, 0x007fffff, 0x447a0001, 0x7f800000, 0x7fc00000,
    ] {
        for field in [
            "quest_xp_rate_bits",
            "kill",
            "reputation",
            "loot",
            "progression",
        ] {
            let mut json = serde_json::to_value(&rules).unwrap();
            match field {
                "kill" => json[field]["kill_rate_bits"] = bits.into(),
                "reputation" => json[field]["gain_rate"] = bits.into(),
                "loot" => json[field]["referenced_rate_bits"] = bits.into(),
                "progression" => json[field]["talent_rate_bits"] = bits.into(),
                _ => json[field] = bits.into(),
            }
            assert!(
                serde_json::from_value::<NativeWorldRules>(json)
                    .and_then(|r| r.validate().map_err(serde::de::Error::custom))
                    .is_err(),
                "{field} {bits:x}"
            );
        }
    }
    for bits in [0, 0x00800000, 0x447a0000] {
        let mut changed = rules.clone();
        changed.quest_xp_rate_bits = bits;
        changed.validate().unwrap();
    }
}

fn reward(
    rules: &NativeWorldRules,
    level: u8,
    quest_level: u32,
    money: u32,
) -> quest::QuestRewardQuote {
    quest::reward_quote(
        quest::QuestRewardInput {
            xp: quest::QuestXpInput {
                quest_level,
                reward_money_max_level: money,
            },
            reward_or_required_money: 25,
            quest_flags: 0,
        },
        quest::RewardContext {
            player_level: level,
            max_player_level: rules.progression.max_player_level as u8,
            xp_rate: rules.quest_xp_rate().unwrap(),
            money_rate: rules.money_rate().unwrap(),
        },
    )
    .unwrap()
}

#[test]
fn quest_xp_comes_from_money_fields_before_rate_and_cap_conversion() {
    let mut rules = NativeWorldRules::cmangos_classic_stock();
    for level in 1..=3 {
        for (quest_level, money, xp) in [(1, 24, 40), (2, 102, 170), (3, 150, 250)] {
            assert_eq!(
                reward(&rules, level, quest_level, money).xp_to_give,
                Some(xp)
            );
        }
    }
    rules.quest_xp_rate_bits = 0x3f000000;
    assert_eq!(reward(&rules, 1, 2, 102).xp_to_give, Some(85));
    rules.quest_xp_rate_bits = 0;
    assert_eq!(reward(&rules, 1, 2, 102).xp_to_give, Some(0));
    rules.quest_xp_rate_bits = 0x3f7fffff;
    assert_eq!(reward(&rules, 1, 2, 102).xp_to_give, Some(169));
    rules.progression =
        benilla_sim::player_progression::Rules::from_classic_config(3, false, 0x3f800000).unwrap();
    rules.loot.money_rate_bits = 0x3fc00000;
    let quote = reward(&rules, 3, 2, 102);
    assert_eq!(quote.xp_to_give, None);
    assert_eq!(quote.max_level_money, Some(153));
    assert_eq!(quote.reward_or_required_money, 37);
    assert_eq!(
        quest::reward_or_required_money(-25, rules.money_rate().unwrap()).unwrap(),
        -25
    );
}

fn victim(level: u32) -> kill::Victim {
    kill::Victim {
        guid: 2,
        entry: 900006,
        level,
        experience_multiplier_bits: 0x3f800000,
        max_health: 100,
        damage_by_others: 0,
        is_totem: false,
        is_pet: false,
        is_no_xp: false,
        is_critter: false,
        is_elite: false,
        player_controlled: false,
    }
}

#[test]
fn ordinary_vermin_levels_use_zero_difference_trivial_gate_and_kill_rate() {
    let mut rules = NativeWorldRules::cmangos_classic_stock();
    for (player, mob, expected) in [
        (1, 1, 50),
        (1, 2, 52),
        (2, 1, 44),
        (2, 2, 55),
        (3, 1, 36),
        (3, 2, 48),
        (6, 1, 0),
        (7, 2, 0),
    ] {
        assert_eq!(
            kill::gain(player, victim(mob), rules.kill).unwrap(),
            expected
        );
    }
    rules.kill.kill_rate_bits = 0x3f000000;
    assert_eq!(kill::gain(2, victim(2), rules.kill).unwrap(), 28);
    assert_eq!(kill::gain(1, victim(1), rules.kill).unwrap(), 25);
    rules.kill.kill_rate_bits = 0;
    assert_eq!(kill::gain(1, victim(1), rules.kill).unwrap(), 0);
    let mut elite = victim(1);
    elite.is_elite = true;
    assert!(kill::gain(1, elite, rules.kill).is_err());
}

#[test]
fn reputation_global_policy_does_not_replace_sql_reward_rate_or_low_level_floor() {
    let mut rules = NativeWorldRules::cmangos_classic_stock();
    rules.reputation.gain_rate = SpellFloat::from_bits(0x3fc00000).unwrap();
    let catalog = rep::Catalog::new(
        vec![rep::Row {
            faction: rep::Faction {
                id: 990072,
                reputation_list_id: 0,
                race_masks: [0; 4],
                class_masks: [0; 4],
                base_values: [0; 4],
                flags: [0; 4],
                team: 0,
            },
            reward_rates: rep::Resolved::Present(rep::RewardRates {
                quest: SpellFloat::from_bits(0x3f000000).unwrap(),
                creature: SpellFloat::ONE,
                spell: SpellFloat::ONE,
            }),
            spillover: rep::Resolved::Absent,
        }],
        vec![],
        rules.reputation,
        rep::Audit {
            general_reputation_auras: rep::Evidence::Absent,
            faction_reputation_auras: rep::Evidence::Absent,
            forced_reactions: rep::Evidence::Absent,
            reputation_scripts: rep::Evidence::Absent,
        },
    )
    .unwrap();
    let mut gain = rep::GainInput {
        source: rep::Source::Quest,
        reputation: 75,
        faction: 990072,
        player_level: 1,
        target_level: 1,
        no_aura_bonus: false,
    };
    assert_eq!(catalog.calculate_gain(gain).unwrap(), 56);
    gain.player_level = 20;
    assert_eq!(catalog.calculate_gain(gain).unwrap(), 11);
    gain.reputation = -75;
    assert_eq!(catalog.calculate_gain(gain).unwrap(), -11);
}
