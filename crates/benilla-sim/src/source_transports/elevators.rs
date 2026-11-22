// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS 8ec338a1 Transports.cpp:468-481; G3D Modified BSD notice in licenses/G3D-NOTICE.cpp.txt.
use super::{finite, integer, lerp, numeric::sqrt_single, scalar, Error, Result, S};

/// The vector multiplies the normalized quaternion's matrix on the left, then flips Y.
pub fn elevator_position(
    previous: [u32; 3],
    next: [u32; 3],
    progress: u32,
    previous_time: u32,
    next_time: u32,
    rotation: [u32; 4],
    origin: [u32; 3],
) -> Result<[u32; 3]> {
    // Equal coordinates bypass the quotient, including distinct signed zeros; Transports.cpp:468-474.
    let equal = (0..3).all(|i| f32::from_bits(previous[i]) == f32::from_bits(next[i]));
    let position = if equal {
        previous
    } else {
        lerp(
            previous,
            next,
            integer(progress.wrapping_sub(previous_time))
                / integer(next_time.wrapping_sub(previous_time)),
        )?
    };
    let [x, y, z, w] = rotation.map(finite);
    let (qx, qy, qz, qw) = (x?, y?, z?, w?);
    // Matrix3 normalizes with 1/sqrtf(dot), not inverse length in double; Matrix3.cpp:117-136, Quat.h:158-159,318-319.
    let length = sqrt_single(((qx * qx + qy * qy) + qz * qz) + qw * qw)?;
    if length == S::ZERO {
        return Err(Error::NumericDomain);
    }
    let inverse = S::bits(0x3f80_0000) / length;
    let (x, y, z, w) = (qx * inverse, qy * inverse, qz * inverse, qw * inverse);
    let two = S::bits(0x4000_0000);
    let (xx, xy, xz, xw) = (two * x * x, two * x * y, two * x * z, two * x * w);
    let (yy, yz, yw) = (two * y * y, two * y * z, two * y * w);
    let (zz, zw) = (two * z * z, two * z * w);
    let one = S::bits(0x3f80_0000);
    let columns = [
        [one - yy - zz, xy + zw, xz - yw],
        [xy - zw, one - xx - zz, yz + xw],
        [xz + yw, yz - xw, one - xx - yy],
    ];
    let [px, py, pz] = position.map(finite);
    let (px, py, pz) = (px?, py?, pz?);
    let mut out = [0; 3];
    for axis in 0..3 {
        // Left-vector product keeps the source's two additions; Matrix3.h:349-356.
        let [a, b, c] = columns[axis];
        let value = (px * a + py * b) + pz * c;
        out[axis] = scalar((if axis == 1 { -value } else { value }) + finite(origin[axis])?)?;
    }
    Ok(out)
}
