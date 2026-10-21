// SPDX-License-Identifier: GPL-3.0-or-later
//! Conservative proof geometry only. The caller must already hold the sealed source support
//! capability for H/Q; these numbers cannot establish generation or ground authority here.
use super::super::super::{
    core::{Body, Corridor, Failure},
    local_corridor::LocalCorridor,
};
use avian3d::prelude::{Collider, ColliderAabb, SimpleCollider};
use benilla_assets::coords::wow_to_bevy;
use benilla_sim::creature_ai::Real;
use benilla_world::collision::WorldCollision;
use bevy::prelude::{Dir3, Quat, Vec3};

/// Native points and initialized Body remain untouched. Only this private enclosure is
/// projected to the proved raw plane. Source M2 LOS and final freshness remain the caller's.
pub(super) fn certify(
    local: &mut LocalCorridor<'_, '_, '_>,
    from: [f32; 3],
    to: [f32; 3],
    body: Body,
    floor: f32,
    upper: f32,
) -> Result<(), Failure> {
    let plan = Plan::new(from, to, body, floor, upper)?;
    local.certify(plan.floor_points[0], plan.floor_points[1], plan.proof_body)?;
    // The stationary enclosure cannot replace the native descending-facing law. In
    // particular this cast includes the supporting terrain, with no floor exception.
    if local
        .collision
        .cast_body(&plan.actual, plan.center, plan.movement, 0.0)
        .is_some()
    {
        return Err(Failure::Obstacle);
    }
    if let Some((direction, distance)) = plan.ray {
        // The stationary proof has no top ray. The actual head-height LOS remains
        // mandatory, including camera-only WMO DETAIL faces (LocalCorridor's same union).
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

struct Plan {
    floor_points: [[f32; 3]; 2],
    proof_body: Body,
    actual: Collider,
    center: Vec3,
    movement: Vec3,
    head: Vec3,
    ray: Option<(Dir3, f32)>,
    #[cfg(test)]
    bounds: (Vec3, Vec3),
}

/// Copied diagnostic bounds only, never generation/support authority. Fixtures use these
/// actual conservative squares to place refusal-only geometry outside earlier segments.
#[cfg(test)]
pub(super) fn bounds_for_test(
    from: [f32; 3],
    to: [f32; 3],
    body: Body,
    floor: f32,
    upper: f32,
) -> Result<(Vec3, Vec3), Failure> {
    Ok(Plan::new(from, to, body, floor, upper)?.bounds)
}

#[derive(Clone, Copy, Debug)]
struct Bounds {
    min: [f64; 3],
    max: [f64; 3],
}
impl Bounds {
    fn cuboid(center: Vec3, half: Vec3) -> Result<Self, Failure> {
        if !center.is_finite() || !half.is_finite() || !half.cmpgt(Vec3::ZERO).all() {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        let mut min = [0.0; 3];
        let mut max = [0.0; 3];
        for axis in 0..3 {
            min[axis] = exact_sum(f64::from(center[axis]), -f64::from(half[axis]))?;
            max[axis] = exact_sum(f64::from(center[axis]), f64::from(half[axis]))?;
        }
        Ok(Self { min, max })
    }
    fn manual(min: Vec3, max: Vec3) -> Result<Self, Failure> {
        if !min.is_finite() || !max.is_finite() || !min.cmple(max).all() {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        Ok(Self {
            min: min.to_array().map(f64::from),
            max: max.to_array().map(f64::from),
        })
    }
    fn contains(self, other: Self) -> bool {
        (0..3).all(|axis| self.min[axis] <= other.min[axis] && self.max[axis] >= other.max[axis])
    }
    #[cfg(test)]
    fn contains_point(self, point: Vec3) -> bool {
        point.is_finite()
            && (0..3).all(|axis| {
                self.min[axis] <= f64::from(point[axis]) && self.max[axis] >= f64::from(point[axis])
            })
    }
    fn include(&mut self, other: Self) {
        for axis in 0..3 {
            self.min[axis] = self.min[axis].min(other.min[axis]);
            self.max[axis] = self.max[axis].max(other.max[axis]);
        }
    }
    fn point(point: Vec3) -> Result<Self, Failure> {
        Self::manual(point, point)
    }
    fn ray_end(self, ray: Option<(Dir3, f32)>) -> Result<Self, Failure> {
        let Some((direction, distance)) = ray else {
            return Ok(self);
        };
        let direction = direction.as_vec3();
        let mut result = self;
        for axis in 0..3 {
            // A product of two finite binary32 numbers is exact in binary64, including
            // their exponent range. The addition may need directed enclosure instead.
            let delta = f64::from(direction[axis]) * f64::from(distance);
            result.min[axis] = outward_sum(self.min[axis], delta)?.0;
            result.max[axis] = outward_sum(self.max[axis], delta)?.1;
        }
        Ok(result)
    }
}

impl Plan {
    fn new(
        from: [f32; 3],
        to: [f32; 3],
        body: Body,
        floor: f32,
        upper: f32,
    ) -> Result<Self, Failure> {
        if !from.into_iter().chain(to).all(f32::is_finite)
            || !floor.is_finite()
            || !upper.is_finite()
            || floor > upper
        {
            return Err(Failure::Invalid);
        }
        if [from[2], to[2]].into_iter().any(|z| z < floor || z > upper) {
            return Err(Failure::UnsupportedGround);
        }
        let radius = body.radius.as_f32();
        let height = body.height.as_f32();
        // Same initialized-body bounds as core::Body. The derived proof height is not a
        // replacement body and is never copied into a Route, Facts or owner configuration.
        if !(radius > 0.0 && radius <= 4.0 && height > 0.0 && height <= 16.0) {
            return Err(Failure::UnresolvedBody);
        }
        let feet = [wow_to_bevy(from), wow_to_bevy(to)];
        let movement = feet[1] - feet[0];
        let ray = checked_ray(movement)?;
        let actual = Collider::cuboid(radius * 2.0, height, radius * 2.0);
        let actual_half = half_extents(&actual)?;
        let normalized_move = ray.map_or(Vec3::ZERO, |(direction, distance)| {
            direction.as_vec3() * distance
        });
        let centers = [
            feet[0] + Vec3::Y * (height * 0.5),
            feet[1] + Vec3::Y * (height * 0.5),
            feet[0] + Vec3::Y * (height * 0.5) + movement,
            feet[0] + Vec3::Y * (height * 0.5) + normalized_move,
        ];
        let heads = [
            feet[0] + Vec3::Y * height,
            feet[1] + Vec3::Y * height,
            feet[0] + Vec3::Y * height + movement,
            feet[0] + Vec3::Y * height + normalized_move,
        ];
        let start = Bounds::cuboid(centers[0], actual_half)?;
        let mut swept = start;
        for (center, head) in centers.into_iter().zip(heads) {
            let bounds = Bounds::cuboid(center, actual_half)?;
            checked_aabb(&actual, center, bounds)?;
            swept.include(bounds);
            swept.include(Bounds::point(head)?);
        }
        // Cover the real affine trajectory used by the normalized cast, not merely a
        // rounded `start + movement` or `start + direction * length` endpoint.
        swept.include(start.ray_end(ray)?);
        swept.include(Bounds::point(heads[0])?.ray_end(ray)?);
        for at in feet {
            swept.max[1] = swept.max[1].max(exact_sum(f64::from(at.y), f64::from(height))?);
        }
        if swept.min[1] < f64::from(floor) {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        swept.min[1] = f64::from(floor);
        let extent = power_of_two_ceiling(exact_sum(swept.max[1], -f64::from(floor))?)?;
        let mut base = Vec3::new(0.0, floor, 0.0);
        let mut needed_radius = f64::from(radius);
        for axis in [0, 2] {
            let low = down32(swept.min[axis])?;
            let high = up32(swept.max[axis])?;
            let midpoint = exact_sum(f64::from(low), f64::from(high))? * 0.5;
            base[axis] = midpoint as f32;
            needed_radius = needed_radius
                .max(exact_sum(f64::from(base[axis]), -f64::from(low))?)
                .max(exact_sum(f64::from(high), -f64::from(base[axis]))?);
        }
        let proof_radius = up32(needed_radius)?;
        let diameter = proof_radius * 2.0;
        if !diameter.is_finite() || diameter <= 0.0 {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        let proof = Collider::cuboid(diameter, extent, diameter);
        let proof_half = half_extents(&proof)?;
        let proof_center = base + Vec3::Y * (extent * 0.5);
        let enclosure = Bounds::cuboid(proof_center, proof_half)?;
        let radius3 = Vec3::new(proof_radius, 0.0, proof_radius);
        // ONE stationary square contains the entire native swept body and vertical gap.
        // No second normalized proof sweep is assumed to retrace the native cast. Exact
        // raw coverage, membership, hollow-hull rejection and overlap checks all apply
        // to this larger square. It is not a claim that the native corridor is square.
        let floor_point = [-base.z, -base.x, floor];
        if wow_to_bevy(floor_point) != base {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        let floor_points = [floor_point; 2];
        let proof_feet = floor_points.map(wow_to_bevy);
        // Reproduce, rather than replace, LocalCorridor's separately rounded boxes;
        // retain the same expressions even though the two proof endpoints coincide.
        let membership = Bounds::manual(
            proof_feet[0].min(proof_feet[1]) - radius3,
            proof_feet[0].max(proof_feet[1]) + radius3 + Vec3::Y * extent,
        )?;
        if enclosure.min[1] != f64::from(floor)
            || !enclosure.contains(swept)
            || !membership.contains(enclosure)
        {
            return Err(Failure::UnsupportedPointRepresentation);
        }
        for at in proof_feet {
            let endpoint = Bounds::manual(at - radius3, at + radius3 + Vec3::Y * extent)?;
            if !endpoint.contains(enclosure) {
                return Err(Failure::UnsupportedPointRepresentation);
            }
        }
        checked_aabb(&proof, proof_center, enclosure)?;
        Ok(Self {
            floor_points,
            proof_body: Body {
                radius: Real::new(proof_radius)
                    .map_err(|_| Failure::UnsupportedPointRepresentation)?,
                height: Real::new(extent).map_err(|_| Failure::UnsupportedPointRepresentation)?,
            },
            actual,
            center: centers[0],
            movement,
            head: heads[0],
            ray,
            #[cfg(test)]
            bounds: (
                Vec3::from_array(membership.min.map(|v| v as f32)),
                Vec3::from_array(membership.max.map(|v| v as f32)),
            ),
        })
    }
}

fn half_extents(collider: &Collider) -> Result<Vec3, Failure> {
    let half = collider
        .shape_scaled()
        .as_cuboid()
        .ok_or(Failure::UnsupportedPointRepresentation)?
        .half_extents;
    if !half.is_finite() || !half.cmpgt(Vec3::ZERO).all() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(half)
}

fn checked_aabb(collider: &Collider, center: Vec3, exact: Bounds) -> Result<(), Failure> {
    let ColliderAabb { min, max } = collider.aabb(center, Quat::IDENTITY);
    if !Bounds::manual(min, max)?.contains(exact) {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(())
}

fn checked_ray(movement: Vec3) -> Result<Option<(Dir3, f32)>, Failure> {
    if !movement.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    if movement == Vec3::ZERO {
        return Ok(None);
    }
    let (direction, length) =
        Dir3::new_and_length(movement).map_err(|_| Failure::UnsupportedPointRepresentation)?;
    if !length.is_finite() || length <= 0.0 {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(Some((direction, length)))
}

/// Error-free TwoSum checks representability, not a geometric tolerance. Binary32
/// promotion is exact, but an extreme exponent gap can still lose a term in binary64.
fn exact_sum(a: f64, b: f64) -> Result<f64, Failure> {
    let (sum, residual) = sum_parts(a, b)?;
    if residual != 0.0 {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(sum)
}

fn sum_parts(a: f64, b: f64) -> Result<(f64, f64), Failure> {
    let sum = a + b;
    let b_virtual = sum - a;
    let residual = (a - (sum - b_virtual)) + (b - b_virtual);
    if !a.is_finite() || !b.is_finite() || !sum.is_finite() || !residual.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok((sum, residual))
}

/// Directed numerical enclosure of a real cast endpoint, not an admitted motion epsilon.
fn outward_sum(a: f64, b: f64) -> Result<(f64, f64), Failure> {
    let (sum, residual) = sum_parts(a, b)?;
    let low = if residual < 0.0 { sum.next_down() } else { sum };
    let high = if residual > 0.0 { sum.next_up() } else { sum };
    if !low.is_finite() || !high.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok((low, high))
}

fn up32(value: f64) -> Result<f32, Failure> {
    let mut rounded = value as f32;
    if !value.is_finite() || !rounded.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    if f64::from(rounded) < value {
        rounded = rounded.next_up();
    }
    if !rounded.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(rounded)
}

fn down32(value: f64) -> Result<f32, Failure> {
    let mut rounded = value as f32;
    if !value.is_finite() || !rounded.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    if f64::from(rounded) > value {
        rounded = rounded.next_down();
    }
    if !rounded.is_finite() {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(rounded)
}

/// One conservative candidate, never a search for an accepting collider. Refuse
/// subnormal/overflow constructions and let the real-bound checks reject bad anchoring.
fn power_of_two_ceiling(value: f64) -> Result<f32, Failure> {
    let mut rounded = value as f32;
    if !value.is_finite() || value <= 0.0 || !rounded.is_normal() || rounded <= 0.0 {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    if f64::from(rounded) < value {
        rounded = rounded.next_up();
    }
    let bits = rounded.to_bits();
    let exponent = bits & 0x7f80_0000;
    let result = if bits & 0x007f_ffff == 0 {
        rounded
    } else {
        f32::from_bits(exponent + 0x0080_0000)
    };
    if !result.is_finite() || f64::from(result) < value {
        return Err(Failure::UnsupportedPointRepresentation);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Operand-level tests only, not installed-scene or source-provenance evidence.
    const H: f32 = -50.0;
    const Q: f32 = f32::from_bits(3_259_428_591);
    const POINTS: [[u32; 3]; 4] = [
        [3_313_891_166, 3_313_871_841, 3_259_498_496],
        [3_313_899_351, 3_313_872_188, 3_259_428_591],
        [3_313_907_536, 3_313_872_535, 3_259_428_591],
        [3_313_910_789, 3_313_872_673, 3_259_498_496],
    ];
    fn body() -> Body {
        Body {
            radius: Real::new(0.5).unwrap(),
            height: Real::new(3.59375).unwrap(),
        }
    }
    fn expect_failure(result: Result<Plan, Failure>, failure: Failure) {
        assert!(matches!(result, Err(actual) if actual == failure));
    }

    #[test]
    fn actual_native_operands_have_checked_floor_prisms_without_changing_points() {
        for pair in POINTS.windows(2) {
            let from = pair[0].map(f32::from_bits);
            let to = pair[1].map(f32::from_bits);
            let plan = Plan::new(from, to, body(), H, Q).unwrap();
            assert_eq!(plan.proof_body.height.as_f32(), 4.0);
            assert!(plan.proof_body.radius.as_f32() >= body().radius.as_f32());
            assert_eq!(plan.floor_points[0], plan.floor_points[1]);
            assert_eq!(plan.floor_points[0][2], H);
            assert_eq!(plan.center, wow_to_bevy(from) + Vec3::Y * 1.796875);
            assert_eq!(
                half_extents(&plan.actual).unwrap(),
                Vec3::new(0.5, 1.796875, 0.5)
            );
            assert_eq!(from.map(f32::to_bits), pair[0]);
            assert_eq!(to.map(f32::to_bits), pair[1]);
        }
    }

    #[test]
    fn exact_envelope_bounds_accept_and_adjacent_outside_values_refuse() {
        let from = POINTS[0].map(f32::from_bits);
        let to = POINTS[1].map(f32::from_bits);
        assert!(Plan::new(from, to, body(), H, Q).is_ok());
        for (index, value) in [(0, H.next_down()), (1, Q.next_up())] {
            let mut points = [from, to];
            points[index][2] = value;
            expect_failure(
                Plan::new(points[0], points[1], body(), H, Q),
                Failure::UnsupportedGround,
            );
        }
    }

    #[test]
    fn nonfinite_invalid_and_overflow_operands_refuse() {
        let from = POINTS[0].map(f32::from_bits);
        let to = POINTS[1].map(f32::from_bits);
        for bad in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            let mut changed = from;
            changed[0] = bad;
            expect_failure(Plan::new(changed, to, body(), H, Q), Failure::Invalid);
            expect_failure(Plan::new(from, to, body(), bad, Q), Failure::Invalid);
            expect_failure(Plan::new(from, to, body(), H, bad), Failure::Invalid);
        }
        expect_failure(Plan::new(from, to, body(), Q, H), Failure::Invalid);
        expect_failure(
            Plan::new([f32::MAX, 0.0, H], [-f32::MAX, 0.0, H], body(), H, Q),
            Failure::UnsupportedPointRepresentation,
        );
        for bad in [
            0.0,
            f64::INFINITY,
            f64::from(f32::MAX),
            f64::from(f32::from_bits(1)),
        ] {
            assert_eq!(
                power_of_two_ceiling(bad),
                Err(Failure::UnsupportedPointRepresentation)
            );
        }
    }

    #[test]
    fn one_candidate_is_checked_not_adjusted_until_floor_contact_appears() {
        // Adding half-height 2 loses the tiny negative floor. Candidate bottom becomes 0,
        // so even an otherwise enclosing box cannot claim a prism anchored at this H.
        let floor = -1.0 / 1_073_741_824.0;
        expect_failure(
            Plan::new([0.0, 0.0, floor], [1.0, 0.0, floor], body(), floor, floor),
            Failure::UnsupportedPointRepresentation,
        );
        assert_eq!(
            exact_sum(1.0, f64::from(f32::from_bits(1))),
            Err(Failure::UnsupportedPointRepresentation)
        );
    }

    #[test]
    fn rounded_manual_boxes_cannot_hide_undercontainment() {
        // At 4096 the small radius falls between binary32 coordinates. A rounded
        // AABB is not an exact bound on the real box defined by center/half-extents.
        let small = Body {
            radius: Real::new(0.0001).unwrap(),
            ..body()
        };
        expect_failure(
            Plan::new([4096.0, 4096.0, H], [4097.0, 4096.0, H], small, H, H),
            Failure::UnsupportedPointRepresentation,
        );
        let exact = Bounds::cuboid(
            Vec3::new(4096.0, H + 2.0, 4096.0),
            Vec3::new(0.0001, 2.0, 0.0001),
        )
        .unwrap();
        let rounded = Bounds::manual(
            Vec3::new(4096.0, H, 4096.0),
            Vec3::new(4096.0, H + 4.0, 4096.0),
        )
        .unwrap();
        assert!(!rounded.contains(exact));
    }

    #[test]
    fn proof_boxes_include_low_gap_and_true_upper_body_regions() {
        let from = POINTS[1].map(f32::from_bits);
        let to = POINTS[2].map(f32::from_bits);
        let plan = Plan::new(from, to, body(), H, Q).unwrap();
        let base = wow_to_bevy(from);
        let bounds = Bounds::manual(plan.bounds.0, plan.bounds.1).unwrap();
        assert!(bounds.contains_point(Vec3::new(base.x, H + 0.125, base.z)));
        assert!(bounds.contains_point(Vec3::new(base.x, Q + 3.5, base.z)));
        assert!(H + 0.125 < from[2]);
        assert!(Q + 3.5 < from[2] + body().height.as_f32());
    }

    #[test]
    fn stationary_reverse_and_invalid_initialized_body_operands_are_explicit() {
        let from = POINTS[1].map(f32::from_bits);
        let to = POINTS[2].map(f32::from_bits);
        assert!(Plan::new(from, from, body(), H, Q).unwrap().ray.is_none());
        assert!(Plan::new(to, from, body(), H, Q).is_ok());
        for (radius, height) in [
            (0.0, 3.59375),
            (4.0_f32.next_up(), 3.59375),
            (0.5, 0.0),
            (0.5, 16.0_f32.next_up()),
        ] {
            let invalid = Body {
                radius: Real::new(radius).unwrap(),
                height: Real::new(height).unwrap(),
            };
            expect_failure(Plan::new(from, to, invalid, H, Q), Failure::UnresolvedBody);
        }
        let subnormal = Body {
            height: Real::new(f32::from_bits(1)).unwrap(),
            ..body()
        };
        expect_failure(
            Plan::new(from, to, subnormal, H, Q),
            Failure::UnsupportedPointRepresentation,
        );
    }

    #[test]
    fn real_normalized_cast_endpoint_not_only_rounded_movement_is_enclosed() {
        let from = POINTS[0].map(f32::from_bits);
        let to = POINTS[1].map(f32::from_bits);
        let plan = Plan::new(from, to, body(), H, Q).unwrap();
        let (direction, distance) = plan.ray.unwrap();
        assert!((0..3).any(|axis| {
            f64::from(direction.as_vec3()[axis]) * f64::from(distance)
                != f64::from(plan.movement[axis])
        }));
        let body_end = Bounds::cuboid(plan.center, half_extents(&plan.actual).unwrap())
            .unwrap()
            .ray_end(plan.ray)
            .unwrap();
        let head_end = Bounds::point(plan.head).unwrap().ray_end(plan.ray).unwrap();
        let enclosure = Bounds::manual(plan.bounds.0, plan.bounds.1).unwrap();
        assert!(enclosure.contains(body_end));
        assert!(enclosure.contains(head_end));
        assert_eq!(
            bounds_for_test(from, to, body(), H, Q).unwrap(),
            plan.bounds
        );
        assert_eq!(
            outward_sum(1.0, f64::from(f32::from_bits(1))).unwrap(),
            (1.0, 1.0_f64.next_up())
        );
    }
}
