// SPDX-License-Identifier: MIT
// Copyright (c) 2022-2024 Alexei Sibidanov and Paul Zimmermann.
// Rust software-IEEE translation, modified 2026 by Benilla fork contributors.
// GNU libc 2.41 e_atan2f.c / CORE-MATH 7835c5d. See ../../FACING.md and MIT-NOTICE.txt.
use super::soft::{D, S};

const CN: [u64; 7] = [
    0x3ff0000000000000,
    0x40040e0698f94c35,
    0x400248c5da347f0d,
    0x3fed873386572976,
    0x3fc46fa40b20f1d0,
    0x3f833f5e041eed0f,
    0x3f1546bbf28667c5,
];
const CD: [u64; 7] = [
    0x3ff0000000000000,
    0x4006b8b143a3f6da,
    0x4008421201d18ed5,
    0x3ff8221d086914eb,
    0x3fd670657e3a07ba,
    0x3fa0f4951fd1e72d,
    0x3f4b3874b8798286,
];
const C: [[u64; 2]; 32] = [
    [0x3ff0000000000000, 0xba88c1dac5492248],
    [0xbfd5555555555555, 0xbc755553bf3a2abe],
    [0x3fc999999999999a, 0xbc699deed1ec9071],
    [0xbfc2492492492492, 0xbc5fd99c8d18269a],
    [0x3fbc71c71c71c717, 0xbc2651eee4c4d9d0],
    [0xbfb745d1745d1649, 0xbc5632683d6c44a6],
    [0x3fb3b13b13b11c63, 0x3c5bf69c1f8af41d],
    [0xbfb11111110e6338, 0x3c23c3e431e8bb68],
    [0x3fae1e1e1dc45c4a, 0xbc4be2db05c77bbf],
    [0xbfaaf286b8164b4f, 0x3c2a4673491f0942],
    [0x3fa86185e9ad4846, 0x3c4e12e32d79fcee],
    [0xbfa642c6d5161fae, 0x3c43ce76c1ca03f0],
    [0x3fa47ad6f277e5bf, 0xbc3abd8d85bdb714],
    [0xbfa2f64a2ee8896d, 0x3c2ef87d4b615323],
    [0x3fa1a6a2b31741b5, 0x3c1a5d9d973547ee],
    [0xbfa07fbdad65e0a6, 0xbc265ac07f5d35f4],
    [0x3f9ee9932a9a5f8b, 0x3c2f8b9623f6f55a],
    [0xbf9ce8b5b9584dc6, 0x3c2fe5af96e8ea2d],
    [0x3f9ac9cb288087b7, 0xbc3450cdfceaf5ca],
    [0xbf984b025351f3e6, 0x3c2579561b0d73da],
    [0x3f952f5b8ecdd52b, 0x3c3036bd2c6fba47],
    [0xbf9163a8c44909dc, 0x3c318f735ffb9f16],
    [0x3f8a400dce3eea6f, 0xbc2c90569c0c1b5c],
    [0xbf81caa78ae6db3a, 0xbc24c60f8161ea09],
    [0x3f752672453c0731, 0x3c1834efb598c338],
    [0xbf65850c5be137cf, 0xbc0445fc150ca7f5],
    [0x3f523eb98d22e1ca, 0xbbf388fbaf1d7830],
    [0xbf38f4e974a40741, 0x3bd271198a97da34],
    [0x3f1a5cf2e9cf76e5, 0xbbb887eb4a63b665],
    [0xbef420c270719e32, 0x3b8efd595b27888b],
    [0x3ec3ba2d69b51677, 0xbb64fb06829cdfc7],
    [0xbe829b7e6f676385, 0xbb2a783b6de718fb],
];

fn muldd(xh: D, xl: D, ch: D, cl: D) -> (D, D) {
    let ahlh = ch * xl;
    let alhh = cl * xh;
    let ahhh = ch * xh;
    let ahhl = ch.fma(xh, -ahhh);
    let ahhl = ahhl + (alhh + ahlh);
    let ch = ahhh + ahhl;
    (ch, (ahhh - ch) + ahhl)
}
fn polydd(xh: D, xl: D) -> (D, D) {
    let [ch, cl] = C[31];
    let (mut ch, mut cl) = (D::bits(ch), D::bits(cl));
    for c in C[..31].iter().rev() {
        (ch, cl) = muldd(xh, xl, ch, cl);
        let th = ch + D::bits(c[0]);
        let tl = (D::bits(c[0]) - th) + ch;
        ch = th;
        cl = cl + (tl + D::bits(c[1]));
    }
    (ch, cl)
}
fn tiny(y: S, x: S) -> S {
    let (dy, dx) = (y.double(), x.double());
    let z = dy / dx;
    let e = (-z).fma(dx, dy);
    let c = D::bits(0xbfd5_5555_5555_5555);
    let zz = z * z;
    let cz = c * z;
    let e = e / dx + cz * zz;
    let mut t = z.raw();
    if t & 0x0fff_ffff == 0 {
        if z * e > D::ZERO {
            t = t.wrapping_add(1);
        } else {
            t = t.wrapping_sub(1);
        }
    }
    D::bits(t).single()
}

/// Finite branch of the unchanged source algorithm. Nonfinite inputs are refused
/// at the public boundary rather than taking its NaN/infinity return branches.
/// RNE and explicit software FMA are part of the selected numeric profile.
pub(super) fn atan2(y: S, x: S) -> S {
    let pi = D::bits(0x4009_21fb_5444_2d18);
    let pi2 = D::bits(0x3ff9_21fb_5444_2d18);
    let pi2l = D::bits(0x3c91_a626_3314_5c07);
    let twice = D::bits(0x4000_0000_0000_0000);
    let off = [D::ZERO, pi2, pi, pi2, -D::ZERO, -pi2, -pi, -pi2];
    let offl = [
        D::ZERO,
        pi2l,
        twice * pi2l,
        pi2l,
        -D::ZERO,
        -pi2l,
        -(twice * pi2l),
        -pi2l,
    ];
    let sgn = [D::one(), -D::one()];
    let m = [D::ZERO, D::one()];
    let (ux, uy) = (x.raw(), y.raw());
    let (ax, ay) = (ux & 0x7fff_ffff, uy & 0x7fff_ffff);
    if ay == 0 {
        if ax == 0 {
            let i = ((uy >> 31) * 4 + (ux >> 31) * 2) as usize;
            return if ux >> 31 != 0 {
                off[i] + offl[i]
            } else {
                off[i]
            }
            .single();
        }
        if ux >> 31 == 0 {
            return (D::ZERO * sgn[(uy >> 31) as usize]).single();
        }
    }
    let gt = usize::from(ay > ax);
    let i = (uy >> 31) as usize * 4 + (ux >> 31) as usize * 2 + gt;
    let (zx, zy) = (x.double(), y.double());
    let mut z = (m[gt] * zx + m[1 - gt] * zy) / (m[gt] * zy + m[1 - gt] * zx);
    let d = ax as i32 - ay as i32;
    let mut r = if d < 27 << 23 && d > -(27 << 23) {
        let z2 = z * z;
        let z4 = z2 * z2;
        let z8 = z4 * z4;
        let mut cn0 = D::bits(CN[0]) + z2 * D::bits(CN[1]);
        let cn2 = D::bits(CN[2]) + z2 * D::bits(CN[3]);
        let mut cn4 = D::bits(CN[4]) + z2 * D::bits(CN[5]);
        cn0 = cn0 + z4 * cn2;
        cn4 = cn4 + z4 * D::bits(CN[6]);
        cn0 = cn0 + z8 * cn4;
        let mut cd0 = D::bits(CD[0]) + z2 * D::bits(CD[1]);
        let cd2 = D::bits(CD[2]) + z2 * D::bits(CD[3]);
        let mut cd4 = D::bits(CD[4]) + z2 * D::bits(CD[5]);
        cd0 = cd0 + z4 * cd2;
        cd4 = cd4 + z4 * D::bits(CD[6]);
        cd0 = cd0 + z8 * cd4;
        cn0 / cd0
    } else {
        D::one()
    };
    z = z * sgn[gt];
    r = z * r + off[i];
    if r.raw().wrapping_add(8) & 0x0fff_ffff <= 16 {
        if ay < ax && (ax - ay) >> 23 >= 25 {
            return tiny(y, x);
        }
        let (mut zh, mut zl) = if gt == 0 {
            let zh = zy / zx;
            (zh, zh.fma(-zx, zy) / zx)
        } else {
            let zh = zx / zy;
            (zh, zh.fma(-zy, zx) / zy)
        };
        let (z2h, z2l) = muldd(zh, zl, zh, zl);
        let (ph, pl) = polydd(z2h, z2l);
        zh = zh * sgn[gt];
        zl = zl * sgn[gt];
        let (ph, pl) = muldd(zh, zl, ph, pl);
        let sh = ph + off[i];
        let sl = ((off[i] - sh) + ph) + pl + offl[i];
        let th = sh.single().double();
        let dh = sh - th;
        let mut tm = dh + sl;
        let tth = th.raw();
        let sixty = D::bits(0x3c30_0000_0000_0000); // 0x1p-60
        if th + th * sixty == th - th * sixty {
            let tth = (tth & (0x7ff << 52)).wrapping_sub(24 << 52);
            tm = tm
                * if tm.abs() > D::bits(tth) {
                    D::bits(0x3ff4_0000_0000_0000) // 1.25
                } else {
                    D::bits(0x3fe8_0000_0000_0000) // 0.75
                };
        }
        r = th + tm;
    }
    r.single()
}
