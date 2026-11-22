// SPDX-License-Identifier: GPL-3.0-or-later
//! Versioned diagnostic adapter for the original TSV, never an F0 event envelope.
use super::*;
use benilla_sim::source_rng::{
    DrawCursor, EventKind as RngEvent, Mt19937, MtState, SourceRandom, TraceEvent, Value,
};
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round,
};
use std::collections::BTreeMap;

const ORIGINAL_HASH: &str = "574813534352216ffbc4f7ca884db032bb3cfa08e292489438d60e1d9a8a836b";
#[derive(Debug, Serialize)]
pub struct OriginalRngReport {
    pub adapter: &'static str,
    pub projection: Projection,
    pub trace_sha256: Sha256Id,
    pub records: u64,
    pub initial_states: u64,
    pub raw_words: u64,
    pub semantic_draws: u64,
    pub semantic_families: BTreeMap<String, u64>,
    pub pending: Vec<PendingF1Obligation>,
    pub whole_execution_accepted: bool,
}
struct Stream {
    engine: Mt19937,
    cursor: DrawCursor,
}
struct Call {
    name: String,
    a: u64,
    b: u64,
    engine: Option<u64>,
    words: Vec<(u64, u64)>,
}
fn number<T: std::str::FromStr>(value: Option<&str>) -> Result<T> {
    value
        .ok_or_else(|| refuse("execution.original_rng_fields"))?
        .parse()
        .map_err(|_| refuse("execution.original_rng_number"))
}
fn address(value: Option<&str>) -> Result<u64> {
    u64::from_str_radix(
        value
            .and_then(|s| s.strip_prefix("0x"))
            .ok_or_else(|| refuse("execution.original_rng_address"))?,
        16,
    )
    .map_err(|_| refuse("execution.original_rng_address"))
}
fn double(value: Option<&str>) -> Result<u64> {
    let text = value.ok_or_else(|| refuse("execution.original_rng_fields"))?;
    Ok(Double::from_str_r(text, Round::NearestTiesToEven)
        .map_err(|_| refuse("execution.original_rng_float"))?
        .value
        .to_bits() as u64)
}
fn single(value: Option<&str>) -> Result<u32> {
    let result: Single = Double::from_bits(u128::from(double(value)?))
        .convert_r(Round::NearestTiesToEven, &mut false)
        .value;
    Ok(result.to_bits() as u32)
}
fn call(data: &str) -> Result<Call> {
    let mut fields = data.split_whitespace();
    let name = fields
        .next()
        .ok_or_else(|| refuse("execution.original_rng_call"))?;
    fields
        .next()
        .ok_or_else(|| refuse("execution.original_rng_call"))?;
    fields
        .next()
        .ok_or_else(|| refuse("execution.original_rng_call"))?;
    let (a, b) = match name {
        "irand" => (
            number::<i32>(fields.next())? as u64,
            number::<i32>(fields.next())? as u64,
        ),
        "urand" => (
            u64::from(number::<u32>(fields.next())?),
            u64::from(number::<u32>(fields.next())?),
        ),
        "frand" => (
            u64::from(single(fields.next())?),
            u64::from(single(fields.next())?),
        ),
        "irand_default" | "urand_default" | "rand_norm" | "rand_norm_f" | "rand_chance"
        | "rand_chance_f" => (0, 0),
        _ => return Err(refuse("execution.original_rng_unsupported_owner")),
    };
    require(fields.next().is_none(), "execution.original_rng_fields")?;
    Ok(Call {
        name: name.into(),
        a,
        b,
        engine: None,
        words: Vec::new(),
    })
}
fn expected(call: &Call, data: &str) -> Result<Value> {
    let mut fields = data.split_whitespace();
    require(
        fields.next() == Some(call.name.as_str()),
        "execution.original_rng_result_owner",
    )?;
    let value = match call.name.as_str() {
        "irand" | "irand_default" => Value::I64(i64::from(number::<i32>(fields.next())?)),
        "urand" | "urand_default" => Value::U64(u64::from(number::<u32>(fields.next())?)),
        "frand" | "rand_norm_f" | "rand_chance_f" => Value::F32(single(fields.next())?),
        "rand_norm" | "rand_chance" => Value::F64(double(fields.next())?),
        _ => return Err(refuse("execution.original_rng_unsupported_owner")),
    };
    require(fields.next().is_none(), "execution.original_rng_fields")?;
    Ok(value)
}
fn invoke<S: FnMut(TraceEvent)>(
    call: &Call,
    random: &mut SourceRandom<'_, Mt19937, S>,
) -> Result<Value> {
    let result = match call.name.as_str() {
        "irand" => random
            .irand(call.a as i32, call.b as i32)
            .map(|v| Value::I64(i64::from(v))),
        "urand" => random
            .urand(call.a as u32, call.b as u32)
            .map(|v| Value::U64(u64::from(v))),
        "frand" => random
            .frand(f32::from_bits(call.a as u32), f32::from_bits(call.b as u32))
            .map(|v| Value::F32(v.to_bits())),
        "irand_default" => random.irand_default().map(|v| Value::I64(i64::from(v))),
        "urand_default" => random.urand_default().map(|v| Value::U64(u64::from(v))),
        "rand_norm" => random.rand_norm().map(|v| Value::F64(v.to_bits())),
        "rand_norm_f" => random.rand_norm_f().map(|v| Value::F32(v.to_bits())),
        "rand_chance" => random.rand_chance().map(|v| Value::F64(v.to_bits())),
        "rand_chance_f" => random.rand_chance_f().map(|v| Value::F32(v.to_bits())),
        _ => return Err(refuse("execution.original_rng_unsupported_owner")),
    };
    result.map_err(|_| refuse("execution.original_rng_distribution"))
}

/// Recomputes helper calls from their arguments and initial MT states, not expected words.
/// Private OS-thread/address keys stay confined to this unsealed adapter.
pub fn compare_original_unsealed_rng(path: &Path) -> Result<OriginalRngReport> {
    let file = ExecutionFile::open(path, 1_073_741_824)?;
    let mut buffer = vec![0; 1_048_576];
    let mut line = Vec::with_capacity(1024);
    let mut hash = Sha256::new();
    let mut at = 0;
    let mut records = 0;
    let mut raw_words = 0;
    let mut counts = BTreeMap::<String, u64>::new();
    let mut streams = BTreeMap::<(u64, u64), Stream>::new();
    let mut calls = BTreeMap::<u64, Call>::new();
    let mut sequences = BTreeMap::<u64, u64>::new();
    while at < file.len() {
        let size = (file.len() - at).min(buffer.len() as u64) as usize;
        file.read_into(at, &mut buffer[..size])?;
        hash.update(&buffer[..size]);
        let mut start = 0;
        while start < size {
            let end = buffer[start..size]
                .iter()
                .position(|b| *b == b'\n')
                .map(|p| start + p);
            let stop = end.unwrap_or(size);
            require(
                line.len() + stop - start <= 65_536,
                "execution.original_rng_line_limit",
            )?;
            line.extend_from_slice(&buffer[start..stop]);
            start = stop + usize::from(end.is_some());
            if end.is_none() {
                break;
            }
            records += 1;
            let result = project_line(
                &line,
                &mut streams,
                &mut calls,
                &mut sequences,
                &mut raw_words,
                &mut counts,
            );
            if let Err(mut reason) = result {
                reason.id.push_str(&format!(".record_{records}"));
                return Err(reason);
            }
            line.clear();
        }
        at += size as u64;
    }
    file.check()?;
    require(
        line.is_empty() && calls.is_empty(),
        "execution.original_rng_truncated",
    )?;
    let trace_sha256 = Sha256Id(format!("{:x}", hash.finalize()));
    require(
        trace_sha256.as_str() == ORIGINAL_HASH && records == 15_371_848,
        "execution.original_rng_identity",
    )?;
    require(
        streams.len() == 8
            && raw_words == 69_586
            && counts.values().sum::<u64>() == 27_865
            && streams.values().map(|s| s.cursor.raw).sum::<u64>() == raw_words,
        "execution.original_rng_exhaustion",
    )?;
    Ok(OriginalRngReport {
        adapter: "benilla-original-unsealed-tsv-rng-v1",
        projection: Projection::OriginalUnsealedRngV1,
        trace_sha256,
        records,
        initial_states: streams.len() as u64,
        raw_words,
        semantic_draws: counts.values().sum(),
        semantic_families: counts,
        pending: vec![
            PendingF1Obligation::SealedReferenceExecution,
            PendingF1Obligation::CompleteCausalCoverage,
            PendingF1Obligation::UnsupportedSemanticOwners,
        ],
        whole_execution_accepted: false,
    })
}
fn project_line(
    line: &[u8],
    streams: &mut BTreeMap<(u64, u64), Stream>,
    calls: &mut BTreeMap<u64, Call>,
    sequences: &mut BTreeMap<u64, u64>,
    raw_words: &mut u64,
    counts: &mut BTreeMap<String, u64>,
) -> Result<()> {
    let text = std::str::from_utf8(line).map_err(|_| refuse("execution.original_rng_utf8"))?;
    let mut fields = text.splitn(4, '\t');
    let thread: u64 = number(fields.next())?;
    let sequence: u64 = number(fields.next())?;
    let kind = fields
        .next()
        .ok_or_else(|| refuse("execution.original_rng_fields"))?;
    let data = fields
        .next()
        .ok_or_else(|| refuse("execution.original_rng_fields"))?;
    let prior = sequences.entry(thread).or_default();
    require(sequence == *prior, "execution.original_rng_thread_order")?;
    *prior += 1;
    require(sequences.len() <= 64, "execution.original_rng_thread_limit")?;
    match kind {
        "mt_state" => {
            let mut fields = data.split_whitespace();
            let address = address(fields.next())?;
            let mut words = [0; 624];
            for word in &mut words {
                *word = number(fields.next())?;
            }
            let index = number(fields.next())?;
            require(fields.next().is_none(), "execution.original_rng_fields")?;
            let engine = Mt19937::from_state(MtState { words, index })
                .map_err(|_| refuse("execution.original_rng_state"))?;
            require(
                streams
                    .insert(
                        (thread, address),
                        Stream {
                            engine,
                            cursor: DrawCursor::default(),
                        },
                    )
                    .is_none()
                    && streams.len() <= 64,
                "execution.original_rng_duplicate_state",
            )?;
        }
        "rng_call" => {
            require(
                calls.insert(thread, call(data)?).is_none(),
                "execution.original_rng_nested_call",
            )?;
        }
        "mt_raw" => {
            let mut fields = data.split_whitespace();
            let address = address(fields.next())?;
            let position = number(fields.next())?;
            let word = u64::from(number::<u32>(fields.next())?);
            require(fields.next().is_none(), "execution.original_rng_fields")?;
            let stream = streams
                .get_mut(&(thread, address))
                .ok_or_else(|| refuse("execution.original_rng_missing_state"))?;
            if let Some(call) = calls.get_mut(&thread) {
                require(
                    call.engine.is_none_or(|e| e == address)
                        && call.words.len() < 4096
                        && position == stream.cursor.raw + call.words.len() as u64,
                    "execution.original_rng_raw_span",
                )?;
                call.engine = Some(address);
                call.words.push((position, word));
            } else {
                require(
                    position == stream.cursor.raw,
                    "execution.original_rng_cursor",
                )?;
                let mut sink = |_| {};
                let mut random =
                    SourceRandom::new(thread, &mut stream.engine, &mut stream.cursor, &mut sink)
                        .map_err(|_| refuse("execution.original_rng_generator"))?;
                require(
                    random
                        .next_raw()
                        .map_err(|_| refuse("execution.original_rng_generator"))?
                        == word,
                    "execution.original_rng_raw_mismatch",
                )?;
            }
            *raw_words += 1;
        }
        "rng_result" => {
            let call = calls
                .remove(&thread)
                .ok_or_else(|| refuse("execution.original_rng_missing_call"))?;
            let engine = call
                .engine
                .ok_or_else(|| refuse("execution.original_rng_no_span"))?;
            let stream = streams
                .get_mut(&(thread, engine))
                .ok_or_else(|| refuse("execution.original_rng_missing_state"))?;
            let mut matched = 0;
            let mut equal = true;
            let result = {
                let mut sink = |event: TraceEvent| {
                    if let RngEvent::Raw { position, word, .. } = event.kind {
                        equal &= call.words.get(matched) == Some(&(position, word));
                        matched += 1;
                    }
                };
                let mut random =
                    SourceRandom::new(thread, &mut stream.engine, &mut stream.cursor, &mut sink)
                        .map_err(|_| refuse("execution.original_rng_generator"))?;
                invoke(&call, &mut random)?
            };
            require(
                equal && matched == call.words.len(),
                "execution.original_rng_raw_mismatch",
            )?;
            require(
                result == expected(&call, data)?,
                "execution.original_rng_semantic_mismatch",
            )?;
            *counts.entry(call.name).or_default() += 1;
        }
        _ => (),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_hexadecimal_and_decimal_float_bits() {
        assert_eq!(
            double(Some("0x1.3b5408d0bf385p+1")).unwrap(),
            0x4003b5408d0bf385
        );
        assert_eq!(single(Some("-0x0p+0")).unwrap(), 0x80000000);
        assert_eq!(single(Some("1.25")).unwrap(), 1.25_f32.to_bits());
        assert!(double(Some("not-a-float")).is_err());
    }
}
