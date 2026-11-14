// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 Transports.cpp:567-589; glibc 2.36 s_tanf.c/k_tanf.c.
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Developed at SunPro, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
use super::*;

/// Generic flt-32 tanf, with the existing transport orientation domain ±TAU.
pub fn passenger_tangent(bits: u32) -> Result<u32> {
    if bits & 0x7fff_ffff > 0x40c9_0fdb {
        return Err(Error::NumericDomain);
    }
    let mut x = S::bits(bits);
    let (mut y, iy) = if bits & 0x7fff_ffff <= 0x3f49_0fda {
        (S::ZERO, 1)
    } else {
        let d = x.double();
        let n = ((d * D::bits(0x4164_5f30_6dc9_c883))
            .truncate_i32()
            .ok_or(Error::NumericDomain)?
            + 0x80_0000)
            >> 24;
        let d = d - D::integer(n) * D::bits(0x3ff9_21fb_5444_2d18);
        x = d.single();
        ((d - x.double()).single(), 1 - ((n & 1) << 1))
    };
    let hx = x.raw();
    let ix = hx & 0x7fff_ffff;
    let one = S::bits(0x3f80_0000);
    let two = S::bits(0x4000_0000);
    let v = if iy == 1 { one } else { -one };
    if ix < 0x3900_0000 {
        return Ok(if iy == 1 { x } else { -one / x }.raw());
    }
    let sign = if hx >> 31 == 0 { one } else { -one };
    if ix >= 0x3f2c_a140 {
        if hx >> 31 != 0 {
            x = -x;
            y = -y;
        }
        x = (S::bits(0x3f49_0fda) - x) + (S::bits(0x3322_2168) - y);
        y = S::ZERO;
        if x.raw() & 0x7fff_ffff < 0x3900_0000 {
            return Ok((sign * v * (one - two * v * x)).raw());
        }
    }
    let t = [
        0x3eaa_aaab,
        0x3e08_8889,
        0x3d5d_0dd1,
        0x3cb3_27a4,
        0x3c11_371f,
        0x3b6b_6916,
        0x3abe_de48,
        0x3a1a_26c8,
        0x3981_37b9,
        0x38a3_f445,
        0x3895_c07a,
        0xb79b_ae5f,
        0x37d9_5384,
    ]
    .map(S::bits);
    let z = x * x;
    let w = z * z;
    let r = t[1] + w * (t[3] + w * (t[5] + w * (t[7] + w * (t[9] + w * t[11]))));
    let a = z * (t[2] + w * (t[4] + w * (t[6] + w * (t[8] + w * (t[10] + w * t[12])))));
    let s = z * x;
    let r = (y + z * (s * (r + a) + y)) + t[0] * s;
    let w = x + r;
    if ix >= 0x3f2c_a140 {
        return Ok((sign * (v - two * (x - (w * w / (w + v) - r)))).raw());
    }
    if iy == 1 {
        return Ok(w.raw());
    }
    let z = S::bits(w.raw() & 0xffff_f000);
    let v = r - (z - x);
    let a = -one / w;
    let t = S::bits(a.raw() & 0xffff_f000);
    let s = one + t * z;
    Ok((t + a * (s + t * v)).raw())
}
fn trig(o: u32) -> Result<[S; 2]> {
    crate::source_trig_fma::sin_cos_radians_bits_soft(o)
        .map(|v| v.map(S::bits))
        .map_err(|_| Error::NumericDomain)
}
fn normalize(o: S) -> Result<u32> {
    crate::facing::normalize_bits(o.raw()).map_err(|_| Error::NumericDomain)
}
/// Float tangent quotient, not an inverse rotation; Transports.cpp:578-588.
pub fn passenger_offset(world: [u32; 4], transport: [u32; 4]) -> Result<[u32; 4]> {
    let [x, y, z, o] = world.map(S::bits);
    let [tx, ty, tz, to] = transport.map(S::bits);
    let [s, c] = trig(to.raw())?;
    let t = S::bits(passenger_tangent(to.raw())?);
    let x = x - tx;
    let y = y - ty;
    Ok([
        scalar((x + y * t) / (c + s * t))?,
        scalar((y - x * t) / (c + s * t))?,
        scalar(z - tz)?,
        normalize(o - to)?,
    ])
}
/// Ordinary float rotation with normalized orientation; Transports.cpp:567-575.
pub fn passenger_position(offset: [u32; 4], transport: [u32; 4]) -> Result<[u32; 4]> {
    let [x, y, z, o] = offset.map(S::bits);
    let [tx, ty, tz, to] = transport.map(S::bits);
    let [s, c] = trig(to.raw())?;
    Ok([
        scalar(tx + x * c - y * s)?,
        scalar(ty + y * c + x * s)?,
        scalar(tz + z)?,
        normalize(to + o)?,
    ])
}
