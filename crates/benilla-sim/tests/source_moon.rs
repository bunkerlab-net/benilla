// SPDX-License-Identifier: GPL-3.0-or-later
// Authored mathematical invariants from moon-spec.md, not reference execution.
use benilla_sim::source_moon::{self as moon, CivilDate, Error};

#[test]
fn negative_integral_whole_part_is_not_floor() {
    assert_eq!(moon::whole_part(-2.0), -3.0);
    assert_eq!(moon::whole_part(-2.5), -3.0);
    assert_eq!(moon::whole_part(2.0), 2.0);
}

#[test]
fn floating_integer_conversion_refuses_undefined_values() {
    assert_eq!(moon::truncate_i32(2147483647.5), Ok(i32::MAX));
    assert_eq!(
        moon::truncate_i32(2147483648.0),
        Err(Error::FloatConversion)
    );
    assert_eq!(
        moon::truncate_i64(9223372036854775808.0),
        Err(Error::FloatConversion)
    );
    assert_eq!(moon::truncate_i64(-9223372036854775808.0), Ok(i64::MIN));
    assert_eq!(moon::truncate_i64(f64::NAN), Err(Error::FloatConversion));
}

#[test]
fn hybrid_calendar_uses_civil_cutover_keys_and_omits_year_zero() {
    let date = |year, month, day| CivilDate { year, month, day };
    assert_eq!(moon::julian_day(date(1582, 10, 4)), Ok(2299160));
    assert_eq!(moon::julian_day(date(1582, 10, 14)), Ok(2299170));
    assert_eq!(moon::julian_day(date(1582, 10, 15)), Ok(2299161));
    assert_eq!(moon::civil_date(2299160), Ok(date(1582, 10, 4)));
    assert_eq!(moon::civil_date(2299161), Ok(date(1582, 10, 15)));
    assert_eq!(moon::julian_day(date(0, 12, 21)), Ok(0));
    assert_eq!(moon::julian_day(date(-1, 1, 1)), Ok(1721058));
    assert_eq!(moon::civil_date(1721058), Ok(date(-1, 1, 1)));
    assert_eq!(moon::julian_day(date(2000, 0, 21)), Ok(2451534));
}

#[test]
fn dst_subtraction_and_addition_remain_distinct_observations() {
    let selected = moon::Candidate {
        cycle: 0,
        day: 2451544,
        residual: 0.5,
        full: 2451544.5,
    };
    let mut trace = Vec::new();
    let result = moon::adjust_selected(selected, true, true, &mut trace).unwrap();
    assert_eq!(
        result.civil,
        CivilDate {
            year: 2000,
            month: 1,
            day: 1
        }
    );
    assert_eq!((result.hour, result.minute), (0, 0));
    let operations: Vec<_> = trace
        .iter()
        .filter_map(|item| match item {
            moon::Evidence::Float { name, bits }
                if matches!(
                    *name,
                    "noon_shift" | "current_dst_subtraction" | "anchor_dst_addition"
                ) =>
            {
                Some((*name, *bits))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        operations,
        vec![
            ("noon_shift", 2451545.0_f64.to_bits()),
            ("current_dst_subtraction", 2451544.9583333335_f64.to_bits()),
            ("anchor_dst_addition", 2451545.0_f64.to_bits()),
        ]
    );
}

const OWNER: moon::CallOwner = moon::CallOwner { thread: 1, call: 7 };

#[test]
fn observation_streams_refuse_missing_surplus_wrong_order_and_wrong_owner() {
    use moon::Calendar;
    let mut absent = moon::RecordedCalendar::new(OWNER, &[]);
    assert_eq!(absent.now(), Err(Error::MissingObservation));
    let samples = [moon::CalendarSample {
        owner: OWNER,
        operation: moon::CalendarCall::Now { seconds: 42 },
    }];
    let mut recorded = moon::RecordedCalendar::new(OWNER, &samples);
    assert_eq!(recorded.finish(), Err(Error::SurplusObservation));
    assert_eq!(recorded.now(), Ok(42));
    assert_eq!(recorded.finish(), Ok(()));
    assert_eq!(recorded.now(), Err(Error::MissingObservation));
    let mut wrong_order = moon::RecordedCalendar::new(OWNER, &samples);
    assert_eq!(wrong_order.local(42), Err(Error::ObservationMismatch));
    let mut wrong_owner =
        moon::RecordedCalendar::new(moon::CallOwner { thread: 2, call: 7 }, &samples);
    assert_eq!(wrong_owner.now(), Err(Error::OwnershipMismatch));
}

#[test]
fn missing_historical_sine_binding_refuses_before_calendar_consumption() {
    let mut calendar = moon::RecordedCalendar::new(OWNER, &[]);
    assert_eq!(
        moon::compute_event(0, &mut calendar, &mut moon::UnboundSine, &mut Vec::new()),
        Err(Error::MissingSineBinding),
    );
}

#[test]
fn missing_sine_order_and_surplus_samples_refuse() {
    use moon::Sine64;
    let mut unbound_order = moon::RecordedSine::new(OWNER, None, &[]);
    assert_eq!(
        moon::candidate(0, &mut unbound_order, &mut Vec::new()),
        Err(Error::MissingSineOrder)
    );
    let samples = [moon::SineSample {
        owner: OWNER,
        cycle: 0,
        anomaly: moon::Anomaly::Solar,
        operand: 0,
        result: 0,
    }];
    let mut sine = moon::RecordedSine::new(OWNER, Some(moon::SineOrder::SolarFirst), &samples);
    assert_eq!(sine.finish(), Err(Error::SurplusObservation));
    assert_eq!(
        sine.evaluate(0, moon::Anomaly::Lunar, 0),
        Err(Error::ObservationMismatch)
    );
    assert_eq!(sine.evaluate(0, moon::Anomaly::Solar, 0), Ok(0));
    assert_eq!(sine.finish(), Ok(()));
    assert_eq!(
        sine.evaluate(0, moon::Anomaly::Solar, 0),
        Err(Error::MissingObservation)
    );
}

#[test]
fn calendar_integer_overflow_and_unproved_narrowing_refuse() {
    assert_eq!(moon::civil_date(i64::MAX), Err(Error::Overflow));
    assert_eq!(moon::civil_date(i64::MIN), Err(Error::Overflow));
    assert_eq!(
        moon::julian_day(CivilDate {
            year: i32::MIN,
            month: i32::MAX,
            day: 1
        }),
        Err(Error::Overflow)
    );
    assert_eq!(
        moon::civil_date(1_000_000_000_000_000),
        Err(Error::UnprovedNarrowing)
    );
}

#[test]
fn event_requires_shared_sine_calendar_ownership_and_initial_clock_record() {
    let mut sine = moon::RecordedSine::new(OWNER, Some(moon::SineOrder::SolarFirst), &[]);
    let mut calendar = moon::RecordedCalendar::new(
        moon::CallOwner {
            thread: OWNER.thread,
            call: OWNER.call + 1,
        },
        &[],
    );
    assert_eq!(
        moon::compute_event(0, &mut calendar, &mut sine, &mut Vec::new()),
        Err(Error::OwnershipMismatch),
    );
    let mut calendar = moon::RecordedCalendar::new(OWNER, &[]);
    assert_eq!(
        moon::compute_event(0, &mut calendar, &mut sine, &mut Vec::new()),
        Err(Error::MissingObservation),
    );
}
