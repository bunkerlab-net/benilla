// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;
use std::collections::VecDeque;
struct Calls {
    rolls: VecDeque<(u32, u32, u32)>,
    floats: VecDeque<f32>,
    days: usize,
}
impl Calls {
    fn new(rolls: &[(u32, u32, u32)], floats: &[f32]) -> Self {
        Self {
            rolls: rolls.iter().copied().collect(),
            floats: floats.iter().copied().collect(),
            days: 0,
        }
    }
    fn exhausted(&self) {
        assert!(self.rolls.is_empty() && self.floats.is_empty());
    }
}
impl Random for Calls {
    type Error = ();
    fn urand(&mut self, low: u32, high: u32) -> Result<u32, ()> {
        let (l, h, value) = self.rolls.pop_front().expect("unexpected draw");
        assert_eq!((low, high), (l, h));
        Ok(value)
    }
    fn rand_norm_f(&mut self) -> Result<f32, ()> {
        Ok(self.floats.pop_front().expect("unexpected float"))
    }
}
impl Context for Calls {
    fn year_day(&mut self) -> Result<u32, ()> {
        self.days += 1;
        Ok(78)
    }
}
fn wet(grade: f32) -> Weather {
    Weather {
        kind: RAIN,
        grade_bits: grade.to_bits(),
        permanent: false,
    }
}
const RAIN_ONLY: Chances = Chances([[100, 0, 0]; 4]);

#[test]
fn all_localtime_day_indices_use_source_seasons() {
    // Weather.cpp:112 wraps spring to day 77, summer to 168, fall to 259.
    for day in 0..366 {
        let expected = match day {
            0..=76 => 3,
            77..=167 => 0,
            168..=258 => 1,
            259..=349 => 2,
            _ => 3,
        };
        assert_eq!(season(day), expected, "day {day}");
    }
}
#[test]
fn corrections_preserve_each_chance_and_sums_above_one_hundred() {
    let chance = Chances::corrected([
        [101, 100, 500],
        [0, 99, 1],
        [100, 100, 100],
        [u32::MAX, 0, 0],
    ]);
    assert_eq!(
        chance.0,
        [[25, 100, 25], [0, 99, 1], [100, 100, 100], [25, 0, 0]]
    );
    let mut w = Weather::default();
    let mut calls = Calls::new(&[(0, 99, 60), (1, 100, 100)], &[0.0]);
    w.regenerate(Some(&chance), &mut calls).unwrap();
    assert_eq!(w.kind, SNOW);
    calls.exhausted();
}
#[test]
fn permanent_missing_chances_and_thirty_percent_skip_suffix_draws_and_calendar() {
    let mut calls = Calls::new(&[], &[]);
    let mut w = wet(0.5);
    w.permanent = true;
    assert!(!w.regenerate(Some(&RAIN_ONLY), &mut calls).unwrap());
    w.permanent = false;
    assert!(w.regenerate(None, &mut calls).unwrap());
    assert!(!w.regenerate(None, &mut calls).unwrap());
    assert_eq!(w, Weather::default());
    let mut calls = Calls::new(&[(0, 99, 29)], &[]);
    assert!(!w.regenerate(Some(&RAIN_ONLY), &mut calls).unwrap());
    calls.exhausted();
    assert_eq!(calls.days, 0);
}
#[test]
fn improvement_is_not_a_worsening_and_early_returns_do_not_normalize() {
    let mut w = wet(0.5);
    let mut c = Calls::new(&[(0, 99, 30)], &[]);
    assert!(w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap());
    assert_eq!(w.grade_bits, 0x3e2aaaaa);
    c.exhausted();
    assert_eq!(c.days, 1);
    let mut w = wet(0.9);
    let mut c = Calls::new(&[(0, 99, 60)], &[]);
    assert!(w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap());
    assert!(w.grade() > 1.0);
    c.exhausted();
    w.packet();
    assert_eq!(w.grade_bits, 0x3f7ff972);
}
#[test]
fn light_improvement_can_reselect_same_type_and_clear_weather() {
    let mut w = wet(0.1);
    let mut c = Calls::new(&[(0, 99, 59), (1, 100, 1)], &[0.0]);
    assert!(w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap());
    assert_eq!((w.kind, w.grade_bits), (RAIN, 0));
    c.exhausted();
    let mut c = Calls::new(&[(0, 99, 30), (1, 100, 100)], &[]);
    assert!(w.regenerate(Some(&Chances([[0; 3]; 4])), &mut c).unwrap());
    assert_eq!(w, Weather::default());
    c.exhausted();
}
#[test]
fn radical_light_to_heavy_and_heavy_to_light_use_no_float_draw() {
    let mut w = wet(0.1);
    let mut c = Calls::new(&[(0, 99, 90)], &[]);
    assert!(w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap());
    assert_eq!(w.grade_bits, 0x3f7ff972);
    c.exhausted();
    let mut w = wet(1.0);
    let mut c = Calls::new(&[(0, 99, 99), (0, 99, 49)], &[]);
    assert!(w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap());
    assert_eq!(w.grade_bits, 0x3eaaaaaa);
    c.exhausted();
}
#[test]
fn radical_medium_and_heavy_reselection_preserve_branch_draws() {
    for grade in [0.33333334, 0.6666667, 0.8] {
        for (severity, expected) in [(49, 0x3eaab368), (50, 0x3f2aacda)] {
            let mut rolls = vec![(0, 99, 90)];
            if grade > 0.6666667 {
                rolls.push((0, 99, 50));
            }
            rolls.extend([(1, 100, 1), (0, 99, severity)]);
            let mut c = Calls::new(&rolls, &[0.0]);
            let mut w = wet(grade);
            w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap();
            assert_eq!(w.grade_bits, expected);
            c.exhausted();
        }
    }
}
#[test]
fn normalization_replaces_only_outside_boundaries_and_preserves_negative_zero_and_nan() {
    for (input, expected) in [
        (1.0, 0x3f7ff972),
        (-1.0, 0x38d1b717),
        (0.0, 0),
        (f32::NEG_INFINITY, 0x38d1b717),
        (f32::INFINITY, 0x3f7ff972),
        (-0.0, 0x80000000),
    ] {
        let mut w = wet(input);
        w.normalize();
        assert_eq!(w.grade_bits, expected);
    }
    let mut w = wet(f32::from_bits(0x7fc01234));
    w.normalize();
    assert_eq!(w.grade_bits, 0x7fc01234);
}
#[test]
fn rain_sound_threshold_is_not_shared_with_snow_storm_or_logging_state() {
    for (kind, sounds, states) in [
        (RAIN, [0, 8533, 8534, 8535], [0, 3, 4, 5]),
        (SNOW, [8536, 8536, 8537, 8538], [0, 6, 7, 8]),
        (STORM, [8556, 8556, 8557, 8558], [0, 22, 41, 42]),
    ] {
        for ((grade, sound), state) in [0.0, 0.27, 0.4, 0.7].into_iter().zip(sounds).zip(states) {
            let w = Weather { kind, ..wet(grade) };
            assert_eq!(w.sound(), sound);
            assert_eq!(w.state(), state);
        }
        let w = Weather {
            kind,
            ..wet(f32::NAN)
        };
        assert_eq!(w.sound(), sounds[3]);
        assert_eq!(w.state(), states[3]);
    }
}
#[test]
fn set_same_values_mutates_permanent_without_normalization_or_packet() {
    let mut w = wet(1.2);
    assert!(!w.set(RAIN, 1.2, true));
    assert!(w.permanent);
    assert_eq!(w.grade(), 1.2);
    assert!(!w.set(RAIN, 1.2, false));
    assert!(!w.permanent);
    assert!(w.set(SNOW, 1.0, true));
    assert_eq!(
        w.packet(),
        [2, 0, 0, 0, 0x72, 0xf9, 0x7f, 0x3f, 0x5a, 0x21, 0, 0, 0]
    );
}

#[test]
fn each_selection_threshold_and_exact_improvement_boundary_preserve_draws() {
    let chances = Chances([[20, 30, 40]; 4]);
    for (draw, kind) in [
        (1, RAIN),
        (20, RAIN),
        (21, SNOW),
        (50, SNOW),
        (51, STORM),
        (90, STORM),
        (91, FINE),
        (100, FINE),
    ] {
        let mut c = Calls::new(
            &[(0, 99, 60), (1, 100, draw)],
            if kind == FINE { &[] } else { &[0.0] },
        );
        let mut w = Weather::default();
        w.regenerate(Some(&chances), &mut c).unwrap();
        assert_eq!(w.kind, kind);
        assert_eq!(w.grade_bits, 0);
        c.exhausted();
    }
    let mut w = wet(0.33333334);
    let mut c = Calls::new(&[(0, 99, 30)], &[]);
    assert!(w.regenerate(Some(&RAIN_ONLY), &mut c).unwrap());
    assert_eq!((w.kind, w.grade_bits), (RAIN, 0));
    c.exhausted();
}
