// SPDX-License-Identifier: GPL-3.0-or-later
//! Read-only engine/wrapper projection of the original, explicitly unsealed F1 prefix.
use super::*;
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round,
};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs::File,
    io::{BufRead, BufReader},
    path::PathBuf,
};

#[derive(Clone, Debug)]
struct Call {
    name: String,
    a: u64,
    b: u64,
    at: u64,
    address: Option<u64>,
    words: Vec<(u64, u64)>,
}
struct Stream {
    engine: Mt19937,
    cursor: DrawCursor,
}

fn double_bits(text: &str) -> u64 {
    Double::from_str_r(text, Round::NearestTiesToEven)
        .unwrap()
        .value
        .to_bits() as u64
}
fn single_bits(text: &str) -> u32 {
    let single: Single = Double::from_bits(u128::from(double_bits(text)))
        .convert_r(Round::NearestTiesToEven, &mut false)
        .value;
    single.to_bits() as u32
}
fn parse_call(data: &str, at: u64) -> Call {
    let fields: Vec<_> = data.split_whitespace().collect();
    let name = fields[0];
    let (a, b) = match name {
        "irand" => {
            assert_eq!(fields.len(), 5);
            (
                fields[3].parse::<i32>().unwrap() as u64,
                fields[4].parse::<i32>().unwrap() as u64,
            )
        }
        "urand" => {
            assert_eq!(fields.len(), 5);
            (
                u64::from(fields[3].parse::<u32>().unwrap()),
                u64::from(fields[4].parse::<u32>().unwrap()),
            )
        }
        "frand" => {
            assert_eq!(fields.len(), 5);
            (
                u64::from(single_bits(fields[3])),
                u64::from(single_bits(fields[4])),
            )
        }
        "irand_default" | "urand_default" | "rand_norm" | "rand_norm_f" | "rand_chance"
        | "rand_chance_f" => {
            assert_eq!(fields.len(), 3);
            (0, 0)
        }
        other => panic!("unsupported semantic owner {other} at row {at}"),
    };
    Call {
        name: name.to_owned(),
        a,
        b,
        at,
        address: None,
        words: Vec::new(),
    }
}
fn expected(call: &Call, data: &str) -> Value {
    let mut fields = data.split_whitespace();
    assert_eq!(fields.next(), Some(call.name.as_str()));
    let value = fields.next().unwrap();
    assert_eq!(fields.next(), None);
    match call.name.as_str() {
        "irand" | "irand_default" => Value::I64(i64::from(value.parse::<i32>().unwrap())),
        "urand" | "urand_default" => Value::U64(u64::from(value.parse::<u32>().unwrap())),
        "frand" | "rand_norm_f" | "rand_chance_f" => Value::F32(single_bits(value)),
        "rand_norm" | "rand_chance" => Value::F64(double_bits(value)),
        _ => unreachable!(),
    }
}
fn invoke<S: FnMut(TraceEvent)>(call: &Call, random: &mut SourceRandom<'_, Mt19937, S>) -> Value {
    match call.name.as_str() {
        "irand" => Value::I64(i64::from(
            random.irand(call.a as i32, call.b as i32).unwrap(),
        )),
        "urand" => Value::U64(u64::from(
            random.urand(call.a as u32, call.b as u32).unwrap(),
        )),
        "frand" => Value::F32(
            random
                .frand(f32::from_bits(call.a as u32), f32::from_bits(call.b as u32))
                .unwrap()
                .to_bits(),
        ),
        "irand_default" => Value::I64(i64::from(random.irand_default().unwrap())),
        "urand_default" => Value::U64(u64::from(random.urand_default().unwrap())),
        "rand_norm" => Value::F64(random.rand_norm().unwrap().to_bits()),
        "rand_norm_f" => Value::F32(random.rand_norm_f().unwrap().to_bits()),
        "rand_chance" => Value::F64(random.rand_chance().unwrap().to_bits()),
        "rand_chance_f" => Value::F32(random.rand_chance_f().unwrap().to_bits()),
        _ => unreachable!(),
    }
}

#[test]
fn private_f1_original_prefix_recomputes_recorded_semantic_draws_and_raw_spans() {
    let explicit = std::env::var_os("BENILLA_F1_RNG_TRACE");
    let path = explicit.clone().map(PathBuf::from).unwrap_or_else(|| {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../benilla-config/full-server/f1/run/startup-through-third-map0.tsv")
    });
    let input = match File::open(&path) {
        Ok(input) => input,
        Err(error) if explicit.is_none() && error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP private F1 semantic projection: set BENILLA_F1_RNG_TRACE to the original startup-through-third-map0.tsv prefix");
            return;
        }
        Err(error) => panic!("cannot read {}: {error}", path.display()),
    };
    let mut input = BufReader::new(input);
    let mut line = String::new();
    let mut digest = Sha256::new();
    let mut streams = BTreeMap::<(u64, u64), Stream>::new();
    let mut pending = BTreeMap::<u64, Call>::new();
    let mut counts = BTreeMap::<String, u64>::new();
    let mut row = 0_u64;
    let mut raw = 0_u64;
    let mut semantic_raw = 0_u64;
    let mut states = 0;
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
        if !matches!(kind, "mt_state" | "mt_raw" | "rng_call" | "rng_result") {
            continue;
        }
        let tid: u64 = tid.parse().unwrap();
        match kind {
            "mt_state" => {
                let mut data = data.split_whitespace();
                let address =
                    u64::from_str_radix(data.next().unwrap().strip_prefix("0x").unwrap(), 16)
                        .unwrap();
                let mut words = [0; MT_STATE_WORDS];
                for word in &mut words {
                    *word = data.next().unwrap().parse().unwrap();
                }
                let index = data.next().unwrap().parse().unwrap();
                assert_eq!(data.next(), None);
                let stream = Stream {
                    engine: Mt19937::from_state(MtState { words, index }).unwrap(),
                    cursor: DrawCursor::default(),
                };
                assert!(
                    streams.insert((tid, address), stream).is_none(),
                    "duplicate initial state at row {row}"
                );
                states += 1;
            }
            "rng_call" => {
                assert!(
                    pending.insert(tid, parse_call(data, row)).is_none(),
                    "nested original-prefix wrapper at row {row}"
                );
            }
            "mt_raw" => {
                let mut data = data.split_whitespace();
                let address =
                    u64::from_str_radix(data.next().unwrap().strip_prefix("0x").unwrap(), 16)
                        .unwrap();
                let position: u64 = data.next().unwrap().parse().unwrap();
                let word: u64 = data.next().unwrap().parse::<u32>().map(u64::from).unwrap();
                assert_eq!(data.next(), None);
                let stream = streams
                    .get_mut(&(tid, address))
                    .expect("raw word before full state");
                if let Some(call) = pending.get_mut(&tid) {
                    if let Some(prior) = call.address {
                        assert_eq!(prior, address, "wrapper crossed engines");
                    }
                    call.address = Some(address);
                    assert_eq!(
                        position,
                        stream.cursor.raw + call.words.len() as u64,
                        "raw position at row {row}"
                    );
                    call.words.push((position, word));
                    semantic_raw += 1;
                } else {
                    assert_eq!(
                        stream.cursor.raw, position,
                        "direct raw position at row {row}"
                    );
                    let mut sink = |_| {};
                    let mut random =
                        SourceRandom::new(tid, &mut stream.engine, &mut stream.cursor, &mut sink)
                            .unwrap();
                    assert_eq!(
                        random.next_raw(),
                        Ok(word),
                        "direct raw mismatch at row {row}"
                    );
                }
                raw += 1;
            }
            "rng_result" => {
                let call = pending.remove(&tid).expect("semantic result without call");
                let stream = streams
                    .get_mut(&(tid, call.address.expect("recorded wrapper has no raw span")))
                    .unwrap();
                let mut matched = 0;
                let result = {
                    let mut sink = |event: TraceEvent| {
                        if let EventKind::Raw { position, word, .. } = event.kind {
                            assert_eq!(
                                call.words.get(matched),
                                Some(&(position, word)),
                                "raw mismatch for {} at row {}",
                                call.name,
                                call.at
                            );
                            matched += 1;
                        }
                    };
                    let mut random =
                        SourceRandom::new(tid, &mut stream.engine, &mut stream.cursor, &mut sink)
                            .unwrap();
                    invoke(&call, &mut random)
                };
                assert_eq!(
                    matched,
                    call.words.len(),
                    "missing raw words for {} at row {}",
                    call.name,
                    call.at
                );
                assert_eq!(
                    result,
                    expected(&call, data),
                    "semantic mismatch for {} at row {}",
                    call.name,
                    call.at
                );
                *counts.entry(call.name).or_default() += 1;
            }
            _ => unreachable!(),
        }
    }
    assert_eq!(
        format!("{:x}", digest.finalize()),
        "574813534352216ffbc4f7ca884db032bb3cfa08e292489438d60e1d9a8a836b"
    );
    assert_eq!(row, 15_371_848);
    assert!(pending.is_empty());
    assert_eq!(states, 8);
    assert_eq!(streams.len(), 8);
    assert_eq!(raw, 69_586);
    assert_eq!(streams.values().map(|s| s.cursor.raw).sum::<u64>(), raw);
    assert_eq!(counts.values().sum::<u64>(), 27_865);
    println!("Recomputed 27865 recorded semantic draws and all 69586 raw MT words; {semantic_raw} raw words belong to recorded wrapper spans: {counts:?}");
    println!("NON-ADMITTING: original unsealed prefix, wrapper/engine projection only; direct shuffles and gameplay chronology are not proved");
}
