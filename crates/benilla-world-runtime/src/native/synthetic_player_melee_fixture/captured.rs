// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored complete selected projections through the real learning/outfit adapters.
use crate::native::{
    synthetic_spell_fixture::SpatialOnly, synthetic_two_owner_fixture::CompleteAuthority,
    Authority, AuthorityReply, AuthorityRequest, NativeError, Query,
};
use benilla_sim::starter_init::*;

#[derive(Default)]
pub struct CapturedStaffAuthority {
    scene: CompleteAuthority,
    spell: SpatialOnly,
}
impl Authority for CapturedStaffAuthority {
    fn resolve(
        &mut self,
        request: &AuthorityRequest,
    ) -> std::result::Result<AuthorityReply, NativeError> {
        if matches!(
            request.query,
            Query::SourceSpell(_) | Query::SourceLocation(_)
        ) {
            self.spell.resolve(request)
        } else {
            self.scene.resolve(request)
        }
    }
    fn observe_derived(&mut self, reply: &AuthorityReply) -> std::result::Result<(), NativeError> {
        if matches!(reply.request.query, Query::SourceSpell(_)) {
            self.spell.observe_derived(reply)
        } else {
            self.scene.observe_derived(reply)
        }
    }
}

pub(super) fn starter(input: StarterInput) -> StarterInput {
    let Rows::Complete(defaults) = &input.default_skills else {
        unreachable!()
    };
    let Rows::Complete(roots) = &input.default_spells else {
        unreachable!()
    };
    let Rows::Complete(abilities) = &input.skill_abilities else {
        unreachable!()
    };
    let numeric: Vec<_> = input
        .spells
        .iter()
        .map(|row| LearningSpellNumeric {
            source_identity: [71; 32],
            id: row.id,
            client_words: row.words,
            effective_words: row.words,
            duration_ms: row.duration_ms,
        })
        .collect();
    let capture = CapturedStarterLearning {
        source_identity: [71; 32],
        selected_spell_ids: input.spells.iter().map(|row| row.id).collect(),
        skill_training: Rows::Complete(vec![]),
        creation_skills: Rows::Complete(
            defaults
                .iter()
                .map(|row| CreationSkill {
                    race_mask: 0,
                    class_mask: 0,
                    skill: u32::from(row.id),
                    step: u32::from(row.step),
                })
                .collect(),
        ),
        creation_spells: Rows::Complete(
            roots
                .iter()
                .map(|&spell| CreationSpell {
                    race: 1,
                    class: 8,
                    spell,
                })
                .collect(),
        ),
        skill_lines: Rows::Complete(
            input
                .skill_lines
                .iter()
                .map(|line| {
                    let mut row = [0; 22];
                    row[0] = u32::from(line.id);
                    row[1] = line.category as u32;
                    row
                })
                .collect(),
        ),
        skill_race_class: Rows::Complete(
            input
                .skill_lines
                .iter()
                .enumerate()
                .map(|(index, line)| {
                    let mut row = [0; 8];
                    row[0] = index as u32 + 1;
                    row[1] = u32::from(line.id);
                    row[2] = 1;
                    row[3] = 128;
                    row
                })
                .collect(),
        ),
        skill_tiers: Rows::Complete(vec![]),
        skill_abilities: Rows::Complete(
            abilities
                .iter()
                .map(|ability| {
                    let mut row = [0; 15];
                    row[0] = ability.row_id;
                    row[1] = u32::from(ability.skill);
                    row[2] = ability.spell;
                    row[3] = ability.race_mask;
                    row[4] = ability.class_mask;
                    row[7] = ability.required_value;
                    row[8] = ability.forward_spell_id;
                    row[9] = ability.learn_on_get_skill;
                    row[10] = ability.max_value;
                    row
                })
                .collect(),
        ),
        talents: Rows::Complete(vec![]),
        chains: Rows::Complete(vec![]),
        learn_spells: Rows::Complete(vec![]),
        global_chain_probe: Rows::Complete(vec![]),
        global_learn_probe: Rows::Complete(vec![]),
        spell_existence: Rows::Complete(input.spells.iter().map(|row| (row.id, true)).collect()),
        numeric: Rows::Complete(numeric.clone()),
    };
    let learning = capture.compile(1, 8).unwrap();
    let hooks = Rows::Complete(
        input
            .spells
            .iter()
            .map(|row| LearningInitialHooks {
                spell: row.id,
                source_identity: [71; 32],
                external_hooks: Some(row.external_initial_hooks),
            })
            .collect(),
    );
    let spells = learning.bind_spell_rows(&numeric, &hooks).unwrap();
    let input = learning.populate(input, spells).unwrap();
    CapturedStarterItems {
        outfits: Rows::Complete(vec![CharStartOutfitRow {
            id: 901,
            race_class_gender: 0x801,
            items: [900001, 35, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        }]),
        custom: Rows::Complete(vec![]),
        items: input
            .items
            .iter()
            .cloned()
            .map(|item| StartingItemRow {
                item,
                buy_count: 1,
                first_spell_category: 0,
            })
            .collect(),
    }
    .compile(input)
    .unwrap()
}
