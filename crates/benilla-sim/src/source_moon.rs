// SPDX-License-Identifier: GPL-3.0-or-later
// Original implementation of moon-spec.md mathematics, without lunar source access.
use crate::source_calendar::Tm;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Overflow,
    FloatConversion,
    UnprovedNarrowing,
    MissingSineBinding,
    MissingSineOrder,
    MissingObservation,
    SurplusObservation,
    ObservationMismatch,
    OwnershipMismatch,
    CandidateHistory,
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CivilDate {
    pub year: i32,
    pub month: i32,
    pub day: i32,
}

/// Preserve the negative-integral discontinuity specified in §Inputs, outputs and arithmetic contract.
pub fn whole_part(value: f64) -> f64 {
    if value >= 0.0 {
        value.floor()
    } else {
        (value - 1.0).ceil()
    }
}

pub fn truncate_i32(value: f64) -> Result<i32> {
    let integral = value.trunc();
    if !integral.is_finite() || !(-2147483648.0..2147483648.0).contains(&integral) {
        return Err(Error::FloatConversion);
    }
    Ok(integral as i32)
}

pub fn truncate_i64(value: f64) -> Result<i64> {
    let integral = value.trunc();
    if !integral.is_finite() || !(-9223372036854775808.0..9223372036854775808.0).contains(&integral)
    {
        return Err(Error::FloatConversion);
    }
    Ok(integral as i64)
}

fn add(a: i64, b: i64) -> Result<i64> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn sub(a: i64, b: i64) -> Result<i64> {
    a.checked_sub(b).ok_or(Error::Overflow)
}
fn mul(a: i64, b: i64) -> Result<i64> {
    a.checked_mul(b).ok_or(Error::Overflow)
}
fn narrow(value: i64) -> Result<i32> {
    i32::try_from(value).map_err(|_| Error::UnprovedNarrowing)
}
fn add32(a: i32, b: i32) -> Result<i32> {
    a.checked_add(b).ok_or(Error::Overflow)
}
fn sub32(a: i32, b: i32) -> Result<i32> {
    a.checked_sub(b).ok_or(Error::Overflow)
}

/// Preserve the original civil-date cutover key and month zero from §Date conversion requirements.
pub fn julian_day(date: CivilDate) -> Result<i64> {
    if date.year == 0 {
        return Ok(0);
    }
    let adjusted = if date.year < 0 {
        add32(date.year, 1)?
    } else {
        date.year
    };
    let (year, month) = if date.month <= 2 {
        (sub32(adjusted, 1)?, add32(date.month, 13)?)
    } else {
        (adjusted, add32(date.month, 1)?)
    };
    let years = (365.25 * f64::from(year)).floor();
    let months = (30.6001 * f64::from(month)).floor();
    let sum = years + months;
    let sum = sum + f64::from(date.day);
    let mut result = truncate_i64(sum + 1720995.0)?;
    let key = add(
        i64::from(date.day),
        mul(
            31,
            add(i64::from(date.month), mul(12, i64::from(date.year))?)?,
        )?,
    )?;
    if key >= 588829 {
        let century = truncate_i32(0.01 * f64::from(year))?;
        let quarter = truncate_i32(0.25 * f64::from(century))?;
        let correction = add32(sub32(2, century)?, quarter)?;
        result = add(result, i64::from(correction))?;
    }
    Ok(result)
}

/// Apply the hybrid cutover and refuse unproved signed narrowing from §Date conversion requirements.
pub fn civil_date(day: i64) -> Result<CivilDate> {
    let corrected = if day >= 2299161 {
        let shifted = sub(day, 1867216)? as f64;
        let adjustment = truncate_i64((shifted - 0.25) / 36524.25)?;
        sub(
            add(add(day, 1)?, adjustment)?,
            truncate_i64(0.25 * adjustment as f64)?,
        )?
    } else {
        day
    };
    let offset = add(corrected, 1524)?;
    let shifted = sub(offset, 2439870)? as f64;
    let year_like = truncate_i64(6680.0 + (shifted - 122.1) / 365.25)?;
    let year_days = add(mul(365, year_like)?, truncate_i64(0.25 * year_like as f64)?)?;
    let remainder = sub(offset, year_days)?;
    let month_like = truncate_i64(remainder as f64 / 30.6001)?;
    let civil_day = narrow(sub(remainder, truncate_i64(30.6001 * month_like as f64)?)?)?;
    let mut month = narrow(sub(month_like, 1)?)?;
    if month > 12 {
        month = sub32(month, 12)?;
    }
    let mut year = narrow(sub(year_like, 4715)?)?;
    if month > 2 {
        year = sub32(year, 1)?;
    }
    if year <= 0 {
        year = sub32(year, 1)?;
    }
    Ok(CivilDate {
        year,
        month,
        day: civil_day,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Anomaly {
    Solar,
    Lunar,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SineOrder {
    SolarFirst,
    LunarFirst,
}

/// Binary64 dispatch is independent of the binary32 service, per §Required refusal and evidence boundaries.
pub trait Sine64 {
    fn owner(&self) -> Result<CallOwner>;
    fn order(&self) -> Result<SineOrder>;
    fn evaluate(&mut self, cycle: i32, anomaly: Anomaly, operand_bits: u64) -> Result<u64>;
    fn finish(&self) -> Result<()>;
}

pub struct UnboundSine;
impl Sine64 for UnboundSine {
    fn owner(&self) -> Result<CallOwner> {
        Err(Error::MissingSineBinding)
    }
    fn order(&self) -> Result<SineOrder> {
        Err(Error::MissingSineBinding)
    }
    fn evaluate(&mut self, _: i32, _: Anomaly, _: u64) -> Result<u64> {
        Err(Error::MissingSineBinding)
    }
    fn finish(&self) -> Result<()> {
        Err(Error::MissingSineBinding)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub enum Evidence {
    Float {
        name: &'static str,
        bits: u64,
    },
    Integer {
        name: &'static str,
        value: i64,
    },
    Sine {
        cycle: i32,
        anomaly: Anomaly,
        operand: u64,
        result: u64,
    },
    Candidate {
        cycle: i32,
        day: i64,
        residual: u64,
        full: u64,
    },
    Calendar(CalendarCall),
}

pub trait Observe {
    fn observe(&mut self, evidence: Evidence) -> Result<()>;
}
impl Observe for Vec<Evidence> {
    fn observe(&mut self, evidence: Evidence) -> Result<()> {
        self.push(evidence);
        Ok(())
    }
}
fn float(trace: &mut impl Observe, name: &'static str, value: f64) -> Result<f64> {
    if !value.is_finite() {
        return Err(Error::FloatConversion);
    }
    trace.observe(Evidence::Float {
        name,
        bits: value.to_bits(),
    })?;
    Ok(value)
}
fn integer(trace: &mut impl Observe, name: &'static str, value: i64) -> Result<i64> {
    trace.observe(Evidence::Integer { name, value })?;
    Ok(value)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Candidate {
    pub cycle: i32,
    pub day: i64,
    pub residual: f64,
    pub full: f64,
}

/// Retain the split day and each rounded operation from §New-moon time model.
pub fn candidate(
    cycle: i32,
    sine: &mut impl Sine64,
    trace: &mut impl Observe,
) -> Result<Candidate> {
    let order = sine.order()?;
    let phase = float(trace, "phase_quotient", 0.0 / 4.0)?;
    let x = float(trace, "cycle_quantity", f64::from(cycle) + phase)?;
    let tau = float(trace, "tau", x / 1236.85)?;
    let square = float(trace, "tau_squared", tau * tau)?;
    let solar_progress = float(trace, "solar_progress", 29.10535608 * x)?;
    let solar = float(trace, "solar_anomaly", 359.2242 + solar_progress)?;
    let lunar_progress = float(trace, "lunar_progress", 385.81691806 * x)?;
    let lunar_linear = float(trace, "lunar_linear", 306.0253 + lunar_progress)?;
    let lunar_quadratic = float(trace, "lunar_quadratic", 0.010730 * square)?;
    let lunar = float(trace, "lunar_anomaly", lunar_linear + lunar_quadratic)?;
    #[expect(
        clippy::approx_constant,
        reason = "§New-moon time model requires shortened pi"
    )]
    let radians = float(trace, "radians_per_degree", 3.1415926535897 / 180.0)?;
    let solar_angle = float(trace, "solar_operand", radians * solar)?;
    let lunar_angle = float(trace, "lunar_operand", radians * lunar)?;
    let mut values = [0.0; 2];
    let ordered = match order {
        SineOrder::SolarFirst => [
            (Anomaly::Solar, solar_angle, 0),
            (Anomaly::Lunar, lunar_angle, 1),
        ],
        SineOrder::LunarFirst => [
            (Anomaly::Lunar, lunar_angle, 1),
            (Anomaly::Solar, solar_angle, 0),
        ],
    };
    for (anomaly, angle, slot) in ordered {
        let bits = sine.evaluate(cycle, anomaly, angle.to_bits())?;
        let value = f64::from_bits(bits);
        if !value.is_finite() {
            return Err(Error::FloatConversion);
        }
        trace.observe(Evidence::Sine {
            cycle,
            anomaly,
            operand: angle.to_bits(),
            result: bits,
        })?;
        values[slot] = value;
    }
    let mean = float(trace, "mean_residual", 1.53058868 * x)?;
    let secular_product = float(trace, "secular_product", 1.55e-7 * tau)?;
    let secular_coefficient = float(trace, "secular_coefficient", 1.78e-4 - secular_product)?;
    let secular = float(trace, "secular", secular_coefficient * square)?;
    let amplitude_product = float(trace, "amplitude_product", 3.93e-4 * tau)?;
    let amplitude = float(trace, "amplitude", 0.1734 - amplitude_product)?;
    let solar_correction = float(trace, "solar_correction", amplitude * values[0])?;
    let lunar_correction = float(trace, "lunar_correction", 0.4068 * values[1])?;
    let periodic = float(trace, "periodic", solar_correction - lunar_correction)?;
    let epoch_mean = float(trace, "epoch_mean", 0.75933 + mean)?;
    let nonperiodic = float(trace, "nonperiodic", epoch_mean + secular)?;
    let epsilon = float(trace, "epsilon", nonperiodic + periodic)?;
    let whole = float(trace, "epsilon_whole", whole_part(epsilon))?;
    let q = truncate_i32(whole)?;
    integer(trace, "residual_integer", i64::from(q))?;
    let base = integer(
        trace,
        "base",
        add(add(2415020, mul(28, i64::from(cycle))?)?, 0)?,
    )?;
    let day = integer(trace, "candidate_day", add(base, i64::from(q))?)?;
    let residual = float(trace, "candidate_residual", epsilon - f64::from(q))?;
    let full = float(trace, "candidate_full", day as f64 + residual)?;
    trace.observe(Evidence::Candidate {
        cycle,
        day,
        residual: residual.to_bits(),
        full: full.to_bits(),
    })?;
    Ok(Candidate {
        cycle,
        day,
        residual,
        full,
    })
}

/// Re-evaluate revisited indices using the specification's clarified two-direction recurrence.
pub fn following_moon(
    anchor: CivilDate,
    sine: &mut impl Sine64,
    trace: &mut impl Observe,
) -> Result<Candidate> {
    let target = integer(trace, "anchor_day", julian_day(anchor)?)?;
    let year_delta = sub32(anchor.year, 1900)?;
    let month_offset = float(
        trace,
        "estimate_month_offset",
        f64::from(anchor.month) - 0.5,
    )?;
    let month_fraction = float(trace, "estimate_month_fraction", month_offset / 12.0)?;
    let estimate_year = float(
        trace,
        "estimate_year",
        f64::from(year_delta) + month_fraction,
    )?;
    let estimate = float(trace, "estimate", 12.37 * estimate_year)?;
    let mut index = truncate_i32(estimate)?;
    integer(trace, "initial_cycle", i64::from(index))?;
    let mut selected = candidate(index, sine, trace)?;
    while target < selected.day {
        index = sub32(index, 1)?;
        let previous = selected.day;
        selected = candidate(index, sine, trace)?;
        if selected.day >= previous {
            return Err(Error::CandidateHistory);
        }
    }
    while target > selected.day {
        index = add32(index, 1)?;
        let previous = selected.day;
        selected = candidate(index, sine, trace)?;
        if selected.day <= previous {
            return Err(Error::CandidateHistory);
        }
    }
    Ok(selected)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallOwner {
    pub thread: u64,
    pub call: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SineSample {
    pub owner: CallOwner,
    pub cycle: i32,
    pub anomaly: Anomaly,
    pub operand: u64,
    pub result: u64,
}

/// Fixture injection checks every operand in order; it does not establish historical dispatch.
pub struct RecordedSine<'a> {
    owner: CallOwner,
    order: Option<SineOrder>,
    samples: &'a [SineSample],
    cursor: usize,
}
impl<'a> RecordedSine<'a> {
    pub fn new(owner: CallOwner, order: Option<SineOrder>, samples: &'a [SineSample]) -> Self {
        Self {
            owner,
            order,
            samples,
            cursor: 0,
        }
    }
}
impl Sine64 for RecordedSine<'_> {
    fn owner(&self) -> Result<CallOwner> {
        Ok(self.owner)
    }
    fn order(&self) -> Result<SineOrder> {
        self.order.ok_or(Error::MissingSineOrder)
    }
    fn evaluate(&mut self, cycle: i32, anomaly: Anomaly, operand_bits: u64) -> Result<u64> {
        let sample = self
            .samples
            .get(self.cursor)
            .ok_or(Error::MissingObservation)?;
        if sample.owner != self.owner {
            return Err(Error::OwnershipMismatch);
        }
        if (sample.cycle, sample.anomaly, sample.operand) != (cycle, anomaly, operand_bits) {
            return Err(Error::ObservationMismatch);
        }
        self.cursor += 1;
        Ok(sample.result)
    }
    fn finish(&self) -> Result<()> {
        if self.cursor == self.samples.len() {
            Ok(())
        } else {
            Err(Error::SurplusObservation)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarInput {
    Retained(Tm),
    /// Only the explicitly written 03:00:00, civil date and DST=-1 fields exist, per §Clock-dependent civil date.
    AnchorProbe(CivilDate),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalendarCall {
    Now {
        seconds: i64,
    },
    Local {
        seconds: i64,
        result: Tm,
    },
    Epoch {
        input: CalendarInput,
        seconds: i64,
        result: Tm,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CalendarSample {
    pub owner: CallOwner,
    pub operation: CalendarCall,
}

pub trait Calendar {
    fn owner(&self) -> CallOwner;
    fn now(&mut self) -> Result<i64>;
    fn local(&mut self, seconds: i64) -> Result<Tm>;
    fn epoch(&mut self, input: CalendarInput) -> Result<(i64, Tm)>;
    fn finish(&self) -> Result<()>;
}

/// Replay observations from the configured calendar environment without inventing unwritten probe fields.
pub struct RecordedCalendar<'a> {
    owner: CallOwner,
    samples: &'a [CalendarSample],
    cursor: usize,
}
impl<'a> RecordedCalendar<'a> {
    pub fn new(owner: CallOwner, samples: &'a [CalendarSample]) -> Self {
        Self {
            owner,
            samples,
            cursor: 0,
        }
    }
    fn take(&mut self) -> Result<CalendarCall> {
        let sample = self
            .samples
            .get(self.cursor)
            .ok_or(Error::MissingObservation)?;
        if sample.owner != self.owner {
            return Err(Error::OwnershipMismatch);
        }
        self.cursor += 1;
        Ok(sample.operation)
    }
}
impl Calendar for RecordedCalendar<'_> {
    fn owner(&self) -> CallOwner {
        self.owner
    }
    fn now(&mut self) -> Result<i64> {
        match self.take()? {
            CalendarCall::Now { seconds } => Ok(seconds),
            _ => Err(Error::ObservationMismatch),
        }
    }
    fn local(&mut self, seconds: i64) -> Result<Tm> {
        match self.take()? {
            CalendarCall::Local {
                seconds: expected,
                result,
            } if expected == seconds => Ok(result),
            _ => Err(Error::ObservationMismatch),
        }
    }
    fn epoch(&mut self, input: CalendarInput) -> Result<(i64, Tm)> {
        match self.take()? {
            CalendarCall::Epoch {
                input: expected,
                seconds,
                result,
            } if expected == input => Ok((seconds, result)),
            _ => Err(Error::ObservationMismatch),
        }
    }
    fn finish(&self) -> Result<()> {
        if self.cursor == self.samples.len() {
            Ok(())
        } else {
            Err(Error::SurplusObservation)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MoonDate {
    pub civil: CivilDate,
    pub hour: i32,
    pub minute: i32,
}

/// Keep the DST subtraction and addition separately rounded, per §Clock-dependent civil date.
pub fn adjust_selected(
    selected: Candidate,
    current_dst: bool,
    anchor_dst: bool,
    trace: &mut impl Observe,
) -> Result<MoonDate> {
    let value = begin_adjustment(selected, current_dst, trace)?;
    finish_adjustment(value, anchor_dst, trace)
}

fn begin_adjustment(
    selected: Candidate,
    current_dst: bool,
    trace: &mut impl Observe,
) -> Result<f64> {
    let mut value = float(trace, "noon_shift", selected.full + 0.5)?;
    if current_dst {
        let adjustment = float(trace, "dst_subtract_fraction", 1.0 / 24.0)?;
        value = float(trace, "current_dst_subtraction", value - adjustment)?;
    }
    integer(trace, "before_probe_day", truncate_i64(value.floor())?)?;
    Ok(value)
}

fn finish_adjustment(
    mut value: f64,
    anchor_dst: bool,
    trace: &mut impl Observe,
) -> Result<MoonDate> {
    if anchor_dst {
        let adjustment = float(trace, "dst_add_fraction", 1.0 / 24.0)?;
        value = float(trace, "anchor_dst_addition", value + adjustment)?;
    }
    let day = integer(trace, "final_day", truncate_i64(value.floor())?)?;
    let fraction = float(trace, "day_fraction", value - value.floor())?;
    let hours = float(trace, "hours", fraction * 24.0)?;
    let hour = truncate_i32(whole_part(hours))?;
    integer(trace, "hour", i64::from(hour))?;
    let hour_fraction = float(trace, "hour_fraction", hours - f64::from(hour))?;
    let minutes = float(trace, "minutes", hour_fraction * 60.0)?;
    let minute = truncate_i32(minutes.floor())?;
    integer(trace, "minute", i64::from(minute))?;
    Ok(MoonDate {
        civil: civil_date(day)?,
        hour,
        minute,
    })
}

fn observe_now(calendar: &mut impl Calendar, trace: &mut impl Observe) -> Result<i64> {
    let seconds = calendar.now()?;
    trace.observe(Evidence::Calendar(CalendarCall::Now { seconds }))?;
    Ok(seconds)
}

fn observe_local(
    calendar: &mut impl Calendar,
    seconds: i64,
    trace: &mut impl Observe,
) -> Result<Tm> {
    let result = calendar.local(seconds)?;
    trace.observe(Evidence::Calendar(CalendarCall::Local { seconds, result }))?;
    Ok(result)
}

fn observe_epoch(
    calendar: &mut impl Calendar,
    input: CalendarInput,
    trace: &mut impl Observe,
) -> Result<(i64, Tm)> {
    let (seconds, result) = calendar.epoch(input)?;
    trace.observe(Evidence::Calendar(CalendarCall::Epoch {
        input,
        seconds,
        result,
    }))?;
    Ok((seconds, result))
}

fn lunar_date(
    tm: &mut Tm,
    calendar: &mut impl Calendar,
    sine: &mut impl Sine64,
    trace: &mut impl Observe,
) -> Result<()> {
    let anchor = CivilDate {
        year: tm.year,
        month: tm.mon,
        day: tm.mday,
    };
    let selected = following_moon(anchor, sine, trace)?;
    let now = observe_now(calendar, trace)?;
    let current_dst = observe_local(calendar, now, trace)?.isdst != 0;
    let value = begin_adjustment(selected, current_dst, trace)?;
    let unused_now = observe_now(calendar, trace)?;
    observe_local(calendar, unused_now, trace)?;
    let probe = civil_date(julian_day(anchor)?)?;
    let (probe_seconds, _) = observe_epoch(calendar, CalendarInput::AnchorProbe(probe), trace)?;
    let anchor_dst = observe_local(calendar, probe_seconds, trace)?.isdst != 0;
    let date = finish_adjustment(value, anchor_dst, trace)?;
    tm.year = date.civil.year;
    tm.mon = date.civil.month;
    tm.mday = date.civil.day;
    tm.hour = date.hour;
    tm.min = date.minute;
    Ok(())
}

/// Preserve month mismatches, retained seconds and calendar-day duration from §Final event calendar transformation.
pub fn compute_event(
    event_seconds: i64,
    calendar: &mut impl Calendar,
    sine: &mut impl Sine64,
    trace: &mut impl Observe,
) -> Result<(i64, i64)> {
    sine.order()?;
    if sine.owner()? != calendar.owner() {
        return Err(Error::OwnershipMismatch);
    }
    let mut date = observe_local(calendar, event_seconds, trace)?;
    date.year = add32(sub32(date.year, 1)?, 1900)?;
    date.mon = 12;
    date.mday = 21;
    date.hour = 0;
    date.min = 0;
    lunar_date(&mut date, calendar, sine, trace)?;
    date.mday = add32(date.mday, 1)?;
    date.year = sub32(date.year, 1900)?;
    let (seconds, _) = observe_epoch(calendar, CalendarInput::Retained(date), trace)?;
    date = observe_local(calendar, seconds, trace)?;
    date.year = add32(date.year, 1900)?;
    lunar_date(&mut date, calendar, sine, trace)?;
    date.mon = sub32(date.mon, 1)?;
    date.year = sub32(date.year, 1900)?;
    date.hour = add32(date.hour, 8)?;
    date.min = 0;
    let (seconds, _) = observe_epoch(calendar, CalendarInput::Retained(date), trace)?;
    date = observe_local(calendar, seconds, trace)?;
    date.hour = 0;
    let (start, normalized) = observe_epoch(calendar, CalendarInput::Retained(date), trace)?;
    date = normalized;
    date.mday = add32(date.mday, 21)?;
    let (end, _) = observe_epoch(calendar, CalendarInput::Retained(date), trace)?;
    calendar.finish()?;
    sine.finish()?;
    Ok((start, end))
}
