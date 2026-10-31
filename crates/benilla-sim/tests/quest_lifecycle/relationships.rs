// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn introduction(id: u32) -> QuestRow {
    let mut quest = row(id, 0, 3);
    quest.required_creature = [0; 4];
    quest.required_creature_count = [0; 4];
    quest.special_flags = 0;
    quest
}
fn resolve(rows: Vec<QuestRow>, capture: RelationshipCapture) -> Result<Catalog, Error> {
    let r = relations(&rows);
    let mut evidence = audit();
    evidence.complete_relation_scan = false;
    Catalog::from_relationship_capture(rows, r, evidence, &capture)
}
#[test]
fn authored_introduction_kill_and_collection_branches_preserve_rewarded_prerequisites() {
    let mut intro = introduction(783);
    intro.next_in_chain = 7;
    let mut kills = row(7, 0, 5);
    kills.previous_quest = 783;
    kills.next_in_chain = 15;
    let mut next = row(15, 0, 0);
    next.previous_quest = 7;
    let mut delivery = introduction(5261);
    delivery.previous_quest = 783;
    delivery.next_in_chain = 33;
    let mut collection = row(33, 1, 0);
    collection.previous_quest = 5261;
    let rows = vec![intro, kills, next, delivery, collection];
    let capture = captured(&rows);
    let c = resolve(rows, capture).unwrap();
    let mut s = State::new(&c);
    let mut h = Ledger::new();
    assert_eq!(
        can_take(&c, &s, &h, 7),
        Err(Error::Rejected(Gate::Previous))
    );
    accept(&c, &mut s, &mut h, 783);
    assert_eq!(
        can_take(&c, &s, &h, 7),
        Err(Error::Rejected(Gate::Previous))
    );
    reward(&c, &mut s, &mut h, 783);
    let saved = serde_json::to_vec(&s.snapshot()).unwrap();
    s = State::restore(&c, serde_json::from_slice(&saved).unwrap()).unwrap();
    assert!(can_take(&c, &s, &h, 7).is_ok());
    assert!(can_take(&c, &s, &h, 5261).is_ok());
    assert!(can_take(&c, &s, &h, 33).is_err());
    let xp = h.xp;
    let money = h.money;
    assert!(reduce(
        &c,
        &s,
        &h,
        s.next_sequence(),
        Command::Reward {
            quest: 783,
            giver: 900,
            announce: true,
            choice: 0,
        }
    )
    .is_err());
    assert_eq!((h.xp, h.money), (xp, money));
    accept(&c, &mut s, &mut h, 7);
    kill(&c, &mut s, &mut h);
    kill(&c, &mut s, &mut h);
    assert!(can_take(&c, &s, &h, 15).is_err());
    reward(&c, &mut s, &mut h, 7);
    assert!(can_take(&c, &s, &h, 15).is_ok());
    accept(&c, &mut s, &mut h, 5261);
    reward(&c, &mut s, &mut h, 5261);
    assert!(can_take(&c, &s, &h, 33).is_ok());
}
#[test]
fn unknown_external_dangling_conflicting_and_unsupported_edges_are_precise() {
    let original = vec![row(7, 0, 0)];
    let mut capture = captured(&original);
    capture.inbound = None;
    assert!(matches!(
        resolve(original.clone(), capture),
        Err(Error::Relationship {
            field: "inbound",
            reason: "unknown_coverage",
            ..
        })
    ));
    for (field, target, reason) in [
        ("NextQuestId", -7, "uncaptured_incoming_quest"),
        ("NextQuestInChain", 7, "uncaptured_incoming_quest"),
        ("BreadcrumbForQuestId", 7, "uncaptured_incoming_quest"),
    ] {
        let mut capture = captured(&original);
        let mut external = RelationshipRow::from(&row(99, 0, 0));
        match field {
            "NextQuestId" => external.next = target,
            "NextQuestInChain" => external.chain = target as u32,
            _ => external.breadcrumb = target as u32,
        }
        capture.inbound.as_mut().unwrap().push(external);
        assert_eq!(
            resolve(original.clone(), capture).unwrap_err(),
            Error::Relationship {
                quest: 99,
                field,
                target: i64::from(target),
                reason
            }
        );
    }
    for (field, target, reason) in [
        ("PrevQuestId", 99, "uncaptured_reference"),
        ("NextQuestId", 99, "uncaptured_reference"),
        ("NextQuestInChain", 99, "uncaptured_reference"),
        ("ExclusiveGroup", -3, "unsupported_path"),
        ("BreadcrumbForQuestId", 99, "unsupported_path"),
        ("PrevQuestId", i32::MIN, "numeric_domain"),
        ("NextQuestInChain", i32::MIN, "numeric_domain"),
        ("StartScript", 9, "unsupported_path"),
        ("CompleteScript", 9, "unsupported_path"),
        ("RequiredCondition", 9, "unsupported_path"),
        ("SrcItemId", 9, "unsupported_path"),
    ] {
        let mut rows = original.clone();
        match field {
            "PrevQuestId" => rows[0].previous_quest = target,
            "NextQuestId" => rows[0].next_quest = target,
            "NextQuestInChain" => rows[0].next_in_chain = target as u32,
            "ExclusiveGroup" => rows[0].excluded.exclusive_group = target,
            "BreadcrumbForQuestId" => rows[0].excluded.breadcrumb_for_quest = target as u32,
            "StartScript" => rows[0].excluded.start_script = target as u32,
            "CompleteScript" => rows[0].excluded.complete_script = target as u32,
            "RequiredCondition" => rows[0].excluded.required_condition = target as u32,
            _ => rows[0].excluded.source_item = target as u32,
        }
        let capture = captured(&rows);
        let expected = if field == "NextQuestInChain" && target == i32::MIN {
            2147483648
        } else {
            i64::from(target)
        };
        assert_eq!(
            resolve(rows, capture).unwrap_err(),
            Error::Relationship {
                quest: 7,
                field,
                target: expected,
                reason
            }
        );
    }
    let mut capture = captured(&original);
    capture.own[0].previous = 9;
    assert!(matches!(
        resolve(original.clone(), capture),
        Err(Error::Relationship {
            quest: 7,
            reason: "conflicting_rows",
            ..
        })
    ));
    let rows = vec![row(7, 0, 0); MAX_RELATIONSHIP_QUESTS + 1];
    assert!(matches!(
        resolve(rows, captured(&original)),
        Err(Error::Relationship {
            reason: "closure_limit",
            ..
        })
    ));
}
#[test]
fn incoming_selected_edge_cannot_disappear_and_chain_is_not_a_prerequisite() {
    let mut source = row(1, 0, 0);
    source.next_quest = -2;
    let rows = vec![source, row(2, 0, 0)];
    let mut capture = captured(&rows);
    capture.inbound = Some(vec![]);
    assert!(matches!(
        resolve(rows, capture),
        Err(Error::Relationship {
            quest: 1,
            reason: "missing_selected_edge",
            ..
        })
    ));
    let mut source = row(1, 0, 0);
    source.next_in_chain = 2;
    let c = catalog(vec![source, row(2, 0, 0)]);
    assert!(can_take(&c, &State::new(&c), &Ledger::new(), 2).is_ok());
}
