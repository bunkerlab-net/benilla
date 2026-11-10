// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned-builder component comparisons, never sealed world execution.
use benilla_sim::source_transports::{
    generate_template, segment_position, MoTransport, TaxiNode, TransportMotion, TransportStep,
};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};
#[derive(Deserialize)]
struct Node {
    id: u32,
    index: u32,
    map: u32,
    position: [u32; 3],
    flags: u32,
    delay: u32,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    entry: u32,
    path: u32,
    speed: u32,
    accel: u32,
    period: Option<u32>,
    nodes: Vec<Node>,
    initial: u32,
    samples: Vec<u32>,
}
fn compare(cases: &str, expected: &str) -> (usize, usize, usize) {
    let cases: Vec<Case> = serde_json::from_str(cases).unwrap();
    let mut rows = expected.lines().peekable();
    let (mut states, mut segments, mut unclamped) = (0, 0, 0);
    for case in cases {
        let definition = MoTransport {
            entry: case.entry,
            path: case.path,
            speed: case.speed,
            acceleration: case.accel,
        };
        let nodes: Arc<[TaxiNode]> = case
            .nodes
            .into_iter()
            .map(|n| TaxiNode {
                id: n.id,
                path: case.path,
                index: n.index,
                map: n.map,
                position: n.position,
                flags: n.flags,
                delay: n.delay,
            })
            .collect::<Vec<_>>()
            .into();
        let maps = nodes.iter().map(|n| (n.map, 0)).collect::<BTreeMap<_, _>>();
        let mut t = generate_template(definition, nodes, &maps).unwrap();
        if let Some(period) = case.period {
            t.override_period(period).unwrap();
        }
        let mut m = TransportMotion::new(case.initial);
        let first = &t.key_frames[0];
        let n = t.nodes[first.node];
        let mut pose = [
            n.position[0],
            n.position[1],
            n.position[2],
            first.orientation,
        ];
        let mut transferred = false;
        for (i, now) in case.samples.into_iter().enumerate() {
            if !transferred {
                match m
                    .update(
                        &t,
                        definition,
                        n.map,
                        now,
                        if i % 2 == 0 { 0 } else { u32::MAX },
                    )
                    .unwrap()
                {
                    TransportStep::Unchanged => {}
                    TransportStep::Position { pose: p, .. } => pose = p,
                    TransportStep::Transfer { .. } => transferred = true,
                }
            }
            let line = rows.next().unwrap();
            let mut fields = line.split_whitespace();
            assert_eq!(fields.next().unwrap(), case.name);
            let expected = fields
                .map(|v| v.parse::<u64>().unwrap())
                .collect::<Vec<_>>();
            let actual = vec![
                i as u64,
                m.progress().into(),
                m.current_frame() as u64,
                m.next_frame() as u64,
                m.is_moving().into(),
                m.position_expiry().into(),
                transferred.into(),
                pose[0].into(),
                pose[1].into(),
                pose[2].into(),
                pose[3].into(),
            ];
            assert_eq!(actual, expected, "{} snapshot {now}", case.name);
            states += 1;
        }
        while rows.peek().is_some_and(|row| row.starts_with("S ")) {
            let line = rows.next().unwrap();
            let mut fields = line.split_whitespace();
            assert_eq!(fields.next(), Some("S"));
            assert_eq!(fields.next().unwrap(), case.name);
            let numbers = fields
                .map(|v| v.parse::<u32>().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(numbers.len(), 9);
            let frame = &t.key_frames[numbers[0] as usize];
            let parameter = segment_position(&t, definition, frame, numbers[1]).unwrap();
            assert_eq!(
                parameter, numbers[2],
                "{} frame {} progress {}",
                case.name, numbers[0], numbers[1]
            );
            let spline = &t.splines[frame.spline.unwrap()];
            assert_eq!(
                spline.evaluate(frame.index, parameter).unwrap(),
                numbers[3..6]
            );
            assert_eq!(
                spline.derivative(frame.index, parameter).unwrap(),
                numbers[6..9]
            );
            segments += 1;
            if !(0.0..=1.0).contains(&f32::from_bits(parameter)) {
                unclamped += 1;
            }
        }
    }
    assert!(rows.next().is_none());
    (states, segments, unclamped)
}
#[test]
fn complete_movement_and_unclamped_segments_match_pinned_builder() {
    let counts = compare(
        include_str!("../tools/fixtures/source_transport_movement/cases.json"),
        include_str!("../tools/fixtures/source_transport_movement/expected.tsv"),
    );
    assert!(counts.0 > 1000 && counts.1 > 100 && counts.2 > 0);
    println!(
        "Authored pinned movement comparisons: states={}, segments={}, unclamped={}",
        counts.0, counts.1, counts.2
    );
}
#[test]
fn retained_raw_stock_movement_or_explicit_absence() {
    let Some(root) = std::env::var_os("BENILLA_F4_MO_TRANSPORT_STOCK") else {
        println!("SKIP raw stock movement oracle: set BENILLA_F4_MO_TRANSPORT_STOCK");
        return;
    };
    let root = std::path::Path::new(&root);
    let cases = std::fs::read_to_string(root.join("movement-oracle/cases.json")).unwrap();
    let expected = std::fs::read_to_string(root.join("stock-movement-expected.tsv")).unwrap();
    let counts = compare(&cases, &expected);
    println!("Unsealed raw stock movement comparisons: states={}, segments={}, unclamped={}; not executed ObjectMgr, SQL periods, collision, or F1 acceptance",counts.0,counts.1,counts.2);
}
