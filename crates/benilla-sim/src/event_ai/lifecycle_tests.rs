use super::*;

#[test]
fn reversed_timer_disables_without_drawing_and_equal_timer_draws_nothing() {
    // CreatureEventAI.cpp:35-48 leaves the old timer intact on reversal.
    let mut timer = 91;
    let mut enabled = true;
    let mut draw = |_, _| -> Result<u32, ()> { panic!("unexpected draw") };
    assert!(!update_holder_timer(&mut timer, &mut enabled, [9, 2], &mut draw).unwrap());
    assert_eq!((timer, enabled), (91, false));
    assert!(update_holder_timer(&mut timer, &mut enabled, [7, 7], &mut draw).unwrap());
    assert_eq!((timer, enabled), (7, false));
}

#[test]
fn clock_keeps_strict_boundary_and_unsigned_debt() {
    // CreatureEventAI.cpp:1932-1974.
    let mut clock = EventClock::constructed();
    clock.reset();
    assert_eq!(clock.begin_update(500), None);
    assert_eq!((clock.remaining, clock.debt), (0, 500));
    assert_eq!(clock.begin_update(0), None);
    assert_eq!(clock.begin_update(1), Some(501));
    clock.reset();
    assert_eq!(clock.begin_update(499), None);
    assert_eq!(clock.begin_update(u32::MAX), Some(498));
}

#[test]
fn varied_timer_uses_the_calling_thread_draw_once() {
    let mut timer = 0;
    let mut enabled = true;
    let mut requests = Vec::new();
    assert!(
        update_holder_timer(&mut timer, &mut enabled, [3, 9], &mut |min, max| {
            requests.push((min, max));
            Ok::<_, ()>(8)
        })
        .unwrap()
    );
    assert_eq!((timer, enabled), (8, true));
    assert_eq!(requests, [(3, 9)]);
}
