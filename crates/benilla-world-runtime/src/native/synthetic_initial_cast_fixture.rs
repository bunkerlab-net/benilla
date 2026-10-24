// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored synthetic evidence only; no captured DBC/order or native admission.
use super::*;
use benilla_sim::starter_init as init;

pub fn row(id: u32) -> init::SpellRow {
    assert!(matches!(id, 81 | 3050));
    let mut w = [0; 173];
    w[0] = id;
    w[6] = if id == 81 { 0x50 } else { 0xc0 };
    w[7] = if id == 3050 { 0x0200_0000 } else { 0 };
    w[18] = u32::from(id != 81);
    w[25] = 101;
    w[28] = u32::from(id == 81);
    w[29] = u32::from(id == 81);
    w[36] = 1;
    w[58] = u32::MAX;
    w[59] = u32::MAX;
    w[61] = if id == 81 { 20 } else { 49 };
    w[64] = if id == 81 { 6 } else { 1 };
    w[67] = u32::from(id == 3050);
    w[73] = if id == 3050 { 0x40a0_0000 } else { 0 };
    w[76] = if id == 3050 { u32::MAX } else { 0 };
    w[97] = if id == 81 { 0x3f80_0000 } else { 0 };
    w[106] = if id == 81 { 90 } else { 0 };
    w[164] = u32::from(id == 81);
    w[165] = u32::from(id == 81);
    w[166] = u32::MAX;
    w[167..170].fill(0x3f80_0000);
    init::SpellRow {
        id,
        words: w,
        duration_ms: Some(0),
        previous_rank: None,
        talent: false,
        learned_spells: init::Rows::Complete(vec![]),
        external_initial_hooks: false,
    }
}

/// Append source casts before genuine learning; no caller-authored phase plan enters RNG.
pub fn prepare(
    mut profile: Profile,
    mut input: init::StarterInput,
) -> (
    Profile,
    init::StarterInput,
    init::PreparedStarter,
    NativeInitialCastSourceInput,
) {
    let init::Rows::Complete(defaults) = &mut input.default_spells else {
        unreachable!()
    };
    defaults.extend([81, 3050]);
    input.spells.extend([row(81), row(3050)]);
    let dependencies: Vec<_> = [81, 3050]
        .into_iter()
        .map(|id| InitialCastDependencies {
            spell_id: id,
            cast_time: InitialSpellLookup::Missing {
                index: u32::from(id != 81),
            },
            duration: InitialSpellLookup::Missing { index: 0 },
            aura_script_names: SpellSourceRows::Complete(vec![]),
            server_side: 0,
            server_attributes: 0,
            bonus_coefficients: [if id == 81 { 0x3f80_0000 } else { 0 }, 0, 0],
            attack_power_coefficients: [0; 3],
        })
        .collect();
    let mut queried: Vec<_> = profile
        .abilities
        .iter()
        .map(|a| a.id)
        .chain([81, 3050])
        .collect();
    queried.sort_unstable();
    queried.dedup();
    let source = NativeInitialCastSourceInput {
        source_identity: input.source_identity(),
        entry_location: None,
        store_identity: [71; 32],
        dependencies: SpellSourceRows::Complete(dependencies.clone()),
        hooks: synthetic_spell_fixture::hooks(),
        queried_spell_ids: SpellSourceRows::Complete(queried),
    };
    let numeric = init::StarterInitialDependencies {
        source_identity: input.source_identity(),
        store_identity: source.store_identity,
        hook_scope_identity: source.hook_scope_identity(),
        queried_spell_ids: vec![81, 3050],
        rows: init::Rows::Complete(
            dependencies
                .iter()
                .map(|d| init::StarterInitialDependency {
                    spell_id: d.spell_id,
                    source_hash: input
                        .spells
                        .iter()
                        .find(|r| r.id == d.spell_id)
                        .unwrap()
                        .source_hash(),
                    cast_time: d.cast_time.clone(),
                    duration: d.duration.clone(),
                    server_side: d.server_side,
                    server_attributes: d.server_attributes,
                    bonus_coefficients: d.bonus_coefficients,
                    attack_power_coefficients: d.attack_power_coefficients,
                })
                .collect(),
        ),
    };
    let prepared = init::prepare_starter(
        &input,
        &numeric,
        &init::StarterCastOrder {
            creation: init::InitialCastOrderEvidence::Authored { identity: [72; 32] },
            first_entry: init::InitialCastOrderEvidence::Authored { identity: [73; 32] },
            entry_skill_order: Some(vec![95, 8]),
        },
    )
    .unwrap();
    profile
        .initialize_player_prepared(&input, &prepared, synthetic_fixture::float(100.0), 1)
        .unwrap();
    (profile, input, prepared, source)
}

pub fn inputs() -> (
    Profile,
    init::StarterInput,
    init::PreparedStarter,
    NativeInitialCastSourceInput,
) {
    prepare(
        synthetic_fixture::initialized_profile(),
        synthetic_fixture::initialized_starter_input(),
    )
}
pub fn profile() -> Profile {
    let (mut profile, input, _, source) = inputs();
    profile
        .initialize_initial_cast_source(&input, source)
        .unwrap();
    profile
}
pub fn world(seed: u64) -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [8; 16], synthetic_fixture::player(), seed)
        .unwrap()
}
