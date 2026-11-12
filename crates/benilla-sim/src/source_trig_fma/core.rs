// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright (C) 2018-2022 Free Software Foundation, Inc.
// Rust translation, modified 2026: Benilla fork contributors.
// glibc 2.36 s_sinf.c, s_cosf.c, s_sincosf.h, sincosf_poly.h, s_sincosf_data.c.
// Contractions follow pinned libm+0x73280 and +0x73720; see GLIBC-NOTICE.txt.
use std::ops::{Mul, Neg};

pub(super) trait Arithmetic {
    type Double: Copy + Mul<Output = Self::Double> + Neg<Output = Self::Double>;
    fn double(bits: u64) -> Self::Double;
    fn single(bits: u32) -> Self::Double;
    fn integer(value: i64) -> Self::Double;
    fn truncate(value: Self::Double) -> i32;
    fn narrow(value: Self::Double) -> u32;
    fn fma(x: Self::Double, y: Self::Double, z: Self::Double) -> Self::Double;
}

const HPI_INV: u64 = 0x4164_5f30_6dc9_c883;
const HPI: u64 = 0x3ff9_21fb_5444_2d18;
const PI63: u64 = 0x3c19_21fb_5444_2d18;
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
const INV_PIO4: [u32; 24] = [
    0xa2, 0xa2f9, 0xa2f983, 0xa2f9836e, 0xf9836e4e, 0x836e4e44, 0x6e4e4415, 0x4e441529, 0x441529fc,
    0x1529fc27, 0x29fc2757, 0xfc2757d1, 0x2757d1f5, 0x57d1f534, 0xd1f534dd, 0xf534ddc0, 0x34ddc0db,
    0xddc0db62, 0xc0db6295, 0xdb629599, 0x6295993c, 0x95993c43, 0x993c4390, 0x3c439041,
];

pub(super) fn sin_cos<A: Arithmetic>(bits: u32) -> [u32; 2] {
    let top = (bits >> 20) & 0x7ff;
    if top >= 0x7f8 {
        // __math_invalidf: x-x quiets NaNs, infinity yields x86's negative qNaN.
        let nan = if bits & 0x7fff_ffff == 0x7f80_0000 {
            0xffc0_0000
        } else {
            bits | 0x0040_0000
        };
        return [nan; 2];
    }
    if top < 0x398 {
        return [bits, 0x3f80_0000];
    }
    let x = A::single(bits);
    if top < 0x3f4 {
        return [
            polynomial::<A>(x, x * x, 0, false),
            polynomial::<A>(x, x * x, 1, false),
        ];
    }
    let (x, n, sign) = if top < 0x42f {
        let n = (A::truncate(x * A::double(HPI_INV)) + 0x80_0000) >> 24;
        // vfnmadd132sd at 0x7331c / 0x737b4: one rounding, not x - rounded(n*hpi).
        (A::fma(-A::integer(i64::from(n)), A::double(HPI), x), n, n)
    } else {
        let (x, n) = reduce_large::<A>(bits);
        (x, n, n + (bits >> 31) as i32)
    };
    let signed = x * A::integer([1, -1, -1, 1][(sign & 3) as usize]);
    let x2 = x * x;
    [
        polynomial::<A>(signed, x2, n, sign & 2 != 0),
        polynomial::<A>(signed, x2, n ^ 1, sign & 2 != 0),
    ]
}

fn reduce_large<A: Arithmetic>(bits: u32) -> (A::Double, i32) {
    let index = ((bits >> 26) & 15) as usize;
    let shift = (bits >> 23) & 7;
    let xi = ((bits & 0x00ff_ffff) | 0x0080_0000) << shift;
    // The first C product is uint32_t before widening. All additions wrap.
    let res0 = u64::from(xi.wrapping_mul(INV_PIO4[index]));
    let res1 = u64::from(xi) * u64::from(INV_PIO4[index + 4]);
    let res2 = u64::from(xi) * u64::from(INV_PIO4[index + 8]);
    let res0 = ((res2 >> 32) | (res0 << 32)).wrapping_add(res1);
    let n = res0.wrapping_add(1 << 61) >> 62;
    let remainder = res0.wrapping_sub(n << 62) as i64;
    (A::integer(remainder) * A::double(PI63), n as i32)
}

fn polynomial<A: Arithmetic>(x: A::Double, x2: A::Double, n: i32, negate_cos: bool) -> u32 {
    if n & 1 == 0 {
        let x3 = x * x2;
        let s1 = A::fma(x2, A::double(SIN[2]), A::double(SIN[1]));
        let x7 = x3 * x2;
        let s = A::fma(x3, A::double(SIN[0]), x);
        A::narrow(A::fma(x7, s1, s))
    } else {
        let c = COS.map(|bits| A::double(bits ^ if negate_cos { 1 << 63 } else { 0 }));
        let x4 = x2 * x2;
        let c2 = A::fma(x2, c[4], c[3]);
        let c1 = A::fma(x2, c[1], c[0]);
        let x6 = x4 * x2;
        let c = A::fma(x4, c[2], c1);
        A::narrow(A::fma(x6, c2, c))
    }
}
