// SPDX-License-Identifier: GPL-3.0-or-later
// Authored component fixtures, not a sealed execution.
use benilla_sim::source_calendar::{compute_schedule, field_time, gauss_easter};

#[test]
fn dmf_stages_and_yearly_leap_dates_preserve_source_tm_mutations() {
    let today = field_time(b"2024-01-31 12:00:00").unwrap().seconds;
    let show = compute_schedule(2, 0, 0, today).unwrap();
    assert_eq!(
        show,
        (
            field_time(b"2024-01-01 00:00:00").unwrap().seconds,
            field_time(b"2024-01-08 00:00:00").unwrap().seconds
        )
    );
    let stage = compute_schedule(5, 0, 0, today).unwrap();
    assert_eq!(stage.0, field_time(b"2023-12-30 00:00:00").unwrap().seconds);
    assert_eq!(stage.1 - stage.0, 86400);
    let old = field_time(b"2020-02-29 03:04:05").unwrap().seconds;
    let yearly = compute_schedule(
        11,
        old,
        old + 86400,
        field_time(b"2023-07-01 00:00:00").unwrap().seconds,
    )
    .unwrap();
    assert_eq!(
        yearly.0,
        field_time(b"2023-03-01 03:04:05").unwrap().seconds
    );
    assert_eq!(yearly.0, yearly.1);
}

#[test]
fn easter_retains_the_pinned_float_algorithm_corner_years() {
    // The pinned fractional P/4 formula differs from the real Easter calendar.
    assert_eq!(gauss_easter(1954).unwrap(), (1954, 4, 24));
    assert_eq!(gauss_easter(1981).unwrap(), (1981, 4, 18));
    assert_eq!(gauss_easter(2024).unwrap(), (2024, 3, 31));
}

#[test]
fn every_complete_source_oracle_record_matches() {
    compare_cases(
        include_str!("../tools/fixtures/source_calendar/cases.json"),
        include_str!("../tools/fixtures/source_calendar/expected.tsv"),
    );
}

fn compare_cases(cases: &str, expected: &str) {
    let cases: Vec<serde_json::Value> = serde_json::from_str(cases).unwrap();
    let outputs: Vec<_> = expected.lines().collect();
    assert_eq!(cases.len(), outputs.len());
    for (case, expected) in cases.iter().zip(outputs) {
        let words: Vec<_> = expected.split_whitespace().collect();
        let actual = match case[0].as_str().unwrap() {
            "F" => {
                let result = field_time(case[1].as_str().unwrap().as_bytes()).unwrap();
                let mut values = vec![
                    result.seconds,
                    i64::from(result.parsed),
                    result.consumed as i64,
                ];
                for t in [result.before, result.after] {
                    values.extend(
                        [
                            t.sec, t.min, t.hour, t.mday, t.mon, t.year, t.isdst, t.wday, t.yday,
                        ]
                        .map(i64::from),
                    );
                }
                values
            }
            "E" => {
                let (year, month, day) = gauss_easter(case[1].as_i64().unwrap() as i32).unwrap();
                vec![i64::from(year), i64::from(month), i64::from(day)]
            }
            "S" => {
                let (start, end) = compute_schedule(
                    case[1].as_u64().unwrap() as u32,
                    case[2].as_i64().unwrap(),
                    case[3].as_i64().unwrap(),
                    case[4].as_i64().unwrap(),
                )
                .unwrap();
                vec![start, end]
            }
            _ => panic!("unknown source fixture"),
        };
        assert_eq!(
            actual,
            words[1..]
                .iter()
                .map(|s| s.parse::<i64>().unwrap())
                .collect::<Vec<_>>(),
            "{case}"
        );
    }
}

#[test]
fn raw_stock_dates_or_explicit_private_absence() {
    let Some(path) = std::env::var_os("BENILLA_F4_EVENT_CALENDAR_STOCK") else {
        println!("SKIP unsealed stock event date fixtures: set BENILLA_F4_EVENT_CALENDAR_STOCK");
        return;
    };
    let path = std::path::Path::new(&path);
    let cases = std::fs::read_to_string(path.join("cases.json")).unwrap();
    let expected = std::fs::read_to_string(path.join("expected.tsv")).unwrap();
    let rows: Vec<serde_json::Value> = serde_json::from_str(&cases).unwrap();
    assert_eq!(rows.len(), 76);
    compare_cases(&cases, &expected);
    println!("All 76 raw stock event date conversions match the pinned source oracle; not effective definitions or F1 acceptance");
}
