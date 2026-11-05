// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::cell::Cell;

const SEEDS: [&[u32]; 4] = [&[], &[1], &[1, 2, 3, 4], &[u32::MAX, 0, 0x8000_0000, 42]];
const FIXTURE: &str = include_str!("../../tests/fixtures/source_rng_gcc12.txt");

fn rows(kind: &str) -> impl Iterator<Item = Vec<i64>> + '_ {
    FIXTURE.lines().filter_map(move |line| {
        let mut fields = line.split_whitespace();
        (fields.next() == Some(kind)).then(|| fields.map(|v| v.parse().unwrap()).collect())
    })
}

fn mt_state(values: &[i64]) -> MtState {
    assert_eq!(values.len(), 625);
    let mut words = [0; MT_STATE_WORDS];
    for (word, &value) in words.iter_mut().zip(values) {
        *word = u32::try_from(value).unwrap();
    }
    MtState {
        words,
        index: u64::try_from(values[624]).unwrap(),
    }
}

#[test]
fn seed_seq_vectors_cover_empty_short_and_every_mixing_threshold() {
    let mut cases = 0;
    for row in rows("seq") {
        let n = row[1] as usize;
        assert_eq!(row.len(), n + 2);
        let mut actual = vec![0xdead_beef; n];
        seed_seq_generate(SEEDS[row[0] as usize], &mut actual);
        let expected: Vec<u32> = row[2..]
            .iter()
            .map(|&v| u32::try_from(v).unwrap())
            .collect();
        assert_eq!(actual, expected, "seed set {}, length {n}", row[0]);
        cases += 1;
    }
    assert_eq!(cases, 48);
}

#[test]
fn scalar_seed_recurrence_matches_every_cpp_initial_word() {
    let mut cases = 0;
    for row in rows("initial") {
        let actual = Mt19937::from_seed(row[0] as u64);
        assert_eq!(actual.state(), &mt_state(&row[1..]));
        cases += 1;
    }
    assert_eq!(cases, 4);
    assert_eq!(Mt19937::from_seed(4_294_967_297), Mt19937::from_seed(1));
    assert_eq!(
        Mt19937::from_seed(u64::MAX),
        Mt19937::from_seed(u64::from(u32::MAX))
    );
}

#[test]
fn seed_seq_mt_initial_state_matches_every_cpp_word_and_position() {
    let mut cases = 0;
    for row in rows("seeded") {
        assert_eq!(
            Mt19937::from_seed_seq(SEEDS[row[0] as usize]).state(),
            &mt_state(&row[1..])
        );
        cases += 1;
    }
    assert_eq!(cases, 4);
}

#[test]
fn mt_raw_vectors_cross_first_and_second_twist_boundaries() {
    let mut cases = 0;
    for kind in ["mt", "seqmt"] {
        let mut engine = Mt19937::default();
        let mut prior = None;
        let mut next = 0;
        for row in rows(kind) {
            if prior != Some(row[0]) {
                engine = if kind == "mt" {
                    Mt19937::from_seed(row[0] as u64)
                } else {
                    Mt19937::from_seed_seq(SEEDS[row[0] as usize])
                };
                prior = Some(row[0]);
                next = 0;
            }
            let target = row[1] as usize;
            while next < target {
                engine.next_u32();
                next += 1;
            }
            assert_eq!(
                engine.next_u32(),
                row[2] as u32,
                "{kind} {} word {target}",
                row[0]
            );
            next += 1;
            cases += 1;
        }
    }
    assert_eq!(cases, 120);
}

#[test]
fn default_mt_10000th_output_is_the_cpp_standard_known_answer() {
    let mut engine = Mt19937::default();
    for _ in 0..9999 {
        engine.next_u32();
    }
    assert_eq!(engine.next_u32(), 4_123_659_995);
}

#[test]
fn complete_mt_state_round_trip_preserves_the_next_two_thousand_words() {
    for count in [0, 1, 623, 624, 625, 1248, 10000] {
        let mut original = Mt19937::from_seed_seq(&[1, 2, 3, 4]);
        for _ in 0..count {
            original.next_u32();
        }
        let state = original.state().clone();
        let mut restored = Mt19937::from_state(state.clone()).unwrap();
        assert_eq!(restored.state(), &state);
        for _ in 0..2000 {
            assert_eq!(original.next_u32(), restored.next_u32());
        }
        assert_eq!(original.into_state(), restored.into_state());
    }
}

#[test]
fn invalid_mt_position_refuses_and_restored_zero_state_is_not_reseeded() {
    let zero = MtState {
        words: [0; MT_STATE_WORDS],
        index: 624,
    };
    let mut engine = Mt19937::from_state(zero).unwrap();
    assert_eq!(engine.next_u32(), 0);
    assert_eq!(engine.state().index, 1);
    assert_eq!(
        Mt19937::from_state(MtState {
            words: [0; MT_STATE_WORDS],
            index: 625
        }),
        Err(Error::InvalidMtIndex)
    );
}

#[test]
fn seed_observations_preserve_lp64_size_t_then_uint32_narrowing() {
    assert_eq!(
        SeedObservations {
            unix_seconds: -1,
            process_clock: 4_294_967_297
        }
        .words(),
        [u32::MAX, 1]
    );
    assert_eq!(
        SeedObservations {
            unix_seconds: i64::MIN,
            process_clock: i64::MAX
        }
        .words(),
        [0, u32::MAX]
    );
}

fn observed_seed() -> SeedObservations {
    SeedObservations {
        unix_seconds: 1_793_805_039,
        process_clock: 123_456,
    }
}

#[test]
fn thread_engine_is_lazy_and_later_gets_keep_the_same_stream() {
    let mut thread = ThreadRng::new(7);
    assert_eq!(thread.thread_birth(), 7);
    assert!(!thread.initialized());
    let calls = Cell::new(0);
    let mut expected = Mt19937::from_seed_seq(&observed_seed().words());
    for _ in 0..1250 {
        let engine = thread.get_or_init(|| {
            calls.set(calls.get() + 1);
            observed_seed()
        });
        assert_eq!(engine.next_u32(), expected.next_u32());
    }
    assert_eq!(calls.get(), 1);
    assert!(thread.initialized());
    assert_eq!(thread.into_state(), Some(expected.into_state()));
    assert_eq!(ThreadRng::new(8).into_state(), None);
}

#[test]
fn interleaved_logical_threads_have_independent_engines() {
    let mut a = ThreadRng::new(1);
    let mut b = ThreadRng::new(2);
    let mut expected_a = Mt19937::from_seed_seq(&observed_seed().words());
    let other = SeedObservations {
        process_clock: 99,
        ..observed_seed()
    };
    let mut expected_b = Mt19937::from_seed_seq(&other.words());
    for _ in 0..1000 {
        assert_eq!(b.get_or_init(|| other).next_u32(), expected_b.next_u32());
        for _ in 0..3 {
            assert_eq!(
                a.get_or_init(observed_seed).next_u32(),
                expected_a.next_u32()
            );
        }
    }
}

#[test]
fn destroyed_thread_rebirth_does_not_inherit_a_reused_os_ids_engine() {
    let os_id = 42;
    let first = {
        let mut binding = (os_id, ThreadRng::new(100));
        let first = binding.1.get_or_init(observed_seed).next_u32();
        let second = binding
            .1
            .get_or_init(|| panic!("must not reseed"))
            .next_u32();
        assert_ne!(first, second);
        first
    };
    let mut binding = (os_id, ThreadRng::new(101));
    assert_eq!(binding.0, 42);
    assert_eq!(binding.1.thread_birth(), 101);
    assert!(!binding.1.initialized());
    assert_eq!(binding.1.get_or_init(observed_seed).next_u32(), first);
}

#[test]
fn thread_round_trip_preserves_initialized_and_uninitialized_lifetimes() {
    let mut absent = ThreadRng::from_state(9, None).unwrap();
    assert!(!absent.initialized());
    absent.get_or_init(observed_seed).next_u32();
    let state = absent.into_state();
    let mut restored = ThreadRng::from_state(9, state.clone()).unwrap();
    assert_eq!(restored.thread_birth(), 9);
    assert!(restored.initialized());
    let mut expected = Mt19937::from_state(state.unwrap()).unwrap();
    assert_eq!(
        restored
            .get_or_init(|| panic!("restoration must not reseed"))
            .next_u32(),
        expected.next_u32()
    );
}

const C_FIRST_TEN: [i32; 10] = [
    1804289383, 846930886, 1681692777, 1714636915, 1957747793, 424238335, 719885386, 1649760492,
    596516649, 1189641421,
];

#[test]
fn glibc_type3_srand_one_has_the_known_first_ten_values() {
    let mut engine = ProcessCRand::from_seed(1);
    for expected in C_FIRST_TEN {
        assert_eq!(engine.next_i32(), expected);
    }
}

#[test]
fn c_default_before_srand_and_repeated_zero_one_resets_match_source() {
    let mut engine = ProcessCRand::default();
    assert_eq!(engine, ProcessCRand::from_seed(1));
    assert_eq!(engine, ProcessCRand::from_seed(0));
    for seed in [1, 0, 1] {
        for expected in C_FIRST_TEN {
            assert_eq!(engine.next_i32(), expected);
        }
        engine.srand(seed);
    }
    assert_eq!(engine, ProcessCRand::default());
}

#[test]
fn c_stream_is_shared_by_interleaved_callers_not_owned_by_threads() {
    let mut process = ProcessCRand::default();
    let mut by_caller = [Vec::new(), Vec::new()];
    for (index, expected) in C_FIRST_TEN.into_iter().enumerate() {
        let value = process.next_i32();
        assert_eq!(value, expected);
        by_caller[index % 2].push(value);
    }
    assert_eq!(
        by_caller[0],
        [1804289383, 1681692777, 1957747793, 719885386, 596516649]
    );
    assert_eq!(
        by_caller[1],
        [846930886, 1714636915, 424238335, 1649760492, 1189641421]
    );
}

#[test]
fn c_full_state_round_trip_preserves_ring_wrap_and_high_bit_seeds() {
    for seed in [0, 1, 0x8000_0000, u32::MAX] {
        let mut engine = ProcessCRand::from_seed(seed);
        for _ in 0..97 {
            engine.next_i32();
        }
        let state = engine.state().clone();
        assert_eq!(state.rear, 4);
        let mut restored = ProcessCRand::from_state(state.clone()).unwrap();
        assert_eq!(restored.state(), &state);
        for _ in 0..1000 {
            let word = engine.next_i32();
            assert!((0..=i32::MAX).contains(&word));
            assert_eq!(word, restored.next_i32());
        }
        assert_eq!(engine.into_state(), restored.into_state());
    }
    assert_eq!(
        ProcessCRand::from_state(CRandState {
            words: [0; 31],
            rear: 31
        }),
        Err(Error::InvalidCIndex)
    );
}

#[test]
fn c_high_bit_seed_expansion_uses_signed_int32_before_lp64_division() {
    for (seed, expected) in [
        (
            0x8000_0000,
            [1336741213, 1210407648, 1447044896, 337392383, 82502902],
        ),
        (
            u32::MAX,
            [254925627, 1205188300, 366127624, 1401405153, 76053476],
        ),
    ] {
        let mut engine = ProcessCRand::from_seed(seed);
        for word in expected {
            assert_eq!(engine.next_i32(), word);
        }
    }
}

#[test]
fn ephemeral_wse_vectors_use_seed_one_for_zero_and_do_not_advance_shared_engines() {
    let mut cases = 0;
    let mut thread = ThreadRng::new(10);
    let process = ProcessCRand::default();
    let before = process.state().clone();
    for row in rows("wse") {
        assert_eq!(
            wse_mersenne_random(row[0] as u32, row[1] as u32),
            Ok(row[2] as i32)
        );
        cases += 1;
    }
    assert_eq!(cases, 20);
    assert!(!thread.initialized());
    assert_eq!(process.state(), &before);
    assert_eq!(wse_mersenne_random(100, 0), Ok(46));
    assert_eq!(wse_mersenne_random(100, 0), Ok(46));
    assert_eq!(wse_mersenne_random(1, u32::MAX), Ok(1));
    assert_eq!(wse_mersenne_random(0, 1), Err(Error::ZeroModulus));
    assert_eq!(thread.get_or_init(observed_seed).state().index, 624);
}

#[test]
fn ordinary_wse_random_uses_calling_mt_and_matches_cpp_outputs_and_full_state() {
    let mut cases = 0;
    for state in rows("random_state") {
        let arg1 = state[0] as u32;
        let arg2 = state[1] as u32;
        let mut thread =
            ThreadRng::from_state(1, Some(Mt19937::from_seed(1).into_state())).unwrap();
        let mut cursor = DrawCursor::default();
        let mut sink = |_| {};
        for row in rows("random").filter(|row| row[0] == state[0] && row[1] == state[1]) {
            assert_eq!(
                wse_random(
                    &mut thread,
                    arg2,
                    arg1,
                    || panic!("must not reseed"),
                    &mut cursor,
                    &mut sink
                ),
                Ok(row[3] as i32)
            );
            cases += 1;
        }
        assert_eq!(thread.into_state(), Some(mt_state(&state[2..])));
    }
    assert_eq!(cases, 50);
}

#[test]
fn ordinary_wse_invalid_signed_bounds_refuse_before_lazy_initialization() {
    let mut thread = ThreadRng::new(1);
    let mut cursor = DrawCursor::default();
    let mut sink = |_| {};
    assert_eq!(
        wse_random(
            &mut thread,
            0,
            u32::MAX,
            || panic!("must refuse before seeding"),
            &mut cursor,
            &mut sink
        ),
        Err(Error::InvalidWseBounds)
    );
    assert!(!thread.initialized());
    assert_eq!(cursor, DrawCursor::default());
}

#[test]
fn private_f1_original_prefix_recomputes_all_captured_mt_words() {
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        fs::File,
        io::{BufRead, BufReader},
        path::PathBuf,
    };
    let explicit = std::env::var_os("BENILLA_F1_RNG_TRACE");
    let path = explicit.clone().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benilla-config/full-server/f1/run/startup-through-third-map0.tsv")
    });
    let input = match File::open(&path) {
        Ok(input) => input,
        Err(error) if explicit.is_none() && error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP private F1 RNG projection: set BENILLA_F1_RNG_TRACE to the original startup-through-third-map0.tsv prefix");
            return;
        }
        Err(error) => panic!("cannot read {}: {error}", path.display()),
    };
    let mut input = BufReader::new(input);
    let mut line = String::new();
    let mut digest = Sha256::new();
    let mut streams = BTreeMap::<(u64, u64), (Mt19937, u64)>::new();
    let mut by_thread = BTreeMap::<u64, u64>::new();
    let mut states = 0;
    let mut row = 0;
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap() == 0 {
            break;
        }
        digest.update(line.as_bytes());
        row += 1;
        let mut fields = line.splitn(4, '\t');
        let tid = fields.next().unwrap();
        let _sequence = fields.next().expect("missing sequence");
        let kind = fields.next().expect("missing kind");
        let data = fields.next().expect("missing payload");
        if kind != "mt_state" && kind != "mt_raw" {
            continue;
        }
        let tid: u64 = tid.parse().unwrap();
        let mut data = data.split_whitespace();
        let address = data.next().unwrap().strip_prefix("0x").unwrap();
        let address = u64::from_str_radix(address, 16).unwrap();
        let key = (tid, address);
        if kind == "mt_state" {
            let values: Vec<i64> = data.map(|v| v.parse().unwrap()).collect();
            let engine = Mt19937::from_state(mt_state(&values)).unwrap();
            assert!(
                streams.insert(key, (engine, 0)).is_none(),
                "duplicate state at row {row}"
            );
            states += 1;
        } else {
            let position: u64 = data.next().unwrap().parse().unwrap();
            let expected: u32 = data.next().unwrap().parse().unwrap();
            assert_eq!(data.next(), None);
            let (engine, next) = streams
                .get_mut(&key)
                .expect("raw word before initial state");
            assert_eq!(position, *next, "stream discontinuity at row {row}");
            assert_eq!(
                engine.next_u32(),
                expected,
                "raw MT mismatch at row {row}, thread {tid}, word {position}"
            );
            *next += 1;
            *by_thread.entry(tid).or_default() += 1;
        }
    }
    assert_eq!(
        format!("{:x}", digest.finalize()),
        "574813534352216ffbc4f7ca884db032bb3cfa08e292489438d60e1d9a8a836b"
    );
    assert_eq!(row, 15_371_848);
    assert_eq!(states, 8);
    assert_eq!(streams.len(), 8);
    assert_eq!(
        by_thread,
        BTreeMap::from([
            (1, 8759),
            (11, 35791),
            (12, 15775),
            (13, 1401),
            (14, 2147),
            (15, 381),
            (17, 5331),
            (21, 1)
        ])
    );
    assert_eq!(by_thread.values().sum::<u64>(), 69_586);
    println!("Recomputed 69586 exact raw MT words from 8 captured states in the hash-bound original F1 prefix: {by_thread:?}");
    println!("NON-ADMITTING: engine projection only, not clocks, scheduling, SQL, traversal or gameplay replay");
}
