// SPDX-License-Identifier: GPL-3.0-or-later
// Derived from CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f:
// Movement/packet_builder.cpp::WriteLinearPath, Util/ByteBuffer.h::appendPackXYZ,
// and G3D/Vector3.h::squaredMagnitude. Copyright CMaNGOS contributors;
// source GPL-2.0-or-later, Rust translation GPL-3.0-or-later. WITHOUT ANY WARRANTY.
//! The source launch packet drops short endpoint offsets and quantizes the rest.
//! These decoded presentation vertices never replace the authoritative full path.
use super::{ai, point, source, Error};
use rustc_apfloat::{ieee::Single, Float, Round, Status};

const ROUND: Round = Round::NearestTiesToEven;
const QUARTER: u128 = 0x3e80_0000;
fn coordinates(point: ai::Point) -> [Single; 3] {
    [point.x, point.y, point.z].map(|value| Single::from_bits(u128::from(value.bits())))
}

pub(super) fn packed_offsets(path: &[ai::Point]) -> Result<Vec<u32>, Error> {
    if !(2..=source::MAX_PATH_POINTS).contains(&path.len())
        || path.iter().any(|point| {
            [point.x, point.y, point.z]
                .iter()
                .any(|coordinate| coordinate.bits() & 0x7fff_ffff > 0x46ff_f000)
        })
    {
        return Err(Error::Unrepresentable);
    }
    let endpoint = coordinates(*path.last().unwrap());
    let mut packed = Vec::with_capacity(path.len() - 2);
    for intermediate in &path[1..path.len() - 1] {
        let vertex = coordinates(*intermediate);
        let offset: [Single; 3] =
            std::array::from_fn(|axis| endpoint[axis].sub_r(vertex[axis], ROUND).value);
        let squares = offset.map(|value| value.mul_r(value, ROUND).value);
        let magnitude = squares[0]
            .add_r(squares[1], ROUND)
            .value
            .add_r(squares[2], ROUND)
            .value;
        if magnitude < Single::from_bits(0x3f00_0000) {
            continue;
        }
        let mut word = 0;
        for ((value, mask), shift) in offset
            .into_iter()
            .zip([0x7ff, 0x7ff, 0x3ff])
            .zip([0, 11, 22])
        {
            // Source casts x / .25f to int, truncating toward zero, then masks.
            // ±32760 coordinates bound this conversion well inside signed i32.
            let integer = value
                .div_r(Single::from_bits(QUARTER), ROUND)
                .value
                .to_i128_r(32, Round::TowardZero, &mut false);
            if !(integer.status & !Status::INEXACT).is_empty() {
                return Err(Error::Unrepresentable);
            }
            word |= ((integer.value as i32 as u32) & mask) << shift;
        }
        packed.push(word);
    }
    Ok(packed)
}

pub(super) fn linear_path(path: &[ai::Point]) -> Result<Vec<[f32; 3]>, Error> {
    let packed = packed_offsets(path)?;
    let endpoint = *path.last().unwrap();
    let endpoint_values = coordinates(endpoint);
    let mut decoded = Vec::with_capacity(packed.len() + 2);
    decoded.push(point(path[0]));
    for word in packed {
        // Sign extension matches benilla-protocol::wire::packed_to_vector3d.
        // Long offsets wrap in the source's 11/11/10-bit fields; do not clamp.
        let signed = [
            ((word << 21) as i32) >> 21,
            ((word << 10) as i32) >> 21,
            (word as i32) >> 22,
        ];
        decoded.push(std::array::from_fn(|axis| {
            let offset = Single::from_i128(i128::from(signed[axis]))
                .value
                .mul_r(Single::from_bits(QUARTER), ROUND)
                .value;
            f32::from_bits(endpoint_values[axis].sub_r(offset, ROUND).value.to_bits() as u32)
        }));
    }
    decoded.push(point(endpoint));
    Ok(decoded)
}
