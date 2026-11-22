// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 TargetedMovementGenerator.cpp:1210-1347.
use super::{asin::sqrt, path_angle, s, S};
use rustc_apfloat::{ieee::Single, Float, Round, Status};
pub type Point = [u32; 3];
#[derive(Clone, Copy, Debug)]
pub struct Spline {
    pub finalized: bool,
    pub speed: u32,
    pub raw_index: i32,
    pub last: i32,
}
#[derive(Debug)]
pub enum PathError<E> {
    Owner(E),
    Undefined(&'static str),
    Numeric,
}
/// Each method computes from the actual live spline or geometry owner, not a recorded result.
pub trait PathOwner {
    type Error;
    fn owner_position(&mut self) -> Result<Point, Self::Error>;
    fn master_position(&mut self) -> Result<Point, Self::Error>;
    fn master_orientation(&mut self) -> Result<u32, Self::Error>;
    fn master_spline(&mut self) -> Result<Spline, Self::Error>;
    fn spline_point(&mut self, index: i32) -> Result<Point, Self::Error>;
    fn spline_time(&mut self, index: i32) -> Result<i32, Self::Error>;
    fn first_collision(
        &mut self,
        origin: Point,
        distance: u32,
        angle: u32,
    ) -> Result<Point, Self::Error>;
    fn pathfind(&mut self, from: Point, to: Point, use_mmap: bool) -> Result<(), Self::Error>;
    fn path(&self) -> &[Point];
    fn allowed_z(&mut self, point: &mut Point) -> Result<(), Self::Error>;
    fn walk_speed(&mut self) -> Result<u32, Self::Error>;
}
#[derive(Debug)]
pub struct FollowPath {
    pub points: Vec<Point>,
    pub speed: u32,
}
fn difference(a: Point, b: Point) -> [S; 3] {
    std::array::from_fn(|i| S::bits(a[i]) - S::bits(b[i]))
}
fn length(a: Point, b: Point) -> S {
    let v = difference(a, b);
    sqrt(v[0] * v[0] + v[1] * v[1] + v[2] * v[2])
}
fn lerp(a: Point, b: Point, factor: S) -> Point {
    std::array::from_fn(|i| (S::bits(a[i]) + (S::bits(b[i]) - S::bits(a[i])) * factor).raw())
}
fn signed(value: i32) -> S {
    S::bits(
        Single::from_i128_r(i128::from(value), Round::NearestTiesToEven)
            .value
            .to_bits() as u32,
    )
}
fn truncate(value: S) -> Option<i32> {
    let v = Single::from_bits(u128::from(value.raw())).to_i128_r(32, Round::TowardZero, &mut false);
    if v.status.intersects(Status::INVALID_OP | Status::OVERFLOW) {
        None
    } else {
        Some(v.value as i32)
    }
}
/// Execute BuildPath without launching a synthetic movement generator.
pub fn build_path<O: PathOwner>(
    owner: &mut O,
    slot: [u32; 2],
    last_angle: &mut u32,
    random: bool,
    mmap: bool,
) -> Result<FollowPath, PathError<O::Error>> {
    use PathError::{Numeric, Owner, Undefined};
    let spline = owner.master_spline().map_err(Owner)?;
    let mut angle = S::bits(owner.master_orientation().map_err(Owner)?);
    let owner_pos = owner.owner_position().map_err(Owner)?;
    let master_pos = owner.master_position().map_err(Owner)?;
    let mut result = FollowPath {
        points: Vec::new(),
        speed: (-1_f32).to_bits(),
    };
    if spline.finalized {
        angle = angle + S::bits(slot[0]);
        let next = owner
            .first_collision(master_pos, slot[1], angle.raw())
            .map_err(Owner)?;
        if length(master_pos, next) > s(0.1) {
            result.points.push(owner_pos);
            result.points.push(next);
            result.speed = owner.walk_speed().map_err(Owner)?;
        }
        return Ok(result);
    }
    let ahead = S::bits(spline.speed) / s(3.0) * s(10.0);
    let good = ahead - ahead / s(5.0);
    let mut distance = S::ZERO;
    let mut travel_time = 0_i32;
    let mut previous = master_pos;
    result.points.push(owner_pos);
    let start = spline
        .raw_index
        .checked_add(1)
        .ok_or(Undefined("path_index_overflow"))?;
    for index in start..=spline.last {
        let mut factor = s(1.0);
        let mut next = owner.spline_point(index).map_err(Owner)?;
        let dist = length(previous, next);
        if dist > ahead {
            factor = s(1.0) / (dist / (ahead - distance));
            next = lerp(previous, next, factor);
        }
        let dir = difference(next, previous);
        *last_angle = path_angle([dir[0].raw(), dir[1].raw()], slot[0], *last_angle, random)
            .map_err(|_| Numeric)?;
        let best = owner
            .first_collision(next, slot[1], *last_angle)
            .map_err(Owner)?;
        let from = *result.points.last().unwrap();
        owner.pathfind(from, best, mmap).map_err(Owner)?;
        let path = owner.path();
        // begin()+1 on an empty vector is undefined; TargetedMovementGenerator.cpp:1306.
        if path.is_empty() {
            return Err(Undefined("empty_path_iterator"));
        }
        let mut path_length = S::ZERO;
        for pair in path.windows(2) {
            path_length = path_length + length(pair[0], pair[1]);
        }
        if path_length > s(0.5) {
            distance = distance + path_length;
            let begin = result.points.len();
            let time = owner.spline_time(index).map_err(Owner)?;
            let next_time = time
                .checked_sub(travel_time)
                .ok_or(Undefined("travel_time_subtract_overflow"))?;
            travel_time = truncate(signed(travel_time) + signed(next_time) * factor)
                .ok_or(Undefined("travel_time_conversion"))?;
            result.points.extend_from_slice(&owner.path()[1..]);
            for point in &mut result.points[begin..] {
                owner.allowed_z(point).map_err(Owner)?;
            }
        }
        if distance > good {
            break;
        }
        previous = next;
        if index == i32::MAX {
            return Err(Undefined("path_index_overflow"));
        }
    }
    if distance > s(0.1) {
        let speed = distance / (signed(travel_time) / s(1000.0));
        let speed = if s(1.0) < speed { speed } else { s(1.0) };
        let limit = S::bits(spline.speed) * s(1.5);
        result.speed = if speed < limit {
            speed.raw()
        } else {
            limit.raw()
        };
    }
    Ok(result)
}
