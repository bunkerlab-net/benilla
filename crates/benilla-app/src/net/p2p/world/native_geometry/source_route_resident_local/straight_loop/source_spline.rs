// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounds the source's successful software-RNE linear positions, not the separate
//! normalized solver sweep. Only the current sealed two-point SamePolygon arm calls it.
use super::*;
use benilla_sim::{creature_ai::Point, creature_motion::linear_segment_enclosure};

pub(super) fn certify(
    local: &mut LocalCorridor<'_, '_, '_>,
    from: [f32; 3],
    to: [f32; 3],
    launch: [f32; 3],
    body: Body,
    floor: f32,
    upper: f32,
) -> Result<(), Failure> {
    let plan = SourceSplinePlan::new(from, to, launch, body, floor, upper)?;
    let enclosure = local.certify_enclosure(plan.floor_point, plan.proof_body)?;
    let (floor_point, proof_body) = enclosure.operands();
    if floor_point.map(f32::to_bits) != plan.floor_point.map(f32::to_bits)
        || proof_body != plan.proof_body
    {
        return Err(Failure::Binding);
    }
    let proved = checked_enclosure_bounds(floor_point, proof_body)?;
    if !proved.contains(plan.source_swept) {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    let local = enclosure.local();
    // Retain the actual terrain-inclusive sweep. Its normalized affine endpoint is
    // NOT the source spline. Only this independently proved source-body occupancy
    // permits adjudicating a checked-terrain hit, never a nonterrain hit or TOI epsilon.
    if let Some(hit) = local
        .collision
        .cast_body(&plan.actual, plan.center, plan.movement, 0.0)
    {
        if hit.entity != enclosure.terrain_root() {
            return Err(Failure::Obstacle);
        }
        eprintln!(
            "resident source-spline terrain hit adjudicated: root={:?} source-RNE-position-enclosure=passed full-square-clearance=passed normalized-solver-parity=not-claimed",
            hit.entity
        );
    }
    if let Some((direction, distance)) = plan.ray {
        if local
            .spatial
            .cast_ray(
                plan.head,
                direction,
                distance,
                true,
                &WorldCollision::conservative_los_filter(),
            )
            .is_some()
        {
            return Err(Failure::Obstacle);
        }
    }
    Ok(())
}

struct SourceSplinePlan {
    floor_point: [f32; 3],
    proof_body: Body,
    source_swept: Bounds,
    actual: Collider,
    center: Vec3,
    movement: Vec3,
    head: Vec3,
    ray: Option<(Dir3, f32)>,
}

impl SourceSplinePlan {
    fn new(
        from: [f32; 3],
        to: [f32; 3],
        launch: [f32; 3],
        body: Body,
        floor: f32,
        upper: f32,
    ) -> Result<Self, Failure> {
        if launch.map(f32::to_bits) != from.map(f32::to_bits) {
            return Err(Failure::Binding);
        }
        if !floor.is_finite() || !upper.is_finite() || floor > upper {
            return Err(Failure::Invalid);
        }
        let point = |v: [f32; 3]| -> Result<Point, Failure> {
            let p = Point::new(v[0], v[1], v[2])
                .map_err(|_| Failure::UnsupportedPointRepresentation)?;
            if [p.x.bits(), p.y.bits(), p.z.bits()] != v.map(f32::to_bits) {
                return Err(Failure::UnsupportedPointRepresentation);
            }
            Ok(p)
        };
        let range = linear_segment_enclosure(point(from)?, point(to)?)
            .map_err(|_| Failure::UnsupportedPointRepresentation)?;
        let xyz = |p: Point| [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()];
        // wow_to_bevy reverses horizontal axes; re-order after that exact sign/permutation.
        let endpoints = range.map(|p| wow_to_bevy(xyz(p)));
        let feet_low = endpoints[0].min(endpoints[1]);
        let feet_high = endpoints[0].max(endpoints[1]);
        if feet_low.y < floor || feet_high.y > upper {
            return Err(Failure::UnsupportedGround);
        }
        let radius = body.radius.as_f32();
        let height = body.height.as_f32();
        if !(radius > 0.0 && radius <= 4.0 && height > 0.0 && height <= 16.0) {
            return Err(Failure::UnresolvedBody);
        }
        let actual = Collider::cuboid(radius * 2.0, height, radius * 2.0);
        let half = half_extents(&actual)?;
        if vec_bits(half) != vec_bits(Vec3::new(radius, height * 0.5, radius)) {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        // Each rounded center/head addition is monotone. Thus these extrema enclose
        // every successful source position and its actual cuboid, not only sample ticks.
        let centers = [
            feet_low + Vec3::Y * (height * 0.5),
            feet_high + Vec3::Y * (height * 0.5),
        ];
        let heads = [feet_low + Vec3::Y * height, feet_high + Vec3::Y * height];
        let mut source_swept = Bounds::cuboid(centers[0], half)?;
        source_swept.include(Bounds::cuboid(centers[1], half)?);
        source_swept.include(Bounds::point(heads[0])?);
        source_swept.include(Bounds::point(heads[1])?);
        if source_swept.min[1] < f64::from(floor) {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        // Expand to H only for the independent source-bound gap/nonterrain proof.
        let mut swept = source_swept;
        swept.min[1] = f64::from(floor);
        let extent = power_of_two_ceiling(exact_sum(swept.max[1], -f64::from(floor))?)?;
        let mut base = Vec3::new(0.0, floor, 0.0);
        let mut needed_radius = f64::from(radius);
        for axis in [0, 2] {
            let low = down32(swept.min[axis])?;
            let high = up32(swept.max[axis])?;
            base[axis] = (exact_sum(f64::from(low), f64::from(high))? * 0.5) as f32;
            needed_radius = needed_radius
                .max(exact_sum(f64::from(base[axis]), -f64::from(low))?)
                .max(exact_sum(f64::from(high), -f64::from(base[axis]))?);
        }
        let proof_radius = up32(needed_radius)?;
        let proof_body = Body {
            radius: Real::new(proof_radius).map_err(|_| Failure::UnsupportedPointRepresentation)?,
            height: Real::new(extent).map_err(|_| Failure::UnsupportedPointRepresentation)?,
        };
        let floor_point = [-base.z, -base.x, floor];
        if wow_to_bevy(floor_point) != base {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        let proof_bounds = checked_enclosure_bounds(floor_point, proof_body)?;
        let radius3 = Vec3::new(proof_radius, 0.0, proof_radius);
        let membership = Bounds::manual(base - radius3, base + radius3 + Vec3::Y * extent)?;
        if !proof_bounds.contains(swept) || !membership.contains(proof_bounds) {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        let feet = wow_to_bevy(from);
        let movement = wow_to_bevy(to) - feet;
        Ok(Self {
            floor_point,
            proof_body,
            source_swept,
            actual,
            center: feet + Vec3::Y * (height * 0.5),
            movement,
            head: feet + Vec3::Y * height,
            ray: checked_ray(movement)?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const FROM: [u32; 3] = [3_313_896_757, 3_313_872_078, 3_259_450_747];
    const TO: [u32; 3] = [3_313_918_466, 3_313_872_738, 3_259_498_496];
    const H: f32 = -50.0;
    const Q: f32 = f32::from_bits(3_259_428_591);
    fn body() -> Body {
        Body {
            radius: Real::new(0.5).unwrap(),
            height: Real::new(3.59375).unwrap(),
        }
    }

    #[test]
    fn actual_sixth_source_positions_are_enclosed_without_normalized_solver_parity() {
        let (from, to) = (FROM.map(f32::from_bits), TO.map(f32::from_bits));
        let plan = SourceSplinePlan::new(from, to, from, body(), H, Q).unwrap();
        assert_eq!(plan.source_swept.min[1], f64::from(H));
        assert!(checked_enclosure_bounds(plan.floor_point, plan.proof_body)
            .unwrap()
            .contains(plan.source_swept));
        assert!(matches!(
            Plan::new(from, to, body(), H, Q),
            Err(Failure::UnsupportedPointRepresentation)
        ));
        let p = |v: [f32; 3]| Point::new(v[0], v[1], v[2]).unwrap();
        let mut spline = benilla_sim::creature_motion::LinearSpline::launch(
            vec![p(from), p(to)],
            p(from),
            Real::new(7.0).unwrap(),
        )
        .unwrap();
        for _ in 0..=spline.duration_ms() {
            let current = spline.position().unwrap();
            let foot = wow_to_bevy([current.x.as_f32(), current.y.as_f32(), current.z.as_f32()]);
            let center = foot + Vec3::Y * (body().height.as_f32() * 0.5);
            assert!(plan
                .source_swept
                .contains(Bounds::cuboid(center, half_extents(&plan.actual).unwrap()).unwrap()));
            assert!(plan
                .source_swept
                .contains(Bounds::point(foot + Vec3::Y * body().height.as_f32()).unwrap()));
            spline.advance(1);
        }
    }
    #[test]
    fn ascending_descending_and_stationary_source_intervals_are_enclosed() {
        for (a, b) in [
            ([0.0, 0.0, H], [1.0, 2.0, -49.75]),
            ([1.0, 2.0, -49.75], [0.0, 0.0, H]),
            ([1.0, 2.0, H], [1.0, 2.0, H]),
        ] {
            let plan = SourceSplinePlan::new(a, b, a, body(), H, Q).unwrap();
            assert!(checked_enclosure_bounds(plan.floor_point, plan.proof_body)
                .unwrap()
                .contains(plan.source_swept));
        }
    }
    #[test]
    fn a_foreign_launch_origin_is_not_the_certified_segment() {
        let (from, to) = (FROM.map(f32::from_bits), TO.map(f32::from_bits));
        let mut launch = from;
        launch[0] = launch[0].next_up();
        assert!(matches!(
            SourceSplinePlan::new(from, to, launch, body(), H, Q),
            Err(Failure::Binding)
        ));
    }
    #[test]
    fn endpoint_reconstruction_and_real_domain_are_not_repaired() {
        assert!(matches!(
            SourceSplinePlan::new(
                [10_000.0, 0.0, H],
                [f32::from_bits(0x3580_0000), 0.0, H],
                [10_000.0, 0.0, H],
                body(),
                H,
                Q
            ),
            Err(Failure::UnsupportedPointRepresentation)
        ));
        for bad in [f32::from_bits(1), -0.0, f32::INFINITY] {
            assert!(SourceSplinePlan::new(
                [bad, 0.0, H],
                [1.0, 0.0, H],
                [bad, 0.0, H],
                body(),
                H,
                Q
            )
            .is_err());
        }
    }
    #[test]
    fn rounded_body_center_below_floor_cannot_use_source_feet_as_clearance() {
        let body = Body {
            radius: Real::new(0.5).unwrap(),
            height: Real::new(0.1).unwrap(),
        };
        assert!(matches!(
            SourceSplinePlan::new([0.0, 0.0, H], [1.0, 0.0, H], [0.0, 0.0, H], body, H, Q),
            Err(Failure::UnsupportedPointRepresentation)
        ));
    }
    #[test]
    fn source_endpoint_outside_proved_band_refuses() {
        for z in [H.next_down(), Q.next_up()] {
            assert!(matches!(
                SourceSplinePlan::new([0.0, 0.0, z], [1.0, 0.0, H], [0.0, 0.0, z], body(), H, Q),
                Err(Failure::UnsupportedGround)
            ));
        }
    }
}
