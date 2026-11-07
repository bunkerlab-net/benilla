// SPDX-License-Identifier: GPL-3.0-or-later
//! Authored callback vectors, not client rows or observed Human initialization.
//! The C++ fixture extracts SetSkill/UpdateSkillTrainedSpells; its addSpell endpoints
//! install authored auras. Here the corresponding bounded Rust learning path runs.
use benilla_sim::starter_init::*;

fn aura(id: u32, kind: u32, amount: u32) -> SpellRow {
    let mut words = [0; 173];
    words[0] = id;
    words[6] = 0x40;
    words[58] = u32::MAX;
    words[61] = 6;
    words[64] = 1;
    words[67] = 1;
    words[76] = amount - 1;
    words[82] = 1;
    words[91] = kind;
    words[106] = 60001;
    SpellRow {
        id,
        words,
        duration_ms: Some(-1),
        previous_rank: None,
        talent: false,
        learned_spells: Rows::Complete(vec![]),
        external_initial_hooks: false,
    }
}

fn input(threshold: u32, reverse: bool) -> StarterInput {
    let mut rows: Vec<_> = [
        (70001, 1, 128),
        (70002, 2, 128),
        (70003, 1, 1),
        (70004, 1, 128),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (spell, race_mask, class_mask))| SkillAbilityRow {
        row_id: if reverse {
            4 - index as u32
        } else {
            index as u32 + 1
        },
        skill: 60001,
        spell,
        forward_spell_id: 0,
        race_mask,
        class_mask,
        required_value: if matches!(spell, 70001 | 70004) {
            threshold
        } else {
            0
        },
        learn_on_get_skill: 1,
        max_value: 0,
    })
    .collect();
    // Deliberately scramble storage: source loaded-map row IDs determine traversal.
    rows.rotate_left(1);
    StarterInput {
        race: 1,
        class: 8,
        gender: 0,
        level: 1,
        class_power_type: 0,
        create_attributes: Attributes([17, 23, 29, 31, 37]),
        create_health: 41,
        create_mana: 59,
        default_skills: Rows::Complete(vec![DefaultSkill { id: 60001, step: 0 }]),
        default_spells: Rows::Complete(vec![]),
        skill_lines: vec![SkillLineRow {
            id: 60001,
            category: 6,
        }],
        skill_race_class: Rows::Complete(vec![]),
        skill_tiers: vec![],
        skill_abilities: Rows::Complete(rows),
        spells: vec![
            aura(70001, 30, 2),
            aura(70002, 30, 11),
            aura(70003, 98, 13),
            aura(70004, 98, 5),
        ],
        active_support: vec![],
        item_grants: Rows::Complete(vec![]),
        items: vec![],
        item_creation_hooks: false,
    }
}

#[test]
fn unchanged_source_skill_callbacks_match_reached_cast_order_and_retained_bonuses() {
    let mut count = 0;
    for line in include_str!("fixtures/starter_callbacks_cmangos.tsv").lines() {
        let fields: Vec<_> = line.split('\t').collect();
        if !fields[0].starts_with("starter_threshold") {
            continue;
        }
        let (threshold, order) = fields[0]
            .strip_prefix("starter_threshold")
            .unwrap()
            .split_once('_')
            .unwrap();
        let threshold = threshold.parse().unwrap();
        assert!(matches!(order, "forward" | "reverse"));
        let initialized = initialize_mage(&input(threshold, order == "reverse")).unwrap();
        let source_calls: Vec<u32> = fields[1]
            .split(';')
            .filter_map(|event| {
                event
                    .strip_prefix("add-endpoint:")
                    .map(|event| event.split(':').next().unwrap().parse().unwrap())
            })
            .collect();
        let casts: Vec<_> = initialized
            .trace
            .iter()
            .filter_map(|event| match event {
                InitEvent::InitialCast { spell } => Some(*spell),
                _ => None,
            })
            .collect();
        assert_eq!(casts, source_calls, "{line}");
        let learned: Vec<_> = initialized
            .trace
            .iter()
            .filter_map(|event| match event {
                InitEvent::SpellLearned { spell } => Some(*spell),
                _ => None,
            })
            .collect();
        assert_eq!(learned, source_calls, "{line}");
        let source_skill: Vec<u32> = fields[2]
            .trim_end_matches(';')
            .split(':')
            .map(|word| word.parse().unwrap())
            .collect();
        assert_eq!(source_skill.len(), 6, "{line}");
        let skill = initialized.skills[&60001];
        assert_eq!(u32::from(skill.id), source_skill[0]);
        assert_eq!(u32::from(skill.step), source_skill[3] >> 16);
        assert_eq!(u32::from(skill.value), source_skill[4] & 0xffff);
        assert_eq!(u32::from(skill.maximum), source_skill[4] >> 16);
        assert_eq!(
            skill.temporary_bonus, source_skill[5] as u16 as i16,
            "{line}"
        );
        assert_eq!(
            skill.permanent_bonus,
            (source_skill[5] >> 16) as u16 as i16,
            "{line}"
        );
        assert_eq!(initialized.initial_auras.len(), source_calls.len());
        let skill_set = initialized
            .trace
            .iter()
            .position(|event| matches!(event, InitEvent::SkillSet { skill: 60001, .. }))
            .unwrap();
        for (index, event) in initialized.trace.iter().enumerate() {
            if matches!(event, InitEvent::InitialCast { .. }) {
                assert!(skill_set < index, "skill fields precede callbacks: {line}");
            }
        }
        count += 1;
    }
    assert_eq!(count, 6);
}
