// SPDX-License-Identifier: GPL-3.0-or-later
//! Source-derived CMaNGOS triangle/model transforms and G3D math. See PROVENANCE.md.
use super::{Bounds, Error, Ray};
pub(super) type Vector = [f32; 3];
pub(super) type Matrix = [[f32; 3]; 3];
pub(super) fn sub(a: Vector, b: Vector) -> Vector {
    std::array::from_fn(|i| a[i] - b[i])
}
pub(super) fn scale(a: Vector, s: f32) -> Vector {
    a.map(|v| v * s)
}
pub(super) fn dot(a: Vector, b: Vector) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: Vector, b: Vector) -> Vector {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub(super) fn apply(m: Matrix, v: Vector) -> Vector {
    m.map(|row| dot(row, v))
}
fn multiply(a: Matrix, b: Matrix) -> Matrix {
    std::array::from_fn(|r| {
        std::array::from_fn(|c| a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c])
    })
}
/// G3D::Matrix3::fromEulerAnglesZYX; ModelInstance uses {rotation.y,x,z}.
pub(super) fn rotation(angles: Vector) -> Matrix {
    let radians = |angle: f32| (std::f64::consts::PI * f64::from(angle) / 180.0) as f32;
    // Pinned G3D resolves the float sin/cos overload after the angle cast.
    let sc = |angle: f32| {
        let a = radians(angle);
        (a.sin(), a.cos())
    };
    let (s, c) = sc(angles[1]);
    let z = [[c, -s, 0.0], [s, c, 0.0], [0.0, 0.0, 1.0]];
    let (s, c) = sc(angles[0]);
    let y = [[c, 0.0, s], [0.0, 1.0, 0.0], [-s, 0.0, c]];
    let (s, c) = sc(angles[2]);
    let x = [[1.0, 0.0, 0.0], [0.0, c, -s], [0.0, s, c]];
    multiply(z, multiply(y, x))
}
/// Source cofactor inverse, not a transpose approximation of a float rotation.
pub(super) fn inverse(m: Matrix) -> Result<Matrix, Error> {
    let mut out = [
        [
            m[1][1] * m[2][2] - m[1][2] * m[2][1],
            m[0][2] * m[2][1] - m[0][1] * m[2][2],
            m[0][1] * m[1][2] - m[0][2] * m[1][1],
        ],
        [
            m[1][2] * m[2][0] - m[1][0] * m[2][2],
            m[0][0] * m[2][2] - m[0][2] * m[2][0],
            m[0][2] * m[1][0] - m[0][0] * m[1][2],
        ],
        [
            m[1][0] * m[2][1] - m[1][1] * m[2][0],
            m[0][1] * m[2][0] - m[0][0] * m[2][1],
            m[0][0] * m[1][1] - m[0][1] * m[1][0],
        ],
    ];
    let det = m[0][0] * out[0][0] + m[0][1] * out[1][0] + m[0][2] * out[2][0];
    if !det.is_finite() || det.abs() <= 1e-6 {
        return Err(Error::Format);
    }
    let inv = (1.0f64 / f64::from(det)) as f32;
    for row in &mut out {
        for v in row {
            *v *= inv;
        }
    }
    Ok(out)
}
/// Original IntersectTriangle expression order and strict (0,max_distance) interval.
pub(super) fn triangle(points: [Vector; 3], ray: Ray, distance: &mut f32) -> bool {
    let e1 = sub(points[1], points[0]);
    let e2 = sub(points[2], points[0]);
    let p = cross(ray.direction, e2);
    let a = dot(e1, p);
    if a.abs() < 1e-5 {
        return false;
    }
    let f = 1.0 / a;
    let s = sub(ray.origin, points[0]);
    let u = f * dot(s, p);
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let q = cross(s, e1);
    let v = f * dot(ray.direction, q);
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let t = f * dot(e2, q);
    if t > 0.0 && t < *distance {
        *distance = t;
        true
    } else {
        false
    }
}
/// Exact boolean gate used by Ray::intersectionTime(AABox), including signed-zero tests.
/// Source checks infinity only; no distance or collision normal is needed by ModelInstance.
pub(super) fn intersects_box(ray: Ray, bounds: Bounds) -> bool {
    let mut inside = true;
    let mut at = [0.0; 3];
    let mut max_t = [-1.0f32; 3];
    for i in 0..3 {
        if ray.origin[i] < bounds.low[i] {
            at[i] = bounds.low[i];
            inside = false;
            if ray.direction[i].to_bits() != 0 {
                max_t[i] = (at[i] - ray.origin[i]) / ray.direction[i];
            }
        } else if ray.origin[i] > bounds.high[i] {
            at[i] = bounds.high[i];
            inside = false;
            if ray.direction[i].to_bits() != 0 {
                max_t[i] = (at[i] - ray.origin[i]) / ray.direction[i];
            }
        }
    }
    if inside {
        return true;
    }
    let mut plane = 0;
    if max_t[1] > max_t[plane] {
        plane = 1;
    }
    if max_t[2] > max_t[plane] {
        plane = 2;
    }
    if max_t[plane].is_sign_negative() {
        return false;
    }
    for (i, at) in at.iter_mut().enumerate() {
        if i != plane {
            *at = ray.origin[i] + max_t[plane] * ray.direction[i];
            if *at < bounds.low[i] || *at > bounds.high[i] {
                return false;
            }
        }
    }
    // Ray::intersectionTime rejects infinite distance; NaN is deliberately not infinity.
    dot(sub(at, ray.origin), sub(at, ray.origin)).sqrt() != f32::INFINITY
}
