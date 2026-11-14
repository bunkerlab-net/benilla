// SPDX-License-Identifier: GPL-3.0-or-later
// Authored analytic geometry, not a production F5/F6 owner.
use benilla_sim::source_formations::motion::{build_path, PathError, PathOwner, Point, Spline};
use std::fmt::Write;
fn point(x: f32, y: f32, z: f32) -> Point {
    [x.to_bits(), y.to_bits(), z.to_bits()]
}
struct Host {
    id: u32,
    points: Vec<Point>,
    path: Vec<Point>,
    calls: Vec<&'static str>,
    empty: bool,
    refuse_collision: bool,
    raw_index: i32,
    final_index: Option<i32>,
    time: Option<i32>,
    time_sequence: Option<[i32; 3]>,
}
impl Host {
    fn new(id: u32) -> Self {
        let points = match id {
            4 => vec![
                point(0.0, 0.0, 0.0),
                point(0.1, 0.1, 0.0),
                point(0.2, 0.2, 0.0),
                point(0.3, 0.3, 0.0),
            ],
            5 => vec![
                point(0.0, 0.0, 0.0),
                point(-2.0, 1.0, 0.0),
                point(-4.0, 2.0, 0.0),
                point(-6.0, 3.0, 0.0),
            ],
            6 => vec![point(0.0, 0.0, 0.0)],
            _ => vec![
                point(0.0, 0.0, 0.0),
                point(2.0, 1.0, 0.0),
                point(4.0, 2.0, 0.0),
                point(100.0, 20.0, 0.0),
            ],
        };
        Self {
            id,
            points,
            path: vec![],
            calls: vec![],
            empty: false,
            refuse_collision: false,
            raw_index: 0,
            final_index: None,
            time: None,
            time_sequence: None,
        }
    }
}
impl PathOwner for Host {
    type Error = &'static str;
    fn owner_position(&mut self) -> Result<Point, Self::Error> {
        self.calls.push("owner-position");
        Ok(if self.id.is_multiple_of(2) {
            point(-2.0, -3.0, 0.0)
        } else {
            point(0.25, 0.5, 0.0)
        })
    }
    fn master_position(&mut self) -> Result<Point, Self::Error> {
        self.calls.push("master-position");
        Ok(point(0.0, 0.0, 0.0))
    }
    fn master_orientation(&mut self) -> Result<u32, Self::Error> {
        self.calls.push("orientation");
        Ok(0.25_f32.to_bits())
    }
    fn master_spline(&mut self) -> Result<Spline, Self::Error> {
        self.calls.push("spline");
        Ok(Spline {
            finalized: self.id < 2,
            speed: if self.id == 3 { 0.25_f32 } else { 3.0 }.to_bits(),
            raw_index: self.raw_index,
            last: self.final_index.unwrap_or(self.points.len() as i32 - 1),
        })
    }
    fn spline_point(&mut self, index: i32) -> Result<Point, Self::Error> {
        self.calls.push("point");
        Ok(self.points[index as usize % self.points.len()])
    }
    fn spline_time(&mut self, index: i32) -> Result<i32, Self::Error> {
        self.calls.push("time");
        Ok(self.time_sequence.map_or_else(
            || self.time.unwrap_or(index * 1000),
            |times| times[index as usize - 1],
        ))
    }
    fn first_collision(
        &mut self,
        origin: Point,
        distance: u32,
        angle: u32,
    ) -> Result<Point, Self::Error> {
        self.calls.push("collision");
        if self.refuse_collision {
            return Err("execution.formation.f6_first_collision_owner");
        }
        let [sin, cos] = benilla_sim::source_trig::sin_cos_radians_bits_native(angle)
            .unwrap()
            .map(f32::from_bits);
        let d = f32::from_bits(distance);
        let [x, y, z] = origin.map(f32::from_bits);
        Ok(point(x + d * cos, y + d * sin, z))
    }
    fn pathfind(&mut self, from: Point, to: Point, mmap: bool) -> Result<(), Self::Error> {
        self.calls.push(if mmap { "pathfind" } else { "append" });
        if mmap {
            self.path.clear();
            self.path.extend([from, to]);
        } else {
            self.path.push(to);
        }
        if self.empty {
            self.path.clear();
        }
        Ok(())
    }
    fn path(&self) -> &[Point] {
        &self.path
    }
    fn allowed_z(&mut self, p: &mut Point) -> Result<(), Self::Error> {
        self.calls.push("allowed-z");
        p[2] = 0;
        Ok(())
    }
    fn walk_speed(&mut self) -> Result<u32, Self::Error> {
        self.calls.push("walk-speed");
        Ok(2.5_f32.to_bits())
    }
}
#[test]
fn follow_build_path_matches_unchanged_cpp_over_analytic_geometry() {
    // TargetedMovementGenerator.cpp:1210-1347 with actual source spline arithmetic.
    let mut output = String::new();
    for id in 0..8 {
        let mut host = Host::new(id);
        let mut last = 0;
        let path = build_path(
            &mut host,
            [3_f32.to_bits(), if id == 1 { 0 } else { 2_f32.to_bits() }],
            &mut last,
            id.is_multiple_of(2),
            id != 7,
        )
        .unwrap();
        write!(
            output,
            "{id} {:08x} {last:08x} {}",
            path.speed,
            path.points.len()
        )
        .unwrap();
        for p in path.points {
            for x in p {
                write!(output, " {x:08x}").unwrap();
            }
        }
        output.push('\n');
    }
    assert_eq!(
        output,
        include_str!("../tools/fixtures/source_formation_path/expected.tsv")
    );
}
#[test]
fn collision_path_time_and_z_callbacks_follow_source_order() {
    // TargetedMovementGenerator.cpp:1220-1247,1265-1325.
    let mut host = Host::new(0);
    let mut last = 0;
    build_path(
        &mut host,
        [3_f32.to_bits(), 2_f32.to_bits()],
        &mut last,
        true,
        true,
    )
    .unwrap();
    assert_eq!(
        host.calls,
        [
            "spline",
            "orientation",
            "owner-position",
            "master-position",
            "collision",
            "walk-speed"
        ]
    );
    let mut host = Host::new(2);
    host.refuse_collision = true;
    assert!(matches!(
        build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true),
        Err(PathError::Owner(
            "execution.formation.f6_first_collision_owner"
        ))
    ));
    assert_eq!(
        host.calls,
        [
            "spline",
            "orientation",
            "owner-position",
            "master-position",
            "point",
            "collision"
        ]
    );
    let mut host = Host::new(2);
    build_path(
        &mut host,
        [3_f32.to_bits(), 2_f32.to_bits()],
        &mut last,
        true,
        true,
    )
    .unwrap();
    assert_eq!(
        &host.calls[4..9],
        ["point", "collision", "pathfind", "time", "allowed-z"]
    );
}
#[test]
fn empty_path_and_signed_path_index_boundaries_refuse_only_undefined_side() {
    // TargetedMovementGenerator.cpp:1262,1306 performs signed ++ and begin()+1.
    let mut host = Host::new(2);
    host.empty = true;
    let mut last = 0;
    assert!(matches!(
        build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true),
        Err(PathError::Undefined("empty_path_iterator"))
    ));
    assert_eq!(host.calls.last(), Some(&"pathfind"));
    let mut host = Host::new(6);
    assert_eq!(
        build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true)
            .unwrap()
            .points
            .len(),
        1
    );
    host.raw_index = i32::MAX;
    assert!(matches!(
        build_path(&mut host, [0, 0], &mut last, true, true),
        Err(PathError::Undefined("path_index_overflow"))
    ));
    host.raw_index = i32::MAX - 1;
    host.final_index = Some(i32::MAX - 1);
    assert!(build_path(&mut host, [0, 0], &mut last, true, true).is_ok());
}

#[test]
fn travel_time_conversion_refuses_after_time_but_before_allowed_z() {
    // TargetedMovementGenerator.cpp:1317-1325 converts float time to int32 before Z callbacks.
    let mut last = 0;
    let mut host = Host::new(2);
    host.time = Some(i32::MAX - 127);
    assert!(build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true).is_ok());
    let mut host = Host::new(2);
    host.time = Some(i32::MAX - 63);
    assert!(matches!(
        build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true),
        Err(PathError::Undefined("travel_time_conversion"))
    ));
    assert_eq!(host.calls.last(), Some(&"time"));
    assert!(!host.calls.contains(&"allowed-z"));
}

#[test]
fn signed_travel_time_subtraction_preserves_prior_segment_effects() {
    // TargetedMovementGenerator.cpp:1317 subtracts int32 before float scaling.
    let mut last = 0;
    let mut host = Host::new(2);
    host.time_sequence = Some([i32::MIN, -1, -1]);
    assert!(build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true).is_ok());
    let mut host = Host::new(2);
    host.time_sequence = Some([i32::MIN, 0, 0]);
    assert!(matches!(
        build_path(&mut host, [0, 1_f32.to_bits()], &mut last, true, true),
        Err(PathError::Undefined("travel_time_subtract_overflow"))
    ));
    assert_eq!(host.calls.last(), Some(&"time"));
    assert_eq!(
        host.calls
            .iter()
            .filter(|call| **call == "allowed-z")
            .count(),
        1
    );
}
