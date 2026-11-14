// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn captured() -> EventAiCapturedInput {
    let input = super::tests::input();
    let EventAiRows::Complete(mut owners) = input.owners else {
        panic!()
    };
    let owner = owners.remove(0);
    EventAiCapturedInput {
        selectors: EventAiRows::Complete(vec![-99001, 60001]),
        events: owner.entry_events,
        owners: EventAiRows::Complete(vec![EventAiCapturedOwner {
            spawn_id: owner.spawn_id,
            database_guid: owner.database_guid,
            entry: owner.entry,
            captured_entry: owner.entry,
            gender: owner.gender,
            name: owner.name,
            name_locales: owner.name_locales,
        }]),
        broadcasts: input.broadcasts,
        locales: input.locales,
    }
}

#[test]
fn captured_chances_and_entry_guid_order() {
    for chance in [0, 10, 100] {
        let mut input = captured();
        let EventAiRows::Complete(rows) = &mut input.events else {
            panic!()
        };
        rows[0].chance = chance;
        let mut guid = rows[0].clone();
        guid.id = 90000;
        guid.creature_id = -99001;
        rows.insert(0, guid);
        if chance == 0 {
            assert!(matches!(input.compile(), Err(NativeError::EventAi(_))));
            continue;
        }
        let catalog = input.compile().unwrap();
        assert_eq!(
            catalog
                .events(2)
                .unwrap()
                .iter()
                .map(AggroText::event_id)
                .collect::<Vec<_>>(),
            vec![91001, 90000]
        );
    }
}

#[test]
fn captured_dependencies_and_runtime_context_fail_closed() {
    let mut input = captured();
    input.selectors = EventAiRows::Unknown;
    assert!(input.compile().is_err());
    let mut input = captured();
    input.broadcasts = EventAiRows::Complete(vec![]);
    assert!(input.compile().is_err());
    let mut input = captured();
    let EventAiRows::Complete(owners) = &mut input.owners else {
        panic!()
    };
    owners[0].captured_entry = 1;
    assert!(input.compile().is_err());
    assert!(captured().compile_runtime(EventAiRows::Unknown).is_err());
}

#[test]
fn captured_ooc_text_accepts_only_exact_timer_flags_and_action_shape() {
    let mut input = captured();
    let EventAiRows::Complete(rows) = &mut input.events else {
        panic!()
    };
    rows[0].event_type = 1;
    rows[0].flags = 1;
    rows[0].parameters = [1000, 2000, 3000, 4000, 0, 0];
    rows[0].actions[0].parameters[2] = rows[0].actions[0].parameters[0];
    let catalog = input.clone().compile().unwrap();
    assert_eq!(catalog.ooc_events(2).unwrap().len(), 1);
    assert!(catalog.events(2).unwrap().is_empty());
    for mutation in 0..8 {
        let mut changed = input.clone();
        let EventAiRows::Complete(rows) = &mut changed.events else {
            panic!()
        };
        match mutation {
            0 => rows[0].event_type = 2,
            1 => rows[0].inverse_phase_mask = 1,
            2 => rows[0].flags = 2,
            3 => rows[0].parameters[0] = 2001,
            4 => rows[0].parameters[3] = 2999,
            5 => rows[0].parameters[4] = 1,
            6 => rows[0].actions[0].action_type = 4,
            7 => rows[0].actions[1].action_type = 1,
            _ => unreachable!(),
        }
        assert!(changed.compile().is_err(), "mutation {mutation}");
    }
    let EventAiRows::Complete(rows) = &mut input.events else {
        panic!()
    };
    rows[0].flags = 0;
    rows[0].parameters = [0, i64::from(u32::MAX), 0, i64::from(u32::MAX), 0, 0];
    assert!(input.compile().is_ok());
}
