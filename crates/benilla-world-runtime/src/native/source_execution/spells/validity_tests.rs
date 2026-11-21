use super::*;
use benilla_sim::source_rng::{DrawCursor, Mt19937, SourceRandom};
use std::collections::BTreeSet;

fn items(ids: impl IntoIterator<Item = u32>) -> SourceStartupItems {
    let mut items = SourceStartupItems::default();
    let mut engine = Mt19937::from_seed(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
    items
        .validate(
            ids.into_iter()
                .map(|id| (id, StartupItemPrototype::default()))
                .collect(),
            &BTreeSet::new(),
            &mut random,
        )
        .unwrap();
    assert_eq!(random.cursor().raw, 0);
    items
}

#[test]
fn null_none_and_unrelated_effects_do_not_read_item_prototypes() {
    let spells = SourceSpellStores::default();
    let mut log = Vec::new();
    let mut observe = |issue| log.push(issue);
    assert!(!spells
        .is_spell_valid(None, None, true, &mut observe)
        .unwrap());
    let spell = SpellTemplate {
        id: 1,
        effect2: 6,
        reagent1: 99,
        ..Default::default()
    };
    assert!(spells
        .is_spell_valid(Some(&spell), None, true, &mut observe)
        .unwrap());
    assert!(log.is_empty());
}

#[test]
fn create_item_checks_products_before_positive_reagents() {
    let spells = SourceSpellStores::default();
    let mut log = Vec::new();
    let mut spell = SpellTemplate {
        id: 1,
        effect1: 24,
        effect2: 24,
        effectitemtype1: 2,
        effectitemtype2: 3,
        reagent1: -19,
        reagent2: 0,
        reagent3: 4,
        ..Default::default()
    };
    for (ids, line, reference) in [([2, 8], 2030, 3), ([2, 3], 2068, 4)] {
        let items = items(ids);
        assert!(!spells
            .is_spell_valid(Some(&spell), Some(&items), true, &mut |issue| log
                .push(issue))
            .unwrap());
        assert_eq!(
            log.pop(),
            Some(SpellValidityNotice {
                line,
                spell: 1,
                operand: reference
            })
        );
        assert!(log.is_empty());
    }
    let items = items([2, 3, 4]);
    assert!(spells
        .is_spell_valid(Some(&spell), Some(&items), true, &mut |issue| log
            .push(issue))
        .unwrap());
    assert_eq!(
        spells
            .is_spell_valid(Some(&spell), None, true, &mut |issue| log.push(issue))
            .unwrap_err()
            .id,
        "execution.spells.SpellMgr_2023.item_template_owner"
    );
    spell.effectitemtype2 = 99;
    assert!(!spells
        .is_spell_valid(Some(&spell), Some(&items), false, &mut |issue| log
            .push(issue))
        .unwrap());
    assert!(log.is_empty());
}

#[test]
fn learn_spell_uses_the_loaded_trigger_and_reports_child_first() {
    let mut spells = SourceSpellStores::default();
    let mut log = Vec::new();
    let spell = SpellTemplate {
        id: 1,
        effect1: 36,
        effecttriggerspell1: 2,
        reagent1: 99,
        ..Default::default()
    };
    let loaded_items = items([3]);
    assert!(!spells
        .is_spell_valid(Some(&spell), Some(&loaded_items), true, &mut |issue| log
            .push(issue))
        .unwrap());
    let learning = SpellValidityNotice {
        line: 2048,
        spell: 1,
        operand: 2,
    };
    assert_eq!(log, [learning]);
    spells
        .insert_template(SpellTemplate {
            id: 2,
            effect3: 24,
            effectitemtype3: 4,
            ..Default::default()
        })
        .unwrap();
    log.clear();
    assert!(!spells
        .is_spell_valid(Some(&spell), Some(&loaded_items), true, &mut |issue| log
            .push(issue))
        .unwrap());
    assert_eq!(
        log,
        [
            SpellValidityNotice {
                line: 2030,
                spell: 2,
                operand: 4
            },
            learning
        ]
    );
    log.clear();
    let loaded_items = items([4]);
    assert!(spells
        .is_spell_valid(Some(&spell), Some(&loaded_items), true, &mut |issue| log
            .push(issue))
        .unwrap());
    assert!(log.is_empty());
}
