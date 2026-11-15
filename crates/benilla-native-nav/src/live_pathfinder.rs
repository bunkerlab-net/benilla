// SPDX-License-Identifier: GPL-3.0-or-later
//! PathFinder.cpp:306-383 on the real map-instance query, not a bounded route query.
use super::*;
use crate::{SourceOwnerFilter, SourceWarmLookup, SourceWarmLookupOrigin};

/// The declaration lifetime matters, especially the loop-local prior at :830.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DistanceDeclaration {
    BuildPolyStart,
    BuildPolyEnd,
    RayHit,
    SteppedHeight,
    RandomPoint,
}
#[derive(Debug)]
pub struct PathDistance {
    pub declaration: DistanceDeclaration,
    value: Option<f32>,
}
impl PathDistance {
    pub fn new(declaration: DistanceDeclaration) -> Self {
        // Deviation: PathFinder.cpp:410,717,830,1400 use absent cmangos-pathfinder-distance-intent-v1 priors until a source write because the original locals are unwritten, not defined reference bytes.
        Self {
            declaration,
            value: None,
        }
    }
    pub fn value(&self) -> Option<f32> {
        self.value
    }
}

/// Source storage survives clear(), including inactive slots and appended tail refs.
#[derive(Debug)]
pub struct PathCache {
    storage: Vec<u64>,
    active: usize,
    point_limit: usize,
}
impl PathCache {
    pub fn new() -> Self {
        Self {
            storage: vec![0; 74],
            active: 0,
            point_limit: 74,
        }
    }
    pub fn storage(&self) -> &[u64] {
        &self.storage
    }
    pub fn active_len(&self) -> usize {
        self.active
    }
    pub fn point_limit(&self) -> usize {
        self.point_limit
    }
    pub fn clear(&mut self) {
        self.active = 0;
    }
    /// Only the source path producer installs the active prefix, not recorded answers.
    #[cfg(test)]
    pub(crate) fn install_prefix(&mut self, refs: &[u64]) -> Result<(), LiveError> {
        if refs.len() > self.point_limit {
            return Err(Error::Budget.into());
        }
        self.storage[..refs.len()].copy_from_slice(refs);
        self.active = refs.len();
        Ok(())
    }
}
impl Default for PathCache {
    fn default() -> Self {
        Self::new()
    }
}

impl MapMesh {
    pub fn pathfinder_lookup(
        &self,
        handle: &QueryHandle,
        cache: &mut PathCache,
        filter: SourceOwnerFilter,
        point: [f32; 3],
        distance: &mut PathDistance,
    ) -> Result<Option<SourceWarmLookup>, LiveError> {
        numeric()?;
        let raw = self.checked_query(handle)?.raw.as_ptr();
        if !point.iter().all(|v| v.is_finite() && v.abs() <= 32768.) {
            return Err(Error::Budget.into());
        }
        #[cfg(benilla_native_detour)]
        {
            native_lookup(self, raw, cache, filter, point, distance)
        }
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (raw, cache, filter, point, distance);
            Err(Error::Unsupported.into())
        }
    }
}

#[cfg(benilla_native_detour)]
#[repr(C)]
#[derive(Default)]
struct Scan {
    reference: u64,
    minimum_squared: f32,
    distance: f32,
}
#[cfg(benilla_native_detour)]
unsafe extern "C" {
    fn benilla_live_warm_reset(scan: *mut Scan);
    fn benilla_live_warm_scan(
        raw: *mut c_void,
        reference: u64,
        point: *const f32,
        scan: *mut Scan,
    ) -> u32;
    fn benilla_live_warm_nearest(
        raw: *mut c_void,
        point: *const f32,
        extent: f32,
        include: u16,
        exclude: u16,
        reference: *mut u64,
        distance: *mut f32,
    ) -> u32;
}
#[cfg(benilla_native_detour)]
fn native_lookup(
    mesh: &MapMesh,
    raw: *mut c_void,
    cache: &mut PathCache,
    filter: SourceOwnerFilter,
    point: [f32; 3],
    distance: &mut PathDistance,
) -> Result<Option<SourceWarmLookup>, LiveError> {
    for (refs, maximum, origin) in [
        (
            &cache.storage[..cache.active],
            3.,
            SourceWarmLookupOrigin::Active,
        ),
        (
            if cache.storage.len() > cache.active.max(cache.point_limit) {
                &cache.storage[cache.point_limit..]
            } else {
                &[]
            },
            7.,
            SourceWarmLookupOrigin::Tail,
        ),
    ] {
        if refs.is_empty() {
            continue;
        }
        let mut scan = Scan::default();
        // SAFETY: checked live query, finite point, initialized C-layout output.
        unsafe { benilla_live_warm_reset(&mut scan) };
        for &reference in refs {
            // Invalid/stale polygon refs have a defined Detour failure and are skipped.
            let status =
                unsafe { benilla_live_warm_scan(raw, reference, point.as_ptr(), &mut scan) };
            if status & (1 << 31) != 0 {
                continue;
            }
            if scan.minimum_squared < 1. {
                break;
            }
        }
        distance.value = Some(scan.distance);
        // Source compares squared distance with the unsquared 3/7 constants.
        if scan.minimum_squared <= maximum && scan.reference != 0 {
            return Ok(Some(SourceWarmLookup {
                polygon: scan.reference,
                distance_bits: scan.distance.to_bits(),
                origin,
                appended: false,
            }));
        }
    }
    for (extent, origin) in [
        (5., SourceWarmLookupOrigin::Nearest5),
        (10., SourceWarmLookupOrigin::Nearest10),
    ] {
        for axis in [0, 2] {
            let origin =
                f32::from_le_bytes(mesh.params[axis * 4..axis * 4 + 4].try_into().unwrap());
            let dimension = 12 + axis * 2;
            let width =
                f32::from_le_bytes(mesh.params[dimension..dimension + 4].try_into().unwrap());
            let low = ((point[axis] - extent - origin) / width).floor();
            let high = ((point[axis] + extent - origin) / width).floor();
            if !(-1024. ..=1024.).contains(&low) || !(-1024. ..=1024.).contains(&high) {
                return Err(Error::Budget.into());
            }
        }
        let mut polygon = 0;
        let mut found_distance = 0.;
        // SAFETY: authenticated live allocation and bounded finite nearest rectangle.
        let status = unsafe {
            benilla_live_warm_nearest(
                raw,
                point.as_ptr(),
                extent,
                filter.include_flags,
                filter.exclude_flags,
                &mut polygon,
                &mut found_distance,
            )
        };
        if status & (1 << 30) != 0 && polygon != 0 {
            let appended = distance
                .value
                .is_none_or(|prior| prior == 0. || found_distance < prior);
            if appended {
                cache
                    .storage
                    .try_reserve(1)
                    .map_err(|_| LiveError::Allocation)?;
                cache.storage.push(polygon);
            }
            distance.value = Some(found_distance);
            return Ok(Some(SourceWarmLookup {
                polygon,
                distance_bits: found_distance.to_bits(),
                origin,
                appended,
            }));
        }
    }
    Ok(None)
}

#[cfg(all(test, benilla_native_detour))]
#[path = "live_pathfinder_tests.rs"]
mod tests;

#[path = "live_path.rs"]
mod path;
pub use path::{PathQuery, PolyResult};
