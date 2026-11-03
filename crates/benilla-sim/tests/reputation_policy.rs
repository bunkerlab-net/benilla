// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored SQL-shaped policy lookup inputs, never a claimed reference capture.
use benilla_sim::{classic_spell::SpellFloat, reputation::*};
fn factions() -> InitialFactionCapture {
    let mut a = [0; 19];
    a[0] = 900;
    a[1] = 0;
    a[10] = 1000;
    a[18] = 901;
    let mut b = [0; 19];
    b[0] = 901;
    b[1] = 1;
    InitialFactionCapture {
        factions: Some(vec![a, b]),
        templates: vec![],
        required_templates: vec![],
        required_factions: vec![],
        race: 1,
        class: 8,
    }
}
fn lookups() -> Vec<FactionPolicyLookup> {
    let mut spill = [RawSpillover {
        faction: 0,
        rate: SpellFloat::ZERO,
        maximum_rank: 0,
    }; 4];
    spill[0] = RawSpillover {
        faction: 901,
        rate: SpellFloat::from_bits(0x3f000000).unwrap(),
        maximum_rank: 7,
    };
    vec![
        FactionPolicyLookup {
            faction: 900,
            reward_rates: None,
            spillover: Some(spill),
        },
        FactionPolicyLookup {
            faction: 901,
            reward_rates: None,
            spillover: None,
        },
    ]
}
fn catalog(inputs: &[FactionPolicyLookup], aura: bool) -> Catalog {
    let capture = factions();
    let policies = compile_faction_policies(&capture.compile().unwrap(), inputs).unwrap();
    Catalog::from_initial_factions(
        &capture,
        policies,
        Config {
            gain_rate: SpellFloat::ONE,
            low_level_kill_rate: SpellFloat::ONE,
            low_level_quest_rate: SpellFloat::ONE,
        },
        Audit {
            general_reputation_auras: if aura {
                Evidence::Present
            } else {
                Evidence::Absent
            },
            faction_reputation_auras: Evidence::Absent,
            forced_reactions: Evidence::Absent,
            reputation_scripts: Evidence::Absent,
        },
        if aura {
            vec![GeneralReputationAura {
                spell_id: 20599,
                effect_index: 0,
                aura_type: 156,
                misc_value: 0,
                amount: 10,
                attributes: 0x50,
                attributes_ex3: 0,
                caster_aura_state: 0,
                aura_interrupt_flags: 0,
                duration_ms: -1,
                source_spell_hash: [8; 32],
            }]
        } else {
            vec![]
        },
    )
    .unwrap()
}
fn gain(reputation: i32) -> GainInput {
    GainInput {
        source: Source::Kill,
        reputation,
        faction: 900,
        player_level: 1,
        target_level: 1,
        no_aura_bonus: false,
    }
}
#[test]
fn absent_and_zero_rates_are_distinct_and_retained_aura_is_not_an_initial_multiplier() {
    let plain = catalog(&lookups(), false);
    let diplomacy = catalog(&lookups(), true);
    let id = factions().identity().unwrap();
    assert_eq!(
        diplomacy
            .initialize(id)
            .unwrap()
            .current(&diplomacy, 900)
            .unwrap(),
        1000
    );
    assert_eq!(plain.calculate_gain(gain(100)).unwrap(), 100);
    assert_eq!(diplomacy.calculate_gain(gain(100)).unwrap(), 110);
    assert_eq!(diplomacy.calculate_gain(gain(-100)).unwrap(), -90);
    let mut zero = lookups();
    zero[0].reward_rates = Some(RewardRates {
        quest: SpellFloat::ONE,
        creature: SpellFloat::ZERO,
        spell: SpellFloat::ONE,
    });
    assert_eq!(catalog(&zero, true).calculate_gain(gain(100)).unwrap(), 0);
    zero[0].reward_rates.as_mut().unwrap().quest = SpellFloat::from_bits(0xbf800000).unwrap();
    assert_eq!(catalog(&zero, true).calculate_gain(gain(100)).unwrap(), 110);
}
#[test]
fn complete_policy_spillover_survives_save_reopen_with_exact_standings() {
    let c = catalog(&lookups(), true);
    let initial = c.initialize(factions().identity().unwrap()).unwrap();
    let amount = c.calculate_gain(gain(100)).unwrap();
    let command = Command::Set {
        faction: 900,
        standing: amount,
        incremental: true,
        no_spillover: false,
    };
    let changed = initial.reduce(&c, command).unwrap().state;
    assert_eq!(changed.current(&c, 900).unwrap(), 1110);
    assert_eq!(changed.current(&c, 901).unwrap(), 55);
    let saved = serde_json::to_vec(&changed).unwrap();
    let reopened: State = serde_json::from_slice(&saved).unwrap();
    reopened.validate(&c).unwrap();
    assert_eq!(reopened, changed);
    assert_eq!(
        reopened.reduce(&c, command).unwrap().state,
        changed.reduce(&c, command).unwrap().state
    );
}
#[test]
fn missing_duplicate_extra_policy_and_unrepresentable_accepted_rank_refuse() {
    let factions = factions().compile().unwrap();
    assert!(matches!(
        compile_faction_policies(&factions, &lookups()[..1]),
        Err(Error::Unsupported("reputation_policy_lookup_missing"))
    ));
    let mut duplicate = lookups();
    duplicate[1].faction = 900;
    assert!(matches!(
        compile_faction_policies(&factions, &duplicate),
        Err(Error::Invalid("reputation_policy_duplicate_lookup"))
    ));
    duplicate[1].faction = 999;
    assert!(matches!(
        compile_faction_policies(&factions, &duplicate),
        Err(Error::Invalid("reputation_policy_extra_lookup"))
    ));
    let mut invalid = lookups();
    invalid[0].spillover.as_mut().unwrap()[0].maximum_rank = 8;
    assert!(matches!(
        compile_faction_policies(&factions, &invalid),
        Err(Error::Unsupported("reputation_spillover_rank"))
    ));
}
#[test]
fn loader_inner_continue_retains_unlisted_targets_but_missing_target_skips_row() {
    let mut factions = factions().compile().unwrap();
    factions[1].reputation_list_id = -1;
    let policy = compile_faction_policies(&factions, &lookups()).unwrap();
    assert!(matches!(policy[0].spillover, Resolved::Present(_)));
    let mut invalid = lookups();
    invalid[0].spillover.as_mut().unwrap()[0].faction = 999;
    assert!(matches!(
        compile_faction_policies(&factions, &invalid).unwrap()[0].spillover,
        Resolved::Absent
    ));
}
