// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::BTreeMap;

#[path = "loot_reference_audit_tests.rs"]
mod reference_audit;

#[derive(Default)]
struct Inputs {
    rows: BTreeMap<LootStoreKind, Vec<LootRow>>,
    queries: Vec<LootStoreKind>,
    lookups: Vec<(&'static str, u32)>,
    notices: Vec<LootNotice>,
}

impl LootInputs for Inputs {
    fn rows(&mut self, kind: LootStoreKind, sql: &str) -> Result<Vec<LootRow>> {
        assert_eq!(
            sql,
            format!("SELECT entry, item, ChanceOrQuestChance, groupid, mincountOrRef, maxcount, condition_id FROM {}", kind.table())
        );
        self.queries.push(kind);
        Ok(self.rows.remove(&kind).unwrap_or_default())
    }

    fn condition_exists(&mut self, id: u16) -> Result<bool> {
        self.lookups.push(("condition", u32::from(id)));
        Ok(id != 3)
    }

    fn condition_without_player(&mut self, id: u16) -> Result<bool> {
        self.lookups.push(("without_player", u32::from(id)));
        Ok(id == 2)
    }

    fn item_exists(&mut self, id: u32) -> Result<bool> {
        self.lookups.push(("item", id));
        Ok(id != 27)
    }

    fn notice(&mut self, notice: LootNotice) {
        self.notices.push(notice);
    }
}

fn row(entry: u32) -> LootRow {
    LootRow {
        entry,
        item: 25,
        chance: 10.0,
        group: 0,
        min_count_or_ref: 1,
        max_count: 1,
        condition: 0,
    }
}

#[test]
fn family_keeps_source_query_order_and_independent_template_keys() {
    use LootStoreKind::*;
    let mut inputs = Inputs {
        rows: BTreeMap::from([(Creature, vec![row(6)]), (Reference, vec![row(6), row(1)])]),
        ..Inputs::default()
    };
    let family = SourceLootStores::load(&mut inputs).unwrap();
    assert_eq!(
        inputs.queries,
        [
            Creature,
            Fishing,
            Gameobject,
            Item,
            Mail,
            Pickpocketing,
            Skinning,
            Disenchant,
            Reference
        ]
    );
    assert!(family.store(Creature).have_loot_for(6));
    assert!(!family.store(Creature).have_loot_for(1));
    assert!(family.store(Reference).have_loot_for(1));
    assert!(family.store(Reference).have_loot_for(6));
    assert!(family.store(Mail).get_loot_for(6).is_none());
    assert!(!Mail.rates_allowed());
    assert!(!Reference.rates_allowed());
    assert!(Creature.rates_allowed());
}

#[test]
fn row_checks_keep_source_precedence_and_boundaries() {
    use LootRowRejection::*;
    let rows = [
        (
            LootRow {
                max_count: 256,
                condition: 3,
                ..row(1)
            },
            Some(MaxCountWidth),
        ),
        (
            LootRow {
                max_count: 255,
                ..row(2)
            },
            None,
        ),
        (
            LootRow {
                condition: 3,
                group: 128,
                ..row(3)
            },
            Some(MissingCondition),
        ),
        (
            LootRow {
                condition: 1,
                ..row(4)
            },
            None,
        ),
        (
            LootRow {
                condition: 1,
                min_count_or_ref: -2,
                ..row(5)
            },
            Some(ConditionRequiresPlayer),
        ),
        (
            LootRow {
                condition: 2,
                min_count_or_ref: -2,
                item: 27,
                ..row(6)
            },
            None,
        ),
        (
            LootRow {
                group: 128,
                min_count_or_ref: 0,
                ..row(7)
            },
            Some(GroupWidth),
        ),
        (
            LootRow {
                group: 127,
                ..row(8)
            },
            None,
        ),
        (
            LootRow {
                min_count_or_ref: 0,
                item: 27,
                ..row(9)
            },
            Some(ZeroMinimum),
        ),
        (
            LootRow {
                item: 27,
                chance: 0.0,
                ..row(10)
            },
            Some(MissingItem),
        ),
        (
            LootRow {
                chance: 0.0,
                ..row(11)
            },
            Some(UngroupedZeroChance),
        ),
        (
            LootRow {
                chance: -0.0,
                group: 1,
                ..row(12)
            },
            None,
        ),
        (
            LootRow {
                chance: -0.0000009,
                max_count: 0,
                ..row(13)
            },
            Some(LowChance),
        ),
        (
            LootRow {
                chance: -0.000001,
                ..row(14)
            },
            None,
        ),
        (
            LootRow {
                min_count_or_ref: 2,
                max_count: 1,
                ..row(15)
            },
            Some(CountRange),
        ),
        (
            LootRow {
                min_count_or_ref: 2,
                max_count: 2,
                ..row(16)
            },
            None,
        ),
        (
            LootRow {
                chance: -10.0,
                min_count_or_ref: -2,
                ..row(17)
            },
            Some(NegativeReferenceChance),
        ),
        (
            LootRow {
                chance: 0.0,
                min_count_or_ref: -2,
                ..row(18)
            },
            Some(UngroupedZeroChance),
        ),
        (
            LootRow {
                chance: 0.0,
                group: 1,
                min_count_or_ref: -2,
                max_count: 0,
                ..row(19)
            },
            None,
        ),
        (
            LootRow {
                chance: 0.0000009,
                min_count_or_ref: -2,
                ..row(20)
            },
            None,
        ),
    ];
    let kind = LootStoreKind::Reference;
    let mut inputs = Inputs {
        rows: BTreeMap::from([(kind, rows.iter().map(|&(row, _)| row).collect())]),
        ..Inputs::default()
    };
    let family = SourceLootStores::load(&mut inputs).unwrap();
    let store = family.store(kind);
    for (row, reason) in rows {
        assert_eq!(
            store.have_loot_for(row.entry),
            reason.is_none(),
            "entry {}",
            row.entry
        );
        if let Some(reason) = reason {
            assert!(inputs.notices.contains(&LootNotice::Rejected {
                kind,
                entry: row.entry,
                item: row.item,
                reason
            }));
        }
    }
    assert_eq!(
        inputs.lookups,
        [
            ("item", 25), // max 255
            ("condition", 3),
            ("condition", 1),
            ("item", 25),
            ("condition", 1),
            ("without_player", 1),
            ("condition", 2),
            ("without_player", 2),
            ("item", 25), // group 127
            ("item", 27),
            ("item", 25),
            ("item", 25),
            ("item", 25),
            ("item", 25),
            ("item", 25),
            ("item", 25),
        ]
    );
}

#[test]
fn rows_keep_admitted_ordinals_and_group_buckets() {
    let kind = LootStoreKind::Creature;
    let mut inputs = Inputs {
        rows: BTreeMap::from([(
            kind,
            vec![
                LootRow {
                    group: 3,
                    chance: 0.0,
                    ..row(6)
                },
                LootRow { item: 27, ..row(6) },
                LootRow {
                    group: 3,
                    chance: -50.0,
                    item: 26,
                    condition: 1,
                    ..row(6)
                },
                LootRow {
                    item: 28,
                    min_count_or_ref: -12,
                    max_count: 2,
                    ..row(6)
                },
                LootRow {
                    group: 3,
                    chance: 102.0,
                    item: 29,
                    ..row(6)
                },
            ],
        )]),
        ..Inputs::default()
    };
    let family = SourceLootStores::load(&mut inputs).unwrap();
    let template = family.store(kind).get_loot_for(6).unwrap();
    assert_eq!(template.entries.len(), 1);
    let reference = &template.entries[0];
    assert_eq!(
        (
            reference.item_index,
            reference.item,
            reference.min_count_or_ref,
            reference.max_count
        ),
        (3, 28, -12, 2)
    );
    assert_eq!(template.groups.len(), 3);
    assert!(template.groups[..2]
        .iter()
        .all(|g| g.equal.is_empty() && g.explicit.is_empty()));
    let group = &template.groups[2];
    assert_eq!(
        group.equal.iter().map(|r| r.item_index).collect::<Vec<_>>(),
        [1]
    );
    assert_eq!(
        group
            .explicit
            .iter()
            .map(|r| r.item_index)
            .collect::<Vec<_>>(),
        [2, 4]
    );
    let quest = &group.explicit[0];
    assert_eq!((quest.group(), quest.chance, quest.condition), (3, 50.0, 1));
    assert!(quest.needs_quest());
    assert!(!group.equal[0].needs_quest());
    assert!(inputs.notices.contains(&LootNotice::GroupChanceAbove101 {
        kind,
        entry: 6,
        group: 3,
        chance: 102.0
    }));
    assert!(inputs
        .notices
        .contains(&LootNotice::EqualChanceGroupExhausted {
            kind,
            entry: 6,
            group: 3,
            chance: 102.0
        }));
}

#[test]
fn group_notices_exclude_quest_chances_and_preserve_boundaries() {
    let kind = LootStoreKind::Creature;
    let mut inputs = Inputs {
        rows: BTreeMap::from([(
            kind,
            vec![
                LootRow {
                    group: 1,
                    chance: 0.0,
                    ..row(1)
                },
                LootRow {
                    group: 1,
                    chance: 99.0,
                    ..row(1)
                },
                LootRow {
                    group: 1,
                    chance: -100.0,
                    ..row(1)
                },
                LootRow {
                    group: 1,
                    chance: 0.0,
                    ..row(2)
                },
                LootRow {
                    group: 1,
                    chance: 101.0,
                    ..row(2)
                },
            ],
        )]),
        ..Inputs::default()
    };
    let family = SourceLootStores::load(&mut inputs).unwrap();
    assert_eq!(family.store(kind).templates().count(), 2);
    assert_eq!(
        inputs
            .notices
            .iter()
            .filter(|notice| !matches!(notice, LootNotice::Empty { .. }))
            .collect::<Vec<_>>(),
        [&LootNotice::EqualChanceGroupExhausted {
            kind,
            entry: 2,
            group: 1,
            chance: 101.0
        }]
    );
}

#[test]
fn missing_owners_refuse_at_the_call_that_needs_them() {
    struct Unbound(LootRow, bool);
    impl LootInputs for Unbound {
        fn rows(&mut self, kind: LootStoreKind, _: &str) -> Result<Vec<LootRow>> {
            Ok(if kind == LootStoreKind::Creature {
                vec![self.0]
            } else {
                Vec::new()
            })
        }
        fn condition_exists(&mut self, id: u16) -> Result<bool> {
            if self.1 {
                Ok(true)
            } else {
                // Run the trait's default lookup through an unbound input.
                Bare.condition_exists(id)
            }
        }
    }
    struct Bare;
    impl LootInputs for Bare {}

    assert_eq!(
        SourceLootStores::load(&mut Bare).unwrap_err().id,
        "execution.loot.LootMgr_135.rows_owner"
    );
    for (row, has_condition, error) in [
        (
            row(1),
            false,
            "execution.loot.LootMgr_327.item_prototype_owner",
        ),
        (
            LootRow {
                condition: 1,
                ..row(1)
            },
            false,
            "execution.loot.LootMgr_162.condition_owner",
        ),
        (
            LootRow {
                condition: 1,
                min_count_or_ref: -2,
                ..row(1)
            },
            true,
            "execution.loot.LootMgr_169.condition_without_player_owner",
        ),
    ] {
        assert_eq!(
            SourceLootStores::load(&mut Unbound(row, has_condition))
                .unwrap_err()
                .id,
            error
        );
    }
}
