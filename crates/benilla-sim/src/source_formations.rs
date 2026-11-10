// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1, SpawnGroup.cpp:1541-1911; upstream GPL-2.0-or-later.
//! Formation offsets and variation only. Live slots, clocks and movement belong to runtime.
mod asin;
pub mod motion;
use crate::facing::soft::S;
use crate::source_rng::{RawGenerator, SourceRandom, TraceEvent};

pub trait Random {
    type Error;
    fn frand(&mut self, low: f32, high: f32) -> Result<f32, Self::Error>;
    fn urand(&mut self, low: u32, high: u32) -> Result<u32, Self::Error>;
}
impl<E: RawGenerator, T: FnMut(TraceEvent)> Random for SourceRandom<'_, E, T> {
    type Error = crate::source_rng::Error;
    fn frand(&mut self, low: f32, high: f32) -> Result<f32, Self::Error> {
        SourceRandom::frand(self, low, high)
    }
    fn urand(&mut self, low: u32, high: u32) -> Result<u32, Self::Error> {
        SourceRandom::urand(self, low, high)
    }
}
fn pi() -> S {
    S::bits(0x4049_0fdb)
}
fn s(value: f32) -> S {
    S::bits(value.to_bits())
}
fn f(value: S) -> f32 {
    f32::from_bits(value.raw())
}
fn integer(value: u32) -> S {
    use rustc_apfloat::{ieee::Single, Float, Round};
    S::bits(
        Single::from_u128_r(u128::from(value), Round::NearestTiesToEven)
            .value
            .to_bits() as u32,
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
pub struct Position {
    pub real_angle: u32,
    pub real_distance: u32,
    pub angle: u32,
    pub distance: u32,
    pub max_angle: u32,
    pub max_distance: u32,
    pub destination_angle: u32,
    pub destination_distance: u32,
}
impl Default for Position {
    fn default() -> Self {
        // SpawnGroup.h:128-132 initializes real distance to one but current distance to zero.
        Self {
            real_angle: 0,
            real_distance: 1_f32.to_bits(),
            angle: 0,
            distance: 0,
            max_angle: 0,
            max_distance: 0,
            destination_angle: 0,
            destination_distance: 0,
        }
    }
}
impl Position {
    pub fn set(&mut self, angle: u32, distance: u32) {
        self.real_angle = angle;
        self.angle = angle;
        self.real_distance = distance;
        self.distance = distance;
    }
    pub fn angle(&self, mirroring: bool, mirrored: bool) -> u32 {
        // Mirroring does not normalize zero back from TAU; SpawnGroup.cpp:1866-1875.
        if mirroring && mirrored {
            (s(2.0) * pi() - S::bits(self.angle)).raw()
        } else {
            self.angle
        }
    }
    pub fn add_variation<R: Random>(&mut self, now: bool, random: &mut R) -> Result<(), R::Error> {
        // Angle precedes distance, including equal bounds; SpawnGroup.cpp:1885-1890.
        let angle = S::bits(self.max_angle);
        let draw = s(random.frand(f(angle / s(100.0)), f(angle))?);
        self.destination_angle = (S::bits(self.real_angle) - angle / s(2.0) + draw).raw();
        let distance = S::bits(self.max_distance);
        let draw = s(random.frand(f(distance / s(100.0)), f(distance))?);
        self.destination_distance = (S::bits(self.real_distance) - distance / s(2.0) + draw).raw();
        if now {
            self.angle = self.destination_angle;
            self.distance = self.destination_distance;
        }
        Ok(())
    }
    pub fn update(&mut self) {
        // Negative differences snap rather than ease; SpawnGroup.cpp:1896-1910.
        for (current, destination, threshold) in [
            (&mut self.angle, self.destination_angle, s(0.01)),
            (&mut self.distance, self.destination_distance, s(0.1)),
        ] {
            let diff = S::bits(destination) - S::bits(*current);
            *current = if diff > threshold {
                (S::bits(*current) + diff / s(20.0)).raw()
            } else {
                destination
            };
        }
    }
}

pub type SlotOffset = (u32, u32, Option<[u32; 2]>);

/// One selected follower, after the caller's distinct count/filter passes.
/// Unknown shapes make no writes; SpawnGroup.cpp:1568-1853.
pub fn offset(
    shape: u32,
    spread: u32,
    width: u32,
    followers: f32,
    index: u32,
) -> Option<SlotOffset> {
    let spread = S::bits(spread);
    let width = S::bits(width);
    let (angle, distance, variation) = match shape {
        0 => {
            let inter = width * s(3.0);
            let (angle, distance) = match index {
                1 | 2 | 5 | 6 | 7 | 8 => {
                    let row = match index {
                        1 | 2 => 1,
                        5 | 6 => 2,
                        _ => 3,
                    };
                    let back = spread + inter * integer(row);
                    let hyp = asin::sqrt(back * back + inter * inter);
                    let delta = asin::asin(inter / hyp);
                    (
                        if index & 1 == 0 {
                            pi() + delta
                        } else {
                            pi() - delta
                        },
                        hyp,
                    )
                }
                3 => (pi(), spread + inter * s(2.0)),
                4 => (pi(), spread + inter * s(3.0)),
                9 => (pi(), spread + inter),
                _ => (
                    if index & 1 == 0 {
                        pi() / s(2.0) + pi()
                    } else {
                        pi() / s(2.0)
                    },
                    inter * integer((index.wrapping_sub(10) / 2) + 1),
                ),
            };
            (
                angle,
                distance,
                Some([(pi() / s(8.0) * width).raw(), inter.raw()]),
            )
        }
        1 => (pi(), spread * integer(index), None),
        2 => (
            if index & 1 == 0 {
                pi() / s(2.0) + pi()
            } else {
                pi() / s(2.0)
            },
            spread * integer((index.wrapping_sub(1) / 2) + 1),
            None,
        ),
        3 => (
            if index & 1 == 0 {
                pi() + pi() / s(4.0)
            } else {
                pi() - pi() / s(4.0)
            },
            spread * integer((index.wrapping_sub(1) / 2) + 1),
            None,
        ),
        4 => (
            pi() / s(2.0) + (pi() / s(followers)) * integer(index.wrapping_sub(1)),
            spread,
            None,
        ),
        5 => {
            let mut angle =
                pi() + pi() / s(2.0) + (pi() / s(followers)) * integer(index.wrapping_sub(1));
            if angle > pi() * s(2.0) {
                angle = angle - pi() * s(2.0);
            }
            (angle, spread, None)
        }
        6 => (
            ((pi() * s(2.0)) / s(followers)) * integer(index.wrapping_sub(1)),
            spread,
            None,
        ),
        _ => return None,
    };
    Some((angle.raw(), distance.raw(), variation))
}

/// Source moving-path angular easing; TargetedMovementGenerator.cpp:1278-1289.
pub fn path_angle(
    direction: [u32; 2],
    slot_angle: u32,
    previous: u32,
    random: bool,
) -> Result<u32, crate::facing::Error> {
    let angle =
        S::bits(crate::facing::atan2_bits(direction[1], direction[0])?) + S::bits(slot_angle);
    let diff = angle - S::bits(previous);
    let magnitude = S::bits(diff.raw() & 0x7fff_ffff);
    Ok(if magnitude < pi() / s(2.0) && magnitude > pi() / s(36.0) {
        (S::bits(previous) + diff / s(if random { 20.0 } else { 5.0 })).raw()
    } else {
        angle.raw()
    })
}
