// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors, upstream GPL-2.0-or-later;
// GNU libstdc++ GCC 12.2 distribution translation; see SOURCE-RNG.md.
use super::{DrawCursor, Error, Mt19937, MtState, Operation, SourceRandom, TraceEvent};

/// Supply captured LP64 time_t/clock_t observations, never sample clocks; Util.cpp:29-32.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedObservations {
    pub unix_seconds: i64,
    pub process_clock: i64,
}

impl SeedObservations {
    /// Preserve size_t conversion followed by seed_seq's modulo 2^32; Util.cpp:31, random.tcc:3241-3257.
    pub fn words(self) -> [u32; 2] {
        [self.unix_seconds as u32, self.process_clock as u32]
    }
}

/// Own one logical thread birth's lazily constructed engine; TSS.h:29-54, Util.cpp:35-39.
#[derive(Debug)]
pub struct ThreadRng {
    thread_birth: u64,
    engine: Option<Mt19937>,
}

impl ThreadRng {
    pub fn new(thread_birth: u64) -> Self {
        Self {
            thread_birth,
            engine: None,
        }
    }

    pub fn thread_birth(&self) -> u64 {
        self.thread_birth
    }

    pub fn initialized(&self) -> bool {
        self.engine.is_some()
    }

    /// Invoke the observation supplier only at the first get; TSS.h:46-54, Util.cpp:29-35.
    pub fn get_or_init(&mut self, seed: impl FnOnce() -> SeedObservations) -> &mut Mt19937 {
        self.engine
            .get_or_insert_with(|| Mt19937::from_seed_seq(&seed().words()))
    }

    /// Retain absence as well as complete initialized state; TSS.h:43-54.
    pub fn into_state(self) -> Option<MtState> {
        self.engine.map(Mt19937::into_state)
    }

    pub fn from_state(thread_birth: u64, state: Option<MtState>) -> Result<Self, Error> {
        Ok(Self {
            thread_birth,
            engine: state.map(Mt19937::from_state).transpose()?,
        })
    }
}

/// Create a fresh local MT, except the no-engine arg1==1 branch; WorldStateExpression.cpp:269-277.
pub fn wse_mersenne_random(arg1: u32, arg2: u32) -> Result<i32, Error> {
    if arg1 == 1 {
        return Ok(1);
    }
    if arg1 == 0 {
        return Err(Error::ZeroModulus);
    }
    let mut engine = Mt19937::from_seed(u64::from(if arg2 == 0 { 1 } else { arg2 }));
    Ok((engine.next_u32() % arg1 + 1) as i32)
}

/// Draw ordinary RANDOM from the calling thread, not a local MT; WorldStateExpression.cpp:65-69.
pub fn wse_random(
    thread: &mut ThreadRng,
    arg1: u32,
    arg2: u32,
    seed: impl FnOnce() -> SeedObservations,
    cursor: &mut DrawCursor,
    sink: &mut impl FnMut(TraceEvent),
) -> Result<i32, Error> {
    let min = arg1.min(arg2) as i32;
    let max = arg1.max(arg2) as i32;
    if min > max {
        return Err(Error::InvalidWseBounds);
    }
    let stream = thread.thread_birth();
    let mut random = SourceRandom::new(stream, thread.get_or_init(seed), cursor, sink)?;
    random.operation(
        Operation::WseRandom,
        [u64::from(arg1), u64::from(arg2), 0],
        |random| random.uniform_i32(min, max),
    )
}
