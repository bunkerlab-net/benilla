// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::VecDeque;
fn event(repeatable: bool) -> OocText {
    OocText::new(
        AggroText::new(41, 10, [101, 103, 107]).unwrap(),
        [100, 200],
        [300, 400],
        repeatable,
    )
    .unwrap()
}
#[test]
fn authored_ooc_boundary_failed_chance_and_exact_draw_order() {
    let mut expected = VecDeque::from([
        (
            OocDraw::Initial {
                event_id: 41,
                min: 100,
                max: 200,
            },
            150,
        ),
        (OocDraw::Chance { event_id: 41 }, 99),
        (
            OocDraw::Repeat {
                event_id: 41,
                min: 300,
                max: 400,
            },
            321,
        ),
        (OocDraw::Chance { event_id: 41 }, 0),
        (OocDraw::Action { event_id: 41 }, u32::MAX),
        (
            OocDraw::Repeat {
                event_id: 41,
                min: 300,
                max: 400,
            },
            399,
        ),
    ]);
    let mut draw = |request| -> Result<u32, Error> {
        let (wanted, word) = expected.pop_front().unwrap();
        assert_eq!(request, wanted);
        Ok(word)
    };
    let events = [event(true)];
    let mut timers = OocTimers::new(&events, &mut draw).unwrap();
    assert!(timers
        .update(500, false, false, &mut draw)
        .unwrap()
        .is_empty());
    assert_eq!(timers.snapshot().holders[0].timer_ms, 150);
    assert!(timers
        .update(1, false, false, &mut draw)
        .unwrap()
        .is_empty());
    assert_eq!(timers.snapshot().holders[0].timer_ms, 321);
    assert_eq!(
        timers.update(501, false, false, &mut draw).unwrap(),
        vec![TextSelection {
            event_id: 41,
            text_id: 101,
            text_slot: 0
        }]
    );
    assert_eq!(timers.snapshot().holders[0].timer_ms, 399);
    assert!(expected.is_empty());
}
#[test]
fn combat_and_evade_count_down_but_do_not_fire_enter_only_resets_clock() {
    let mut calls = 0;
    let mut draw = |_: OocDraw| -> Result<u32, Error> {
        calls += 1;
        Ok(150)
    };
    let events = [event(true)];
    let mut timers = OocTimers::new(&events, &mut draw).unwrap();
    timers.update(100, false, false, &mut draw).unwrap();
    timers.enter_combat();
    assert_eq!(timers.snapshot().update_remaining_ms, 500);
    assert_eq!(timers.snapshot().holders[0].timer_ms, 150);
    assert!(timers
        .update(501, true, false, &mut draw)
        .unwrap()
        .is_empty());
    assert!(timers
        .update(501, false, true, &mut draw)
        .unwrap()
        .is_empty());
    assert_eq!(timers.snapshot().holders[0].timer_ms, 0);
    timers.reset(&mut draw).unwrap();
    assert_eq!(timers.snapshot().holders[0].timer_ms, 150);
    assert_eq!(calls, 2);
}
#[test]
fn nonrepeatable_row_fires_once_until_source_reset() {
    let text = AggroText::new(41, 100, [101, 103, 107]).unwrap();
    let event = OocText::new(text, [0, 0], [50, 60], false).unwrap();
    let mut requests = Vec::new();
    let mut draw = |request| -> Result<u32, Error> {
        requests.push(request);
        Ok(if matches!(request, OocDraw::Repeat { .. }) {
            55
        } else {
            2
        })
    };
    let events = [event];
    let mut timers = OocTimers::new(&events, &mut draw).unwrap();
    assert_eq!(
        timers.update(501, false, false, &mut draw).unwrap()[0].text_id,
        107
    );
    assert!(!timers.snapshot().holders[0].enabled);
    assert!(timers
        .update(501, false, false, &mut draw)
        .unwrap()
        .is_empty());
    assert_eq!(
        requests,
        vec![
            OocDraw::Chance { event_id: 41 },
            OocDraw::Action { event_id: 41 },
            OocDraw::Repeat {
                event_id: 41,
                min: 50,
                max: 60
            },
        ]
    );
}
#[test]
fn mid_timer_snapshot_roundtrip_retains_shared_debt_and_next_draw() {
    let event = OocText::new(
        AggroText::new(41, 100, [101, 103, 107]).unwrap(),
        [900, 900],
        [1000, 1000],
        true,
    )
    .unwrap();
    let mut draw = |_: OocDraw| -> Result<u32, Error> { Ok(2) };
    let events = [event];
    let mut timers = OocTimers::new(&events, &mut draw).unwrap();
    timers.update(600, false, false, &mut draw).unwrap();
    timers.update(200, false, false, &mut draw).unwrap();
    let snapshot = timers.snapshot();
    assert_eq!(snapshot.holders[0].timer_ms, 300);
    assert_eq!(snapshot.debt_ms, 200);
    let encoded = serde_json::to_vec(&snapshot).unwrap();
    let mut reopened =
        OocTimers::restore(&events, serde_json::from_slice(&encoded).unwrap()).unwrap();
    for diff in [300, 1, 500, 501] {
        assert_eq!(
            timers.update(diff, false, false, &mut draw).unwrap(),
            reopened.update(diff, false, false, &mut draw).unwrap()
        );
        assert_eq!(timers.snapshot(), reopened.snapshot());
    }
}
