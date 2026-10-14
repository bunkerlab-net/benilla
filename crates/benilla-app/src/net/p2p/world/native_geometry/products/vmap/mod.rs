// SPDX-License-Identifier: GPL-3.0-or-later
//! Bounded dry static VMap loader/query port, not Map LOS, navigation or admission.
//! Every product is already inventoried; all hierarchy/model references are validated
//! before publication. No untrusted bytes are passed to native code. See PROVENANCE.md.
mod bih;
mod math;
mod trig;
use self::{bih::Bih, math::*};
use super::{Error, Hash, InventoriedProducts, ProductId};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, time::Instant};

const MAX_VMAP_BYTES: usize = 16 * 1024 * 1024;
const MAX_ELEMENTS: usize = 262_144;
const MAX_GROUPS: usize = 4096;
const MAX_SPAWNS: usize = 16_384;
pub(crate) const MAX_QUERY_WORK: usize = 32_768;
const POLICY: &[u8] = b"benilla-dry-static-vmap-ray-v2-glibc-2.41-bounded-trig";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SnapshotBinding {
    pub map: u32,
    pub content: Hash,
    pub source_epoch: Hash,
    pub inventory: Hash,
    pub policy: Hash,
}
/// Bit-preserving endpoints and query identity. A reply echoes this entire value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RayRequest {
    pub snapshot: SnapshotBinding,
    pub ticket: u64,
    pub from: [u32; 3],
    pub to: [u32; 3],
    pub ignore_m2: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StaticRay {
    /// Only this loaded static VMap closure; not server LOS or body traversability.
    Miss,
    /// Nearest strictly interior intersection distance in original world units, f32 bits.
    Hit { distance: u32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RayReply {
    pub request: RayRequest,
    pub result: StaticRay,
}

#[derive(Debug)]
pub(crate) struct VmapSnapshot {
    binding: SnapshotBinding,
    tree: Bih,
    models: Vec<Model>,
    spawns: Vec<Option<Spawn>>,
}
impl InventoriedProducts {
    /// Decode and validate actual VMap geometry from these immutable inventoried bytes.
    /// Terrain and Detour remain independent; this cannot construct an authority session.
    pub(crate) fn load_static_vmap(&self) -> Result<VmapSnapshot, Error> {
        VmapSnapshot::load(self)
    }
}
impl VmapSnapshot {
    fn load(products: &InventoriedProducts) -> Result<Self, Error> {
        let mut bytes = 0usize;
        for (id, p) in &products.products {
            if matches!(
                id,
                ProductId::VmapTree | ProductId::VmapTile(_) | ProductId::VmapModel(_)
            ) {
                bytes = bytes.checked_add(p.bytes.len()).ok_or(Error::Budget)?;
            }
        }
        if bytes > MAX_VMAP_BYTES {
            return Err(Error::Budget);
        }
        let data = products.inventoried_bytes(products.map, &ProductId::VmapTree)?;
        let mut c = Cursor::new(data);
        c.magic(b"VMAP_7.0\x01NODE")?;
        let tree = Bih::decode(&mut c)?;
        c.magic(b"GOBJ")?;
        c.finish()?;
        let mut models = Vec::new();
        let mut model_ids = BTreeMap::new();
        let mut groups = 0usize;
        for (id, p) in &products.products {
            if let ProductId::VmapModel(name) = id {
                let model = Model::decode(&p.bytes)?;
                groups = groups
                    .checked_add(model.groups.len())
                    .ok_or(Error::Budget)?;
                if groups > MAX_GROUPS {
                    return Err(Error::Budget);
                }
                model_ids.insert(name.as_str(), models.len());
                models.try_reserve(1).map_err(|_| Error::Budget)?;
                models.push(model);
            }
        }
        let mut spawns: Vec<Option<Spawn>> = allocate(tree.primitive_count())?;
        spawns.resize_with(tree.primitive_count(), || None);
        let mut spawn_ids = BTreeMap::new();
        let mut classifications = BTreeMap::new();
        let mut total_spawns = 0usize;
        let mut placement_work = 0usize;
        for (id, p) in &products.products {
            if !matches!(id, ProductId::VmapTile(_)) {
                continue;
            }
            let mut c = Cursor::new(&p.bytes);
            c.magic(b"VMAP_7.0")?;
            let count = c.count()?;
            total_spawns = total_spawns.checked_add(count).ok_or(Error::Budget)?;
            if total_spawns > MAX_SPAWNS {
                return Err(Error::Budget);
            }
            for _ in 0..count {
                let (reference, spawn) = Spawn::decode(&mut c, &model_ids)?;
                let slot = spawns.get_mut(reference).ok_or(Error::Format)?;
                if let Some(previous) = slot {
                    if !previous.same_source(&spawn) {
                        return Err(Error::Dependency);
                    }
                } else {
                    if spawn_ids.insert(spawn.id, reference).is_some() {
                        return Err(Error::Duplicate);
                    }
                    // Upstream shares modelFlags across all instances of the same model.
                    // Mixed classification would make ignoreM2 load-order dependent.
                    if classifications
                        .insert(spawn.model, spawn.flags & 1)
                        .is_some_and(|flags| flags != spawn.flags & 1)
                    {
                        return Err(Error::Unsupported);
                    }
                    let model = &models[spawn.model];
                    charge_placement(&mut placement_work, model)?;
                    spawn.validate_model_bounds(model)?;
                    *slot = Some(spawn);
                }
            }
            c.finish()?;
        }
        let bounds: Vec<_> = spawns
            .iter()
            .map(|s| s.as_ref().map(|s| s.bounds))
            .collect();
        tree.validate_bounds(&bounds)?;
        let inventory = products.revision();
        let mut h = Sha256::new();
        h.update(POLICY);
        h.update(inventory);
        Ok(Self {
            binding: SnapshotBinding {
                map: products.map,
                content: products.content,
                source_epoch: products.source_epoch,
                inventory,
                policy: h.finalize().into(),
            },
            tree,
            models,
            spawns,
        })
    }
    pub(crate) fn binding(&self) -> SnapshotBinding {
        self.binding
    }

    /// CMaNGOS world coordinates, converted once with VMapManager2's exact expression.
    /// Expired, missing and over-budget inputs never become a clear ray.
    pub(crate) fn query(&self, request: RayRequest, deadline: Instant) -> Result<RayReply, Error> {
        if request.snapshot != self.binding || request.ticket == 0 {
            return Err(Error::Identity);
        }
        let from = request.from.map(f32::from_bits);
        let to = request.to.map(f32::from_bits);
        for value in from.into_iter().chain(to) {
            if !value.is_finite() || value.abs() > 100_000.0 {
                return Err(Error::Format);
            }
        }
        let result = self.segment_internal(
            to_internal(from),
            to_internal(to),
            request.ignore_m2,
            &mut QueryBudget {
                remaining: MAX_QUERY_WORK,
                deadline,
            },
        )?;
        complete_query(request, result, deadline)
    }
    fn segment_internal(
        &self,
        from: Vector,
        to: Vector,
        ignore_m2: bool,
        budget: &mut QueryBudget,
    ) -> Result<StaticRay, Error> {
        budget.take()?;
        let delta = sub(to, from);
        let mut distance = dot(delta, delta).sqrt();
        if !distance.is_finite() || distance > 4096.0 {
            return Err(Error::Budget);
        }
        if distance < 1e-10 {
            return Ok(StaticRay::Miss);
        }
        // G3D Vector3 division computes a reciprocal and multiplies components.
        let ray = Ray {
            origin: from,
            direction: scale(delta, 1.0 / distance),
        };
        self.intersect_internal(ray, &mut distance, ignore_m2, budget)
    }
    fn intersect_internal(
        &self,
        ray: Ray,
        distance: &mut f32,
        ignore_m2: bool,
        budget: &mut QueryBudget,
    ) -> Result<StaticRay, Error> {
        let hit = self
            .tree
            .intersect(ray, distance, budget, |id, distance, budget| {
                let spawn = self
                    .spawns
                    .get(id as usize)
                    .and_then(Option::as_ref)
                    .ok_or(Error::Missing)?;
                spawn.intersect(&self.models[spawn.model], ray, distance, ignore_m2, budget)
            })?;
        Ok(if hit {
            StaticRay::Hit {
                distance: distance.to_bits(),
            }
        } else {
            StaticRay::Miss
        })
    }
}

fn complete_query(
    request: RayRequest,
    result: StaticRay,
    deadline: Instant,
) -> Result<RayReply, Error> {
    // A completed last work step does not renew a caller's authority window after descheduling.
    if Instant::now() >= deadline {
        return Err(Error::Budget);
    }
    Ok(RayReply { request, result })
}

pub(super) struct QueryBudget {
    remaining: usize,
    deadline: Instant,
}
impl QueryBudget {
    fn take(&mut self) -> Result<(), Error> {
        if self.remaining == 0 || Instant::now() >= self.deadline {
            return Err(Error::Budget);
        }
        self.remaining -= 1;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug)]
struct Ray {
    origin: Vector,
    direction: Vector,
}
#[derive(Clone, Copy, Debug, PartialEq)]
struct Bounds {
    low: Vector,
    high: Vector,
}
impl Bounds {
    fn contains(self, p: Vector) -> bool {
        (0..3).all(|i| self.low[i] <= p[i] && p[i] <= self.high[i])
    }
    fn encloses(self, b: Self) -> bool {
        self.contains(b.low) && self.contains(b.high)
    }
    fn triangle(points: [Vector; 3]) -> Self {
        Self {
            low: std::array::from_fn(|i| points[0][i].min(points[1][i]).min(points[2][i])),
            high: std::array::from_fn(|i| points[0][i].max(points[1][i]).max(points[2][i])),
        }
    }
}
fn to_internal(p: Vector) -> Vector {
    const MID: f32 = 0.5f32 * 64.0f32 * 533.333_3_f32;
    [MID - p[0], MID - p[1], p[2]]
}

#[derive(Debug)]
struct Group {
    bounds: Bounds,
    vertices: Vec<Vector>,
    triangles: Vec<[u32; 3]>,
    tree: Option<Bih>,
}
impl Group {
    fn decode(c: &mut Cursor<'_>) -> Result<Self, Error> {
        let bounds = c.bounds()?;
        c.word()?; // MOGP flags affect area/liquid queries, not IntersectRay.
        c.word()?; // group WMO ID
        c.magic(b"VERT")?;
        let length = c.word()? as usize;
        let count = c.count()?;
        if length != 4 + count * 12 {
            return Err(Error::Framing);
        }
        c.require(count * 12)?;
        let mut vertices = allocate(count)?;
        for _ in 0..count {
            let p = c.vector()?;
            if !bounds.contains(p) {
                return Err(Error::Format);
            }
            vertices.push(p);
        }
        if count == 0 {
            return Ok(Self {
                bounds,
                vertices,
                triangles: vec![],
                tree: None,
            });
        }
        c.magic(b"TRIM")?;
        let length = c.word()? as usize;
        let count = c.count()?;
        if length != 4 + count * 12 {
            return Err(Error::Framing);
        }
        c.require(count * 12)?;
        let mut triangles = allocate(count)?;
        let mut triangle_bounds = allocate(count)?;
        for _ in 0..count {
            let tri = [c.word()?, c.word()?, c.word()?];
            if tri.iter().any(|i| *i as usize >= vertices.len()) {
                return Err(Error::Format);
            }
            triangle_bounds.push(Some(Bounds::triangle(tri.map(|i| vertices[i as usize]))));
            triangles.push(tri);
        }
        c.magic(b"MBIH")?;
        let tree = Bih::decode(c)?;
        tree.validate_bounds(&triangle_bounds)?;
        c.magic(b"LIQU")?;
        if c.word()? != 0 {
            return Err(Error::Unsupported);
        }
        Ok(Self {
            bounds,
            vertices,
            triangles,
            tree: Some(tree),
        })
    }
    fn intersect(
        &self,
        ray: Ray,
        distance: &mut f32,
        budget: &mut QueryBudget,
    ) -> Result<bool, Error> {
        budget.take()?;
        if self.triangles.is_empty() {
            return Ok(false);
        }
        self.tree.as_ref().ok_or(Error::Missing)?.intersect(
            ray,
            distance,
            budget,
            |id, distance, budget| {
                budget.take()?;
                Ok(triangle(
                    self.triangles[id as usize].map(|i| self.vertices[i as usize]),
                    ray,
                    distance,
                ))
            },
        )
    }
}
#[derive(Debug)]
struct Model {
    groups: Vec<Group>,
    tree: Option<Bih>,
    validation_work: usize,
}
impl Model {
    fn decode(data: &[u8]) -> Result<Self, Error> {
        let mut c = Cursor::new(data);
        c.magic(b"VMAP_7.0WMOD")?;
        if c.word()? != 8 {
            return Err(Error::Framing);
        }
        c.word()?; // Root WMO ID
        if c.remaining() == 0 {
            return Ok(Self {
                groups: vec![],
                tree: None,
                validation_work: 1,
            });
        }
        c.magic(b"GMOD")?;
        let count = c.count()?;
        if count == 0 || count > MAX_GROUPS {
            return Err(Error::Budget);
        }
        let mut groups = allocate(count)?;
        for _ in 0..count {
            groups.push(Group::decode(&mut c)?);
        }
        c.magic(b"GBIH")?;
        let tree = Bih::decode(&mut c)?;
        let bounds: Vec<_> = groups.iter().map(|g| Some(g.bounds)).collect();
        tree.validate_bounds(&bounds)?;
        c.finish()?;
        let validation_work =
            1 + groups.len() + groups.iter().map(|g| g.vertices.len()).sum::<usize>();
        Ok(Self {
            groups,
            tree: Some(tree),
            validation_work,
        })
    }
    fn intersect(
        &self,
        ray: Ray,
        distance: &mut f32,
        budget: &mut QueryBudget,
    ) -> Result<bool, Error> {
        budget.take()?;
        if self.groups.len() == 1 {
            return self.groups[0].intersect(ray, distance, budget);
        }
        if self.groups.is_empty() {
            return Ok(false);
        }
        self.tree.as_ref().ok_or(Error::Missing)?.intersect(
            ray,
            distance,
            budget,
            |id, distance, budget| self.groups[id as usize].intersect(ray, distance, budget),
        )
    }
}
fn charge_placement(work: &mut usize, model: &Model) -> Result<(), Error> {
    *work = work
        .checked_add(model.validation_work)
        .ok_or(Error::Budget)?;
    if *work > 1_048_576 {
        return Err(Error::Budget);
    }
    Ok(())
}

#[derive(Debug)]
struct Spawn {
    flags: u32,
    adt: u16,
    id: u32,
    position: Vector,
    angles: Vector,
    scale: f32,
    inverse: Matrix,
    inverse_scale: f32,
    bounds: Bounds,
    model: usize,
}
impl Spawn {
    fn decode(c: &mut Cursor<'_>, models: &BTreeMap<&str, usize>) -> Result<(usize, Self), Error> {
        let flags = c.word()?;
        if flags & !5 != 0 || flags & 4 == 0 {
            return Err(Error::Unsupported);
        }
        let adt = u16::from_le_bytes(c.take(2)?.try_into().map_err(|_| Error::Framing)?);
        let id = c.word()?;
        let position = c.vector()?;
        let angles = c.vector()?;
        if angles.iter().any(|a| a.abs() > 360.0) {
            return Err(Error::Unsupported);
        }
        let scale = c.float()?;
        if !(0.001..=1000.0).contains(&scale) {
            return Err(Error::Unsupported);
        }
        let bounds = c.bounds()?;
        let length = c.count()?;
        if length == 0 || length > 156 {
            return Err(Error::Identity);
        }
        let name = std::str::from_utf8(c.take(length)?).map_err(|_| Error::Identity)?;
        let model = *models
            .get(format!("{name}.vmo").as_str())
            .ok_or(Error::Missing)?;
        let reference = c.word()? as usize;
        Ok((
            reference,
            Self {
                flags,
                adt,
                id,
                position,
                angles,
                scale,
                inverse: inverse(rotation(angles)?)?,
                inverse_scale: 1.0 / scale,
                bounds,
                model,
            },
        ))
    }
    fn same_source(&self, other: &Self) -> bool {
        self.flags == other.flags
            && self.adt == other.adt
            && self.id == other.id
            && self.model == other.model
            && self.position.map(f32::to_bits) == other.position.map(f32::to_bits)
            && self.angles.map(f32::to_bits) == other.angles.map(f32::to_bits)
            && self.scale.to_bits() == other.scale.to_bits()
            && self.bounds.low.map(f32::to_bits) == other.bounds.low.map(f32::to_bits)
            && self.bounds.high.map(f32::to_bits) == other.bounds.high.map(f32::to_bits)
    }
    fn validate_model_bounds(&self, model: &Model) -> Result<(), Error> {
        let r = rotation(self.angles)?;
        for group in &model.groups {
            for p in &group.vertices {
                // Source assembler transforms vertices with ModelPosition rotation/scale.
                let world = apply(r, scale(*p, self.scale));
                let world = std::array::from_fn(|i| world[i] + self.position[i]);
                if !self.bounds.contains(world) {
                    return Err(Error::Format);
                }
            }
        }
        Ok(())
    }
    fn intersect(
        &self,
        model: &Model,
        ray: Ray,
        distance: &mut f32,
        ignore_m2: bool,
        budget: &mut QueryBudget,
    ) -> Result<bool, Error> {
        budget.take()?;
        if !intersects_box(ray, self.bounds) || (ignore_m2 && self.flags & 1 != 0) {
            return Ok(false);
        }
        let local = Ray {
            origin: scale(
                apply(self.inverse, sub(ray.origin, self.position)),
                self.inverse_scale,
            ),
            direction: apply(self.inverse, ray.direction),
        };
        let mut local_distance = *distance * self.inverse_scale;
        let hit = model.intersect(local, &mut local_distance, budget)?;
        if hit {
            *distance = local_distance * self.scale;
        }
        Ok(hit)
    }
}

struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }
    fn remaining(&self) -> usize {
        self.bytes.len() - self.at
    }
    fn require(&self, n: usize) -> Result<(), Error> {
        if n > self.remaining() {
            Err(Error::Framing)
        } else {
            Ok(())
        }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8], Error> {
        self.require(n)?;
        let value = &self.bytes[self.at..self.at + n];
        self.at += n;
        Ok(value)
    }
    fn magic(&mut self, magic: &[u8]) -> Result<(), Error> {
        if self.take(magic.len())? != magic {
            Err(Error::Format)
        } else {
            Ok(())
        }
    }
    fn word(&mut self) -> Result<u32, Error> {
        super::u32_at(self.take(4)?, 0)
    }
    fn count(&mut self) -> Result<usize, Error> {
        let n = self.word()? as usize;
        if n > MAX_ELEMENTS {
            Err(Error::Budget)
        } else {
            Ok(n)
        }
    }
    fn float(&mut self) -> Result<f32, Error> {
        let v = f32::from_bits(self.word()?);
        if !v.is_finite() || v.abs() > 1_000_000.0 {
            Err(Error::Format)
        } else {
            Ok(v)
        }
    }
    fn vector(&mut self) -> Result<Vector, Error> {
        Ok([self.float()?, self.float()?, self.float()?])
    }
    fn bounds(&mut self) -> Result<Bounds, Error> {
        let b = Bounds {
            low: self.vector()?,
            high: self.vector()?,
        };
        if (0..3).any(|i| b.low[i] > b.high[i]) {
            Err(Error::Format)
        } else {
            Ok(b)
        }
    }
    fn finish(&self) -> Result<(), Error> {
        if self.remaining() != 0 {
            Err(Error::Framing)
        } else {
            Ok(())
        }
    }
}
fn allocate<T>(n: usize) -> Result<Vec<T>, Error> {
    if n > MAX_ELEMENTS {
        return Err(Error::Budget);
    }
    let mut v = Vec::new();
    v.try_reserve_exact(n).map_err(|_| Error::Budget)?;
    Ok(v)
}
#[cfg(test)]
pub(crate) mod tests;
