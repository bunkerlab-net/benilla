// SPDX-License-Identifier: GPL-3.0-or-later
//! Pinned engine arithmetic from explicit seeds/state, not a production RNG cutover.
//! No host entropy, libc RNG, OS thread lookup, gameplay receipts, or persistence.
mod c_rng;
mod local;
mod mt;

pub use c_rng::{CRandState, ProcessCRand};
pub use local::{wse_mersenne_random, wse_random, SeedObservations, ThreadRng};
pub use mt::{seed_seq_generate, Mt19937, MtState, MT_STATE_WORDS};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidMtIndex,
    InvalidCIndex,
    ZeroModulus,
    InvalidWseBounds,
}

#[cfg(test)]
mod tests;
