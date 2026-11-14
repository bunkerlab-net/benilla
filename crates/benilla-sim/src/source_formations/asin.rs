// SPDX-License-Identifier: LGPL-2.1-or-later
// Software IEEE RNE translation of glibc 2.36 flt-32/e_asinf.c.
// Copyright (C) 1993 by Sun Microsystems, Inc. All rights reserved.
// Developed at SunPro, a Sun Microsystems, Inc. business.
// Permission to use, copy, modify, and distribute this software is freely
// granted, provided that this notice is preserved.
// Modifications for single precision expansion are
// Copyright (C) 2001 Stephen L. Moshier <moshier@na-net.ornl.gov>
// and are incorporated herein by permission of the author. The author
// reserves the right to distribute this material elsewhere under different
// copying permissions. These modifications are distributed here under
// the following terms:
// This library is free software; you can redistribute it and/or modify it
// under the terms of the GNU Lesser General Public License as published by
// the Free Software Foundation; either version 2.1 of the License, or
// (at your option) any later version.
// This library is distributed in the hope that it will be useful, but
// WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU
// Lesser General Public License for more details.
// You should have received a copy of the GNU Lesser General Public License
// along with this library; if not, see <https://www.gnu.org/licenses/>.
use super::{s, S};
pub(super) fn sqrt(x: S) -> S {
    // Reuse the exact integer sqrtf kernel, retaining IEEE infinity/NaN results.
    if x.raw() == 0x7f80_0000 {
        return x;
    }
    crate::source_transports::numeric::sqrt_single(x).unwrap_or_else(|_| S::bits(0xffc0_0000))
}
pub(super) fn asin(x: S) -> S {
    let ix = x.raw() & 0x7fff_ffff;
    let hi = S::bits(0x3fc9_0fdb);
    let lo = s(-4.371_139e-8);
    let quarter = S::bits(0x3f49_0fdb);
    if ix == 0x3f80_0000 {
        return x * hi + x * lo;
    }
    if ix > 0x3f80_0000 {
        return S::bits(0xffc0_0000);
    }
    fn polynomial(t: S) -> S {
        t * (s(1.666_675_2e-1)
            + t * (s(7.495_297_6e-2)
                + t * (s(4.547_037_6e-2) + t * (s(2.417_951_4e-2) + t * s(4.216_631e-2)))))
    }
    if ix < 0x3f00_0000 {
        if ix < 0x3200_0000 {
            return x;
        }
        return x + x * polynomial(x * x);
    }
    let t = (s(1.0) - S::bits(ix)) * s(0.5);
    let p = polynomial(t);
    let root = sqrt(t);
    let out = if ix >= 0x3f79_999a {
        hi - (s(2.0) * (root + root * p) - lo)
    } else {
        let w = S::bits(root.raw() & 0xffff_f000);
        let c = (t - w * w) / (root + w);
        let p = s(2.0) * root * p - (lo - s(2.0) * c);
        let q = quarter - s(2.0) * w;
        quarter - (p - q)
    };
    if x.raw() >> 31 == 0 {
        out
    } else {
        -out
    }
}
