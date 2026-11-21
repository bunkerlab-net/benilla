// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Object.h movementFlagsMask and Unit::IsMoving/IsWalking, ordinary ground subset.
use super::MotionError;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
/// Sequenced source XY/turn/walk movement flags, not displacement or renderer
/// animation. Flight, jumping, pitch, transport, root and forced-spline flags
/// require different authoritative state and are rejected by this dry subset.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroundMovementFlags(u32);
impl GroundMovementFlags {
    pub const STOPPED: Self = Self(0);
    pub fn from_bits(bits: u32) -> Result<Self, MotionError> {
        if bits & !0x13f != 0 {
            return Err(MotionError::Unsupported(
                "non-ground or authority-owned movement flags",
            ));
        }
        Ok(Self(bits))
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
    pub const fn is_moving(self) -> bool {
        self.0 & 0xf != 0
    }
    pub const fn is_walking(self) -> bool {
        self.0 & 0x100 != 0
    }
    pub const fn moving_and_running(self) -> bool {
        self.is_moving() && !self.is_walking()
    }
}
impl Serialize for GroundMovementFlags {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.0)
    }
}
impl<'de> Deserialize<'de> for GroundMovementFlags {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_bits(u32::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}
