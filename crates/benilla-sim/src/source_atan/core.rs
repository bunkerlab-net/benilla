// SPDX-License-Identifier: LGPL-2.1-or-later
// IBM Accurate Mathematical Library, written by International Business Machines Corp.
// Copyright (C) 2001-2025 Free Software Foundation, Inc.
// Bounded Rust translation, modified 2026-10-15: Benilla fork contributors.
// Derived from GNU C Library 2.41 s_atan.c, atnat.h and non-FMA dla.h macros.
// See GLIBC-NOTICE.txt, GLIBC-LICENSE.txt and ../../SOURCE-ATAN.md.
use super::table::CIJ;
use crate::facing::soft::D;

// Original atnat.h bit patterns. No decimal floating-point conversions.
const A: u64 = 0x3e4b_b67a_0000_0000;
const B: u64 = 0x3fb0_0000_0000_0000;
const C: u64 = 0x3ff0_0000_0000_0000;
const LIMIT_D: u64 = 0x4030_0000_0000_0000;
const E: u64 = 0x4334_9ff2_0000_0000;
const HPI: u64 = 0x3ff9_21fb_5444_2d18;
const HPI1: u64 = 0x3c91_a626_3314_5c07;
const D3: u64 = 0xbfd5_5555_5555_5555;
const D5: u64 = 0x3fc9_9999_9999_97fd;
const D7: u64 = 0xbfc2_4924_923f_7603;
const D9: u64 = 0x3fbc_71c6_e512_9a3b;
const D11: u64 = 0xbfb7_4580_22b1_3c25;
const D13: u64 = 0x3fb3_75f0_8b31_cbce;
const TWO52: u64 = 0x4330_0000_0000_0000;
const CN: u64 = 0x41a0_0000_0200_0000;

/// Original non-FMA EMULV: each operator performs its own binary64 rounding.
fn emulv(x: D, y: D) -> (D, D) {
    let p = D::bits(CN) * x;
    let hx = (x - p) + p;
    let tx = x - hx;
    let p = D::bits(CN) * y;
    let hy = (y - p) + p;
    let ty = y - hy;
    let z = x * y;
    let zz = (((hx * hy - z) + hx * ty) + tx * hy) + tx * ty;
    (z, zz)
}

/// Original ESUB, including its strict absolute-magnitude comparison.
fn esub(x: D, y: D) -> (D, D) {
    let z = x - y;
    let zz = if x.abs() > y.abs() {
        (x - z) - y
    } else {
        x - (y + z)
    };
    (z, zz)
}

fn polynomial(x: D, v: D) -> D {
    let yy = D::bits(D11) + v * D::bits(D13);
    let yy = D::bits(D9) + v * yy;
    let yy = D::bits(D7) + v * yy;
    let yy = D::bits(D5) + v * yy;
    let yy = D::bits(D3) + v * yy;
    yy * (x * v)
}

fn table(u: D) -> [D; 7] {
    // Source rounds 256*u with the TWO52 addition before C int truncation.
    // Both callers have 1/16 <= u <= 1, hence the rounded integer is 16..=256.
    let index = ((D::bits(TWO52) + D::integer(256) * u) - D::bits(TWO52))
        .truncate_i32()
        .expect("bounded source atan table integer")
        - 16;
    CIJ[usize::try_from(index).expect("bounded source atan table index")].map(D::bits)
}

/// The wrapper establishes nonnegative widened binary32 or negative zero.
pub(super) fn atan(x: D) -> D {
    let u = if x < D::ZERO { -x } else { x };
    if u < D::bits(C) {
        if u < D::bits(B) {
            if u < D::bits(A) {
                // Source underflow-flag forcing has no numeric effect here.
                x
            } else {
                let v = x * x;
                let yy = polynomial(x, v);
                x + yy
            }
        } else {
            let cij = table(u);
            let z = u - cij[0];
            let yy = cij[5] + z * cij[6];
            let yy = cij[4] + z * yy;
            let yy = cij[3] + z * yy;
            let yy = cij[2] + z * yy;
            let yy = yy * z;
            let t1 = cij[1];
            t1 + yy
        }
    } else if u < D::bits(LIMIT_D) {
        let w = D::one() / u;
        let (t1, t2) = emulv(w, u);
        let ww = w * ((D::one() - t1) - t2);
        let cij = table(w);
        let z = (w - cij[0]) + ww;
        let yy = cij[5] + z * cij[6];
        let yy = cij[4] + z * yy;
        let yy = cij[3] + z * yy;
        let yy = cij[2] + z * yy;
        let yy = D::bits(HPI1) - z * yy;
        let t1 = D::bits(HPI) - cij[1];
        t1 + yy
    } else if u < D::bits(E) {
        let w = D::one() / u;
        let v = w * w;
        let (t1, t2) = emulv(w, u);
        let yy = polynomial(w, v);
        let ww = w * ((D::one() - t1) - t2);
        let (t3, cor) = esub(D::bits(HPI), w);
        let yy = ((D::bits(HPI1) + cor) - ww) - yy;
        t3 + yy
    } else {
        // Positive infinity and large positive inputs return the source HPI.
        D::bits(HPI)
    }
}
