// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

struct Inputs;
impl ConditionValidationInputs for Inputs {}

fn condition(entry: u32, kind: i32, values: [u32; 4]) -> ConditionRow {
    ConditionRow {
        entry,
        kind,
        values,
        flags: 0,
    }
}

#[test]
fn erased_conditions_are_absent_from_later_link_validation() {
    let rows = [
        condition(8, -2, [1, 2, 0, 0]),
        condition(7, -1, [1, 2, 3, 0]),
        condition(6, -1, [1, 2, 0, 3]),
        condition(5, -3, [3, 0, 0, 0]),
        condition(4, -2, [2, 4, 0, 0]),
        condition(3, 6, [0; 4]),
        condition(2, 6, [469, 0, 0, 0]),
        condition(1, 18, [6, 0, 0, 0]),
    ];
    let loaded = SourceConditions::load(rows, &mut Inputs).unwrap();
    assert_eq!(
        loaded.rows().map(|row| row.entry).collect::<Vec<_>>(),
        [1, 2, 8]
    );
    assert!(loaded.get(3).is_none());
    assert!(loaded.get(5).is_none());
}

#[test]
fn without_player_short_circuits_both_or_links_and_respects_target_kind() {
    let rows = [
        condition(1, 18, [6, 1, 0, 0]),
        condition(2, 6, [469, 0, 0, 0]),
        condition(3, -2, [1, 1, 0, 0]),
        condition(4, -2, [1, 2, 0, 0]),
        condition(5, -2, [2, 1, 0, 0]),
        condition(6, -3, [2, 0, 0, 0]),
        condition(7, -3, [1, 0, 0, 0]),
        condition(8, 11, [0, 5, 0, 0]),
        condition(9, -1, [1, 8, 2, 0]),
    ];
    let loaded = SourceConditions::load(rows, &mut Inputs).unwrap();
    for id in [1, 3, 7, 8, 9] {
        assert!(loaded.can_be_used_without_player(id), "condition {id}");
    }
    for id in [0, 2, 4, 5, 6] {
        assert!(!loaded.can_be_used_without_player(id), "condition {id}");
    }
}

#[test]
fn validation_distinguishes_a_missing_owner_from_a_missing_row() {
    assert_eq!(
        SourceConditions::load([condition(1, 1, [12, 3, 0, 0])], &mut Inputs)
            .unwrap_err()
            .id,
        "execution.conditions.Conditions_676.spell_template_owner"
    );
    assert_eq!(
        SourceConditions::load([condition(1, 15, [1, 1, 0, 0])], &mut Inputs)
            .unwrap_err()
            .id,
        "execution.conditions.Conditions_846.max_player_level_owner"
    );
    assert!(
        SourceConditions::load([condition(1, 15, [0, 1, 0, 0])], &mut Inputs)
            .unwrap()
            .get(1)
            .is_none()
    );

    struct SpellRows;
    impl ConditionValidationInputs for SpellRows {
        fn exists(&mut self, domain: ConditionDependency, id: u32) -> Result<Option<bool>> {
            assert_eq!(domain, ConditionDependency::SpellTemplate);
            Ok(Some(id == 12))
        }
    }
    let loaded = SourceConditions::load(
        [
            condition(1, 1, [12, 2, 0, 0]),
            condition(2, 1, [12, 3, 0, 0]),
            condition(3, 1, [13, 2, 0, 0]),
        ],
        &mut SpellRows,
    )
    .unwrap();
    assert_eq!(loaded.rows().map(|row| row.entry).collect::<Vec<_>>(), [1]);
}

#[test]
fn quest_conditions_use_loaded_templates_and_are_player_only() {
    struct QuestRows;
    impl ConditionValidationInputs for QuestRows {
        fn exists(&mut self, domain: ConditionDependency, id: u32) -> Result<Option<bool>> {
            assert_eq!(domain, ConditionDependency::QuestTemplate);
            Ok(Some(id == 783))
        }
    }

    for kind in [8, 9, 19, 22] {
        let existing = condition(1, kind, [783, 2, 0, 0]);
        let absent = condition(2, kind, [99, 2, 0, 0]);
        let loaded = SourceConditions::load([existing, absent], &mut QuestRows).unwrap();
        assert_eq!(loaded.get(1), Some(&existing));
        assert!(loaded.get(2).is_none());
        assert!(!loaded.can_be_used_without_player(1));
        assert_eq!(
            SourceConditions::load([existing], &mut Inputs)
                .unwrap_err()
                .id,
            "execution.conditions.Conditions_778.quest_template_owner"
        );
    }
}

#[test]
fn value_checks_gate_only_the_fields_read_by_each_condition_type() {
    use benilla_sim::player_progression::Rules;
    struct Authored {
        rules: Rules,
        encounter_calls: Vec<u32>,
    }
    impl ConditionValidationInputs for Authored {
        fn exists(&mut self, _: ConditionDependency, id: u32) -> Result<Option<bool>> {
            Ok(Some(id == 1))
        }
        fn progression(&self) -> Option<&Rules> {
            Some(&self.rules)
        }
        fn encounter_map(&mut self, id: u32) -> Result<Option<u32>> {
            self.encounter_calls.push(id);
            Ok(match id {
                1 | 2 => Some(30),
                3 => Some(33),
                _ => None,
            })
        }
    }
    // Each line has the accepted operands and the rejected operands for that kind.
    let cases = [
        (1, [1, 2], [1, 3]),
        (1, [1, 2], [2, 2]),
        (2, [1, 1], [1, 0]),
        (2, [1, 1], [2, 1]),
        (23, [1, 1], [1, 0]),
        (3, [1, 0], [2, 0]),
        (4, [1, 1], [2, 1]),
        (4, [1, 1], [1, 2]),
        (5, [1, 7], [2, 7]),
        (30, [1, 7], [1, 8]),
        (6, [67, 0], [1, 0]),
        (6, [469, 0], [1, 0]),
        (7, [1, 300], [1, 301]),
        (7, [1, 1], [1, 0]),
        (29, [1, 1], [2, 1]),
        (12, [65537, 0], [65538, 0]),
        (13, [0, 1], [0, 0]),
        (14, [0xff, 0x400], [0x100, 0x400]),
        (14, [0, 0x400], [0, 0]),
        (14, [1, 0], [1, 0x200]),
        (15, [60, 2], [0, 2]),
        (15, [60, 2], [61, 2]),
        (15, [60, 2], [60, 3]),
        (17, [1, 1], [2, 1]),
        (17, [1, 1], [1, 2]),
        (28, [1, 0], [2, 0]),
        (28, [1, 1], [1, 2]),
        (31, [1, 0], [0, 0]),
        (31, [1, 2], [1, 4]),
        (31, [1, 2], [1, 3]),
        (33, [0, 2], [0, 3]),
        (35, [2, 0], [3, 0]),
        (36, [3, 0], [4, 0]),
        (37, [1, 1], [2, 1]),
        (37, [1, 1], [1, 0]),
        (39, [1, 0], [2, 0]),
        (42, [1, 7], [1, 8]),
        (42, [1, 7], [2, 7]),
        (43, [1, 0], [2, 0]),
    ];
    let mut inputs = Authored {
        rules: Rules::from_classic_config(60, false, 1.0f32.to_bits()).unwrap(),
        encounter_calls: Vec::new(),
    };
    for (kind, [a, b], [bad_a, bad_b]) in cases {
        let pass = condition(1, kind, [a, b, u32::MAX, u32::MAX]);
        let fail = condition(2, kind, [bad_a, bad_b, 0, 0]);
        let loaded = SourceConditions::load([fail, pass], &mut inputs).unwrap();
        assert_eq!(loaded.get(1), Some(&pass), "kind {kind}");
        assert!(loaded.get(2).is_none(), "kind {kind}");
    }
    assert_eq!(inputs.encounter_calls, [1, 0, 0, 0, 1, 2, 1, 4, 1, 2, 1, 3]);
    for kind in [0, 10, 11, 18, 26, 40] {
        let pass = condition(1, kind, [u32::MAX; 4]);
        assert_eq!(
            SourceConditions::load([pass], &mut Inputs).unwrap().get(1),
            Some(&pass)
        );
    }
    assert!(
        SourceConditions::load([condition(1, 44, [0; 4])], &mut Inputs)
            .unwrap()
            .get(1)
            .is_none()
    );
}
