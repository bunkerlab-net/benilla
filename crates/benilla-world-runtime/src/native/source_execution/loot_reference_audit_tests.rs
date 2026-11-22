// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn reference(entry: u32, target: u32) -> LootRow {
    LootRow {
        min_count_or_ref: -(target as i32),
        ..row(entry)
    }
}

#[test]
fn reference_audit_uses_source_template_iteration_when_disabling_a_cycle() {
    use LootReferenceNotice::*;
    let kind = LootStoreKind::Reference;
    let mut inputs = Inputs {
        rows: BTreeMap::from([(kind, vec![reference(10, 20), reference(20, 10)])]),
        ..Inputs::default()
    };
    let mut family = SourceLootStores::load(&mut inputs).unwrap();
    assert_eq!(
        family
            .store(kind)
            .templates()
            .map(|(id, _)| id)
            .collect::<Vec<_>>(),
        [20, 10]
    );
    let mut notices = Vec::new();
    family
        .check_reference_templates(&BTreeSet::new(), |notice| notices.push(notice))
        .unwrap();
    assert_eq!(
        notices,
        [
            Cycle {
                id: 10,
                active: vec![10, 20]
            },
            TemplateIssue { kind, entry: 20 }
        ]
    );
    assert!(family.unused_reference_ids().is_empty());
    let refs = family.store(kind);
    assert_eq!(
        refs.get_loot_for(10).unwrap().entries[0].min_count_or_ref,
        -20
    );
    assert_eq!(
        refs.get_loot_for(20).unwrap().entries[0].min_count_or_ref,
        0
    );
}

#[test]
fn reference_audit_continues_to_the_next_template_and_preserves_missing_edges() {
    use LootReferenceNotice::*;
    use LootStoreKind::*;
    let mut inputs = Inputs {
        rows: BTreeMap::from([
            (
                Creature,
                vec![
                    reference(7, 50),
                    LootRow {
                        group: 1,
                        chance: 0.0,
                        ..reference(6, 60)
                    },
                    LootRow {
                        group: 1,
                        ..reference(6, 99)
                    },
                ],
            ),
            (
                Reference,
                vec![row(10), row(20), reference(50, 10), row(60)],
            ),
        ]),
        ..Inputs::default()
    };
    let mut family = SourceLootStores::load(&mut inputs).unwrap();
    let mut notices = Vec::new();
    family
        .check_reference_templates(&BTreeSet::from([20]), |notice| notices.push(notice))
        .unwrap();
    assert_eq!(
        notices,
        [
            MissingId { id: 99 },
            TemplateIssue {
                kind: Creature,
                entry: 6
            },
            UnusedId { id: 60 }
        ]
    );
    assert_eq!(family.unused_reference_ids(), &BTreeSet::from([60]));
    assert!(family.store(Reference).have_loot_for(60));
    let group = &family.store(Creature).get_loot_for(6).unwrap().groups[0];
    assert_eq!(group.explicit[0].min_count_or_ref, -99);
    assert_eq!(group.equal[0].min_count_or_ref, -60);
}

#[test]
fn reference_audit_marks_back_edge_but_not_failed_ancestors() {
    use LootReferenceNotice::*;
    use LootStoreKind::*;
    let mut inputs = Inputs {
        rows: BTreeMap::from([
            (Creature, vec![reference(6, 10)]),
            (Reference, vec![reference(10, 20), reference(20, 20)]),
        ]),
        ..Inputs::default()
    };
    let mut family = SourceLootStores::load(&mut inputs).unwrap();
    let mut notices = Vec::new();
    family
        .check_reference_templates(&BTreeSet::new(), |notice| notices.push(notice))
        .unwrap();
    assert_eq!(
        notices,
        [
            Cycle {
                id: 20,
                active: vec![10, 20]
            },
            TemplateIssue {
                kind: Creature,
                entry: 6
            },
            UnusedId { id: 10 }
        ]
    );
    assert_eq!(family.unused_reference_ids(), &BTreeSet::from([10]));
    assert_eq!(
        family.store(Reference).get_loot_for(20).unwrap().entries[0].min_count_or_ref,
        0
    );
}

#[test]
fn reference_audit_refuses_signed_negation_overflow_only() {
    use LootReferenceNotice::*;
    use LootStoreKind::*;
    let mut inputs = Inputs {
        rows: BTreeMap::from([(
            Creature,
            vec![
                LootRow {
                    min_count_or_ref: i32::MIN + 1,
                    ..row(7)
                },
                LootRow {
                    min_count_or_ref: i32::MIN,
                    ..row(6)
                },
            ],
        )]),
        ..Inputs::default()
    };
    let mut family = SourceLootStores::load(&mut inputs).unwrap();
    let mut notices = Vec::new();
    assert_eq!(
        family
            .check_reference_templates(&BTreeSet::new(), |notice| notices.push(notice))
            .unwrap_err()
            .id,
        "execution.loot.LootMgr_78.reference_negation_undefined"
    );
    assert!(notices.is_empty());
    assert_eq!(
        family.store(Creature).get_loot_for(6).unwrap().entries[0].min_count_or_ref,
        i32::MIN
    );

    let mut inputs = Inputs {
        rows: BTreeMap::from([(
            Creature,
            vec![LootRow {
                min_count_or_ref: i32::MIN + 1,
                ..row(7)
            }],
        )]),
        ..Inputs::default()
    };
    let mut family = SourceLootStores::load(&mut inputs).unwrap();
    family
        .check_reference_templates(&BTreeSet::new(), |notice| notices.push(notice))
        .unwrap();
    assert_eq!(
        notices,
        [
            MissingId {
                id: i32::MAX as u32
            },
            TemplateIssue {
                kind: Creature,
                entry: 7
            }
        ]
    );
}

#[test]
fn reference_audit_checks_mail_last_before_the_reference_store() {
    use LootReferenceNotice::*;
    use LootStoreKind::*;
    let mut inputs = Inputs {
        rows: LootStoreKind::LOAD_ORDER
            .into_iter()
            .map(|kind| (kind, vec![reference(2, 10 + kind as u32)]))
            .collect(),
        ..Inputs::default()
    };
    let mut family = SourceLootStores::load(&mut inputs).unwrap();
    let mut notices = Vec::new();
    family
        .check_reference_templates(&BTreeSet::new(), |notice| notices.push(notice))
        .unwrap();
    let source_order = [
        Creature,
        Fishing,
        Gameobject,
        Item,
        Pickpocketing,
        Skinning,
        Disenchant,
        Mail,
        Reference,
    ];
    let expected = source_order
        .into_iter()
        .flat_map(|kind| {
            [
                MissingId {
                    id: 10 + kind as u32,
                },
                TemplateIssue { kind, entry: 2 },
            ]
        })
        .chain([UnusedId { id: 2 }]);
    assert!(notices.into_iter().eq(expected));
}
