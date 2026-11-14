// SPDX-License-Identifier: GPL-3.0-or-later
// Compile the exact production generic kernel and its native arithmetic adapter.
#[path = "../src/source_trig_fma/core.rs"]
mod core;
#[path = "../src/source_trig_fma/native.rs"]
mod native;
#[unsafe(no_mangle)]
pub extern "C" fn port(bits: u32) -> u64 {
    let [sine, cosine] = core::sin_cos::<native::Native>(bits);
    u64::from(sine) | (u64::from(cosine) << 32)
}
