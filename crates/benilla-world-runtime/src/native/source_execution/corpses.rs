// SPDX-License-Identifier: GPL-3.0-or-later
// CMaNGOS Classic 8ec338a1 contributors; upstream GPL-2.0-or-later.
//! Global resurrectable corpse ownership, independent of map/grid residency.
use super::identity_table::IdentityTable;
use super::*;
use benilla_sim::{
    corpse_appearance::{self, EquippedItem, OwnerAppearance},
    resident_map::{compute_cell_pair, compute_grid_pair, CellPair},
};
use std::collections::BTreeMap;

pub const SOURCE_CORPSE_FIELDS: usize = 38;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u32)]
pub enum SourceCorpseType {
    Bones = 0,
    Pve = 1,
    Pvp = 2,
}
impl SourceCorpseType {
    fn object_kind(self) -> SourceObjectKind {
        if self == Self::Bones {
            SourceObjectKind::Bones
        } else {
            SourceObjectKind::Corpse
        }
    }
}
#[derive(Clone, Debug)]
pub struct SourceCorpse {
    pub object: ObjectHandle,
    pub corpse_type: SourceCorpseType,
    pub created_seconds: i64,
    pub map: u32,
    pub instance: u32,
    pub grid: [u16; 2],
    pub position: [u32; 4],
    pub rank_snapshot: u8,
    fields: [u32; SOURCE_CORPSE_FIELDS],
}
impl SourceCorpse {
    pub fn fields(&self) -> &[u32; SOURCE_CORPSE_FIELDS] {
        &self.fields
    }
    pub fn owner(&self) -> SourceGuid {
        SourceGuid::from_raw(u64::from(self.fields[6]) | (u64::from(self.fields[7]) << 32))
    }
    pub fn set_field(&mut self, index: usize, value: u32) -> Result<()> {
        let field = self
            .fields
            .get_mut(index)
            .ok_or_else(|| refuse("execution.corpse.field_index"))?;
        *field = value;
        Ok(())
    }
    pub fn set_appearance(&mut self, appearance: &OwnerAppearance) {
        let appearance = corpse_appearance::capture_ordinary(appearance);
        self.fields[12] = appearance.native_display_id;
        self.fields[13..32].copy_from_slice(&appearance.equipment);
        self.fields[32] = appearance.bytes_1;
        self.fields[33] = appearance.bytes_2;
        self.fields[34] = appearance.guild_id;
        self.fields[35] = appearance.flags;
    }
    // Only the owner registry is swept; bones have no periodic caller; Corpse.cpp:270-274, ObjectAccessor.cpp:293-301.
    pub fn is_expired(&self, now: i64) -> Result<bool> {
        let age = if self.corpse_type == SourceCorpseType::Bones {
            3600
        } else {
            259200
        };
        let threshold = now
            .checked_sub(age)
            .ok_or_else(|| refuse("execution.corpse.expiry_subtraction_undefined"))?;
        Ok(self.created_seconds < threshold)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CorpseInventory {
    pub object: ObjectHandle,
    pub corpse_type: u32,
    pub owner: SourceGuid,
    pub created_seconds: i64,
    pub map: u32,
    pub instance: u32,
    pub grid: [u16; 2],
    pub position: [u32; 4],
    pub rank_snapshot: u8,
    pub fields: Vec<u32>,
    pub map_binding: Option<MapIdentity>,
    pub reference: Option<CorpseGridReference>,
    pub in_world: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct CorpseGridReference {
    pub grid: GridIdentity,
    pub cell: [u16; 2],
}
pub type CorpseCellInventory = (u32, u32, Vec<(u32, u32)>);
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CorpseRegistryInventory {
    pub initialized: bool,
    pub stopped: bool,
    pub buckets: usize,
    pub owners_in_source_order: Vec<(SourceGuid, ObjectHandle)>,
    pub corpses: Vec<CorpseInventory>,
    pub cells: Vec<CorpseCellInventory>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseEvent {
    Construct(ObjectHandle),
    FailedLoad(ObjectHandle),
    Loaded(ObjectHandle),
    Add(ObjectHandle),
    Remove(ObjectHandle),
    Destroy(ObjectHandle),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseSql {
    Begin,
    DeleteOwner(u32),
    Insert {
        guid: u32,
        player: u32,
        position: [u32; 4],
        map: u32,
        time: u64,
        corpse_type: u32,
        instance: u32,
    },
    Commit,
}
pub trait CorpseAppearanceOwner {
    /// Effective GetPlayerInfo, including its zero-display rejection; ObjectMgr.h:522-528.
    fn player_displays(&mut self, race: u8, class: u8) -> Result<Option<[u32; 2]>>;
    fn item(&mut self, id: u32) -> Result<Option<EquippedItem>>;
}
pub trait CorpseWork {
    fn constructor_clock(
        &mut self,
        object: ObjectHandle,
        objects: &mut SourceObjects,
    ) -> Result<ClockSample>;
    fn observe(&mut self, event: CorpseEvent) -> Result<()>;
    fn query_corpses(&mut self) -> Result<CharacterQueryStatus>;
    fn player_rank(&mut self, _player: ObjectHandle) -> Result<u8> {
        Err(refuse("execution.corpse.player_rank_owner"))
    }
    fn sql(&mut self, _operation: CorpseSql) -> Result<()> {
        Err(refuse("execution.corpse.character_sql_owner"))
    }
}
/// Supply the previous map's actual SourceCells, never infer a binding from corpse coordinates.
pub trait CorpseGridOwner {
    fn unlink_other_map(
        &mut self,
        _object: ObjectHandle,
        _previous: CellIdentity,
        _objects: &mut SourceObjects,
    ) -> Result<()> {
        Err(refuse("execution.corpse.cross_map_cell_owner"))
    }
    fn add_active(
        &mut self,
        _object: ObjectHandle,
        _objects: &mut SourceObjects,
        _cells: &mut SourceCells,
    ) -> Result<()> {
        Err(refuse("execution.corpse.active_owner"))
    }
}
/// Concrete cross-map reference mutation over retained map-cell owners.
pub struct CorpseOtherMaps<'a> {
    pub maps: &'a mut BTreeMap<MapIdentity, SourceCells>,
}
impl CorpseGridOwner for CorpseOtherMaps<'_> {
    fn unlink_other_map(
        &mut self,
        object: ObjectHandle,
        previous: CellIdentity,
        objects: &mut SourceObjects,
    ) -> Result<()> {
        let cells = self
            .maps
            .get_mut(&previous.grid.map)
            .ok_or_else(|| refuse("execution.corpse.cross_map_cell_owner"))?;
        require(
            cells.registered_cell(object) == Some(previous),
            "execution.corpse.cross_map_reference",
        )?;
        cells.unlink(object);
        objects.get_mut(object)?.corpse_grid_reference = None;
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CorpseMapPhase {
    RemoveVisibility,
    AddedViewpoint,
    AddedVisibility,
}
pub trait CorpseConversionWork: CorpseWork {
    fn sweep_clock(&mut self) -> Result<ClockSample>;
    fn is_battleground(&mut self, map: MapIdentity) -> Result<bool>;
    fn map_phase(
        &mut self,
        object: ObjectHandle,
        phase: CorpseMapPhase,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
    ) -> Result<()>;
}
pub struct CorpseConversion<'a> {
    pub thread: u64,
    pub bones_world: bool,
    pub bones_battleground: bool,
    pub maps: &'a mut BTreeMap<MapIdentity, SourceCells>,
}
#[derive(Default)]
pub struct SourceCorpseRegistry {
    owners: IdentityTable<ObjectHandle>,
    corpses: BTreeMap<ObjectHandle, SourceCorpse>,
    cells: BTreeMap<(u32, u32), BTreeMap<u32, u32>>,
    initialized: bool,
    stopped: bool,
}
impl SourceCorpseRegistry {
    pub fn inventory(&self, objects: &SourceObjects) -> Result<CorpseRegistryInventory> {
        let mut corpses = Vec::with_capacity(self.corpses.len());
        for corpse in self.corpses.values() {
            let object = objects.get(corpse.object)?;
            corpses.push(CorpseInventory {
                object: corpse.object,
                corpse_type: corpse.corpse_type as u32,
                owner: corpse.owner(),
                created_seconds: corpse.created_seconds,
                map: corpse.map,
                instance: corpse.instance,
                grid: corpse.grid,
                position: corpse.position,
                rank_snapshot: corpse.rank_snapshot,
                fields: corpse.fields.to_vec(),
                map_binding: object.map,
                reference: object.corpse_grid_reference.map(|c| CorpseGridReference {
                    grid: c.grid,
                    cell: [c.x, c.y],
                }),
                in_world: object.in_world,
            });
        }
        Ok(CorpseRegistryInventory {
            initialized: self.initialized,
            stopped: self.stopped,
            buckets: self.owners.bucket_count(),
            owners_in_source_order: self
                .owners
                .iter()
                .map(|(owner, object)| (SourceGuid::from_raw(owner), *object))
                .collect(),
            corpses,
            cells: self
                .cells
                .iter()
                .map(|(&(map, cell), entries)| {
                    (map, cell, entries.iter().map(|(&p, &i)| (p, i)).collect())
                })
                .collect(),
        })
    }
    pub fn corpse(&self, object: ObjectHandle) -> Result<&SourceCorpse> {
        self.corpses
            .get(&object)
            .ok_or_else(|| refuse("execution.corpse.stale_object"))
    }
    pub fn corpse_mut(&mut self, object: ObjectHandle) -> Result<&mut SourceCorpse> {
        require(!self.stopped, "execution.corpse.stopped")?;
        self.corpses
            .get_mut(&object)
            .ok_or_else(|| refuse("execution.corpse.stale_object"))
    }
    pub fn for_player(&self, owner: SourceGuid) -> Result<Option<ObjectHandle>> {
        let object = self.owners.get(owner.raw()).copied();
        if let Some(object) = object {
            require(
                self.corpse(object)?.corpse_type != SourceCorpseType::Bones,
                "execution.corpse.owner_bones_assertion",
            )?;
        }
        Ok(object)
    }
    fn construct(
        &mut self,
        thread: u64,
        kind: SourceCorpseType,
        objects: &mut SourceObjects,
        work: &mut impl CorpseWork,
    ) -> Result<SourceCorpse> {
        let object = objects.construct(thread, kind.object_kind())?;
        work.observe(CorpseEvent::Construct(object))?;
        let time = work.constructor_clock(object, objects)?;
        require(
            time.kind() == ClockKind::WallSeconds,
            "execution.corpse.constructor_clock_kind",
        )?;
        Ok(SourceCorpse {
            object,
            corpse_type: kind,
            created_seconds: time.value(),
            map: 0,
            instance: 0,
            grid: [0; 2],
            position: [0; 4],
            rank_snapshot: 0,
            fields: [0; SOURCE_CORPSE_FIELDS],
        })
    }
    // LoadCorpses constructs default bones before LoadFromDB changes type; ObjectMgr.cpp:7396-7403.
    pub fn load_initial(
        &mut self,
        inputs: &CorpseInitialInputs<'_>,
        thread: u64,
        objects: &mut SourceObjects,
        appearance: &mut impl CorpseAppearanceOwner,
        work: &mut impl CorpseWork,
    ) -> Result<usize> {
        let result = (|| {
            require(
                !self.stopped && !self.initialized,
                "execution.corpse.initialization",
            )?;
            self.initialized = true;
            require(
                inputs
                    .query_owner()
                    .is_none_or(|owner| owner.thread == thread),
                "execution.corpse.query_thread",
            )?;
            let status = work.query_corpses()?;
            inputs.validate_query_result(status)?;
            if status != CharacterQueryStatus::Rows {
                return Ok(0);
            }
            let mut count = 0;
            for (_, row) in inputs.rows() {
                let mut corpse = self.construct(thread, SourceCorpseType::Bones, objects, work)?;
                if !load_fields(&mut corpse, row, objects, appearance)? {
                    objects.destroy_unbound(corpse.object)?;
                    work.observe(CorpseEvent::FailedLoad(corpse.object))?;
                    continue;
                }
                let object = corpse.object;
                self.corpses.insert(object, corpse);
                work.observe(CorpseEvent::Loaded(object))?;
                self.add(object, work)?;
                count += 1;
            }
            Ok(count)
        })();
        self.stopped |= result.is_err();
        result
    }
    // Create assigns the owner's real map but neither registers nor publishes; Corpse.cpp:71-99.
    #[allow(clippy::too_many_arguments)]
    pub fn create_for_player(
        &mut self,
        thread: u64,
        low_guid: u32,
        kind: SourceCorpseType,
        player: ObjectHandle,
        objects: &mut SourceObjects,
        cells: &SourceCells,
        work: &mut impl CorpseWork,
    ) -> Result<Option<ObjectHandle>> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let mut corpse = self.construct(thread, kind, objects, work)?;
            let owner = objects.get(player)?;
            require(
                owner.kind == SourceObjectKind::Player,
                "execution.corpse.owner_player_assertion",
            )?;
            let map = owner
                .map
                .ok_or_else(|| refuse("execution.corpse.player_map_owner"))?;
            let guid = owner
                .guid
                .ok_or_else(|| refuse("execution.object_guid_absent"))?;
            let pose = objects.pose(player)?;
            objects.assign_guid(
                corpse.object,
                0,
                SourceGuid::without_entry(HighGuid::Corpse, low_guid),
            )?;
            objects.set_pose(corpse.object, pose)?;
            objects.bind_map(corpse.object, map, cells)?;
            if !valid_corpse_position(pose.0) {
                objects.reset_map(corpse.object)?;
                objects.destroy_unbound(corpse.object)?;
                return Ok(None);
            }
            initialize_fields(&mut corpse, low_guid);
            corpse.map = map.map;
            corpse.instance = map.instance;
            corpse.position = pose.0;
            corpse.fields[6..8].copy_from_slice(&[guid.raw() as u32, (guid.raw() >> 32) as u32]);
            corpse.fields[8] = pose.0[3];
            corpse.fields[9..12].copy_from_slice(&pose.0[..3]);
            let grid = compute_grid_pair(pose.0[0], pose.0[1])
                .map_err(|_| refuse("execution.corpse.grid_coordinates"))?;
            corpse.grid = [grid.x(), grid.y()];
            corpse.rank_snapshot = work.player_rank(player)?;
            let object = corpse.object;
            self.corpses.insert(object, corpse);
            Ok(Some(object))
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn add(&mut self, object: ObjectHandle, work: &mut impl CorpseWork) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let corpse = self.corpse(object)?;
            require(
                corpse.corpse_type != SourceCorpseType::Bones,
                "execution.corpse.add_bones_assertion",
            )?;
            require(
                self.owners.get(corpse.owner().raw()).is_none(),
                "execution.corpse.duplicate_owner_assertion",
            )?;
            let (owner, map, instance, pair) = (
                corpse.owner(),
                corpse.map,
                corpse.instance,
                corpse_cell(corpse.position)?,
            );
            self.owners.insert(owner.raw(), object, 100_000)?;
            self.cells
                .entry((map, cell_id(pair)))
                .or_default()
                .insert(owner.counter(), instance);
            work.observe(CorpseEvent::Add(object))
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn remove(
        &mut self,
        object: ObjectHandle,
        objects: &mut SourceObjects,
        work: &mut impl CorpseWork,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let corpse = self.corpse(object)?;
            require(
                corpse.corpse_type != SourceCorpseType::Bones,
                "execution.corpse.remove_bones_assertion",
            )?;
            let (owner, map, pair) = (corpse.owner(), corpse.map, corpse_cell(corpse.position)?);
            if self.owners.get(owner.raw()).is_none() {
                return Ok(());
            }
            self.cells
                .entry((map, cell_id(pair)))
                .or_default()
                .remove(&owner.counter());
            objects.remove_from_world(object)?;
            self.owners.erase(owner.raw());
            work.observe(CorpseEvent::Remove(object))
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn save_to_db(
        &mut self,
        object: ObjectHandle,
        objects: &SourceObjects,
        work: &mut impl CorpseWork,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let corpse = self.corpse(object)?;
            require(
                corpse.corpse_type != SourceCorpseType::Bones,
                "execution.corpse.save_bones_assertion",
            )?;
            let guid = objects
                .get(object)?
                .guid
                .ok_or_else(|| refuse("execution.object_guid_absent"))?;
            require(
                objects.get(object)?.map.is_some(),
                "execution.corpse.save_map_owner",
            )?;
            work.sql(CorpseSql::Begin)?;
            work.sql(CorpseSql::DeleteOwner(corpse.owner().counter()))?;
            work.sql(CorpseSql::Insert {
                guid: guid.counter(),
                player: corpse.owner().counter(),
                position: corpse.position,
                map: corpse.map,
                time: corpse.created_seconds as u64,
                corpse_type: corpse.corpse_type as u32,
                instance: corpse.instance,
            })?;
            work.sql(CorpseSql::Commit)
        })();
        self.stopped |= result.is_err();
        result
    }
    // Preadvance the live GCC chain before conversion erases its current node; ObjectAccessor.cpp:289-302.
    pub fn remove_old_corpses(
        &mut self,
        objects: &mut SourceObjects,
        context: &mut CorpseConversion<'_>,
        work: &mut impl CorpseConversionWork,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let now = work.sweep_clock()?;
            require(
                now.kind() == ClockKind::WallSeconds,
                "execution.corpse.sweep_clock_kind",
            )?;
            let mut next = self.owners.first_key();
            while let Some(owner) = next {
                next = self.owners.next_key(owner);
                let object = *self.owners.get(owner).unwrap();
                if self.corpse(object)?.is_expired(now.value())? {
                    self.convert_for_player(
                        SourceGuid::from_raw(owner),
                        false,
                        objects,
                        context,
                        work,
                    )?;
                }
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
    // Remove owner/world/SQL before constructing optional fresh bones; ObjectAccessor.cpp:225-286.
    pub fn convert_for_player(
        &mut self,
        player: SourceGuid,
        insignia: bool,
        objects: &mut SourceObjects,
        context: &mut CorpseConversion<'_>,
        work: &mut impl CorpseConversionWork,
    ) -> Result<Option<ObjectHandle>> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let Some(object) = self.for_player(player)? else {
                return Ok(None);
            };
            self.remove(object, objects, work)?;
            let corpse = self.corpse(object)?;
            let (map_id, instance, position) = (corpse.map, corpse.instance, corpse.position);
            let mut matches = context
                .maps
                .keys()
                .filter(|m| m.map == map_id && m.instance == instance);
            let map = matches.next().copied();
            require(matches.next().is_none(), "execution.corpse.map_generation")?;
            if let Some(map) = map {
                let cells = context.maps.get_mut(&map).unwrap();
                let pair = corpse_cell(position)?;
                if cells.object_data_loaded(pair.grid_x(), pair.grid_y()) {
                    require(
                        !objects.get(object)?.active,
                        "execution.corpse.active_owner",
                    )?;
                    work.map_phase(object, CorpseMapPhase::RemoveVisibility, objects, cells)?;
                    if let Some(reference) = objects.get(object)?.corpse_grid_reference {
                        if reference.grid.map != map {
                            let previous = context
                                .maps
                                .get_mut(&reference.grid.map)
                                .ok_or_else(|| refuse("execution.corpse.cross_map_cell_owner"))?;
                            require(
                                previous.registered_cell(object) == Some(reference),
                                "execution.corpse.cross_map_reference",
                            )?;
                            previous.unlink(object);
                            objects.get_mut(object)?.corpse_grid_reference = None;
                        }
                    }
                    let cells = context.maps.get_mut(&map).unwrap();
                    objects.unlink_cell(object, cells)?;
                    cells.record_removed_guid(objects.get(object)?.guid.unwrap());
                    objects.reset_map(object)?;
                }
            }
            work.sql(CorpseSql::DeleteOwner(player.counter()))?;
            let mut bones = None;
            if let Some(map) = map {
                let enabled = insignia
                    || if work.is_battleground(map)? {
                        context.bones_battleground
                    } else {
                        context.bones_world
                    };
                let grid_pair = compute_grid_pair(position[0], position[1])
                    .map_err(|_| refuse("execution.corpse.grid_coordinates"))?;
                let cells = context.maps.get_mut(&map).unwrap();
                let grid = cells.grids().identity(grid_pair.x(), grid_pair.y());
                let removal = match grid {
                    Some(grid) => cells.grids().phase(grid)? == NGridPhase::Removal,
                    None => true,
                };
                if enabled && !removal {
                    let mut new =
                        self.construct(context.thread, SourceCorpseType::Bones, objects, work)?;
                    let guid = objects
                        .get(object)?
                        .guid
                        .ok_or_else(|| refuse("execution.object_guid_absent"))?;
                    initialize_fields(&mut new, guid.counter());
                    objects.assign_guid(new.object, guid.counter(), guid)?;
                    let old = self.corpse(object)?;
                    new.fields[3..].copy_from_slice(&old.fields[3..]);
                    new.grid = old.grid;
                    new.position = old.position;
                    new.fields[35] = 5;
                    new.fields[6..8]
                        .copy_from_slice(&[player.raw() as u32, (player.raw() >> 32) as u32]);
                    new.fields[13..32].fill(0);
                    new.map = map.map;
                    new.instance = map.instance;
                    objects.set_pose(new.object, SourcePose(new.position))?;
                    objects.bind_map(new.object, map, cells)?;
                    let pair = corpse_cell(new.position)?;
                    let cell = cells.cell(pair)?;
                    objects.link_cell(new.object, cell, cells)?;
                    objects.add_to_world(new.object)?;
                    let handle = new.object;
                    self.corpses.insert(handle, new);
                    work.map_phase(handle, CorpseMapPhase::AddedViewpoint, objects, cells)?;
                    work.map_phase(handle, CorpseMapPhase::AddedVisibility, objects, cells)?;
                    bones = Some(handle);
                }
            }
            // The destructor unlinks even a reference left by a cold Map::Remove; GridReference.h:58-61.
            if let Some(reference) = objects.get(object)?.corpse_grid_reference {
                let cells = context
                    .maps
                    .get_mut(&reference.grid.map)
                    .ok_or_else(|| refuse("execution.corpse.cross_map_cell_owner"))?;
                require(
                    cells.registered_cell(object) == Some(reference),
                    "execution.corpse.cross_map_reference",
                )?;
                cells.unlink(object);
                objects.get_mut(object)?.corpse_grid_reference = None;
            }
            objects.reset_map(object)?;
            objects.destroy_unbound(object)?;
            self.corpses.remove(&object);
            work.observe(CorpseEvent::Destroy(object))?;
            Ok(bones)
        })();
        self.stopped |= result.is_err();
        result
    }
    // Queueing performs cleanup, so the later source lookup may miss; Corpse.cpp:127-138, Map.cpp:1466,1488.
    pub fn delete_bones_from_world<D: GridDefinitions, P: ResidencyProductOwner>(
        &mut self,
        object: ObjectHandle,
        context: &mut GridFactoryContext<'_, D, P>,
        owner: &mut impl ResidencyOwner<D, P>,
    ) -> Result<bool> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            require(
                self.corpse(object)?.corpse_type == SourceCorpseType::Bones,
                "execution.corpse.delete_bones_assertion",
            )?;
            let value = context.objects.get(object)?;
            require(
                value.map == Some(context.cells.map()),
                "execution.corpse.bones_map_owner",
            )?;
            let guid = value
                .guid
                .ok_or_else(|| refuse("execution.object_guid_absent"))?;
            if context
                .objects
                .lookup(context.cells.map(), LiveLookupKind::Corpse, guid)
                .is_none()
            {
                return Ok(false);
            }
            context.add_to_remove(object, owner)?;
            Ok(true)
        })();
        self.stopped |= result.is_err();
        result
    }
    pub(super) fn retire_deleted_bones(&mut self, object: ObjectHandle) -> Result<()> {
        require(
            self.corpse(object)?.corpse_type == SourceCorpseType::Bones,
            "execution.corpse.delete_bones_assertion",
        )?;
        self.corpses.remove(&object);
        Ok(())
    }
    pub fn load_cell(
        &mut self,
        pair: CellPair,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
        owner: &mut dyn CorpseGridOwner,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            let Some(entries) = self.cells.get(&(cells.map().map, cell_id(pair))) else {
                return Ok(());
            };
            for (&player, &instance) in entries {
                if instance != cells.map().instance {
                    continue;
                }
                let Some(object) =
                    self.for_player(SourceGuid::without_entry(HighGuid::Player, player))?
                else {
                    continue;
                };
                let cell = cells.cell(pair)?;
                link_reference(object, cell, objects, cells, owner)?;
                // SetMap follows the reference insertion, even for an already-world corpse; ObjectGridLoader.cpp:179-183.
                objects.get_mut(object)?.map = Some(cells.map());
                objects.add_to_world(object)?;
                if objects.get(object)?.active {
                    owner.add_active(object, objects, cells)?;
                }
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
    pub fn add_corpses_to_grid(
        &mut self,
        pair: CellPair,
        instanceable: bool,
        objects: &mut SourceObjects,
        cells: &mut SourceCells,
        owner: &mut dyn CorpseGridOwner,
    ) -> Result<()> {
        let result = (|| {
            require(!self.stopped, "execution.corpse.stopped")?;
            for (_, object) in self.owners.iter() {
                let corpse = self.corpse(*object)?;
                // Intentionally omit map ID and actual cell filtering; ObjectAccessor.cpp:203-221.
                if corpse.grid == [pair.grid_x(), pair.grid_y()]
                    && (!instanceable || corpse.instance == cells.map().instance)
                {
                    link_reference(*object, cells.cell(pair)?, objects, cells, owner)?;
                }
            }
            Ok(())
        })();
        self.stopped |= result.is_err();
        result
    }
}
fn link_reference(
    object: ObjectHandle,
    cell: CellIdentity,
    objects: &mut SourceObjects,
    cells: &mut SourceCells,
    owner: &mut dyn CorpseGridOwner,
) -> Result<()> {
    if let Some(previous) = objects.get(object)?.corpse_grid_reference {
        if previous.grid.map != cell.grid.map {
            owner.unlink_other_map(object, previous, objects)?;
        }
    }
    cells.link(object, SourceObjectKind::Corpse, cell)?;
    objects.get_mut(object)?.corpse_grid_reference = Some(cell);
    Ok(())
}
fn initialize_fields(corpse: &mut SourceCorpse, guid: u32) {
    let guid = SourceGuid::without_entry(HighGuid::Corpse, guid).raw();
    corpse.fields[0] = guid as u32;
    corpse.fields[1] = (guid >> 32) as u32;
    corpse.fields[2] = 129;
    corpse.fields[4] = 1.0f32.to_bits();
}
fn load_fields(
    corpse: &mut SourceCorpse,
    row: &CorpseLoadRow,
    objects: &mut SourceObjects,
    appearance: &mut impl CorpseAppearanceOwner,
) -> Result<bool> {
    initialize_fields(corpse, row.guid);
    objects.assign_guid(
        corpse.object,
        row.guid,
        SourceGuid::without_entry(HighGuid::Corpse, row.guid),
    )?;
    corpse.created_seconds = row.time as i64;
    // The unfixed enum conversion precedes source validation; Corpse.h:27-33, Corpse.cpp:169-174.
    require(
        row.corpse_type <= 3,
        "execution.corpse.type_conversion_undefined",
    )?;
    corpse.corpse_type = match row.corpse_type {
        0 => SourceCorpseType::Bones,
        1 => SourceCorpseType::Pve,
        2 => SourceCorpseType::Pvp,
        _ => return Ok(false),
    };
    objects.get_mut(corpse.object)?.kind = corpse.corpse_type.object_kind();
    let guid = SourceGuid::without_entry(HighGuid::Player, row.player).raw();
    corpse.fields[6..8].copy_from_slice(&[guid as u32, (guid >> 32) as u32]);
    let Some(displays) = appearance.player_displays(row.race, row.class)? else {
        return Ok(false);
    };
    let mut equipment = [None; 19];
    let cache = row
        .equipment_cache
        .split(|b| *b == 0)
        .next()
        .unwrap_or_default();
    let mut tokens = cache.split(|b| *b == b' ').filter(|s| !s.is_empty());
    for slot in &mut equipment {
        let id = equipment_id(tokens.next().unwrap_or_default())?;
        tokens.next();
        *slot = appearance.item(id)?;
    }
    corpse.set_appearance(&OwnerAppearance {
        race: row.race,
        gender: row.gender,
        skin: row.player_bytes as u8,
        face: (row.player_bytes >> 8) as u8,
        hair_style: (row.player_bytes >> 16) as u8,
        hair_color: (row.player_bytes >> 24) as u8,
        facial_hair: row.player_bytes2 as u8,
        native_display_id: displays[usize::from(row.gender == 1)],
        guild_id: row.guild,
        hide_helm: row.player_flags & 0x400 != 0,
        hide_cloak: row.player_flags & 0x800 != 0,
        equipment,
    });
    corpse.map = row.map;
    corpse.instance = row.instance;
    corpse.position = row.position;
    objects.set_pose(corpse.object, SourcePose(row.position))?;
    if !valid_corpse_position(row.position) {
        return Ok(false);
    }
    let grid = compute_grid_pair(row.position[0], row.position[1])
        .map_err(|_| refuse("execution.corpse.grid_coordinates"))?;
    corpse.grid = [grid.x(), grid.y()];
    Ok(true)
}
// XY uses the source float grid size then a widened 0.5 subtraction; GridDefines.h:38,56-57,177-194.
pub(crate) fn valid_corpse_position(position: [u32; 4]) -> bool {
    let p = position.map(f32::from_bits);
    let bound = f64::from(533.333_3_f32 * 64.0 / 2.0) - 0.5;
    p.iter().all(|p| p.is_finite())
        && f64::from(p[0].abs()) <= bound
        && f64::from(p[1].abs()) <= bound
}
fn corpse_cell(position: [u32; 4]) -> Result<CellPair> {
    compute_cell_pair(position[0], position[1])
        .map_err(|_| refuse("execution.corpse.cell_coordinates"))
}
fn cell_id(pair: CellPair) -> u32 {
    u32::from(pair.y()) * 1024 + u32::from(pair.x())
}
// StrSplit skips empty space tokens; atoi reads a signed decimal prefix; Util.cpp:122-147.
fn equipment_id(raw: &[u8]) -> Result<u32> {
    let raw = raw.trim_ascii_start();
    let negative = raw.first() == Some(&b'-');
    let start = usize::from(matches!(raw.first(), Some(b'+' | b'-')));
    let mut value = 0i64;
    let limit = if negative { 2147483648 } else { 2147483647 };
    for &byte in &raw[start..] {
        if !byte.is_ascii_digit() {
            break;
        }
        value = value
            .checked_mul(10)
            .and_then(|v| v.checked_add(i64::from(byte - b'0')))
            .filter(|v| *v <= limit)
            .ok_or_else(|| refuse("execution.corpse.equipment_atoi_overflow_undefined"))?;
    }
    Ok((if negative { -value } else { value }) as i32 as u32)
}

pub struct CorpseGridBindings<'a> {
    pub registry: &'a mut SourceCorpseRegistry,
    pub other_maps: &'a mut dyn CorpseGridOwner,
    pub instanceable: bool,
}

/// Rebuild the effective display subset from retained F2 SQL and DBC operands.
pub struct CorpseCatalogAppearance<'a> {
    catalog: &'a mut super::super::production_compiler::VerifiedCatalog,
    displays: BTreeMap<(u8, u8), [u32; 2]>,
    items: BTreeMap<u32, Option<EquippedItem>>,
}
impl<'a> CorpseCatalogAppearance<'a> {
    pub fn load(
        catalog: &'a mut super::super::production_compiler::VerifiedCatalog,
    ) -> Result<Self> {
        let rows = catalog.execution_corpse_player_rows()?;
        let mut races = BTreeMap::new();
        {
            let mut view = catalog.dbc_view("ChrRaces.dbc")?;
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                let id = row["key"]
                    .as_u64()
                    .ok_or_else(|| refuse("execution.corpse.race_layout"))?
                    as u32;
                races.insert(id, [dbc_word(&row, 4)?, dbc_word(&row, 5)?]);
            }
        }
        let mut classes = std::collections::BTreeSet::new();
        {
            let mut view = catalog.dbc_view("ChrClasses.dbc")?;
            for ordinal in 0..view.record_count() {
                let row = view.record(ordinal)?;
                classes.insert(
                    row["key"]
                        .as_u64()
                        .ok_or_else(|| refuse("execution.corpse.class_layout"))?
                        as u32,
                );
            }
        }
        let maps: BTreeMap<_, _> = catalog.execution_map_entries()?.into_iter().collect();
        let mut displays = BTreeMap::new();
        for row in rows {
            // Preserve race/class, coordinate and non-instanceable loader filtering; ObjectMgr.cpp:3106-3143.
            if !(1..=8).contains(&row.race)
                || row.class == 0
                || row.class > 11
                || (0x5dfu32 & (1 << (row.class - 1))) == 0
                || !classes.contains(&row.class)
                || !valid_corpse_position(row.position)
            {
                continue;
            }
            let Some(&map_type) = maps.get(&row.map) else {
                continue;
            };
            if (PersistentMapEntry {
                id: row.map,
                map_type,
            })
            .instanceable()
            {
                continue;
            }
            let Some(&race) = races.get(&row.race) else {
                continue;
            };
            displays.insert((row.race as u8, row.class as u8), race);
        }
        Ok(Self {
            catalog,
            displays,
            items: BTreeMap::new(),
        })
    }
}
impl CorpseAppearanceOwner for CorpseCatalogAppearance<'_> {
    fn player_displays(&mut self, race: u8, class: u8) -> Result<Option<[u32; 2]>> {
        Ok(self
            .displays
            .get(&(race, class))
            .copied()
            .filter(|d| d[0] != 0 && d[1] != 0))
    }
    fn item(&mut self, id: u32) -> Result<Option<EquippedItem>> {
        if let Some(item) = self.items.get(&id) {
            return Ok(*item);
        }
        let item = self.catalog.execution_corpse_item(id)?;
        self.items.insert(id, item);
        Ok(item)
    }
}
fn dbc_word(row: &serde_json::Value, column: u32) -> Result<u32> {
    row["values"]
        .as_array()
        .and_then(|values| {
            values
                .iter()
                .find(|v| v[0].as_u64() == Some(u64::from(column)))
        })
        .and_then(|v| v[2].as_u64())
        .map(|v| v as u32)
        .ok_or_else(|| refuse("execution.corpse.dbc_layout"))
}
