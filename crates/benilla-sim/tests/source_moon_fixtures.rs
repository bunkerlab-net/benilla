// SPDX-License-Identifier: GPL-3.0-or-later
// Conditional component evidence: libc answers and sine results are injected, per source_moon/FORMAT.md.
use benilla_sim::{
    source_calendar::{self, Tm},
    source_moon::{self as moon, Calendar, Sine64},
};
use serde_json::{json, Value};
use std::{cell::RefCell, path::Path, rc::Rc};

const OWNER: moon::CallOwner = moon::CallOwner { thread: 1, call: 1 };
// Bind one ordering for the entire measured corpus, per source_moon/FORMAT.md §Numerical service records.
const ORDER: moon::SineOrder = moon::SineOrder::SolarFirst;

struct Stream {
    case: usize,
    records: Vec<Value>,
    cursor: usize,
    clocks: Vec<i64>,
    clock_cursor: usize,
    candidate: Option<(i32, usize)>,
    utc: bool,
}
type Shared = Rc<RefCell<Stream>>;

fn signed(value: &Value) -> i64 {
    value.as_i64().expect("exact signed integer")
}
fn int(value: &Value) -> i32 {
    i32::try_from(signed(value)).expect("signed 32-bit integer")
}
fn word(value: &Value) -> u64 {
    let text = value.as_str().expect("binary64 word");
    assert_eq!(text.len(), 16);
    assert!(text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));
    u64::from_str_radix(text, 16).unwrap()
}
fn civil(value: &Value) -> moon::CivilDate {
    assert_eq!(value.as_array().unwrap().len(), 3);
    moon::CivilDate {
        year: int(&value[0]),
        month: int(&value[1]),
        day: int(&value[2]),
    }
}
fn date_value(date: moon::CivilDate) -> Value {
    json!([date.year, date.month, date.day])
}
fn moon_value(date: moon::MoonDate) -> Value {
    json!([
        date.civil.year,
        date.civil.month,
        date.civil.day,
        date.hour,
        date.minute
    ])
}
fn tm(value: &Value) -> Tm {
    assert_eq!(value.as_array().unwrap().len(), 9);
    Tm {
        sec: int(&value[0]),
        min: int(&value[1]),
        hour: int(&value[2]),
        mday: int(&value[3]),
        mon: int(&value[4]),
        year: int(&value[5]),
        isdst: int(&value[6]),
        wday: int(&value[7]),
        yday: int(&value[8]),
    }
}
fn tm_input(value: Tm) -> Value {
    json!([
        value.sec,
        value.min,
        value.hour,
        value.mday,
        value.mon,
        value.year,
        value.isdst
    ])
}
fn tm_output(value: Tm) -> Value {
    json!([
        value.sec,
        value.min,
        value.hour,
        value.mday,
        value.mon,
        value.year,
        value.isdst,
        value.wday,
        value.yday
    ])
}
fn first_difference(expected: &Value, actual: &Value, field: String) -> Option<String> {
    if let (Some(expected), Some(actual)) = (expected.as_array(), actual.as_array()) {
        if expected.len() != actual.len() {
            return Some(format!("{field}.length"));
        }
        for (index, (expected, actual)) in expected.iter().zip(actual).enumerate() {
            if let Some(found) = first_difference(expected, actual, format!("{field}[{index}]")) {
                return Some(found);
            }
        }
        None
    } else if expected != actual {
        Some(field)
    } else {
        None
    }
}
impl Stream {
    fn peek(&self, label: &str) -> Value {
        let record = self.records.get(self.cursor).unwrap_or_else(|| {
            panic!(
                "case {} missing record {} for {label}",
                self.case, self.cursor
            )
        });
        assert_eq!(
            record[0].as_str(),
            Some(label),
            "case {} record {} operation",
            self.case,
            self.cursor
        );
        record.clone()
    }
    fn compare(&mut self, actual: Value) {
        let expected = self.records.get(self.cursor).unwrap_or_else(|| {
            panic!(
                "case {} surplus record {}: {actual}",
                self.case, self.cursor
            )
        });
        if let Some(field) = first_difference(expected, &actual, "record".into()) {
            panic!(
                "case {} record {} field {field}: expected {expected}, actual {actual}",
                self.case, self.cursor
            );
        }
        self.cursor += 1;
    }
}
struct Trace(Shared);
impl moon::Observe for Trace {
    fn observe(&mut self, evidence: moon::Evidence) -> moon::Result<()> {
        use moon::Evidence;
        let mut stream = self.0.borrow_mut();
        let record = match evidence {
            Evidence::CandidateBegin { cycle } => {
                assert!(stream.candidate.is_none(), "previous candidate incomplete");
                stream.candidate = Some((cycle, 0));
                json!(["candidate_begin", cycle, 0])
            }
            Evidence::Candidate {
                cycle,
                day,
                residual,
                full,
            } => {
                assert_eq!(stream.candidate.take(), Some((cycle, 2)));
                json!([
                    "candidate_end",
                    day,
                    format!("{residual:016x}"),
                    format!("{full:016x}")
                ])
            }
            Evidence::CivilDay { input, day } => json!(["civil_day", date_value(input), day]),
            Evidence::DayCivil { day, result } => json!(["day_civil", day, date_value(result)]),
            Evidence::NextBegin(input) => json!(["next_begin", moon_value(input), 0]),
            Evidence::NextEnd(output) => json!(["next_end", moon_value(output)]),
            Evidence::SummerDayBegin(day) => json!(["summer_day_begin", day]),
            Evidence::SummerDayEnd(value) => json!(["summer_day_end", i32::from(value)]),
            Evidence::SummerCivilBegin(date) => json!(["summer_civil_begin", date_value(date)]),
            Evidence::SummerCivilEnd(value) => json!(["summer_civil_end", i32::from(value)]),
            // These internal roundings have no oracle records; external calls are compared by their injected adapters.
            Evidence::Float { .. }
            | Evidence::Integer { .. }
            | Evidence::Sine { .. }
            | Evidence::Calendar(_) => return Ok(()),
        };
        stream.compare(record);
        Ok(())
    }
}
struct Sine(Shared);
impl Sine64 for Sine {
    fn owner(&self) -> moon::Result<moon::CallOwner> {
        Ok(OWNER)
    }
    fn order(&self) -> moon::Result<moon::SineOrder> {
        Ok(ORDER)
    }
    fn evaluate(&mut self, cycle: i32, anomaly: moon::Anomaly, operand: u64) -> moon::Result<u64> {
        let mut stream = self.0.borrow_mut();
        let (expected_cycle, position) = stream.candidate.expect("sine outside candidate");
        assert_eq!(expected_cycle, cycle);
        assert_eq!(
            anomaly,
            if position == 0 {
                moon::Anomaly::Solar
            } else {
                moon::Anomaly::Lunar
            }
        );
        assert!(position < 2);
        let expected = stream.peek("sine");
        let result = word(&expected[2]);
        word(&expected[1]);
        stream.compare(json!([
            "sine",
            format!("{operand:016x}"),
            format!("{result:016x}")
        ]));
        stream.candidate = Some((cycle, position + 1));
        Ok(result)
    }
    fn finish(&self) -> moon::Result<()> {
        if self.0.borrow().candidate.is_none() {
            Ok(())
        } else {
            Err(moon::Error::MissingObservation)
        }
    }
}
struct Clock(Shared);
impl Calendar for Clock {
    fn owner(&self) -> moon::CallOwner {
        OWNER
    }
    fn now(&mut self) -> moon::Result<i64> {
        let mut stream = self.0.borrow_mut();
        let seconds = *stream
            .clocks
            .get(stream.clock_cursor)
            .ok_or(moon::Error::MissingObservation)?;
        stream.clock_cursor += 1;
        stream.compare(json!(["time", seconds]));
        Ok(seconds)
    }
    fn local(&mut self, seconds: i64) -> moon::Result<Tm> {
        let mut stream = self.0.borrow_mut();
        let expected = stream.peek("local");
        let mut result = tm(&expected[2]);
        let mut offset = signed(&expected[3]);
        let mut zone = expected[4].as_str().unwrap();
        if stream.utc {
            result = source_calendar::localtime(seconds).expect("UTC local conversion");
            offset = 0;
            zone = "UTC";
        }
        stream.compare(json!(["local", seconds, tm_output(result), offset, zone]));
        Ok(result)
    }
    fn epoch(&mut self, input: moon::CalendarInput) -> moon::Result<(i64, Tm)> {
        let mut stream = self.0.borrow_mut();
        let expected = stream.peek("make");
        let fields = match input {
            moon::CalendarInput::Retained(value) => value,
            moon::CalendarInput::AnchorProbe(date) => Tm {
                sec: 0,
                min: 0,
                hour: 3,
                mday: date.day,
                mon: date.month.checked_sub(1).unwrap(),
                year: date.year.checked_sub(1900).unwrap(),
                isdst: -1,
                ..Tm::default()
            },
        };
        let mut seconds = signed(&expected[2]);
        let mut result = tm(&expected[3]);
        let mut offset = signed(&expected[4]);
        let mut zone = expected[5].as_str().unwrap();
        if stream.utc {
            result = fields;
            seconds = source_calendar::mktime(&mut result).expect("UTC epoch conversion");
            offset = 0;
            zone = "UTC";
        }
        stream.compare(json!([
            "make",
            tm_input(fields),
            seconds,
            tm_output(result),
            offset,
            zone
        ]));
        Ok((seconds, result))
    }
    fn finish(&self) -> moon::Result<()> {
        let stream = self.0.borrow();
        if stream.clock_cursor == stream.clocks.len() {
            Ok(())
        } else {
            Err(moon::Error::SurplusObservation)
        }
    }
}

fn optional_fixture(path: &Path) -> Option<String> {
    match std::fs::read_to_string(path) {
        Ok(value) => Some(value),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("SKIP: authored lunar fixture absent: {}", path.display());
            None
        }
        Err(error) => panic!("{}: {error}", path.display()),
    }
}

#[test]
fn every_registered_lunar_record_matches_in_order_without_tolerance() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tools/fixtures/source_moon");
    let Some(manifest) = optional_fixture(&root.join("cases.json")) else {
        return;
    };
    let Some(expected) = optional_fixture(&root.join("expected.jsonl")) else {
        return;
    };
    let manifest: Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["format"], "benilla-moon-authored-inputs-v1");
    let cases = manifest["cases"].as_array().unwrap();
    let expected: Vec<Value> = expected
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(cases.len(), 11192);
    assert_eq!(expected.len(), cases.len());
    let mut counts = [0; 5];
    for (index, (case, expected)) in cases.iter().zip(expected).enumerate() {
        assert_eq!(signed(&case["case"]), index as i64);
        assert_eq!(signed(&expected["case"]), index as i64);
        let kind = case["kind"].as_str().unwrap();
        let clocks: Vec<_> = case.get("clocks").map_or_else(Vec::new, |values| {
            values.as_array().unwrap().iter().map(signed).collect()
        });
        let shared = Rc::new(RefCell::new(Stream {
            case: index,
            records: expected["records"].as_array().unwrap().clone(),
            cursor: 0,
            clocks,
            clock_cursor: 0,
            candidate: None,
            utc: case.get("zone").is_none_or(|zone| zone == "UTC"),
        }));
        let mut trace = Trace(shared.clone());
        let mut clock = Clock(shared.clone());
        let mut sine = Sine(shared.clone());
        shared.borrow_mut().compare(json!(["begin", kind]));
        let result = match kind {
            "event" => {
                counts[0] += 1;
                let (start, end) =
                    moon::compute_event(signed(&case["epoch"]), &mut clock, &mut sine, &mut trace)
                        .unwrap();
                json!([start, end])
            }
            "next_date" => {
                counts[1] += 1;
                let input = case["civil"].as_array().unwrap();
                assert_eq!(input.len(), 5);
                let date = moon::MoonDate {
                    civil: moon::CivilDate {
                        year: int(&input[0]),
                        month: int(&input[1]),
                        day: int(&input[2]),
                    },
                    hour: int(&input[3]),
                    minute: int(&input[4]),
                };
                moon_value(moon::next_date(date, &mut clock, &mut sine, &mut trace).unwrap())
            }
            "candidate" => {
                counts[2] += 1;
                let value = moon::candidate(int(&case["cycle"]), &mut sine, &mut trace).unwrap();
                json!([
                    value.day,
                    format!("{:016x}", value.residual.to_bits()),
                    format!("{:016x}", value.full.to_bits())
                ])
            }
            "civil_to_day" => {
                counts[3] += 1;
                json!(moon::observe_julian_day(civil(&case["civil"]), &mut trace).unwrap())
            }
            "day_to_civil" => {
                counts[4] += 1;
                date_value(moon::observe_civil_date(signed(&case["day"]), &mut trace).unwrap())
            }
            _ => panic!("unregistered lunar case kind {kind}"),
        };
        sine.finish().unwrap();
        clock.finish().unwrap();
        let mut stream = shared.borrow_mut();
        stream.compare(json!(["result", kind, result]));
        let consumed = stream.clock_cursor;
        stream.compare(json!(["complete", consumed]));
        assert_eq!(
            stream.cursor,
            stream.records.len(),
            "case {index} surplus records"
        );
    }
    assert_eq!(counts, [204, 960, 10001, 16, 11]);
}
