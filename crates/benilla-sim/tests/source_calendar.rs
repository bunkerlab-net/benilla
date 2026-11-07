// SPDX-License-Identifier: GPL-3.0-or-later
// Authored calendar fixtures, not sealed reference execution.
use benilla_sim::source_calendar::{field_time, localtime, mktime, Tm};

#[test]
fn utc_conversion_normalizes_partial_fields_and_dst_input() {
    let mut date = Tm {
        year: 124,
        mon: 1,
        mday: 30,
        ..Tm::default()
    };
    assert_eq!(mktime(&mut date).unwrap(), 1_709_251_200);
    assert_eq!((date.mon, date.mday, date.wday, date.yday), (2, 1, 5, 60));
    assert_eq!(localtime(1_709_251_200).unwrap(), date);
    date.isdst = 1;
    assert_eq!(mktime(&mut date).unwrap(), 1_709_247_600);
    assert_eq!((date.mon, date.mday, date.hour, date.isdst), (1, 29, 23, 0));
    assert_eq!(field_time(b"not a date").unwrap().seconds, -2_209_075_200);
    assert_eq!(
        field_time(b"2024-02-30 12:34:56 trailing").unwrap().seconds,
        1_709_296_496
    );
}
