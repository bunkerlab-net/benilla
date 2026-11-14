// SPDX-License-Identifier: GPL-3.0-or-later
//! GameObject.cpp:125-131,2067-2139; GameObjectModel.cpp:30-205.
use super::*;
use crate::native::production_compiler::VerifiedCatalog;
use benilla_native_vmap::live::{
    DynamicError, DynamicModelHandle, DynamicModelPose, DynamicModels, GameObjectModelList,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

pub(super) fn dynamic_error(error: DynamicError) -> Refusal {
    match error {
        DynamicError::Vmap(error) => vmap_manager::vmap_error(error),
        error => refuse(match error {
            DynamicError::ModelListReadUndefined => {
                "execution.dynamic_collision.model_list_read_undefined"
            }
            DynamicError::CellConversionUndefined => {
                "execution.dynamic_collision.cell_conversion_undefined"
            }
            DynamicError::CellIndexUndefined => "execution.dynamic_collision.cell_index_undefined",
            DynamicError::CellIndexAssertion => "execution.dynamic_collision.cell_index_assertion",
            DynamicError::ModelAllocationUnbound => {
                "execution.dynamic_collision.GameObjectModel_allocation_owner"
            }
            DynamicError::ModelAllocationOverlap => {
                "execution.dynamic_collision.model_allocation_overlap"
            }
            DynamicError::StaleModel => "execution.dynamic_collision.stale_model_generation",
            DynamicError::UnbalancedCounterOverflow => {
                "execution.dynamic_collision.unbalanced_counter_overflow_undefined"
            }
            DynamicError::Vmap(_) => unreachable!(),
        }),
    }
}
#[derive(Clone, Copy)]
pub struct DynamicGameObject {
    pub handle: ObjectHandle,
    pub display: u32,
    pub pose: SourcePose,
    pub scale_bits: u32,
}
#[derive(Default)]
pub struct SourceDynamicModels {
    pub models: DynamicModels,
    list: Option<GameObjectModelList>,
    list_complete: bool,
    displays: Option<BTreeSet<u32>>,
    instances: BTreeMap<ObjectHandle, DynamicModelHandle>,
    constructing: BTreeMap<ObjectHandle, DynamicModelHandle>,
    allocations: BTreeMap<ObjectHandle, VecDeque<(u64, u64)>>,
    live_allocations: BTreeMap<u64, (u64, ObjectHandle)>,
}
pub struct SourceDynamicModelState {
    pub models: Vec<benilla_native_vmap::live::DynamicModelInventory>,
    pub next_generation: u64,
    pub instances: Vec<(ObjectHandle, DynamicModelHandle)>,
    pub constructing: Vec<(ObjectHandle, DynamicModelHandle)>,
    pub construction_fields: Vec<benilla_native_vmap::live::DynamicModelConstruction>,
    pub pending_allocations: Vec<(ObjectHandle, Vec<(u64, u64)>)>,
    pub live_allocations: Vec<(u64, u64, ObjectHandle)>,
    pub model_list_loaded: bool,
    pub model_list_attempted: bool,
    pub display_store_loaded: bool,
}
impl SourceDynamicModels {
    pub fn inventory(&self) -> SourceDynamicModelState {
        SourceDynamicModelState {
            models: self.models.inventory(),
            next_generation: self.models.next_generation(),
            instances: self
                .instances
                .iter()
                .map(|(h, m)| (*h, m.clone()))
                .collect(),
            constructing: self
                .constructing
                .iter()
                .map(|(h, m)| (*h, m.clone()))
                .collect(),
            construction_fields: self.models.construction_inventory(),
            pending_allocations: self
                .allocations
                .iter()
                .map(|(h, q)| (*h, q.iter().copied().collect()))
                .collect(),
            live_allocations: self
                .live_allocations
                .iter()
                .map(|(base, (end, h))| (*base, *end, *h))
                .collect(),
            model_list_loaded: self.list_complete,
            model_list_attempted: self.list.is_some(),
            display_store_loaded: self.displays.is_some(),
        }
    }
    pub fn load_model_list(&mut self, vmap: &SourceVMapManager) -> Result<()> {
        self.list_complete = false;
        self.list
            .get_or_insert_default()
            .load(&vmap.collision.files)
            .map_err(dynamic_error)?;
        self.list_complete = true;
        Ok(())
    }
    pub fn bind_displays(&mut self, displays: impl IntoIterator<Item = u32>) -> Result<()> {
        require(
            self.displays.is_none(),
            "execution.dynamic_collision.display_store_replacement",
        )?;
        self.displays = Some(displays.into_iter().collect());
        Ok(())
    }
    pub fn load_displays(&mut self, catalog: &mut VerifiedCatalog) -> Result<()> {
        let mut dbc = catalog.dbc_view("GameObjectDisplayInfo.dbc")?;
        let mut displays = BTreeSet::new();
        for ordinal in 0..dbc.record_count() {
            let row = dbc.record(ordinal)?;
            displays.insert(
                row["key"]
                    .as_u64()
                    .and_then(|v| u32::try_from(v).ok())
                    .ok_or_else(|| refuse("execution.dynamic_collision.display_key"))?,
            );
        }
        self.bind_displays(displays)
    }
    /// Actual GameObjectModel allocations, not GameObject addresses or supplied iteration order.
    /// Uses the same nonoverlapping source-extent contract as SourceObjects::bind_source_allocation.
    pub fn bind_allocation(&mut self, object: ObjectHandle, base: u64, bytes: u64) -> Result<()> {
        require(
            base != 0 && bytes != 0,
            "execution.dynamic_collision.model_allocation_extent",
        )?;
        let end = base
            .checked_add(bytes)
            .ok_or_else(|| refuse("execution.dynamic_collision.model_allocation_extent"))?;
        self.allocations
            .entry(object)
            .or_default()
            .push_back((base, end));
        Ok(())
    }
    pub fn handle(&self, object: ObjectHandle) -> Result<&DynamicModelHandle> {
        self.instances
            .get(&object)
            .ok_or_else(|| refuse("execution.dynamic_collision.model_handle_owner"))
    }
    pub fn operation(
        &mut self,
        map: &mut SourceDynamicMap,
        vmap: &mut SourceVMapManager,
        object: DynamicGameObject,
        operation: GameObjectModelOperation,
    ) -> Result<bool> {
        use GameObjectModelOperation as O;
        let position = std::array::from_fn(|i| f32::from_bits(object.pose.0[i]));
        let orientation = f32::from_bits(object.pose.0[3]);
        match operation {
            O::ConstructAndInsert => {
                require(
                    !self.instances.contains_key(&object.handle),
                    "execution.dynamic_collision.model_already_constructed",
                )?;
                require(
                    !self.constructing.contains_key(&object.handle),
                    "execution.dynamic_collision.model_construction_in_flight",
                )?;
                let displays = self.displays.as_ref().ok_or_else(|| {
                    refuse("execution.dynamic_collision.GameObjectDisplayInfo_store_owner")
                })?;
                if !displays.contains(&object.display) {
                    return Ok(false);
                }
                let allocation = self
                    .allocations
                    .get_mut(&object.handle)
                    .and_then(VecDeque::pop_front);
                if let Some((base, end)) = allocation {
                    require(
                        self.live_allocations
                            .range(..=base)
                            .next_back()
                            .is_none_or(|(_, (old_end, _))| *old_end <= base)
                            && self
                                .live_allocations
                                .range(base..)
                                .next()
                                .is_none_or(|(&next, _)| end <= next),
                        "execution.dynamic_collision.model_allocation_overlap",
                    )?;
                }
                let list = self
                    .list
                    .as_ref()
                    .filter(|_| self.list_complete)
                    .ok_or_else(|| {
                        refuse("execution.dynamic_collision.LoadGameObjectModelList_owner")
                    })?;
                let before = self.models.next_generation();
                let constructed = self.models.construct(
                    &mut vmap.collision,
                    list,
                    Some(object.display),
                    DynamicModelPose {
                        position,
                        orientation,
                        scale: f32::from_bits(object.scale_bits),
                    },
                    allocation.map(|a| a.0),
                );
                let model = match constructed {
                    Ok(model) => model,
                    Err(error) => {
                        if let Some(partial) = self.models.latest_construction() {
                            if partial.handle.native_generation() > before {
                                self.constructing
                                    .insert(object.handle, partial.handle.clone());
                                if let Some((base, end)) = allocation {
                                    self.live_allocations.insert(base, (end, object.handle));
                                }
                            }
                        }
                        return Err(dynamic_error(error));
                    }
                };
                let Some(model) = model else {
                    return Ok(false);
                };
                if let Some((base, end)) = allocation {
                    self.live_allocations.insert(base, (end, object.handle));
                }
                self.instances.insert(object.handle, model.clone());
                map.tree
                    .insert(&self.models, &model)
                    .map_err(dynamic_error)?;
                Ok(true)
            }
            O::Contains => Ok(map.tree.contains(self.handle(object.handle)?)),
            O::Insert => {
                map.tree
                    .insert(&self.models, self.handle(object.handle)?)
                    .map_err(dynamic_error)?;
                Ok(true)
            }
            O::Remove => {
                map.tree
                    .remove(self.handle(object.handle)?)
                    .map_err(dynamic_error)?;
                Ok(true)
            }
            O::Enable(enabled) => {
                let handle = self.handle(object.handle)?.clone();
                self.models
                    .enable(&handle, enabled)
                    .map_err(dynamic_error)?;
                Ok(true)
            }
            O::Relocate => {
                let handle = self.handle(object.handle)?.clone();
                let list = self
                    .list
                    .as_ref()
                    .filter(|_| self.list_complete)
                    .ok_or_else(|| {
                        refuse("execution.dynamic_collision.LoadGameObjectModelList_owner")
                    })?;
                self.models
                    .relocate(
                        &vmap.collision,
                        list,
                        &handle,
                        object.display,
                        position,
                        orientation,
                    )
                    .map_err(dynamic_error)
            }
            O::Destroy => {
                let handle = self.handle(object.handle)?.clone();
                self.models
                    .release(&mut vmap.collision, &handle)
                    .map_err(dynamic_error)?;
                self.instances.remove(&object.handle);
                if let Some(pointer) = handle.pointer() {
                    self.live_allocations.remove(&pointer);
                }
                Ok(true)
            }
        }
    }
}

#[cfg(test)]
#[path = "dynamic_census_tests.rs"]
mod census_tests;
#[cfg(test)]
#[path = "dynamic_tests.rs"]
pub(super) mod tests;
