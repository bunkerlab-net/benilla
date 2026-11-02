// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn source(rows: Vec<QuestRow>) -> CapturedOrdinaryQuests {
    let relations = relations(&rows);
    CapturedOrdinaryQuests {
        source: [7; 32],
        quests: Some(rows),
        starters: Some(relations.starters.clone()),
        finishers: Some(relations.finishers.clone()),
        loaded_starters: Some(relations.starters),
        loaded_finishers: Some(relations.finishers),
        givers: Some(vec![CapturedQuestGiver {
            spawn: 1,
            database_guid: 1,
            generation: 1,
            entry: 900,
            npc_flags: 2,
            unit_flags: 0,
            script_name: String::new(),
            loaded_script: 0,
        }]),
    }
}
fn context(rows: &[QuestRow]) -> QuestDependencySource {
    let items = rows
        .iter()
        .flat_map(|q| q.required_item.into_iter().chain([q.excluded.source_item]))
        .filter(|id| *id != 0)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    QuestDependencySource {
        source: [7; 32],
        mechanics: None,
        scripts: Some(vec![]),
        allowed_areas: vec![9],
        global_spell_areas: Some(vec![]),
        item_keys: items,
        item_deletion_effects: Some(vec![]),
        gossip: Some(vec![CapturedGossipScope {
            entry: 900,
            menu: 0,
            options: vec![],
        }]),
    }
}
#[test]
fn captured_givers_compile_loader_flags_and_preserve_order() {
    let mut rows = vec![row(783, 0, 0), row(7, 0, 25)];
    rows[0].required_creature = [0; 4];
    rows[0].required_creature_count = [0; 4];
    rows[0].special_flags = 0;
    rows[1].previous_quest = 783;
    rows[1].special_flags = 0;
    let capture = captured(&rows);
    let mut input = source(rows.clone());
    input.loaded_starters.as_mut().unwrap().reverse();
    let deps = context(&rows).resolve(&input).unwrap();
    let compiled = input.compile(&capture, &deps).unwrap();
    assert_eq!(compiled.quests()[1].special_flags, KILL | SPEAK);
    assert_eq!(compiled.relations().starters[0].quest, 7);
    assert_eq!(compiled.catalog().ids().collect::<Vec<_>>(), vec![7, 783]);
}
#[test]
fn captured_givers_refuse_unknown_hooks_flags_scope_and_order() {
    let rows = vec![row(1, 0, 0)];
    let capture = captured(&rows);
    let input = source(rows.clone());
    let mut deps = context(&rows);
    deps.global_spell_areas = None;
    assert!(deps.resolve(&input).is_err());
    let mut deps = context(&rows);
    deps.scripts = None;
    assert!(deps.resolve(&input).is_err());
    let mut bad = input.clone();
    bad.givers.as_mut().unwrap()[0].npc_flags = 0;
    assert!(context(&rows).resolve(&bad).is_err());
    let deps = context(&rows).resolve(&input).unwrap();
    let mut bad = input.clone();
    bad.loaded_finishers = None;
    assert!(bad.compile(&capture, &deps).is_err());
    let mut bad = input.clone();
    bad.givers.as_mut().unwrap()[0].generation = 2;
    assert!(bad.compile(&capture, &deps).is_err());
    let mut bad = input;
    bad.givers.as_mut().unwrap()[0].script_name = "authored_hook".into();
    bad.givers.as_mut().unwrap()[0].loaded_script = 1;
    let mut deps = context(&rows);
    deps.scripts = Some(vec![CapturedQuestScript {
        id: 1,
        name: "authored_hook".into(),
        callbacks: vec![QuestCallback::Accept],
    }]);
    assert!(deps.resolve(&bad).is_err());
}
#[test]
fn captured_real_shaped_tail_and_attached_source_item_are_not_filtered() {
    let mut rows = vec![
        row(783, 0, 0),
        row(7, 0, 0),
        row(15, 0, 0),
        row(21, 0, 0),
        row(3104, 0, 0),
    ];
    rows[2].next_in_chain = 21;
    rows[3].excluded.complete_script = 21;
    rows[4].excluded.source_item = 9571;
    let capture = captured(&rows);
    let input = source(rows.clone());
    let deps = context(&rows).resolve(&input).unwrap();
    assert!(matches!(
        input.compile(&capture, &deps),
        Err(Error::Relationship {
            quest: 21,
            field: "CompleteScript",
            target: 21,
            ..
        })
    ));
    rows[3].excluded.complete_script = 0;
    let capture = captured(&rows);
    let input = source(rows.clone());
    let deps = context(&rows).resolve(&input).unwrap();
    let compiled = input.compile(&capture, &deps).unwrap();
    assert_eq!(
        compiled.catalog().ids().collect::<Vec<_>>(),
        [7, 15, 21, 783, 3104]
    );
    let source_item = &compiled.catalog().row(3104).unwrap().excluded;
    assert_eq!(
        (source_item.source_item, source_item.source_item_count),
        (9571, 1)
    );
}
