// SPDX-License-Identifier: GPL-3.0-or-later
// Source: CMaNGOS Classic 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
// CMaNGOS contributors, GPL-2.0-or-later.
//! Only the original candidate's strict vertical test and target-eye arithmetic.
//! No LOS result, selector fallback, complete NearPoint or navigation is inferred.
use crate::facing::soft::S;

pub const POLICY: &str = "cmangos-8ec338a1-near-point-original-acceptance-soft-rne-v1";
/// LF-normalized Object.cpp, Object.h, ObjectPosSelector.cpp/.h, Map.cpp, World.cpp.
pub const SOURCE_SHA256: [&str; 6] = [
    "410904520fe2847a855c396d9fe442065268fd4ff5bb510633c3aadb5c54665b",
    "259cdb1a061258d6a8c3e977c743c8c83848c763353b504ca3e617580f58c128",
    "e4d85d02d78f975e5bf1912bb1a9d7677b5920ecee9fdf9c5928e4f6a5458b8b",
    "777f09cec2af7a5222383de444740a6c594fc5af4140362489d2556bef1b02f7",
    "b9ada3277b8f60dd93f6ceba7e85c1dffa129385fbb4d9043fd93771a9704d12",
    "405e2a0def584dc2e4a9a9bc2861aac4aacefcecc8ec2fa08856324e32d866dd",
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NonFinite,
    NegativeSize,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "source primary candidate numeric domain: {self:?}")
    }
}
impl std::error::Error for Error {}
fn finite(bits: u32) -> Result<S, Error> {
    if bits & 0x7f80_0000 == 0x7f80_0000 {
        Err(Error::NonFinite)
    } else {
        Ok(S::bits(bits))
    }
}
fn size(bits: u32) -> Result<S, Error> {
    let value = finite(bits)?;
    if value < S::ZERO {
        Err(Error::NegativeSize)
    } else {
        Ok(value)
    }
}
/// Preserve the left-associated binary32 radius additions and strict comparison.
/// Invalid ground is not rejected here: allowed-Z may retain initial Z in source.
pub fn vertical_acceptance_bits(
    initial_z: u32,
    adjusted_z: u32,
    distance: u32,
    searcher_radius: u32,
    target_radius: u32,
) -> Result<bool, Error> {
    let difference = finite(initial_z)? - finite(adjusted_z)?;
    let difference = S::bits(difference.raw() & 0x7fff_ffff);
    let limit = (size(distance)? + size(searcher_radius)?) + size(target_radius)?;
    Ok(difference < limit)
}
/// WorldObject::IsWithinLOS starts at this target's eye; destination stays feet.
pub fn target_eye_bits(position: [u32; 3], collision_height: u32) -> Result<[u32; 3], Error> {
    finite(position[0])?;
    finite(position[1])?;
    let z = finite(position[2])? + size(collision_height)?;
    finite(z.raw())?;
    Ok([position[0], position[1], z.raw()])
}
