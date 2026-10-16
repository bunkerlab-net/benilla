// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored placeholder rows, not Classic content or admission evidence.
use super::*;
use benilla_sim::{quest_lifecycle as q, reputation as rep, starter_init as init};
pub const GRANT: u32 = 70001;
pub const OBJECTIVE: u32 = 70002;
pub const GATED: u32 = 70003;
pub const LOSS: u32 = 70004;
pub const OTHER_OBJECTIVE: u32 = 70005;
pub const PRIMARY: u32 = 91001;
pub const SPILLOVER: u32 = 91002;
pub const OTHER: u32 = 91003;
/// Authored wire-representable placeholder, not the real Diplomacy spell ID.
pub const MODIFIER_SPELL: u32 = 60007;
pub fn catalog() -> rep::Catalog {
    catalog_with_auras(vec![])
}
fn catalog_with_auras(auras: Vec<rep::GeneralReputationAura>) -> rep::Catalog {
    let general_reputation_auras = if auras.is_empty() {
        rep::Evidence::Absent
    } else {
        rep::Evidence::Present
    };
    let row = |id, reputation_list_id, base, flags| rep::Row {
        faction: rep::Faction {
            id,
            reputation_list_id,
            race_masks: [0; 4],
            class_masks: [0; 4],
            base_values: [base, 0, 0, 0],
            flags: [flags, 0, 0, 0],
            team: 0,
        },
        reward_rates: rep::Resolved::Absent,
        spillover: rep::Resolved::Absent,
    };
    let mut primary = row(PRIMARY, 3, -100, rep::AT_WAR);
    primary.spillover = rep::Resolved::Present([
        rep::Spillover {
            faction: SPILLOVER,
            rate: spell::SpellFloat::ONE,
            maximum_rank: rep::Rank::Exalted,
        },
        rep::Spillover {
            faction: 0,
            rate: spell::SpellFloat::ZERO,
            maximum_rank: rep::Rank::Hated,
        },
        rep::Spillover {
            faction: 0,
            rate: spell::SpellFloat::ZERO,
            maximum_rank: rep::Rank::Hated,
        },
        rep::Spillover {
            faction: 0,
            rate: spell::SpellFloat::ZERO,
            maximum_rank: rep::Rank::Hated,
        },
    ]);
    rep::Catalog::new_with_general_reputation_auras(
        vec![primary, row(SPILLOVER, 7, 0, 0), row(OTHER, 11, 0, 0)],
        vec![],
        rep::Config {
            gain_rate: spell::SpellFloat::ONE,
            low_level_kill_rate: spell::SpellFloat::ONE,
            low_level_quest_rate: spell::SpellFloat::ONE,
        },
        rep::Audit {
            general_reputation_auras,
            faction_reputation_auras: rep::Evidence::Absent,
            forced_reactions: rep::Evidence::Absent,
            reputation_scripts: rep::Evidence::Absent,
        },
        auras,
    )
    .unwrap()
}
pub fn setup() -> (Profile, init::StarterInput, QuestProgressionInput) {
    let (p, input, mut data) = synthetic_quest_fixture::setup();
    data.reputation = Some(catalog());
    let mut row = data.quests[0].clone();
    row.required_creature = [0; 4];
    row.required_creature_count = [0; 4];
    row.special_flags = 0;
    let mut objective = row.clone();
    objective.id = OBJECTIVE;
    objective.excluded.rep_objective_faction = SPILLOVER;
    objective.excluded.rep_objective_value = 150;
    objective.reward.xp.reward_money_max_level = 0;
    let mut other = objective.clone();
    other.id = OTHER_OBJECTIVE;
    other.excluded.rep_objective_faction = OTHER;
    let mut gated = objective.clone();
    gated.id = GATED;
    gated.excluded.rep_objective_faction = 0;
    gated.excluded.rep_objective_value = 0;
    gated.excluded.required_min_rep_faction = SPILLOVER;
    gated.excluded.required_min_rep_value = 150;
    let mut loss = gated.clone();
    loss.id = LOSS;
    loss.excluded.required_min_rep_faction = 0;
    loss.excluded.required_min_rep_value = 0;
    loss.excluded.reward_rep_faction[0] = PRIMARY;
    loss.excluded.reward_rep_value[0] = -500;
    row.excluded.reward_rep_faction = [PRIMARY, PRIMARY, PRIMARY, 0, 0];
    row.excluded.reward_rep_value = [200, -100, 100, 0, 0];
    data.quests = vec![row, objective, gated, loss, other];
    data.relations = q::QuestRelations {
        starters: vec![],
        finishers: vec![],
    };
    for row in &data.quests {
        let relation = q::CreatureQuestRelation {
            creature: 60003,
            quest: row.id,
        };
        data.relations.starters.push(relation.clone());
        data.relations.finishers.push(relation);
    }
    (p, input, data)
}
pub fn profile() -> Profile {
    let (mut p, input, data) = setup();
    p.initialize_quest_progression(&input, data).unwrap();
    p
}
pub fn world() -> NativeWorld {
    NativeWorld::synthetic_component_test(profile(), [1; 16], synthetic_fixture::player(), 42)
        .unwrap()
}

/// Source-shaped authored effect; IDs and resolved duration are synthetic.
/// Actual Diplomacy's client duration/range/cast-time rows remain unverified.
pub fn setup_with_modifier(amount: i32) -> (Profile, init::StarterInput, QuestProgressionInput) {
    let (mut profile, mut input, mut data) = setup();
    let mut words = [0; 173];
    words[0] = MODIFIER_SPELL;
    words[6] = 0x50;
    words[25] = 101;
    words[58] = u32::MAX;
    words[61] = 6;
    words[64] = 1;
    words[67] = 1;
    words[76] = amount.checked_sub(1).unwrap() as u32;
    words[82] = 1;
    words[91] = 156;
    words[167] = 1.0f32.to_bits();
    let raw = init::SpellRow {
        id: words[0],
        words,
        duration_ms: Some(-1),
        previous_rank: None,
        talent: false,
        learned_spells: init::Rows::Complete(vec![]),
        external_initial_hooks: false,
    };
    let aura = rep::GeneralReputationAura {
        spell_id: raw.id,
        effect_index: 0,
        aura_type: 156,
        misc_value: 0,
        amount,
        attributes: 0x50,
        attributes_ex3: 0,
        caster_aura_state: 0,
        aura_interrupt_flags: 0,
        duration_ms: -1,
        source_spell_hash: raw.source_hash(),
    };
    let init::Rows::Complete(spells) = &mut input.default_spells else {
        unreachable!()
    };
    spells.push(raw.id);
    input.spells.push(raw);
    profile
        .initialize_player(
            &input,
            spell::SpellFloat::from_bits(100.0f32.to_bits()).unwrap(),
            1,
        )
        .unwrap();
    data.reputation = Some(catalog_with_auras(vec![aura]));
    (profile, input, data)
}
pub fn profile_with_modifier(amount: i32) -> Profile {
    let (mut profile, input, data) = setup_with_modifier(amount);
    profile.initialize_quest_progression(&input, data).unwrap();
    profile
}
pub fn world_with_modifier(amount: i32) -> NativeWorld {
    NativeWorld::synthetic_component_test(
        profile_with_modifier(amount),
        [1; 16],
        synthetic_fixture::player(),
        42,
    )
    .unwrap()
}
