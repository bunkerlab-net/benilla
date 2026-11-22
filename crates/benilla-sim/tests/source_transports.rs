// SPDX-License-Identifier: GPL-3.0-or-later
// Authored component fixtures, not sealed transport execution.
use benilla_sim::source_transports::TransportSpline;

#[test]
fn source_catmull_rom_uses_original_controls_and_unclamped_parameter() {
    let points = [0.0_f32, 10.0, 20.0, 30.0].map(|x| [x.to_bits(), 0, 0]);
    let spline = TransportSpline::from_controls(&points).unwrap();
    assert_eq!(
        spline.evaluate(2, 0.5_f32.to_bits()).unwrap(),
        [15.0_f32.to_bits(), 0, 0]
    );
    assert_eq!(
        spline.evaluate(2, 1.5_f32.to_bits()).unwrap(),
        [25.0_f32.to_bits(), 0, 0]
    );
    assert_eq!(
        spline.derivative(2, 0.5_f32.to_bits()).unwrap(),
        [10.0_f32.to_bits(), 0, 0]
    );
}

#[test]
fn complete_template_bits_match_the_pinned_source_oracle() {
    compare_case_rows(
        serde_json::from_str(include_str!(
            "../tools/fixtures/source_transports/cases.json"
        ))
        .unwrap(),
        include_str!("../tools/fixtures/source_transports/expected.tsv"),
    );
}

fn compare_case_rows(cases: Vec<serde_json::Value>, expected: &str) {
    use benilla_sim::source_transports::{generate_template, MoTransport, TaxiNode};
    use std::{collections::BTreeMap, sync::Arc};
    assert_eq!(cases.len(), expected.lines().count());
    for (case, row) in cases.iter().zip(expected.lines()) {
        let number = |name: &str| case[name].as_u64().unwrap() as u32;
        let definition = MoTransport {
            entry: number("entry"),
            path: number("path"),
            speed: number("speed"),
            acceleration: number("accel"),
        };
        let nodes: Vec<_> = case["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| {
                let word = |name: &str| n[name].as_u64().unwrap() as u32;
                TaxiNode {
                    id: word("id"),
                    path: definition.path,
                    index: word("index"),
                    map: word("map"),
                    position: std::array::from_fn(|a| n["position"][a].as_u64().unwrap() as u32),
                    flags: word("flags"),
                    delay: word("delay"),
                }
            })
            .collect();
        let maps: BTreeMap<_, _> = nodes.iter().map(|n| (n.map, 0)).collect();
        let mut template = generate_template(definition, Arc::from(nodes), &maps).unwrap();
        if let Some(period) = case["period"].as_u64() {
            template.override_period(period as u32).unwrap();
        }
        let mut fields: Vec<i128> = vec![
            template.entry.into(),
            template.in_instance.into(),
            template.path_time.into(),
            template.accel_time.into(),
            template.accel_dist.into(),
            template.maps_used.len() as i128,
        ];
        fields.extend(template.maps_used.iter().map(|n| i128::from(*n)));
        fields.push(template.key_frames.len() as i128);
        for f in &template.key_frames {
            fields.extend([
                template.nodes[f.node].index.into(),
                f.index as i128,
                f.orientation.into(),
                f.dist_since_stop.into(),
                f.dist_until_stop.into(),
                f.dist_from_prev.into(),
                f.time_from.into(),
                f.time_to.into(),
                f.teleport.into(),
                f.update.into(),
                f.arrive_time.into(),
                f.departure_time.into(),
                f.next_dist_from_prev.into(),
                f.next_arrive_time.into(),
                f.spline.map_or(-1, |s| s as i128),
            ]);
        }
        fields.push(template.splines.len() as i128);
        for s in &template.splines {
            fields.extend([s.points.len() as i128, s.first as i128, s.last as i128]);
            fields.extend(s.points.iter().flatten().map(|n| i128::from(*n)));
            fields.extend(s.lengths.iter().map(|n| i128::from(*n)));
        }
        let mut original = row.split_whitespace();
        assert_eq!(original.next().unwrap(), case["name"].as_str().unwrap());
        let original: Vec<i128> = original.map(|n| n.parse().unwrap()).collect();
        assert_eq!(fields, original, "{}", case["name"]);
    }
}

fn inputs(
    name: &str,
) -> (
    benilla_sim::source_transports::MoTransport,
    std::sync::Arc<[benilla_sim::source_transports::TaxiNode]>,
    std::collections::BTreeMap<u32, u32>,
) {
    use benilla_sim::source_transports::{MoTransport, TaxiNode};
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../tools/fixtures/source_transports/cases.json"
    ))
    .unwrap();
    let c = cases.iter().find(|c| c["name"] == name).unwrap();
    let u = |name: &str| c[name].as_u64().unwrap() as u32;
    let definition = MoTransport {
        entry: u("entry"),
        path: u("path"),
        speed: u("speed"),
        acceleration: u("accel"),
    };
    let nodes: Vec<_> = c["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|n| {
            let u = |name: &str| n[name].as_u64().unwrap() as u32;
            TaxiNode {
                id: u("id"),
                path: definition.path,
                index: u("index"),
                map: u("map"),
                position: std::array::from_fn(|i| n["position"][i].as_u64().unwrap() as u32),
                flags: u("flags"),
                delay: u("delay"),
            }
        })
        .collect();
    let maps = nodes.iter().map(|n| (n.map, 0)).collect();
    (definition, nodes.into(), maps)
}
fn template(name: &str) -> benilla_sim::source_transports::TransportTemplate {
    let (definition, nodes, maps) = inputs(name);
    benilla_sim::source_transports::generate_template(definition, nodes, &maps).unwrap()
}
#[test]
fn control_nodes_and_teleport_pairs_are_not_keyframes_and_legs_remain_separate() {
    for name in ["teleport", "map_change"] {
        let t = template(name);
        assert_eq!(
            t.key_frames
                .iter()
                .map(|f| t.nodes[f.node].index)
                .collect::<Vec<_>>(),
            [1, 2, 3, 6, 7, 8, 9, 10]
        );
        assert_eq!(t.splines.len(), 2);
        assert!(t.key_frames[2].teleport && t.key_frames.last().unwrap().teleport);
        assert_eq!(t.key_frames[2].spline, Some(0));
        assert_eq!(t.key_frames[3].spline, Some(1));
        assert_eq!(t.key_frames[2].next_dist_from_prev, 0);
        assert_eq!(t.key_frames[3].dist_from_prev, 0);
    }
}
#[test]
fn consecutive_stops_no_stop_anchor_and_short_cruise_branches_are_preserved() {
    let t = template("consecutive");
    assert_eq!(t.key_frames[0].dist_since_stop, 0);
    assert_eq!(t.key_frames[1].dist_since_stop, 0);
    assert!(t.key_frames[0].dist_until_stop != 0 && t.key_frames[1].dist_until_stop != 0);
    let t = template("no_stop");
    assert!(t.nodes.iter().all(|n| n.flags == 0));
    assert_eq!(t.key_frames[0].time_from, 0);
    assert!(t.key_frames.last().unwrap().teleport);
    for (name, short) in [("short", true), ("cruise", false)] {
        let t = template(name);
        let threshold = 2.0 * f32::from_bits(t.accel_dist);
        assert_eq!(
            t.key_frames
                .iter()
                .all(
                    |f| f32::from_bits(f.dist_since_stop) + f32::from_bits(f.dist_until_stop)
                        < threshold
                ),
            short
        );
    }
}
#[test]
fn one_control_leg_reads_the_next_backing_point_and_one_frame_is_legal() {
    let t = template("one_control_leg");
    assert_eq!(t.splines[0].last, 1);
    assert_ne!(t.splines[0].points[0], t.splines[0].points[1]);
    let t = template("single_frame");
    assert_eq!(t.key_frames.len(), 1);
    assert_eq!(t.key_frames[0].spline, None);
    assert!(t.key_frames[0].teleport);
}
#[test]
fn period_override_changes_only_source_fields_and_ferry_refresh_is_frame_twelve() {
    let before = template("period");
    for period in [0, 1, 123_456] {
        let mut t = before.clone();
        t.override_period(period).unwrap();
        let mut expected = before.clone();
        expected.path_time = period;
        expected.key_frames.last_mut().unwrap().departure_time = period;
        assert_eq!(t, expected);
    }
    for name in ["feathermoon", "teldrassil"] {
        let t = template(name);
        assert_eq!(
            t.key_frames
                .iter()
                .enumerate()
                .filter(|(_, f)| f.update)
                .map(|(i, _)| i)
                .collect::<Vec<_>>(),
            [12]
        );
    }
}
#[test]
fn undefined_source_controls_frames_and_time_conversions_refuse() {
    use benilla_sim::source_transports::{generate_template, Error};
    let (d, n, m) = inputs("short");
    for count in 0..3 {
        assert_eq!(
            generate_template(d, n[..count].to_vec().into(), &m),
            Err(Error::SourceUndefined(
                "orientation_controls_or_empty_frames"
            ))
        );
    }
    let mut nodes = n.to_vec();
    nodes[1].flags = 1;
    assert_eq!(
        generate_template(d, nodes.into(), &m),
        Err(Error::SourceUndefined("teleport_without_previous_frame"))
    );
    let mut ferry = d;
    ferry.path = 293;
    let mut ferry_nodes = n.to_vec();
    for node in &mut ferry_nodes {
        node.path = 293;
    }
    assert_eq!(
        generate_template(ferry, ferry_nodes.into(), &m),
        Err(Error::SourceUndefined("refresh_frame_12"))
    );
    let mut nodes = n.to_vec();
    nodes[1].delay = u32::MAX;
    assert_eq!(
        generate_template(d, nodes.into(), &m),
        Err(Error::SourceUndefined("float_to_uint32"))
    );
}
#[test]
fn missing_maps_and_cross_map_instances_refuse_without_default_classification() {
    use benilla_sim::source_transports::{generate_template, Error};
    let (d, n, _) = inputs("short");
    assert_eq!(
        generate_template(d, n, &std::collections::BTreeMap::new()),
        Err(Error::MissingMap(0))
    );
    let (d, n, mut maps) = inputs("map_change");
    maps.insert(1, 1);
    assert_eq!(generate_template(d, n, &maps), Err(Error::CrossMapInstance));
}
#[test]
fn retained_raw_stock_template_bits_or_explicit_absence() {
    let Ok(root) = std::env::var("BENILLA_F4_TRANSPORT_STOCK") else {
        println!("SKIP unsealed raw stock template oracle: set BENILLA_F4_TRANSPORT_STOCK");
        return;
    };
    let root = std::path::Path::new(&root);
    let cases =
        serde_json::from_slice(&std::fs::read(root.join("stock-transport-inputs.json")).unwrap())
            .unwrap();
    let expected = std::fs::read_to_string(root.join("stock-transport-expected.tsv")).unwrap();
    compare_case_rows(cases, &expected);
    println!("Unsealed raw stock templates match the pinned source oracle; not effective ObjectMgr or F1 acceptance");
}
