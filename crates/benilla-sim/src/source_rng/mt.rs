// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from GNU libstdc++ GCC 12.2, copyright FSF 2009-2022;
// upstream GPL-3.0-or-later WITH GCC-exception-3.1; see SOURCE-RNG.md.
use super::Error;

pub const MT_STATE_WORDS: usize = 624;

/// Generate uint32 seed words without allocating; bits/random.tcc:3262-3349.
pub fn seed_seq_generate(seeds: &[u32], output: &mut [u32]) {
    let n = output.len();
    if n == 0 {
        return;
    }
    output.fill(0x8b8b_8b8b);
    let s = seeds.len();
    let t = match n {
        623.. => 11,
        68.. => 7,
        39.. => 5,
        7.. => 3,
        _ => (n - 1) / 2,
    };
    let p = (n - t) / 2;
    let q = p + t;
    let m = (s + 1).max(n);
    let r1 = 1_371_501_266_u32;
    let r2 = r1.wrapping_add(s as u32);
    output[p] = output[p].wrapping_add(r1);
    output[q] = output[q].wrapping_add(r2);
    output[0] = r2;
    for k in 1..m {
        let kn = k % n;
        let kpn = (kn + p) % n;
        let kqn = (kn + q) % n;
        let arg = output[kn] ^ output[kpn] ^ output[(k - 1) % n];
        let r1 = 1_664_525_u32.wrapping_mul(arg ^ (arg >> 27));
        let mut r2 = r1.wrapping_add(kn as u32);
        if k <= s {
            r2 = r2.wrapping_add(seeds[k - 1]);
        }
        output[kpn] = output[kpn].wrapping_add(r1);
        output[kqn] = output[kqn].wrapping_add(r2);
        output[kn] = r2;
    }
    for k in m..m + n {
        let kn = k % n;
        let kpn = (kn + p) % n;
        let kqn = (kn + q) % n;
        let arg = output[kn]
            .wrapping_add(output[kpn])
            .wrapping_add(output[(k - 1) % n]);
        let r3 = 1_566_083_941_u32.wrapping_mul(arg ^ (arg >> 27));
        let r4 = r3.wrapping_sub(kn as u32);
        output[kpn] ^= r3;
        output[kqn] ^= r4;
        output[kn] = r4;
    }
}

/// Keep every mathematical word and the LP64 position; bits/random.tcc:477-501.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MtState {
    pub words: [u32; MT_STATE_WORDS],
    pub index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Mt19937 {
    state: MtState,
}

impl Default for Mt19937 {
    fn default() -> Self {
        Self::from_seed(5489)
    }
}

impl Mt19937 {
    /// Reduce the scalar seed modulo 2^32 and expand it; bits/random.tcc:328-343.
    pub fn from_seed(seed: u64) -> Self {
        let mut words = [0; MT_STATE_WORDS];
        words[0] = seed as u32;
        for i in 1..MT_STATE_WORDS {
            let previous = words[i - 1];
            words[i] = (previous ^ (previous >> 30))
                .wrapping_mul(1_812_433_253)
                .wrapping_add(i as u32);
        }
        Self {
            state: MtState { words, index: 624 },
        }
    }

    /// Generate all 624 words and repair only a seeded zero state; bits/random.tcc:354-389.
    pub fn from_seed_seq(seeds: &[u32]) -> Self {
        let mut words = [0; MT_STATE_WORDS];
        seed_seq_generate(seeds, &mut words);
        if words[0] & 0x8000_0000 == 0 && words[1..].iter().all(|&word| word == 0) {
            words[0] = 0x8000_0000;
        }
        Self {
            state: MtState { words, index: 624 },
        }
    }

    /// Restore complete state without applying seed-only normalization; bits/random.tcc:505-529.
    pub fn from_state(state: MtState) -> Result<Self, Error> {
        if state.index > MT_STATE_WORDS as u64 {
            return Err(Error::InvalidMtIndex);
        }
        Ok(Self { state })
    }

    pub fn state(&self) -> &MtState {
        &self.state
    }

    pub fn into_state(self) -> MtState {
        self.state
    }

    /// Twist on exhaustion, then temper exactly one word; bits/random.tcc:455-469.
    pub fn next_u32(&mut self) -> u32 {
        if self.state.index == MT_STATE_WORDS as u64 {
            self.twist();
        }
        let mut word = self.state.words[self.state.index as usize];
        self.state.index += 1;
        word ^= word >> 11;
        word ^= (word << 7) & 0x9d2c_5680;
        word ^= (word << 15) & 0xefc6_0000;
        word ^= word >> 18;
        word
    }

    /// Mutate the recurrence in source order, including the final wrap; bits/random.tcc:399-424.
    fn twist(&mut self) {
        for i in 0..227 {
            let joined =
                (self.state.words[i] & 0x8000_0000) | (self.state.words[i + 1] & 0x7fff_ffff);
            self.state.words[i] = self.state.words[i + 397]
                ^ (joined >> 1)
                ^ if joined & 1 != 0 { 0x9908_b0df } else { 0 };
        }
        for i in 227..623 {
            let joined =
                (self.state.words[i] & 0x8000_0000) | (self.state.words[i + 1] & 0x7fff_ffff);
            self.state.words[i] = self.state.words[i - 227]
                ^ (joined >> 1)
                ^ if joined & 1 != 0 { 0x9908_b0df } else { 0 };
        }
        let joined = (self.state.words[623] & 0x8000_0000) | (self.state.words[0] & 0x7fff_ffff);
        self.state.words[623] =
            self.state.words[396] ^ (joined >> 1) ^ if joined & 1 != 0 { 0x9908_b0df } else { 0 };
        self.state.index = 0;
    }
}
