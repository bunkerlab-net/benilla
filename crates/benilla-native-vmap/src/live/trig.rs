// SPDX-License-Identifier: GPL-3.0-or-later
use super::Error;
/// ModelInstance.cpp:31; G3D Matrix3.cpp:1655-1671.
/// Exhaustive glibc 2.36 FMA comparison found ten cosf differences in the
/// existing glibc 2.41 port. Only the exact signed-zero identity is bound here.
pub(super) fn sin_cos_degrees(angle: f32) -> Result<(f32, f32), Error> {
    // ModelInstance.cpp:31 widens the degrees before narrowing radians to float.
    let radians = (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(angle) / 180.0) as f32;
    if radians.to_bits() & 0x7fff_ffff > 0x41fb_53d1 {
        return Err(Error::RotationDomain);
    }
    if angle == 0.0 {
        Ok((angle, 1.0))
    } else {
        Err(Error::NumericBinding)
    }
}
