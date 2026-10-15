// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2018-2025 Free Software Foundation, Inc.
// Bounded Rust translation, modified 2026-10-15: Benilla fork contributors.
// Derived from GNU C Library 2.41 sinf/cosf, sinf_poly and reduce_fast.
// See GLIBC-NOTICE.txt, GLIBC-LICENSE.txt and ../../SOURCE-TRIG.md.
use super::Error;
use std::ops::{Add, Mul, Neg, Sub};

// Private static dispatch keeps the reducer and polynomial identical in both profiles.
pub(super) trait Arithmetic {
    type Double: Copy
        + Add<Output = Self::Double>
        + Sub<Output = Self::Double>
        + Mul<Output = Self::Double>
        + Neg<Output = Self::Double>;
    fn binary64(bits: u64) -> Self::Double;
    fn binary32(bits: u32) -> Self::Double;
    fn integer(value: i32) -> Self::Double;
    fn single(value: Self::Double) -> u32;
    fn truncate(value: Self::Double) -> Option<i32>;
}

// Original hexadecimal binary64 coefficients, with no decimal conversion.
const HPI_INV: u64 = 0x4164_5f30_6dc9_c883; // 2/pi * 2^24
const HPI: u64 = 0x3ff9_21fb_5444_2d18;
const COS: [u64; 5] = [
    0x3ff0_0000_0000_0000,
    0xbfdf_ffff_fd0c_621c,
    0x3fa5_5553_e106_8f19,
    0xbf56_c087_e89a_359d,
    0x3ef9_9343_027b_f8c3,
];
const SIN: [u64; 3] = [
    0xbfc5_5554_5995_a603,
    0x3f81_1076_0523_0bc4,
    0xbf29_94eb_3774_cf24,
];

pub(super) fn sin_cos<A: Arithmetic>(radians: u32) -> Result<[u32; 2], Error> {
    let top = (radians >> 20) & 0x7ff;
    let x = A::binary32(radians);
    // Source abstop12 branches, including its signed-zero and tiny-input behavior.
    if top < 0x3f4 {
        if top < 0x398 {
            return Ok([radians, 0x3f80_0000]);
        }
        return Ok([
            polynomial::<A>(x, x * x, 0, false),
            polynomial::<A>(x, x * x, 1, false),
        ]);
    }
    // The public finite |radians| <= float(2*pi) bound is well below 120.
    // Source TOINT_INTRINSICS=0: truncate toward zero, then signed arithmetic shift.
    let n = (A::truncate(x * A::binary64(HPI_INV)).ok_or(Error::Reduction)? + 0x80_0000) >> 24;
    let x = x - A::integer(n) * A::binary64(HPI);
    let sign = A::integer([1, -1, -1, 1][(n & 3) as usize]);
    Ok([
        polynomial::<A>(x * sign, x * x, n, n & 2 != 0),
        polynomial::<A>(x * sign, x * x, n ^ 1, n & 2 != 0),
    ])
}

fn polynomial<A: Arithmetic>(x: A::Double, x2: A::Double, n: i32, negate_cos: bool) -> u32 {
    if n & 1 == 0 {
        let x3 = x * x2;
        let s1 = A::binary64(SIN[1]) + x2 * A::binary64(SIN[2]);
        let x7 = x3 * x2;
        let s = x + x3 * A::binary64(SIN[0]);
        A::single(s + x7 * s1)
    } else {
        // Source negates every cosine coefficient before polynomial evaluation.
        let c = COS.map(|bits| {
            let c = A::binary64(bits);
            if negate_cos {
                -c
            } else {
                c
            }
        });
        let x4 = x2 * x2;
        let c2 = c[3] + x2 * c[4];
        let c1 = c[0] + x2 * c[1];
        let x6 = x4 * x2;
        let c = c1 + x4 * c[2];
        A::single(c + x6 * c2)
    }
}
