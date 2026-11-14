/// Namespaced source membership; MODD indices are local to one WMO placement.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum NativeHullId {
    M2(u32),
    WmoWalk(u32),
    WmoCamera(u32),
    WmoDoodad(u32, u32),
}

// Exact, allocation-free collider equality over a bounded source-built triangle stream.
// Kept engine-independent so corrupt, omitted and reordered hull fixtures hit the real gate.
pub(super) const MAX_HULL_VERTICES: usize = 262_144;
pub(super) const MAX_HULL_TRIANGLES: usize = 262_144;

pub(super) fn matches(
    expected_vertices: &[[f32; 3]],
    expected_triangles: &[[u32; 3]],
    actual_vertices: impl ExactSizeIterator<Item = [f32; 3]>,
    actual_triangles: &[[u32; 3]],
) -> bool {
    if expected_vertices.len() > MAX_HULL_VERTICES
        || expected_triangles.len() > MAX_HULL_TRIANGLES
        || expected_vertices.is_empty()
        || expected_triangles.is_empty()
        || expected_vertices.len() != actual_vertices.len()
        || expected_triangles != actual_triangles
        || expected_triangles
            .iter()
            .flatten()
            .any(|&i| i as usize >= expected_vertices.len())
    {
        return false;
    }
    expected_vertices
        .iter()
        .zip(actual_vertices)
        .all(|(expected, actual)| expected.iter().all(|v| v.is_finite()) && *expected == actual)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn source_weld_exactly_matches_live_hull() {
        let v = [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [3.0, 0.0, 0.0],
            [4.0, 0.0, 0.0],
            [3.0, 1.0, 0.0],
        ];
        let t = [[0, 1, 2], [3, 4, 5]];
        assert!(matches(&v, &t, v.into_iter(), &t));
        assert!(!matches(&v, &t, v[..3].iter().copied(), &t[..1]));
        assert!(!matches(&v, &t, v.into_iter(), &[[0, 1, 2], [0, 1, 2]]));
        let mut moved = v;
        moved[3][0] += 0.1;
        assert!(!matches(&v, &t, moved.into_iter(), &t));
        assert!(!matches(&v, &t, v.into_iter(), &[[0, 1, 2], [5, 4, 3]]));
        assert!(!matches(&v, &[[0, 1, 7]], v.into_iter(), &[[0, 1, 7]]));
        let mut invalid = v;
        invalid[0][0] = f32::NAN;
        assert!(!matches(&invalid, &t, invalid.into_iter(), &t));
        assert!(!matches(&[], &[], [].into_iter(), &[]));
    }
}

/// Complete-source exterior broadphase, in WoW XY. Only the audited producer supplies bounds;
/// the positive answer says that all potentially crossing hulls appear in the checked guard.
#[derive(Default)]
pub(super) struct ExteriorIndex {
    bounds: Vec<ExteriorBound>,
    buckets: std::collections::BTreeMap<(u32, u32), Vec<usize>>,
    memberships: usize,
}
struct ExteriorBound {
    id: u32,
    digest: [u8; 32],
    min: [f32; 2],
    max: [f32; 2],
}
impl ExteriorIndex {
    pub(super) fn insert(
        &mut self,
        id: u32,
        digest: [u8; 32],
        min: [f32; 2],
        max: [f32; 2],
    ) -> bool {
        if min.into_iter().chain(max).any(|v| !v.is_finite())
            || (0..2).any(|i| min[i] > max[i])
            || self.bounds.len() >= 262_144
        {
            return false;
        }
        let a = benilla_formats::world_to_tile(min[0], min[1]);
        let b = benilla_formats::world_to_tile(max[0], max[1]);
        let (xmin, xmax, ymin, ymax) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
        let additional = ((xmax - xmin + 1) * (ymax - ymin + 1)) as usize;
        if self.memberships + additional > 1_048_576 {
            return false;
        }
        for y in ymin..=ymax {
            for x in xmin..=xmax {
                self.buckets
                    .entry((x, y))
                    .or_default()
                    .push(self.bounds.len());
            }
        }
        self.memberships += additional;
        self.bounds.push(ExteriorBound {
            id,
            digest,
            min,
            max,
        });
        true
    }
    pub(super) fn covers(
        &self,
        from: [f32; 3],
        to: [f32; 3],
        radius: f32,
        known: &[(u32, [u8; 32])],
    ) -> bool {
        if from
            .into_iter()
            .chain(to)
            .chain([radius])
            .any(|v| !v.is_finite())
            || radius < 0.0
        {
            return false;
        }
        let min = [
            f64::from(from[0].min(to[0])) - f64::from(radius),
            f64::from(from[1].min(to[1])) - f64::from(radius),
        ];
        let max = [
            f64::from(from[0].max(to[0])) + f64::from(radius),
            f64::from(from[1].max(to[1])) + f64::from(radius),
        ];
        // world_to_tile has f32 transitions displaced slightly from the canonical f64
        // terrain lattice. Consult the whole expanded range plus one neighbor on each side;
        // exact f64 AABB rejection below removes unrelated entries without missing an edge.
        let a = benilla_formats::world_to_tile(min[0] as f32, min[1] as f32);
        let b = benilla_formats::world_to_tile(max[0] as f32, max[1] as f32);
        let (xmin, xmax, ymin, ymax) = (
            a.0.min(b.0).saturating_sub(1),
            (a.0.max(b.0) + 1).min(63),
            a.1.min(b.1).saturating_sub(1),
            (a.1.max(b.1) + 1).min(63),
        );
        if xmax - xmin > 3 || ymax - ymin > 3 || known.len() > 4096 {
            return false;
        }
        let mut candidates = std::collections::BTreeSet::new();
        for y in ymin..=ymax {
            for x in xmin..=xmax {
                if let Some(bucket) = self.buckets.get(&(x, y)) {
                    if bucket.len() > 4096 {
                        return false;
                    }
                    candidates.extend(bucket);
                    if candidates.len() > 4096 {
                        return false;
                    }
                }
            }
        }
        let known: std::collections::BTreeMap<_, _> = known.iter().copied().collect();
        candidates.into_iter().all(|&i| {
            let b = &self.bounds[i];
            !(f64::from(b.min[0]) <= max[0]
                && f64::from(b.max[0]) >= min[0]
                && f64::from(b.min[1]) <= max[1]
                && f64::from(b.max[1]) >= min[1])
                || known.get(&b.id) == Some(&b.digest)
        })
    }
}
#[cfg(test)]
mod exterior_tests {
    use super::*;
    #[test]
    fn giant_unloaded_exterior_hull_must_be_in_actual_guard_proof() {
        let mut index = ExteriorIndex::default();
        assert!(index.insert(42, [1; 32], [-2000.0, -2000.0], [-5.0, -5.0]));
        assert!(!index.covers([-10.0, -10.0, 0.0], [-12.0, -10.0, 0.0], 0.5, &[]));
        assert!(index.covers(
            [-10.0, -10.0, 0.0],
            [-12.0, -10.0, 0.0],
            0.5,
            &[(42, [1; 32])]
        ));
        assert!(!index.covers(
            [-10.0, -10.0, 0.0],
            [-12.0, -10.0, 0.0],
            0.5,
            &[(42, [2; 32])]
        ));
    }
    #[test]
    fn unrelated_exterior_bounds_do_not_block_and_boundaries_are_included() {
        let mut index = ExteriorIndex::default();
        assert!(index.insert(1, [1; 32], [-20.0, -20.0], [-15.0, -15.0]));
        assert!(index.covers([-10.0, -10.0, 0.0], [-12.0, -10.0, 0.0], 0.5, &[]));
        assert!(!index.covers([-14.5, -15.0, 0.0], [-14.5, -15.0, 0.0], 0.5, &[]));
        assert!(!index.insert(2, [1; 32], [f32::NAN, 0.0], [1.0, 1.0]));
        assert!(!index.insert(2, [1; 32], [2.0, 0.0], [1.0, 1.0]));
        assert!(!index.covers([-10.0, -10.0, 0.0], [1000.0, -10.0, 0.0], 0.5, &[]));
    }
}

#[cfg(test)]
mod bucket_rounding_tests {
    use super::*;
    #[test]
    fn displaced_f32_tile_boundary_cannot_hide_an_exterior_hull() {
        let mut index = ExteriorIndex::default();
        assert!(index.insert(42, [1; 32], [0.0006, -10.0], [0.0007, -9.0]));
        assert!(!index.covers([0.5005, -9.5, 0.0], [0.5005, -9.5, 0.0], 0.5, &[]));
        assert!(index.covers(
            [0.5005, -9.5, 0.0],
            [0.5005, -9.5, 0.0],
            0.5,
            &[(42, [1; 32])]
        ));
    }
}

#[cfg(test)]
mod membership_tests {
    use super::NativeHullId;
    #[test]
    fn identical_numeric_ids_cannot_substitute_for_wmo_parts_or_other_placements() {
        use NativeHullId::*;
        let ids = [
            M2(7),
            WmoWalk(7),
            WmoCamera(7),
            WmoDoodad(7, 0),
            WmoDoodad(7, 1),
            WmoDoodad(8, 0),
        ];
        let unique: std::collections::BTreeSet<_> = ids.into_iter().collect();
        assert_eq!(unique.len(), ids.len());
        assert!(!unique.contains(&WmoDoodad(8, 1)));
        for id in ids {
            match id {
                M2(i) | WmoWalk(i) | WmoCamera(i) => assert_eq!(i, 7),
                WmoDoodad(i, d) => assert!(i >= 7 && d <= 1),
            }
        }
    }
}
