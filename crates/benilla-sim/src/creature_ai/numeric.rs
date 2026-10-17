// SPDX-License-Identifier: GPL-3.0-or-later
// Source/attribution and domain: ../../CREATURE-AI.md and ../../PROVENANCE.md.
use super::AiError;
use rustc_apfloat::{
    ieee::{Double, Single},
    Float, FloatConvert, Round, Status, StatusAnd,
};

/// Finite bounded binary32. Arithmetic in this module uses software RNE, not the host FPU.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Real(u32);
impl Real {
    pub const ZERO: Self = Self(0);
    pub const ONE: Self = Self(0x3f80_0000);
    pub fn new(value: f32) -> Result<Self, AiError> {
        Self::from_bits(value.to_bits())
    }
    pub fn from_bits(bits: u32) -> Result<Self, AiError> {
        let exponent = bits & 0x7f80_0000;
        if exponent == 0x7f80_0000 || (exponent == 0 && bits & 0x7fff_ffff != 0) {
            return Err(AiError::Invalid("finite normal binary32"));
        }
        let value = if bits & 0x7fff_ffff == 0 { 0 } else { bits };
        if Single::from_bits(u128::from(value)).abs() > Single::from_bits(0x5368_d4a5) {
            return Err(AiError::Invalid("binary32 magnitude <= 1e12"));
        }
        Ok(Self(value))
    }
    pub const fn bits(self) -> u32 {
        self.0
    }
    pub fn as_f32(self) -> f32 {
        f32::from_bits(self.0)
    }
    pub(crate) fn value(self) -> Single {
        Single::from_bits(u128::from(self.0))
    }
    pub(crate) fn constant(bits: u32) -> Self {
        Self(bits)
    }
    pub(crate) fn integer(value: u32) -> Self {
        Self(Single::from_u128(u128::from(value)).value.to_bits() as u32)
    }
    fn checked(value: StatusAnd<Single>) -> Result<Self, AiError> {
        if !(value.status & !Status::INEXACT).is_empty() {
            return Err(AiError::Invalid("software binary32 exception"));
        }
        Self::from_bits(value.value.to_bits() as u32)
    }
    pub(crate) fn add(self, other: Self) -> Result<Self, AiError> {
        Self::checked(self.value().add_r(other.value(), Round::NearestTiesToEven))
    }
    pub(crate) fn sub(self, other: Self) -> Result<Self, AiError> {
        Self::checked(self.value().sub_r(other.value(), Round::NearestTiesToEven))
    }
    pub(crate) fn mul(self, other: Self) -> Result<Self, AiError> {
        Self::checked(self.value().mul_r(other.value(), Round::NearestTiesToEven))
    }
    pub(crate) fn div(self, other: Self) -> Result<Self, AiError> {
        Self::checked(self.value().div_r(other.value(), Round::NearestTiesToEven))
    }
    pub(crate) fn nonnegative(self, name: &'static str) -> Result<(), AiError> {
        if self.value() < Single::ZERO {
            Err(AiError::Invalid(name))
        } else {
            Ok(())
        }
    }
    pub(crate) fn greater(self, other: Self) -> bool {
        self.value() > other.value()
    }
    pub(crate) fn less(self, other: Self) -> bool {
        self.value() < other.value()
    }
}

/// Native WoW coordinates, not renderer-axis coordinates. No transport-local positions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Point {
    pub x: Real,
    pub y: Real,
    pub z: Real,
}
impl Point {
    pub fn new(x: f32, y: f32, z: f32) -> Result<Self, AiError> {
        let result = Self {
            x: Real::new(x)?,
            y: Real::new(y)?,
            z: Real::new(z)?,
        };
        result.validate()?;
        Ok(result)
    }
    pub(crate) fn validate(self) -> Result<(), AiError> {
        for v in [self.x, self.y, self.z] {
            if v.value().abs() > Real::constant(0x46ff_f000).value() {
                // 32760
                return Err(AiError::Invalid("world coordinate"));
            }
        }
        Ok(())
    }
    /// Software-binary32 squared world distance for deterministic runtime adapters.
    pub fn distance_squared(self, to: Self) -> Result<Real, AiError> {
        self.validate()?;
        to.validate()?;
        self.square_distance(to, true)
    }
    pub(crate) fn square_distance(self, to: Self, three_d: bool) -> Result<Real, AiError> {
        let x = self.x.sub(to.x)?;
        let y = self.y.sub(to.y)?;
        let xy = x.mul(x)?.add(y.mul(y)?)?;
        if !three_d {
            return Ok(xy);
        }
        let z = self.z.sub(to.z)?;
        xy.add(z.mul(z)?)
    }
}

/// Unit::GetAttackDistance, ordinary player target, without either detect-range aura.
/// Detection range and aggro rate must be resolved native values; neither is guessed.
pub fn attack_distance(
    creature_level: u8,
    player_level: u8,
    detection: Real,
    rate: Real,
) -> Result<Real, AiError> {
    if creature_level == 0 || player_level == 0 {
        return Err(AiError::Invalid("level"));
    }
    detection.nonnegative("detection")?;
    rate.nonnegative("aggro rate")?;
    if rate == Real::ZERO || detection == Real::ZERO {
        return Ok(Real::ZERO);
    }
    let difference = (i32::from(player_level) - i32::from(creature_level)).max(-25);
    let delta = Real::from_bits(Single::from_i128(i128::from(difference)).value.to_bits() as u32)?;
    let mut distance = detection.sub(delta)?;
    let minimum = Real::constant(0x40a0_0000); // 5.0f
    if distance.less(minimum) {
        distance = minimum;
    }
    distance.mul(rate)
}

/// WorldObject::GetCombinedCombatReach, the ordinary melee overload, flat_mod = 0.
pub fn combat_reach(creature: Real, target: Real) -> Result<Real, AiError> {
    creature.nonnegative("creature reach")?;
    target.nonnegative("target reach")?;
    let reach = creature.add(target)?.add(Real::constant(0x3faa_3d71))?; // 1.33f
    Ok(if reach.less(Real::constant(0x40a0_0000)) {
        Real::constant(0x40a0_0000)
    } else {
        reach
    })
}

/// Unit::CanReachWithMeleeAttack for an uncharmed NPC. Its distance check is 2D and strict.
/// Leeway is supplied only when BOTH bodies are moving and not walking, from native metadata.
pub fn in_melee_range(
    origin: Point,
    target: Point,
    reach: Real,
    running_leeway: Real,
) -> Result<bool, AiError> {
    origin.validate()?;
    target.validate()?;
    reach.nonnegative("reach")?;
    running_leeway.nonnegative("melee leeway")?;
    let reach = reach.add(running_leeway)?;
    Ok(origin
        .square_distance(target, false)?
        .less(reach.mul(reach)?))
}

/// The ordinary-player threshold branch of ThreatContainer::selectNextVictim.
/// Taunts, hostile suppression, nonplayer preference, offline refs and root suppression
/// are deliberately outside this numeric helper; an authority must resolve them first.
pub fn pulls_aggro(
    current: Real,
    challenger: Real,
    challenger_in_melee: bool,
) -> Result<bool, AiError> {
    current.nonnegative("current threat")?;
    challenger.nonnegative("challenger threat")?;
    let low = current.mul(Real::constant(0x3f8c_cccd))?; // 1.1f
    if !challenger.greater(low) {
        return Ok(false);
    }
    let high = current.mul(Real::constant(0x3fa6_6666))?; // 1.3f
    Ok(challenger.greater(high) || challenger_in_melee)
}

// Compare a * 2^ae and b * 2^be without host floating point or oversized shifts.
fn scaled_cmp(a: u128, ae: i32, b: u128, be: i32) -> std::cmp::Ordering {
    if a == 0 || b == 0 {
        return a.cmp(&b);
    }
    let ab = 128 - a.leading_zeros() as i32;
    let bb = 128 - b.leading_zeros() as i32;
    match (ab + ae).cmp(&(bb + be)) {
        std::cmp::Ordering::Equal => {
            if ae >= be {
                (a << (ae - be)).cmp(&b)
            } else {
                a.cmp(&(b << (be - ae)))
            }
        }
        order => order,
    }
}
fn double_parts(bits: u64) -> (u128, i32) {
    if bits == 0 {
        return (0, 0);
    }
    (
        u128::from((bits & 0x000f_ffff_ffff_ffff) | 0x0010_0000_0000_0000),
        ((bits >> 52) as i32) - 1023 - 52,
    )
}
// Correctly rounded binary64 sqrt of a nonnegative normal binary32 input. The
// source uses global sqrt(float), promoted to double, then subtracts in double.
fn sqrt_double(value: Real) -> Double {
    if value == Real::ZERO {
        return Double::ZERO;
    }
    let mut lost = false;
    let d: Double = value.value().convert(&mut lost).value;
    let exponent = ((value.bits() >> 23) & 255) as i32 - 127 - 23;
    let mantissa = u128::from((value.bits() & 0x7f_ffff) | 0x80_0000);
    let compare_square = |bits| {
        let (m, e) = double_parts(bits);
        scaled_cmp(m * m, 2 * e, mantissa, exponent)
    };
    let mut low = 0u64;
    let mut high = (d.to_bits() as u64).max(0x3ff0_0000_0000_0000);
    while high - low > 1 {
        let middle = low + (high - low) / 2;
        if compare_square(middle).is_gt() {
            high = middle;
        } else {
            low = middle;
        }
    }
    if compare_square(low).is_eq() {
        return Double::from_bits(u128::from(low));
    }
    let (m, e) = double_parts(low);
    let mid = m * 2 + 1;
    let midpoint = scaled_cmp(mid * mid, (e - 1) * 2, mantissa, exponent);
    let rounded = if midpoint.is_lt() || (midpoint.is_eq() && low & 1 != 0) {
        high
    } else {
        low
    };
    Double::from_bits(u128::from(rounded))
}

/// WorldObject::GetDistance2d's DEFAULT bounding-radius overload. CombatManager
/// uses this, not center distance: sqrt(binary32 dx²+dy²) - this object's radius.
pub fn distance_2d(origin: Point, to: Point, bounding_radius: Real) -> Result<Real, AiError> {
    origin.validate()?;
    to.validate()?;
    bounding_radius.nonnegative("bounding radius")?;
    let square = origin.square_distance(to, false)?;
    let mut lost = false;
    let radius: Double = bounding_radius.value().convert(&mut lost).value;
    let result = sqrt_double(square).sub_r(radius, Round::NearestTiesToEven);
    if !(result.status & !Status::INEXACT).is_empty() {
        return Err(AiError::Invalid("distance exception"));
    }
    let result: StatusAnd<Single> = result.value.convert(&mut lost);
    let result = Real::checked(result)?;
    Ok(if result.less(Real::ZERO) {
        Real::ZERO
    } else {
        result
    })
}

impl serde::Serialize for Real {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u32(self.bits())
    }
}
impl<'de> serde::Deserialize<'de> for Real {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::from_bits(<u32 as serde::Deserialize>::deserialize(deserializer)?)
            .map_err(serde::de::Error::custom)
    }
}

/// Player::GetNPCIfCanInteractWith -> WorldObject::_IsWithinDist, ordinary 3D path.
/// Source INTERACTION_DISTANCE is 5; the reach sum rounds before adding it.
pub fn within_interaction_distance(
    player: Point,
    npc: Point,
    player_reach: Real,
    npc_reach: Real,
) -> Result<bool, AiError> {
    player_reach.nonnegative("player reach")?;
    npc_reach.nonnegative("npc reach")?;
    let size = player_reach.add(npc_reach)?;
    let maximum = Real::constant(0x40a00000).add(size)?;
    Ok(player
        .square_distance(npc, true)?
        .less(maximum.mul(maximum)?))
}
