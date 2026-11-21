// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from glibc 2.36 random.c/random_r.c, copyright FSF 1995-2022
// and Regents of the University of California 1983; see licenses/SOURCE-RNG-NOTICES.txt.
use super::Error;

/// Preserve TYPE_3's 31 words and rear pointer; glibc random.c:138-193.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CRandState {
    pub words: [u32; 31],
    pub rear: u32,
}

/// One caller-owned process stream, never host libc state; glibc random.c:208-219,285-305.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProcessCRand {
    state: CRandState,
}

impl Default for ProcessCRand {
    /// Use the literal pre-srand state instead of recomputing it; glibc random.c:138-158.
    fn default() -> Self {
        let signed: [i32; 31] = [
            -1726662223,
            379960547,
            1735697613,
            1040273694,
            1313901226,
            1627687941,
            -179304937,
            -2073333483,
            1780058412,
            -1989503057,
            -615974602,
            344556628,
            939512070,
            -1249116260,
            1507946756,
            -812545463,
            154635395,
            1388815473,
            -1926676823,
            525320961,
            -1009028674,
            968117788,
            -123449607,
            1284210865,
            435012392,
            -2017506339,
            -911064859,
            -370259173,
            1132637927,
            1398500161,
            -205601318,
        ];
        Self {
            state: CRandState {
                words: signed.map(|word| word as u32),
                rear: 0,
            },
        }
    }
}

impl ProcessCRand {
    /// Reproduce srand's uint32 seed and TYPE_3 warmup; glibc random_r.c:176-223.
    pub fn from_seed(seed: u32) -> Self {
        let mut engine = Self {
            state: CRandState {
                words: [0; 31],
                rear: 0,
            },
        };
        engine.srand(seed);
        engine
    }

    /// Replace the process stream, mapping zero to one; glibc random_r.c:178-207.
    pub fn srand(&mut self, seed: u32) {
        let seed = if seed == 0 { 1 } else { seed };
        self.state.words[0] = seed;
        let mut word = i64::from(seed as i32);
        for slot in &mut self.state.words[1..] {
            let hi = word / 127_773;
            let lo = word % 127_773;
            word = 16_807 * lo - 2_836 * hi;
            if word < 0 {
                word += 2_147_483_647;
            }
            *slot = word as u32;
        }
        self.state.rear = 0;
        for _ in 0..310 {
            self.next_i32();
        }
    }

    /// Restore the rear position with front fixed three words ahead; glibc random_r.c:310-335.
    pub fn from_state(state: CRandState) -> Result<Self, Error> {
        if state.rear >= 31 {
            return Err(Error::InvalidCIndex);
        }
        Ok(Self { state })
    }

    pub fn state(&self) -> &CRandState {
        &self.state
    }

    pub fn into_state(self) -> CRandState {
        self.state
    }

    /// Add modulo 2^32, drop the low bit, and advance the ring; glibc random_r.c:353-395.
    pub fn next_i32(&mut self) -> i32 {
        let rear = self.state.rear as usize;
        let front = (rear + 3) % 31;
        let word = self.state.words[front].wrapping_add(self.state.words[rear]);
        self.state.words[front] = word;
        self.state.rear = if rear == 30 { 0 } else { rear as u32 + 1 };
        (word >> 1) as i32
    }
}
