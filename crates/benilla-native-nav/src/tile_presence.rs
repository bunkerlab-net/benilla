// SPDX-License-Identifier: GPL-3.0-or-later
//! Loaded nontransport HaveTile observations; never owner/configuration admission.
use crate::{Error, Mesh};
use std::time::Instant;

pub const SOURCE_TILE_PRESENCE_POLICY: u32 = 1;

/// First source calculate coordinate check to fail, before any tile observation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceMapCoordError {
    DestinationInvalid,
    StartInvalid,
}
/// Exact GridDefines.h IsValidMapCoord pair predicate, destination then start.
/// XY magnitude bits are bounded by (533.33333f * 64 / 2) - 0.5, exactly
/// 17066.166015625; Z only has to be finite. Integer checks preserve subnormals,
/// signed zeros and signaling NaNs without depending on the floating environment.
pub fn source_map_coord_pair(from: [u32; 3], to: [u32; 3]) -> Result<(), SourceMapCoordError> {
    fn valid(point: [u32; 3]) -> bool {
        point[0] & 0x7fff_ffff <= 0x4685_5455
            && point[1] & 0x7fff_ffff <= 0x4685_5455
            && point[2] & 0x7fff_ffff < 0x7f80_0000
    }
    if !valid(to) {
        return Err(SourceMapCoordError::DestinationInvalid);
    }
    if !valid(from) {
        return Err(SourceMapCoordError::StartInvalid);
    }
    Ok(())
}

/// Read-only source coordinates and the layer-zero presence predicate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SourceTilePresence {
    coordinates: [i32; 2],
    present: bool,
}
impl SourceTilePresence {
    pub fn coordinates(self) -> [i32; 2] {
        self.coordinates
    }
    pub fn present(self) -> bool {
        self.present
    }
}

/// Observations tied to original world endpoint bits and one immutable loaded mesh.
/// This proves only nontransport HaveTile. It cannot establish live source nav/query
/// availability, IGNORE_PATHFINDING, enabled policy, dungeon classification or owner state.
///
/// ```compile_fail
/// use benilla_native_nav::{LoadedSourceTilePresence, SourceTilePresence};
/// let observation = SourceTilePresence { coordinates: [0, 0], present: true };
/// let forged = LoadedSourceTilePresence { mesh_id: 1, from: [0; 3], to: [0; 3],
///     start: observation, end: Some(observation) };
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoadedSourceTilePresence {
    mesh_id: u64,
    from: [u32; 3],
    to: [u32; 3],
    start: SourceTilePresence,
    end: Option<SourceTilePresence>,
}
impl LoadedSourceTilePresence {
    pub fn from(&self) -> [u32; 3] {
        self.from
    }
    pub fn to(&self) -> [u32; 3] {
        self.to
    }
    pub fn start(&self) -> SourceTilePresence {
        self.start
    }
    /// None means source short-circuiting skipped the destination after start failed.
    pub fn end(&self) -> Option<SourceTilePresence> {
        self.end
    }
    pub fn both_present(&self) -> bool {
        self.start.present && self.end.is_some_and(|end| end.present)
    }
    pub fn check_mesh(&self, mesh: &Mesh) -> Result<(), Error> {
        if self.mesh_id != mesh.instance_id {
            return Err(Error::Identity);
        }
        Ok(())
    }
}
impl Mesh {
    /// Query actual loaded tile presence for original world {x,y,z} bit patterns.
    /// Source coordinate validity is checked destination-first, then the narrower
    /// native finite +/-100000 domain. Source
    /// calcTileLoc uses binary32/floorf; exactly -1 is rejected, other negatives are
    /// looked up. At most two layer-zero hash lookups over at most 64 loaded tiles.
    pub fn source_tile_presence_pair(
        &self,
        from: [u32; 3],
        to: [u32; 3],
        until: Instant,
    ) -> Result<LoadedSourceTilePresence, Error> {
        #[cfg(not(benilla_native_detour))]
        {
            let _ = (from, to, until);
            Err(Error::Unsupported)
        }
        #[cfg(benilla_native_detour)]
        {
            crate::deadline(until)?;
            source_map_coord_pair(from, to).map_err(|_| Error::Format)?;
            crate::detour(from).map_err(|_| Error::Unsupported)?;
            crate::detour(to).map_err(|_| Error::Unsupported)?;
            let start = native::observe(self, from)?;
            crate::deadline(until)?;
            let end = if start.present {
                Some(native::observe(self, to)?)
            } else {
                None
            };
            crate::deadline(until)?;
            Ok(LoadedSourceTilePresence {
                mesh_id: self.instance_id,
                from,
                to,
                start,
                end,
            })
        }
    }
}
#[cfg(benilla_native_detour)]
mod native {
    use super::*;
    use std::ffi::c_void;
    unsafe extern "C" {
        fn benilla_nav_source_tile_presence(
            mesh: *const c_void,
            world: *const f32,
            coordinates: *mut i32,
            present: *mut u8,
        ) -> u32;
    }
    pub(super) fn observe(mesh: &Mesh, point: [u32; 3]) -> Result<SourceTilePresence, Error> {
        let point = point.map(f32::from_bits);
        let mut coordinates = [-1; 2];
        let mut present = 0;
        // SAFETY: immutable validated mesh, three validated world floats and exact
        // output widths. The native guard rejects noncanonical floating environments.
        let status = unsafe {
            benilla_nav_source_tile_presence(
                mesh.raw.as_ptr(),
                point.as_ptr(),
                coordinates.as_mut_ptr(),
                &mut present,
            )
        };
        if status == 0 {
            return Err(Error::Unsupported);
        }
        crate::success(status)?;
        Ok(SourceTilePresence {
            coordinates,
            present: present != 0,
        })
    }
}

#[cfg(all(test, benilla_native_detour))]
#[path = "tile_presence_tests.rs"]
mod tests;
