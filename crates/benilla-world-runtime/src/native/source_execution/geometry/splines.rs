// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 MoveSpline.h:120-136; WaypointMovementGenerator.cpp:354-359.
use super::*;
use benilla_sim::source_spline::MoveSpline;

pub(super) fn spline_error(error: benilla_sim::source_spline::Error) -> Refusal {
    Refusal {
        id: format!("execution.spline.{}", error.0),
    }
}
impl MotionContext<'_> {
    pub fn spline(&self, unit: ObjectHandle) -> Result<&MoveSpline> {
        Ok(&self.units.get(self.objects, unit)?.movespline)
    }
    /// MoveSpline.h:123 reads done even before the first launch.
    pub fn spline_finalized(&self, unit: ObjectHandle) -> Result<bool> {
        Ok(self.spline(unit)?.finalized())
    }
    pub fn spline_position(&self, unit: ObjectHandle) -> Result<[f32; 4]> {
        let spline = self.spline(unit)?;
        require(
            spline.initialized(),
            "execution.waypoint.f6_spline_compute_position_owner",
        )?;
        Ok(spline
            .compute_position()
            .map_err(spline_error)?
            .map(f32::from_bits))
    }
    pub fn spline_current_index(&self, unit: ObjectHandle) -> Result<i32> {
        let spline = self.spline(unit)?;
        require(
            spline.initialized(),
            "execution.waypoint.f6_spline_current_index_owner",
        )?;
        spline.current_path_index().map_err(spline_error)
    }
    pub fn fixed_path_spline(&self, unit: ObjectHandle) -> Result<FixedPathSplineState> {
        let spline = self.spline(unit)?;
        require(
            spline.initialized(),
            "execution.fixed_path.f6_spline_state_owner",
        )?;
        Ok(FixedPathSplineState {
            destination: spline.final_destination().map(f32::from_bits),
            finalized: spline.finalized(),
        })
    }
    pub fn fixed_path_current_index(&self, unit: ObjectHandle) -> Result<i32> {
        let spline = self.spline(unit)?;
        require(
            spline.initialized(),
            "execution.fixed_path.f6_spline_state_owner",
        )?;
        spline.current_path_index().map_err(spline_error)
    }
    /// The transform's result is discarded by source cpp:359, but the call still executes.
    pub fn transform_waypoint_spline_start(
        &self,
        unit: ObjectHandle,
        start: [f32; 3],
        passengers: &SourcePassengers,
    ) -> Result<()> {
        if let Some(transport) = passengers.movement(unit)?.transport {
            let pose = self.objects.pose(transport)?;
            benilla_sim::source_transports::passenger_position_xyz(start.map(f32::to_bits), pose.0)
                .map_err(|_| refuse("execution.waypoint.passenger_position_numeric_domain_358"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use benilla_sim::source_spline::{Args, FINAL_POINT};
    struct Bound;
    impl MotionExecutionOwner for Bound {}
    #[test]
    fn constructor_reads_are_owned_but_launched_reads_still_refuse() {
        let mut objects = SourceObjects::default();
        let unit = objects.construct(1, SourceObjectKind::Creature).unwrap();
        let mut units = SourceUnits::default();
        units
            .register(
                &objects,
                unit,
                SourceUnitFields::creature(&super::super::motion_master_tests::template()),
            )
            .unwrap();
        let c = MotionContext {
            objects: &objects,
            units: &mut units,
        };
        assert!(Bound.spline_finalized(unit, &c).unwrap());
        assert_eq!(
            Bound.spline_compute_position(unit, &c).unwrap_err().id,
            "execution.waypoint.f6_spline_compute_position_owner"
        );
        assert_eq!(
            Bound.spline_current_index(unit, &c).unwrap_err().id,
            "execution.waypoint.f6_spline_current_index_owner"
        );
        assert_eq!(
            Bound.fixed_path_spline(unit, &c).unwrap_err().id,
            "execution.fixed_path.f6_spline_state_owner"
        );
        let args = Args {
            path: vec![[0; 3], [3f32.to_bits(), 4f32.to_bits(), 0]],
            facing: [9f32.to_bits(), 0, 0],
            flags: FINAL_POINT,
            velocity: 5f32.to_bits(),
            spline_id: 17,
            path_index_offset: 23,
        };
        let s = &mut c.units.get_mut(&objects, unit).unwrap().movespline;
        s.initialize(&args).unwrap();
        s.update_state(500, |_| {}).unwrap();
        assert!(!Bound.spline_finalized(unit, &c).unwrap());
        assert_eq!(Bound.spline_current_index(unit, &c).unwrap(), 23);
        let position = Bound.spline_compute_position(unit, &c).unwrap();
        assert_eq!(
            position.map(f32::to_bits),
            c.spline(unit).unwrap().compute_position().unwrap()
        );
        let fixed = Bound.fixed_path_spline(unit, &c).unwrap();
        assert_eq!(fixed.destination, [3., 4., 0.]);
        assert!(!fixed.finalized);
        assert_eq!(Bound.fixed_path_current_index(unit, &c).unwrap(), 23);
        c.units
            .get_mut(&objects, unit)
            .unwrap()
            .movespline
            .update_state(u32::MAX, |_| {})
            .unwrap();
        assert!(c.spline_finalized(unit).unwrap());
        assert_eq!(c.spline_current_index(unit).unwrap(), 24);
        assert_eq!(&c.spline_position(unit).unwrap()[..3], &[3., 4., 0.]);
        assert_eq!(
            MotionCall::StopSpline.missing_owner().id,
            "execution.motion.f6_spline_stop_owner"
        );
    }
    #[test]
    fn null_orientation_passenger_transform_does_not_normalize_a_facing_word() {
        use benilla_sim::source_transports::{passenger_position, passenger_position_xyz, Error};
        let offset = [1f32.to_bits(), 2f32.to_bits(), 3f32.to_bits()];
        let transport = [10f32.to_bits(), 20f32.to_bits(), 30f32.to_bits(), 0];
        assert_eq!(
            passenger_position_xyz(offset, transport).unwrap(),
            [11f32.to_bits(), 22f32.to_bits(), 33f32.to_bits()]
        );
        assert_eq!(
            passenger_position([offset[0], offset[1], offset[2], 0x7fc00000], transport),
            Err(Error::NumericDomain)
        );
    }
}
