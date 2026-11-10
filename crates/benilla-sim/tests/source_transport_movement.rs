// SPDX-License-Identifier: GPL-3.0-or-later
use benilla_sim::source_transports::{
    generate_template, MoTransport, TaxiNode, TransportMotion, TransportStep,
};
use std::{collections::BTreeMap, sync::Arc};
fn fixture() -> (
    MoTransport,
    benilla_sim::source_transports::TransportTemplate,
) {
    let definition = MoTransport {
        entry: 42,
        path: 1,
        speed: 10,
        acceleration: 2,
    };
    let nodes = (0..7)
        .map(|i| TaxiNode {
            id: i + 1,
            path: 1,
            index: i,
            map: 0,
            position: [(i as f32 * 10.0).to_bits(), 0, 0],
            flags: if i == 1 || i == 5 { 2 } else { 0 },
            delay: 2,
        })
        .collect::<Vec<_>>();
    (
        definition,
        generate_template(definition, Arc::from(nodes), &BTreeMap::from([(0, 0)])).unwrap(),
    )
}
#[test]
fn dwell_departure_and_supplied_diff_use_snapshot_not_diff() {
    // Transports.cpp:356-377 ignores diff and includes arrival but excludes departure.
    let (d, t) = fixture();
    let mut a = TransportMotion::new(0);
    let mut b = TransportMotion::new(0);
    assert_eq!(
        a.update(&t, d, 0, 1999, 1).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(
        b.update(&t, d, 0, 1999, u32::MAX).unwrap(),
        TransportStep::Unchanged
    );
    assert!(!a.is_moving());
    assert_eq!(
        a.update(&t, d, 0, 2000, 1).unwrap(),
        b.update(&t, d, 0, 2000, u32::MAX).unwrap()
    );
    assert!(a.is_moving());
    assert_eq!(a.progress(), 2000);
    assert_eq!(
        a.update(&t, d, 0, 2000, 50).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(a.progress(), 2000);
}
#[test]
fn short_paths_and_unsigned_snapshot_guard_precede_modulo() {
    // Transports.cpp:353-365 returns before modulo even for zero period.
    let (d, mut t) = fixture();
    let mut m = TransportMotion::new(u32::MAX - 5);
    assert_eq!(
        m.update(&t, d, 0, 4, 999).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(m.progress(), u32::MAX - 5);
    t.path_time = 0;
    t.key_frames.truncate(1);
    assert_eq!(
        m.update(&t, d, 0, u32::MAX, 1).unwrap(),
        TransportStep::Unchanged
    );
    t.key_frames.clear();
    assert_eq!(
        m.update(&t, d, 0, u32::MAX, 1).unwrap(),
        TransportStep::Unchanged
    );
}
#[test]
fn tracker_resets_to_fifty_without_carrying_excess() {
    // Transports.cpp:395-400 resets the saturated ShortTimeTracker to 50.
    let (d, t) = fixture();
    let mut m = TransportMotion::new(0);
    m.update(&t, d, 0, 2001, 0).unwrap();
    assert_eq!(m.position_expiry(), 50);
    assert_eq!(
        m.update(&t, d, 0, 2050, u32::MAX).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(m.position_expiry(), 1);
    assert!(matches!(
        m.update(&t, d, 0, 2051, 0).unwrap(),
        TransportStep::Position { .. }
    ));
    assert_eq!(m.position_expiry(), 50);
}
#[test]
fn zero_period_refuses_at_the_executed_modulo_only() {
    let (d, mut t) = fixture();
    t.path_time = 0;
    let mut m = TransportMotion::new(10);
    assert_eq!(
        m.update(&t, d, 0, 10, 99).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(
        m.update(&t, d, 0, 11, 99).unwrap_err(),
        benilla_sim::source_transports::Error::SourceUndefined("period_modulo_zero")
    );
    // Period one is defined and produces zero cycle progress; Transports.cpp:365.
    t.path_time = 1;
    let mut defined = TransportMotion::new(10);
    assert_eq!(
        defined.update(&t, d, 0, 11, 99).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(defined.progress(), 11);
}
#[test]
fn multiple_crossings_arrival_equality_and_progress_modulo() {
    // Transports.cpp:365-380 includes arrival, excludes departure, and crosses multiple frames.
    let (d, mut t) = fixture();
    t.path_time = 1000;
    for (i, frame) in t.key_frames.iter_mut().enumerate() {
        frame.arrive_time = i as u32 * 200;
        frame.departure_time = frame.arrive_time + 100;
        frame.next_arrive_time = frame.arrive_time + 200;
    }
    let mut m = TransportMotion::new(0);
    assert_eq!(
        m.update(&t, d, 0, 400, 0).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(
        (m.current_frame(), m.next_frame(), m.is_moving()),
        (2, 3, false)
    );
    m.update(&t, d, 0, 499, 0).unwrap();
    assert!(!m.is_moving());
    m.update(&t, d, 0, 500, 0).unwrap();
    assert!(m.is_moving());
    let mut wrapped = TransportMotion::new(0);
    assert_eq!(
        wrapped.update(&t, d, 0, 1001, 0).unwrap(),
        TransportStep::Unchanged
    );
    assert_eq!(wrapped.progress(), 1001);
    assert_eq!(wrapped.current_frame(), 0);
    assert!(!wrapped.is_moving());
}
