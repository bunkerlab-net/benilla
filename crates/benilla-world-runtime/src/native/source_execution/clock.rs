// SPDX-License-Identifier: GPL-3.0-or-later
//! Recorded observations, never host time or fixed-frame substitutes.
use super::*;
use benilla_sim::source_time::WorldTimer;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockKind {
    RelativeSteadyMilliseconds,
    WallSeconds,
    MonotonicNanoseconds,
    ProcessCpuTicks,
}
impl ClockKind {
    pub fn name(self) -> &'static str {
        match self {
            Self::RelativeSteadyMilliseconds => "relative_steady_milliseconds",
            Self::WallSeconds => "wall_seconds",
            Self::MonotonicNanoseconds => "monotonic_nanoseconds",
            Self::ProcessCpuTicks => "process_cpu_ticks",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ClockSample {
    kind: ClockKind,
    value: i64,
}
impl ClockSample {
    // Decode the LP64 observation at its input owner; Util.cpp:29-35,60-64.
    pub fn consume(
        inputs: &mut VerifiedExecutionInputs,
        stream: &str,
        owner: &InputOwner,
        kind: ClockKind,
    ) -> Result<Self> {
        Self::decode(inputs.consume(stream, owner)?, kind)
    }
    fn decode(input: AdmissibleInput, kind: ClockKind) -> Result<Self> {
        let AdmissibleInput::ClockObservation { clock, value } = input else {
            return Err(refuse("execution.clock_family"));
        };
        require(
            clock == kind.name() && value.ty == ScalarType::I64,
            "execution.clock_kind_width",
        )?;
        let bytes: [u8; 8] = value
            .bytes
            .try_into()
            .map_err(|_| refuse("execution.clock_width"))?;
        Ok(Self {
            kind,
            value: i64::from_le_bytes(bytes),
        })
    }
    pub fn value(self) -> i64 {
        self.value
    }
    pub fn kind(self) -> ClockKind {
        self.kind
    }
    #[cfg(test)]
    pub(super) fn authored(kind: ClockKind, value: i64) -> Self {
        Self { kind, value }
    }
}
#[derive(Default)]
pub struct RunnableClock {
    timer: WorldTimer,
    initialized: bool,
}
impl RunnableClock {
    // Discard the initial diff before entering the update loop; WorldRunnable.cpp:76-84.
    pub fn initialize(&mut self, sample: ClockSample) -> Result<()> {
        require(
            !self.initialized && sample.kind == ClockKind::RelativeSteadyMilliseconds,
            "execution.clock_initialization",
        )?;
        self.timer.tick(sample.value);
        self.initialized = true;
        Ok(())
    }
    pub fn tick(&mut self, sample: ClockSample) -> Result<u32> {
        require(
            self.initialized && sample.kind == ClockKind::RelativeSteadyMilliseconds,
            "execution.clock_tick_owner",
        )?;
        Ok(self.timer.tick(sample.value))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CalendarOwner {
    World,
    Map(MapIdentity),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Calendar {
    owner: CalendarOwner,
    seconds: i64,
}
impl Calendar {
    pub fn new(owner: CalendarOwner, seconds: i64) -> Self {
        Self { owner, seconds }
    }
    pub fn seconds(self) -> i64 {
        self.seconds
    }
    // World and each map own distinct time(nullptr) observations; World.cpp:2055-2057, Map.cpp:702.
    pub fn update(&mut self, owner: CalendarOwner, sample: ClockSample) -> Result<u32> {
        require(
            owner == self.owner && sample.kind == ClockKind::WallSeconds,
            "execution.calendar_owner",
        )?;
        let elapsed = sample
            .value
            .checked_sub(self.seconds)
            .ok_or_else(|| refuse("execution.calendar_overflow"))? as u32;
        self.seconds = sample.value;
        Ok(elapsed)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn independent_samples_never_advance_the_world_diff() {
        let mut clock = RunnableClock::default();
        clock
            .initialize(ClockSample::authored(
                ClockKind::RelativeSteadyMilliseconds,
                100,
            ))
            .unwrap();
        assert!(clock
            .tick(ClockSample::authored(ClockKind::ProcessCpuTicks, 1000))
            .is_err());
        assert!(clock
            .tick(ClockSample::authored(ClockKind::MonotonicNanoseconds, 1000))
            .is_err());
        assert_eq!(
            clock
                .tick(ClockSample::authored(
                    ClockKind::RelativeSteadyMilliseconds,
                    150
                ))
                .unwrap(),
            50
        );
        let mut calendar = Calendar::new(CalendarOwner::World, 200);
        assert_eq!(
            calendar
                .update(
                    CalendarOwner::World,
                    ClockSample::authored(ClockKind::WallSeconds, 199)
                )
                .unwrap(),
            u32::MAX
        );
        assert_eq!(
            clock
                .tick(ClockSample::authored(
                    ClockKind::RelativeSteadyMilliseconds,
                    199
                ))
                .unwrap(),
            49
        );
    }
    #[test]
    fn sample_kind_and_width_are_not_interchangeable() {
        let input = |ty, bytes| AdmissibleInput::ClockObservation {
            clock: "wall_seconds".into(),
            value: TypedBytes { ty, bytes },
        };
        assert!(
            ClockSample::decode(input(ScalarType::U64, vec![0; 8]), ClockKind::WallSeconds)
                .is_err()
        );
        assert!(
            ClockSample::decode(input(ScalarType::I64, vec![0; 4]), ClockKind::WallSeconds)
                .is_err()
        );
        assert!(ClockSample::decode(
            input(ScalarType::I64, vec![0; 8]),
            ClockKind::ProcessCpuTicks
        )
        .is_err());
    }
}
