// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde::Deserialize;
#[derive(Deserialize)]
struct Case {
    name: String,
    kind: String,
    path: Vec<Point>,
    flags: u32,
    velocity: u32,
    id: u32,
    offset: i32,
    facing: [u32; 3],
    steps: serde_json::Value,
}
fn state(name: &str, sample: i32, s: &MoveSpline, events: &[u32]) -> String {
    let mut values = vec![
        sample.to_string(),
        s.id().to_string(),
        s.flags().to_string(),
        s.speed().to_string(),
        s.time_passed().to_string(),
        s.raw_index().to_string(),
        s.current_path_index().unwrap().to_string(),
        u32::from(s.initialized()).to_string(),
        u32::from(s.finalized()).to_string(),
    ];
    let mut push = |v: u32| values.push(v.to_string());
    push(if s.initialized() {
        s.duration().unwrap()
    } else {
        0
    });
    push(if s.initialized() {
        s.elapsed_value().unwrap()
    } else {
        0
    });
    for v in s
        .current_destination()
        .unwrap()
        .into_iter()
        .chain(s.final_destination())
    {
        push(v);
    }
    for v in if s.initialized() {
        s.compute_position().unwrap()
    } else {
        [0; 4]
    } {
        push(v);
    }
    let spline = s.spline();
    push(spline.first() as u32);
    push(spline.last() as u32);
    push(spline.points().len() as u32);
    for v in spline.points().iter().flatten() {
        push(*v);
    }
    push(spline.lengths().len() as u32);
    for v in spline.lengths() {
        push(*v as u32);
    }
    push(events.len() as u32);
    for v in events {
        push(*v);
    }
    format!("{name} {}", values.join(" "))
}
#[test]
fn pinned_cpp_move_spline_and_spline_outputs_match_every_bit() {
    let cases: Vec<Case> = serde_json::from_str(include_str!(
        "../../tools/fixtures/source_spline/cases.json"
    ))
    .unwrap();
    let mut rows = Vec::new();
    for c in &cases {
        let args = Args {
            path: c.path.clone(),
            facing: c.facing.map(Some),
            flags: c.flags,
            velocity: c.velocity,
            spline_id: c.id,
            path_index_offset: c.offset,
        };
        if c.kind == "M" {
            let mut s = MoveSpline::default();
            rows.push(state(&c.name, -1, &s, &[]));
            s.initialize(&args).unwrap();
            rows.push(state(&c.name, 0, &s, &[]));
            for (i, step) in c.steps.as_array().unwrap().iter().enumerate() {
                let op = step[0].as_u64().unwrap();
                let diff = step[1].as_u64().unwrap() as u32;
                let mut events = Vec::new();
                match op {
                    0 => s.update_state(diff, |e| events.push(e as u32)).unwrap(),
                    1 => s.interrupt(),
                    2 => s.finalize().unwrap(),
                    3 => {
                        let mut args = args.clone();
                        args.flags |= DONE;
                        s.initialize(&args).unwrap();
                    }
                    _ => panic!("unknown oracle operation"),
                }
                rows.push(state(&c.name, i as i32 + 1, &s, &events));
            }
        } else {
            let mut s = Spline::initialize(
                &c.path,
                if c.flags & FLYING != 0 {
                    Mode::CatmullRom
                } else {
                    Mode::Linear
                },
                if c.flags & CYCLIC != 0 {
                    Some(usize::from(c.flags & ENTER_CYCLE != 0))
                } else {
                    None
                },
            )
            .unwrap();
            s.init_lengths().unwrap();
            for (sample, t) in c.steps.as_array().unwrap().iter().enumerate() {
                let t = t.as_u64().unwrap() as u32;
                let (index, u) = s.compute_index(t).unwrap();
                let mut v = vec![
                    sample as u32,
                    index as u32,
                    u,
                    s.first() as u32,
                    s.last() as u32,
                ];
                v.extend(s.lengths().iter().map(|x| *x as u32));
                for i in s.first()..s.last() {
                    v.push(s.segment_length(i).unwrap());
                    v.extend(s.evaluate(i, t, false).unwrap());
                    v.extend(s.evaluate(i, t, true).unwrap());
                }
                rows.push(format!(
                    "{} {}",
                    c.name,
                    v.iter().map(u32::to_string).collect::<Vec<_>>().join(" ")
                ));
            }
        }
    }
    let expected = include_str!("../../tools/fixtures/source_spline/expected.tsv")
        .lines()
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), expected.len());
    for (actual, expected) in rows.iter().zip(expected) {
        assert_eq!(
            actual,
            expected,
            "case {}",
            actual.split_whitespace().next().unwrap()
        );
    }
    let receipt: serde_json::Value = serde_json::from_str(include_str!(
        "../../tools/fixtures/source_spline/provenance.json"
    ))
    .unwrap();
    assert_eq!(receipt["cases"], cases.len());
    assert_eq!(receipt["rows"], rows.len());
    assert_eq!(receipt["ubsan"].as_array().unwrap().len(), 12);
    assert_eq!(receipt["asan"]["kind"], "heap-buffer-overflow");
}
fn args(path: Vec<Point>) -> Args {
    Args {
        path,
        facing: [Some(0); 3],
        flags: 0,
        velocity: 1f32.to_bits(),
        spline_id: 1,
        path_index_offset: 0,
    }
}
#[test]
fn signed_duration_conversion_refuses_at_the_cpp_undefined_operation() {
    // Matching C++ operations are executed under UBSan by source_spline_generate.py.
    let unit = vec![[0; 3], [1f32.to_bits(), 0, 0]];
    for (velocity, last) in [
        (1, None),
        (0x7fc00000, None),
        (1f32.to_bits(), Some(0x7f800000)),
    ] {
        let mut a = args(unit.clone());
        a.velocity = velocity;
        if let Some(last) = last {
            a.path[1][0] = last;
        }
        let mut s = MoveSpline::default();
        assert_eq!(
            s.initialize(&a),
            Err(Error("spline_common_duration_conversion_undefined"))
        );
        assert!(!s.initialized() || s.spline().lengths().last() == Some(&0));
    }
    let mut a = args(vec![[0; 3], [0, 0, (-2.0e8f32).to_bits()]]);
    a.flags = FALLING;
    assert_eq!(
        MoveSpline::default().initialize(&a),
        Err(Error("spline_fall_duration_conversion_undefined"))
    );
    let mut a = args(unit);
    a.path_index_offset = i32::MAX;
    let mut s = MoveSpline::default();
    s.initialize(&a).unwrap();
    assert_eq!(
        s.current_path_index(),
        Err(Error("spline_path_index_addition_undefined"))
    );
    a.path_index_offset = i32::MAX - 1;
    s.initialize(&a).unwrap();
    assert_eq!(s.current_path_index().unwrap(), i32::MAX - 1);
    a.flags = DONE;
    s.initialize(&a).unwrap();
    assert_eq!(
        s.current_path_index(),
        Err(Error("spline_path_index_final_addition_undefined"))
    );
    a.flags = CYCLIC;
    a.path_index_offset = 0;
    s.initialize(&a).unwrap();
    a.flags |= DONE;
    s.initialize(&a).unwrap();
    assert_eq!(
        s.current_path_index(),
        Err(Error("spline_path_index_remainder_undefined"))
    );
    a.flags = 0;
    a.path[1][0] = 2147483520f32.to_bits();
    a.velocity = 1000f32.to_bits();
    a.path_index_offset = 0;
    s.initialize(&a).unwrap();
    assert_eq!(s.duration().unwrap(), 2147483520);
    a.path[1][0] = 2147483648f32.to_bits();
    assert_eq!(
        s.initialize(&a),
        Err(Error("spline_common_duration_conversion_undefined"))
    );
}
#[test]
fn default_stop_and_zero_diff_keep_source_lifecycle_quirks() {
    let mut s = MoveSpline::default();
    assert!(s.finalized());
    assert!(!s.initialized());
    assert_eq!(s.current_path_index().unwrap(), 1);
    assert_eq!(
        s.compute_position(),
        Err(Error("spline_position_initialized_assertion"))
    );
    let mut a = args(vec![[0; 3]; 4]);
    s.initialize(&a).unwrap();
    assert_eq!(s.spline().lengths(), [0, 0, 1, 1, 1]);
    let mut events = Vec::new();
    s.update_state(1, |e| events.push(e)).unwrap();
    assert_eq!(events, [UpdateResult::NextSegment]);
    assert!(!s.finalized());
    s.update_state(0, |e| events.push(e)).unwrap();
    assert_eq!(
        events,
        [UpdateResult::NextSegment, UpdateResult::NextSegment]
    );
    s.update_state(0, |e| events.push(e)).unwrap();
    assert!(s.finalized());
    assert_eq!(s.raw_index(), 3);
    assert_eq!(s.current_path_index().unwrap(), 3);
    a.flags = DONE;
    s.initialize(&a).unwrap();
    assert!(!s.initialized());
    assert!(s.finalized());
    assert_eq!(s.raw_index(), 3);
    assert_eq!(s.speed(), a.velocity);
    assert_eq!(s.time_passed(), 0);
}
#[test]
fn signed_spline_length_conversion_and_fall_monotonic_assertion_are_not_saturation() {
    let mut s = Spline::initialize(
        &[[0; 3], [2147483648f32.to_bits(), 0, 0]],
        Mode::Linear,
        None,
    )
    .unwrap();
    assert_eq!(
        s.init_lengths(),
        Err(Error("spline_length_conversion_undefined"))
    );
    let mut a = args(vec![
        [0, 0, 100f32.to_bits()],
        [0, 0, 0],
        [0, 0, 50f32.to_bits()],
    ]);
    a.flags = FALLING;
    assert_eq!(
        MoveSpline::default().initialize(&a),
        Err(Error("spline_lengths_monotonic_assertion"))
    );
}

#[test]
fn enter_cycle_needs_the_source_virtual_control_after_its_wrap_point() {
    let mut a = args(vec![[0; 3], [1f32.to_bits(), 0, 0]]);
    a.flags = CYCLIC | ENTER_CYCLE;
    assert_eq!(
        MoveSpline::default().initialize(&a),
        Err(Error("spline_cycle_next_control_undefined"))
    );
    a.path.push([2f32.to_bits(), 0, 0]);
    assert!(MoveSpline::default().initialize(&a).is_ok());
}

#[test]
fn pinned_cpp_init_facing_and_packet_arithmetic_match_every_bit() {
    let fixture = include_str!("../../tools/fixtures/source_spline/init_arithmetic.tsv");
    let mut count = 0;
    for line in fixture.lines() {
        let mut columns = line.split_whitespace();
        let kind = columns.next().unwrap();
        let values = columns
            .map(|v| v.parse::<u32>().unwrap())
            .collect::<Vec<_>>();
        let actual = match kind {
            "orientation" => {
                assert_eq!(values.len(), 6);
                relocation_orientation(
                    [values[0], values[1], 0, values[2]],
                    [values[3], values[4], 0, 0],
                )
                .unwrap()
            }
            "wrap" => {
                assert_eq!(values.len(), 2);
                wrap_facing(values[0]).unwrap()
            }
            "pack" => {
                assert_eq!(values.len(), 2);
                pack_xyz([values[0], 0, 0]).unwrap()
            }
            _ => panic!("unknown oracle operation"),
        };
        assert_eq!(actual, *values.last().unwrap(), "{line}");
        count += 1;
    }
    assert_eq!(count, 14);
}

#[test]
fn facing_constructors_preserve_defined_words_and_refuse_each_indeterminate_read() {
    let angle = 0xbf80_0000;
    let target = 0x1234_5678_9abc_def0;
    let cases = [
        (facing_from_angle(angle), [Some(angle), None, None]),
        (
            facing_from_target(target),
            [Some(0x9abc_def0), Some(0x1234_5678), None],
        ),
        (facing_from_target(0), [Some(0), Some(0), None]),
    ];
    for (facing, expected) in cases {
        assert_eq!(facing, expected);
        let mut a = args(vec![[0; 3], [1f32.to_bits(), 0, 0], [2f32.to_bits(), 0, 0]]);
        a.facing = facing;
        a.flags = CYCLIC | ENTER_CYCLE;
        let mut s = MoveSpline::default();
        s.initialize(&a).unwrap();
        assert_eq!(s.facing_words(), expected);
        for (index, word) in expected.into_iter().enumerate() {
            assert_eq!(
                s.facing_word(index),
                word.ok_or(Error("spline_facing_union_read_undefined"))
            );
        }
        s.update_state(10000, |_| {}).unwrap();
        assert_eq!(s.facing_words(), expected);
    }
}
