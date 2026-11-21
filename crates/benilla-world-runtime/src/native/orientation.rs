// SPDX-License-Identifier: GPL-3.0-or-later
//! Binary32 player orientation. Normalization is derived from CMaNGOS classic,
//! GPL-2.0-or-later, revision 8ec338a1704e7dcb1c0213eb7ed58f9231ade40f,
//! src/game/Maps/MapManager.h:121–132. See ORIENTATION.md for scope and notices.
use super::NativeError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A finite normalized binary32 angle in [0, 2*pi], encoded as exact bits.
/// Source normalization retains 2*pi for negative whole turns. Signed zero alone
/// is canonicalized to +0 so equivalent JSON inputs cannot encode a second zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Orientation(u32);
impl Orientation {
    pub const ZERO: Self = Self(0);
    pub fn new(radians: f32) -> Result<Self, NativeError> {
        if !radians.is_finite() {
            return Err(NativeError::InvalidInput);
        }
        let tau = std::f32::consts::TAU;
        let normalized = if radians < 0.0 {
            -(-radians % tau) + tau
        } else {
            radians % tau
        };
        Self::from_bits(if normalized == 0.0 {
            0
        } else {
            normalized.to_bits()
        })
    }
    /// Canonical stored angles are validated, never silently renormalized on decode.
    pub fn from_bits(bits: u32) -> Result<Self, NativeError> {
        if bits > std::f32::consts::TAU.to_bits() {
            return Err(NativeError::InvalidInput);
        }
        Ok(Self(bits))
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
    pub fn as_f32(self) -> f32 {
        f32::from_bits(self.0)
    }
}
impl Serialize for Orientation {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0)
    }
}
impl<'de> Deserialize<'de> for Orientation {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_bits(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
