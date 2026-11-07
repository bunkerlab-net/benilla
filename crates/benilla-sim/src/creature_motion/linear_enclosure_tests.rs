// SPDX-License-Identifier: GPL-3.0-or-later
use super::*;

fn point(v: [f32; 3]) -> Point {
    Point::new(v[0], v[1], v[2]).unwrap()
}
fn contains(bounds: [Point; 2], p: Point) -> bool {
    let values = |p: Point| [p.x.as_f32(), p.y.as_f32(), p.z.as_f32()];
    values(bounds[0])
        .into_iter()
        .zip(values(p))
        .zip(values(bounds[1]))
        .all(|((low, v), high)| low <= v && v <= high)
}
#[test]
fn actual_sixth_enclosure_contains_source_spline_ticks() {
    let a = point([3_313_896_757, 3_313_872_078, 3_259_450_747].map(f32::from_bits));
    let b = point([3_313_918_466, 3_313_872_738, 3_259_498_496].map(f32::from_bits));
    let bounds = linear_segment_enclosure(a, b).unwrap();
    let mut spline = LinearSpline::launch(vec![a, b], a, Real::new(7.0).unwrap()).unwrap();
    for _ in 0..=spline.duration_ms() {
        assert!(contains(bounds, spline.position().unwrap()));
        spline.advance(1);
    }
    assert_eq!(spline.position().unwrap(), b);
    assert_eq!(bounds[0].z, b.z);
}
#[test]
fn ascending_descending_and_constant_coordinates_enclose_samples() {
    for (a, b) in [
        ([1.0, 2.0, -50.0], [4.0, -2.0, -49.75]),
        ([4.0, -2.0, -49.75], [1.0, 2.0, -50.0]),
        ([1.0, 2.0, -50.0], [1.0, 2.0, -50.0]),
    ] {
        let (a, b) = (point(a), point(b));
        let bounds = linear_segment_enclosure(a, b).unwrap();
        for u in [0.0, 0.125, 0.5, 0.875, 1.0] {
            assert!(contains(
                bounds,
                numeric::interpolate(a, b, Real::new(u).unwrap()).unwrap()
            ));
        }
    }
}
#[test]
fn rounded_delta_cannot_substitute_a_different_endpoint() {
    let a = point([10_000.0, 1.0, -50.0]);
    let b = point([f32::from_bits(0x3580_0000), 1.0, -50.0]);
    assert_ne!(numeric::interpolate(a, b, Real::ONE).unwrap(), b);
    assert_eq!(
        linear_segment_enclosure(a, b),
        Err(MotionError::Unsupported("linear endpoint reconstruction"))
    );
}
#[test]
fn invalid_world_coordinate_is_not_enclosure_evidence() {
    let a = Point {
        x: Real::new(40_000.0).unwrap(),
        y: Real::ZERO,
        z: Real::ZERO,
    };
    assert!(linear_segment_enclosure(a, point([1.0, 0.0, 0.0])).is_err());
    assert!(Real::from_bits(1).is_err());
    assert!(Real::new(f32::INFINITY).is_err());
}
