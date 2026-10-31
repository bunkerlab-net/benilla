// SPDX-License-Identifier: GPL-3.0-or-later
//! Private per-candidate arithmetic from CMaNGOS 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f.
//!
//! `model.cpp::Doodad::ExtractSet`, G3D `Matrix3`, `Quat`, and `g3dmath.h`
//! define the operation order. This named compatibility profile uses unfused
//! native IEEE binary32/binary64 round-to-nearest-even with gradual underflow,
//! the bounded source sine/cosine adapter, and pinned libm 0.2.16 inverse
//! trigonometry and square root. It does not claim bit equivalence to every
//! platform's upstream C++ libm. Callers must select the compatibility profile;
//! this module grants no native-bit-parity or assembled-collision capability.

use benilla_sim::source_trig::sin_cos_radians_bits_native;
use std::fmt;

pub(crate) const POLICY: &str = "g3d-portable-libm-0.2.16-v1";

const PI: f32 = f32::from_bits(0x4049_0fdb);
const HALF_PI: f32 = f32::from_bits(0x3fc9_0fdb);
type Matrix = [[f32; 3]; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransformMathError {
    NonFiniteInput,
    ParentRotationOutsideDomain,
    QuaternionNormZero,
    QuaternionNormNonFinite,
    NonFiniteIntermediate,
    SourceTrigDomain,
}

impl fmt::Display for TransformMathError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::NonFiniteInput => "doodad transform input is not finite",
            Self::ParentRotationOutsideDomain => {
                "doodad parent rotation is outside inclusive -360..360 degrees"
            }
            Self::QuaternionNormZero => "doodad quaternion squared norm is zero or underflowed",
            Self::QuaternionNormNonFinite => "doodad quaternion squared norm overflowed",
            Self::NonFiniteIntermediate => "doodad transform arithmetic is not finite",
            Self::SourceTrigDomain => "doodad parent rotation exceeds the source trig domain",
        })
    }
}

impl std::error::Error for TransformMathError {}

/// Parent position is post-WMOInstance (permuted only after an emitting parent);
/// MODD local position and quaternion retain their raw source order;
/// parent rotation and the returned rotation are the source xyz degree fields.
/// Scale is deliberately absent: ExtractSet does not scale the local position.
///
/// This safety envelope rejects nonfinite inputs, degrees outside ±360, a norm²
/// that overflows or rounds to zero, and any nonfinite intermediate. A positive
/// subnormal norm² and harmless component underflow retain source arithmetic.
/// Signed zero is never canonicalized; there is no identity-quaternion fallback.
pub(crate) fn project(
    parent_position: [f32; 3],
    parent_rotation: [f32; 3],
    local_position: [f32; 3],
    quaternion: [f32; 4],
) -> Result<([f32; 3], [f32; 3]), TransformMathError> {
    if parent_position
        .iter()
        .chain(&parent_rotation)
        .chain(&local_position)
        .chain(&quaternion)
        .any(|value| !value.is_finite())
    {
        return Err(TransformMathError::NonFiniteInput);
    }
    let parent = parent_matrix(parent_rotation)?;
    let mut position = [0.0; 3];
    for (index, row) in parent.iter().enumerate() {
        let offset = dot3(*row, local_position)?;
        position[index] = finite(parent_position[index] + offset)?;
    }

    // ExtractSet multiplies local quaternion rotation on the LEFT of the parent.
    let combined = multiply(quaternion_matrix(quaternion)?, parent)?;
    let [x, y, z] = euler_xyz(combined)?;
    // toEulerAnglesXYZ(rotation.z, rotation.x, rotation.y), then float toDegrees.
    let rotation = [degrees(y)?, degrees(z)?, degrees(x)?];
    Ok((position, rotation))
}

fn finite(value: f32) -> Result<f32, TransformMathError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(TransformMathError::NonFiniteIntermediate)
    }
}

fn radians(value: f32) -> f32 {
    // G3D's float overload rounds BOTH operations, unlike the VMap f64 wrapper.
    let product = value * PI;
    product / 180.0_f32
}

fn degrees(value: f32) -> Result<f32, TransformMathError> {
    let product = finite(value * 180.0_f32)?;
    finite(product / PI)
}

fn sin_cos_degrees(value: f32) -> Result<[f32; 2], TransformMathError> {
    if !value.is_finite() {
        return Err(TransformMathError::NonFiniteInput);
    }
    if !(-360.0..=360.0).contains(&value) {
        return Err(TransformMathError::ParentRotationOutsideDomain);
    }
    sin_cos_radians_bits_native(radians(value).to_bits())
        .map(|values| values.map(f32::from_bits))
        .map_err(|_| TransformMathError::SourceTrigDomain)
}

fn parent_matrix(rotation: [f32; 3]) -> Result<Matrix, TransformMathError> {
    let [sy, cy] = sin_cos_degrees(rotation[0])?;
    let [sz, cz] = sin_cos_degrees(rotation[1])?;
    let [sx, cx] = sin_cos_degrees(rotation[2])?;
    let z = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    let y = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let x = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    multiply(z, multiply(y, x)?)
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> Result<f32, TransformMathError> {
    let first = finite(a[0] * b[0])?;
    let second = finite(a[1] * b[1])?;
    let third = finite(a[2] * b[2])?;
    let pair = finite(first + second)?;
    finite(pair + third)
}

fn multiply(a: Matrix, b: Matrix) -> Result<Matrix, TransformMathError> {
    let mut result = [[0.0; 3]; 3];
    for (index, row) in a.iter().enumerate() {
        for (column, entry) in result[index].iter_mut().enumerate() {
            *entry = dot3(*row, [b[0][column], b[1][column], b[2][column]])?;
        }
    }
    Ok(result)
}

fn quaternion_matrix(q: [f32; 4]) -> Result<Matrix, TransformMathError> {
    // G3D Quat::dot is (((x*x + y*y) + z*z) + w*w); no rescaling.
    let norm2 = ((q[0] * q[0] + q[1] * q[1]) + q[2] * q[2]) + q[3] * q[3];
    if !norm2.is_finite() {
        return Err(TransformMathError::QuaternionNormNonFinite);
    }
    if norm2 == 0.0 {
        return Err(TransformMathError::QuaternionNormZero);
    }
    // G3D rsq is exactly 1.0f / sqrtf(norm²), not a fast rsqrt or f64 norm.
    let length = finite(libm::sqrtf(norm2))?;
    let reciprocal = finite(1.0_f32 / length)?;
    let [x, y, z, w] = [
        finite(q[0] * reciprocal)?,
        finite(q[1] * reciprocal)?,
        finite(q[2] * reciprocal)?,
        finite(q[3] * reciprocal)?,
    ];

    // Keep (2*q_i)*q_j, not 2*(q_i*q_j), and left-associated subtraction.
    let xx = finite((2.0_f32 * x) * x)?;
    let xy = finite((2.0_f32 * x) * y)?;
    let xz = finite((2.0_f32 * x) * z)?;
    let xw = finite((2.0_f32 * x) * w)?;
    let yy = finite((2.0_f32 * y) * y)?;
    let yz = finite((2.0_f32 * y) * z)?;
    let yw = finite((2.0_f32 * y) * w)?;
    let zz = finite((2.0_f32 * z) * z)?;
    let zw = finite((2.0_f32 * z) * w)?;
    Ok([
        [finite((1.0 - yy) - zz)?, finite(xy - zw)?, finite(xz + yw)?],
        [finite(xy + zw)?, finite((1.0 - xx) - zz)?, finite(yz - xw)?],
        [finite(xz - yw)?, finite(yz + xw)?, finite((1.0 - xx) - yy)?],
    ])
}

fn atan2(y: f32, x: f32) -> Result<f32, TransformMathError> {
    // G3D aTan2 takes doubles; the separate source-facing f32 atan2 is not this.
    finite(libm::atan2(f64::from(y), f64::from(x)) as f32)
}

fn euler_xyz(matrix: Matrix) -> Result<[f32; 3], TransformMathError> {
    // G3D's branch comparisons also saturate finite roundoff outside [-1, 1].
    if matrix[0][2] < 1.0 {
        if matrix[0][2] > -1.0 {
            Ok([
                atan2(-matrix[1][2], matrix[2][2])?,
                finite(libm::asin(f64::from(matrix[0][2])) as f32)?,
                atan2(-matrix[0][1], matrix[0][0])?,
            ])
        } else {
            let x = -libm::atan2(f64::from(matrix[1][0]), f64::from(matrix[1][1]));
            Ok([finite(x as f32)?, -HALF_PI, 0.0])
        }
    } else {
        Ok([atan2(matrix[1][0], matrix[1][1])?, HALF_PI, 0.0])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const IDENTITY: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    #[test]
    fn identity_translates_without_reordering_position() {
        let (position, rotation) =
            project([10.0, -20.0, 30.0], [0.0; 3], [2.0, 3.0, -4.0], IDENTITY).unwrap();
        assert_eq!(position, [12.0, -17.0, 26.0]);
        assert_eq!(rotation.map(f32::to_bits), [0, 0x8000_0000, 0x8000_0000]);
    }

    #[test]
    fn parent_y_is_z_axis_and_output_y() {
        let (position, rotation) =
            project([0.0; 3], [0.0, 90.0, 0.0], [1.0, 0.0, 0.0], IDENTITY).unwrap();
        // Original-source fixture: wmo_doodad_transform_source/math_parent_z90.
        assert_eq!(position.map(f32::to_bits), [0xb33b_bd2e, 0x3f80_0000, 0]);
        assert_eq!(rotation.map(f32::to_bits), [0, 0x42b4_0000, 0x8000_0000]);
    }

    #[test]
    fn parent_x_hits_both_gimbal_branches() {
        for angle in [-90.0, 90.0] {
            let (_, rotation) = project([0.0; 3], [angle, 0.0, 0.0], [0.0; 3], IDENTITY).unwrap();
            assert_eq!(rotation[0], angle);
            assert_eq!(rotation[1].to_bits(), 0);
        }
    }

    #[test]
    fn local_quaternion_multiplies_on_the_left() {
        let (_, rotation) =
            project([0.0; 3], [90.0, 0.0, 0.0], [0.0; 3], [1.0, 0.0, 0.0, 0.0]).unwrap();
        // Original-source fixture math_gimbal_signed_zero: atan2(-0, -1) = -PI.
        assert_eq!(rotation.map(f32::to_bits), [0x42b4_0000, 0, 0xc334_0000]);
    }

    #[test]
    fn local_quaternion_is_normalized_without_rotating_its_position() {
        let identity = project([0.0; 3], [0.0; 3], [1.0, 2.0, 3.0], IDENTITY).unwrap();
        let scaled = project([0.0; 3], [0.0; 3], [1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 8.0]).unwrap();
        assert_eq!(identity.0.map(f32::to_bits), scaled.0.map(f32::to_bits));
        assert_eq!(identity.1.map(f32::to_bits), scaled.1.map(f32::to_bits));
        let (position, rotation) =
            project([0.0; 3], [0.0; 3], [1.0, 2.0, 3.0], [0.0, 0.0, 4.0, 4.0]).unwrap();
        assert_eq!(position, [1.0, 2.0, 3.0]);
        // Original-source fixture: wmo_doodad_transform_source/math_quat_z90_scaled.
        assert_eq!(rotation.map(f32::to_bits), [0, 0x42b3_ffff, 0x8000_0000]);
    }

    #[test]
    fn signed_zero_survives_conversions_and_inverse_trig() {
        assert_eq!(radians(-0.0).to_bits(), 0x8000_0000);
        assert_eq!(degrees(-0.0).unwrap().to_bits(), 0x8000_0000);
        assert_eq!(sin_cos_degrees(-0.0).unwrap()[0].to_bits(), 0x8000_0000);
        assert_eq!(atan2(-0.0, 1.0).unwrap().to_bits(), 0x8000_0000);
        assert_eq!(atan2(-0.0, -1.0).unwrap().to_bits(), (-PI).to_bits());
    }

    #[test]
    fn angle_domain_is_inclusive_and_does_not_wrap() {
        for angle in [-360.0, 360.0] {
            assert!(project([0.0; 3], [angle; 3], [0.0; 3], IDENTITY).is_ok());
        }
        for angle in [
            -f32::from_bits(360.0_f32.to_bits() + 1),
            f32::from_bits(360.0_f32.to_bits() + 1),
        ] {
            assert_eq!(
                project([0.0; 3], [angle, 0.0, 0.0], [0.0; 3], IDENTITY),
                Err(TransformMathError::ParentRotationOutsideDomain)
            );
        }
    }

    #[test]
    fn degenerate_and_overflowed_norms_are_rejected() {
        for quaternion in [[0.0; 4], [-0.0; 4], [f32::from_bits(1); 4]] {
            assert_eq!(
                project([0.0; 3], [0.0; 3], [0.0; 3], quaternion),
                Err(TransformMathError::QuaternionNormZero)
            );
        }
        assert_eq!(
            project([0.0; 3], [0.0; 3], [0.0; 3], [f32::MAX; 4]),
            Err(TransformMathError::QuaternionNormNonFinite)
        );
    }

    #[test]
    fn positive_subnormal_norm_and_harmless_component_underflow_are_accepted() {
        // (2^-70)^2 = 2^-140, an exactly representable positive subnormal norm².
        let tiny = f32::from_bits((127 - 70) << 23);
        assert!((tiny * tiny).is_subnormal());
        let (_, rotation) = project([0.0; 3], [0.0; 3], [0.0; 3], [0.0, 0.0, 0.0, tiny]).unwrap();
        // Power-of-two sqrt/reciprocal/products give exactly the identity here.
        assert_eq!(rotation.map(f32::to_bits), [0, 0x8000_0000, 0x8000_0000]);
        assert!(project(
            [0.0; 3],
            [0.0; 3],
            [0.0; 3],
            [f32::from_bits(1), 0.0, 0.0, 1.0],
        )
        .is_ok());
    }

    #[test]
    fn every_input_group_rejects_nonfinite_values() {
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            assert_eq!(
                project([bad; 3], [0.0; 3], [0.0; 3], IDENTITY),
                Err(TransformMathError::NonFiniteInput)
            );
            assert_eq!(
                project([0.0; 3], [bad; 3], [0.0; 3], IDENTITY),
                Err(TransformMathError::NonFiniteInput)
            );
            assert_eq!(
                project([0.0; 3], [0.0; 3], [bad; 3], IDENTITY),
                Err(TransformMathError::NonFiniteInput)
            );
            assert_eq!(
                project([0.0; 3], [0.0; 3], [0.0; 3], [bad; 4]),
                Err(TransformMathError::NonFiniteInput)
            );
        }
    }

    #[test]
    fn finite_extreme_positions_require_finite_intermediates() {
        let (position, _) = project([f32::MAX; 3], [0.0; 3], [0.0; 3], IDENTITY).unwrap();
        assert_eq!(position, [f32::MAX; 3]);
        assert_eq!(
            project([f32::MAX; 3], [0.0; 3], [f32::MAX; 3], IDENTITY),
            Err(TransformMathError::NonFiniteIntermediate)
        );
        // A later cancellation cannot rescue an overflowing left-associated sum.
        assert_eq!(
            dot3([1.0; 3], [f32::MAX, f32::MAX, -f32::MAX]),
            Err(TransformMathError::NonFiniteIntermediate)
        );
    }
}
