// SPDX-License-Identifier: LGPL-2.1-or-later
// IBM Accurate Mathematical Library, Copyright (C) 2001-2022 Free Software Foundation, Inc.
// Rust translation, modified 2026: Benilla fork contributors; see GLIBC-NOTICE.txt.
use super::table::TABLE;
use crate::facing::soft::D;

fn d(bits: u64) -> D {
    D::bits(bits)
}
const BIG: u64 = 0x42c8000000000000;
const HP0: u64 = 0x3ff921fb54442d18;
const HP1: u64 = 0x3c91a62633145c07;
const MP1: u64 = 0x3ff921fb58000000;
const MP2: u64 = 0xbe4dde973c000000;
const PP3: u64 = 0xbc8cb3b398000000;
const PP4: u64 = 0xbacd747f23e32ed7;
const TOINT: u64 = 0x4338000000000000;
const SN3: u64 = 0xbfc5555555555515;
const SN5: u64 = 0x3f811110e829872f;
const CS2: u64 = 0x3fe0000000000000;
const CS4: u64 = 0xbfa5555555555535;
const CS6: u64 = 0x3f56c16bedd9e239;

fn copy_sign(x: D, sign: D) -> D {
    d((x.raw() & !(1 << 63)) | (sign.raw() & (1 << 63)))
}

// Software contractions follow libm+0x702c0..0x70ac9, not host compiler contraction.
fn sin(x: D, mut dx: D) -> D {
    let original = x;
    if x.abs() < d(0x3fc020c49ba5e354) {
        let xx = x * x;
        let p = d(0xbe5addffc2fcdf59).fma(xx, d(0x3ec71de27b9a7ed9));
        let p = p.fma(xx, d(0xbf2a01a019db08b8));
        let p = p.fma(xx, d(0x3f81111111110ece));
        let p = p.fma(xx, d(0xbfc5555555555555));
        let t = p.fma(x, -(d(CS2) * dx)).fma(xx, dx);
        return x + t;
    }
    if x <= D::ZERO {
        dx = -dx;
    }
    let u = d(BIG) + x.abs();
    let x = x.abs() - (u - d(BIG));
    let xx = x * x;
    let s = x + (x * xx).fma(d(SN5).fma(xx, d(SN3)), dx);
    let c = x.fma(dx, xx * d(CS6).fma(xx, d(CS4)).fma(xx, d(CS2)));
    let k = (u.raw() as u32 as usize) * 4;
    let [sn, ssn, cs, ccs] = [TABLE[k], TABLE[k + 1], TABLE[k + 2], TABLE[k + 3]].map(d);
    let cor = cs.fma(s, (-sn).fma(c, s.fma(ccs, ssn)));
    copy_sign(sn + cor, original)
}

fn cos(x: D, mut dx: D) -> D {
    if x < D::ZERO {
        dx = -dx;
    }
    let u = d(BIG) + x.abs();
    let x = (x.abs() - (u - d(BIG))) + dx;
    let xx = x * x;
    let s = (x * xx).fma(d(SN5).fma(xx, d(SN3)), x);
    let c = xx * d(CS6).fma(xx, d(CS4)).fma(xx, d(CS2));
    let k = (u.raw() as u32 as usize) * 4;
    let [sn, ssn, cs, ccs] = [TABLE[k], TABLE[k + 1], TABLE[k + 2], TABLE[k + 3]].map(d);
    let cor = (-sn).fma(s, (-cs).fma(c, (-s).fma(ssn, ccs)));
    cs + cor
}

fn reduce(x: D) -> (i32, D, D) {
    let t = x.fma(d(0x3fe45f306dc9c883), d(TOINT));
    let xn = t - d(TOINT);
    let y = (-xn).fma(d(MP2), (-xn).fma(d(MP1), x));
    let t2 = (-xn).fma(d(PP3), y);
    let db = (-xn).fma(d(PP3), y - t2);
    let b = (-xn).fma(d(PP4), t2);
    let db = db + (-xn).fma(d(PP4), t2 - b);
    ((t.raw() as i32) & 3, b, db)
}

pub(super) fn sine(bits: u64) -> u64 {
    let x = d(bits);
    let k = (bits >> 32) as u32 & 0x7fffffff;
    if k < 0x3e500000 {
        return bits;
    }
    if k < 0x3feb6000 {
        return sin(x, D::ZERO).raw();
    }
    if k < 0x400368fd {
        return copy_sign(cos(d(HP0) - x.abs(), d(HP1)), x).raw();
    }
    if k >= 0x7ff00000 {
        return if bits & 0x000fffffffffffff == 0 {
            0xfff8000000000000
        } else {
            bits | 0x0008000000000000
        };
    }
    let (n, a, da) = if k < 0x419921fb {
        reduce(x)
    } else {
        branred(x)
    };
    let result = if n & 1 != 0 { cos(a, da) } else { sin(a, da) };
    (if n & 2 != 0 { -result } else { result }).raw()
}

// The shared non-FMA reducer preserves branred.c:54-143 rounded operations.
fn branred(x: D) -> (i32, D, D) {
    let x = x * d(0x1a70000000000000);
    let split = d(0x41a0000002000000);
    let t = x * split;
    let x1 = t - (t - x);
    let x2 = x - x1;
    let (sum1, b1, bb1) = part(x1);
    let (sum2, b2, bb2) = part(x2);
    let mut sum = sum1 + sum2;
    let mut b = b1 + b2;
    let bb = if b1.abs() > b2.abs() {
        (b1 - b) + b2
    } else {
        (b2 - b) + b1
    };
    if b > d(0x3fe0000000000000) {
        b = b - D::one();
        sum = sum + D::one();
    } else if b < -d(0x3fe0000000000000) {
        b = b + D::one();
        sum = sum - D::one();
    }
    let s = b + ((bb + bb1) + bb2);
    let t = ((b - s) + bb) + (bb1 + bb2);
    let b = s * split;
    let t1 = b - (b - s);
    let t2 = s - t1;
    let b = s * d(HP0);
    let mp2 = d(0xbe4dde9740000000);
    let bb =
        (((t1 * d(MP1) - b) + t1 * mp2) + t2 * d(MP1)) + ((t2 * mp2 + s * d(HP1)) + t * d(HP0));
    let s = b + bb;
    let t = (b - s) + bb;
    (sum.truncate_i32().expect("branred quadrant") & 3, s, t)
}

fn part(x: D) -> (D, D, D) {
    let k = ((((x.raw() >> 52) & 2047) as i32 - 450) / 24).max(0) as usize;
    let mut gor = d(0x63f0000000000000 - ((k as u64 * 24) << 52));
    let mut r = [D::ZERO; 6];
    for (i, value) in r.iter_mut().enumerate() {
        *value = (x * d(TOVERP[k + i])) * gor;
        gor = gor * d(0x3e70000000000000);
    }
    let mut sum = D::ZERO;
    for value in &mut r[..3] {
        let s = (*value + d(TOINT)) - d(TOINT);
        sum = sum + s;
        *value = *value - s;
    }
    let mut t = D::ZERO;
    for value in r.iter().rev() {
        t = t + *value;
    }
    let bb = (((((r[0] - t) + r[1]) + r[2]) + r[3]) + r[4]) + r[5];
    let s = (t + d(TOINT)) - d(TOINT);
    sum = sum + s;
    let t = t - s;
    let b = t + bb;
    let bb = (t - b) + bb;
    let s = (sum + d(0x4358000000000000)) - d(0x4358000000000000);
    (sum - s, b, bb)
}

const TOVERP: [u64; 75] = [
    0x41645f3060000000,
    0x415b939100000000,
    0x413529fc00000000,
    0x4143abe880000000,
    0x416ea69ba0000000,
    0x41681b6c40000000,
    0x4162b32780000000,
    0x4150e41040000000,
    0x416fca2c60000000,
    0x41657bd760000000,
    0x4168ac36e0000000,
    0x4142371d00000000,
    0x4150937480000000,
    0x416c00c920000000,
    0x4147750480000000,
    0x416a324380000000,
    0x416fc3bd60000000,
    0x413cb12900000000,
    0x4164e7dd00000000,
    0x416046bea0000000,
    0x41475da200000000,
    0x41609d3380000000,
    0x415c09ad00000000,
    0x4157df9040000000,
    0x414cc8eb00000000,
    0x414cc1a980000000,
    0x414cfa4e00000000,
    0x41608bf160000000,
    0x4167bf2500000000,
    0x414d8ffc00000000,
    0x4162fffbc0000000,
    0x4116603c00000000,
    0x416de5e220000000,
    0x41616b4140000000,
    0x415b47db40000000,
    0x414b3f6780000000,
    0x4143e58480000000,
    0x4166e9e8c0000000,
    0x414fb34f00000000,
    0x4157fa8b40000000,
    0x415d49ee80000000,
    0x4168fd7ca0000000,
    0x416e2f67a0000000,
    0x411ce7dc00000000,
    0x41614a5240000000,
    0x416d4d7f60000000,
    0x4157ec47c0000000,
    0x4161aba100000000,
    0x415580cc00000000,
    0x4151bf1ec0000000,
    0x415aeafc00000000,
    0x4169f78400000000,
    0x41635e86c0000000,
    0x413da9e300000000,
    0x41622c2bc0000000,
    0x416cc36100000000,
    0x4159666140000000,
    0x4157c52800000000,
    0x415a102340000000,
    0x416ffb1000000000,
    0x41535cc9c0000000,
    0x4148830300000000,
    0x413556ca00000000,
    0x415cea3240000000,
    0x4158389ec0000000,
    0x4168118d60000000,
    0x4151f10640000000,
    0x41686cf9a0000000,
    0x416b9d0120000000,
    0x414541ac80000000,
    0x41688ed160000000,
    0x4162c394c0000000,
    0x416bb5e880000000,
    0x416a2ae320000000,
    0x4114fa9400000000,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_reduction_word_equals_its_exact_source_integer_limb() {
        let limbs = [
            10680707, 7228996, 1387004, 2578385, 16069853, 12639074, 9804092, 4427841, 16666979,
            11263675, 12935607, 2387514, 4345298, 14681673, 3074569, 13734428, 16653803, 1880361,
            10960616, 8533493, 3062596, 8710556, 7349940, 6258241, 3772886, 3769171, 3798172,
            8675211, 12450088, 3874808, 9961438, 366607, 15675153, 9132554, 7151469, 3571407,
            2607881, 12013382, 4155038, 6285869, 7677882, 13102053, 15825725, 473591, 9065106,
            15363067, 6271263, 9264392, 5636912, 4652155, 7056368, 13614112, 10155062, 1944035,
            9527646, 15080200, 6658437, 6231200, 6832269, 16767104, 5075751, 3212806, 1398474,
            7579849, 6349435, 12618859, 4703257, 12806093, 14477321, 2786137, 12875403, 9837734,
            14528324, 13719321, 343717,
        ];
        assert_eq!(TOVERP.len(), limbs.len());
        for (index, (word, limb)) in TOVERP.into_iter().zip(limbs).enumerate() {
            assert_eq!(word, D::integer(limb).raw(), "source limb {index}");
        }
    }
}
