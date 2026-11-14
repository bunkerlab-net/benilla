// SPDX-License-Identifier: GPL-3.0-or-later
//! CMaNGOS 8ec338a1 src/game/vmap/GameObjectModel.cpp:30-205.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicError {
    Vmap(Error),
    ModelListReadUndefined,
    CellConversionUndefined,
    CellIndexUndefined,
    CellIndexAssertion,
    ModelAllocationUnbound,
    ModelAllocationOverlap,
    StaleModel,
    UnbalancedCounterOverflow,
}
impl From<Error> for DynamicError {
    fn from(value: Error) -> Self {
        Self::Vmap(value)
    }
}
pub type DynamicResult<T> = std::result::Result<T, DynamicError>;
#[derive(Clone, Debug)]
struct ModelData {
    name: Arc<[u8]>,
    bounds: Bounds,
}
#[derive(Default)]
pub struct GameObjectModelList {
    entries: BTreeMap<u32, ModelData>,
}
impl GameObjectModelList {
    /// Absence is distinct from an uncaptured file in ProductFiles.
    pub fn load(&mut self, files: &ProductFiles) -> DynamicResult<()> {
        let Some(bytes) = files.read(b"temp_gameobject_models")? else {
            return Ok(());
        };
        let mut at = 0;
        let mut eof = false;
        let mut display_bytes = [0; 4];
        let mut length_bytes = [0; 4];
        let mut display_known = 0;
        let mut length_known = 0;
        let mut name_bytes = [0; 500];
        let mut name_known = 0;
        while !eof {
            let mut read = |out: &mut [u8]| {
                let n = out.len().min(bytes.len() - at);
                out[..n].copy_from_slice(&bytes[at..at + n]);
                at += n;
                eof |= n < out.len();
                n
            };
            display_known = display_known.max(read(&mut display_bytes));
            length_known = length_known.max(read(&mut length_bytes));
            if length_known != 4 {
                return Err(DynamicError::ModelListReadUndefined);
            }
            let len = u32::from_le_bytes(length_bytes) as usize;
            if len >= 500 {
                break;
            }
            name_known = name_known.max(read(&mut name_bytes[..len]));
            // Vector3 default initialization makes short bounds reads defined.
            let mut bounds_bytes = [0; 24];
            read(&mut bounds_bytes[..12]);
            read(&mut bounds_bytes[12..]);
            if display_known != 4 || name_known < len {
                return Err(DynamicError::ModelListReadUndefined);
            }
            let bounds = Cursor::new(&bounds_bytes).bounds()?;
            self.entries
                .entry(u32::from_le_bytes(display_bytes))
                .or_insert_with(|| ModelData {
                    name: Arc::from(&name_bytes[..len]),
                    bounds,
                });
        }
        Ok(())
    }
    pub fn bounds(&self, display: u32) -> Option<[[f32; 3]; 2]> {
        self.entries
            .get(&display)
            .map(|m| [m.bounds.low, m.bounds.high])
    }
    pub fn name(&self, display: u32) -> Option<&Arc<[u8]>> {
        self.entries.get(&display).map(|m| &m.name)
    }
}
#[derive(Clone, Debug)]
pub struct DynamicModelHandle {
    identity: Arc<Option<u64>>,
    key: u64,
}
impl DynamicModelHandle {
    pub fn pointer(&self) -> Option<u64> {
        *self.identity
    }
    pub(super) fn key(&self) -> u64 {
        self.key
    }
    pub fn native_generation(&self) -> u64 {
        self.key
    }
}
impl PartialEq for DynamicModelHandle {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.identity, &other.identity)
    }
}
impl Eq for DynamicModelHandle {}
struct GameObjectModel {
    handle: DynamicModelHandle,
    shared: ModelGeneration,
    position: Vector,
    inverse: Matrix,
    scale: f32,
    inverse_scale: f32,
    bounds: Bounds,
    enabled: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct DynamicModelInventory {
    pub handle: DynamicModelHandle,
    pub shared: ModelGeneration,
    pub position: Vector,
    pub inverse: Matrix,
    pub scale: f32,
    pub inverse_scale: f32,
    pub bounds: [[f32; 3]; 2],
    pub enabled: bool,
}
#[derive(Clone, Copy, Debug)]
pub struct DynamicModelPose {
    pub position: Vector,
    pub orientation: f32,
    pub scale: f32,
}
#[derive(Clone, Debug)]
pub struct DynamicModelConstruction {
    pub handle: DynamicModelHandle,
    pub shared: ModelGeneration,
    pub display: u32,
    pub model_bounds: [[f32; 3]; 2],
    pub position: Vector,
    pub orientation: f32,
    pub scale: f32,
    pub inverse_scale: f32,
    pub enabled: bool,
    pub refusal: DynamicError,
}
#[derive(Default)]
pub struct DynamicModels {
    models: BTreeMap<u64, GameObjectModel>,
    next_key: u64,
    pointers: BTreeMap<u64, u64>,
    constructing: BTreeMap<u64, DynamicModelConstruction>,
}
impl DynamicModels {
    pub fn next_generation(&self) -> u64 {
        self.next_key
    }
    pub fn construction_inventory(&self) -> Vec<DynamicModelConstruction> {
        self.constructing.values().cloned().collect()
    }
    pub fn latest_construction(&self) -> Option<&DynamicModelConstruction> {
        self.constructing.last_key_value().map(|(_, value)| value)
    }
    pub fn inventory(&self) -> Vec<DynamicModelInventory> {
        self.models
            .values()
            .map(|m| DynamicModelInventory {
                handle: m.handle.clone(),
                shared: m.shared.clone(),
                position: m.position,
                inverse: m.inverse,
                scale: m.scale,
                inverse_scale: m.inverse_scale,
                bounds: [m.bounds.low, m.bounds.high],
                enabled: m.enabled,
            })
            .collect()
    }
    fn model(&self, h: &DynamicModelHandle) -> DynamicResult<&GameObjectModel> {
        self.models
            .get(&h.key)
            .filter(|m| m.handle == *h)
            .ok_or(DynamicError::StaleModel)
    }
    fn model_mut(&mut self, h: &DynamicModelHandle) -> DynamicResult<&mut GameObjectModel> {
        self.models
            .get_mut(&h.key)
            .filter(|m| m.handle == *h)
            .ok_or(DynamicError::StaleModel)
    }
    pub(super) fn bounds(&self, h: &DynamicModelHandle) -> DynamicResult<Bounds> {
        Ok(self.model(h)?.bounds)
    }
    pub(super) fn equivalent(
        &self,
        a: &DynamicModelHandle,
        b: &DynamicModelHandle,
    ) -> DynamicResult<bool> {
        let a = self.model(a)?;
        let b = self.model(b)?;
        let bits = |v: Vector| v.map(f32::to_bits);
        Ok(a.shared == b.shared
            && a.enabled == b.enabled
            && bits(a.position) == bits(b.position)
            && a.inverse.map(bits) == b.inverse.map(bits)
            && a.scale.to_bits() == b.scale.to_bits()
            && a.inverse_scale.to_bits() == b.inverse_scale.to_bits()
            && bits(a.bounds.low) == bits(b.bounds.low)
            && bits(a.bounds.high) == bits(b.bounds.high))
    }
    /// Source addresses bind bucket history, not successful resource construction.
    pub fn construct(
        &mut self,
        manager: &mut Manager,
        list: &GameObjectModelList,
        display: Option<u32>,
        pose: DynamicModelPose,
        allocation: Option<u64>,
    ) -> DynamicResult<Option<DynamicModelHandle>> {
        let DynamicModelPose {
            position,
            orientation,
            scale,
        } = pose;
        let Some(display) = display else {
            return Ok(None);
        };
        let pointer = allocation.filter(|p| *p != 0);
        if pointer.is_some_and(|p| self.pointers.contains_key(&p)) {
            return Err(DynamicError::ModelAllocationOverlap);
        }
        let Some(data) = list.entries.get(&display) else {
            return Ok(None);
        };
        arithmetic_environment()?;
        if data.bounds.low == [0.0; 3] && data.bounds.high == [0.0; 3] {
            return Ok(None);
        }
        let Some(shared) = manager.acquire_model(&data.name)? else {
            return Ok(None);
        };
        self.next_key = self.next_key.checked_add(1).ok_or(Error::Budget)?;
        let handle = DynamicModelHandle {
            identity: Arc::new(pointer),
            key: self.next_key,
        };
        if let Some(pointer) = pointer {
            self.pointers.insert(pointer, handle.key);
        }
        let inverse_scale = 1.0 / scale;
        let (inverse, bounds) = match transform(manager, data.bounds, position, orientation, scale)
        {
            Ok(value) => value,
            Err(refusal) => {
                // Retain the source's live locals at the refused rotation call.
                self.constructing.insert(
                    handle.key,
                    DynamicModelConstruction {
                        handle,
                        shared,
                        display,
                        model_bounds: [data.bounds.low, data.bounds.high],
                        position,
                        orientation,
                        scale,
                        inverse_scale,
                        enabled: true,
                        refusal,
                    },
                );
                return Err(refusal);
            }
        };
        self.models.insert(
            handle.key,
            GameObjectModel {
                handle: handle.clone(),
                shared,
                position,
                inverse,
                scale,
                inverse_scale,
                bounds,
                enabled: true,
            },
        );
        Ok(Some(handle))
    }
    pub fn enable(&mut self, h: &DynamicModelHandle, enabled: bool) -> DynamicResult<()> {
        self.model_mut(h)?.enabled = enabled;
        Ok(())
    }
    pub fn relocate(
        &mut self,
        manager: &Manager,
        list: &GameObjectModelList,
        h: &DynamicModelHandle,
        display: u32,
        position: Vector,
        orientation: f32,
    ) -> DynamicResult<bool> {
        let m = self.model_mut(h)?;
        manager.validate_model(&m.shared)?;
        let Some(data) = list.entries.get(&display) else {
            return Ok(false);
        };
        arithmetic_environment()?;
        if data.bounds.low == [0.0; 3] && data.bounds.high == [0.0; 3] {
            return Ok(false);
        }
        m.position = position;
        let (inverse, bounds) = transform(manager, data.bounds, position, orientation, m.scale)?;
        m.inverse = inverse;
        m.bounds = bounds;
        Ok(true)
    }
    pub fn release(&mut self, manager: &mut Manager, h: &DynamicModelHandle) -> DynamicResult<()> {
        self.model(h)?;
        let m = self.models.remove(&h.key).unwrap();
        if let Some(pointer) = h.pointer() {
            self.pointers.remove(&pointer);
        }
        manager.validate_model(&m.shared)?;
        manager.release(&m.shared.name);
        Ok(())
    }
    pub(super) fn intersect(
        &self,
        manager: &Manager,
        h: &DynamicModelHandle,
        ray: Ray,
        distance: &mut f32,
        budget: &mut QueryBudget,
        ignore_m2: bool,
    ) -> DynamicResult<bool> {
        let m = self.model(h)?;
        if !m.enabled || !intersects_box(ray, m.bounds) {
            return Ok(false);
        }
        let shared = manager.model(&m.shared)?;
        if ignore_m2 && shared.flags & 1 != 0 {
            return Ok(false);
        }
        let ray = Ray {
            origin: scale(
                apply(m.inverse, sub(ray.origin, m.position)),
                m.inverse_scale,
            ),
            direction: apply(m.inverse, ray.direction),
        };
        let mut local = *distance * m.inverse_scale;
        let hit = shared.model.ray(ray, &mut local, budget, true)?;
        if hit {
            *distance = local * m.scale;
        }
        Ok(hit)
    }
}
fn transform(
    manager: &Manager,
    bounds: Bounds,
    position: Vector,
    orientation: f32,
    scale_by: f32,
) -> DynamicResult<(Matrix, Bounds)> {
    let sin_cos = |angle: f32| -> Result<(f32, f32)> {
        if angle != 0.0 && manager.rotation_dispatch.is_none() {
            return Err(Error::NumericBinding);
        }
        let [s, c] = benilla_sim::source_trig_fma::sin_cos_bits_native(angle.to_bits());
        Ok((f32::from_bits(s), f32::from_bits(c)))
    };
    // GameObjectModel takes radians directly, unlike ModelInstance's degrees.
    let rotation = rotation_with([0.0, orientation, 0.0], sin_cos)?;
    let inverse = inverse(rotation)?;
    let low = scale(bounds.low, scale_by);
    let high = scale(bounds.high, scale_by);
    // Vector3's default constructor zeroes AABox's two members, Vector3.h:572.
    let mut bounds = Bounds {
        low: [0.0; 3],
        high: [0.0; 3],
    };
    for indices in [
        [0, 0, 1],
        [1, 0, 1],
        [1, 1, 1],
        [0, 1, 1],
        [0, 0, 0],
        [1, 0, 0],
        [1, 1, 0],
        [0, 1, 0],
    ] {
        let p = apply(
            rotation,
            std::array::from_fn(|i| [low, high][indices[i]][i]),
        );
        for (i, value) in p.into_iter().enumerate() {
            bounds.low[i] = if bounds.low[i] < value {
                bounds.low[i]
            } else {
                value
            };
            bounds.high[i] = if value < bounds.high[i] {
                bounds.high[i]
            } else {
                value
            };
        }
    }
    for i in 0..3 {
        bounds.low[i] += position[i];
        bounds.high[i] += position[i];
    }
    Ok((inverse, bounds))
}
