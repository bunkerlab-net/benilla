// SPDX-License-Identifier: GPL-3.0-or-later
// Authored ownership/chronology only; numerical path cases live in benilla-sim.
use super::*;
use benilla_sim::source_formations::motion::{PathOwner, Point, Spline};
struct PathHost {
    follower: ObjectHandle,
    master: ObjectHandle,
    missing_spline: bool,
    calls: Vec<&'static str>,
}
impl FormationPathOwner for PathHost {
    fn formation_path_units(&self) -> (ObjectHandle, ObjectHandle) {
        (self.follower, self.master)
    }
}
impl PathOwner for PathHost {
    type Error = Refusal;
    fn owner_position(&mut self) -> Result<Point> {
        self.calls.push("owner");
        Ok([0; 3])
    }
    fn master_position(&mut self) -> Result<Point> {
        self.calls.push("master");
        Ok([0; 3])
    }
    fn master_orientation(&mut self) -> Result<u32> {
        self.calls.push("orientation");
        Ok(0)
    }
    fn master_spline(&mut self) -> Result<Spline> {
        self.calls.push("spline");
        if self.missing_spline {
            return Err(refuse("execution.formation.f5_live_spline_owner"));
        }
        Ok(Spline {
            finalized: true,
            speed: 0,
            raw_index: 0,
            last: 0,
        })
    }
    fn spline_point(&mut self, _: i32) -> Result<Point> {
        panic!("unreached moving spline")
    }
    fn spline_time(&mut self, _: i32) -> Result<i32> {
        panic!("unreached moving spline")
    }
    fn first_collision(&mut self, _: Point, _: u32, _: u32) -> Result<Point> {
        self.calls.push("collision");
        Err(refuse("execution.formation.f6_first_collision_owner"))
    }
    fn pathfind(&mut self, _: Point, _: Point, _: bool) -> Result<()> {
        panic!("unreached pathfinder")
    }
    fn path(&self) -> &[Point] {
        panic!("unreached pathfinder")
    }
    fn allowed_z(&mut self, _: &mut Point) -> Result<()> {
        panic!("unreached allowed-Z")
    }
    fn walk_speed(&mut self) -> Result<u32> {
        panic!("unreached walk speed")
    }
}
#[test]
fn real_follow_binding_refuses_missing_spline_then_geometry_without_canned_pose() {
    // TargetedMovementGenerator.cpp:1213-1239 consumes actual spline/poses before collision.
    let (mut f, mut host) = fixture(2, false, 0);
    attach(&mut f, &mut host, 1);
    let slot = attach(&mut f, &mut host, 2);
    let follow = FormationMotion::Follow {
        slot,
        master: object(1),
    };
    let mut path = PathHost {
        follower: object(2),
        master: object(1),
        missing_spline: true,
        calls: vec![],
    };
    let mut last = 0;
    assert_eq!(
        follow
            .build_path(object(2), &f, true, &mut last, &mut path)
            .unwrap_err()
            .id,
        "execution.formation.f5_live_spline_owner"
    );
    assert_eq!(path.calls, ["spline"]);
    path.calls.clear();
    path.missing_spline = false;
    assert_eq!(
        follow
            .build_path(object(2), &f, true, &mut last, &mut path)
            .unwrap_err()
            .id,
        "execution.formation.f6_first_collision_owner"
    );
    assert_eq!(
        path.calls,
        ["spline", "orientation", "owner", "master", "collision"]
    );
    path.calls.clear();
    path.master = object(9);
    assert_eq!(
        follow
            .build_path(object(2), &f, true, &mut last, &mut path)
            .unwrap_err()
            .id,
        "execution.formation.follow_path_binding"
    );
    assert!(path.calls.is_empty());
}
