// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, GameEventMgr.cpp:1162-1323; Field.cpp:24-30.
// UTC Gregorian conversion and the reached GCC 12.2 std::get_time numeric format.
use serde::{Deserialize, Serialize};

pub const POLICY: &str = "cmangos-8ec338a1-glibc236-utc-calendar-v1";
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Overflow,
    YearRange,
    LunarOwner,
}
pub type Result<T> = std::result::Result<T, Error>;

/// The seven consumed mktime inputs and its defined weekday/year-day outputs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tm {
    pub sec: i32,
    pub min: i32,
    pub hour: i32,
    pub mday: i32,
    pub mon: i32,
    pub year: i32,
    pub isdst: i32,
    pub wday: i32,
    pub yday: i32,
}

fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn year_start(year: i64) -> i64 {
    let y = year - 1;
    365 * (year - 1970) + y.div_euclid(4) - 1969_i64.div_euclid(4) - y.div_euclid(100)
        + 1969_i64.div_euclid(100)
        + y.div_euclid(400)
        - 1969_i64.div_euclid(400)
}
const MONTH_DAYS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];

/// Pinned UTC localtime and gmtime have identical defined output fields.
pub fn localtime(seconds: i64) -> Result<Tm> {
    let days = seconds.div_euclid(86400);
    // The full int tm_year range fits comfortably inside this estimate.
    let mut year = 1970 + days.div_euclid(366);
    if year < i64::from(i32::MIN) + 1899 || year > i64::from(i32::MAX) + 1902 {
        return Err(Error::YearRange);
    }
    // Calendar cycles avoid a loop proportional to a far-away year.
    let era = (days + 719528).div_euclid(146097);
    year = era * 400;
    while year_start(year + 1) <= days {
        year += 1;
    }
    while year_start(year) > days {
        year -= 1;
    }
    let year_field = i32::try_from(year - 1900).map_err(|_| Error::YearRange)?;
    let yday = days - year_start(year);
    let mut day = yday;
    let mut mon = 0;
    while mon < 11 {
        let length = MONTH_DAYS[mon] + i64::from(mon == 1 && leap(year));
        if day < length {
            break;
        }
        day -= length;
        mon += 1;
    }
    let clock = seconds.rem_euclid(86400);
    Ok(Tm {
        year: year_field,
        mon: mon as i32,
        mday: day as i32 + 1,
        hour: (clock / 3600) as i32,
        min: (clock / 60 % 60) as i32,
        sec: (clock % 60) as i32,
        isdst: 0,
        wday: (days + 4).rem_euclid(7) as i32,
        yday: yday as i32,
    })
}

/// Normalize in the bound UTC zone. A positive tm_isdst requests a one-hour shift.
pub fn mktime(tm: &mut Tm) -> Result<i64> {
    let year = i64::from(tm.year) + 1900 + i64::from(tm.mon).div_euclid(12);
    let mon = i64::from(tm.mon).rem_euclid(12) as usize;
    let mut days = year_start(year) + i64::from(tm.mday) - 1;
    for (m, length) in MONTH_DAYS.iter().enumerate().take(mon) {
        days += length + i64::from(m == 1 && leap(year));
    }
    let seconds = days
        .checked_mul(86400)
        .and_then(|s| s.checked_add(i64::from(tm.hour) * 3600))
        .and_then(|s| s.checked_add(i64::from(tm.min) * 60))
        .and_then(|s| s.checked_add(i64::from(tm.sec)))
        .and_then(|s| s.checked_sub(if tm.isdst > 0 { 3600 } else { 0 }))
        .ok_or(Error::Overflow)?;
    let normalized = localtime(seconds)?;
    *tm = normalized;
    Ok(seconds)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedTime {
    pub seconds: i64,
    pub parsed: bool,
    pub consumed: usize,
    pub before: Tm,
    pub after: Tm,
}

// GCC12.2 locale_facets_nonio.tcc:1048-1077. Overflow leaves the offending byte.
fn number(input: &[u8], cursor: &mut usize, low: i32, high: i32, width: usize) -> Option<i32> {
    let mut value = 0;
    let mut count = 0;
    while count < width {
        let Some(digit) = input.get(*cursor).filter(|v| v.is_ascii_digit()) else {
            break;
        };
        value = value * 10 + i32::from(digit - b'0');
        if value > high {
            return None;
        }
        *cursor += 1;
        count += 1;
    }
    (count > 0 && value >= low).then_some(value)
}

/// Field::GetTime ignores get_time failure and calls mktime on its partial writes.
pub fn field_time(input: &[u8]) -> Result<ParsedTime> {
    let input = input.split(|byte| *byte == 0).next().unwrap_or_default();
    let mut tm = Tm::default();
    let mut cursor = 0;
    // The istream sentry skips leading whitespace, unlike individual numeric fields.
    while input.get(cursor).is_some_and(u8::is_ascii_whitespace) {
        cursor += 1;
    }
    let mut parsed = cursor < input.len();
    let mut want_xday = false;
    for (field, (low, high, width, separator)) in [
        (0, 9999, 4, b'-'),
        (1, 12, 2, b'-'),
        (1, 31, 2, b' '),
        (0, 23, 2, b':'),
        (0, 59, 2, b':'),
        (0, 60, 2, 0),
    ]
    .into_iter()
    .enumerate()
    {
        if cursor == input.len() {
            parsed = false;
            break;
        }
        if field == 2 && input[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(value) = number(input, &mut cursor, low, high, width) else {
            parsed = false;
            break;
        };
        match field {
            0 => {
                tm.year = value - 1900;
                want_xday = true;
            }
            1 => tm.mon = value - 1,
            2 => tm.mday = value,
            3 => tm.hour = value,
            4 => tm.min = value,
            _ => tm.sec = value,
        }
        // time_get stops on eofbit alone, without failbit, immediately after a field.
        if cursor == input.len() {
            break;
        }
        if separator == b' ' {
            while input.get(cursor).is_some_and(u8::is_ascii_whitespace) {
                cursor += 1;
            }
        } else if separator != 0 {
            if input.get(cursor) != Some(&separator) {
                parsed = false;
                break;
            }
            cursor += 1;
        }
    }
    // GCC12.2 locale_facets.cc:211-263 finalizes even after a partial failure.
    if want_xday {
        let corr_year = 1900 + tm.year - i32::from(tm.mon < 2);
        let month_start: i32 = MONTH_DAYS[..tm.mon as usize].iter().sum::<i64>() as i32;
        let wday = -473 + 365 * (tm.year - 70) + corr_year / 4 - (corr_year / 4) / 25
            + i32::from((corr_year / 4) % 25 < 0)
            + ((corr_year / 4) / 25) / 4
            + month_start
            + tm.mday
            - 1;
        tm.wday = wday.rem_euclid(7);
        tm.yday =
            month_start + i32::from(tm.mon > 1 && leap(i64::from(tm.year) + 1900)) + tm.mday - 1;
    }
    let before = tm;
    let seconds = mktime(&mut tm)?;
    Ok(ParsedTime {
        seconds,
        parsed,
        consumed: cursor,
        before,
        after: tm,
    })
}

/// Literal binary32 Gauss calculation from GameEventMgr.cpp:1162-1208.
pub fn gauss_easter(year: i32) -> Result<(i32, i32, i32)> {
    let a = (year % 19) as f32;
    let b = (year % 4) as f32;
    let c = (year % 7) as f32;
    let p = (year / 100) as f32;
    let q = ((13.0 + 8.0 * p) / 25.0).floor();
    let integer = |x: f32| -> Result<i32> {
        if x >= i32::MIN as f32 && x < 2147483648.0 && x.is_finite() {
            Ok(x as i32)
        } else {
            Err(Error::Overflow)
        }
    };
    let m = (integer(15.0 - q + p - p / 4.0)? % 30) as f32;
    let n = (integer(4.0 + p - p / 4.0)? % 7) as f32;
    let d = (integer(19.0 * a + m)? % 30) as f32;
    let e = (integer(2.0 * b + 4.0 * c + 6.0 * d + n)? % 7) as f32;
    if d == 29.0 && e == 6.0 {
        return Ok((year, 4, 19));
    }
    if d == 28.0 && e == 6.0 {
        return Ok((year, 4, 18));
    }
    let days = integer(22.0 + d + e)?;
    Ok(if days > 31 {
        (year, 4, days - 31)
    } else {
        (year, 3, days)
    })
}

/// Nonlunar schedule calculation. Lunar execution has its own clock/libm owners.
pub fn compute_schedule(schedule: u32, start: i64, end: i64, today: i64) -> Result<(i64, i64)> {
    let current = localtime(today)?;
    let mut date = current;
    match schedule {
        2 | 3 | 5 | 6 | 8 | 9 => {
            date.sec = 0;
            date.min = 0;
            date.hour = 0;
            let remainder = date.mon % 3;
            date.mon += if matches!(schedule, 2 | 5 | 8) {
                (3 - remainder) % 3
            } else {
                (4 - remainder) % 3
            };
            mktime(&mut date)?;
            date.mday = (date.mday - 1 - (date.wday - 1) + 7) % 7 + 1;
            date.mday -= if matches!(schedule, 5 | 6) {
                2
            } else {
                i32::from(matches!(schedule, 8 | 9))
            };
            let first = mktime(&mut date)?;
            date.mday += if matches!(schedule, 2 | 3) { 7 } else { 1 };
            Ok((first, mktime(&mut date)?))
        }
        11 => {
            let mut first = localtime(start)?;
            let mut last = localtime(end)?;
            let year = localtime(today)?.year;
            first.year = year;
            last.year = year;
            Ok((mktime(&mut first)?, mktime(&mut last)?))
        }
        12 => Err(Error::LunarOwner),
        13 => {
            let (_, mon, day) = gauss_easter(date.year.checked_add(1900).ok_or(Error::Overflow)?)?;
            date.sec = 0;
            date.min = 0;
            date.hour = 0;
            date.mon = mon - 1;
            date.mday = day;
            let first = mktime(&mut date)?;
            date.mday += 7;
            Ok((first, mktime(&mut date)?))
        }
        _ => Ok((start, end)),
    }
}
