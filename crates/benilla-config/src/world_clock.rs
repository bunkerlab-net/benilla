//! Fork exploration clock: explicit IANA timezone, never the operating system's local zone.
use chrono::{Datelike, TimeZone, Timelike, Utc};
use serde::{Deserialize, Deserializer};
use sha2::{Digest, Sha256};
use std::time::{SystemTime, UNIX_EPOCH};

/// Validated, bundled timezone rules shared by every participant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldTimezone(chrono_tz::Tz);

impl Default for WorldTimezone {
    fn default() -> Self {
        Self(chrono_tz::UTC)
    }
}
impl<'de> Deserialize<'de> for WorldTimezone {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let name = String::deserialize(deserializer)?;
        name.parse().map(Self).map_err(|_| {
            serde::de::Error::custom("timezone must be a recognized IANA timezone name")
        })
    }
}

/// Minute-resolution input to the existing session clock and stock GetGameTime API.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WorldClockSample {
    pub hours: u8,
    pub minutes: u8,
    /// Calendar days since the common era; unlike packed realm dates, continuous at month ends.
    pub day_serial: u32,
}
impl WorldClockSample {
    /// Gregorian calendar components for the stock client clock; days/months are zero based.
    pub fn calendar(self) -> Option<(i32, i32, i32, i32)> {
        let date = chrono::NaiveDate::from_num_days_from_ce_opt(self.day_serial.try_into().ok()?)?;
        Some((
            date.year() - 2000,
            date.month0() as i32,
            date.day0() as i32,
            date.weekday().num_days_from_sunday() as i32,
        ))
    }
}
impl WorldTimezone {
    pub fn name(self) -> &'static str {
        self.0.name()
    }

    /// Include the zone and bundled rules version in admission, without changing gameplay rules.
    /// Exact names must match, including aliases, so configuration disagreement fails closed.
    pub fn exploration_fingerprint(self, rules: crate::Ruleset) -> [u8; 32] {
        let mut hash = Sha256::new();
        hash.update(b"benilla-exploration-clock-v1\0");
        hash.update(rules.fingerprint());
        hash.update(chrono_tz::IANA_TZDB_VERSION.as_bytes());
        hash.update([0]);
        hash.update(self.name().as_bytes());
        hash.finalize().into()
    }

    /// Read UTC wall time, then apply only the configured zone. Invalid OS dates fail closed.
    pub fn now(self) -> Option<WorldClockSample> {
        let seconds = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
        self.at_unix(i64::try_from(seconds).ok()?)
    }

    pub fn at_unix(self, seconds: i64) -> Option<WorldClockSample> {
        let utc = Utc.timestamp_opt(seconds, 0).single()?;
        let local = utc.with_timezone(&self.0);
        Some(WorldClockSample {
            hours: local.hour() as u8,
            minutes: local.minute() as u8,
            day_serial: local.num_days_from_ce().try_into().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn zone(name: &str) -> WorldTimezone {
        WorldTimezone(name.parse().unwrap())
    }
    fn sample(zone: WorldTimezone, date: &str) -> WorldClockSample {
        zone.at_unix(
            chrono::DateTime::parse_from_rfc3339(date)
                .unwrap()
                .timestamp(),
        )
        .unwrap()
    }
    #[test]
    fn stock_calendar_uses_zero_based_month_day_and_sunday_weekday() {
        let leap = sample(zone("UTC"), "2028-02-29T23:59:00Z");
        assert_eq!(leap.calendar(), Some((28, 1, 28, 2)));
        let march = sample(zone("UTC"), "2028-03-01T00:00:00Z");
        assert_eq!(march.calendar(), Some((28, 2, 0, 3)));
        let invalid = WorldClockSample {
            hours: 0,
            minutes: 0,
            day_serial: u32::MAX,
        };
        assert!(invalid.calendar().is_none());
    }
    #[test]
    fn utc_is_default_and_johannesburg_wraps_calendar_dates() {
        assert_eq!(WorldTimezone::default().name(), "UTC");
        let utc = sample(WorldTimezone::default(), "2026-12-31T23:59:00Z");
        let local = sample(zone("Africa/Johannesburg"), "2026-12-31T23:59:00Z");
        assert_eq!((utc.hours, utc.minutes), (23, 59));
        assert_eq!((local.hours, local.minutes), (1, 59));
        assert_eq!(local.day_serial, utc.day_serial + 1);
        let feb = sample(zone("UTC"), "2028-02-29T23:59:00Z");
        let march = sample(zone("UTC"), "2028-03-01T00:00:00Z");
        assert_eq!(march.day_serial, feb.day_serial + 1);
    }
    #[test]
    fn named_zone_follows_both_dst_transitions() {
        let ny = zone("America/New_York");
        for (instant, expected) in [
            ("2026-03-08T06:59:00Z", (1, 59)),
            ("2026-03-08T07:00:00Z", (3, 0)),
            ("2026-11-01T05:59:00Z", (1, 59)),
            ("2026-11-01T06:00:00Z", (1, 0)),
        ] {
            let got = sample(ny, instant);
            assert_eq!((got.hours, got.minutes), expected);
        }
    }
    #[test]
    fn compatibility_includes_zone_and_rules_and_excludes_old_noon_clients() {
        let rules = crate::Ruleset::default();
        let utc = WorldTimezone::default().exploration_fingerprint(rules);
        assert_eq!(utc, zone("UTC").exploration_fingerprint(rules));
        assert_ne!(
            utc,
            zone("Africa/Johannesburg").exploration_fingerprint(rules)
        );
        assert_ne!(utc, rules.fingerprint());
        assert_ne!(
            utc,
            zone("UTC").exploration_fingerprint(crate::Ruleset {
                xp_milli: 2000,
                drop_milli: 1000
            })
        );
    }
    #[test]
    fn polling_is_stable_within_a_minute_and_detects_clock_corrections() {
        let zone = WorldTimezone::default();
        let initial = sample(zone, "2026-10-15T18:20:37Z");
        assert_eq!(initial, sample(zone, "2026-10-15T18:20:59Z"));
        assert_ne!(initial, sample(zone, "2026-10-15T18:21:00Z"));
        assert_ne!(initial, sample(zone, "2026-10-15T18:19:59Z"));
        assert_ne!(initial, sample(zone, "2026-10-16T18:20:37Z"));
    }
    #[test]
    fn out_of_range_dates_fail_closed() {
        assert_eq!(zone("UTC").at_unix(i64::MAX), None);
    }
}
