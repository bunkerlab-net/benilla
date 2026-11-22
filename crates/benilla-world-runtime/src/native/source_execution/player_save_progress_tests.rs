// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

struct AuthoredQuestInputs;
impl QuestLoaderOwner for AuthoredQuestInputs {}

fn authored_templates(rows: &[(u32, u32)]) -> SourceQuestMap {
    SourceQuestMap::load_quests(
        rows.iter().map(|&(id, method)| {
            let mut words = [0; 131];
            [words[0], words[1]] = [id, method];
            SourceQuestTemplate::from_decoded_fields(words, Default::default())
        }),
        &mut AuthoredQuestInputs,
    )
    .unwrap()
}

#[test]
fn authored_quest_projection_orders_states_and_uses_cached_timer() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::super::tests::construct(&mut objects, &mut units);
    let quests = authored_templates(&[(10, 0), (20, 1), (30, 0), (40, 1), (50, 0)]);
    // QuestDef.h:365-373.
    let new = QuestStatusData::default();
    // Player.cpp:16117-16163. Unrewarded autocomplete rows skip both write branches.
    player.quest_status.extend([
        (
            50,
            QuestStatusData {
                rewarded: true,
                status: 1,
                ..new
            },
        ),
        (
            40,
            QuestStatusData {
                state: QuestSaveState::Unchanged,
                ..new
            },
        ),
        (
            30,
            QuestStatusData {
                state: QuestSaveState::Changed,
                ..new
            },
        ),
        (
            20,
            QuestStatusData {
                status: 3,
                explored: true,
                timer_ms: 2_999,
                mob: [7, 1, 0, 9],
                item: [0, 4, 8, 0],
                state: QuestSaveState::Changed,
                ..new
            },
        ),
        (10, new),
    ]);
    let mut rows = Vec::new();
    player
        .save_quest_status(
            &quests,
            Calendar::new(CalendarOwner::World, 700),
            9,
            &mut |op| {
                rows.push(op);
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(
        rows,
        [
            SourcePlayerSaveObservation::Row(RowOperation::Update(RowUpdate::Quest(QuestRow {
                guid: 9,
                quest: 20,
                status: 3,
                rewarded: false,
                explored: true,
                timer_seconds: 702,
                mob: [7, 1, 0, 9],
                item: [0, 4, 8, 0],
            }))),
            SourcePlayerSaveObservation::Row(RowOperation::Insert(RowInsert::Quest(QuestRow {
                guid: 9,
                quest: 50,
                status: 1,
                rewarded: true,
                explored: false,
                timer_seconds: 700,
                mob: [0; 4],
                item: [0; 4],
            }))),
        ]
    );
    assert_eq!(player.quest_status[&10].state, QuestSaveState::New);
    assert_eq!(player.quest_status[&30].state, QuestSaveState::Changed);
    assert_eq!(player.quest_status[&20].state, QuestSaveState::Changed);
}

#[test]
fn authored_dirty_quest_refuses_missing_template_and_timer_overflow() {
    // C:p1502-1607 reaches neither branch.
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::super::tests::construct(&mut objects, &mut units);
    let templates = authored_templates(&[(9, 1)]);
    for (state, missing, overflow) in [
        (QuestSaveState::New, 16120, 16131),
        (QuestSaveState::Changed, 16142, 16151),
    ] {
        player.quest_status.insert(
            9,
            QuestStatusData {
                timer_ms: 1000,
                state,
                ..Default::default()
            },
        );
        let calendar = Calendar::new(CalendarOwner::World, i64::MAX);
        let mut no_write = |_| panic!("invalid quest must not emit a row");
        let err = player
            .save_quest_status(&SourceQuestMap::default(), calendar, 7, &mut no_write)
            .unwrap_err();
        assert_eq!(
            err.id,
            format!("execution.player_save.Player_{missing}.quest_template_null_undefined")
        );
        let err = player
            .save_quest_status(&templates, calendar, 7, &mut no_write)
            .unwrap_err();
        assert_eq!(
            err.id,
            format!("execution.player_save.Player_{overflow}.quest_timer_add_undefined")
        );
    }
}

#[test]
fn authored_weekly_projection_requires_dirty_nonempty_set() {
    let mut objects = SourceObjects::default();
    let mut units = SourceUnits::default();
    let mut player = super::super::tests::construct(&mut objects, &mut units);
    // Player.cpp:16170,16180-16185.
    for (dirty, quests) in [(false, &[41][..]), (true, &[])] {
        player.weekly_quests = quests.iter().copied().collect();
        player.weekly_quests_changed = dirty;
        player
            .save_weekly_quest_status(9, &mut |_| panic!("guarded weekly save"))
            .unwrap();
    }
    player.weekly_quests.extend([42, 19]);
    let mut rows = Vec::new();
    player
        .save_weekly_quest_status(9, &mut |op| {
            rows.push(op);
            Ok(())
        })
        .unwrap();
    assert_eq!(
        rows,
        [
            SourcePlayerSaveObservation::Row(RowOperation::Delete(RowDelete::WeeklyQuestsByGuid(
                9
            ))),
            SourcePlayerSaveObservation::Row(RowOperation::Insert(RowInsert::WeeklyQuest {
                guid: 9,
                quest: 19
            })),
            SourcePlayerSaveObservation::Row(RowOperation::Insert(RowInsert::WeeklyQuest {
                guid: 9,
                quest: 42
            })),
        ]
    );
    assert!(player.weekly_quests_changed);
}
