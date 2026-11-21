// SPDX-License-Identifier: GPL-3.0-or-later
// Software IEEE RNE translation of glibc-2.36 e_atan2f.c and s_atanf.c.
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Developed at SunPro, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
use super::soft::S;
fn abs(x: S) -> S {
    S::bits(x.raw() & 0x7fff_ffff)
}
fn atan(mut x: S) -> S {
    let hx = x.raw();
    let ix = hx & 0x7fff_ffff;
    let hi = [0x3eed_6338, 0x3f49_0fda, 0x3f7b_985e, 0x3fc9_0fda].map(S::bits);
    let lo = [0x31ac_3769, 0x3322_2168, 0x3314_0fb4, 0x33a2_2168].map(S::bits);
    if ix >= 0x4c00_0000 {
        return if hx >> 31 == 0 {
            hi[3] + lo[3]
        } else {
            -hi[3] - lo[3]
        };
    }
    let one = S::bits(0x3f80_0000);
    let two = S::bits(0x4000_0000);
    let half3 = S::bits(0x3fc0_0000);
    let id = if ix < 0x3ee0_0000 {
        if ix < 0x3100_0000 {
            return x;
        }
        None
    } else {
        x = abs(x);
        if ix < 0x3f98_0000 {
            if ix < 0x3f30_0000 {
                x = (two * x - one) / (two + x);
                Some(0)
            } else {
                x = (x - one) / (x + one);
                Some(1)
            }
        } else if ix < 0x401c_0000 {
            x = (x - half3) / (one + half3 * x);
            Some(2)
        } else {
            x = -one / x;
            Some(3)
        }
    };
    // Decimal aT[0] rounds to 0x3eaaaaab. The original 0x3eaaaaaa comment is stale.
    let a = [
        0x3eaa_aaab,
        0xbe4c_cccd,
        0x3e12_4925,
        0xbde3_8e38,
        0x3dba_2e6e,
        0xbd9d_8795,
        0x3d88_6b35,
        0xbd6e_f16b,
        0x3d4b_da59,
        0xbd15_a221,
        0x3c85_69d7,
    ]
    .map(S::bits);
    let z = x * x;
    let w = z * z;
    let s1 = z * (a[0] + w * (a[2] + w * (a[4] + w * (a[6] + w * (a[8] + w * a[10])))));
    let s2 = w * (a[1] + w * (a[3] + w * (a[5] + w * (a[7] + w * a[9]))));
    if let Some(id) = id {
        let z = hi[id] - ((x * (s1 + s2) - lo[id]) - x);
        if hx >> 31 != 0 {
            -z
        } else {
            z
        }
    } else {
        x - x * (s1 + s2)
    }
}
/// Inputs are finite binary32. The internal quotient may legally overflow to infinity.
pub(super) fn atan2(y: S, x: S) -> S {
    let hx = x.raw();
    let hy = y.raw();
    let ix = hx & 0x7fff_ffff;
    let iy = hy & 0x7fff_ffff;
    if hx == 0x3f80_0000 {
        return atan(y);
    }
    let m = ((hy >> 31) & 1) | ((hx >> 30) & 2);
    let pi = S::bits(0x4049_0fdb);
    let half_pi = S::bits(0x3fc9_0fdb);
    let pi_lo = S::bits(0xb3bb_bd2e);
    // Adding source tiny changes exception flags, not RNE result bits.
    if iy == 0 {
        return match m {
            0 | 1 => y,
            2 => pi,
            _ => -pi,
        };
    }
    if ix == 0 {
        return if hy >> 31 != 0 { -half_pi } else { half_pi };
    }
    let k = (iy as i32 - ix as i32) >> 23;
    let z = if k > 60 {
        half_pi + S::bits(0x3f00_0000) * pi_lo
    } else if hx >> 31 != 0 && k < -60 {
        S::ZERO
    } else {
        atan(abs(y / x))
    };
    match m {
        0 => z,
        1 => S::bits(z.raw() ^ 0x8000_0000),
        2 => pi - (z - pi_lo),
        _ => (z - pi_lo) - pi,
    }
}
