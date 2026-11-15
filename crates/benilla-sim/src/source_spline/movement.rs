// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 Movement/MoveSpline.cpp:31-162,210-281,304-329.
use super::*;

pub const DONE: u32 = 1;
pub const FALLING: u32 = 2;
pub const FLYING: u32 = 0x200;
pub const FINAL_POINT: u32 = 0x10000;
pub const FINAL_TARGET: u32 = 0x20000;
pub const FINAL_ANGLE: u32 = 0x40000;
pub const CYCLIC: u32 = 0x100000;
pub const ENTER_CYCLE: u32 = 0x200000;
const FACING: u32 = FINAL_POINT | FINAL_TARGET | FINAL_ANGLE;

/// Numerical operands already prepared by MoveSplineInit. This does not implement Launch.
#[derive(Clone, Debug)]
pub struct Args {
    pub path: Vec<Point>,
    /// FacingInfo union storage. Target facing deliberately reads none of these words.
    pub facing: [u32; 3],
    pub flags: u32,
    pub path_index_offset: i32,
    pub velocity: u32,
    pub spline_id: u32,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum UpdateResult {
    None = 1,
    Arrived = 2,
    NextSegment = 8,
}
/// No Deserialize implementation: F7 must restore this mutable state, not run its constructor.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct MoveSpline {
    spline: Spline,
    facing: [Option<u32>; 3],
    id: u32,
    flags: u32,
    speed: u32,
    time_passed: u32,
    point_index: i32,
    point_index_offset: i32,
}
impl Default for MoveSpline {
    /// MoveSpline.cpp:159-162 and FacingInfo(), MoveSplineInitArgs.h:42.
    fn default() -> Self {
        Self {
            spline: Spline::default(),
            // FacingInfo::FacingInfo() initializes the uint64 target, not its trailing word.
            facing: [Some(0), Some(0), None],
            id: 0,
            flags: DONE,
            speed: 0,
            time_passed: 0,
            point_index: 0,
            point_index_offset: 0,
        }
    }
}
fn gravity() -> S {
    S::bits(19.291_105_f32.to_bits())
}
fn terminal_velocity() -> S {
    S::bits(60.148_003_f32.to_bits())
}
fn terminal_time() -> S {
    terminal_velocity() / gravity()
}
fn terminal_length() -> S {
    (terminal_velocity() * terminal_velocity()) / (integer(2) * gravity())
}
/// Movement/util.cpp:34-56, the unsafe-fall branch actually called by FallInitializer.
fn fall_time(length: S) -> S {
    if length < S::ZERO {
        S::ZERO
    } else if length >= terminal_length() {
        (length - terminal_length()) / terminal_velocity() + terminal_time()
    } else {
        sqrt(integer(2) * length / gravity())
    }
}
/// Movement/util.cpp:84-98, the one-argument overload actually called by MoveSpline.
fn fall_elevation(time: S) -> S {
    if time > terminal_time() {
        terminal_velocity() * (time - terminal_time()) + terminal_length()
    } else {
        time * time * gravity() * S::bits(0x3f00_0000)
    }
}
impl MoveSpline {
    /// MoveSpline.cpp:141-157. A stop clears only the spline, retaining speed and raw index.
    pub fn initialize(&mut self, args: &Args) -> Result<()> {
        self.flags = args.flags;
        self.facing = args.facing.map(Some);
        self.id = args.spline_id;
        self.point_index_offset = args.path_index_offset;
        self.time_passed = 0;
        if self.finalized() {
            self.spline.clear();
            return Ok(());
        }
        self.init_spline(args)
    }
    /// MoveSpline.cpp:104-139, CommonInitializer:92-102, FallInitializer:77-85,
    /// spline.h:188-205. Repair only the final timestamp, not each zero segment.
    fn init_spline(&mut self, args: &Args) -> Result<()> {
        self.spline.initialize_controls(
            &args.path,
            if args.flags & FLYING != 0 {
                Mode::CatmullRom
            } else {
                Mode::Linear
            },
            if args.flags & CYCLIC != 0 {
                Some(usize::from(self.flags & ENTER_CYCLE != 0))
            } else {
                None
            },
        )?;
        let mut time = 1;
        let velocity_inv = if self.flags & FALLING != 0 {
            S::ZERO
        } else {
            integer(1000) / S::bits(args.velocity)
        };
        let start_z = S::bits(self.spline.points[self.spline.first][2]);
        let mut previous = 0;
        self.spline.lengths.resize(self.spline.last + 1, 0);
        for i in self.spline.first..self.spline.last {
            let mut next = if self.flags & FALLING != 0 {
                truncate(
                    fall_time(start_z - S::bits(self.spline.points[i + 1][2])) * integer(1000),
                    "spline_fall_duration_conversion_undefined",
                )?
            } else {
                // Compound assignment converts time to float, adds, then converts the SUM to int32.
                time = truncate(
                    signed(time) + S::bits(self.spline.segment_length(i)?) * velocity_inv,
                    "spline_common_duration_conversion_undefined",
                )?;
                time
            };
            if next < 0 {
                next = i32::MAX;
            }
            self.spline.lengths[i + 1] = next;
            if previous > next {
                return Err(Error("spline_lengths_monotonic_assertion"));
            }
            previous = next;
        }
        if self.spline.total_length()? < 1 {
            self.spline.lengths[self.spline.last] = if self.spline.cyclic { 1000 } else { 1 };
        }
        self.point_index = self.spline.first as i32;
        self.speed = args.velocity;
        Ok(())
    }
    pub fn spline(&self) -> &Spline {
        &self.spline
    }
    pub fn id(&self) -> u32 {
        self.id
    }
    pub fn flags(&self) -> u32 {
        self.flags
    }
    pub fn speed(&self) -> u32 {
        self.speed
    }
    /// MoveSpline.h:100,123,135,88,93,95,136. Signed/unsigned conversions retain source bits.
    pub fn initialized(&self) -> bool {
        !self.spline.empty()
    }
    pub fn finalized(&self) -> bool {
        self.flags & DONE != 0
    }
    pub fn duration(&self) -> Result<u32> {
        Ok(self.spline.total_length()? as u32)
    }
    pub fn time_passed(&self) -> u32 {
        self.time_passed
    }
    pub fn raw_index(&self) -> i32 {
        self.point_index
    }
    pub fn elapsed_value(&self) -> Result<u32> {
        Ok((integer(self.time_passed) / integer(self.duration()?)).raw())
    }
    pub fn compute_time_to_index(&self, index: usize) -> Result<i32> {
        Ok((self.spline.length(index)? as u32).wrapping_sub(self.time_passed) as i32)
    }
    /// MoveSpline.cpp:311-321 and MoveSpline.h:131.
    pub fn final_destination(&self) -> Point {
        if !self.initialized() {
            [0; 3]
        } else {
            self.spline.points[self.spline.last - usize::from(self.flags & CYCLIC != 0)]
        }
    }
    pub fn current_destination(&self) -> Result<Point> {
        if !self.initialized() {
            return Ok([0; 3]);
        }
        self.spline
            .points
            .get((self.point_index + 1) as usize)
            .copied()
            .ok_or(Error("spline_current_destination_index_undefined"))
    }
    /// MoveSpline.cpp:323-329. Refuse each signed addition/subtraction before it overflows.
    pub fn current_path_index(&self) -> Result<i32> {
        let mut point = self
            .point_index_offset
            .checked_add(self.point_index)
            .ok_or(Error("spline_path_index_addition_undefined"))?
            .checked_sub(self.spline.first as i32)
            .ok_or(Error("spline_path_index_subtraction_undefined"))?
            .checked_add(i32::from(self.finalized()))
            .ok_or(Error("spline_path_index_final_addition_undefined"))?;
        if self.flags & CYCLIC != 0 {
            let divisor = self.spline.last as i32 - self.spline.first as i32;
            point = point
                .checked_rem(divisor)
                .ok_or(Error("spline_path_index_remainder_undefined"))?;
        }
        Ok(point)
    }
    /// MoveSpline.cpp:31-70, using the bound glibc atan2f port, not host libm.
    pub fn compute_position(&self) -> Result<[u32; 4]> {
        if !self.initialized() {
            return Err(Error("spline_position_initialized_assertion"));
        }
        let index = self.point_index as usize;
        let segment_time = self.spline.segment_time(index)?;
        let u = if segment_time > 0 {
            integer(
                self.time_passed
                    .wrapping_sub(self.spline.length(index)? as u32),
            ) / signed(segment_time)
        } else {
            integer(1)
        };
        let mut position = self.spline.evaluate(index, u.raw(), false)?;
        if self.flags & FALLING != 0 {
            // Movement/typedefs.h:42-45 divides by binary32 1000, rather than multiplying.
            let z = S::bits(self.spline.points[self.spline.first][2])
                - fall_elevation(integer(self.time_passed) / integer(1000));
            let final_z = S::bits(self.final_destination()[2]);
            position[2] = if z < final_z { final_z.raw() } else { z.raw() };
        }
        let angle = if self.finalized() && self.flags & FACING != 0 {
            if self.flags & FINAL_ANGLE != 0 {
                self.facing[0].ok_or(Error("spline_facing_angle_read_undefined"))?
            } else if self.flags & FINAL_POINT != 0 {
                atan2(
                    S::bits(self.facing[1].ok_or(Error("spline_facing_y_read_undefined"))?)
                        - S::bits(position[1]),
                    S::bits(self.facing[0].ok_or(Error("spline_facing_x_read_undefined"))?)
                        - S::bits(position[0]),
                )?
            } else {
                0
            }
        } else {
            let derivative = self.spline.evaluate(index, u.raw(), true)?;
            atan2(S::bits(derivative[1]), S::bits(derivative[0]))?
        };
        Ok([position[0], position[1], position[2], angle])
    }
    /// MoveSpline.h:104-118, do/while even for zero diff, one callback per source transition.
    pub fn update_state(
        &mut self,
        mut diff: u32,
        mut handler: impl FnMut(UpdateResult),
    ) -> Result<()> {
        if !self.initialized() {
            return Err(Error("spline_update_initialized_assertion"));
        }
        loop {
            handler(self.update_step(&mut diff)?);
            if diff == 0 {
                return Ok(());
            }
        }
    }
    /// MoveSpline.cpp:210-281. Cycles report NextSegment, never the unused NextCycle enum.
    pub fn update_step(&mut self, diff: &mut u32) -> Result<UpdateResult> {
        if self.finalized() {
            *diff = 0;
            return Ok(UpdateResult::Arrived);
        }
        let next = self.spline.length((self.point_index + 1) as usize)? as u32;
        let minimal = (*diff).min(next.wrapping_sub(self.time_passed));
        self.time_passed = self.time_passed.wrapping_add(minimal);
        *diff -= minimal;
        if self.time_passed < next {
            return Ok(UpdateResult::None);
        }
        self.point_index = self
            .point_index
            .checked_add(1)
            .ok_or(Error("spline_point_index_increment_undefined"))?;
        if self.point_index < self.spline.last as i32 {
            return Ok(UpdateResult::NextSegment);
        }
        if !self.spline.cyclic {
            self.finalize()?;
            *diff = 0;
            return Ok(UpdateResult::Arrived);
        }
        self.point_index = self.spline.first as i32;
        self.time_passed = self
            .time_passed
            .checked_rem(self.duration()?)
            .ok_or(Error("spline_cycle_duration_remainder_undefined"))?;
        if self.flags & ENTER_CYCLE != 0 {
            self.flags &= !ENTER_CYCLE;
            let mut args = Args {
                path: self.spline.points[self.spline.first + 1..self.spline.last].to_vec(),
                facing: self
                    .facing
                    .map(|word| word.expect("Initialize supplied FacingInfo")),
                flags: self.flags,
                path_index_offset: self.point_index_offset,
                velocity: integer(1).raw(),
                spline_id: self.id,
            };
            // MoveSplineInitArgs::Validate:177-178 is used here, not Launch.
            if args.path.len() > 1 {
                let mut temporary = Self::default();
                temporary.initialize(&args)?;
                args.velocity = (integer(temporary.duration()?) / integer(self.duration()?)).raw();
                if S::bits(args.velocity) > S::ZERO {
                    self.init_spline(&args)?;
                }
            }
        }
        Ok(UpdateResult::NextSegment)
    }
    /// MoveSpline.cpp:304-309; MoveSpline.h:94.
    pub fn finalize(&mut self) -> Result<()> {
        self.flags |= DONE;
        self.point_index = (self.spline.last as i32)
            .checked_sub(1)
            .ok_or(Error("spline_final_index_subtraction_undefined"))?;
        self.time_passed = self.duration()?;
        Ok(())
    }
    pub fn interrupt(&mut self) {
        self.flags |= DONE;
    }
}
fn atan2(y: S, x: S) -> Result<u32> {
    crate::facing::atan2_bits(y.raw(), x.raw()).map_err(|_| Error("spline_atan2_numeric_domain"))
}
