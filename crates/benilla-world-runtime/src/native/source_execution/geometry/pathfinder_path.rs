// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 PathFinder.cpp:81-179,386-1021,1122-1138.
use super::*;
use benilla_native_nav::live::{DistanceDeclaration, PathQuery};
const NORMAL: u32 = 1;
const INCOMPLETE: u32 = 4;
const NOPATH: u32 = 8;
const NOT_USING: u32 = 16;
const SHORT: u32 = 32;
fn yzx(p: [f32; 3]) -> [f32; 3] {
    [p[1], p[2], p[0]]
}
fn xyz(p: [f32; 3]) -> [f32; 3] {
    [p[2], p[0], p[1]]
}
fn distance2(a: [f32; 3], b: [f32; 3]) -> f32 {
    let d = [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
    (d[0] * d[0] + d[1] * d[1]) + d[2] * d[2]
}
fn sqrt(value: f32) -> Result<f32> {
    benilla_sim::source_transports::sqrt_single_bits(value.to_bits())
        .map(f32::from_bits)
        .map_err(|_| refuse("execution.pathfinder.square_root_numeric_domain"))
}
fn step_count(value: f32) -> Result<u32> {
    // PathFinder.cpp:813 truncates before uint32 conversion; (-1,0) is defined zero.
    require(
        value.is_finite() && value > -1. && value < 4294967296.,
        "execution.pathfinder.step_count_conversion_undefined_813",
    )?;
    Ok(value as u32)
}
impl SourceTerrainManager {
    /// Map::IsDungeon uses Map.dbc instance type 1 or 2, DBCStructure.h:593-600.
    pub fn bind_pathfinder_maps(
        &mut self,
        catalog: &mut super::super::super::production_compiler::VerifiedCatalog,
    ) -> Result<()> {
        self.pathfinder_map_types = Some(catalog.execution_map_entries()?.into_iter().collect());
        Ok(())
    }
}
impl SourcePathFinder {
    fn query<'a>(&self, geometry: &'a SourceTerrainManager) -> Result<PathQuery<'a>> {
        let map = self
            .current_mesh_map
            .ok_or_else(|| refuse("execution.pathfinder.current_mesh_owner"))?;
        let mesh = geometry
            .mmap
            .mesh(map)
            .ok_or_else(|| refuse("execution.mmap.stale_query"))?;
        mesh.pathfinder_query(
            self.current_query
                .as_ref()
                .ok_or_else(|| refuse("execution.pathfinder.current_query_owner"))?,
        )
        .map_err(mmap_manager::mmap_error)
    }
    fn read_attachment(&mut self) -> Result<()> {
        let attachment = self
            .attachment
            .take()
            .ok_or_else(|| refuse("execution.pathfinder.transport_owner"))??;
        self.transport = if attachment.is_some() {
            SourcePathTransport::Attached
        } else {
            SourcePathTransport::None
        };
        Ok(())
    }
    /// PathFinder.cpp:83-99 leaves a previously selected mesh untouched when disabled.
    fn select_mesh(
        &mut self,
        geometry: &SourceTerrainManager,
        context: &MotionContext<'_>,
    ) -> Result<()> {
        let object = context.objects.get(self.unit)?;
        let map = object
            .map
            .ok_or_else(|| refuse("execution.pathfinder.map_owner"))?;
        let fields = context.units.get(context.objects, self.unit)?;
        let forcing = if fields.mmap_extra_flags & 0x4000 != 0 {
            MMapForcing::Ignored
        } else if fields.mmap_extra_flags & 0x2000 != 0 || fields.flags & 8 != 0 {
            MMapForcing::Forced
        } else {
            MMapForcing::Default
        };
        if !geometry.mmap.pathfinding_enabled(map.map, forcing)? {
            return Ok(());
        }
        self.read_attachment()?;
        match self.transport {
            SourcePathTransport::Unbound => {
                return Err(refuse("execution.pathfinder.transport_owner_86"))
            }
            SourcePathTransport::Attached => {
                return Err(refuse(
                    "execution.pathfinder.transport_model_query_owner_87",
                ))
            }
            SourcePathTransport::None => (),
        }
        if self.default_map.map != map.map {
            self.default_query =
                SourceDefaultQuery::Lookup(geometry.mmap.query(map.map, map.instance));
        }
        self.current_query = self.read_default_query()?.cloned();
        if let Some(query) = &self.current_query {
            // PathFinder.cpp:97 dereferences the retained query before assigning the mesh.
            geometry.mmap.query_capacity(map.map, query)?;
            self.current_mesh_map = Some(map.map);
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub fn calculate(
        &mut self,
        geometry: &mut SourceTerrainManager,
        context: &MotionContext<'_>,
        attachment: Result<Option<ObjectHandle>>,
        start: [f32; 3],
        dest: [f32; 3],
        force: bool,
        straight_line: bool,
    ) -> Result<bool> {
        if benilla_native_nav::source_map_coord_pair(
            start.map(f32::to_bits),
            dest.map(f32::to_bits),
        )
        .is_err()
        {
            return Ok(false);
        }
        self.start = start;
        self.end = dest;
        self.actual_end = dest;
        self.force_destination = force;
        self.straight_line = straight_line;
        self.transport = SourcePathTransport::Unbound;
        self.attachment = Some(attachment);
        self.select_mesh(geometry, context)?;
        // Unit.h:367 UNIT_STAT_IGNORE_PATHFINDING; source short-circuits both HaveTile calls.
        if self.current_mesh_map.is_none()
            || self.current_query.is_none()
            || context.units.get(context.objects, self.unit)?.state & 0x1000_0000 != 0
            || !self
                .query(geometry)?
                .have_tile(yzx(start))
                .map_err(mmap_manager::mmap_error)?
            || !self
                .query(geometry)?
                .have_tile(yzx(dest))
                .map_err(mmap_manager::mmap_error)?
        {
            self.shortcut(geometry)?;
            self.path_type = NORMAL | NOT_USING;
        } else {
            self.update_filter(geometry, context.objects)?;
            self.build_poly_path(geometry, context)?;
        }
        geometry.pathfinder_calculate_completions += 1;
        Ok(true)
    }
    fn normalize(&mut self, geometry: &SourceTerrainManager) -> Result<()> {
        if !geometry.mmap.pathfinder_setting("PathFinder.NormalizeZ")? || self.ignore_normalization
        {
            return Ok(());
        }
        if self.points.is_empty() {
            return Ok(());
        }
        // PathFinder.cpp:993-999 first needs the concrete attachment and Unit allowed-Z owner.
        if self.transport == SourcePathTransport::Unbound {
            self.read_attachment()?;
        }
        match self.transport {
            SourcePathTransport::Unbound => Err(refuse("execution.pathfinder.transport_owner_993")),
            SourcePathTransport::Attached => {
                Err(refuse("execution.pathfinder.passenger_position_owner_998"))
            }
            SourcePathTransport::None => {
                Err(refuse("execution.pathfinder.allowed_position_z_owner_999"))
            }
        }
    }
    fn shortcut(&mut self, geometry: &SourceTerrainManager) -> Result<()> {
        self.clear();
        self.points.extend([self.start, self.actual_end]);
        self.normalize(geometry)?;
        self.path_type = 2;
        Ok(())
    }
    fn build_poly_path(
        &mut self,
        geometry: &mut SourceTerrainManager,
        context: &MotionContext<'_>,
    ) -> Result<()> {
        let map = context
            .objects
            .get(self.unit)?
            .map
            .ok_or_else(|| refuse("execution.pathfinder.map_owner"))?
            .map;
        let kind = geometry
            .pathfinder_map_types
            .as_ref()
            .and_then(|m| m.get(&map))
            .copied()
            .ok_or_else(|| refuse("execution.pathfinder.map_dungeon_owner_395"))?;
        if kind == 1 || kind == 2 {
            self.cache
                .dungeon_limit(sqrt(distance2(self.end, self.start))?);
        }
        let mut start_distance = PathDistance::new(DistanceDeclaration::BuildPolyStart);
        let mut end_distance = PathDistance::new(DistanceDeclaration::BuildPolyEnd);
        let start_point = yzx(self.start);
        let mut end_point = yzx(self.end);
        let start = self.nearest_poly(geometry, start_point, &mut start_distance)?;
        let end = self.nearest_poly(geometry, end_point, &mut end_distance)?;
        if start.is_none() || end.is_none() {
            self.shortcut(geometry)?;
            let wet = (start.is_none() && geometry.is_swimmable(map, self.start, 1.5, None)?)
                || (end.is_none() && geometry.is_swimmable(map, self.end, 1.5, None)?);
            let f = context.units.get(context.objects, self.unit)?;
            self.path_type = if if wet {
                f.can_swim
            } else {
                context.objects.get(self.unit)?.kind != SourceObjectKind::Player && f.can_fly
            } {
                NORMAL | NOT_USING
            } else {
                NOPATH
            };
            return Ok(());
        }
        let start = start.unwrap().polygon;
        let end = end.unwrap().polygon;
        let far = start_distance.value().unwrap() > 7. || end_distance.value().unwrap() > 7.;
        if far {
            let p = if start_distance.value().unwrap() > 7. {
                self.start
            } else {
                self.end
            };
            let wet = geometry.is_under_water(map, p, None)?;
            let f = context.units.get(context.objects, self.unit)?;
            if if wet { f.can_swim } else { f.can_fly } {
                self.shortcut(geometry)?;
                self.path_type = NORMAL | NOT_USING;
                return Ok(());
            }
            let (status, closest) = self
                .query(geometry)?
                .closest(end, end_point)
                .map_err(mmap_manager::mmap_error)?;
            if status & (1 << 30) != 0 {
                end_point = closest;
                self.actual_end = xyz(closest);
            }
            self.path_type = INCOMPLETE;
        }
        if start == end {
            self.shortcut(geometry)?;
            self.cache.same_poly(start);
            self.path_type = if far { INCOMPLETE } else { NORMAL };
            return Ok(());
        }
        if !self.cache.reuse_subrange(start, end) {
            self.clear();
            let result = self
                .query(geometry)?
                .poly_path(
                    &mut self.cache,
                    self.filter,
                    [start, end],
                    start_point,
                    end_point,
                    self.straight_line,
                )
                .map_err(mmap_manager::mmap_error)?;
            if self.straight_line && result.hit != f32::MAX {
                // PathFinder.cpp:723 reads model-derived collision width before the hit threshold.
                let width = self
                    .collision_width
                    .ok_or_else(|| refuse("execution.pathfinder.collision_width_owner_723"))?;
                let hit = result.hit - width;
                if hit < 0.1 {
                    self.path_type = NOPATH;
                    return Ok(());
                }
                let mut position = [0.; 3];
                for axis in 0..3 {
                    position[axis] =
                        start_point[axis] + (end_point[axis] - start_point[axis]) * hit;
                }
                let mut distance = PathDistance::new(DistanceDeclaration::RayHit);
                if let Some(poly) = self.nearest_poly(geometry, position, &mut distance)? {
                    let mut height = position[1];
                    self.query(geometry)?
                        .height(poly.polygon, position, &mut height)
                        .map_err(mmap_manager::mmap_error)?;
                    position[1] = height;
                    if f64::from(distance2(self.start, xyz(position))) > 0.01 {
                        self.end = xyz(position);
                        self.actual_end = self.end;
                        self.build_point_path(geometry, start_point, position)?;
                        self.path_type = INCOMPLETE;
                        return Ok(());
                    }
                }
                self.path_type = NOPATH;
                return Ok(());
            }
            if self.cache.active_len() == 0 || result.status & (1 << 31) != 0 {
                self.shortcut(geometry)?;
                self.path_type = NOPATH;
                return Ok(());
            }
        }
        self.path_type =
            if self.cache.active().last() == Some(&end) && self.path_type & INCOMPLETE == 0 {
                NORMAL
            } else {
                INCOMPLETE
            };
        self.build_point_path(geometry, start_point, end_point)
    }
    fn build_point_path(
        &mut self,
        geometry: &SourceTerrainManager,
        start: [f32; 3],
        end: [f32; 3],
    ) -> Result<()> {
        if self.cached_points.len() < self.cache.point_limit() * 3 {
            self.cached_points.resize(self.cache.point_limit() * 3, 0.);
        }
        let (status, count) = if self.straight_line {
            let start = xyz(start);
            let end = xyz(end);
            let length = sqrt(distance2(end, start))?;
            if length > 0.1 {
                let step_float = length / 4.;
                let mut steps = step_count(step_float)?;
                let part = step_float - steps as f32;
                if steps > 1 && part < 0.3 {
                    steps -= 1;
                }
                let diff = [
                    (end[0] - start[0]) * (4. / length),
                    (end[1] - start[1]) * (4. / length),
                    (end[2] - start[2]) * (4. / length),
                ];
                let mut prev = start;
                self.points.clear();
                require(
                    steps <= 16384,
                    "execution.pathfinder.stepped_path_budget_825",
                )?;
                self.points
                    .try_reserve(steps as usize + 2)
                    .map_err(|_| refuse("execution.pathfinder.point_reserve_owner_825"))?;
                self.points.push(start);
                let mut distance = PathDistance::new(DistanceDeclaration::SteppedHeight);
                for _ in 0..steps {
                    for axis in 0..3 {
                        prev[axis] += diff[axis];
                    }
                    let p = yzx(prev);
                    if let Some(poly) = self.nearest_poly(geometry, p, &mut distance)? {
                        self.query(geometry)?
                            .height(poly.polygon, p, &mut prev[2])
                            .map_err(mmap_manager::mmap_error)?;
                    }
                    self.points.push(prev);
                }
                self.points.push(end);
                (1 << 30, self.points.len())
            } else {
                (1 << 31, 0)
            }
        } else {
            let (status, count) = self
                .query(geometry)?
                .points(
                    &self.cache,
                    self.filter,
                    start,
                    end,
                    self.use_straight_path,
                    &mut self.cached_points,
                    &mut self.smooth_refs,
                )
                .map_err(mmap_manager::mmap_error)?;
            let optimize = count > 2
                && geometry
                    .mmap
                    .pathfinder_setting("PathFinder.OptimizePath")?;
            let final_count = self
                .query(geometry)?
                .convert_points(&self.cached_points, count, optimize, &mut self.points)
                .map_err(mmap_manager::mmap_error)?;
            (status, final_count)
        };
        if count < 2 || status & (1 << 31) != 0 {
            self.shortcut(geometry)?;
            self.path_type = NOPATH;
            return Ok(());
        }
        if count == self.cache.point_limit() {
            self.shortcut(geometry)?;
            self.path_type = SHORT;
            return Ok(());
        }
        self.actual_end = self.points[count - 1];
        let d = [
            self.end[0] - self.actual_end[0],
            self.end[1] - self.actual_end[1],
            self.end[2] - self.actual_end[2],
        ];
        let in_range = d[0] * d[0] + d[1] * d[1] < 1. && d[2].abs() < 1.;
        if self.force_destination && (self.path_type & NORMAL == 0 || !in_range) {
            if distance2(self.actual_end, self.end) < 0.3 * distance2(self.start, self.end) {
                self.actual_end = self.end;
                *self.points.last_mut().unwrap() = self.end;
            } else {
                self.actual_end = self.end;
                self.shortcut(geometry)?;
            }
            self.path_type = NORMAL | NOT_USING;
        }
        self.normalize(geometry)
    }
}

#[cfg(test)]
#[path = "pathfinder_path_tests.rs"]
mod tests;
