use super::*;
use benilla_sim::event_ai::update_holder_timer;
use benilla_sim::source_rng::{DrawCursor, RawGenerator, SourceRandom};
use std::collections::{BTreeMap, VecDeque};
use std::io::{BufRead, BufReader};

#[derive(Default)]
struct RecordedWords(VecDeque<u64>);
impl RawGenerator for RecordedWords {
    const MIN: u64 = 0;
    const MAX: u64 = u32::MAX as u64;
    fn next_word(&mut self) -> u64 {
        self.0
            .pop_front()
            .expect("projection requested unrecorded word")
    }
}
struct Call {
    range: [u32; 2],
    words: RecordedWords,
}
#[test]
fn retained_seal16_all_1306_varied_timer_requests_match_or_explicit_absence() {
    let Some(path) = std::env::var_os("BENILLA_F5_EVENT_AI_TRACE") else {
        println!("SKIP EventAI F1 varied-timer projection: set BENILLA_F5_EVENT_AI_TRACE to seal16-full-01/run/trace.tsv; other lifecycle/event bindings unavailable");
        return;
    };
    // Comparison-only raw words never enter SourceEventAi or a live source engine.
    let file = std::fs::File::open(path).unwrap();
    let mut reader = BufReader::with_capacity(1 << 20, file);
    let mut line = String::with_capacity(4096);
    let mut hash = Sha256::new();
    let mut pending = BTreeMap::<u64, Call>::new();
    let mut records = 0u64;
    let mut requests = 0usize;
    let mut words = 0usize;
    loop {
        line.clear();
        if reader.read_line(&mut line).unwrap() == 0 {
            break;
        }
        hash.update(line.as_bytes());
        records += 1;
        let mut fields = line.trim_end_matches('\n').splitn(4, '\t');
        let thread = fields.next().unwrap().parse::<u64>().unwrap();
        let _sequence = fields.next().unwrap();
        let kind = fields.next().unwrap();
        let data = fields.next().unwrap_or("");
        if kind == "rng_call"
            && data
                .starts_with("urand /reference/source/src/game/AI/EventAI/CreatureEventAI.cpp 40 ")
        {
            let mut args = data.split_whitespace().skip(3);
            let min = args.next().unwrap().parse().unwrap();
            let max = args.next().unwrap().parse().unwrap();
            assert!(args.next().is_none());
            assert!(pending
                .insert(
                    thread,
                    Call {
                        range: [min, max],
                        words: RecordedWords::default()
                    }
                )
                .is_none());
        } else if kind == "mt_raw" {
            if let Some(call) = pending.get_mut(&thread) {
                let word = data
                    .split_whitespace()
                    .nth(2)
                    .unwrap()
                    .parse::<u64>()
                    .unwrap();
                call.words.0.push_back(word);
            }
        } else if kind == "rng_result" {
            if let Some(mut call) = pending.remove(&thread) {
                let mut result = data.split_whitespace();
                assert_eq!(result.next(), Some("urand"));
                let expected = result.next().unwrap().parse::<u32>().unwrap();
                assert!(result.next().is_none());
                assert!(call.range[0] < call.range[1]);
                let raw_count = call.words.0.len();
                let mut cursor = DrawCursor::default();
                let mut timer = 0;
                let mut enabled = true;
                let mut sink = |_| {};
                let mut random =
                    SourceRandom::new(thread, &mut call.words, &mut cursor, &mut sink).unwrap();
                assert!(update_holder_timer(
                    &mut timer,
                    &mut enabled,
                    call.range,
                    &mut |min, max| random.urand(min, max)
                )
                .unwrap());
                assert_eq!(timer, expected, "timer request {requests}");
                assert!(enabled);
                assert!(call.words.0.is_empty());
                assert_eq!(cursor.raw, raw_count as u64);
                words += raw_count;
                requests += 1;
            }
        }
    }
    assert!(pending.is_empty());
    assert_eq!(records, 73721588);
    assert_eq!(requests, 1306);
    assert_eq!(
        format!("{:x}", hash.finalize()),
        "9cfa759e74a34e4768fa011f69365b32b050ede9d35b2a37afbda8df2b7b091a"
    );
    println!("seal16 EventAI varied timers: {requests} requests, {words} raw words, zero mismatches; helper-only unsealed comparison. Per-instance lifecycle/event binding projection unavailable.");
}
