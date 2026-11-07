// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use serde_json::Value as Json;
use sha2::{Digest, Sha256};
use std::fmt::Write;

const FIXTURE: &str = include_str!("../../tests/fixtures/source_distributions_gcc12.jsonl");
const WORDS64: [u64; 8] = [
    0,
    1,
    u64::MAX,
    u64::MAX - 1,
    0x8000_0000_0000_0000,
    0x1234_5678_9abc_def0,
    0x1111_1111_1111_1111,
    0xeeee_eeee_eeee_eeee,
];
const NARROW: [u64; 8] = [109, 108, 10, 11, 59, 88, 45, 12];

struct Script<'a, const MIN: u64, const MAX: u64> {
    words: &'a [u64],
    index: usize,
}
impl<const MIN: u64, const MAX: u64> RawGenerator for Script<'_, MIN, MAX> {
    const MIN: u64 = MIN;
    const MAX: u64 = MAX;
    fn next_word(&mut self) -> u64 {
        let value = self.words[self.index % self.words.len()];
        self.index += 1;
        value
    }
}
fn rows() -> impl Iterator<Item = Json> {
    FIXTURE
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
}
fn number(row: &Json, key: &str) -> u64 {
    row[key].as_u64().unwrap()
}
fn hash(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}
fn mt_hash(engine: &Mt19937) -> String {
    let state = engine.state();
    let mut text = String::new();
    for word in state.words {
        write!(&mut text, "{word} ").unwrap();
    }
    write!(&mut text, "{}", state.index).unwrap();
    hash(&text)
}
fn execute<E: RawGenerator>(engine: &mut E, cursor: &mut DrawCursor, row: &Json) {
    assert_eq!(cursor.raw, number(row, "begin"));
    let a = number(row, "a");
    let b = number(row, "b");
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, engine, cursor, &mut sink).unwrap();
    if row["op"] == "shuffle" {
        let mut values: Vec<u64> = (0..a).map(|i| if b != 0 { i % 3 } else { i }).collect();
        random.shuffle(&mut values).unwrap();
        let mut digest = Sha256::new();
        for value in values {
            digest.update(value.to_le_bytes());
        }
        assert_eq!(
            format!("{:x}", digest.finalize()),
            row["out_sha"].as_str().unwrap()
        );
    } else {
        let actual = match row["op"].as_str().unwrap() {
            "u32" => u64::from(random.uniform_u32(a as u32, b as u32).unwrap()),
            "i32" => random.uniform_i32(a as i32, b as i32).unwrap() as u64,
            "u64" => random.uniform_u64(a, b).unwrap(),
            "i64" => random.uniform_i64(a as i64, b as i64).unwrap() as u64,
            "f32" => u64::from(
                random
                    .frand(f32::from_bits(a as u32), f32::from_bits(b as u32))
                    .unwrap()
                    .to_bits(),
            ),
            "f64" => random
                .uniform_f64(f64::from_bits(a), f64::from_bits(b))
                .unwrap()
                .to_bits(),
            "irand_default" => random.irand_default().unwrap() as u64,
            "urand_default" => u64::from(random.urand_default().unwrap()),
            "rand_norm" => random.rand_norm().unwrap().to_bits(),
            "rand_norm_f" => u64::from(random.rand_norm_f().unwrap().to_bits()),
            "rand_chance" => random.rand_chance().unwrap().to_bits(),
            "rand_chance_f" => u64::from(random.rand_chance_f().unwrap().to_bits()),
            "roll_chance_f" => u64::from(random.roll_chance_f(f32::from_bits(a as u32)).unwrap()),
            "roll_chance_i" => u64::from(random.roll_chance_i(a as i32).unwrap()),
            "combat" => u64::from(random.roll_chance_combat(f32::from_bits(a as u32)).unwrap()),
            "pick" => {
                let values: Vec<u64> = (0..a).collect();
                *random.pick_random_value(&values).unwrap()
            }
            other => panic!("unknown fixture operation {other}"),
        };
        assert_eq!(actual, number(row, "out"), "{}", row);
    }
    assert_eq!(random.cursor().raw, number(row, "end"), "{}", row);
}

#[test]
fn cpp_mixed_mt_vectors_match_outputs_cursors_and_complete_post_engine_states() {
    let mut engine = Mt19937::from_seed_seq(&[1, 2]);
    let mut cursor = DrawCursor::default();
    let mut count = 0;
    for row in rows().filter(|row| row["family"] == "mt") {
        execute(&mut engine, &mut cursor, &row);
        assert_eq!(mt_hash(&engine), row["state"].as_str().unwrap(), "{}", row);
        count += 1;
    }
    assert_eq!(count, 2403);
}

#[test]
fn cpp_full_width_64_bit_generator_exercises_lemire128_and_real_paths() {
    let mut engine = Script::<0, { u64::MAX }> {
        words: &WORDS64,
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut count = 0;
    for row in rows().filter(|row| row["family"] == "wide") {
        execute(&mut engine, &mut cursor, &row);
        assert_eq!(
            hash(&format!("script64 {}", engine.index)),
            row["state"].as_str().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 140);
}

#[test]
fn cpp_nonzero_minimum_fallback_upscaling_and_shuffle_paths_match() {
    let mut engine = Script::<10, 109> {
        words: &NARROW,
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut count = 0;
    for row in rows().filter(|row| row["family"] == "narrow") {
        execute(&mut engine, &mut cursor, &row);
        assert_eq!(
            hash(&format!("narrow {}", engine.index)),
            row["state"].as_str().unwrap()
        );
        count += 1;
    }
    assert_eq!(count, 60);
}

#[test]
fn cpp_double_to_float_vectors_preserve_ties_signed_zero_subnormals_and_nan_payload() {
    let mut count = 0;
    for row in rows().filter(|row| row["op"] == "cast") {
        assert_eq!(
            double_to_float_bits(number(&row, "a")),
            number(&row, "out") as u32
        );
        count += 1;
    }
    assert_eq!(count, 9);
}

#[test]
fn cpp_chance_u_vectors_preserve_roundf_nan_max_order_and_negative_clamp() {
    let mut count = 0;
    for row in rows().filter(|row| row["op"] == "chance_u") {
        assert_eq!(
            chance_u(f32::from_bits(number(&row, "a") as u32)),
            Ok(number(&row, "out") as u32)
        );
        count += 1;
    }
    assert_eq!(count, 11);
}

#[test]
fn canonical_endpoints_repair_one_without_changing_low_word_first_order() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[u32::MAX as u64],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    {
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(random.canonical_f32().unwrap().to_bits(), 0x3f7f_ffff);
        assert_eq!(
            random.canonical_f64().unwrap().to_bits(),
            0x3fef_ffff_ffff_ffff
        );
        assert_eq!(random.cursor().raw, 3);
    }
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[0, u32::MAX as u64],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        random.canonical_f64().unwrap().to_bits(),
        0x3fef_ffff_ffe0_0000
    );
    assert_eq!(random.cursor().raw, 2);
}

#[test]
fn affine_rounding_can_reach_the_upper_endpoint_and_equal_real_bounds_still_draw() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[u32::MAX as u64],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(
        random
            .uniform_f32(1.0, f32::from_bits(0x3f80_0001))
            .unwrap()
            .to_bits(),
        0x3f80_0001
    );
    assert_eq!(random.uniform_f32(-0.0, -0.0).unwrap().to_bits(), 0);
    assert_eq!(
        random.uniform_f64(0.5, 0.5).unwrap().to_bits(),
        0x3fe0_0000_0000_0000
    );
    assert_eq!(random.cursor().raw, 4);
}

#[test]
fn every_32_bit_rejection_and_nested_wrapper_span_has_exact_raw_positions() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[0, 0, u32::MAX as u64],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    {
        let mut sink = |event| events.push(event);
        let mut random = SourceRandom::new(77, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(random.urand(0, 0x8000_0000), Ok(0x8000_0000));
        assert_eq!(random.cursor().raw, 3);
    }
    assert!(events.iter().all(|event| event.stream == 77));
    assert_eq!(
        events[0].kind,
        EventKind::Begin {
            span: 0,
            parent: None,
            at: 0,
            operation: Operation::Urand,
            args: [0, 0x8000_0000, 0]
        }
    );
    assert_eq!(
        events[1].kind,
        EventKind::Begin {
            span: 1,
            parent: Some(0),
            at: 0,
            operation: Operation::UniformU32,
            args: [0, 0x8000_0000, 0]
        }
    );
    let raw: Vec<_> = events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Raw { position, word, .. } => Some((position, word)),
            _ => None,
        })
        .collect();
    assert_eq!(raw, [(0, 0), (1, 0), (2, u32::MAX as u64)]);
    let rejected: Vec<_> = events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Rejected {
                span,
                begin,
                end,
                reason,
            } => Some((span, begin, end, reason)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rejected,
        [
            (1, 0, 1, Rejection::Lemire32),
            (1, 1, 2, Rejection::Lemire32)
        ]
    );
    assert_eq!(
        events.last().unwrap().kind,
        EventKind::End {
            span: 0,
            begin: 0,
            end: 3,
            result: Ok(Value::U64(0x8000_0000))
        }
    );
}

#[test]
fn every_64_bit_lemire_rejection_is_retained() {
    let mut engine = Script::<0, { u64::MAX }> {
        words: &[0, 0, u64::MAX],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    {
        let mut sink = |event| events.push(event);
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(
            random.uniform_u64(0, 0x8000_0000_0000_0000),
            Ok(0x8000_0000_0000_0000)
        );
    }
    let rejected: Vec<_> = events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Rejected {
                begin, end, reason, ..
            } => Some((begin, end, reason)),
            _ => None,
        })
        .collect();
    assert_eq!(
        rejected,
        [(0, 1, Rejection::Lemire64), (1, 2, Rejection::Lemire64)]
    );
    assert_eq!(cursor.raw, 3);
}

#[test]
fn full_64_bit_range_upscaling_rejects_whole_out_of_range_candidate() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[u32::MAX as u64, u32::MAX as u64, 0, 0],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    {
        let mut sink = |event| events.push(event);
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(random.uniform_u64(0, u64::MAX - 16), Ok(0));
    }
    assert!(events.iter().any(|e| e.kind
        == EventKind::Rejected {
            span: 0,
            begin: 0,
            end: 2,
            reason: Rejection::Upscale
        }));
    assert_eq!(cursor.raw, 4);
}

#[test]
fn equal_integer_bounds_consume_words_and_signed_extremes_do_not_overflow() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[0, u32::MAX as u64],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(random.uniform_i32(-7, -7), Ok(-7));
    assert_eq!(random.uniform_u64(u64::MAX, u64::MAX), Ok(u64::MAX));
    assert_eq!(random.cursor().raw, 2);
    assert_eq!(random.uniform_i32(i32::MIN, i32::MAX), Ok(i32::MIN));
    assert_eq!(random.uniform_i32(i32::MIN, i32::MAX), Ok(i32::MAX));
    assert_eq!(
        random.uniform_i64(i64::MIN, i64::MAX),
        Ok(i64::MIN + 4_294_967_295)
    );
}

#[test]
fn combat_rounded_zero_skips_but_nonzero_strict_threshold_draws() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[u32::MAX as u64],
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    {
        let mut sink = |event| events.push(event);
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        for chance in [0.0, -1.0, f32::NAN, f32::NEG_INFINITY, 0.0049] {
            assert_eq!(random.roll_chance_combat(chance), Ok(false));
        }
        assert_eq!(random.cursor().raw, 0);
        assert_eq!(random.roll_chance_combat(0.005), Ok(false));
        assert_eq!(random.roll_chance_combat(100.0), Ok(false));
        assert_eq!(random.cursor().raw, 2);
    }
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::NoDraw {
                    reason: NoDraw::CombatRoundedZero,
                    ..
                }
            ))
            .count(),
        5
    );
}

#[test]
fn die_uses_supplied_values_inclusive_thresholds_and_wrapping_weights_without_draws() {
    let mut die = Die::<3>::new(9).unwrap();
    die.set(0, 20.0).unwrap();
    die.set(2, 30.0).unwrap();
    die.set(3, f32::INFINITY).unwrap();
    assert_eq!(die.weights(), &[2000, 0, 3000]);
    let mut engine = Mt19937::default();
    let before = engine.state().clone();
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    {
        let mut sink = |event| events.push(event);
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        for (supplied, expected) in [
            (0, 0),
            (2000, 0),
            (2001, 2),
            (5000, 2),
            (5001, 9),
            (u32::MAX, 9),
        ] {
            assert_eq!(random.die_roll(&die, supplied), Ok(expected));
        }
        let empty = Die::<0>::new(7).unwrap();
        assert_eq!(random.die_roll(&empty, 0), Ok(7));
        let mut overflowing = Die::<2>::new(7).unwrap();
        overflowing.set(0, 30_000_000.0).unwrap();
        overflowing.set(1, 30_000_000.0).unwrap();
        assert_eq!(random.die_roll(&overflowing, u32::MAX), Ok(7));
    }
    assert_eq!(cursor.raw, 0);
    assert_eq!(engine.state(), &before);
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(
                e.kind,
                EventKind::NoDraw {
                    reason: NoDraw::SuppliedDie,
                    ..
                }
            ))
            .count(),
        8
    );
    assert!(events.iter().any(|e| matches!(
        e.kind,
        EventKind::DieSide {
            side: 1,
            weight: 3_000_000_000,
            cumulative: 1_705_032_704,
            ..
        }
    )));
    assert_eq!(Die::<256>::new(0), Err(Error::InvalidDieSides));
}

#[test]
fn undefined_float_to_uint_chances_refuse_instead_of_saturating() {
    assert_eq!(
        chance_u(f32::INFINITY),
        Err(Error::UndefinedChanceConversion)
    );
    assert_eq!(chance_u(f32::MAX), Err(Error::UndefinedChanceConversion));
    assert_eq!(chance_u(f32::NEG_INFINITY), Ok(0));
    assert_eq!(chance_u(f32::NAN), Ok(0));
}

#[test]
fn invalid_ranges_and_unreached_non_power_of_two_real_generators_do_not_draw() {
    let mut engine = Script::<10, 109> {
        words: &NARROW,
        index: 0,
    };
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    {
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(random.canonical_f32(), Err(Error::UnreachedRealGenerator));
        assert_eq!(random.canonical_f64(), Err(Error::UnreachedRealGenerator));
        assert_eq!(random.uniform_u64(2, 1), Err(Error::InvalidRange));
        assert_eq!(
            random.uniform_i64(i64::MAX, i64::MIN),
            Err(Error::InvalidRange)
        );
        assert_eq!(
            random.uniform_f32(f32::NAN, 1.0),
            Err(Error::InvalidRealRange)
        );
        assert_eq!(
            random.uniform_f64(-f64::MAX, f64::MAX),
            Err(Error::InvalidRealRange)
        );
        assert_eq!(
            random.pick_random_value::<u8>(&[]),
            Err(Error::EmptySelection)
        );
    }
    assert_eq!(cursor.raw, 0);
    assert_eq!(engine.index, 0);
}

#[test]
fn cursor_overflow_refuses_before_consuming_an_engine_word() {
    let mut engine = Script::<0, { u32::MAX as u64 }> {
        words: &[1],
        index: 0,
    };
    let mut cursor = DrawCursor {
        raw: u64::MAX,
        next_span: 0,
    };
    let mut sink = |_| {};
    {
        let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
        assert_eq!(random.next_raw(), Err(Error::CursorOverflow));
    }
    assert_eq!(engine.index, 0);
    let mut cursor = DrawCursor {
        raw: 0,
        next_span: u64::MAX,
    };
    let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
    assert_eq!(random.irand(0, 99), Err(Error::CursorOverflow));
    assert_eq!(random.cursor().raw, 0);
}

#[test]
fn continuing_contexts_keep_raw_cursors_unique_spans_and_restored_root_parent() {
    let mut engine = Mt19937::default();
    let mut cursor = DrawCursor::default();
    let mut events = Vec::new();
    for _ in 0..2 {
        let mut sink = |event| events.push(event);
        let mut random = SourceRandom::new(5, &mut engine, &mut cursor, &mut sink).unwrap();
        random.urand(7, 7).unwrap();
    }
    let roots: Vec<_> = events
        .iter()
        .filter_map(|e| match e.kind {
            EventKind::Begin {
                span,
                parent: None,
                at,
                ..
            } => Some((span, at)),
            _ => None,
        })
        .collect();
    assert_eq!(roots, [(0, 0), (2, 1)]);
    assert_eq!(
        cursor,
        DrawCursor {
            raw: 2,
            next_span: 4
        }
    );
}

#[test]
fn shuffle_trivial_and_two_at_a_time_paths_preserve_exact_draw_counts_and_swaps() {
    for (count, draws) in [
        (0_usize, 0_u64),
        (1, 0),
        (2, 1),
        (3, 1),
        (4, 2),
        (5, 2),
        (10, 5),
        (11, 5),
    ] {
        let mut engine = Mt19937::default();
        let mut cursor = DrawCursor::default();
        let mut events = Vec::new();
        let mut values: Vec<_> = (0..count).collect();
        {
            let mut sink = |event| events.push(event);
            let mut random = SourceRandom::new(1, &mut engine, &mut cursor, &mut sink).unwrap();
            random.shuffle(&mut values).unwrap();
        }
        assert_eq!(cursor.raw, draws);
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(e.kind, EventKind::Swap { .. }))
                .count(),
            count.saturating_sub(1)
        );
        assert_eq!(
            events
                .iter()
                .filter(|e| matches!(
                    e.kind,
                    EventKind::NoDraw {
                        reason: NoDraw::ShuffleTrivial,
                        ..
                    }
                ))
                .count(),
            usize::from(count < 2)
        );
        let mut reconstructed: Vec<_> = (0..count).collect();
        for event in events {
            if let EventKind::Swap { left, right, .. } = event.kind {
                reconstructed.swap(left as usize, right as usize);
            }
        }
        assert_eq!(reconstructed, values);
    }
}
