// SPDX-License-Identifier: GPL-3.0-or-later
use super::Error;
/// ModelInstance.cpp:31; G3D Matrix3.cpp:1655-1671.
/// Unbound callers retain their original refusals.
pub(super) fn sin_cos_degrees(angle: f32) -> Result<(f32, f32), Error> {
    // ModelInstance.cpp:31 widens the degrees before narrowing radians to float.
    let radians = (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(angle) / 180.0) as f32;
    if radians.to_bits() & 0x7fff_ffff > 0x41fb_53d1 {
        return Err(Error::RotationDomain);
    }
    if angle == 0.0 {
        let [s, c] = benilla_sim::source_trig_fma::sin_cos_bits_native(radians.to_bits());
        Ok((f32::from_bits(s), f32::from_bits(c)))
    } else {
        Err(Error::NumericBinding)
    }
}

/// ModelInstance.cpp:31 and G3D Matrix3.cpp:1655-1671 impose no angle limit.
pub(super) fn sin_cos_degrees_bound(
    angle: f32,
    _binding: benilla_sim::source_trig_fma::DispatchBinding,
) -> (f32, f32) {
    let radians = (f64::from_bits(0x4009_21fb_5444_2d28) * f64::from(angle) / 180.0) as f32;
    let [s, c] = benilla_sim::source_trig_fma::sin_cos_bits_native(radians.to_bits());
    (f32::from_bits(s), f32::from_bits(c))
}
