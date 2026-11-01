// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored DBC-shaped rows only; no game content.
use benilla_sim::{classic_spell::SpellFloat, reputation::*};

fn row(id: u32, index: i32) -> [u32; 19] {
    let mut words = [0; 19];
    words[0] = id;
    words[1] = index as u32;
    words
}
fn capture(rows: Vec<[u32; 19]>) -> InitialFactionCapture {
    let mut template = [0; 14];
    template[0] = 800;
    template[1] = 900;
    InitialFactionCapture {
        factions: Some(rows),
        templates: vec![template],
        required_templates: vec![800],
        required_factions: vec![900],
        race: 1,
        class: 8,
    }
}
fn catalog(capture: &InitialFactionCapture, auras: Vec<GeneralReputationAura>) -> Result<Catalog> {
    let policies = capture
        .compile()?
        .iter()
        .map(|f| FactionRewardPolicy {
            faction: f.id,
            reward_rates: Resolved::Absent,
            spillover: Resolved::Absent,
        })
        .collect();
    Catalog::from_initial_factions(
        capture,
        policies,
        Config {
            gain_rate: SpellFloat::ONE,
            low_level_kill_rate: SpellFloat::ONE,
            low_level_quest_rate: SpellFloat::ONE,
        },
        Audit {
            general_reputation_auras: if auras.is_empty() {
                Evidence::Absent
            } else {
                Evidence::Present
            },
            faction_reputation_auras: Evidence::Absent,
            forced_reactions: Evidence::Absent,
            reputation_scripts: Evidence::Absent,
        },
        auras,
    )
}

#[test]
fn masks_select_four_slots_and_wildcard_fallback_without_racial_gain() {
    let mut faction = row(900, 12);
    faction[2..6].copy_from_slice(&[1, 2, 1, 0]);
    faction[6..10].copy_from_slice(&[128, 128, 1, 0]);
    faction[10..14].copy_from_slice(&[1200, (-750i32) as u32, 85, 0]);
    faction[14..18].copy_from_slice(&[VISIBLE | PEACE_FORCED, AT_WAR, INACTIVE, 0]);
    // The final zero wildcard slot is padding and remains a source first-match fallback.
    for (race, class, base, flags) in [
        (1, 8, 1200, VISIBLE | PEACE_FORCED),
        (2, 8, -750, AT_WAR),
        (1, 1, 85, INACTIVE),
        (3, 8, 0, 0),
    ] {
        let mut input = capture(vec![faction]);
        input.race = race;
        input.class = class;
        let catalog = catalog(&input, vec![]).unwrap();
        let state = catalog.initialize(input.identity().unwrap()).unwrap();
        assert_eq!(state.current(&catalog, 900).unwrap(), base);
        assert_eq!(state.factions()[&12].standing, 0);
        assert_eq!(state.factions()[&12].flags, flags);
        assert!(state.factions()[&12].need_send);
        assert!(state.factions()[&12].need_save);
    }
}

#[test]
fn hidden_inactive_and_peace_flags_are_copied_not_normalized() {
    let all_flags = VISIBLE | AT_WAR | HIDDEN | INVISIBLE_FORCED | PEACE_FORCED | INACTIVE | 0x40;
    let mut faction = row(900, 63);
    faction[10] = (-5000i32) as u32;
    faction[14] = all_flags;
    let input = capture(vec![faction, row(901, -1)]);
    let catalog = catalog(&input, vec![]).unwrap();
    let state = catalog.initialize(input.identity().unwrap()).unwrap();
    assert_eq!(state.factions().len(), 1);
    assert_eq!(state.current(&catalog, 900).unwrap(), -5000);
    assert_eq!(state.factions()[&63].flags, all_flags);
    let sent = state.reduce(&catalog, Command::SendInitial).unwrap();
    assert_eq!(sent.state.factions()[&63].standing, 0);
    assert_eq!(sent.state.factions()[&63].flags, all_flags);
    assert!(!sent.state.factions()[&63].need_send);
    assert!(sent.state.factions()[&63].need_save);
}

#[test]
fn diplomacy_aura_changes_gain_but_never_initial_base_or_delta() {
    let mut faction = row(900, 3);
    faction[10] = 1000;
    let input = capture(vec![faction]);
    let aura = GeneralReputationAura {
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
    };
    let boosted = catalog(&input, vec![aura]).unwrap();
    let plain = catalog(&input, vec![]).unwrap();
    for catalog in [&boosted, &plain] {
        let state = catalog.initialize(input.identity().unwrap()).unwrap();
        assert_eq!(state.current(catalog, 900).unwrap(), 1000);
        assert_eq!(state.factions()[&3].standing, 0);
    }
    let gain = GainInput {
        source: Source::Quest,
        reputation: 250,
        faction: 900,
        player_level: 1,
        target_level: 1,
        no_aura_bonus: false,
    };
    assert_eq!(plain.calculate_gain(gain).unwrap(), 250);
    assert_eq!(boosted.calculate_gain(gain).unwrap(), 275);
}

#[test]
fn missing_duplicate_invalid_and_conflicting_rows_refuse() {
    let good = capture(vec![row(900, 3)]);
    let mut bad = good.clone();
    bad.factions = None;
    assert_eq!(
        bad.compile(),
        Err(Error::Unsupported("initial_faction_table_missing"))
    );
    let mut bad = good.clone();
    bad.factions.as_mut().unwrap().clear();
    assert_eq!(bad.compile(), Err(Error::Unresolved(900)));
    let mut bad = good.clone();
    bad.templates.clear();
    assert_eq!(
        bad.compile(),
        Err(Error::Invalid("initial_faction_template_missing"))
    );
    let mut bad = good.clone();
    bad.templates[0][6] = 999;
    assert_eq!(bad.compile(), Err(Error::Unresolved(999)));
    let mut bad = good.clone();
    bad.factions.as_mut().unwrap().push(row(900, -1));
    assert_eq!(
        bad.compile(),
        Err(Error::Invalid("initial_faction_duplicate_row"))
    );
    for index in [-2, 64, i32::MAX] {
        let bad = capture(vec![row(900, index)]);
        assert_eq!(
            bad.compile(),
            Err(Error::Invalid("initial_faction_index_or_id"))
        );
    }
    let mut faction = row(900, 3);
    faction[14] = 128;
    assert_eq!(
        capture(vec![faction]).compile(),
        Err(Error::Unsupported("initial_faction_flag_bits"))
    );
    faction[14] = 0;
    faction[10] = 100;
    faction[11] = 200;
    assert_eq!(
        capture(vec![faction]).compile(),
        Err(Error::Unsupported("initial_faction_conflicting_slots"))
    );
    faction[11] = 100;
    assert!(capture(vec![faction]).compile().is_ok());
    for (race, class) in [(0, 8), (9, 8), (1, 6), (1, 32)] {
        let mut bad = good.clone();
        bad.race = race;
        bad.class = class;
        assert_eq!(
            bad.compile(),
            Err(Error::Invalid("initial_faction_identity"))
        );
    }
}

#[test]
fn signed_base_domain_is_not_the_reward_clamp() {
    for base in [i32::MIN, i32::MAX] {
        let mut faction = row(900, 3);
        faction[10] = base as u32;
        let input = capture(vec![faction]);
        let catalog = catalog(&input, vec![]).unwrap();
        assert_eq!(
            catalog
                .initialize(input.identity().unwrap())
                .unwrap()
                .current(&catalog, 900)
                .unwrap(),
            base
        );
    }
}
