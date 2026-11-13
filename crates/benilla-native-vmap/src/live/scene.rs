// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 WorldModel.cpp:35-85,156-201,309-390,428-513.
use super::*;
pub(super) struct WorldModel {
    root: u32,
    groups: Vec<Group>,
    tree: Option<Bih>,
}
struct Group {
    bounds: Bounds,
    flags: u32,
    id: u32,
    vertices: Vec<Vector>,
    triangles: Vec<[u32; 3]>,
    tree: Option<Bih>,
    liquid: Option<Liquid>,
}
struct Liquid {
    x: u32,
    y: u32,
    corner: Vector,
    kind: u32,
    heights: Vec<f32>,
    flags: Vec<u8>,
}
fn source_magic(c: &mut Cursor<'_>, magic: &[u8]) -> Result<()> {
    c.magic(magic).map_err(|_| Error::SourceReadFailure)
}
impl WorldModel {
    pub(super) fn decode(bytes: &[u8]) -> Result<Self> {
        let mut c = Cursor::new(bytes);
        source_magic(&mut c, b"VMAP_7.0WMOD")?;
        c.word()?;
        let root = c.word()?;
        // WorldModel.cpp:565 ignores a missing or mismatching GMOD chunk.
        if c.magic(b"GMOD").is_err() {
            return Ok(Self {
                root,
                groups: Vec::new(),
                tree: None,
            });
        }
        let n = c.word()? as usize;
        let mut groups = reserve(n)?;
        for _ in 0..n {
            groups.push(Group::decode(&mut c)?);
        }
        source_magic(&mut c, b"GBIH")?;
        let tree = Bih::decode(&mut c)?;
        if tree.primitive_count() != n {
            return Err(Error::Format);
        }
        Ok(Self {
            root,
            groups,
            tree: Some(tree),
        })
    }
    fn ray(&self, ray: Ray, d: &mut f32, b: &mut QueryBudget, stop: bool) -> Result<bool> {
        if self.groups.len() == 1 {
            return self.groups[0].ray(ray, d, b, stop);
        }
        let Some(tree) = &self.tree else {
            return Ok(false);
        };
        tree.intersect_source(ray, d, b, stop, |i, d, b| {
            self.groups[i as usize].ray(ray, d, b, stop)
        })
    }
    fn point(&self, p: Vector, down: Vector, b: &mut QueryBudget) -> Result<Option<(usize, f32)>> {
        let Some(tree) = &self.tree else {
            return Ok(None);
        };
        let mut selected = None;
        let mut minimum = f32::INFINITY;
        tree.intersect_point(p, b, |i, b| {
            let g = &self.groups[i as usize];
            if !g.triangles.is_empty() && g.bounds.contains(p) {
                let ray = Ray {
                    origin: sub(p, scale(down, 0.1)),
                    direction: down,
                };
                let mut d = f32::INFINITY;
                if g.ray(ray, &mut d, b, false)? {
                    let d = d - 0.1;
                    if d < minimum {
                        minimum = d;
                        selected = Some((i as usize, d));
                    }
                }
            }
            Ok(())
        })?;
        Ok(selected)
    }
}
impl Group {
    fn decode(c: &mut Cursor<'_>) -> Result<Self> {
        let bounds = c.bounds()?;
        let flags = c.word()?;
        let id = c.word()?;
        source_magic(c, b"VERT")?;
        c.word()?;
        let n = c.word()? as usize;
        let mut vertices = reserve(n)?;
        for _ in 0..n {
            vertices.push(c.vector()?);
        }
        if n == 0 {
            return Ok(Self {
                bounds,
                flags,
                id,
                vertices,
                triangles: Vec::new(),
                tree: None,
                liquid: None,
            });
        }
        source_magic(c, b"TRIM")?;
        c.word()?;
        let n = c.word()? as usize;
        let mut triangles = reserve(n)?;
        for _ in 0..n {
            let tri = [c.word()?, c.word()?, c.word()?];
            if tri.iter().any(|&i| i as usize >= vertices.len()) {
                return Err(Error::Format);
            }
            triangles.push(tri);
        }
        source_magic(c, b"MBIH")?;
        let tree = Bih::decode(c)?;
        if tree.primitive_count() != triangles.len() {
            return Err(Error::Format);
        }
        source_magic(c, b"LIQU")?;
        let length = c.word()?;
        let liquid = if length == 0 {
            None
        } else {
            Some(Liquid::decode(c)?)
        };
        Ok(Self {
            bounds,
            flags,
            id,
            vertices,
            triangles,
            tree: Some(tree),
            liquid,
        })
    }
    fn ray(&self, ray: Ray, d: &mut f32, b: &mut QueryBudget, stop: bool) -> Result<bool> {
        if self.triangles.is_empty() {
            return Ok(false);
        }
        self.tree
            .as_ref()
            .ok_or(Error::Missing)?
            .intersect_source(ray, d, b, stop, |i, d, b| {
                b.take()?;
                Ok(triangle(
                    self.triangles[i as usize].map(|v| self.vertices[v as usize]),
                    ray,
                    d,
                ))
            })
    }
}
impl Liquid {
    fn decode(c: &mut Cursor<'_>) -> Result<Self> {
        let x = c.word()?;
        let y = c.word()?;
        let corner = c.vector()?;
        let kind = c.word()?;
        // WorldModel.cpp:234-238 uses defined unsigned multiplication/wrap.
        let nx = x.wrapping_add(1);
        let ny = y.wrapping_add(1);
        let n = nx.wrapping_mul(ny) as usize;
        let mut heights = reserve(n)?;
        for _ in 0..n {
            heights.push(c.float()?);
        }
        let n = x.wrapping_mul(y) as usize;
        let flags = c.take(n)?.to_vec();
        Ok(Self {
            x,
            y,
            corner,
            kind,
            heights,
            flags,
        })
    }
    fn height(&self, p: Vector) -> Result<Option<f32>> {
        let size = 533.333_f32 / 128.0;
        let tx = (p[0] - self.corner[0]) / size;
        // The source converts before checking negative coordinates, including UB.
        let cast = |f: f32| {
            if f.is_finite() && f > -1.0 && f < 4_294_967_296.0 {
                Ok(f as u32)
            } else {
                Err(Error::UndefinedRead)
            }
        };
        let x = cast(tx)?;
        if tx < 0.0 || x >= self.x {
            return Ok(None);
        }
        let ty = (p[1] - self.corner[1]) / size;
        let y = cast(ty)?;
        if ty < 0.0 || y >= self.y {
            return Ok(None);
        }
        let flag_index = x.wrapping_add(y.wrapping_mul(self.x)) as usize;
        if self.flags.get(flag_index).ok_or(Error::UndefinedRead)? & 15 == 15 {
            return Ok(None);
        }
        let dx = tx - x as f32;
        let dy = ty - y as f32;
        let row = self.x.wrapping_add(1);
        let i = x.wrapping_add(y.wrapping_mul(row));
        let h = |index: u32| {
            self.heights
                .get(index as usize)
                .copied()
                .ok_or(Error::UndefinedRead)
        };
        let (sx, sy) = if dx > dy {
            (
                h(i.wrapping_add(1))? - h(i)?,
                h(i.wrapping_add(1).wrapping_add(row))? - h(i.wrapping_add(1))?,
            )
        } else {
            (
                h(i.wrapping_add(1).wrapping_add(row))? - h(i.wrapping_add(row))?,
                h(i.wrapping_add(row))? - h(i)?,
            )
        };
        let height = h(i)? + dx * sx + dy * sy;
        // Arm and SSE generate different NaN bits on finite-height overflow.
        #[cfg(not(target_arch = "x86_64"))]
        if height.is_nan() {
            return Err(Error::LiquidInterpolationNaN);
        }
        Ok(Some(height))
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AreaInfo {
    pub floor: f32,
    pub flags: u32,
    pub adt: i32,
    pub root: i32,
    pub group: i32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LocationInfo {
    pub floor: f32,
    pub root: i32,
    pub group: i32,
    pub model: ModelGeneration,
    map: u32,
    pub slot: usize,
    group_index: usize,
    position: Vector,
    instance_generation: u64,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LiquidOutput {
    pub level: f32,
    pub floor: f32,
    pub kind: u32,
}
impl Manager {
    fn instance_model(&self, i: &Instance) -> Result<Option<&SharedModel>> {
        i.model.as_ref().map(|m| self.model(m)).transpose()
    }
    fn ray_instance(
        &self,
        i: &Instance,
        ray: Ray,
        d: &mut f32,
        b: &mut QueryBudget,
        stop: bool,
        ignore_m2: bool,
    ) -> Result<bool> {
        let Some(m) = self.instance_model(i)? else {
            return Ok(false);
        };
        if !intersects_box(ray, i.spawn.bounds.ok_or(Error::UndefinedRead)?) {
            return Ok(false);
        }
        // WorldModel flags, not the immutable instance classification, control LOS.
        if ignore_m2 && m.flags & 1 != 0 {
            return Ok(false);
        }
        let local = Ray {
            origin: scale(
                apply(i.inverse, sub(ray.origin, i.spawn.position)),
                i.inverse_scale,
            ),
            direction: apply(i.inverse, ray.direction),
        };
        let mut distance = *d * i.inverse_scale;
        let hit = m.model.ray(local, &mut distance, b, stop)?;
        if hit {
            *d = distance * i.spawn.scale;
        }
        Ok(hit)
    }
    fn ray_tree(
        &self,
        t: &Tree,
        ray: Ray,
        d: &mut f32,
        b: &mut QueryBudget,
        stop: bool,
        ignore_m2: bool,
    ) -> Result<bool> {
        t.bih.intersect_source(ray, d, b, stop, |n, d, b| {
            if let Some(i) = t.slots[n as usize].as_ref() {
                self.ray_instance(i, ray, d, b, stop, ignore_m2)
            } else {
                Ok(false)
            }
        })
    }
    /// VMapManager2.cpp:187-202, MapTree.cpp:155-165.
    pub fn line_of_sight(
        &self,
        map: u32,
        from: Vector,
        to: Vector,
        ignore_m2: bool,
        deadline: Instant,
    ) -> Result<bool> {
        if !self.los_enabled {
            return Ok(true);
        }
        let Some(t) = self.trees.get(&map).and_then(Option::as_ref) else {
            return Ok(true);
        };
        arithmetic_environment()?;
        let from = to_internal(from);
        let to = to_internal(to);
        if from == to {
            return Ok(true);
        }
        let delta = sub(to, from);
        let mut d = dot(delta, delta).sqrt();
        if !d.is_finite() || d >= f32::MAX {
            return Err(Error::NumericBinding);
        }
        if d < 1e-10 {
            return Ok(true);
        }
        let ray = Ray {
            origin: from,
            direction: scale(delta, 1.0 / d),
        };
        Ok(!self.ray_tree(
            t,
            ray,
            &mut d,
            &mut QueryBudget::new(deadline),
            true,
            ignore_m2,
        )?)
    }
    /// VMapManager2.cpp:208-229; MapTree.cpp:173-208, including miss roundtrip.
    pub fn hit_position(
        &self,
        map: u32,
        from: Vector,
        to: Vector,
        modify: f32,
        deadline: Instant,
    ) -> Result<(bool, Vector)> {
        if !self.los_enabled {
            return Ok((false, to));
        }
        let Some(t) = self.trees.get(&map).and_then(Option::as_ref) else {
            return Ok((false, to));
        };
        arithmetic_environment()?;
        let from = to_internal(from);
        let to = to_internal(to);
        let delta = sub(to, from);
        let mut distance = dot(delta, delta).sqrt();
        if !distance.is_finite() || distance >= f32::MAX {
            return Err(Error::NumericBinding);
        }
        if distance < 1e-10 {
            return Ok((false, to_internal(to)));
        }
        let direction = scale(delta, 1.0 / distance);
        let hit = self.ray_tree(
            t,
            Ray {
                origin: from,
                direction,
            },
            &mut distance,
            &mut QueryBudget::new(deadline),
            false,
            false,
        )?;
        let mut position = to;
        if hit {
            position = std::array::from_fn(|a| from[a] + direction[a] * distance);
            if modify < 0.0 {
                let travelled = sub(position, from);
                if dot(travelled, travelled).sqrt() > -modify {
                    position = std::array::from_fn(|a| position[a] + direction[a] * modify);
                } else {
                    position = from;
                }
            } else {
                position = std::array::from_fn(|a| position[a] + direction[a] * modify);
            }
            // Preserve x86 source results; other hosts cannot promise its NaN bits.
            #[cfg(not(target_arch = "x86_64"))]
            if position.iter().any(|v| v.is_nan()) {
                return Err(Error::HitPositionNaN);
            }
        }
        Ok((hit, to_internal(position)))
    }
    /// VMapManager2.cpp:237-253, MapTree.cpp:213-230, including upward search.
    pub fn height(&self, map: u32, p: Vector, search: f32, deadline: Instant) -> Result<f32> {
        const INVALID: f32 = -200_000.0;
        if !self.height_enabled {
            return Ok(INVALID);
        }
        let Some(t) = self.trees.get(&map).and_then(Option::as_ref) else {
            return Ok(INVALID);
        };
        arithmetic_environment()?;
        let p = to_internal(p);
        let mut d = search.abs();
        let direction = if search >= 0.0 {
            [0.0, 0.0, -1.0]
        } else {
            [0.0, 0.0, 1.0]
        };
        if self.ray_tree(
            t,
            Ray {
                origin: p,
                direction,
            },
            &mut d,
            &mut QueryBudget::new(deadline),
            false,
            false,
        )? {
            Ok(if search >= 0.0 { p[2] - d } else { p[2] + d })
        } else {
            Ok(INVALID)
        }
    }
    fn point_instance(
        &self,
        i: &Instance,
        p: Vector,
        b: &mut QueryBudget,
    ) -> Result<Option<(usize, f32, Vector)>> {
        let Some(m) = self.instance_model(i)? else {
            return Ok(None);
        };
        if i.spawn.flags & 1 != 0 {
            return Ok(None);
        }
        if !i.spawn.bounds.ok_or(Error::UndefinedRead)?.contains(p) {
            return Ok(None);
        }
        let local = scale(apply(i.inverse, sub(p, i.spawn.position)), i.inverse_scale);
        let down = apply(i.inverse, [0.0, 0.0, -1.0]);
        let Some((group, d)) = m.model.point(local, down, b)? else {
            return Ok(None);
        };
        let ground: Vector = std::array::from_fn(|a| local[a] + d * down[a]);
        // Source row-vector multiplication by inverse, not a second inverse.
        let z = (ground[0] * i.inverse[0][2]
            + ground[1] * i.inverse[1][2]
            + ground[2] * i.inverse[2][2])
            * i.spawn.scale
            + i.spawn.position[2];
        Ok(Some((group, z, local)))
    }
    /// Area metadata changes on every hit, floor and ADT only on a higher hit.
    /// ModelInstance.cpp:84-95; WorldModel.cpp:481-494.
    pub fn area_info(&self, map: u32, p: Vector, deadline: Instant) -> Result<Option<AreaInfo>> {
        let Some(t) = self.trees.get(&map).and_then(Option::as_ref) else {
            return Ok(None);
        };
        arithmetic_environment()?;
        let p = to_internal(p);
        let mut out = AreaInfo {
            floor: f32::NEG_INFINITY,
            flags: 0,
            adt: 0,
            root: 0,
            group: 0,
        };
        let mut hit = false;
        t.bih
            .intersect_point(p, &mut QueryBudget::new(deadline), |n, b| {
                if let Some(i) = &t.slots[n as usize] {
                    if let Some((g, z, _)) = self.point_instance(i, p, b)? {
                        let m = self.instance_model(i)?.unwrap();
                        let group = &m.model.groups[g];
                        out.root = m.model.root as i32;
                        out.group = group.id as i32;
                        out.flags = group.flags;
                        hit = true;
                        if out.floor < z {
                            out.floor = z;
                            out.adt = i.spawn.adt as i32;
                        }
                    }
                }
                Ok(())
            })?;
        Ok(hit.then_some(out))
    }
    pub fn location(&self, map: u32, p: Vector, deadline: Instant) -> Result<Option<LocationInfo>> {
        let Some(t) = self.trees.get(&map).and_then(Option::as_ref) else {
            return Ok(None);
        };
        arithmetic_environment()?;
        let internal = to_internal(p);
        let mut selected: Option<LocationInfo> = None;
        t.bih
            .intersect_point(internal, &mut QueryBudget::new(deadline), |n, b| {
                if let Some(i) = &t.slots[n as usize] {
                    if let Some((g, z, _)) = self.point_instance(i, internal, b)? {
                        if selected.as_ref().is_none_or(|s| s.floor < z) {
                            let m = self.instance_model(i)?.unwrap();
                            selected = Some(LocationInfo {
                                floor: z,
                                root: m.model.root as i32,
                                group: m.model.groups[g].id as i32,
                                model: i.model.clone().unwrap(),
                                map,
                                slot: n as usize,
                                group_index: g,
                                position: internal,
                                instance_generation: i.generation,
                            });
                        }
                    }
                }
                Ok(())
            })?;
        Ok(selected)
    }
    /// VMapManager2.cpp:288-305. Floor and type writes precede mask rejection.
    pub fn liquid_level(
        &self,
        map: u32,
        p: Vector,
        mask: u8,
        out: &mut LiquidOutput,
        deadline: Instant,
    ) -> Result<bool> {
        let Some(location) = self.location(map, p, deadline)? else {
            return Ok(false);
        };
        self.liquid_at(&location, mask, out)
    }
    pub fn liquid_at(
        &self,
        location: &LocationInfo,
        mask: u8,
        out: &mut LiquidOutput,
    ) -> Result<bool> {
        let model = self.model(&location.model)?;
        let i = self
            .trees
            .get(&location.map)
            .and_then(Option::as_ref)
            .and_then(|t| t.slots.get(location.slot))
            .and_then(Option::as_ref)
            .filter(|i| {
                i.model.as_ref() == Some(&location.model)
                    && i.generation == location.instance_generation
            })
            .ok_or(Error::StaleGeneration)?;
        let group = &model.model.groups[location.group_index];
        out.floor = location.floor;
        out.kind = group.liquid.as_ref().map_or(0, |l| l.kind);
        // VMapManager2.cpp:272-283; GridMapDefines.h:66-75, not DBC Type indices.
        let kind_mask = match out.kind {
            1 | 41 | 61 => 8,
            2 => 2,
            3 => 1,
            4 | 21 => 4,
            _ => 0,
        };
        if mask != 0 && mask & kind_mask == 0 {
            return Ok(false);
        }
        let Some(liquid) = &group.liquid else {
            return Ok(false);
        };
        let local = scale(
            apply(i.inverse, sub(location.position, i.spawn.position)),
            i.inverse_scale,
        );
        if let Some(z) = liquid.height(local)? {
            out.level = (z - local[2]) * i.spawn.scale + location.position[2];
            Ok(true)
        } else {
            Ok(false)
        }
    }
}
