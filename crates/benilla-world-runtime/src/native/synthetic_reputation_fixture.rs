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
pub fn catalog() -> rep::Catalog {
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
    rep::Catalog::new(
        vec![primary, row(SPILLOVER, 7, 0, 0), row(OTHER, 11, 0, 0)],
        vec![],
        rep::Config {
            gain_rate: spell::SpellFloat::ONE,
            low_level_kill_rate: spell::SpellFloat::ONE,
            low_level_quest_rate: spell::SpellFloat::ONE,
        },
        rep::Audit {
            general_reputation_auras: rep::Evidence::Absent,
            faction_reputation_auras: rep::Evidence::Absent,
            forced_reactions: rep::Evidence::Absent,
            reputation_scripts: rep::Evidence::Absent,
        },
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
