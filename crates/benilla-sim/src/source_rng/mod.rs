// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned engine arithmetic from explicit seeds/state, not a production RNG cutover.
//! No host entropy, libc RNG, OS thread lookup, gameplay receipts, or persistence.
mod c_rng;
mod distributions;
mod local;
mod mt;
mod shuffle;
mod trace;
mod wrappers;

pub use c_rng::{CRandState, ProcessCRand};
pub use distributions::double_to_float_bits;
pub use local::{wse_mersenne_random, wse_random, SeedObservations, ThreadRng};
pub use mt::{seed_seq_generate, Mt19937, MtState, MT_STATE_WORDS};
pub use trace::{
    DrawCursor, EventKind, NoDraw, Operation, RawGenerator, Rejection, SourceRandom, TraceEvent,
    Value,
};
pub use wrappers::{chance_u, Die};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidMtIndex,
    InvalidCIndex,
    ZeroModulus,
    InvalidWseBounds,
    InvalidGenerator,
    InvalidGeneratorWord,
    InvalidRange,
    InvalidRealRange,
    UnreachedRealGenerator,
    UndefinedChanceConversion,
    InvalidDieSides,
    EmptySelection,
    CursorOverflow,
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod distribution_tests;
#[cfg(test)]
mod semantic_projection;
